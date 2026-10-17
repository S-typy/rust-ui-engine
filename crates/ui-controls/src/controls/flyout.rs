//! Retained arbitrary-content overlays with explicit ownership.
use super::*;

const MAX_FLYOUTS: usize = 256;
const MAX_OPEN: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlyoutPlacement {
    #[default]
    Below,
    Beside,
    /// Align to the bottom-left of the client viewport. A shorter desired
    /// height leaves a reserved strip above the surface, for window chrome.
    Client,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flyout {
    pub container: WidgetId,
    pub content: WidgetId,
}
#[derive(Clone)]
struct Entry {
    handle: Flyout,
    anchor: WidgetId,
    desired: Size,
    placement: FlyoutPlacement,
    previous_focus: Option<WidgetId>,
}
#[derive(Default)]
pub(super) struct FlyoutState {
    entries: HashMap<WidgetId, Entry>,
    open: Vec<WidgetId>,
    buttons: HashMap<WidgetId, WidgetId>,
}
impl Controls {
    /// Resolves focus in nested retained overlays to their owner inside a subtree.
    pub(super) fn flyout_focus_anchor_in(&self, root: WidgetId) -> Option<WidgetId> {
        let mut target = self.runtime.focused()?;
        for _ in 0..MAX_OPEN {
            let entry = self
                .flyouts
                .open
                .iter()
                .rev()
                .filter_map(|id| self.flyouts.entries.get(id))
                .find(|entry| self.flyout_descendant(target, entry.handle.container))?;
            if self.flyout_descendant(entry.anchor, root) {
                return Some(entry.anchor);
            }
            target = entry.anchor;
        }
        None
    }
    pub(super) fn preflight_flyout_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self
            .flyouts
            .entries
            .values()
            .any(|entry| entry.handle.content == id)
        {
            Err(ControlError::Invalid(
                "Remove the flyout container or owner, not its required content host".into(),
            ))
        } else {
            Ok(())
        }
    }
    /// Creates a hidden retained overlay. Build any controls inside `content`;
    /// closing only hides the subtree, preserving editor state and widget IDs.
    pub fn flyout(&mut self, anchor: WidgetId, desired: Size) -> Result<Flyout, ControlError> {
        validate_size(desired)?;
        if !self.runtime.tree().contains(anchor) || !self.controls.contains_key(&anchor) {
            return Err(ControlError::Invalid(
                "Flyout requires a live owner control".into(),
            ));
        }
        self.prune_flyouts()?;
        if self.flyouts.entries.len() >= MAX_FLYOUTS {
            return Err(ControlError::Invalid(
                "At most 256 retained flyouts are supported".into(),
            ));
        }
        let container = self.panel(
            self.overlay,
            LayoutStyle {
                width: Length::Px(desired.width),
                height: Length::Px(desired.height),
                padding: Edges::all(8.0),
                ..Default::default()
            },
        )?;
        self.controls.get_mut(&container).unwrap().popup_surface = true;
        self.set_role(container, Role::Group);
        self.edit_props(container, |props| {
            props.visible = false;
            props.clip = true;
            props.z_index = 900;
        })?;
        self.runtime.set_focus_scope(
            container,
            Some(rust_desktop_ui_core::FocusScope {
                cycle_tab: true,
                isolate_shortcuts: false,
            }),
        )?;
        let content = self.transparent_panel(
            container,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                gap: 6.0,
                ..Default::default()
            },
        )?;
        let handle = Flyout { container, content };
        self.flyouts.entries.insert(
            container,
            Entry {
                handle,
                anchor,
                desired,
                placement: FlyoutPlacement::Below,
                previous_focus: None,
            },
        );
        Ok(handle)
    }
    pub fn set_flyout_placement(
        &mut self,
        flyout: Flyout,
        desired: Size,
        placement: FlyoutPlacement,
    ) -> Result<(), ControlError> {
        validate_size(desired)?;
        let entry = self
            .flyouts
            .entries
            .get_mut(&flyout.container)
            .filter(|entry| entry.handle == flyout)
            .ok_or_else(|| ControlError::Invalid("Unknown flyout".into()))?;
        if entry.desired != desired || entry.placement != placement {
            entry.desired = desired;
            entry.placement = placement;
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub fn flyout_is_open(&self, flyout: Flyout) -> bool {
        self.flyouts
            .entries
            .get(&flyout.container)
            .is_some_and(|entry| entry.handle == flyout)
            && self.flyouts.open.contains(&flyout.container)
    }
    /// Changes an overlay owner after a complete form has resolved named
    /// targets. Closing the old chain precedes retargeting; old button bindings
    /// are removed and may be explicitly attached to the new owner afterward.
    pub fn set_flyout_anchor(
        &mut self,
        flyout: Flyout,
        anchor: WidgetId,
    ) -> Result<(), ControlError> {
        if !self
            .flyouts
            .entries
            .get(&flyout.container)
            .is_some_and(|entry| entry.handle == flyout)
            || !self.runtime.tree().contains(anchor)
            || !self.controls.contains_key(&anchor)
            || self.flyout_descendant(anchor, flyout.container)
        {
            return Err(ControlError::Invalid(
                "Flyout owner must be live and outside its content".into(),
            ));
        }
        if self.flyouts.entries[&flyout.container].anchor == anchor {
            return Ok(());
        }
        self.close_flyout(flyout)?;
        self.flyouts.buttons.retain(|_, id| *id != flyout.container);
        self.flyouts
            .entries
            .get_mut(&flyout.container)
            .unwrap()
            .anchor = anchor;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn can_attach_flyout(&self, button: WidgetId) -> bool {
        self.runtime.tree().contains(button)
            && self.repeat_button(button).is_none()
            && self.controls.get(&button).is_some_and(|control| {
                matches!(control.kind, Kind::Button)
                    && control.command.is_none()
                    && control.dropdown_target.is_none()
            })
    }
    /// Associated retained content, independent of whether it is currently open.
    /// Split primary parts resolve to their disclosure part.
    pub fn button_flyout(&self, id: WidgetId) -> Option<Flyout> {
        self.runtime.tree().node(id)?;
        let button = self
            .controls
            .get(&id)
            .and_then(|control| control.dropdown_target)
            .unwrap_or(id);
        let container = self.flyouts.buttons.get(&button)?;
        self.flyouts
            .entries
            .get(container)
            .map(|entry| entry.handle)
    }
    /// Associates an unbound Button (including a SplitButton's disclosure part)
    /// with retained arbitrary content. Command-bound primary actions stay independent.
    pub fn set_button_flyout(
        &mut self,
        button: WidgetId,
        flyout: Flyout,
    ) -> Result<(), ControlError> {
        if !self.can_attach_flyout(button)
            || !self.runtime.tree().contains(button)
            || !self
                .flyouts
                .entries
                .get(&flyout.container)
                .is_some_and(|entry| entry.handle == flyout && entry.anchor == button)
        {
            return Err(ControlError::Invalid(
                "Flyout disclosure requires its unbound owner Button".into(),
            ));
        }
        if self.popup_expanded(button) == Some(true) {
            self.close_popup()?;
        }
        self.controls.get_mut(&button).unwrap().menu = None;
        self.flyouts.buttons.insert(button, flyout.container);
        self.scene_dirty = true;
        Ok(())
    }
    pub fn clear_button_flyout(&mut self, button: WidgetId) -> Result<(), ControlError> {
        if let Some(id) = self.flyouts.buttons.remove(&button)
            && let Some(entry) = self.flyouts.entries.get(&id)
        {
            self.close_flyout(entry.handle)?;
        }
        Ok(())
    }
    /// Validates ownership, visibility, window activation and nesting without
    /// changing focus, popup state or the visual tree.
    pub fn preflight_open_flyout(&self, flyout: Flyout) -> Result<(), ControlError> {
        let entry = self
            .flyouts
            .entries
            .get(&flyout.container)
            .filter(|entry| entry.handle == flyout)
            .ok_or_else(|| ControlError::Invalid("Unknown flyout".into()))?;
        if !self.runtime.tree().contains(entry.handle.content)
            || !self.is_enabled(entry.anchor)
            || self
                .runtime
                .bounds(entry.anchor)
                .zip(self.runtime.clip(entry.anchor))
                .and_then(|(bounds, clip)| bounds.intersection(clip))
                .is_none()
            || !self.window_focused
            || self.viewport.width <= 0.0
            || self.viewport.height <= 0.0
        {
            return Err(ControlError::Invalid(
                "Flyout owner must be enabled, visible and laid out".into(),
            ));
        }
        let depth = self
            .flyouts
            .open
            .iter()
            .rposition(|id| self.flyout_descendant(entry.anchor, *id))
            .map_or(0, |index| index + 1);
        if depth >= MAX_OPEN {
            return Err(ControlError::Invalid(
                "Flyout nesting exceeds eight surfaces".into(),
            ));
        }
        Ok(())
    }
    pub fn open_flyout(&mut self, flyout: Flyout) -> Result<(), ControlError> {
        self.open_flyout_with_focus(flyout, true)
    }
    pub(super) fn open_flyout_preserving_focus(
        &mut self,
        flyout: Flyout,
    ) -> Result<(), ControlError> {
        self.open_flyout_with_focus(flyout, false)
    }
    fn open_flyout_with_focus(
        &mut self,
        flyout: Flyout,
        focus_content: bool,
    ) -> Result<(), ControlError> {
        let entry = self
            .flyouts
            .entries
            .get(&flyout.container)
            .filter(|entry| entry.handle == flyout)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Unknown flyout".into()))?;
        if self.flyout_is_open(flyout) {
            return Ok(());
        }
        if !self.is_enabled(entry.anchor)
            || self
                .runtime
                .bounds(entry.anchor)
                .zip(self.runtime.clip(entry.anchor))
                .and_then(|(bounds, clip)| bounds.intersection(clip))
                .is_none()
            || !self.window_focused
            || self.viewport.width <= 0.0
            || self.viewport.height <= 0.0
        {
            return Err(ControlError::Invalid(
                "Flyout owner must be enabled, visible and laid out".into(),
            ));
        }
        let parent = self
            .flyouts
            .open
            .iter()
            .rposition(|id| self.flyout_descendant(entry.anchor, *id));
        let depth = parent.map_or(0, |index| index + 1);
        if depth >= MAX_OPEN {
            return Err(ControlError::Invalid(
                "Flyout nesting exceeds eight surfaces".into(),
            ));
        }
        self.close_flyouts_from(depth, false)?;
        self.close_popup_from(0, popup::CloseReason::Outside)?;
        self.close_rich_popup(false)?;
        self.flyouts
            .entries
            .get_mut(&flyout.container)
            .unwrap()
            .previous_focus = self.runtime.focused();
        self.flyouts.open.push(flyout.container);
        self.edit_props(flyout.container, |props| {
            props.visible = true;
            props.z_index = 900 + depth as i32;
        })?;
        self.dismiss_tooltip();
        self.pressed = None;
        self.keyboard_pressed = false;
        self.runtime.apply(Mutation::CapturePointer(None))?;
        self.update(self.viewport)?;
        let first = self.runtime.tree().preorder().into_iter().find(|id| {
            self.flyout_descendant(*id, flyout.content)
                && self
                    .runtime
                    .tree()
                    .node(*id)
                    .is_some_and(|node| node.props.focusable)
                && self.is_enabled(*id)
        });
        if focus_content && let Some(first) = first {
            self.runtime.apply(Mutation::Focus(Some(first)))?;
        }
        self.synchronize_editor_focus()?;
        Ok(())
    }
    pub fn close_flyout(&mut self, flyout: Flyout) -> Result<(), ControlError> {
        if !self
            .flyouts
            .entries
            .get(&flyout.container)
            .is_some_and(|entry| entry.handle == flyout)
        {
            return Err(ControlError::Invalid("Unknown flyout".into()));
        }
        if let Some(index) = self
            .flyouts
            .open
            .iter()
            .position(|id| *id == flyout.container)
        {
            self.close_flyouts_from(index, true)?;
        }
        Ok(())
    }
    pub(super) fn close_all_flyouts(&mut self, restore: bool) -> Result<(), ControlError> {
        self.close_flyouts_from(0, restore)
    }
    pub(super) fn close_flyout_preserving_focus(
        &mut self,
        flyout: Flyout,
    ) -> Result<(), ControlError> {
        if let Some(index) = self
            .flyouts
            .open
            .iter()
            .position(|id| *id == flyout.container)
        {
            self.close_flyouts_from(index, false)?;
        }
        Ok(())
    }
    fn close_flyouts_from(&mut self, index: usize, restore: bool) -> Result<(), ControlError> {
        if index >= self.flyouts.open.len() {
            return Ok(());
        }
        let target = self.flyouts.entries.get(&self.flyouts.open[index]).cloned();
        let closing = self.flyouts.open[index..].to_vec();
        for id in closing {
            self.gallery_will_close(id)?;
            self.search_will_close(id);
            self.temporal_will_close(id)?;
            self.memo_will_close(id)?;
            self.numeric_will_close(id);
        }
        // Menus nested in this retained surface cannot outlive its visibility.
        if self.popup_anchor().is_some_and(|anchor| {
            self.flyouts.open[index..]
                .iter()
                .any(|id| self.flyout_descendant(anchor, *id))
        }) {
            self.close_popup_from(0, popup::CloseReason::Outside)?;
            self.close_rich_popup(false)?;
        }
        let ids: Vec<_> = self.flyouts.open.drain(index..).collect();
        for id in ids.into_iter().rev() {
            if self.runtime.tree().contains(id) {
                self.edit_props(id, |props| props.visible = false)?;
            }
        }
        if restore
            && self.window_focused
            && let Some(entry) = target
        {
            for candidate in [Some(entry.anchor), entry.previous_focus]
                .into_iter()
                .flatten()
            {
                if self.is_enabled(candidate)
                    && self.runtime.apply(Mutation::Focus(Some(candidate))).is_ok()
                {
                    break;
                }
            }
        }
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn flyout_descendant(&self, mut id: WidgetId, parent: WidgetId) -> bool {
        loop {
            if id == parent {
                return true;
            }
            let Some(next) = self.runtime.tree().parent(id) else {
                return false;
            };
            id = next;
        }
    }
    pub(super) fn flyout_open(&self) -> bool {
        !self.flyouts.open.is_empty()
    }
    pub(super) fn flyout_depth(&self) -> usize {
        self.flyouts.open.len()
    }
    pub(super) fn flyout_anchor(&self) -> Option<WidgetId> {
        self.flyouts
            .open
            .last()
            .and_then(|id| self.flyouts.entries.get(id))
            .map(|entry| entry.anchor)
    }
    pub(super) fn flyout_contains_point(&self, point: Point) -> bool {
        self.flyouts.open.iter().any(|id| {
            self.runtime
                .bounds(*id)
                .is_some_and(|bounds| bounds.contains(point.x, point.y))
        })
    }
    pub(super) fn flyout_expanded(&self, id: WidgetId) -> Option<bool> {
        let button = self
            .controls
            .get(&id)
            .and_then(|control| control.dropdown_target)
            .unwrap_or(id);
        self.flyouts
            .buttons
            .get(&button)
            .map(|id| self.flyouts.open.contains(id))
    }
    pub(super) fn activate_flyout(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let Some(id) = self.flyouts.buttons.get(&button).copied() else {
            return Ok(false);
        };
        let Some(entry) = self.flyouts.entries.get(&id) else {
            return Ok(false);
        };
        let handle = entry.handle;
        if self.flyout_is_open(handle) {
            self.close_flyout(handle)?;
        } else {
            self.open_flyout(handle)?;
        }
        Ok(true)
    }
    pub(super) fn sync_flyouts(&mut self) -> Result<(), ControlError> {
        self.prune_flyouts()?;
        let entries: Vec<_> = self
            .flyouts
            .open
            .iter()
            .filter_map(|id| self.flyouts.entries.get(id).cloned())
            .collect();
        for (index, entry) in entries.into_iter().enumerate() {
            let anchor = self
                .runtime
                .bounds(entry.anchor)
                .zip(self.runtime.clip(entry.anchor))
                .and_then(|(bounds, clip)| bounds.intersection(clip));
            if !self.window_focused || !self.is_enabled(entry.anchor) || anchor.is_none() {
                self.close_flyouts_from(index, false)?;
                break;
            }
            let bounds = if entry.placement == FlyoutPlacement::Client {
                let width = entry.desired.width.min(self.viewport.width).max(0.0);
                let height = entry.desired.height.min(self.viewport.height).max(0.0);
                Rect::new(0.0, self.viewport.height - height, width, height)
            } else {
                popup::place_overlay(
                    anchor.unwrap(),
                    entry.desired,
                    self.viewport,
                    entry.placement == FlyoutPlacement::Beside,
                )
            };
            let mut style = self
                .runtime
                .tree()
                .node(entry.handle.container)
                .unwrap()
                .props
                .style
                .clone();
            style.width = Length::Px(bounds.width);
            style.height = Length::Px(bounds.height);
            style.offset = Point::new(bounds.x, bounds.y);
            self.runtime.apply(Mutation::SetStyle {
                node: entry.handle.container,
                style,
            })?;
        }
        Ok(())
    }
    pub(super) fn prune_flyouts(&mut self) -> Result<(), ControlError> {
        let invalid: Vec<_> = self
            .flyouts
            .entries
            .iter()
            .filter(|(_, entry)| {
                !self.runtime.tree().contains(entry.anchor)
                    || !self.runtime.tree().contains(entry.handle.container)
                    || !self.runtime.tree().contains(entry.handle.content)
            })
            .map(|(id, _)| *id)
            .collect();
        let changed = !invalid.is_empty();
        for id in invalid {
            if let Some(index) = self
                .flyouts
                .open
                .iter()
                .position(|candidate| *candidate == id)
            {
                self.close_flyouts_from(index, false)?;
            }
            self.flyouts.entries.remove(&id);
            if self.runtime.tree().contains(id) {
                self.runtime.apply(Mutation::Remove(id))?;
            }
        }
        self.flyouts.buttons.retain(|button, id| {
            self.runtime.tree().contains(*button) && self.flyouts.entries.contains_key(id)
        });
        if changed {
            self.prune_removed_controls()?;
        }
        Ok(())
    }
    pub(super) fn flyout_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if !self.flyout_open() {
            return Ok(false);
        }
        match event {
            InputEvent::WindowFocused(false) => self.close_all_flyouts(false)?,
            InputEvent::KeyDown {
                key: Key::Escape,
                modifiers,
                repeat: false,
            } if *modifiers == rust_desktop_ui_core::Modifiers::default()
                && self.popup.is_none()
                && self.rich.popup.is_none() =>
            {
                self.close_flyouts_from(self.flyouts.open.len() - 1, true)?;
                return Ok(true);
            }
            InputEvent::PointerDown { position, .. } if !self.menu_contains_point(*position) => {
                let keep = self
                    .flyouts
                    .open
                    .iter()
                    .rposition(|id| {
                        self.runtime
                            .bounds(*id)
                            .is_some_and(|bounds| bounds.contains(position.x, position.y))
                    })
                    .map_or(0, |index| index + 1);
                let on_button = self
                    .flyouts
                    .open
                    .get(keep)
                    .and_then(|id| self.flyouts.entries.get(id))
                    .is_some_and(|entry| {
                        self.runtime
                            .bounds(entry.anchor)
                            .is_some_and(|bounds| bounds.contains(position.x, position.y))
                    });
                if !on_button {
                    self.close_flyouts_from(keep, false)?;
                }
            }
            _ => {}
        }
        Ok(false)
    }
}
fn validate_size(size: Size) -> Result<(), ControlError> {
    if [size.width, size.height]
        .iter()
        .any(|value| !value.is_finite() || !(1.0..=16_384.0).contains(value))
    {
        Err(ControlError::Invalid(
            "Flyout dimensions must be finite and in 1..16384".into(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;
    fn setup() -> (Controls, WidgetId, Flyout, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let anchor = ui
            .button(
                ui.root(),
                "Open",
                None,
                LayoutStyle {
                    width: Length::Px(100.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let flyout = ui.flyout(anchor, Size::new(240.0, 120.0)).unwrap();
        let editor = ui
            .edit_box(flyout.content, "Name", "draft", LayoutStyle::default())
            .unwrap();
        ui.set_button_flyout(anchor, flyout).unwrap();
        ui.update(Size::new(300.0, 200.0)).unwrap();
        (ui, anchor, flyout, editor)
    }
    #[test]
    fn retained_editors_survive_escape_resize_and_reopen_and_owner_removal_disposes() {
        let (mut ui, anchor, flyout, editor) = setup();
        ui.activate(anchor).unwrap();
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.popup_expanded(anchor), Some(true));
        ui.set_text(editor, "retained").unwrap();
        ui.update(Size::new(110.0, 80.0)).unwrap();
        let bounds = ui.runtime.bounds(flyout.container).unwrap();
        assert!(
            bounds.x >= 0.0
                && bounds.y >= 0.0
                && bounds.x + bounds.width <= 110.0
                && bounds.y + bounds.height <= 80.0
        );
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        assert!(!ui.flyout_is_open(flyout));
        assert_eq!(ui.runtime.focused(), Some(anchor));
        assert_eq!(ui.document(editor).unwrap().text(), "retained");
        ui.activate(anchor).unwrap();
        assert_eq!(ui.runtime.focused(), Some(editor));
        ui.remove(anchor).unwrap();
        assert!(!ui.runtime.tree().contains(editor));
        assert!(!ui.popup_open());
        assert_ne!(ui.runtime.focused(), Some(editor));
    }
    #[test]
    fn nested_flyout_escape_closes_only_child_and_blur_closes_all() {
        let (mut ui, anchor, parent, _) = setup();
        let nested_anchor = ui
            .button(parent.content, "Nested", None, LayoutStyle::default())
            .unwrap();
        let nested = ui.flyout(nested_anchor, Size::new(120.0, 80.0)).unwrap();
        ui.label(nested.content, "Child", LayoutStyle::default())
            .unwrap();
        ui.open_flyout(parent).unwrap();
        ui.open_flyout(nested).unwrap();
        assert_eq!(ui.popup_depth(), 2);
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        assert_eq!(ui.popup_depth(), 1);
        assert_eq!(ui.runtime.focused(), Some(nested_anchor));
        ui.set_enabled(anchor, false).unwrap();
        ui.update(Size::new(300.0, 200.0)).unwrap();
        assert!(!ui.popup_open());
        ui.set_enabled(anchor, true).unwrap();
        ui.open_flyout(parent).unwrap();
        ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
        assert!(!ui.popup_open());
    }
    #[test]
    fn split_flyout_primary_command_and_disclosure_remain_independent() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("run", "Run");
        let split = ui
            .split_button_content(
                ui.root(),
                "Run",
                Some("run".into()),
                LayoutStyle {
                    width: Length::Px(200.0),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(ui.controls[&split.primary].menu.is_none());
        assert!(ui.controls[&split.dropdown].menu.is_none());
        let flyout = ui.flyout(split.dropdown, Size::new(200.0, 100.0)).unwrap();
        ui.label(flyout.content, "Custom options", LayoutStyle::default())
            .unwrap();
        ui.set_button_flyout(split.dropdown, flyout).unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        ui.activate(split.primary).unwrap();
        assert_eq!(ui.drain_activated(), vec![split.primary]);
        assert!(!ui.flyout_is_open(flyout));
        ui.expand_popup(split.primary).unwrap();
        assert!(ui.flyout_is_open(flyout));
        assert!(ui.drain_activated().is_empty());
        ui.collapse_popup(split.primary).unwrap();
        assert!(!ui.flyout_is_open(flyout));
    }
    #[test]
    fn owner_retargeting_rejects_cycles_and_releases_the_old_button() {
        let (mut ui, anchor, flyout, editor) = setup();
        let second = ui
            .button(ui.root(), "Second", None, LayoutStyle::default())
            .unwrap();
        ui.open_flyout(flyout).unwrap();
        assert!(ui.set_flyout_anchor(flyout, editor).is_err());
        assert_eq!(ui.button_flyout(anchor), Some(flyout));
        assert!(ui.flyout_is_open(flyout));
        ui.set_flyout_anchor(flyout, second).unwrap();
        assert_eq!(ui.button_flyout(anchor), None);
        assert!(!ui.flyout_is_open(flyout));
        ui.set_button_flyout(second, flyout).unwrap();
        assert_eq!(ui.button_flyout(second), Some(flyout));
        assert!(ui.remove(flyout.content).is_err());
        ui.remove(anchor).unwrap();
        assert!(ui.runtime.tree().contains(editor));
        ui.remove(second).unwrap();
        assert!(!ui.runtime.tree().contains(editor));
    }

    #[test]
    fn replacing_open_disclosure_closes_old_surface_without_recreating_its_editor() {
        let (mut ui, anchor, original, editor) = setup();
        ui.open_flyout(original).unwrap();
        ui.set_text(editor, "retained draft").unwrap();
        let replacement = ui.flyout(anchor, Size::new(160.0, 100.0)).unwrap();
        ui.set_button_flyout(anchor, replacement).unwrap();
        assert!(!ui.flyout_is_open(original));
        assert!(!ui.flyout_is_open(replacement));
        assert_eq!(ui.button_flyout(anchor), Some(replacement));
        assert_eq!(ui.runtime.focused(), Some(anchor));
        assert_eq!(ui.document(editor).unwrap().text(), "retained draft");
        ui.activate(anchor).unwrap();
        assert!(ui.flyout_is_open(replacement));
        assert!(!ui.flyout_is_open(original));
        ui.close_flyout(replacement).unwrap();
        ui.set_button_flyout(anchor, original).unwrap();
        ui.open_flyout(original).unwrap();
        assert_eq!(ui.document(editor).unwrap().text(), "retained draft");
    }
}
