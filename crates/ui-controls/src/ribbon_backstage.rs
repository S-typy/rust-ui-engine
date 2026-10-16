//! Retained application pages and command navigation in a client overlay.
use super::content::{collapsed, descendant, stable_id};
use super::*;
use crate::{Flyout, FlyoutPlacement};
use rust_desktop_ui_core::Size;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonBackstageSpec {
    pub label: String,
    pub key_tip: String,
    pub items: Vec<RibbonBackstageItem>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonBackstageItem {
    Page {
        id: String,
        label: String,
        key_tip: String,
    },
    Command {
        command: CommandId,
        key_tip: String,
    },
}
#[derive(Clone)]
enum BackstageAction {
    Close,
    Page(String),
    Command,
}
#[derive(Default)]
pub(super) struct BackstageState {
    spec: Option<RibbonBackstageSpec>,
    popup: Option<Flyout>,
    pages: HashMap<String, WidgetId>,
    buttons: HashMap<String, WidgetId>,
    actions: HashMap<WidgetId, BackstageAction>,
    tips: HashMap<WidgetId, String>,
    active: Option<String>,
    previous_focus: Option<WidgetId>,
    navigation: Option<WidgetId>,
    opened: bool,
}
impl Ribbon {
    /// Configures at most 32 page or command entries before mounting. Page
    /// content is mounted once into the hosts returned after Ribbon::mount.
    pub fn set_backstage(&mut self, spec: Option<RibbonBackstageSpec>) -> Result<(), ControlError> {
        if self.container.is_some() {
            return Err(ControlError::Invalid(
                "Configure Backstage before mounting".into(),
            ));
        }
        let spec = if let Some(mut spec) = spec {
            spec.key_tip = spec.key_tip.to_uppercase();
            let tab_tips: Vec<_> = self
                .model
                .tabs
                .iter()
                .map(|tab| tab.key_tip.clone())
                .collect();
            if spec.label.is_empty()
                || spec.label.len() > 4096
                || spec.key_tip.len() > 32
                || !valid_key_tip(&spec.key_tip, &tab_tips)
                || spec
                    .key_tip
                    .starts_with(['1', '2', '3', '4', '5', '6', '7', '8', '9'])
                || spec.items.is_empty()
                || spec.items.len() > 32
            {
                return Err(ControlError::Invalid(
                    "Invalid Backstage label, KeyTip or entry count".into(),
                ));
            }
            let mut ids = std::collections::HashSet::new();
            let mut tips = Vec::new();
            for item in &mut spec.items {
                let key = match item {
                    RibbonBackstageItem::Page { id, label, key_tip } => {
                        if !stable_id(id)
                            || !ids.insert(id.clone())
                            || label.is_empty()
                            || label.len() > 4096
                        {
                            return Err(ControlError::Invalid(
                                "Invalid Backstage page identity or label".into(),
                            ));
                        }
                        key_tip
                    }
                    RibbonBackstageItem::Command { command, key_tip } => {
                        if command.0.is_empty() {
                            return Err(ControlError::Invalid(
                                "Backstage command requires an identity".into(),
                            ));
                        }
                        key_tip
                    }
                };
                *key = key.to_uppercase();
                if key.len() > 32 || !valid_key_tip(key, &tips) {
                    return Err(ControlError::Invalid(
                        "Backstage KeyTips must be prefix-free".into(),
                    ));
                }
                tips.push(key.clone());
            }
            Some(spec)
        } else {
            None
        };
        self.backstage.spec = spec;
        self.dirty = true;
        Ok(())
    }
    pub fn backstage_page_host(&self, id: &str) -> Option<WidgetId> {
        self.backstage.pages.get(id).copied()
    }
    pub fn active_backstage_page(&self) -> Option<&str> {
        self.backstage.active.as_deref()
    }
    pub fn backstage_is_open(&self, ui: &Controls) -> bool {
        self.backstage.popup.is_some_and(|p| ui.flyout_is_open(p))
    }
    pub(super) fn backstage_owns(&self, ui: &Controls, id: WidgetId) -> bool {
        self.backstage
            .popup
            .is_some_and(|p| descendant(ui, id, p.container))
    }
    pub(super) fn validate_backstage_tree(&self, ui: &Controls) -> Result<(), ControlError> {
        let Some(popup) = self.backstage.popup else {
            return Ok(());
        };
        let mut ids = vec![popup.container, popup.content];
        ids.extend(self.backstage.navigation);
        ids.extend(self.backstage.pages.values().copied());
        ids.extend(self.backstage.actions.keys().copied());
        if ids.iter().any(|id| !ui.runtime.tree().contains(*id)) {
            return Err(ControlError::Invalid("A required Backstage part was removed; remove the Ribbon owner and create a new Ribbon".into()));
        }
        Ok(())
    }
    pub(super) fn preflight_backstage(&self, ui: &mut Controls) -> Result<(), ControlError> {
        let Some(spec) = &self.backstage.spec else {
            return Ok(());
        };
        let tips: Vec<_> = self.model.tabs.iter().map(|t| t.key_tip.clone()).collect();
        if !valid_key_tip(&spec.key_tip, &tips) {
            return Err(ControlError::Invalid(
                "Merged tab collides with Backstage KeyTip".into(),
            ));
        }
        ui.measure_text(&spec.label, 4096.0, false)?;
        ui.measure_text(&spec.key_tip, 4096.0, false)?;
        for item in &spec.items {
            let label = match item {
                RibbonBackstageItem::Page { label, .. } => label.clone(),
                RibbonBackstageItem::Command { command, .. } => {
                    if !ui.commands().contains(command) {
                        return Err(ControlError::Invalid(
                            "Backstage command must be registered".into(),
                        ));
                    }
                    ui.commands()
                        .label(command)
                        .unwrap_or(&command.0)
                        .to_owned()
                }
            };
            if label.len() > 4096 {
                return Err(ControlError::Invalid(
                    "Backstage label exceeds 4096 UTF-8 bytes".into(),
                ));
            }
            ui.measure_text(&label, 4096.0, false)?;
            let key_tip = match item {
                RibbonBackstageItem::Page { key_tip, .. }
                | RibbonBackstageItem::Command { key_tip, .. } => key_tip,
            };
            ui.measure_text(key_tip, 4096.0, false)?;
        }
        Ok(())
    }
    pub(super) fn backstage_button_width(&self, ui: &mut Controls) -> Result<f32, ControlError> {
        self.backstage
            .spec
            .as_ref()
            .map(|spec| {
                ui.measure_text(&spec.label, 4096.0, false)
                    .map(|m| (m.width.ceil() + 28.0).clamp(48.0, 180.0))
            })
            .transpose()
            .map(|value| value.unwrap_or(0.0))
    }
    pub(super) fn mount_backstage_button(
        &mut self,
        ui: &mut Controls,
        row: WidgetId,
        width: f32,
        height: f32,
    ) -> Result<(), ControlError> {
        if let Some(spec) = &self.backstage.spec {
            let button = ui.button(
                row,
                &spec.label,
                None,
                LayoutStyle {
                    width: Length::Px(width),
                    height: Length::Px(height),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_button_style(button, ribbon_button_style())?;
            ui.set_button_chrome(button, ButtonChrome::Flat)?;
            self.actions.insert(button, Action::Backstage);
            self.tip_nodes
                .insert(button, (Tips::Tabs, spec.key_tip.clone()));
        }
        Ok(())
    }
    pub(super) fn mount_backstage(
        &mut self,
        ui: &mut Controls,
        container: WidgetId,
    ) -> Result<(), ControlError> {
        if self.backstage.popup.is_some() {
            return Ok(());
        }
        let Some(spec) = self.backstage.spec.clone() else {
            return Ok(());
        };
        let popup = ui.flyout(container, Size::new(640.0, 400.0))?;
        ui.set_automation_name(popup.container, &spec.label)?;
        ui.runtime.apply(Mutation::SetStyle {
            node: popup.content,
            style: LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                gap: 12.0,
                ..Default::default()
            },
        })?;
        let navigation = ui.scroll_view(
            popup.content,
            LayoutStyle {
                width: Length::Px(200.0),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
            (spec.items.len() + 1) as f32 * 40.0 + 12.0,
        )?;
        self.backstage.navigation = Some(navigation.viewport);
        let close = ui.button(
            navigation.content,
            "Back",
            None,
            LayoutStyle {
                height: Length::Px(36.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        ui.set_button_chrome(close, ButtonChrome::Flat)?;
        self.backstage.actions.insert(close, BackstageAction::Close);
        let pages = ui.transparent_panel(
            popup.content,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                height: Length::Percent(1.0),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                ..Default::default()
            },
        )?;
        for item in spec.items {
            let (button, key) = match item {
                RibbonBackstageItem::Page { id, label, key_tip } => {
                    let selected = self.backstage.active.is_none();
                    if selected {
                        self.backstage.active = Some(id.clone())
                    }
                    let button = ui.toggle(
                        navigation.content,
                        &label,
                        None,
                        selected,
                        LayoutStyle {
                            height: Length::Px(36.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?;
                    let host = ui.transparent_panel(
                        pages,
                        LayoutStyle {
                            kind: LayoutKind::Overlay,
                            width: Length::Percent(1.0),
                            height: Length::Percent(1.0),
                            ..Default::default()
                        },
                    )?;
                    ui.set_role(host, Role::Group);
                    ui.set_automation_name(host, label)?;
                    collapsed(ui, host, !selected)?;
                    self.backstage.pages.insert(id.clone(), host);
                    self.backstage.buttons.insert(id.clone(), button);
                    self.backstage
                        .actions
                        .insert(button, BackstageAction::Page(id));
                    (button, key_tip)
                }
                RibbonBackstageItem::Command { command, key_tip } => {
                    let label = ui
                        .commands()
                        .label(&command)
                        .unwrap_or(&command.0)
                        .to_owned();
                    let button = ui.button(
                        navigation.content,
                        label,
                        Some(command),
                        LayoutStyle {
                            height: Length::Px(36.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?;
                    self.backstage
                        .actions
                        .insert(button, BackstageAction::Command);
                    (button, key_tip)
                }
            };
            ui.set_button_chrome(button, ButtonChrome::Flat)?;
            self.backstage.tips.insert(button, key);
        }
        self.backstage.popup = Some(popup);
        Ok(())
    }
    pub(super) fn sync_backstage_geometry(
        &mut self,
        ui: &mut Controls,
    ) -> Result<(), ControlError> {
        let Some(popup) = self.backstage.popup else {
            return Ok(());
        };
        if !ui.runtime.tree().contains(popup.container) {
            return Ok(());
        }
        if self.backstage.opened && !ui.flyout_is_open(popup) {
            self.backstage.opened = false;
            if self.tips == Tips::Backstage {
                self.finish_key_tips(ui, false);
            }
        }
        let Some(viewport) = ui.runtime.bounds(ui.root()) else {
            return Ok(());
        };
        let reserved = self
            .header_regions(ui)
            .map(|regions| regions.bounds.y + regions.bounds.height)
            .unwrap_or(0.0)
            .clamp(0.0, viewport.height);
        let height = (viewport.height - reserved).max(0.0);
        if height < 1.0 {
            if ui.flyout_is_open(popup) {
                self.close_backstage(ui)?;
            }
            return Ok(());
        }
        let width = viewport.width.max(1.0);
        ui.set_flyout_placement(popup, Size::new(width, height), FlyoutPlacement::Client)?;
        if let Some(nav) = self.backstage.navigation {
            let mut style = ui.runtime.tree().node(nav).unwrap().props.style.clone();
            style.width = Length::Px(
                (width * 0.3)
                    .clamp(72.0, 220.0)
                    .min((width - 16.0).max(0.0)),
            );
            if ui.runtime.tree().node(nav).unwrap().props.style != style {
                ui.runtime.apply(Mutation::SetStyle { node: nav, style })?;
            }
        }
        self.sync_backstage_tips(ui)
    }
    fn sync_backstage_tips(&self, ui: &mut Controls) -> Result<(), ControlError> {
        for (&id, tip) in &self.backstage.tips {
            if ui.runtime.tree().contains(id) {
                let value =
                    (self.tips == Tips::Backstage && ui.is_enabled(id)).then(|| tip.clone());
                if ui.key_tip(id) != value.as_deref() {
                    ui.set_key_tip(id, value)?;
                }
            }
        }
        Ok(())
    }
    pub fn open_backstage(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let popup = self
            .backstage
            .popup
            .ok_or_else(|| ControlError::Invalid("Backstage is not mounted".into()))?;
        self.validate_retained_tree(ui)?;
        ui.preflight_open_flyout(popup)?;
        let viewport = ui.runtime.bounds(ui.root()).ok_or_else(|| {
            ControlError::Invalid("Backstage requires a laid-out client viewport".into())
        })?;
        let reserved = self
            .header_regions(ui)
            .map(|r| r.bounds.y + r.bounds.height)
            .unwrap_or(0.0);
        if viewport.height - reserved < 1.0 {
            return Err(ControlError::Invalid(
                "No client area below the reserved window header".into(),
            ));
        }
        if ui.flyout_is_open(popup) {
            return Ok(());
        }
        let show_tips = self.tips == Tips::Tabs;
        self.backstage.previous_focus = if show_tips {
            // Alt/F10 has already moved focus to the tab header. Backstage
            // inherits the original return target before ending that session.
            self.key_tip_return_focus
                .filter(|id| ui.is_enabled(*id))
                .or_else(|| ui.runtime.focused())
        } else {
            ui.runtime.focused()
        };
        self.finish_key_tips(ui, false);
        self.close_content_groups(ui)?;
        self.sync_backstage_geometry(ui)?;
        ui.open_flyout(popup)?;
        self.backstage.opened = true;
        self.tips = if show_tips {
            Tips::Backstage
        } else {
            Tips::Hidden
        };
        self.tip_prefix.clear();
        self.sync_backstage_tips(ui)?;
        if let Some(id) = self.backstage.active.clone() {
            self.focus_backstage_page(ui, &id)?;
        }
        Ok(())
    }
    pub fn close_backstage(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.finish_key_tips(ui, false);
        self.sync_backstage_tips(ui)?;
        if let Some(popup) = self.backstage.popup {
            ui.close_flyout(popup)?;
        }
        self.backstage.opened = false;
        if let Some(id) = self
            .backstage
            .previous_focus
            .take()
            .filter(|id| ui.is_enabled(*id))
        {
            ui.runtime.apply(Mutation::Focus(Some(id)))?;
        }
        Ok(())
    }
    pub fn select_backstage_page(
        &mut self,
        ui: &mut Controls,
        id: &str,
    ) -> Result<(), ControlError> {
        self.validate_retained_tree(ui)?;
        if !self.backstage.pages.contains_key(id) {
            return Err(ControlError::Invalid("Unknown Backstage page".into()));
        }
        for (page, &host) in &self.backstage.pages {
            collapsed(ui, host, page != id)?;
        }
        self.backstage.active = Some(id.into());
        for (page, &button) in &self.backstage.buttons {
            ui.set_checked(button, page == id)?;
        }
        if self.backstage_is_open(ui) {
            self.focus_backstage_page(ui, id)?;
        }
        Ok(())
    }
    fn focus_backstage_page(&self, ui: &mut Controls, id: &str) -> Result<(), ControlError> {
        let host = self.backstage.pages[id];
        let target = ui
            .runtime
            .tree()
            .preorder()
            .into_iter()
            .find(|id| {
                descendant(ui, *id, host)
                    && ui.is_enabled(*id)
                    && ui
                        .runtime
                        .tree()
                        .node(*id)
                        .is_some_and(|n| n.props.focusable && n.props.tab_stop)
            })
            .or_else(|| self.backstage.buttons.get(id).copied());
        if let Some(target) = target {
            ui.runtime.apply(Mutation::Focus(Some(target)))?;
        }
        Ok(())
    }
    pub(super) fn process_backstage(
        &mut self,
        ui: &mut Controls,
        id: WidgetId,
    ) -> Result<bool, ControlError> {
        match self.backstage.actions.get(&id).cloned() {
            Some(BackstageAction::Close | BackstageAction::Command) => self.close_backstage(ui)?,
            Some(BackstageAction::Page(id)) => self.select_backstage_page(ui, &id)?,
            None => return Ok(false),
        }
        Ok(true)
    }
    pub(super) fn backstage_matches_tip(&self, key: &str) -> bool {
        self.backstage
            .spec
            .as_ref()
            .is_some_and(|s| s.key_tip == key)
    }
    pub(super) fn backstage_has_prefix(&self, key: &str) -> bool {
        self.backstage
            .spec
            .as_ref()
            .is_some_and(|s| s.key_tip.starts_with(key))
    }
    pub(super) fn handle_backstage_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if !self.backstage_is_open(ui) {
            return Ok(false);
        }
        if !self.container.is_some_and(|id| ui.is_enabled(id)) {
            self.close_backstage(ui)?;
            return Ok(false);
        }
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.finish_key_tips(ui, false);
            self.sync_backstage_tips(ui)?;
            return Ok(false);
        }
        if matches!(event, InputEvent::PointerDown { .. }) {
            self.finish_key_tips(ui, false);
            self.sync_backstage_tips(ui)?;
            return Ok(false);
        }
        let InputEvent::KeyDown {
            key,
            repeat,
            modifiers,
        } = event
        else {
            return Ok(false);
        };
        if *key == Key::Escape {
            // Let nested popup/IME handlers consume Escape before closing pages.
            let nested = ui
                .popup_anchor()
                .is_some_and(|id| self.backstage_owns(ui, id));
            let composing = ui
                .runtime
                .focused()
                .and_then(|id| ui.document(id))
                .is_some_and(|doc| doc.composition().is_some());
            if composing {
                ui.cancel_preedit();
                return Ok(true);
            }
            if nested {
                return Ok(false);
            }
            self.close_backstage(ui)?;
            return Ok(true);
        }
        if matches!(key, Key::Alt | Key::F10) && !modifiers.control && !modifiers.super_key {
            if !repeat {
                ui.cancel_preedit();
                self.tips = if self.tips == Tips::Backstage {
                    Tips::Hidden
                } else {
                    Tips::Backstage
                };
                self.tip_prefix.clear();
                self.sync_backstage_tips(ui)?;
            }
            return Ok(true);
        }
        if self.tips != Tips::Backstage {
            return Ok(false);
        }
        if *key == Key::Tab || modifiers.control || modifiers.super_key {
            self.finish_key_tips(ui, false);
            self.sync_backstage_tips(ui)?;
            return Ok(false);
        }
        let Key::Character(character) = key else {
            return Ok(false);
        };
        if *repeat {
            return Ok(true);
        }
        self.tip_prefix.push_str(&character.to_uppercase());
        if let Some(id) = self
            .backstage
            .tips
            .iter()
            .find_map(|(&id, key)| (key == &self.tip_prefix && ui.is_enabled(id)).then_some(id))
        {
            self.finish_key_tips(ui, false);
            self.sync_backstage_tips(ui)?;
            if matches!(
                self.backstage.actions.get(&id),
                Some(BackstageAction::Command)
            ) && let Some(command) = ui.command_id(id).cloned()
            {
                ui.commands_mut().activate(&command);
            }
            self.process_backstage(ui, id)?;
        } else if !self
            .backstage
            .tips
            .values()
            .any(|tip| tip.starts_with(&self.tip_prefix))
        {
            self.tip_prefix.clear();
        }
        Ok(true)
    }
}
