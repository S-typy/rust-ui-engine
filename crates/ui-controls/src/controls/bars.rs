//! Retained command bars and their explicit, bounded layout persistence.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_BARS: usize = 32;
const MAX_ITEMS: usize = 128;
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub enum ToolBarItemKind {
    Command(CommandId),
    Separator,
    /// A live, unattached-to-another-bar subtree. Its identity and state remain
    /// unchanged when moved to overflow or another docking position.
    Editor(WidgetId),
}
#[derive(Clone, Debug)]
pub struct ToolBarItem {
    pub key: String,
    pub kind: ToolBarItemKind,
    /// Preferred logical width, in 1..16384. Commands may use zero for measured text.
    pub width: f32,
    pub visible: bool,
}
impl ToolBarItem {
    pub fn command(key: impl Into<String>, command: impl Into<CommandId>) -> Self {
        Self {
            key: key.into(),
            kind: ToolBarItemKind::Command(command.into()),
            width: 0.0,
            visible: true,
        }
    }
    pub fn editor(key: impl Into<String>, widget: WidgetId, width: f32) -> Self {
        Self {
            key: key.into(),
            kind: ToolBarItemKind::Editor(widget),
            width,
            visible: true,
        }
    }
    pub fn separator(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: ToolBarItemKind::Separator,
            width: 9.0,
            visible: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolBar {
    pub container: WidgetId,
    pub content: WidgetId,
    pub overflow: WidgetId,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum BarPlacement {
    Top,
    Bottom,
    Left,
    Right,
    /// A movable surface within this Controls viewport. Native windows are
    /// managed by the platform host, not created by a control.
    Floating {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarManager {
    pub container: WidgetId,
    pub content: WidgetId,
    pub top: WidgetId,
    pub bottom: WidgetId,
    pub left: WidgetId,
    pub right: WidgetId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarSnapshot {
    pub version: u32,
    pub bars: Vec<BarSnapshotEntry>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarSnapshotEntry {
    pub key: String,
    pub placement: BarPlacement,
    pub visible: bool,
    pub items: Vec<BarItemSnapshot>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarItemSnapshot {
    pub key: String,
    pub visible: bool,
}
impl BarSnapshot {
    pub fn to_json(&self) -> Result<String, ControlError> {
        self.validate()?;
        let json =
            serde_json::to_string(self).map_err(|_| invalid("Cannot serialize bar snapshot"))?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(invalid("Bar snapshot exceeds 64KiB"));
        }
        Ok(json)
    }
    pub fn from_json(json: &str) -> Result<Self, ControlError> {
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(invalid("Bar snapshot exceeds 64KiB"));
        }
        let snapshot: Self =
            serde_json::from_str(json).map_err(|_| invalid("Invalid bar snapshot JSON"))?;
        snapshot.validate()?;
        Ok(snapshot)
    }
    fn validate(&self) -> Result<(), ControlError> {
        if self.version != 1 || self.bars.len() > MAX_BARS {
            return Err(invalid("Unsupported bar snapshot version or count"));
        }
        let mut bars = HashSet::new();
        for bar in &self.bars {
            valid_key(&bar.key)?;
            valid_placement(bar.placement)?;
            if !bars.insert(&bar.key) || bar.items.len() > MAX_ITEMS {
                return Err(invalid("Duplicate bar or too many items"));
            }
            let mut keys = HashSet::new();
            for item in &bar.items {
                valid_key(&item.key)?;
                if !keys.insert(&item.key) {
                    return Err(invalid("Duplicate item key"));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct MenuBarItem {
    pub label: String,
    pub entries: Vec<MenuEntry>,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct MenuBar {
    pub container: WidgetId,
    pub items: Vec<WidgetId>,
}

#[derive(Clone, Copy)]
enum HostHeight {
    Auto(f32),
    Explicit(Length),
}
impl HostHeight {
    fn new(height: Length, automatic: f32) -> Self {
        if height == Length::Auto {
            Self::Auto(automatic)
        } else {
            Self::Explicit(height)
        }
    }
    fn resolve(&mut self, automatic: f32) -> Length {
        match self {
            Self::Auto(previous) => {
                *previous = automatic;
                Length::Px(automatic)
            }
            Self::Explicit(height) => *height,
        }
    }
    fn observe(&mut self, current: Length) {
        if !matches!(self, Self::Auto(previous) if current == Length::Px(*previous)) {
            *self = Self::Explicit(current);
        }
    }
}

#[derive(Clone)]
struct MenuStrip {
    handle: MenuBar,
    height: HostHeight,
}

#[derive(Clone)]
struct Item {
    definition: ToolBarItem,
    host: WidgetId,
    widget: WidgetId,
}
#[derive(Clone)]
struct Bar {
    handle: ToolBar,
    key: String,
    items: Vec<Item>,
    flyout: Flyout,
    overflow_viewer: ScrollViewer,
    manager: Option<WidgetId>,
    placement: BarPlacement,
    // Floating geometry temporarily owns height; retain the docked policy so
    // viewport clamping and density changes cannot overwrite it.
    docked_height: HostHeight,
    visible: bool,
    owner_hidden: bool,
    force_order: bool,
}
struct Manager {
    handle: BarManager,
    bars: Vec<WidgetId>,
}
#[derive(Default)]
pub(super) struct BarsState {
    bars: HashMap<WidgetId, Bar>,
    managers: HashMap<WidgetId, Manager>,
    menus: HashMap<WidgetId, MenuStrip>,
}
fn invalid(text: &str) -> ControlError {
    ControlError::Invalid(text.into())
}
fn valid_key(key: &str) -> Result<(), ControlError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        Err(invalid(
            "Bar keys require 1..128 ASCII letters/digits/dot/dash/underscore",
        ))
    } else {
        Ok(())
    }
}
fn valid_placement(placement: BarPlacement) -> Result<(), ControlError> {
    if let BarPlacement::Floating {
        x,
        y,
        width,
        height,
    } = placement
        && (![x, y]
            .iter()
            .all(|value| value.is_finite() && (0.0..=16_384.0).contains(value))
            || ![width, height]
                .iter()
                .all(|value| value.is_finite() && (1.0..=16_384.0).contains(value)))
    {
        return Err(invalid("Floating bar geometry must be finite and bounded"));
    }
    Ok(())
}
impl Controls {
    /// Floating and overflow subtrees keep their logical manager policy even
    /// though their visual parent belongs to the overlay layer.
    pub(super) fn bar_owner_blocks(&self, id: WidgetId) -> bool {
        let mut pending = vec![id];
        let mut visited = HashSet::new();
        while let Some(candidate) = pending.pop() {
            for bar in self.bars.bars.values() {
                let Some(manager) = bar.manager else {
                    continue;
                };
                if !(self.flyout_descendant(candidate, bar.handle.container)
                    || self.flyout_descendant(candidate, bar.flyout.container))
                    || !visited.insert(manager)
                {
                    continue;
                }
                if !self.bar_owner_visible(manager) || self.expander_transition_blocks(manager) {
                    return true;
                }
                let mut ancestor = Some(manager);
                while let Some(node) = ancestor {
                    let enabled = self.controls.get(&node).map_or_else(
                        || {
                            self.runtime
                                .tree()
                                .node(node)
                                .is_some_and(|state| state.props.enabled)
                        },
                        |control| {
                            control.enabled
                                && control
                                    .metadata_command()
                                    .is_none_or(|command| self.commands.enabled(command))
                        },
                    );
                    if !enabled {
                        return true;
                    }
                    ancestor = self.runtime.tree().parent(node);
                }
                pending.push(manager);
            }
        }
        false
    }
    fn bar_owner_visible(&self, id: WidgetId) -> bool {
        if !self.runtime.tree().is_attached(id) {
            return false;
        }
        let mut ancestor = Some(id);
        while let Some(node) = ancestor {
            if !self
                .runtime
                .tree()
                .node(node)
                .is_some_and(|state| state.props.visible && !state.props.collapsed)
            {
                return false;
            }
            ancestor = self.runtime.tree().parent(node);
        }
        true
    }
    pub(super) fn preflight_bar_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.bars.bars.values().any(|bar| {
            bar.handle.content == id
                || bar.handle.overflow == id
                || bar.items.iter().any(|item| item.host == id)
        }) || self.bars.managers.values().any(|manager| {
            [
                manager.handle.content,
                manager.handle.top,
                manager.handle.bottom,
                manager.handle.left,
                manager.handle.right,
            ]
            .contains(&id)
        }) {
            Err(invalid(
                "Remove the bar or manager, not one of its required hosts",
            ))
        } else {
            Ok(())
        }
    }
    /// A persistent horizontal menu strip. Arrow keys traverse the strip;
    /// Up/Down/Enter open its menus through the common menu lifecycle.
    pub fn menu_bar(
        &mut self,
        parent: WidgetId,
        items: &[MenuBarItem],
        mut style: LayoutStyle,
    ) -> Result<MenuBar, ControlError> {
        if items.is_empty() || items.len() > 32 {
            return Err(invalid("MenuBar requires 1..32 menus"));
        }
        if self.bars.menus.len() >= 32 {
            return Err(invalid("At most 32 menu bars are supported"));
        }
        for item in items {
            TextLimits::default()
                .validate(&item.label)
                .map_err(edit_error)?;
            self.preflight_menu(&item.entries)?;
        }
        style.validate().map_err(invalid)?;
        style.kind = LayoutKind::Flex(Axis::Horizontal);
        let mut height = HostHeight::new(style.height, self.theme.tokens().control_height);
        style.height = height.resolve(self.theme.tokens().control_height);
        let container = self.transparent_panel(parent, style)?;
        self.set_role(container, Role::MenuBar);
        let mut buttons = Vec::new();
        for item in items {
            let width = self
                .measure_button_content(
                    &item.label,
                    &ButtonStyle::default(),
                    None,
                    ButtonContentLayout::Leading,
                    16_364.0,
                )?
                .width
                .ceil()
                + 20.0;
            let button = self.dropdown_button(
                container,
                &item.label,
                &item.entries,
                LayoutStyle {
                    width: Length::Px(width),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            self.set_button_chrome(button, ButtonChrome::Flat)?;
            self.set_enabled(button, item.enabled)?;
            self.set_role(button, Role::MenuItem);
            buttons.push(button);
        }
        let handle = MenuBar {
            container,
            items: buttons,
        };
        self.bars.menus.insert(
            container,
            MenuStrip {
                handle: handle.clone(),
                height,
            },
        );
        Ok(handle)
    }
    /// Builds a command bar. Existing Editor subtrees are reparented, never
    /// cloned. A bar owns them after successful construction.
    pub fn tool_bar(
        &mut self,
        parent: WidgetId,
        key: impl Into<String>,
        items: &[ToolBarItem],
        mut style: LayoutStyle,
    ) -> Result<ToolBar, ControlError> {
        let key = key.into();
        valid_key(&key)?;
        style.validate().map_err(invalid)?;
        if !self.runtime.tree().contains(parent)
            || items.is_empty()
            || items.len() > MAX_ITEMS
            || self.bars.bars.len() >= MAX_BARS
        {
            return Err(invalid(
                "ToolBar requires a live parent, 1..128 items, at most 32 bars",
            ));
        }
        let mut keys = HashSet::new();
        let mut editors = HashSet::new();
        for item in items {
            valid_key(&item.key)?;
            if !keys.insert(&item.key)
                || !item.width.is_finite()
                || !(0.0..=16_384.0).contains(&item.width)
            {
                return Err(invalid("Duplicate toolbar key or invalid width"));
            }
            match &item.kind {
                ToolBarItemKind::Command(command) => {
                    let label = self
                        .commands
                        .label(command)
                        .ok_or_else(|| invalid("Unknown toolbar command"))?
                        .to_owned();
                    self.measure_text(&label, 4096.0, false)?;
                }
                ToolBarItemKind::Editor(widget) => {
                    if item.width < 1.0
                        || !editors.insert(*widget)
                        || !self.controls.contains_key(widget)
                        || !self.runtime.tree().contains(*widget)
                        || self.flyout_descendant(parent, *widget)
                        || self.bars.bars.values().any(|bar| {
                            bar.items.iter().any(|item| {
                                item.widget == *widget || self.flyout_descendant(*widget, item.host)
                            })
                        })
                    {
                        return Err(invalid(
                            "Embedded editor must be a distinct live subtree outside every toolbar",
                        ));
                    }
                    self.preflight_rich_removal(*widget)?;
                }
                ToolBarItemKind::Separator => {}
            }
        }
        for &editor in &editors {
            if editors
                .iter()
                .any(|other| *other != editor && self.flyout_descendant(editor, *other))
            {
                return Err(invalid("Embedded editor subtrees must not overlap"));
            }
        }
        style.kind = LayoutKind::Flex(Axis::Horizontal);
        style.gap = 2.0;
        let mut docked_height =
            HostHeight::new(style.height, self.theme.tokens().control_height + 8.0);
        style.height = docked_height.resolve(self.theme.tokens().control_height + 8.0);
        let container = self.panel(parent, style)?;
        self.set_role(container, Role::ToolBar);
        self.set_automation_name(container, &key)?;
        let content = self.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Px(0.0),
                height: Length::Percent(1.0),
                flex_grow: 1.0,
                gap: 2.0,
                ..Default::default()
            },
        )?;
        let overflow = self.button(
            container,
            "More commands",
            None,
            LayoutStyle {
                width: Length::Px(32.0),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        self.set_button_icon(
            overflow,
            Some(crate::ButtonIcon::new(IconSymbol::ChevronDown)),
        )?;
        self.set_button_content_layout(overflow, ButtonContentLayout::IconOnly)?;
        self.set_button_chrome(overflow, ButtonChrome::Flat)?;
        let flyout = self.flyout(overflow, Size::new(320.0, 240.0))?;
        self.set_button_flyout(overflow, flyout)?;
        let overflow_viewer = self.scroll_view(
            flyout.content,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
            240.0,
        )?;
        self.edit_props(overflow_viewer.content, |props| props.style.gap = 6.0)?;
        let handle = ToolBar {
            container,
            content,
            overflow,
        };
        let mut retained = Vec::new();
        for definition in items {
            let host = self.transparent_panel(
                content,
                LayoutStyle {
                    width: Length::Px(definition.width.max(1.0)),
                    height: Length::Percent(1.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            let widget = match &definition.kind {
                ToolBarItemKind::Command(command) => {
                    let label = self.commands.label(command).unwrap_or("").to_owned();
                    let button = self.button(
                        host,
                        label,
                        Some(command.clone()),
                        LayoutStyle {
                            width: Length::Percent(1.0),
                            height: Length::Percent(1.0),
                            ..Default::default()
                        },
                    )?;
                    self.set_button_chrome(button, ButtonChrome::Flat)?;
                    button
                }
                ToolBarItemKind::Separator => self.separator(
                    host,
                    SeparatorSettings {
                        orientation: Axis::Vertical,
                        ..Default::default()
                    },
                    LayoutStyle {
                        width: Length::Px(9.0),
                        height: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?,
                ToolBarItemKind::Editor(widget) => {
                    self.runtime.apply(Mutation::Reparent {
                        node: *widget,
                        parent: host,
                    })?;
                    self.edit_props(*widget, |props| {
                        props.style.width = Length::Percent(1.0);
                        props.style.flex_grow = 1.0;
                    })?;
                    *widget
                }
            };
            retained.push(Item {
                definition: definition.clone(),
                host,
                widget,
            });
        }
        self.bars.bars.insert(
            container,
            Bar {
                handle,
                key,
                items: retained,
                flyout,
                overflow_viewer,
                manager: None,
                placement: BarPlacement::Top,
                docked_height,
                visible: true,
                owner_hidden: false,
                force_order: true,
            },
        );
        self.scene_dirty = true;
        Ok(handle)
    }
    pub fn tool_bar_item(&self, bar: ToolBar, key: &str) -> Option<WidgetId> {
        self.runtime.tree().node(bar.container)?;
        self.bars
            .bars
            .get(&bar.container)
            .filter(|state| state.handle == bar)?
            .items
            .iter()
            .find(|item| item.definition.key == key)
            .map(|item| item.widget)
            .filter(|widget| self.runtime.tree().contains(*widget))
    }
    pub fn set_tool_bar_visible(
        &mut self,
        bar: ToolBar,
        visible: bool,
    ) -> Result<(), ControlError> {
        self.prune_bars();
        let state = self
            .bars
            .bars
            .get_mut(&bar.container)
            .filter(|state| state.handle == bar)
            .ok_or_else(|| invalid("Unknown toolbar"))?;
        state.visible = visible;
        let flyout = state.flyout;
        if !visible && self.flyout_is_open(flyout) {
            self.close_flyout(flyout)?;
        }
        self.edit_props(bar.container, |props| {
            props.visible = visible;
            props.collapsed = !visible;
        })?;
        self.scene_dirty = true;
        Ok(())
    }
    /// All current keys must occur exactly once. Validating before mutation
    /// prevents a failed customization from destroying a live editor session.
    pub fn customize_tool_bar(
        &mut self,
        bar: ToolBar,
        items: &[BarItemSnapshot],
    ) -> Result<(), ControlError> {
        self.prune_bars();
        let state = self
            .bars
            .bars
            .get(&bar.container)
            .filter(|state| state.handle == bar)
            .ok_or_else(|| invalid("Unknown toolbar"))?;
        validate_item_order(state, items)?;
        let reordered = items
            .iter()
            .map(|saved| {
                let mut item = state
                    .items
                    .iter()
                    .find(|item| item.definition.key == saved.key)
                    .unwrap()
                    .clone();
                item.definition.visible = saved.visible;
                item
            })
            .collect();
        let state = self.bars.bars.get_mut(&bar.container).unwrap();
        state.items = reordered;
        state.force_order = true;
        self.scene_dirty = true;
        Ok(())
    }
    /// Creates four docking lanes plus the application's content area. Floating
    /// bars use this window's overlay; this API does not create native windows.
    pub fn bar_manager(
        &mut self,
        parent: WidgetId,
        style: LayoutStyle,
    ) -> Result<BarManager, ControlError> {
        if self.bars.managers.len() >= 16 {
            return Err(invalid("At most 16 bar managers are supported"));
        }
        let container = self.transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Vertical),
                ..style
            },
        )?;
        let top = self.transparent_panel(container, LayoutStyle::default())?;
        let middle = self.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(0.0),
                flex_grow: 1.0,
                ..Default::default()
            },
        )?;
        let left = self.transparent_panel(
            middle,
            LayoutStyle {
                width: Length::Auto,
                ..Default::default()
            },
        )?;
        let content = self.transparent_panel(
            middle,
            LayoutStyle {
                width: Length::Px(0.0),
                flex_grow: 1.0,
                ..Default::default()
            },
        )?;
        let right = self.transparent_panel(
            middle,
            LayoutStyle {
                width: Length::Auto,
                ..Default::default()
            },
        )?;
        let bottom = self.transparent_panel(container, LayoutStyle::default())?;
        let handle = BarManager {
            container,
            content,
            top,
            bottom,
            left,
            right,
        };
        self.bars.managers.insert(
            container,
            Manager {
                handle,
                bars: Vec::new(),
            },
        );
        Ok(handle)
    }
    pub fn place_tool_bar(
        &mut self,
        manager: BarManager,
        bar: ToolBar,
        placement: BarPlacement,
    ) -> Result<(), ControlError> {
        self.prune_bars();
        valid_placement(placement)?;
        let owner = self
            .bars
            .managers
            .get(&manager.container)
            .filter(|state| state.handle == manager)
            .ok_or_else(|| invalid("Unknown bar manager"))?;
        let state = self
            .bars
            .bars
            .get(&bar.container)
            .filter(|state| state.handle == bar)
            .ok_or_else(|| invalid("Unknown toolbar"))?;
        if state.manager.is_some_and(|id| id != manager.container)
            || owner
                .bars
                .iter()
                .any(|id| *id != bar.container && self.bars.bars[id].key == state.key)
            || self.flyout_descendant(manager.container, bar.container)
        {
            return Err(invalid(
                "Toolbar belongs to another manager, repeats a key or contains the manager",
            ));
        }
        let parent = match placement {
            BarPlacement::Top => manager.top,
            BarPlacement::Bottom => manager.bottom,
            BarPlacement::Left => manager.left,
            BarPlacement::Right => manager.right,
            BarPlacement::Floating { .. } => self.overlay,
        };
        let mut style = self
            .runtime
            .tree()
            .node(bar.container)
            .unwrap()
            .props
            .style
            .clone();
        let mut docked_height = state.docked_height;
        if !matches!(state.placement, BarPlacement::Floating { .. }) {
            docked_height.observe(style.height);
        }
        style.offset = Point::ZERO;
        match placement {
            BarPlacement::Floating {
                x,
                y,
                width,
                height,
            } => {
                style.width = Length::Px(width);
                style.height = Length::Px(height);
                style.offset = Point::new(x, y);
            }
            BarPlacement::Left | BarPlacement::Right => {
                style.width = Length::Px(240.0);
                style.height = docked_height.resolve(self.theme.tokens().control_height + 8.0);
            }
            _ => {
                style.width = Length::Percent(1.0);
                style.height = docked_height.resolve(self.theme.tokens().control_height + 8.0);
            }
        }
        self.runtime.apply(Mutation::Batch(vec![
            Mutation::Reparent {
                node: bar.container,
                parent,
            },
            Mutation::SetStyle {
                node: bar.container,
                style,
            },
        ]))?;
        self.edit_props(bar.container, |props| {
            props.z_index = if matches!(placement, BarPlacement::Floating { .. }) {
                200
            } else {
                0
            }
        })?;
        let state = self.bars.bars.get_mut(&bar.container).unwrap();
        state.manager = Some(manager.container);
        state.placement = placement;
        state.docked_height = docked_height;
        let owner = self.bars.managers.get_mut(&manager.container).unwrap();
        if !owner.bars.contains(&bar.container) {
            owner.bars.push(bar.container);
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn bar_snapshot(&self, manager: BarManager) -> Result<BarSnapshot, ControlError> {
        if !self.runtime.tree().contains(manager.container) {
            return Err(invalid("Unknown bar manager"));
        }
        let owner = self
            .bars
            .managers
            .get(&manager.container)
            .filter(|state| state.handle == manager)
            .ok_or_else(|| invalid("Unknown bar manager"))?;
        Ok(BarSnapshot {
            version: 1,
            bars: owner
                .bars
                .iter()
                .filter_map(|id| self.bars.bars.get(id))
                .map(|bar| BarSnapshotEntry {
                    key: bar.key.clone(),
                    placement: bar.placement,
                    visible: if bar.owner_hidden {
                        bar.visible
                    } else {
                        self.runtime
                            .tree()
                            .node(bar.handle.container)
                            .is_some_and(|node| node.props.visible && !node.props.collapsed)
                    },
                    items: bar
                        .items
                        .iter()
                        .map(|item| BarItemSnapshot {
                            key: item.definition.key.clone(),
                            visible: item.definition.visible,
                        })
                        .collect(),
                })
                .collect(),
        })
    }
    pub fn restore_bars(
        &mut self,
        manager: BarManager,
        snapshot: &BarSnapshot,
    ) -> Result<(), ControlError> {
        self.prune_bars();
        snapshot.validate()?;
        let owner = self
            .bars
            .managers
            .get(&manager.container)
            .filter(|state| state.handle == manager)
            .ok_or_else(|| invalid("Unknown bar manager"))?;
        if owner.bars.len() != snapshot.bars.len() {
            return Err(invalid("Snapshot must describe every current bar"));
        }
        let mut plans = Vec::new();
        for saved in &snapshot.bars {
            let bar = owner
                .bars
                .iter()
                .filter_map(|id| self.bars.bars.get(id))
                .find(|bar| bar.key == saved.key)
                .ok_or_else(|| invalid("Snapshot contains unknown bar"))?;
            validate_item_order(bar, &saved.items)?;
            plans.push((bar.handle, saved.clone()));
        }
        // All model validation precedes changes. Existing hosts and editor IDs
        // are reused; no callbacks or external I/O run during this operation.
        for (bar, saved) in plans {
            self.place_tool_bar(manager, bar, saved.placement)?;
            self.customize_tool_bar(bar, &saved.items)?;
            self.set_tool_bar_visible(bar, saved.visible)?;
        }
        Ok(())
    }
    pub(super) fn sync_bars(&mut self) -> Result<(), ControlError> {
        let menus: Vec<_> = self.bars.menus.values().cloned().collect();
        for menu in menus {
            let Some(node) = self.runtime.tree().node(menu.handle.container) else {
                continue;
            };
            let mut height = menu.height;
            height.observe(node.props.style.height);
            let desired = height.resolve(self.theme.tokens().control_height);
            if node.props.style.height != desired {
                self.edit_props(menu.handle.container, |props| props.style.height = desired)?;
            }
            self.bars
                .menus
                .get_mut(&menu.handle.container)
                .unwrap()
                .height = height;
        }
        let headers: Vec<_> = self
            .bars
            .menus
            .values()
            .flat_map(|bar| bar.handle.items.iter().copied())
            .collect();
        for id in headers {
            let Some(control) = self.controls.get(&id) else {
                continue;
            };
            let label = control.label.clone();
            let style = control.button_style;
            let icon = control.button_icon;
            let layout = control.button_content_layout;
            // Disclosure space is additional to the exact content plan, which
            // already includes font metrics, icon spacing and style padding.
            let width = self
                .measure_button_content(&label, &style, icon, layout, 16_364.0)?
                .width
                .ceil()
                + 20.0;
            if self
                .runtime
                .tree()
                .node(id)
                .is_some_and(|node| node.props.style.width != Length::Px(width))
            {
                self.edit_props(id, |props| props.style.width = Length::Px(width))?;
            }
        }
        let orphans: Vec<_> = self
            .bars
            .bars
            .iter()
            .filter_map(|(id, bar)| {
                bar.manager
                    .is_some_and(|manager| !self.runtime.tree().contains(manager))
                    .then_some(*id)
            })
            .collect();
        for id in orphans {
            if self.runtime.tree().contains(id) {
                self.remove(id)?;
            }
        }
        self.prune_bars();
        let bars: Vec<_> = self.bars.bars.values().cloned().collect();
        for mut bar in bars {
            if !matches!(bar.placement, BarPlacement::Floating { .. }) {
                let current = self
                    .runtime
                    .tree()
                    .node(bar.handle.container)
                    .unwrap()
                    .props
                    .style
                    .height;
                bar.docked_height.observe(current);
                let desired = bar
                    .docked_height
                    .resolve(self.theme.tokens().control_height + 8.0);
                if current != desired {
                    self.edit_props(bar.handle.container, |props| props.style.height = desired)?;
                }
                self.bars
                    .bars
                    .get_mut(&bar.handle.container)
                    .unwrap()
                    .docked_height = bar.docked_height;
            }
            if let Some(manager) = bar.manager
                && !self.bar_owner_visible(manager)
            {
                if !bar.owner_hidden {
                    let visible = self
                        .runtime
                        .tree()
                        .node(bar.handle.container)
                        .is_some_and(|node| node.props.visible && !node.props.collapsed);
                    let state = self.bars.bars.get_mut(&bar.handle.container).unwrap();
                    state.visible = visible;
                    state.owner_hidden = true;
                }
                self.edit_props(bar.handle.container, |props| props.visible = false)?;
                continue;
            }
            if bar.owner_hidden {
                self.edit_props(bar.handle.container, |props| props.visible = bar.visible)?;
                self.bars
                    .bars
                    .get_mut(&bar.handle.container)
                    .unwrap()
                    .owner_hidden = false;
            }
            if !self
                .runtime
                .tree()
                .node(bar.handle.container)
                .is_some_and(|node| node.props.visible && !node.props.collapsed)
            {
                continue;
            }
            if let BarPlacement::Floating {
                x,
                y,
                width,
                height,
            } = bar.placement
            {
                let width = width.min(self.viewport.width);
                let height = height.min(self.viewport.height);
                let offset = Point::new(
                    x.min((self.viewport.width - width).max(0.0)),
                    y.min((self.viewport.height - height).max(0.0)),
                );
                self.edit_props(bar.handle.container, |props| {
                    props.style.width = Length::Px(width);
                    props.style.height = Length::Px(height);
                    props.style.offset = offset;
                })?;
            }
            let Some(bounds) = self.runtime.bounds(bar.handle.container) else {
                continue;
            };
            let mut widths = Vec::with_capacity(bar.items.len());
            for item in &bar.items {
                if let ToolBarItemKind::Command(command) = &item.definition.kind {
                    let label = self.commands.label(command).unwrap_or("").to_owned();
                    TextLimits::default().validate(&label).map_err(edit_error)?;
                    if self.controls[&item.widget].label != label {
                        self.set_label(item.widget, label)?;
                    }
                }
                widths.push(if item.definition.width > 0.0 {
                    item.definition.width
                } else if let ToolBarItemKind::Command(command) = &item.definition.kind {
                    let label = self.commands.label(command).unwrap_or("").to_owned();
                    self.measure_text(&label, 4096.0, false)?.width + 24.0
                } else {
                    9.0
                });
            }
            let total: f32 = bar
                .items
                .iter()
                .zip(&widths)
                .filter(|(item, _)| item.definition.visible)
                .map(|(_, width)| width + 2.0)
                .sum();
            let available = (bounds.width - if total > bounds.width { 38.0 } else { 0.0 }).max(0.0);
            let height = self.theme.tokens().control_height;
            let mut used = 0.0;
            let mut overflowed = false;
            let mut overflow_count = 0;
            let mut overflow_width = 160.0_f32;
            let mut mutations = Vec::new();
            for (item, width) in bar.items.iter().zip(widths) {
                let in_overflow =
                    item.definition.visible && (overflowed || used + width > available);
                if item.definition.visible {
                    if in_overflow {
                        overflowed = true;
                        overflow_count += 1;
                        overflow_width = overflow_width.max(width);
                    } else {
                        used += width + 2.0;
                    }
                }
                let target = if in_overflow {
                    bar.overflow_viewer.content
                } else {
                    bar.handle.content
                };
                if bar.force_order || self.runtime.tree().parent(item.host) != Some(target) {
                    mutations.push(Mutation::Reparent {
                        node: item.host,
                        parent: target,
                    });
                }
                let mut props = self.runtime.tree().node(item.host).unwrap().props.clone();
                props.visible = item.definition.visible;
                props.style.width = Length::Px(width);
                props.style.height = Length::Px(height);
                mutations.push(Mutation::SetProps {
                    node: item.host,
                    props,
                });
            }
            self.runtime.apply(Mutation::Batch(mutations))?;
            self.set_scroll_extent(
                bar.overflow_viewer.viewport,
                (overflow_count as f32 * (height + 6.0)).max(1.0),
            )?;
            self.set_flyout_placement(
                bar.flyout,
                Size::new(
                    (overflow_width + 16.0).min(16_384.0),
                    (overflow_count as f32 * (height + 6.0) + 16.0).max(32.0),
                ),
                FlyoutPlacement::Below,
            )?;
            if !overflowed && self.flyout_is_open(bar.flyout) {
                self.close_flyout(bar.flyout)?;
            }
            self.edit_props(bar.handle.overflow, |props| {
                props.visible = overflowed;
                props.collapsed = !overflowed;
            })?;
            self.bars
                .bars
                .get_mut(&bar.handle.container)
                .unwrap()
                .force_order = false;
        }
        Ok(())
    }
    pub(super) fn prune_bars(&mut self) {
        self.bars
            .menus
            .retain(|id, _| self.runtime.tree().contains(*id));
        self.bars
            .managers
            .retain(|id, _| self.runtime.tree().contains(*id));
        self.bars.bars.retain(|id, bar| {
            self.runtime.tree().contains(*id)
                && self.runtime.tree().contains(bar.handle.content)
                && self.runtime.tree().contains(bar.handle.overflow)
        });
        for bar in self.bars.bars.values_mut() {
            bar.items.retain(|item| {
                self.runtime.tree().contains(item.host) && self.runtime.tree().contains(item.widget)
            });
        }
        for owner in self.bars.managers.values_mut() {
            owner.bars.retain(|id| self.bars.bars.contains_key(id));
        }
    }
    pub(super) fn bars_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let InputEvent::KeyDown {
            key: Key::ArrowLeft | Key::ArrowRight,
            modifiers,
            repeat: false,
        } = event
        else {
            return Ok(false);
        };
        if *modifiers != rust_desktop_ui_core::Modifiers::default() {
            return Ok(false);
        }
        let Some(focus) = self.runtime.focused() else {
            return Ok(false);
        };
        let anchor = self.menu_root_anchor();
        let menu = self
            .bars
            .menus
            .values()
            .map(|bar| &bar.handle)
            .find(|bar| {
                bar.items.contains(&focus)
                    || anchor.is_some_and(|anchor| bar.items.contains(&anchor))
            })
            .cloned();
        let Some(menu) = menu else {
            return Ok(false);
        };
        if self.menu_chain_depth() > 1 {
            return Ok(false);
        }
        if matches!(
            event,
            InputEvent::KeyDown {
                key: Key::ArrowRight,
                ..
            }
        ) && self
            .controls
            .get(&focus)
            .is_some_and(|control| control.menu.is_some())
            && !menu.items.contains(&focus)
        {
            return Ok(false);
        }
        let items: Vec<_> = menu
            .items
            .iter()
            .copied()
            .filter(|id| self.is_enabled(*id))
            .collect();
        let Some(index) = items.iter().position(|id| *id == anchor.unwrap_or(focus)) else {
            return Ok(false);
        };
        let next = if matches!(
            event,
            InputEvent::KeyDown {
                key: Key::ArrowRight,
                ..
            }
        ) {
            (index + 1) % items.len()
        } else {
            (index + items.len() - 1) % items.len()
        };
        let was_open = self.popup.is_some();
        self.close_popup_from(0, popup::CloseReason::Outside)?;
        self.runtime.apply(Mutation::Focus(Some(items[next])))?;
        if was_open {
            self.expand_popup(items[next])?;
        }
        self.scene_dirty = true;
        Ok(true)
    }
}
fn validate_item_order(bar: &Bar, items: &[BarItemSnapshot]) -> Result<(), ControlError> {
    if items.len() != bar.items.len() {
        return Err(invalid("Customization must contain each toolbar item"));
    }
    let mut keys = HashSet::new();
    for item in items {
        if !keys.insert(&item.key)
            || !bar
                .items
                .iter()
                .any(|existing| existing.definition.key == item.key)
        {
            return Err(invalid(
                "Customization contains a duplicate or unknown item",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ThemeConfig, ThemeDensity, ThemeMode};
    use rust_desktop_ui_core::Modifiers;
    fn density_theme(density: ThemeDensity) -> Theme {
        Theme::Configured(ThemeConfig::new(ThemeMode::Light).with_density(density))
    }
    fn simple_menu(ui: &mut Controls, height: Length) -> MenuBar {
        ui.menu_bar(
            ui.root(),
            &[MenuBarItem {
                label: "Язык".into(),
                entries: vec![MenuEntry::Item(MenuItem::new("Save", "save"))],
                enabled: true,
            }],
            LayoutStyle {
                height,
                ..Default::default()
            },
        )
        .unwrap()
    }
    fn assert_unclipped(ui: &Controls, id: WidgetId) {
        let bounds = ui.runtime.bounds(id).unwrap();
        let clip = ui.runtime.clip(id).unwrap();
        let visible = bounds.intersection(clip).unwrap();
        assert!(
            (visible.width - bounds.width).abs() < 0.001
                && (visible.height - bounds.height).abs() < 0.001,
            "{id:?}: bounds {bounds:?}, clip {clip:?}"
        );
    }

    #[test]
    fn automatic_bar_heights_follow_live_density_without_clipping_focus_or_replacing_items() {
        let mut ui = Controls::new(density_theme(ThemeDensity::Comfortable)).unwrap();
        ui.commands_mut().register("save", "Save");
        let menu = simple_menu(&mut ui, Length::Auto);
        let editor = ui
            .edit_box(ui.root(), "Query", "retained", LayoutStyle::default())
            .unwrap();
        let bar = ui
            .tool_bar(
                ui.root(),
                "commands",
                &[
                    ToolBarItem::command("save", "save"),
                    ToolBarItem::editor("query", editor, 180.0),
                ],
                LayoutStyle::default(),
            )
            .unwrap();
        let save = ui.tool_bar_item(bar, "save").unwrap();
        for density in [
            ThemeDensity::Comfortable,
            ThemeDensity::Touch,
            ThemeDensity::Compact,
            ThemeDensity::Comfortable,
        ] {
            ui.set_theme(density_theme(density)).unwrap();
            ui.runtime
                .apply(Mutation::Focus(Some(menu.items[0])))
                .unwrap();
            ui.update(Size::new(900.0, 500.0)).unwrap();
            let height = ui.theme.tokens().control_height;
            assert_eq!(ui.runtime.bounds(menu.container).unwrap().height, height);
            assert_eq!(
                ui.runtime.bounds(bar.container).unwrap().height,
                height + 8.0
            );
            assert_eq!(ui.tool_bar_item(bar, "save"), Some(save));
            assert_eq!(ui.tool_bar_item(bar, "query"), Some(editor));
            assert_eq!(ui.document(editor).unwrap().text(), "retained");
            for id in [menu.items[0], save, editor] {
                assert_unclipped(&ui, id);
            }
            let bounds = ui.runtime.bounds(menu.items[0]).unwrap();
            let focus_bounds = Rect::new(
                bounds.x + 2.0,
                bounds.y + 2.0,
                bounds.width - 4.0,
                bounds.height - 4.0,
            );
            let focus = ui
                .scene()
                .rounded_rectangles
                .iter()
                .find(|shape| {
                    shape.bounds == focus_bounds
                        && shape.stroke_width == Some(2.0)
                        && shape.color == ui.theme.tokens().focus
                })
                .unwrap();
            assert_eq!(focus.bounds.intersection(focus.clip), Some(focus.bounds));
            assert_eq!(ui.update(Size::new(900.0, 500.0)).unwrap().layout_passes, 0);
        }
        ui.update(Size::new(160.0, 500.0)).unwrap();
        ui.activate(bar.overflow).unwrap();
        ui.set_theme(density_theme(ThemeDensity::Touch)).unwrap();
        ui.update(Size::new(160.0, 500.0)).unwrap();
        assert!(ui.popup_open());
        assert_eq!(ui.document(editor).unwrap().text(), "retained");
        ui.close_popup().unwrap();
        ui.set_tool_bar_visible(bar, false).unwrap();
        ui.set_theme(density_theme(ThemeDensity::Compact)).unwrap();
        ui.update(Size::new(900.0, 500.0)).unwrap();
        ui.set_tool_bar_visible(bar, true).unwrap();
        ui.update(Size::new(900.0, 500.0)).unwrap();
        assert_eq!(ui.runtime.bounds(bar.container).unwrap().height, 36.0);
        assert_unclipped(&ui, save);
    }

    #[test]
    fn explicit_and_runtime_bar_heights_survive_density_and_keep_intentional_clipping() {
        let mut ui = Controls::new(density_theme(ThemeDensity::Comfortable)).unwrap();
        ui.commands_mut().register("save", "Save");
        let explicit_menu = simple_menu(&mut ui, Length::Px(36.0));
        let auto_menu = simple_menu(&mut ui, Length::Auto);
        let explicit_bar = ui
            .tool_bar(
                ui.root(),
                "explicit",
                &[ToolBarItem::command("save", "save")],
                LayoutStyle {
                    height: Length::Px(20.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let auto_bar = ui
            .tool_bar(
                ui.root(),
                "auto",
                &[ToolBarItem::command("save", "save")],
                LayoutStyle::default(),
            )
            .unwrap();
        ui.update(Size::new(900.0, 500.0)).unwrap();
        ui.edit_props(auto_menu.container, |props| {
            props.style.height = Length::Px(71.0)
        })
        .unwrap();
        ui.edit_props(auto_bar.container, |props| {
            props.style.height = Length::Px(79.0)
        })
        .unwrap();
        for density in [
            ThemeDensity::Touch,
            ThemeDensity::Compact,
            ThemeDensity::Comfortable,
        ] {
            ui.set_theme(density_theme(density)).unwrap();
            ui.update(Size::new(900.0, 500.0)).unwrap();
            for (id, expected) in [
                (explicit_menu.container, 36.0),
                (auto_menu.container, 71.0),
                (explicit_bar.container, 20.0),
                (auto_bar.container, 79.0),
            ] {
                assert_eq!(ui.runtime.bounds(id).unwrap().height, expected);
            }
            let command = ui.tool_bar_item(explicit_bar, "save").unwrap();
            let bounds = ui.runtime.bounds(command).unwrap();
            let clip = ui.runtime.clip(command).unwrap();
            assert_eq!(bounds.intersection(clip).unwrap().height, 20.0);
            assert!(bounds.height > 20.0);
        }
    }

    #[test]
    fn floating_height_preserves_docked_auto_or_explicit_policy_through_clamping() {
        let (mut ui, manager, bar, editor) = fixture();
        ui.set_theme(density_theme(ThemeDensity::Comfortable))
            .unwrap();
        let explicit = ui
            .tool_bar(
                manager.top,
                "explicit",
                &[ToolBarItem::command("save", "save")],
                LayoutStyle {
                    height: Length::Px(63.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.place_tool_bar(manager, explicit, BarPlacement::Top)
            .unwrap();
        for candidate in [bar, explicit] {
            ui.place_tool_bar(
                manager,
                candidate,
                BarPlacement::Floating {
                    x: 0.0,
                    y: 0.0,
                    width: 240.0,
                    height: 91.0,
                },
            )
            .unwrap();
        }
        ui.set_theme(density_theme(ThemeDensity::Touch)).unwrap();
        ui.update(Size::new(900.0, 80.0)).unwrap();
        for candidate in [bar, explicit] {
            assert_eq!(ui.runtime.bounds(candidate.container).unwrap().height, 80.0);
        }
        ui.update(Size::new(900.0, 500.0)).unwrap();
        for candidate in [bar, explicit] {
            assert_eq!(ui.runtime.bounds(candidate.container).unwrap().height, 91.0);
        }
        for placement in [
            BarPlacement::Top,
            BarPlacement::Bottom,
            BarPlacement::Left,
            BarPlacement::Right,
        ] {
            ui.place_tool_bar(manager, bar, placement).unwrap();
            ui.place_tool_bar(manager, explicit, placement).unwrap();
            ui.update(Size::new(900.0, 500.0)).unwrap();
            assert_eq!(ui.runtime.bounds(bar.container).unwrap().height, 52.0);
            assert_eq!(ui.runtime.bounds(explicit.container).unwrap().height, 63.0);
            assert_eq!(ui.tool_bar_item(bar, "query"), Some(editor));
        }
        ui.edit_props(bar.container, |props| props.style.height = Length::Px(73.0))
            .unwrap();
        ui.place_tool_bar(
            manager,
            bar,
            BarPlacement::Floating {
                x: 0.0,
                y: 0.0,
                width: 240.0,
                height: 91.0,
            },
        )
        .unwrap();
        ui.set_theme(density_theme(ThemeDensity::Compact)).unwrap();
        ui.place_tool_bar(manager, bar, BarPlacement::Top).unwrap();
        ui.update(Size::new(900.0, 500.0)).unwrap();
        assert_eq!(ui.runtime.bounds(bar.container).unwrap().height, 73.0);
    }
    fn fixture() -> (Controls, BarManager, ToolBar, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("save", "Save");
        ui.commands_mut().register("toggle", "Toggle");
        ui.commands_mut().set_checked(&"toggle".into(), Some(false));
        let manager = ui
            .bar_manager(
                ui.root(),
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Px(240.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let editor = ui
            .edit_box(manager.content, "Search", "kept", LayoutStyle::default())
            .unwrap();
        let bar = ui
            .tool_bar(
                manager.top,
                "main",
                &[
                    ToolBarItem::command("save", "save"),
                    ToolBarItem::command("toggle", "toggle"),
                    ToolBarItem::editor("query", editor, 180.0),
                ],
                LayoutStyle::default(),
            )
            .unwrap();
        ui.place_tool_bar(manager, bar, BarPlacement::Top).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        (ui, manager, bar, editor)
    }
    #[test]
    fn toolbar_overflow_keeps_editor_identity_toggle_state_and_idle_is_quiet() {
        let (mut ui, _, bar, editor) = fixture();
        let toggle = ui.tool_bar_item(bar, "toggle").unwrap();
        ui.activate(toggle).unwrap();
        assert_eq!(ui.checked(toggle), Some(true));
        ui.set_text(editor, "retained query").unwrap();
        ui.update(Size::new(160.0, 320.0)).unwrap();
        ui.update(Size::new(160.0, 320.0)).unwrap();
        assert!(ui.is_enabled(bar.overflow));
        ui.activate(bar.overflow).unwrap();
        assert!(ui.popup_open());
        assert_eq!(ui.document(editor).unwrap().text(), "retained query");
        assert_eq!(ui.checked(toggle), Some(true));
        ui.activate(toggle).unwrap();
        assert_eq!(ui.checked(toggle), Some(false));
        ui.close_popup().unwrap();
        ui.update(Size::new(600.0, 320.0)).unwrap();
        ui.update(Size::new(600.0, 320.0)).unwrap();
        assert!(!ui.is_enabled(bar.overflow));
        assert!(ui.flyout_descendant(editor, bar.content));
        assert_eq!(ui.tool_bar_item(bar, "query"), Some(editor));
        let stats = ui.update(Size::new(600.0, 320.0)).unwrap();
        assert_eq!(stats.layout_passes, 0);
        assert_eq!(stats.paint_passes, 0);
    }
    #[test]
    fn bar_snapshot_order_visibility_floating_and_failed_restore_are_explicit() {
        let (mut ui, manager, bar, editor) = fixture();
        let initial = ui.bar_snapshot(manager).unwrap();
        let mut changed = initial.clone();
        changed.bars[0].placement = BarPlacement::Floating {
            x: 900.0,
            y: 800.0,
            width: 250.0,
            height: 44.0,
        };
        changed.bars[0].items.reverse();
        changed.bars[0].items[1].visible = false;
        let encoded = changed.to_json().unwrap();
        let roundtrip = BarSnapshot::from_json(&encoded).unwrap();
        ui.restore_bars(manager, &roundtrip).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        let bounds = ui.runtime.bounds(bar.container).unwrap();
        assert!(bounds.x + bounds.width <= 500.0 && bounds.y + bounds.height <= 320.0);
        assert_eq!(ui.document(editor).unwrap().text(), "kept");
        assert_eq!(ui.bar_snapshot(manager).unwrap(), changed);
        let mut broken = changed.clone();
        broken.bars[0].items[0].key = "unknown".into();
        assert!(ui.restore_bars(manager, &broken).is_err());
        assert_eq!(ui.bar_snapshot(manager).unwrap(), changed);
        assert!(BarSnapshot::from_json("{\"version\":2,\"bars\":[]}").is_err());
        assert!(BarSnapshot::from_json(&" ".repeat(MAX_SNAPSHOT_BYTES + 1)).is_err());
        ui.restore_bars(manager, &initial).unwrap();
        assert_eq!(ui.tool_bar_item(bar, "query"), Some(editor));
    }
    #[test]
    fn toolbar_visibility_bindings_are_not_overwritten_and_floating_owner_disposes() {
        let (mut ui, manager, bar, editor) = fixture();
        assert!(ui.remove(bar.content).is_err());
        assert!(ui.remove(bar.overflow).is_err());
        let mut props = ui.runtime.tree().node(bar.container).unwrap().props.clone();
        props.collapsed = true;
        ui.runtime
            .apply(Mutation::SetProps {
                node: bar.container,
                props,
            })
            .unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        assert!(ui.runtime.bounds(bar.container).is_none());
        assert!(!ui.bar_snapshot(manager).unwrap().bars[0].visible);
        ui.set_tool_bar_visible(bar, true).unwrap();
        ui.place_tool_bar(
            manager,
            bar,
            BarPlacement::Floating {
                x: 100.0,
                y: 100.0,
                width: 250.0,
                height: 44.0,
            },
        )
        .unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        assert!(ui.runtime.tree().contains(editor));
        ui.set_enabled(manager.container, false).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        assert!(ui.runtime.tree().node(bar.container).unwrap().props.visible);
        let action = ui.tool_bar_item(bar, "save").unwrap();
        assert!(!ui.is_enabled(action));
        ui.activate(action).unwrap();
        assert!(ui.drain_activated().is_empty());
        assert!(ui.bar_snapshot(manager).unwrap().bars[0].visible);
        ui.set_enabled(manager.container, true).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        assert!(ui.is_enabled(action));
        ui.remove(manager.container).unwrap();
        ui.update(Size::new(500.0, 320.0)).unwrap();
        assert!(!ui.runtime.tree().contains(bar.container));
        assert!(!ui.runtime.tree().contains(editor));
    }
    #[test]
    fn menubar_navigation_dynamic_tree_replacement_and_shared_command_state() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("a", "Alpha");
        ui.commands_mut().register("b", "Beta");
        let menu = ui
            .menu_bar(
                ui.root(),
                &[
                    MenuBarItem {
                        label: "File".into(),
                        entries: vec![MenuEntry::Item(MenuItem::new("Alpha", "a"))],
                        enabled: true,
                    },
                    MenuBarItem {
                        label: "Edit".into(),
                        entries: vec![MenuEntry::Item(MenuItem::new("Beta", "b"))],
                        enabled: true,
                    },
                ],
                LayoutStyle::default(),
            )
            .unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        ui.runtime
            .apply(Mutation::Focus(Some(menu.items[0])))
            .unwrap();
        ui.expand_popup(menu.items[0]).unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowRight,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        assert_eq!(ui.menu_root_anchor(), Some(menu.items[1]));
        ui.set_menu_entries(
            menu.items[1],
            &[
                MenuEntry::Item(MenuItem::new("Alpha", "a")),
                MenuEntry::Item(MenuItem::new("Beta", "b")),
            ],
        )
        .unwrap();
        let focused = ui.runtime.focused().unwrap();
        assert_eq!(ui.controls[&focused].command, Some("b".into()));
        assert!(ui.set_menu_entries(menu.items[1], &[]).is_err());
        assert_eq!(ui.runtime.focused(), Some(focused));
        ui.activate(focused).unwrap();
        assert_eq!(ui.drain_activated(), vec![focused]);
        assert!(!ui.popup_open());
        assert_eq!(ui.runtime.focused(), Some(menu.items[1]));
    }

    #[test]
    fn menubar_measures_full_caption_padding_icon_and_disclosure_after_style_changes() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("run", "Run");
        let bar = ui
            .menu_bar(
                ui.root(),
                &[MenuBarItem {
                    label: "Документ".into(),
                    entries: vec![MenuEntry::Item(MenuItem::new("Run", "run"))],
                    enabled: true,
                }],
                LayoutStyle::default(),
            )
            .unwrap();
        let header = bar.items[0];
        for label in ["Документ", "Язык", "Оформление"] {
            ui.set_label(header, label).unwrap();
            ui.update(Size::new(900.0, 400.0)).unwrap();
            let expected = ui
                .measure_button_content(
                    label,
                    &ButtonStyle::default(),
                    None,
                    ButtonContentLayout::Leading,
                    16_364.0,
                )
                .unwrap();
            assert!(ui.runtime.bounds(header).unwrap().width >= expected.width + 20.0);
        }
        let style = ButtonStyle {
            font_size: Some(19.0),
            padding: Some(Edges::all(13.0)),
            ..Default::default()
        };
        let icon = ButtonIcon {
            symbol: IconSymbol::Save,
            size: 20.0,
            spacing: 9.0,
        };
        ui.set_button_style(header, style).unwrap();
        ui.set_button_icon(header, Some(icon)).unwrap();
        ui.update(Size::new(900.0, 400.0)).unwrap();
        let expected = ui
            .measure_button_content(
                "Оформление",
                &style,
                Some(icon),
                ButtonContentLayout::Leading,
                16_364.0,
            )
            .unwrap();
        assert!(ui.runtime.bounds(header).unwrap().width >= expected.width + 20.0);
        assert_eq!(ui.update(Size::new(900.0, 400.0)).unwrap().layout_passes, 0);
    }
}
