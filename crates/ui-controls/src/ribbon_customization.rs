//! Explicit, bounded Ribbon preferences. No callbacks or widget identities persist.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_QUICK_ACCESS_COMMANDS: usize = 256;
const MAX_SETTINGS_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RibbonSettings {
    pub version: u32,
    pub quick_access: Vec<String>,
    pub active_tab: String,
    pub minimized: bool,
}

impl RibbonSettings {
    fn validate(&self) -> Result<(), ControlError> {
        let mut unique = HashSet::new();
        if self.version != 1
            || self.quick_access.len() > MAX_QUICK_ACCESS_COMMANDS
            || self.active_tab.len() > 512
            || self
                .quick_access
                .iter()
                .any(|id| id.is_empty() || id.len() > 512 || !unique.insert(id.as_str()))
        {
            return Err(ControlError::Invalid(
                "Invalid Ribbon preferences version, IDs or QAT size".into(),
            ));
        }
        Ok(())
    }
    pub fn to_json(&self) -> Result<String, ControlError> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| ControlError::Invalid(error.to_string()))?;
        if json.len() > MAX_SETTINGS_BYTES {
            return Err(ControlError::Invalid(
                "Ribbon preferences exceed 64 KiB".into(),
            ));
        }
        Ok(json)
    }
    pub fn from_json(json: &str) -> Result<Self, ControlError> {
        if json.len() > MAX_SETTINGS_BYTES {
            return Err(ControlError::Invalid(
                "Ribbon preferences exceed 64 KiB".into(),
            ));
        }
        let settings: Self =
            serde_json::from_str(json).map_err(|error| ControlError::Invalid(error.to_string()))?;
        settings.validate()?;
        Ok(settings)
    }
}

impl Ribbon {
    pub fn settings(&self) -> RibbonSettings {
        RibbonSettings {
            version: 1,
            quick_access: self
                .model
                .quick_access
                .iter()
                .map(|id| id.0.clone())
                .collect(),
            active_tab: self.active.clone(),
            minimized: self.minimized,
        }
    }

    /// Validate every reference and resulting layout before changing preferences.
    /// `update` subsequently presents the accepted preferences. Commands, checked
    /// state, callbacks and application documents are not serialized or executed.
    pub fn apply_settings(
        &mut self,
        ui: &mut Controls,
        settings: &RibbonSettings,
    ) -> Result<bool, ControlError> {
        settings.validate()?;
        if settings
            .quick_access
            .iter()
            .any(|id| !ui.commands().contains(&id.as_str().into()))
        {
            return Err(ControlError::Invalid(
                "Ribbon preferences contain an unregistered QAT command".into(),
            ));
        }
        let visible = self.visible_tabs();
        if !(visible.iter().any(|tab| tab.id == settings.active_tab)
            || visible.is_empty() && settings.active_tab.is_empty())
        {
            return Err(ControlError::Invalid(
                "Ribbon preferences require a currently visible tab".into(),
            ));
        }
        let old = self.settings();
        let transient = self.transient;
        self.model.quick_access = settings
            .quick_access
            .iter()
            .map(|id| id.as_str().into())
            .collect();
        self.active.clone_from(&settings.active_tab);
        self.minimized = settings.minimized;
        self.transient = false;
        let result = self.preflight(ui, self.width);
        if let Err(error) = result {
            self.model.quick_access = old.quick_access.into_iter().map(CommandId).collect();
            self.active = old.active_tab;
            self.minimized = old.minimized;
            self.transient = transient;
            return Err(error);
        }
        let changed = &old != settings || transient;
        self.dirty |= changed;
        Ok(changed)
    }

    pub fn replace_quick_access(
        &mut self,
        ui: &mut Controls,
        commands: Vec<CommandId>,
    ) -> Result<bool, ControlError> {
        let mut settings = self.settings();
        settings.quick_access = commands.into_iter().map(|id| id.0).collect();
        self.apply_settings(ui, &settings)
    }

    /// Move an existing command to its final zero-based position.
    pub fn move_quick_access(
        &mut self,
        ui: &mut Controls,
        command: &CommandId,
        index: usize,
    ) -> Result<bool, ControlError> {
        let mut commands = self.model.quick_access.clone();
        let old = commands
            .iter()
            .position(|id| id == command)
            .ok_or_else(|| ControlError::Invalid("QAT command is absent".into()))?;
        if index >= commands.len() {
            return Err(ControlError::Invalid(
                "QAT destination is outside the list".into(),
            ));
        }
        let command = commands.remove(old);
        commands.insert(index, command);
        self.replace_quick_access(ui, commands)
    }

    /// Restore the initial application-supplied QAT order, after live validation.
    pub fn reset_quick_access(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        self.replace_quick_access(ui, self.default_quick_access.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Modifiers, Size};

    fn fixture() -> (Controls, Ribbon) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        for id in ["a", "b", "c"] {
            ui.commands_mut().register(id, id);
        }
        ui.commands_mut().set_checked(&"b".into(), Some(true));
        let mut ribbon = Ribbon::new(RibbonModel {
            tabs: vec![RibbonTab {
                id: "home".into(),
                label: "Home".into(),
                key_tip: "H".into(),
                context: None,
                groups: vec![],
            }],
            quick_access: vec!["a".into(), "b".into()],
        })
        .unwrap();
        let root = ui.root();
        ribbon.mount_with_width(&mut ui, root, 600.0).unwrap();
        ui.update(Size::new(600.0, 400.0)).unwrap();
        (ui, ribbon)
    }

    #[test]
    fn serialization_rejects_unknown_fields_versions_duplicates_and_oversize() {
        let (_, ribbon) = fixture();
        let saved = ribbon.settings();
        assert_eq!(
            RibbonSettings::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        for json in [
            r#"{"version":2,"quick_access":[],"active_tab":"home","minimized":false}"#,
            r#"{"version":1,"quick_access":["a","a"],"active_tab":"home","minimized":false}"#,
            r#"{"version":1,"quick_access":[],"active_tab":"home","minimized":false,"callback":"x"}"#,
        ] {
            assert!(RibbonSettings::from_json(json).is_err());
        }
        assert!(RibbonSettings::from_json(&" ".repeat(MAX_SETTINGS_BYTES + 1)).is_err());
        let mut many = saved;
        many.quick_access = (0..257).map(|i| i.to_string()).collect();
        assert!(many.to_json().is_err());
    }

    #[test]
    fn invalid_references_and_measurement_preserve_preferences_and_tree() {
        let (mut ui, mut ribbon) = fixture();
        let old = ribbon.settings();
        let nodes = ui.runtime.tree().preorder();
        let mut invalid = old.clone();
        invalid.quick_access.push("missing".into());
        assert!(ribbon.apply_settings(&mut ui, &invalid).is_err());
        invalid = old.clone();
        invalid.active_tab = "missing".into();
        assert!(ribbon.apply_settings(&mut ui, &invalid).is_err());
        ui.commands_mut().register("long", "x".repeat(5000));
        invalid = old.clone();
        invalid.quick_access.push("long".into());
        assert!(ribbon.apply_settings(&mut ui, &invalid).is_err());
        assert_eq!(ribbon.settings(), old);
        assert_eq!(ui.runtime.tree().preorder(), nodes);
        assert!(ui.commands_mut().drain_invocations().is_empty());
    }

    #[test]
    fn reorder_reset_and_keytips_preserve_command_state_and_editor_draft() {
        let (mut ui, mut ribbon) = fixture();
        let root = ui.root();
        let editor = ui
            .edit_box(root, "Document", "accepted", LayoutStyle::default())
            .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.text_input(" draft").unwrap();
        let draft = ui.document(editor).unwrap().text().to_owned();
        assert!(ribbon.move_quick_access(&mut ui, &"b".into(), 0).unwrap());
        ribbon.update(&mut ui, 600.0).unwrap();
        ui.update(Size::new(600.0, 400.0)).unwrap();
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.document(editor).unwrap().text(), draft);
        assert_eq!(ui.commands().checked(&"b".into()), Some(true));
        assert!(ui.commands_mut().drain_invocations().is_empty());
        for key in [Key::Alt, Key::Character("1".into())] {
            ribbon
                .handle_input(
                    &mut ui,
                    &InputEvent::KeyDown {
                        key,
                        modifiers: Modifiers::default(),
                        repeat: false,
                    },
                )
                .unwrap();
        }
        let invoked = ui.commands_mut().drain_invocations();
        assert_eq!(invoked.len(), 1);
        assert_eq!(ui.commands().checked(&"b".into()), Some(false));
        assert!(ribbon.reset_quick_access(&mut ui).unwrap());
        assert_eq!(ribbon.model().quick_access, vec!["a".into(), "b".into()]);
        assert!(!ribbon.reset_quick_access(&mut ui).unwrap());
        assert!(ribbon.move_quick_access(&mut ui, &"b".into(), 2).is_err());
    }
}
