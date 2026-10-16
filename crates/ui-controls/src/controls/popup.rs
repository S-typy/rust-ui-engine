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
    /// A submenu whose caption and availability follow a registered command.
    /// Opening it never executes or toggles that command.
    CommandSubmenu {
        command: CommandId,
        items: Vec<MenuEntry>,
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
    // A collapsed Ribbon can use its group as a visual anchor while retaining
    // the original dropdown command as the live availability owner.
    command_owner: Option<CommandId>,
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
    commands: &CommandRegistry,
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
                validate_entries(items, commands, depth + 1, count, bytes)?;
                Some(label.len())
            }
            MenuEntry::CommandSubmenu { command, items } => {
                validate_entries(items, commands, depth + 1, count, bytes)?;
                let label = commands.label(command).ok_or_else(|| {
                    ControlError::Invalid(format!("Unknown submenu command '{}'", command.0))
                })?;
                command.0.len().checked_add(label.len())
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

fn level_entry_index(level: &Level, id: WidgetId) -> Option<usize> {
    let position = level.items.iter().position(|candidate| *candidate == id)?;
    level
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !matches!(entry, MenuEntry::Separator))
        .nth(position)
        .map(|(index, _)| index)
}

fn validate_levels(levels: &[Level], commands: &CommandRegistry) -> Result<(), ControlError> {
    let (mut prefix_count, mut prefix_bytes) = (0usize, 0usize);
    for (index, level) in levels.iter().enumerate() {
        let (mut count, mut bytes) = (prefix_count, prefix_bytes);
        validate_entries(&level.entries, commands, index + 1, &mut count, &mut bytes)?;
        prefix_count += level.entries.len();
        for entry in &level.entries {
            let bytes = match entry {
                MenuEntry::Item(item) => item.label.len().checked_add(item.command.0.len()),
                MenuEntry::Submenu { label, .. } => Some(label.len()),
                MenuEntry::CommandSubmenu { command, .. } => {
                    let label = commands.label(command).ok_or_else(|| {
                        ControlError::Invalid(format!("Unknown submenu command '{}'", command.0))
                    })?;
                    command.0.len().checked_add(label.len())
                }
                MenuEntry::Separator => Some(0),
            };
            prefix_bytes = bytes
                .and_then(|bytes| prefix_bytes.checked_add(bytes))
                .filter(|bytes| *bytes <= MAX_MENU_TEXT)
                .ok_or_else(|| ControlError::Invalid("Menu text exceeds 1 MiB".into()))?;
        }
    }
    Ok(())
}

impl Controls {
    pub(super) fn menu_root_anchor(&self) -> Option<WidgetId> {
        self.popup.as_ref().map(|popup| popup.anchor)
    }
    pub(super) fn menu_chain_depth(&self) -> usize {
        self.popup.as_ref().map_or(0, |popup| popup.levels.len())
    }
    /// Replaces a disclosure's menu atomically after validating the complete
    /// tree. An open level is rebuilt, descendants are closed, and focus follows
    /// the same command when it still exists. Failed validation changes nothing.
    pub fn set_menu_entries(
        &mut self,
        owner: WidgetId,
        entries: &[MenuEntry],
    ) -> Result<(), ControlError> {
        self.preflight_menu(entries)?;
        if !self.runtime.tree().contains(owner)
            || self
                .controls
                .get(&owner)
                .is_none_or(|control| control.menu.is_none())
        {
            return Err(ControlError::Invalid(
                "Menu replacement requires a live disclosure".into(),
            ));
        }
        let level_index = self
            .popup
            .as_ref()
            .and_then(|popup| popup.levels.iter().position(|level| level.anchor == owner));
        let old_focus = self
            .runtime
            .focused()
            .and_then(|id| self.controls.get(&id))
            .and_then(|control| control.command.clone());
        let mut prepared = None;
        if let Some(index) = level_index {
            let mut candidate = self.popup.as_ref().unwrap().levels[..=index].to_vec();
            candidate[index].entries = entries.to_vec();
            for child in (1..=index).rev() {
                let anchor = candidate[child].anchor;
                let items = candidate[child].entries.clone();
                if let Some(entry) = level_entry_index(&candidate[child - 1], anchor) {
                    match &mut candidate[child - 1].entries[entry] {
                        MenuEntry::Submenu {
                            items: existing, ..
                        }
                        | MenuEntry::CommandSubmenu {
                            items: existing, ..
                        } => *existing = items,
                        _ => {}
                    }
                }
            }
            validate_levels(&candidate, &self.commands)?;
            let bounds = self.validate_popup_anchor(owner)?;
            let desired = self.menu_size(entries)?;
            let mut level = self.build_menu_level(owner, entries, desired, bounds, index)?;
            level.command_owner = candidate[index].command_owner.clone();
            candidate[index] = level;
            prepared = Some(candidate);
        }
        self.controls.get_mut(&owner).unwrap().menu = Some(entries.to_vec());
        if let Some(levels) = prepared {
            let index = level_index.unwrap();
            let mut popup = self.popup.take().unwrap();
            let removed: Vec<_> = popup
                .levels
                .drain(index..)
                .map(|level| level.container)
                .collect();
            popup.levels = levels;
            for level in popup.levels.iter().skip(1) {
                if let Some(control) = self.controls.get_mut(&level.anchor) {
                    control.menu = Some(level.entries.clone());
                }
            }
            if let Some(control) = self.controls.get_mut(&popup.anchor)
                && control.menu.is_some()
            {
                control.menu = Some(popup.levels[0].entries.clone());
            }
            self.popup = Some(popup);
            for container in removed {
                self.remove(container)?;
            }
            self.update(self.viewport)?;
            let focused = self.popup.as_ref().unwrap().levels[index]
                .items
                .iter()
                .copied()
                .find(|id| {
                    old_focus.is_some()
                        && self.controls[id].command == old_focus
                        && self.is_enabled(*id)
                });
            if let Some(id) = focused {
                self.try_popup_focus(id)?;
            } else {
                self.focus_popup_item(index, false)?;
            }
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn group_popup_focused(&self) -> bool {
        let (Some(popup), Some(mut focused)) = (&self.popup, self.runtime.focused()) else {
            return false;
        };
        loop {
            if popup.levels.iter().any(|level| level.container == focused) {
                return true;
            }
            let Some(parent) = self.runtime.tree().parent(focused) else {
                return false;
            };
            focused = parent;
        }
    }
    pub(crate) fn preflight_menu(&mut self, entries: &[MenuEntry]) -> Result<(), ControlError> {
        validate_entries(entries, &self.commands, 1, &mut 0, &mut 0)?;
        let mut pending = vec![entries];
        while let Some(entries) = pending.pop() {
            for entry in entries {
                let label = match entry {
                    MenuEntry::Item(item) => Some(item.label.clone()),
                    MenuEntry::Submenu { label, items, .. } => {
                        pending.push(items);
                        Some(label.clone())
                    }
                    MenuEntry::CommandSubmenu { command, items } => {
                        pending.push(items);
                        let label = self.commands.label(command).ok_or_else(|| {
                            ControlError::Invalid(format!(
                                "Unknown submenu command '{}'",
                                command.0
                            ))
                        })?;
                        Some(label.to_owned())
                    }
                    MenuEntry::Separator => None,
                };
                if let Some(label) = label {
                    self.measure_text(&label, 4096.0, false)?;
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
                MenuEntry::Item(item) => Some(item.label.clone()),
                MenuEntry::Submenu { label, .. } => Some(label.clone()),
                MenuEntry::CommandSubmenu { command, .. } => {
                    self.commands.label(command).map(str::to_owned)
                }
                MenuEntry::Separator => None,
            };
            if let Some(label) = label {
                width = width.max(self.measure_text(&label, 4096.0, false)?.width + 64.0);
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
    /// Bind a menu button's caption and availability without binding activation
    /// or checked state. `None` restores the button's original explicit label.
    /// An existing primary command cannot be replaced through this API.
    pub fn set_dropdown_command(
        &mut self,
        id: WidgetId,
        command: Option<CommandId>,
    ) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_none()
            || !self.controls.get(&id).is_some_and(|control| {
                matches!(control.kind, Kind::Button | Kind::MenuItem)
                    && control.menu.is_some()
                    && control.command.is_none()
            })
        {
            return Err(ControlError::Invalid(
                "Dropdown metadata requires an unbound menu button".into(),
            ));
        }
        let mut changed_levels = None;
        if let Some(popup) = &self.popup
            && let Some(level_index) = popup
                .levels
                .iter()
                .position(|level| level.items.contains(&id))
        {
            let mut levels = popup.levels.clone();
            let entry_index = level_entry_index(&levels[level_index], id)
                .ok_or_else(|| ControlError::Invalid("Unknown popup entry".into()))?;
            let control = &self.controls[&id];
            let items = control.menu.clone().unwrap();
            levels[level_index].entries[entry_index] = match &command {
                Some(command) => MenuEntry::CommandSubmenu {
                    command: command.clone(),
                    items,
                },
                None => MenuEntry::Submenu {
                    label: control.label.clone(),
                    items,
                    enabled: control.enabled,
                },
            };
            for index in (1..=level_index).rev() {
                let anchor = levels[index].anchor;
                let Some(parent_entry) = level_entry_index(&levels[index - 1], anchor) else {
                    break;
                };
                let entries = levels[index].entries.clone();
                match &mut levels[index - 1].entries[parent_entry] {
                    MenuEntry::Submenu { items, .. } | MenuEntry::CommandSubmenu { items, .. } => {
                        *items = entries
                    }
                    // An explicitly opened menu can use an ordinary leaf as its
                    // anchor. Its separate level is validated with the prefix.
                    _ => break,
                }
            }
            validate_levels(&levels, &self.commands)?;
            changed_levels = Some(levels);
        }
        if let Some(command) = &command {
            let label = self
                .commands
                .label(command)
                .ok_or_else(|| {
                    ControlError::Invalid(format!("Unknown dropdown command '{}'", command.0))
                })?
                .to_owned();
            TextLimits::default().validate(&label).map_err(edit_error)?;
            self.measure_text(&label, 4096.0, false)?;
        }
        self.controls.get_mut(&id).unwrap().dropdown_command = command;
        if let Some(levels) = changed_levels {
            for level in levels.iter().skip(1) {
                if let Some(control) = self.controls.get_mut(&level.anchor)
                    && control.menu.is_some()
                {
                    control.menu = Some(level.entries.clone());
                }
            }
            self.popup.as_mut().unwrap().levels = levels;
        }
        self.scene_dirty = true;
        self.refresh_paints()?;
        self.sync_popup_availability()?;
        Ok(())
    }
    pub fn split_button(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        command: Option<CommandId>,
        items: &[MenuEntry],
        style: LayoutStyle,
    ) -> Result<SplitButton, ControlError> {
        self.validate_menu_builder(parent, items, &style)?;
        let split = self.split_button_content(parent, label, command, style)?;
        self.controls.get_mut(&split.dropdown).unwrap().menu = Some(items.to_vec());
        Ok(split)
    }
    /// Creates independent primary and disclosure buttons for retained content.
    /// Build a Flyout anchored to `dropdown`, then attach it with
    /// [`Self::set_button_flyout`]. No placeholder menu entries are created.
    pub fn split_button_content(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        command: Option<CommandId>,
        mut style: LayoutStyle,
    ) -> Result<SplitButton, ControlError> {
        if self.runtime.tree().node(parent).is_none() {
            return Err(ControlError::Invalid("Unknown parent".into()));
        }
        style
            .validate()
            .map_err(|message| ControlError::Invalid(message.into()))?;
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
            let dropdown = self.button(
                container,
                "",
                None,
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
        self.open_menu_entries(anchor, entries, None)
    }
    pub(crate) fn popup_menu_for_command(
        &mut self,
        anchor: WidgetId,
        owner: CommandId,
        entries: &[MenuEntry],
    ) -> Result<WidgetId, ControlError> {
        self.open_menu_entries(anchor, entries, Some(owner))
    }
    fn open_menu_entries(
        &mut self,
        anchor: WidgetId,
        entries: &[MenuEntry],
        command_owner: Option<CommandId>,
    ) -> Result<WidgetId, ControlError> {
        if command_owner
            .as_ref()
            .is_some_and(|owner| !self.commands.enabled(owner))
        {
            return Err(ControlError::Invalid(
                "Unavailable popup command owner".into(),
            ));
        }
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
                        MenuEntry::CommandSubmenu { command, .. } => {
                            command.0.len() + self.commands.label(command).map_or(0, str::len)
                        }
                        MenuEntry::Separator => 0,
                    };
                }
            }
        }
        validate_entries(entries, &self.commands, depth + 1, &mut count, &mut bytes)?;
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
        let mut level = self.build_menu_level(anchor, entries, desired, bounds, depth)?;
        level.command_owner = command_owner;
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
            self.close_rich_popup(false)?;
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
            self.controls
                .get_mut(&viewer.viewport)
                .unwrap()
                .popup_surface = true;
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
                    MenuEntry::CommandSubmenu { command, items } => {
                        let id = self.add(
                            viewer.content,
                            Kind::MenuItem,
                            String::new(),
                            None,
                            LayoutStyle::default(),
                        )?;
                        self.controls.get_mut(&id).unwrap().menu = Some(items.clone());
                        self.set_dropdown_command(id, Some(command.clone()))?;
                        id
                    }
                };
                self.edit_props(id, |props| props.tab_stop = false)?;
                if !matches!(entry, MenuEntry::Separator) {
                    ids.push(id);
                }
            }
            Ok(Level {
                command_owner: None,
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
        self.close_rich_popup(true)?;
        self.close_popup_from(0, CloseReason::Restore)?;
        self.close_all_flyouts(true)
    }
    pub fn popup_open(&self) -> bool {
        self.popup.is_some() || self.rich.popup.is_some() || self.flyout_open()
    }
    pub(crate) fn popup_anchor(&self) -> Option<WidgetId> {
        self.popup
            .as_ref()
            .map(|popup| popup.anchor)
            .or_else(|| self.rich.popup.as_ref().map(|p| p.owner))
            .or_else(|| self.flyout_anchor())
    }
    /// Whether a client-local point lies in any currently open menu surface.
    /// Compositors can consume an outside click before forwarding it to another view.
    pub fn popup_contains_point(&self, point: Point) -> bool {
        self.menu_contains_point(point) || self.flyout_contains_point(point)
    }
    pub(super) fn menu_contains_point(&self, point: Point) -> bool {
        self.rich.popup.as_ref().is_some_and(|p| {
            self.runtime
                .bounds(p.surface)
                .is_some_and(|r| r.contains(point.x, point.y))
        }) || self.popup.as_ref().is_some_and(|popup| {
            popup.levels.iter().any(|level| {
                self.runtime
                    .bounds(level.container)
                    .is_some_and(|bounds| bounds.contains(point.x, point.y))
            })
        })
    }
    pub fn popup_depth(&self) -> usize {
        self.flyout_depth()
            + self
                .popup
                .as_ref()
                .map_or(usize::from(self.rich.popup.is_some()), |popup| {
                    popup.levels.len()
                })
    }
    pub fn popup_expanded(&self, id: WidgetId) -> Option<bool> {
        if let Some(expanded) = self.flyout_expanded(id) {
            return Some(expanded);
        }
        if let Some(expanded) = self.rich_popup_expanded(id) {
            return Some(expanded);
        }
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
        if let Some(owner) = self.temporal_popup_owner(id) {
            return if self.temporal_popup_is_open(owner) {
                Ok(())
            } else {
                self.open_temporal_popup(owner)
            };
        }
        if let Some(expanded) = self.flyout_expanded(id) {
            let target = self
                .controls
                .get(&id)
                .and_then(|c| c.dropdown_target)
                .unwrap_or(id);
            if !expanded && self.is_enabled(id) && self.is_enabled(target) {
                self.activate_flyout(target)?;
            }
            return Ok(());
        }
        if self.rich_popup_expanded(id).is_some()
            && let Some(owner) = self.rich_owner(id)
        {
            return self.open_editor_popup(owner);
        }
        self.close_rich_popup(false)?;
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
        if let Some(owner) = self.temporal_popup_owner(id) {
            return if self.temporal_popup_is_open(owner) {
                self.close_temporal_popup(owner)
            } else {
                Ok(())
            };
        }
        if let Some(expanded) = self.flyout_expanded(id) {
            let target = self
                .controls
                .get(&id)
                .and_then(|c| c.dropdown_target)
                .unwrap_or(id);
            if expanded {
                self.activate_flyout(target)?;
            }
            return Ok(());
        }
        if self.rich_popup_expanded(id).is_some() {
            return self.close_rich_popup(true);
        }
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
                    || level
                        .command_owner
                        .as_ref()
                        .is_some_and(|owner| !self.commands.enabled(owner))
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
        if self
            .popup
            .as_ref()
            .is_some_and(|popup| validate_levels(&popup.levels, &self.commands).is_err())
        {
            // Registry labels can change independently of menu construction.
            // Close an invalid chain before shaping or changing its geometry.
            self.close_popup_from(0, CloseReason::Unavailable)?;
            return Ok(delta);
        }
        let count = self.popup.as_ref().map_or(0, |popup| popup.levels.len());
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
            let desired = self.menu_size(&level.entries)?;
            if level.theme != self.theme || level.desired != desired {
                level.desired = desired;
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
        if let Some(count) = self.popup.as_ref().map(|popup| popup.levels.len()) {
            self.close_popup_from(count - 1, CloseReason::Restore)?;
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
    has_popup: bool,
    mut bounds: Rect,
    clip: Rect,
    color: Color,
) -> Rect {
    if !has_popup || control.dropdown_target.is_some() {
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
