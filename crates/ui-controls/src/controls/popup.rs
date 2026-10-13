//! Bounded menu overlays. Geometry and focus remain owned by Controls.
use super::*;

pub const MAX_MENU_DEPTH: usize = 8;
pub const MAX_MENU_ITEMS: usize = 256;
const MAX_MENU_TEXT: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub enum MenuEntry {
    Item(MenuItem),
    Submenu {
        label: String,
        items: Vec<MenuEntry>,
        enabled: bool,
    },
    Separator,
}

/// A horizontal composition with independent primary and menu buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitButton {
    pub container: WidgetId,
    pub primary: WidgetId,
    pub dropdown: WidgetId,
}

#[derive(Clone)]
struct Level {
    container: WidgetId,
    anchor: WidgetId,
    items: Vec<WidgetId>,
    entries: Vec<MenuEntry>,
    desired: Size,
    theme: Theme,
}
pub(super) struct Popup {
    anchor: WidgetId,
    previous_focus: Option<WidgetId>,
    levels: Vec<Level>,
}

#[derive(Clone, Copy)]
pub(super) enum CloseReason {
    Restore,
    Outside,
    Unavailable,
}

/// Prefer below/right, then flip when the opposite side has more usable room.
/// The final box is clamped to the viewport even when neither side can fit it.
pub(super) fn place_overlay(anchor: Rect, desired: Size, viewport: Size, submenu: bool) -> Rect {
    let width = desired.width.min(viewport.width).max(0.0);
    let height = desired.height.min(viewport.height).max(0.0);
    let (mut x, mut y) = if submenu {
        let right = viewport.width - (anchor.x + anchor.width);
        let left = anchor.x;
        (
            if width > right && left > right {
                anchor.x - width
            } else {
                anchor.x + anchor.width
            },
            anchor.y,
        )
    } else {
        let below = viewport.height - (anchor.y + anchor.height);
        let above = anchor.y;
        (
            anchor.x,
            if height > below && above > below {
                anchor.y - height
            } else {
                anchor.y + anchor.height
            },
        )
    };
    x = x.clamp(0.0, (viewport.width - width).max(0.0));
    y = y.clamp(0.0, (viewport.height - height).max(0.0));
    Rect::new(x, y, width, height)
}

fn validate_entries(
    entries: &[MenuEntry],
    depth: usize,
    count: &mut usize,
    bytes: &mut usize,
) -> Result<(), ControlError> {
    if entries.is_empty() || depth > MAX_MENU_DEPTH || entries.len() > MAX_MENU_ITEMS {
        return Err(ControlError::Invalid(
            "Menus must be nonempty, at most 8 levels and 256 entries".into(),
        ));
    }
    for entry in entries {
        *count += 1;
        if *count > MAX_MENU_ITEMS {
            return Err(ControlError::Invalid(
                "Menus support at most 256 entries".into(),
            ));
        }
        let text = match entry {
            MenuEntry::Item(item) => item.label.len().checked_add(item.command.0.len()),
            MenuEntry::Submenu { label, items, .. } => {
                validate_entries(items, depth + 1, count, bytes)?;
                Some(label.len())
            }
            MenuEntry::Separator => Some(0),
        };
        *bytes = text
            .and_then(|text| bytes.checked_add(text))
            .filter(|bytes| *bytes <= MAX_MENU_TEXT)
            .ok_or_else(|| ControlError::Invalid("Menu text exceeds 1 MiB".into()))?;
    }
    Ok(())
}

impl Controls {
    fn preflight_menu(&mut self, entries: &[MenuEntry]) -> Result<(), ControlError> {
        validate_entries(entries, 1, &mut 0, &mut 0)?;
        let mut pending = vec![entries];
        while let Some(entries) = pending.pop() {
            for entry in entries {
                let label = match entry {
                    MenuEntry::Item(item) => Some(item.label.as_str()),
                    MenuEntry::Submenu { label, items, .. } => {
                        pending.push(items);
                        Some(label.as_str())
                    }
                    MenuEntry::Separator => None,
                };
                if let Some(label) = label {
                    self.measure_text(label, 4096.0, false)?;
                }
            }
        }
        Ok(())
    }
    fn menu_size(&mut self, entries: &[MenuEntry]) -> Result<Size, ControlError> {
        let mut width = 240.0_f32;
        let mut height = 12.0;
        for entry in entries {
            let label = match entry {
                MenuEntry::Item(item) => Some(item.label.as_str()),
                MenuEntry::Submenu { label, .. } => Some(label.as_str()),
                MenuEntry::Separator => None,
            };
            if let Some(label) = label {
                width = width.max(self.measure_text(label, 4096.0, false)?.width + 64.0);
                height += self.theme.tokens().control_height;
            } else {
                height += 9.0;
            }
        }
        Ok(Size::new(width.min(640.0), height))
    }
    fn popup_anchor_bounds(&self, anchor: WidgetId) -> Option<Rect> {
        if !self.window_focused || !self.is_enabled(anchor) {
            return None;
        }
        let bounds = self.runtime.bounds(anchor)?;
        let clip = self.runtime.clip(anchor)?;
        bounds
            .intersection(clip)
            .filter(|area| area.width > 0.0 && area.height > 0.0)?;
        (self.viewport.width > 0.0 && self.viewport.height > 0.0).then_some(bounds)
    }
    fn validate_popup_anchor(&self, anchor: WidgetId) -> Result<Rect, ControlError> {
        self.popup_anchor_bounds(anchor).ok_or_else(|| {
            ControlError::Invalid(
                "Popup anchor must be live, enabled, visible and laid out in a nonempty viewport"
                    .into(),
            )
        })
    }
    fn validate_menu_builder(
        &mut self,
        parent: WidgetId,
        entries: &[MenuEntry],
        style: &LayoutStyle,
    ) -> Result<(), ControlError> {
        if self.runtime.tree().node(parent).is_none() {
            return Err(ControlError::Invalid("Unknown parent".into()));
        }
        style
            .validate()
            .map_err(|message| ControlError::Invalid(message.into()))?;
        self.preflight_menu(entries)
    }
    pub fn dropdown_button(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        items: &[MenuEntry],
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.validate_menu_builder(parent, items, &style)?;
        let label = label.into();
        TextLimits::default().validate(&label).map_err(edit_error)?;
        let id = self.button(parent, label, None, style)?;
        self.controls.get_mut(&id).unwrap().menu = Some(items.to_vec());
        Ok(id)
    }
    pub fn split_button(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        command: Option<CommandId>,
        items: &[MenuEntry],
        mut style: LayoutStyle,
    ) -> Result<SplitButton, ControlError> {
        self.validate_menu_builder(parent, items, &style)?;
        let label = label.into();
        TextLimits::default().validate(&label).map_err(edit_error)?;
        style.kind = LayoutKind::Flex(Axis::Horizontal);
        style.gap = 0.0;
        if style.height == Length::Auto {
            style.height = Length::Px(self.theme.tokens().control_height);
        }
        let container = self.transparent_panel(parent, style)?;
        self.set_role(container, Role::Group);
        let result = (|| {
            let primary = self.button(
                container,
                label.clone(),
                command,
                LayoutStyle {
                    width: Length::Px(0.0),
                    height: Length::Percent(1.0),
                    flex_grow: 1.0,
                    ..Default::default()
                },
            )?;
            let dropdown = self.dropdown_button(
                container,
                "",
                items,
                LayoutStyle {
                    width: Length::Px(32.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            self.controls.get_mut(&dropdown).unwrap().automation_name = Some(if label.is_empty() {
                "Open menu".into()
            } else {
                label.clone()
            });
            self.controls.get_mut(&primary).unwrap().dropdown_target = Some(dropdown);
            Ok(SplitButton {
                container,
                primary,
                dropdown,
            })
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        result
    }
    /// Flat compatibility wrapper. Toggle hints only initialize missing checked state.
    pub fn popup_menu(
        &mut self,
        anchor: WidgetId,
        items: &[MenuItem],
    ) -> Result<WidgetId, ControlError> {
        if items.len() > MAX_MENU_ITEMS {
            return Err(ControlError::Invalid(
                "Menus support at most 256 entries".into(),
            ));
        }
        self.popup_menu_entries(
            anchor,
            &items
                .iter()
                .cloned()
                .map(MenuEntry::Item)
                .collect::<Vec<_>>(),
        )
    }
    /// Open a bounded menu chain. Validation occurs before replacing an open menu.
    pub fn popup_menu_entries(
        &mut self,
        anchor: WidgetId,
        entries: &[MenuEntry],
    ) -> Result<WidgetId, ControlError> {
        self.preflight_menu(entries)?;
        let bounds = self.validate_popup_anchor(anchor)?;
        let parent_level = self.popup.as_ref().and_then(|popup| {
            popup
                .levels
                .iter()
                .position(|level| level.items.contains(&anchor))
        });
        let depth = parent_level.map_or(0, |level| level + 1);
        let (mut count, mut bytes) = (0, 0);
        if let Some(popup) = &self.popup {
            for level in popup.levels.iter().take(depth) {
                count += level.entries.len();
                for entry in &level.entries {
                    bytes += match entry {
                        MenuEntry::Item(item) => item.label.len() + item.command.0.len(),
                        MenuEntry::Submenu { label, .. } => label.len(),
                        MenuEntry::Separator => 0,
                    };
                }
            }
        }
        validate_entries(entries, depth + 1, &mut count, &mut bytes)?;
        if parent_level.is_none()
            && self.popup.as_ref().is_some_and(|popup| {
                popup.levels.iter().any(|level| {
                    std::iter::successors(Some(anchor), |id| self.runtime.tree().parent(*id))
                        .any(|id| id == level.container)
                })
            })
        {
            return Err(ControlError::Invalid(
                "Popup replacement cannot remove its own anchor".into(),
            ));
        }
        let desired = self.menu_size(entries)?;
        let previous_focus = self.runtime.focused();
        let level = self.build_menu_level(anchor, entries, desired, bounds, depth)?;
        let container = level.container;
        self.dismiss_tooltip();
        self.pressed = None;
        self.keyboard_pressed = false;
        self.runtime.apply(Mutation::CapturePointer(None))?;
        if let Some(parent_level) = parent_level {
            self.close_popup_from(parent_level + 1, CloseReason::Outside)?;
            self.popup.as_mut().unwrap().levels.push(level);
        } else {
            self.close_popup_from(0, CloseReason::Outside)?;
            self.popup = Some(Popup {
                anchor,
                previous_focus,
                levels: vec![level],
            });
        }
        self.update(self.viewport)?;
        self.focus_popup_item(depth, false)?;
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(container)
    }
    fn build_menu_level(
        &mut self,
        anchor: WidgetId,
        entries: &[MenuEntry],
        desired: Size,
        bounds: Rect,
        depth: usize,
    ) -> Result<Level, ControlError> {
        let placed = place_overlay(bounds, desired, self.viewport, depth > 0);
        let viewer = self.scroll_view(
            self.overlay,
            LayoutStyle {
                width: Length::Px(placed.width),
                height: Length::Px(placed.height),
                offset: Point::new(placed.x, placed.y),
                ..Default::default()
            },
            desired.height,
        )?;
        let result = (|| {
            self.edit_props(viewer.viewport, |props| {
                props.z_index = 1000 + depth as i32;
                props.tab_stop = false;
            })?;
            self.set_role(viewer.viewport, Role::Menu);
            self.edit_props(viewer.content, |props| props.style.gap = 0.0)?;
            let mut ids = Vec::new();
            for entry in entries {
                let id = match entry {
                    MenuEntry::Separator => self.separator(
                        viewer.content,
                        SeparatorSettings::default(),
                        LayoutStyle {
                            height: Length::Px(9.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?,
                    MenuEntry::Item(item) => {
                        let id = self.add(
                            viewer.content,
                            Kind::MenuItem,
                            item.label.clone(),
                            Some(item.command.clone()),
                            LayoutStyle::default(),
                        )?;
                        self.controls.get_mut(&id).unwrap().enabled = item.enabled;
                        id
                    }
                    MenuEntry::Submenu {
                        label,
                        items,
                        enabled,
                    } => {
                        let id = self.add(
                            viewer.content,
                            Kind::MenuItem,
                            label.clone(),
                            None,
                            LayoutStyle::default(),
                        )?;
                        let control = self.controls.get_mut(&id).unwrap();
                        control.menu = Some(items.clone());
                        control.enabled = *enabled;
                        id
                    }
                };
                self.edit_props(id, |props| props.tab_stop = false)?;
                if !matches!(entry, MenuEntry::Separator) {
                    ids.push(id);
                }
            }
            Ok(Level {
                container: viewer.viewport,
                anchor,
                items: ids,
                entries: entries.to_vec(),
                desired,
                theme: self.theme,
            })
        })();
        if result.is_err() {
            self.remove(viewer.viewport)?;
        }
        if result.is_ok() {
            for entry in entries {
                if let MenuEntry::Item(item) = entry
                    && item.toggle
                    && self.commands.checked(&item.command).is_none()
                {
                    self.commands.set_checked(&item.command, Some(false));
                }
            }
        }
        result
    }
    /// Close the entire chain and restore an eligible anchor (or prior focus).
    pub fn close_popup(&mut self) -> Result<(), ControlError> {
        self.close_popup_from(0, CloseReason::Restore)
    }
    pub fn popup_open(&self) -> bool {
        self.popup.is_some()
    }
    pub fn popup_depth(&self) -> usize {
        self.popup.as_ref().map_or(0, |popup| popup.levels.len())
    }
    pub fn popup_expanded(&self, id: WidgetId) -> Option<bool> {
        let target = self.popup_target(id)?;
        Some(
            self.popup
                .as_ref()
                .is_some_and(|popup| popup.levels.iter().any(|level| level.anchor == target)),
        )
    }
    pub(super) fn popup_target(&self, id: WidgetId) -> Option<WidgetId> {
        self.runtime.tree().node(id)?;
        let control = self.controls.get(&id)?;
        if control.menu.is_some()
            || self
                .popup
                .as_ref()
                .is_some_and(|popup| popup.levels.iter().any(|level| level.anchor == id))
        {
            Some(id)
        } else {
            control
                .dropdown_target
                .filter(|target| self.runtime.tree().node(*target).is_some())
        }
    }
    pub fn expand_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.sync_popup_availability()?;
        let Some(target) = self.popup_target(id) else {
            return Ok(());
        };
        if !self.is_enabled(id) || !self.is_enabled(target) {
            return Ok(());
        }
        if let Some(index) = self
            .popup
            .as_ref()
            .and_then(|popup| popup.levels.iter().position(|level| level.anchor == target))
        {
            return self.focus_popup_item(index, false);
        }
        let entries = self.controls[&target].menu.clone().unwrap();
        self.popup_menu_entries(target, &entries)?;
        Ok(())
    }
    pub fn collapse_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let Some(target) = self.popup_target(id) else {
            return Ok(());
        };
        if let Some(index) = self
            .popup
            .as_ref()
            .and_then(|popup| popup.levels.iter().position(|level| level.anchor == target))
        {
            self.close_popup_from(index, CloseReason::Restore)?;
        }
        Ok(())
    }
    pub(super) fn close_popup_from(
        &mut self,
        index: usize,
        reason: CloseReason,
    ) -> Result<(), ControlError> {
        let Some(mut popup) = self.popup.take() else {
            return Ok(());
        };
        if index >= popup.levels.len() {
            self.popup = Some(popup);
            return Ok(());
        }
        let restore = if index == 0 {
            popup.anchor
        } else {
            popup.levels[index].anchor
        };
        for level in popup.levels.drain(index..).rev() {
            if self.runtime.tree().node(level.container).is_some() {
                self.remove(level.container)?;
            }
        }
        let previous = popup.previous_focus;
        if !popup.levels.is_empty() {
            self.popup = Some(popup);
        }
        self.pressed = None;
        self.keyboard_pressed = false;
        self.runtime.apply(Mutation::CapturePointer(None))?;
        if matches!(reason, CloseReason::Restore) {
            self.refresh_paints()?;
            if !self.try_popup_focus(restore)?
                && index == 0
                && let Some(previous) = previous
            {
                self.try_popup_focus(previous)?;
            }
        }
        self.scene_dirty = true;
        Ok(())
    }
    fn try_popup_focus(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        if !self.window_focused
            || !self.is_enabled(id)
            || !self
                .runtime
                .tree()
                .node(id)
                .is_some_and(|node| node.props.focusable)
        {
            return Ok(false);
        }
        match self.runtime.apply(Mutation::Focus(Some(id))) {
            Ok(()) => Ok(true),
            Err(RuntimeError::IneligibleFocus(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    pub(super) fn sync_popup_availability(&mut self) -> Result<(), ControlError> {
        let invalid = self.popup.as_ref().and_then(|popup| {
            popup.levels.iter().position(|level| {
                !self.window_focused
                    || self.runtime.tree().node(level.container).is_none()
                    || !self.is_enabled(level.anchor)
            })
        });
        if let Some(index) = invalid {
            self.close_popup_from(index, CloseReason::Unavailable)?;
        }
        Ok(())
    }
    pub(super) fn sync_popup_layout(&mut self) -> Result<FrameStats, ControlError> {
        self.sync_popup_availability()?;
        let mut delta = FrameStats::default();
        let count = self.popup_depth();
        for index in 0..count {
            let Some(mut level) = self
                .popup
                .as_ref()
                .and_then(|popup| popup.levels.get(index))
                .cloned()
            else {
                break;
            };
            let Some(anchor) = self.popup_anchor_bounds(level.anchor) else {
                self.close_popup_from(index, CloseReason::Unavailable)?;
                break;
            };
            if level.theme != self.theme {
                level.desired = self.menu_size(&level.entries)?;
                level.theme = self.theme;
                self.set_scroll_extent(level.container, level.desired.height)?;
                for &item in &level.items {
                    if self.runtime.tree().node(item).is_some() {
                        let height = self.theme.tokens().control_height;
                        self.edit_props(item, |props| props.style.height = Length::Px(height))?;
                    }
                }
                let stored = &mut self.popup.as_mut().unwrap().levels[index];
                stored.desired = level.desired;
                stored.theme = level.theme;
            }
            let bounds = place_overlay(anchor, level.desired, self.viewport, index > 0);
            let mut style = self
                .runtime
                .tree()
                .node(level.container)
                .unwrap()
                .props
                .style
                .clone();
            style.width = Length::Px(bounds.width);
            style.height = Length::Px(bounds.height);
            style.offset = Point::new(bounds.x, bounds.y);
            if self
                .runtime
                .tree()
                .node(level.container)
                .unwrap()
                .props
                .style
                != style
            {
                self.runtime.apply(Mutation::SetStyle {
                    node: level.container,
                    style,
                })?;
            }
            if self.runtime.needs_update() {
                add_stats(
                    &mut delta,
                    self.runtime.update(&mut self.layout, self.viewport)?,
                );
            }
        }
        if self.runtime.needs_update() {
            add_stats(
                &mut delta,
                self.runtime.update(&mut self.layout, self.viewport)?,
            );
        }
        Ok(delta)
    }
    fn eligible_popup_items(&self, index: usize) -> Vec<WidgetId> {
        self.popup
            .as_ref()
            .and_then(|popup| popup.levels.get(index))
            .map_or_else(Vec::new, |level| {
                level
                    .items
                    .iter()
                    .copied()
                    .filter(|id| self.is_enabled(*id))
                    .collect()
            })
    }
    fn focus_popup_item(&mut self, index: usize, last: bool) -> Result<(), ControlError> {
        let items = self.eligible_popup_items(index);
        let target = if last { items.last() } else { items.first() }
            .copied()
            .or_else(|| {
                self.popup
                    .as_ref()
                    .and_then(|popup| popup.levels.get(index))
                    .map(|level| level.container)
            });
        if let Some(id) = target {
            self.try_popup_focus(id)?;
            self.ensure_popup_item_visible(index, id)?;
        }
        Ok(())
    }
    fn ensure_popup_item_visible(
        &mut self,
        index: usize,
        id: WidgetId,
    ) -> Result<(), ControlError> {
        let Some(viewport) = self
            .popup
            .as_ref()
            .and_then(|popup| popup.levels.get(index))
            .map(|level| level.container)
        else {
            return Ok(());
        };
        if let (Some(bounds), Some(clip)) = (self.runtime.bounds(id), self.runtime.bounds(viewport))
        {
            let current = self.runtime.tree().node(viewport).unwrap().props.scroll.y;
            if bounds.y < clip.y {
                self.scroll_to(viewport, current + bounds.y - clip.y)?;
            } else if bounds.y + bounds.height > clip.y + clip.height {
                self.scroll_to(
                    viewport,
                    current + bounds.y + bounds.height - clip.y - clip.height,
                )?;
            }
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn popup_escape(&mut self) -> Result<(), ControlError> {
        if self.popup_open() {
            self.close_popup_from(self.popup_depth() - 1, CloseReason::Restore)?;
        }
        Ok(())
    }
    pub(super) fn popup_pointer_down(&mut self, position: Point) -> Result<(), ControlError> {
        let Some(popup) = &self.popup else {
            return Ok(());
        };
        let level = popup.levels.iter().rposition(|level| {
            self.runtime
                .bounds(level.container)
                .is_some_and(|bounds| bounds.contains(position.x, position.y))
        });
        if let Some(index) = level {
            self.close_popup_from(index + 1, CloseReason::Outside)?;
        } else if !(self
            .controls
            .get(&popup.anchor)
            .is_some_and(|control| control.menu.is_some())
            && self
                .runtime
                .bounds(popup.anchor)
                .is_some_and(|bounds| bounds.contains(position.x, position.y)))
        {
            self.close_popup_from(0, CloseReason::Outside)?;
        }
        Ok(())
    }
    pub(super) fn popup_key(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        if modifiers.control || modifiers.alt || modifiers.super_key {
            return Ok(false);
        }
        if *key == Key::Tab {
            if !repeat {
                self.close_popup()?;
            }
            return Ok(*repeat);
        }
        let Some(popup) = &self.popup else {
            return Ok(false);
        };
        let index = self
            .runtime
            .focused()
            .and_then(|id| {
                popup
                    .levels
                    .iter()
                    .position(|level| level.items.contains(&id) || level.container == id)
            })
            .unwrap_or(popup.levels.len() - 1);
        match key {
            Key::ArrowLeft => {
                if index > 0 && !repeat {
                    self.close_popup_from(index, CloseReason::Restore)?;
                }
                Ok(true)
            }
            Key::ArrowRight => {
                if !repeat && let Some(id) = self.runtime.focused() {
                    self.expand_popup(id)?;
                }
                Ok(true)
            }
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End => {
                self.close_popup_from(index + 1, CloseReason::Outside)?;
                let items = self.eligible_popup_items(index);
                if items.is_empty() {
                    return Ok(true);
                }
                let current = items
                    .iter()
                    .position(|id| Some(*id) == self.runtime.focused());
                let next = match key {
                    Key::Home => 0,
                    Key::End => items.len() - 1,
                    Key::ArrowDown => current.map_or(0, |index| (index + 1) % items.len()),
                    _ => current.map_or(items.len() - 1, |index| {
                        (index + items.len() - 1) % items.len()
                    }),
                };
                self.try_popup_focus(items[next])?;
                self.ensure_popup_item_visible(index, items[next])?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

pub(super) fn paint_indicator(
    scene: &mut Scene,
    control: &Control,
    mut bounds: Rect,
    clip: Rect,
    color: Color,
) -> Rect {
    if control.menu.is_none() {
        return bounds;
    }
    let reserved = bounds.width.min(20.0);
    let size = reserved.min(16.0).min(bounds.height);
    let icon = Rect::new(
        bounds.x + bounds.width - reserved + (reserved - size) * 0.5,
        bounds.y + (bounds.height - size) * 0.5,
        size,
        size,
    );
    crate::primitives::paint_icon(
        scene,
        if matches!(control.kind, Kind::MenuItem) {
            IconSymbol::ChevronRight
        } else {
            IconSymbol::ChevronDown
        },
        icon,
        clip,
        color,
    );
    bounds.width -= reserved;
    bounds
}
