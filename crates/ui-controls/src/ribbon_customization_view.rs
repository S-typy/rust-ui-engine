//! Explicit QAT customization: a local draft, applied after full validation.
use super::*;
use crate::{Flyout, SelectionItem, SelectionSettings};
use rust_desktop_ui_core::{GridPlacement, GridTrack, Size};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditAction {
    Add,
    Remove,
    Up,
    Down,
    Reset,
    Apply,
    Cancel,
}

pub(super) struct QuickAccessEditor {
    popup: Flyout,
    available: WidgetId,
    current: WidgetId,
    error: WidgetId,
    actions: HashMap<WidgetId, EditAction>,
    draft: Vec<CommandId>,
}

fn items(ui: &Controls, commands: &[CommandId]) -> Vec<SelectionItem> {
    commands
        .iter()
        .map(|id| SelectionItem::new(&id.0, ui.commands().label(id).unwrap_or(&id.0)))
        .collect()
}

impl Ribbon {
    /// Enable the customization button with a bounded application-supplied catalog.
    /// Initial and current QAT commands are included so removal is reversible.
    /// `None` hides the button; it never changes the accepted command order.
    pub fn set_quick_access_catalog(
        &mut self,
        ui: &mut Controls,
        commands: Option<Vec<CommandId>>,
    ) -> Result<(), ControlError> {
        let catalog = if let Some(mut commands) = commands {
            if commands.len() > MAX_QUICK_ACCESS_COMMANDS {
                return Err(ControlError::Invalid(
                    "QAT catalog exceeds 256 commands".into(),
                ));
            }
            for id in self
                .default_quick_access
                .iter()
                .chain(&self.model.quick_access)
            {
                if !commands.contains(id) {
                    commands.push(id.clone());
                }
            }
            let mut seen = std::collections::HashSet::new();
            if commands.len() > MAX_QUICK_ACCESS_COMMANDS
                || commands.iter().any(|id| {
                    id.0.is_empty()
                        || id.0.len() > 512
                        || !seen.insert(id.clone())
                        || !ui.commands().contains(id)
                })
            {
                return Err(ControlError::Invalid(
                    "QAT catalog requires unique registered command IDs".into(),
                ));
            }
            for id in &commands {
                ui.measure_text(
                    ui.commands().label(id).unwrap_or(&id.0).to_owned().as_str(),
                    4096.0,
                    false,
                )?;
            }
            Some(commands)
        } else {
            None
        };
        if self.quick_access_catalog != catalog {
            self.quick_access_catalog = catalog;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn quick_access_customization_is_open(&self, ui: &Controls) -> bool {
        self.quick_access_editor
            .as_ref()
            .is_some_and(|state| ui.flyout_is_open(state.popup))
    }

    pub(super) fn discard_quick_access_editor(
        &mut self,
        ui: &mut Controls,
    ) -> Result<(), ControlError> {
        if let Some(state) = self.quick_access_editor.take()
            && ui.runtime.tree().node(state.popup.container).is_some()
        {
            ui.remove(state.popup.container)?;
        }
        Ok(())
    }

    pub(super) fn mount_quick_access_customizer(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        width: f32,
        height: f32,
    ) -> Result<(), ControlError> {
        if self.quick_access_catalog.is_none() {
            return Ok(());
        }
        let button = ui.button(
            parent,
            "Customize quick access",
            None,
            LayoutStyle {
                width: Length::Px(width),
                height: Length::Px(height),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        ui.set_button_icon(
            button,
            Some(ButtonIcon::new(crate::IconSymbol::ChevronDown)),
        )?;
        ui.set_button_content_layout(button, ButtonContentLayout::IconOnly)?;
        ui.set_button_chrome(button, ButtonChrome::Flat)?;
        ui.set_button_style(button, ribbon_button_style())?;
        ui.set_tooltip(button, "Customize quick access")?;
        self.actions.insert(button, Action::CustomizeQuickAccess);
        Ok(())
    }

    fn open_quick_access_editor(
        &mut self,
        ui: &mut Controls,
        anchor: WidgetId,
    ) -> Result<(), ControlError> {
        self.finish_key_tips(ui, false);
        if self.quick_access_customization_is_open(ui) {
            return ui.close_flyout(self.quick_access_editor.as_ref().unwrap().popup);
        }
        self.discard_quick_access_editor(ui)?;
        let commands = self
            .quick_access_catalog
            .clone()
            .ok_or_else(|| ControlError::Invalid("QAT customization is disabled".into()))?;
        let popup = ui.flyout(anchor, Size::new(620.0, 400.0))?;
        let build = (|| {
            ui.runtime.apply(Mutation::SetStyle {
                node: popup.content,
                style: LayoutStyle {
                    kind: LayoutKind::Grid,
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    gap: 8.0,
                    grid_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)],
                    grid_rows: vec![
                        GridTrack::Px(28.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(36.0),
                        GridTrack::Px(28.0),
                        GridTrack::Px(36.0),
                    ],
                    ..Default::default()
                },
            })?;
            let cell = |row, column, span| LayoutStyle {
                grid_row: GridPlacement {
                    start: Some(row),
                    span: 1,
                },
                grid_column: GridPlacement {
                    start: Some(column),
                    span,
                },
                ..Default::default()
            };
            ui.label(popup.content, "Available commands", cell(1, 1, 1))?;
            ui.label(popup.content, "Quick access order", cell(1, 2, 1))?;
            let available = ui
                .list_box(
                    popup.content,
                    "Available quick access commands",
                    items(ui, &commands),
                    SelectionSettings::default(),
                    cell(2, 1, 1),
                )?
                .container;
            let current = ui
                .list_box(
                    popup.content,
                    "Current quick access order",
                    items(ui, &self.model.quick_access),
                    SelectionSettings::default(),
                    cell(2, 2, 1),
                )?
                .container;
            let mut actions = HashMap::new();
            for (column, buttons) in [
                (1, vec![("Add", EditAction::Add)]),
                (
                    2,
                    vec![
                        ("Remove", EditAction::Remove),
                        ("Up", EditAction::Up),
                        ("Down", EditAction::Down),
                    ],
                ),
            ] {
                let row = ui.transparent_panel(
                    popup.content,
                    LayoutStyle {
                        kind: LayoutKind::Flex(Axis::Horizontal),
                        gap: 4.0,
                        ..cell(3, column, 1)
                    },
                )?;
                for (label, action) in buttons {
                    let id = ui.button(
                        row,
                        label,
                        None,
                        LayoutStyle {
                            width: Length::Px(0.0),
                            height: Length::Percent(1.0),
                            flex_grow: 1.0,
                            ..Default::default()
                        },
                    )?;
                    actions.insert(id, action);
                }
            }
            let error = ui.label(popup.content, "", cell(4, 1, 2))?;
            let footer = ui.transparent_panel(
                popup.content,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    gap: 8.0,
                    ..cell(5, 1, 2)
                },
            )?;
            for (label, action) in [
                ("Reset", EditAction::Reset),
                ("Apply", EditAction::Apply),
                ("Cancel", EditAction::Cancel),
            ] {
                let id = ui.button(
                    footer,
                    label,
                    None,
                    LayoutStyle {
                        width: Length::Px(0.0),
                        height: Length::Percent(1.0),
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                )?;
                actions.insert(id, action);
            }
            Ok(QuickAccessEditor {
                popup,
                available,
                current,
                error,
                actions,
                draft: self.model.quick_access.clone(),
            })
        })();
        match build {
            Ok(state) => {
                self.quick_access_editor = Some(state);
                ui.open_flyout(popup)
            }
            Err(error) => {
                ui.remove(popup.container)?;
                Err(error)
            }
        }
    }

    pub(super) fn process_quick_access_editor(
        &mut self,
        ui: &mut Controls,
        id: WidgetId,
    ) -> Result<bool, ControlError> {
        if matches!(self.actions.get(&id), Some(Action::CustomizeQuickAccess)) {
            self.open_quick_access_editor(ui, id)?;
            return Ok(true);
        }
        let Some(state) = self.quick_access_editor.as_ref() else {
            return Ok(false);
        };
        if !ui.flyout_is_open(state.popup) {
            return Ok(false);
        }
        let Some(action) = state.actions.get(&id).copied() else {
            return Ok(false);
        };
        let popup = state.popup;
        if matches!(action, EditAction::Cancel) {
            ui.close_flyout(popup)?;
            return Ok(true);
        }
        if matches!(action, EditAction::Apply) {
            let draft = state.draft.clone();
            let error_label = state.error;
            match self.replace_quick_access(ui, draft) {
                Ok(_) => {
                    ui.close_flyout(popup)?;
                }
                Err(error) => {
                    ui.set_label(error_label, error.to_string())?;
                }
            }
            return Ok(true);
        }
        let state = self.quick_access_editor.as_mut().unwrap();
        let selected = ui
            .selected_keys(state.current)
            .and_then(|keys| keys.first())
            .cloned();
        let mut selection = selected.clone();
        match action {
            EditAction::Add => {
                if let Some(key) = ui
                    .selected_keys(state.available)
                    .and_then(|keys| keys.first())
                    .cloned()
                    && !state.draft.iter().any(|id| id.0 == key)
                    && state.draft.len() < MAX_QUICK_ACCESS_COMMANDS
                {
                    state.draft.push(CommandId(key.clone()));
                    selection = Some(key);
                }
            }
            EditAction::Remove => {
                if let Some(index) = state
                    .draft
                    .iter()
                    .position(|id| Some(&id.0) == selected.as_ref())
                {
                    state.draft.remove(index);
                    selection = state
                        .draft
                        .get(index.min(state.draft.len().saturating_sub(1)))
                        .map(|id| id.0.clone());
                }
            }
            EditAction::Up | EditAction::Down => {
                if let Some(index) = state
                    .draft
                    .iter()
                    .position(|id| Some(&id.0) == selected.as_ref())
                {
                    let next = if matches!(action, EditAction::Up) {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(state.draft.len() - 1)
                    };
                    state.draft.swap(index, next);
                }
            }
            EditAction::Reset => {
                state.draft.clone_from(&self.default_quick_access);
                selection = None;
            }
            EditAction::Apply | EditAction::Cancel => unreachable!(),
        }
        ui.set_selection_items(state.current, items(ui, &state.draft))?;
        ui.set_selected_keys(state.current, &selection.into_iter().collect::<Vec<_>>())?;
        ui.set_label(state.error, "")?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;

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
        ribbon
            .set_quick_access_catalog(&mut ui, Some(vec!["c".into()]))
            .unwrap();
        let root = ui.root();
        ribbon.mount_with_width(&mut ui, root, 800.0).unwrap();
        ui.update(Size::new(800.0, 600.0)).unwrap();
        (ui, ribbon)
    }
    fn open(ui: &mut Controls, ribbon: &mut Ribbon) {
        ui.update(Size::new(800.0, 600.0)).unwrap();
        let anchor = *ribbon
            .actions
            .iter()
            .find(|(_, action)| matches!(action, Action::CustomizeQuickAccess))
            .unwrap()
            .0;
        ui.activate(anchor).unwrap();
        ribbon.process(ui).unwrap();
        ui.update(Size::new(800.0, 600.0)).unwrap();
        assert!(ribbon.quick_access_customization_is_open(ui));
    }
    fn action(ui: &mut Controls, ribbon: &mut Ribbon, action: EditAction) {
        let id = *ribbon
            .quick_access_editor
            .as_ref()
            .unwrap()
            .actions
            .iter()
            .find(|(_, item)| **item == action)
            .unwrap()
            .0;
        ui.activate(id).unwrap();
        ribbon.process(ui).unwrap();
    }

    #[test]
    fn customization_draft_add_reorder_remove_and_cancel_never_execute_commands() {
        let (mut ui, mut ribbon) = fixture();
        let initial = ribbon.settings();
        open(&mut ui, &mut ribbon);
        let available = ribbon.quick_access_editor.as_ref().unwrap().available;
        ui.set_selected_keys(available, &["c".into()]).unwrap();
        action(&mut ui, &mut ribbon, EditAction::Add);
        action(&mut ui, &mut ribbon, EditAction::Up);
        assert_eq!(
            ribbon.quick_access_editor.as_ref().unwrap().draft,
            vec!["a".into(), "c".into(), "b".into()]
        );
        action(&mut ui, &mut ribbon, EditAction::Remove);
        assert_eq!(
            ribbon.quick_access_editor.as_ref().unwrap().draft,
            vec!["a".into(), "b".into()]
        );
        action(&mut ui, &mut ribbon, EditAction::Cancel);
        assert_eq!(ribbon.settings(), initial);
        assert!(ui.commands_mut().drain_invocations().is_empty());
        assert_eq!(ui.commands().checked(&"b".into()), Some(true));
    }

    #[test]
    fn customization_apply_reflows_once_and_reset_requires_apply() {
        let (mut ui, mut ribbon) = fixture();
        open(&mut ui, &mut ribbon);
        let current = ribbon.quick_access_editor.as_ref().unwrap().current;
        ui.set_selected_keys(current, &["b".into()]).unwrap();
        action(&mut ui, &mut ribbon, EditAction::Up);
        action(&mut ui, &mut ribbon, EditAction::Apply);
        assert_eq!(ribbon.model().quick_access, vec!["b".into(), "a".into()]);
        ribbon.update(&mut ui, 800.0).unwrap();
        assert!(matches!(
            ribbon.actions.get(&ui.runtime.focused().unwrap()),
            Some(Action::CustomizeQuickAccess)
        ));
        open(&mut ui, &mut ribbon);
        action(&mut ui, &mut ribbon, EditAction::Reset);
        assert_eq!(ribbon.model().quick_access, vec!["b".into(), "a".into()]);
        action(&mut ui, &mut ribbon, EditAction::Apply);
        assert_eq!(ribbon.model().quick_access, vec!["a".into(), "b".into()]);
        assert!(ui.commands_mut().drain_invocations().is_empty());
    }

    #[test]
    fn escape_and_resize_cancel_customization_preserving_document_and_focus_return() {
        let (mut ui, mut ribbon) = fixture();
        let root = ui.root();
        let editor = ui
            .edit_box(root, "Document", "accepted", LayoutStyle::default())
            .unwrap();
        ui.set_text(editor, "retained draft").unwrap();
        open(&mut ui, &mut ribbon);
        let old_popup = ribbon.quick_access_editor.as_ref().unwrap().popup.container;
        ribbon.update(&mut ui, 320.0).unwrap();
        assert!(!ui.runtime.tree().contains(old_popup));
        assert!(matches!(
            ribbon.actions.get(&ui.runtime.focused().unwrap()),
            Some(Action::CustomizeQuickAccess)
        ));
        assert_eq!(ui.document(editor).unwrap().text(), "retained draft");
        open(&mut ui, &mut ribbon);
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        ribbon.process(&mut ui).unwrap();
        assert!(!ribbon.quick_access_customization_is_open(&ui));
        assert!(matches!(
            ribbon.actions.get(&ui.runtime.focused().unwrap()),
            Some(Action::CustomizeQuickAccess)
        ));
    }

    #[test]
    fn catalog_errors_preserve_settings_and_existing_tree() {
        let (mut ui, mut ribbon) = fixture();
        let catalog = ribbon.quick_access_catalog.clone();
        let nodes = ui.runtime.tree().preorder();
        for commands in [vec!["missing".into()], vec!["a".into(), "a".into()]] {
            assert!(
                ribbon
                    .set_quick_access_catalog(&mut ui, Some(commands))
                    .is_err()
            );
        }
        assert_eq!(ribbon.quick_access_catalog, catalog);
        assert_eq!(ui.runtime.tree().preorder(), nodes);
    }
}
