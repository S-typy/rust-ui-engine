//! Command-oriented Ribbon with adaptive group collapse and keyboard KeyTips.
use crate::{
    ButtonChrome, ButtonContentLayout, ButtonIcon, ButtonStyle, CommandId, ControlError, Controls,
    MenuEntry, MenuItem, Role, SeparatorSettings, SplitButton, Theme,
};
use rust_desktop_ui_core::{
    Axis, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Mutation, WidgetId,
};
use std::collections::HashMap;

#[path = "ribbon_menu.rs"]
mod menu;
pub use menu::RibbonMenuEntry;
use menu::{group_entries, item_entries};

#[path = "ribbon_header.rs"]
mod header;
use header::HeaderMeasurements;
pub use header::{RibbonHeader, RibbonHeaderRegions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonItemKind {
    /// Large presentation; registered command state determines activation.
    Large,
    /// Small presentation; also supports a registered toggle command.
    Small,
    /// Legacy large presentation that initializes an existing command as a
    /// toggle once, on this Ribbon's first mount, if checked is unspecified.
    Toggle,
    Split {
        menu: Vec<CommandId>,
    },
    /// Split command with hierarchical secondary menu entries.
    SplitMenu {
        menu: Vec<RibbonMenuEntry>,
    },
    /// Menu owner. Its command supplies a label and availability; opening this
    /// placement never executes the owner or changes its checked value.
    DropDown {
        menu: Vec<RibbonMenuEntry>,
    },
}

impl RibbonItemKind {
    fn is_split(&self) -> bool {
        matches!(self, Self::Split { .. } | Self::SplitMenu { .. })
    }
}

/// Preferred and minimum visual sizes do not change command behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RibbonItemSize {
    IconOnly,
    Small,
    Large,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonItemPresentation {
    pub tab_id: String,
    pub group_id: String,
    pub key_tip: String,
    pub size: RibbonItemSize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SizeLimits {
    preferred: RibbonItemSize,
    minimum: RibbonItemSize,
}
#[derive(Clone, Debug)]
pub struct RibbonItem {
    pub command: CommandId,
    pub kind: RibbonItemKind,
    pub key_tip: String,
}
impl RibbonItem {
    pub fn new(
        command: impl Into<CommandId>,
        kind: RibbonItemKind,
        key_tip: impl Into<String>,
    ) -> Self {
        Self {
            command: command.into(),
            kind,
            key_tip: key_tip.into().to_uppercase(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct RibbonGroup {
    pub id: String,
    pub label: String,
    pub collapse_priority: u16,
    pub items: Vec<RibbonItem>,
}
#[derive(Clone, Debug)]
pub struct RibbonTab {
    pub id: String,
    pub label: String,
    pub key_tip: String,
    pub context: Option<String>,
    pub groups: Vec<RibbonGroup>,
}
#[derive(Clone, Debug, Default)]
pub struct RibbonModel {
    pub tabs: Vec<RibbonTab>,
    pub quick_access: Vec<CommandId>,
}
/// Presentation shared by a command's Ribbon body and quick-access placements.
/// Labels, enabled state and checked state remain owned by the command registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonCommandAppearance {
    pub icon: Option<ButtonIcon>,
    /// `None` uses the command label; an empty string suppresses the tooltip.
    pub tool_tip: Option<String>,
}

impl RibbonCommandAppearance {
    fn validate(&self) -> Result<(), ControlError> {
        if let Some(icon) = self.icon {
            icon.validate()?;
        }
        if self.tool_tip.as_ref().is_some_and(|text| text.len() > 4096) {
            return Err(ControlError::Invalid(
                "Ribbon tooltip text must not exceed 4096 UTF-8 bytes".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonPresentation {
    Full(String),
    Collapsed(String),
    Overflow(Vec<String>),
}
#[derive(Clone, Debug)]
enum Action {
    Tab(String),
    Dropdown,
    Minimize,
    Command(CommandId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tips {
    Hidden,
    Tabs,
    Commands,
}

#[derive(Clone, Copy)]
struct RibbonMetrics {
    caption: f32,
    small: f32,
    large: f32,
    qat: f32,
    tab: f32,
    groups: f32,
    top: f32,
    top_padding: f32,
}

impl RibbonMetrics {
    fn measure(
        ui: &mut Controls,
        model: &RibbonModel,
        widths: &RibbonWidths,
        appearances: &HashMap<CommandId, RibbonCommandAppearance>,
    ) -> Result<Self, ControlError> {
        let compact = ui.theme() == Theme::Compact;
        let text_height = ui.measure_text("Ag", 4096.0, false)?.height.ceil();
        let mut small = (if compact { 26.0_f32 } else { 28.0 }).max(text_height + 8.0);
        let mut large = 0.0_f32;
        let mut qat = 32.0_f32.max(text_height + 8.0);
        let mut tab_height = 34.0_f32.max(text_height + 8.0);
        // Measure the whole model so tab/context/overflow switches keep a stable
        // height for this theme. Measure each actual content arrangement: an
        // Above caption is limited to two lines and cannot inflate other rows.
        for tab in &model.tabs {
            tab_height =
                tab_height.max(ui.measure_text(&tab.label, 4096.0, false)?.height.ceil() + 8.0);
            for group in &tab.groups {
                ui.measure_text(&group.label, MAX_COMMAND_WIDTH, false)?;
                for profile in widths.profiles(&tab.id, &group.id) {
                    for (index, item) in group.items.iter().enumerate() {
                        let layout = profile.sizes[index].layout();
                        let icon = presentation_icon(
                            appearances
                                .get(&item.command)
                                .and_then(|appearance| appearance.icon),
                            layout,
                        );
                        let label = ui
                            .commands()
                            .label(&item.command)
                            .unwrap_or(&item.command.0)
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        let width = profile.items[index];
                        let size = if item.kind.is_split() {
                            ui.measure_split_button_content(
                                &label,
                                &ribbon_button_style(),
                                icon,
                                layout,
                                width,
                            )?
                        } else {
                            let width = if matches!(item.kind, RibbonItemKind::DropDown { .. }) {
                                (width - 20.0).max(0.0)
                            } else {
                                width
                            };
                            ui.measure_button_content(
                                &label,
                                &ribbon_button_style(),
                                icon,
                                layout,
                                width,
                            )?
                        };
                        if layout == ButtonContentLayout::Above {
                            large = large.max(size.height.ceil());
                        } else {
                            small = small.max(size.height.ceil());
                        }
                    }
                }
            }
        }
        for (index, command) in model.quick_access.iter().enumerate() {
            let label = ui
                .commands()
                .label(command)
                .unwrap_or(&command.0)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let icon = presentation_icon(
                appearances
                    .get(command)
                    .and_then(|appearance| appearance.icon),
                ButtonContentLayout::IconOnly,
            );
            qat = qat.max(
                ui.measure_button_content(
                    &label,
                    &ribbon_button_style(),
                    icon,
                    ButtonContentLayout::IconOnly,
                    widths.qat[index],
                )?
                .height
                .ceil(),
            );
        }
        // Split's primary and caption are independent padded hit areas. Its
        // required height can exceed ordinary Above on fonts with taller lines.
        // Grow the shared row pitch so Large still spans exactly three rows.
        small = small.max(((large - 4.0).max(0.0) / 3.0).ceil());
        let large = small * 3.0 + 4.0;
        let caption = 22.0_f32.max(text_height);
        Ok(Self {
            caption,
            small,
            large,
            qat,
            tab: tab_height,
            groups: large + caption + 18.0,
            top: if model.quick_access.is_empty() {
                0.0
            } else {
                qat
            },
            top_padding: 6.0,
        })
    }
    fn total(self, expanded: bool) -> f32 {
        self.top
            + self.tab
            + self.top_padding
            + 6.0
            + if self.top > 0.0 { 4.0 } else { 0.0 }
            + if expanded { self.groups + 4.0 } else { 0.0 }
    }
}

const MAX_COMMAND_WIDTH: f32 = 512.0;
const QAT_TARGET: f32 = 32.0;
const SURFACE_PADDING: f32 = 4.0;

fn ribbon_button_style() -> ButtonStyle {
    ButtonStyle {
        padding: Some(Edges::all(4.0)),
        ..Default::default()
    }
}

fn presentation_icon(icon: Option<ButtonIcon>, layout: ButtonContentLayout) -> Option<ButtonIcon> {
    icon.map(|icon| ButtonIcon {
        symbol: icon.symbol,
        size: if layout == ButtonContentLayout::Above {
            32.0
        } else {
            16.0
        },
        spacing: if layout == ButtonContentLayout::Above {
            4.0
        } else {
            6.0
        },
    })
}

#[path = "ribbon_layout.rs"]
mod layout;
use layout::{MeasuredGroup, RibbonWidths};

struct RibbonMeasurements {
    metrics: RibbonMetrics,
    widths: RibbonWidths,
    header: Option<HeaderMeasurements>,
}

/// Retained Ribbon composition; application logic stays in the command registry.
pub struct Ribbon {
    model: RibbonModel,
    active: String,
    context: Option<String>,
    parent: Option<WidgetId>,
    container: Option<WidgetId>,
    actions: HashMap<WidgetId, Action>,
    tabs: HashMap<String, WidgetId>,
    tip_nodes: HashMap<WidgetId, (Tips, String)>,
    tips: Tips,
    tip_prefix: String,
    dirty: bool,
    width: f32,
    theme: Option<Theme>,
    presentations: Vec<RibbonPresentation>,
    legacy_toggles_initialized: bool,
    command_appearances: HashMap<CommandId, RibbonCommandAppearance>,
    key_tip_return_focus: Option<WidgetId>,
    size_limits: HashMap<(String, String), SizeLimits>,
    item_presentations: Vec<RibbonItemPresentation>,
    minimized: bool,
    transient: bool,
    return_focus: Option<WidgetId>,
    body: Option<WidgetId>,
    owned_menu_open: bool,
    item_anchors: HashMap<String, WidgetId>,
    group_anchors: HashMap<String, WidgetId>,
    header: Option<RibbonHeader>,
    header_row: Option<WidgetId>,
    header_qat: Option<WidgetId>,
}

impl Ribbon {
    /// Sets placement-specific bounds. The preferred size is Large or Small;
    /// the minimum may additionally be IconOnly. Command behavior is unchanged.
    pub fn set_item_size_limits(
        &mut self,
        tab_id: &str,
        key_tip: &str,
        preferred: RibbonItemSize,
        minimum: RibbonItemSize,
    ) -> Result<(), ControlError> {
        let key_tip = key_tip.to_uppercase();
        if preferred == RibbonItemSize::IconOnly
            || minimum > preferred
            || !self.model.tabs.iter().any(|tab| {
                tab.id == tab_id
                    && tab
                        .groups
                        .iter()
                        .flat_map(|group| &group.items)
                        .any(|item| item.key_tip == key_tip)
            })
        {
            return Err(ControlError::Invalid(
                "Invalid Ribbon placement or size bounds".into(),
            ));
        }
        let key = (tab_id.to_owned(), key_tip);
        let value = SizeLimits { preferred, minimum };
        if self.size_limits.get(&key) != Some(&value) {
            self.size_limits.insert(key, value);
            self.dirty = true;
        }
        Ok(())
    }
    pub fn item_presentations(&self) -> &[RibbonItemPresentation] {
        &self.item_presentations
    }
    pub fn is_minimized(&self) -> bool {
        self.minimized
    }
    /// True when the body is visible, including temporary in-flow expansion.
    pub fn is_expanded(&self) -> bool {
        !self.minimized || self.transient
    }
    pub fn set_minimized(&mut self, minimized: bool) {
        if self.minimized != minimized || self.transient {
            self.minimized = minimized;
            self.transient = false;
            self.dirty = true;
        }
    }
    pub fn new(mut model: RibbonModel) -> Result<Self, ControlError> {
        let mut ids = std::collections::HashSet::new();
        let mut tips: Vec<String> = Vec::new();
        for tab in &mut model.tabs {
            tab.key_tip = tab.key_tip.to_uppercase();
            if tab.id.is_empty()
                || !ids.insert(tab.id.clone())
                || !valid_key_tip(&tab.key_tip, &tips)
                || tab
                    .key_tip
                    .starts_with(['1', '2', '3', '4', '5', '6', '7', '8', '9'])
            {
                return Err(ControlError::Invalid(
                    "Ribbon tab IDs must be nonempty and unique; KeyTips must be alphanumeric, prefix-free, and not start with reserved QAT digits 1-9".into(),
                ));
            }
            tips.push(tab.key_tip.clone());
            let mut command_tips = Vec::new();
            let mut group_ids = std::collections::HashSet::new();
            for group in &mut tab.groups {
                if group.id.is_empty() || !group_ids.insert(group.id.clone()) {
                    return Err(ControlError::Invalid(
                        "Ribbon group IDs must be nonempty and unique within a tab".into(),
                    ));
                }
                for item in &mut group.items {
                    item.key_tip = item.key_tip.to_uppercase();
                    if !valid_key_tip(&item.key_tip, &command_tips) {
                        return Err(ControlError::Invalid(
                            "Item KeyTips must be alphanumeric and prefix-free within a tab".into(),
                        ));
                    }
                    command_tips.push(item.key_tip.clone());
                }
            }
        }
        let active = model
            .tabs
            .iter()
            .find(|tab| tab.context.is_none())
            .map_or_else(String::new, |tab| tab.id.clone());
        Ok(Self {
            model,
            active,
            context: None,
            parent: None,
            container: None,
            actions: HashMap::new(),
            tabs: HashMap::new(),
            tip_nodes: HashMap::new(),
            tips: Tips::Hidden,
            tip_prefix: String::new(),
            dirty: true,
            width: 0.0,
            theme: None,
            presentations: Vec::new(),
            legacy_toggles_initialized: false,
            command_appearances: HashMap::new(),
            key_tip_return_focus: None,
            size_limits: HashMap::new(),
            item_presentations: Vec::new(),
            minimized: false,
            transient: false,
            return_focus: None,
            body: None,
            owned_menu_open: false,
            item_anchors: HashMap::new(),
            group_anchors: HashMap::new(),
            header: None,
            header_row: None,
            header_qat: None,
        })
    }
    pub fn model(&self) -> &RibbonModel {
        &self.model
    }
    pub fn active_tab(&self) -> &str {
        &self.active
    }
    pub fn key_tips_visible(&self) -> bool {
        self.tips != Tips::Hidden
    }
    /// Ends KeyTips without changing focus. Hosts use this before explicit
    /// accessibility actions or other input routes outside `handle_input`.
    pub fn cancel_key_tips(&mut self, ui: &mut Controls) {
        self.finish_key_tips(ui, false);
    }
    pub fn presentations(&self) -> &[RibbonPresentation] {
        &self.presentations
    }
    pub fn container(&self) -> Option<WidgetId> {
        self.container
    }
    /// Changes only this Ribbon's presentation of the command. Validation is
    /// atomic; the next `update` applies a changed appearance to all placements.
    pub fn set_command_appearance(
        &mut self,
        command: impl Into<CommandId>,
        appearance: RibbonCommandAppearance,
    ) -> Result<(), ControlError> {
        appearance.validate()?;
        let command = command.into();
        if self.command_appearances.get(&command) != Some(&appearance) {
            self.command_appearances.insert(command, appearance);
            self.dirty = true;
        }
        Ok(())
    }
    pub fn set_context(&mut self, context: Option<&str>) {
        let next = context.map(str::to_owned);
        if self.context != next {
            self.context = next;
            self.ensure_active();
            self.dirty = true;
        }
    }
    pub fn set_active_tab(&mut self, id: &str) -> bool {
        if self.visible_tabs().iter().any(|tab| tab.id == id) {
            if self.active != id {
                self.active = id.into();
                self.dirty = true;
            }
            true
        } else {
            false
        }
    }
    pub fn add_quick_access(&mut self, command: CommandId) -> bool {
        if self.model.quick_access.contains(&command) {
            return false;
        }
        self.model.quick_access.push(command);
        self.dirty = true;
        true
    }
    pub fn remove_quick_access(&mut self, command: &CommandId) -> bool {
        let before = self.model.quick_access.len();
        self.model.quick_access.retain(|id| id != command);
        let changed = before != self.model.quick_access.len();
        self.dirty |= changed;
        changed
    }
    pub fn mount(&mut self, ui: &mut Controls, parent: WidgetId) -> Result<WidgetId, ControlError> {
        self.mount_with_width(ui, parent, 800.0)
    }
    /// Mounts at the actual available width, without an intermediate 800px
    /// layout. A containing panel may own margins, visibility and explicit size.
    pub fn mount_with_width(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        width: f32,
    ) -> Result<WidgetId, ControlError> {
        if ui.runtime.tree().node(parent).is_none() {
            return Err(ControlError::Invalid("Unknown Ribbon parent".into()));
        }
        if let Some(container) = self.container {
            if ui.runtime.tree().node(container).is_none() {
                return Err(ControlError::Invalid(
                    "Ribbon container was removed; create a new Ribbon to mount again".into(),
                ));
            }
            if self.parent != Some(parent) || ui.runtime.tree().parent(container) != Some(parent) {
                return Err(ControlError::Invalid(
                    "Ribbon remount requires its original live parent".into(),
                ));
            }
        }
        self.preflight(ui, width)?;
        self.parent = Some(parent);
        self.dirty = true;
        self.update(ui, width)?;
        self.container
            .ok_or_else(|| ControlError::Invalid("Ribbon was not mounted".into()))
    }
    /// Validates the requested layout, generated menus, text and presentation
    /// before tree or command mutation. Returns the natural logical height.
    pub fn preflight(&self, ui: &mut Controls, width: f32) -> Result<f32, ControlError> {
        Ok(self
            .preflight_metrics(ui, width)?
            .metrics
            .total(self.is_expanded()))
    }
    fn preflight_metrics(
        &self,
        ui: &mut Controls,
        width: f32,
    ) -> Result<RibbonMeasurements, ControlError> {
        if !width.is_finite() || width < 0.0 {
            return Err(ControlError::Invalid("Invalid Ribbon width".into()));
        }
        for appearance in self.command_appearances.values() {
            appearance.validate()?;
        }
        // Badges are independent text. Validate every mode before any subtree
        // mutation so entering KeyTips later only changes painting.
        for tab in &self.model.tabs {
            ui.measure_text(&tab.key_tip, 4096.0, false)?;
            for item in tab.groups.iter().flat_map(|group| &group.items) {
                ui.measure_text(&item.key_tip, 4096.0, false)?;
                if matches!(item.kind, RibbonItemKind::DropDown { .. })
                    && !ui.commands().contains(&item.command)
                {
                    return Err(ControlError::Invalid(format!(
                        "Unknown Ribbon dropdown command '{}'",
                        item.command.0
                    )));
                }
                if item.kind.is_split() {
                    let label = ui
                        .commands()
                        .label(&item.command)
                        .unwrap_or(&item.command.0);
                    rust_desktop_ui_text::TextLimits::default()
                        .validate(&format!("{label} menu"))
                        .map_err(|error| ControlError::Invalid(error.to_string()))?;
                }
            }
        }
        for command in self.model.quick_access.iter().chain(
            self.model
                .tabs
                .iter()
                .flat_map(|tab| &tab.groups)
                .flat_map(|group| &group.items)
                .map(|item| &item.command),
        ) {
            let label = ui.commands().label(command).unwrap_or(&command.0);
            let tooltip = self
                .command_appearances
                .get(command)
                .and_then(|appearance| appearance.tool_tip.as_deref())
                .unwrap_or(label);
            if tooltip.len() > 4096 {
                return Err(ControlError::Invalid(
                    "Ribbon tooltip text must not exceed 4096 UTF-8 bytes".into(),
                ));
            }
        }
        // Validate even hidden/contextual menus before any tree mutation.
        for item in self
            .model
            .tabs
            .iter()
            .flat_map(|tab| &tab.groups)
            .flat_map(|group| &group.items)
        {
            let entries = item_entries(ui, item)?;
            if !entries.is_empty() {
                ui.preflight_menu(&entries)?;
            }
        }
        let mut widths = RibbonWidths::measure(
            ui,
            &self.model,
            &self.command_appearances,
            &self.size_limits,
        )?;
        for tab in &self.model.tabs {
            widths.adapt(tab, (width - 12.0).max(0.0));
        }
        let available = (width - 12.0).max(0.0);
        let mut metrics =
            RibbonMetrics::measure(ui, &self.model, &widths, &self.command_appearances)?;
        let header = self
            .header
            .as_ref()
            .map(|header| HeaderMeasurements::measure(ui, header, width, &widths, metrics.qat))
            .transpose()?;
        if let Some(header) = &header {
            metrics.top = header.height;
            metrics.top_padding = 0.0;
        }
        let qat_available = header
            .as_ref()
            .map_or(available, |header| header.qat_available);
        let capacity = widths.qat_capacity(qat_available);
        let overflow_count = self.model.quick_access.len().saturating_sub(capacity);
        if overflow_count > crate::MAX_MENU_ITEMS {
            return Err(ControlError::Invalid(format!(
                "Ribbon QAT overflow would contain {overflow_count} menu entries; the maximum is {}",
                crate::MAX_MENU_ITEMS
            )));
        }
        if self.model.quick_access.len() > capacity {
            let entries: Vec<_> = self
                .model
                .quick_access
                .iter()
                .skip(capacity)
                .map(|command| {
                    MenuEntry::Item(MenuItem::new(
                        ui.commands().label(command).unwrap_or(&command.0),
                        command.clone(),
                    ))
                })
                .collect();
            ui.preflight_menu(&entries)?;
        }
        let visible = self.visible_tabs();
        if let Some(tab) = visible
            .iter()
            .find(|tab| tab.id == self.active)
            .or(visible.first())
        {
            for presentation in widths.layout_groups(tab, available) {
                let entries: Vec<_> = match presentation {
                    RibbonPresentation::Full(_) => Vec::new(),
                    RibbonPresentation::Collapsed(id) => {
                        let group = tab.groups.iter().find(|group| group.id == id).unwrap();
                        group_entries(ui, group)?
                    }
                    RibbonPresentation::Overflow(_) => {
                        let mut entries = Vec::new();
                        for group in &tab.groups {
                            entries.extend(group_entries(ui, group)?);
                        }
                        entries
                    }
                };
                if !entries.is_empty() {
                    ui.preflight_menu(&entries)?;
                }
            }
        }
        Ok(RibbonMeasurements {
            metrics,
            widths,
            header,
        })
    }
    fn visible_tabs(&self) -> Vec<&RibbonTab> {
        self.model
            .tabs
            .iter()
            .filter(|tab| tab.context.is_none() || tab.context == self.context)
            .collect()
    }
    fn ensure_active(&mut self) {
        if !self.visible_tabs().iter().any(|tab| tab.id == self.active) {
            self.active = self
                .visible_tabs()
                .first()
                .map_or_else(String::new, |tab| tab.id.clone());
        }
    }
    fn initialize_legacy_toggles(&mut self, ui: &mut Controls) {
        if self.legacy_toggles_initialized {
            return;
        }
        // Include hidden contextual tabs before building QAT. Presentation
        // rebuilds must never undo an application's later Some -> None change.
        for item in self
            .model
            .tabs
            .iter()
            .flat_map(|tab| &tab.groups)
            .flat_map(|group| &group.items)
        {
            if matches!(item.kind, RibbonItemKind::Toggle)
                && ui.commands().contains(&item.command)
                && ui.commands().checked(&item.command).is_none()
            {
                ui.commands_mut().set_checked(&item.command, Some(false));
            }
        }
        self.legacy_toggles_initialized = true;
    }
    /// Reflows only on width/theme/model changes. Commands retain their identity and checked state.
    pub fn update(&mut self, ui: &mut Controls, width: f32) -> Result<(), ControlError> {
        if !width.is_finite() || width < 0.0 {
            return Err(ControlError::Invalid("Invalid Ribbon width".into()));
        }
        if !self.dirty && self.width == width && self.theme == Some(ui.theme()) {
            return self.sync_key_tips(ui);
        }
        let Some(parent) = self.parent else {
            return Ok(());
        };
        let measured = self.preflight_metrics(ui, width)?;
        self.remember_external_focus(ui);
        if !self.is_expanded()
            && ui
                .runtime
                .focused()
                .is_some_and(|id| self.body_owns(ui, id))
            && let Some(id) = self.return_focus.filter(|id| ui.is_enabled(*id))
        {
            ui.runtime.apply(Mutation::Focus(Some(id)))?;
        }
        let metrics = measured.metrics;
        self.initialize_legacy_toggles(ui);
        let popup_anchor = ui.popup_anchor().filter(|id| self.owns_node(ui, *id));
        let focused = ui.runtime.focused();
        let restore_action = focused
            .or(popup_anchor)
            .and_then(|id| self.actions.get(&id))
            .cloned()
            .or_else(|| popup_anchor.and_then(|id| self.actions.get(&id)).cloned());
        let restore_ribbon_focus =
            popup_anchor.is_some() || focused.is_some_and(|id| self.owns_node(ui, id));
        if popup_anchor.is_some() {
            ui.close_popup()?;
        }
        if let Some(container) = self.container {
            let children = ui
                .runtime
                .tree()
                .children(container)
                .unwrap_or_default()
                .to_vec();
            for child in children {
                ui.remove(child)?;
            }
        }
        self.actions.clear();
        self.tabs.clear();
        self.tip_nodes.clear();
        self.presentations.clear();
        self.item_presentations.clear();
        self.item_anchors.clear();
        self.group_anchors.clear();
        self.body = None;
        self.header_row = None;
        self.header_qat = None;
        self.ensure_active();
        self.width = width;
        self.theme = Some(ui.theme());
        let style = LayoutStyle {
            kind: LayoutKind::Stack(Axis::Vertical),
            height: Length::Px(metrics.total(self.is_expanded())),
            flex_shrink: 0.0,
            gap: 4.0,
            padding: Edges {
                top: metrics.top_padding,
                ..Edges::all(6.0)
            },
            ..Default::default()
        };
        let container = if let Some(container) = self.container {
            ui.runtime.apply(Mutation::SetStyle {
                node: container,
                style,
            })?;
            container
        } else {
            ui.transparent_panel(parent, style)?
        };
        self.container = Some(container);
        let available = (width - 12.0).max(0.0);
        if let Some(header) = &measured.header {
            self.mount_header(ui, container, header, &measured.widths, metrics.qat)?;
        } else if !self.model.quick_access.is_empty() {
            self.mount_quick_access(
                ui,
                container,
                available,
                available,
                metrics.qat,
                &measured.widths,
            )?;
        }
        let visible: Vec<_> = self.visible_tabs().into_iter().cloned().collect();
        let minimize_width = 32.0_f32.min(available);
        let tabs_available = (available - minimize_width - 8.0).max(0.0);
        let tab_gaps = visible.len().saturating_sub(1) as f32;
        let tab_gap = if tab_gaps > 0.0 {
            4.0_f32.min(tabs_available / tab_gaps)
        } else {
            0.0
        };
        let natural_tabs: f32 = visible
            .iter()
            .map(|tab| measured.widths.tabs[&tab.id])
            .sum();
        let tab_scale = if natural_tabs > 0.0 {
            ((tabs_available - tab_gap * tab_gaps) / natural_tabs).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let tab_row = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(metrics.tab),
                flex_shrink: 0.0,
                gap: tab_gap,
                ..Default::default()
            },
        )?;
        ui.set_role(tab_row, Role::TabList);
        for tab in visible {
            let id = ui.toggle(
                tab_row,
                tab.label.clone(),
                None,
                tab.id == self.active,
                LayoutStyle {
                    width: Length::Px(measured.widths.tabs[&tab.id] * tab_scale),
                    height: Length::Px(metrics.tab),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_role(id, Role::Tab);
            ui.set_button_style(id, ribbon_button_style())?;
            ui.set_button_chrome(id, ButtonChrome::Tab)?;
            self.tip_nodes.insert(id, (Tips::Tabs, tab.key_tip.clone()));
            self.actions.insert(id, Action::Tab(tab.id.clone()));
            self.tabs.insert(tab.id, id);
        }
        ui.transparent_panel(
            tab_row,
            LayoutStyle {
                flex_grow: 1.0,
                ..Default::default()
            },
        )?;
        let minimize_label = if self.minimized {
            "Pin ribbon"
        } else {
            "Minimize ribbon"
        };
        let minimize = ui.button(
            tab_row,
            minimize_label,
            None,
            LayoutStyle {
                width: Length::Px(minimize_width),
                height: Length::Px(metrics.tab),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        ui.set_button_icon(
            minimize,
            Some(ButtonIcon::new(if self.minimized {
                crate::IconSymbol::Pin
            } else {
                crate::IconSymbol::Remove
            })),
        )?;
        ui.set_button_content_layout(minimize, ButtonContentLayout::IconOnly)?;
        ui.set_button_chrome(minimize, ButtonChrome::Flat)?;
        ui.set_button_style(minimize, ribbon_button_style())?;
        ui.set_tooltip(minimize, minimize_label)?;
        self.actions.insert(minimize, Action::Minimize);
        if self.is_expanded() {
            let groups_row = ui.panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(metrics.groups),
                    gap: 0.0,
                    padding: Edges::all(SURFACE_PADDING),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_ribbon_surface(groups_row)?;
            self.body = Some(groups_row);
            if let Some(tab) = self
                .model
                .tabs
                .iter()
                .find(|tab| tab.id == self.active)
                .cloned()
            {
                self.presentations = measured.widths.layout_groups(&tab, available);
                for (index, presentation) in self.presentations.clone().into_iter().enumerate() {
                    if index > 0 {
                        ui.separator(
                            groups_row,
                            SeparatorSettings {
                                orientation: Axis::Vertical,
                                ..Default::default()
                            },
                            LayoutStyle {
                                width: Length::Px(1.0),
                                height: Length::Percent(1.0),
                                margin: Edges {
                                    left: 2.5,
                                    right: 2.5,
                                    top: 6.0,
                                    bottom: 6.0,
                                },
                                flex_shrink: 0.0,
                                ..Default::default()
                            },
                        )?;
                    }
                    match presentation {
                        RibbonPresentation::Full(id) => {
                            if let Some(group) = tab.groups.iter().find(|g| g.id == id) {
                                self.mount_group(
                                    ui,
                                    groups_row,
                                    group,
                                    metrics,
                                    measured.widths.group(&tab.id, &group.id),
                                )?;
                            }
                        }
                        RibbonPresentation::Collapsed(id) => {
                            if let Some(group) = tab.groups.iter().find(|g| g.id == id) {
                                let entries = group_entries(ui, group)?;
                                let button = ribbon_dropdown(
                                    ui,
                                    groups_row,
                                    group.label.clone(),
                                    &entries,
                                    LayoutStyle {
                                        width: Length::Px(
                                            measured.widths.group(&tab.id, &group.id).collapsed,
                                        ),
                                        height: Length::Percent(1.0),
                                        flex_shrink: 0.0,
                                        ..Default::default()
                                    },
                                )?;
                                ui.set_button_style(button, ribbon_button_style())?;
                                ui.set_button_chrome(button, ButtonChrome::Flat)?;
                                self.actions.insert(button, Action::Dropdown);
                                self.group_anchors.insert(group.id.clone(), button);
                            }
                        }
                        RibbonPresentation::Overflow(_) => {
                            let mut items = Vec::new();
                            for group in &tab.groups {
                                items.extend(group_entries(ui, group)?);
                            }
                            let button = ribbon_dropdown(
                                ui,
                                groups_row,
                                "All commands",
                                &items,
                                LayoutStyle {
                                    width: Length::Px((available - SURFACE_PADDING * 2.0).max(0.0)),
                                    height: Length::Px(48.0),
                                    flex_shrink: 0.0,
                                    ..Default::default()
                                },
                            )?;
                            ui.set_button_style(button, ribbon_button_style())?;
                            ui.set_button_chrome(button, ButtonChrome::Flat)?;
                            self.actions.insert(button, Action::Dropdown);
                            for group in &tab.groups {
                                self.group_anchors.insert(group.id.clone(), button);
                            }
                        }
                    }
                }
            }
        }
        self.dirty = false;
        self.sync_key_tips(ui)?;
        // Focus can be restored before layout because eligibility is a logical tree property.
        let restored = ui
            .runtime
            .tree()
            .preorder()
            .into_iter()
            .find(|id| {
                ui.is_enabled(*id)
                    && match (&restore_action, self.actions.get(id)) {
                        (Some(Action::Command(command)), Some(Action::Command(candidate))) => {
                            command == candidate
                        }
                        (Some(Action::Tab(tab)), Some(Action::Tab(candidate))) => tab == candidate,
                        (Some(Action::Minimize), Some(Action::Minimize)) => true,
                        _ => false,
                    }
            })
            .or_else(|| {
                restore_ribbon_focus
                    .then(|| self.tabs.get(&self.active).copied())
                    .flatten()
                    .filter(|id| ui.is_enabled(*id))
            });
        if let Some(id) = restored {
            let _ = ui.runtime.apply(Mutation::Focus(Some(id)));
        }
        Ok(())
    }
    fn owns_node(&self, ui: &Controls, mut id: WidgetId) -> bool {
        loop {
            if Some(id) == self.container {
                return true;
            }
            let Some(parent) = ui.runtime.tree().parent(id) else {
                return false;
            };
            id = parent;
        }
    }
    fn body_owns(&self, ui: &Controls, id: WidgetId) -> bool {
        std::iter::successors(Some(id), |id| ui.runtime.tree().parent(*id))
            .any(|id| Some(id) == self.body)
    }
    fn remember_external_focus(&mut self, ui: &Controls) {
        if let Some(id) = ui.runtime.focused()
            && !self.owns_node(ui, id)
            && !ui.popup_open()
        {
            self.return_focus = Some(id);
        }
    }
    fn reveal(&mut self, ui: &Controls) {
        if self.minimized && !self.transient {
            self.remember_external_focus(ui);
            self.transient = true;
            self.dirty = true;
        }
    }
    fn close_transient(&mut self, ui: &mut Controls, restore: bool) {
        if !self.transient {
            return;
        }
        self.transient = false;
        self.dirty = true;
        self.finish_key_tips(ui, false);
        if restore
            && let Some(id) = self.return_focus.filter(|id| {
                ui.is_enabled(*id)
                    && ui
                        .runtime
                        .tree()
                        .node(*id)
                        .is_some_and(|node| node.props.focusable)
            })
        {
            let _ = ui.runtime.apply(Mutation::Focus(Some(id)));
        }
    }
    fn apply_command_appearance(
        &self,
        ui: &mut Controls,
        id: WidgetId,
        command: &CommandId,
        label: &str,
        layout: ButtonContentLayout,
    ) -> Result<(), ControlError> {
        let appearance = self.command_appearances.get(command);
        ui.set_ribbon_command(id)?;
        ui.set_button_style(id, ribbon_button_style())?;
        ui.set_button_chrome(id, ButtonChrome::Flat)?;
        ui.set_button_content_layout(id, layout)?;
        ui.set_button_icon(
            id,
            presentation_icon(appearance.and_then(|appearance| appearance.icon), layout),
        )?;
        ui.set_tooltip(
            id,
            appearance
                .and_then(|appearance| appearance.tool_tip.as_deref())
                .unwrap_or(label),
        )
    }
    fn mount_group(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        group: &RibbonGroup,
        metrics: RibbonMetrics,
        measured: &MeasuredGroup,
    ) -> Result<(), ControlError> {
        let panel = ui.transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                width: Length::Px(measured.width),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                padding: Edges::all(4.0),
                gap: 2.0,
                ..Default::default()
            },
        )?;
        ui.set_role(panel, Role::Group);
        ui.set_automation_name(panel, &group.label)?;
        let row = ui.transparent_panel(
            panel,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(metrics.large),
                gap: 4.0,
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        let mut small_column = None;
        let mut small_count = 0;
        for (index, item) in group.items.iter().enumerate() {
            let label = ui
                .commands()
                .label(&item.command)
                .unwrap_or(&item.command.0)
                .to_owned();
            let item_size = measured.sizes[index];
            let layout = item_size.layout();
            self.item_presentations.push(RibbonItemPresentation {
                tab_id: self.active.clone(),
                group_id: group.id.clone(),
                key_tip: item.key_tip.clone(),
                size: item_size,
            });
            let (host, height, width) = if item_size != RibbonItemSize::Large {
                if small_count % 3 == 0 {
                    small_column = Some(ui.transparent_panel(
                        row,
                        LayoutStyle {
                            kind: LayoutKind::Stack(Axis::Vertical),
                            width: Length::Px(measured.items[index]),
                            height: Length::Percent(1.0),
                            gap: 2.0,
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?);
                }
                small_count += 1;
                (
                    small_column.unwrap_or(row),
                    metrics.small,
                    measured.items[index],
                )
            } else {
                small_count = 0;
                small_column = None;
                (row, metrics.large, measured.items[index])
            };
            let style = LayoutStyle {
                width: Length::Px(width),
                height: Length::Px(height),
                flex_shrink: 0.0,
                ..Default::default()
            };
            let id = match &item.kind {
                RibbonItemKind::Split { .. } | RibbonItemKind::SplitMenu { .. } => {
                    let entries = item_entries(ui, item)?;
                    let split = if entries.is_empty() {
                        // Preserve an independent, disabled menu area even when
                        // the application supplies no secondary commands.
                        let container = ui.transparent_panel(host, style)?;
                        ui.set_role(container, Role::Group);
                        let primary = ui.button(
                            container,
                            label.clone(),
                            Some(item.command.clone()),
                            LayoutStyle::default(),
                        )?;
                        let dropdown = ui.button(container, "", None, LayoutStyle::default())?;
                        ui.set_enabled(dropdown, false)?;
                        SplitButton {
                            container,
                            primary,
                            dropdown,
                        }
                    } else {
                        ui.split_button(
                            host,
                            label.clone(),
                            Some(item.command.clone()),
                            &entries,
                            style,
                        )?
                    };
                    self.apply_command_appearance(
                        ui,
                        split.primary,
                        &item.command,
                        &label,
                        layout,
                    )?;
                    ui.set_button_style(split.dropdown, ribbon_button_style())?;
                    ui.set_ribbon_command(split.dropdown)?;
                    ui.set_split_button_presentation(split, layout)?;
                    ui.set_automation_name(split.dropdown, format!("{label} menu"))?;
                    ui.set_tooltip(
                        split.dropdown,
                        self.command_appearances
                            .get(&item.command)
                            .and_then(|appearance| appearance.tool_tip.as_deref())
                            .unwrap_or(&label),
                    )?;
                    self.actions.insert(split.dropdown, Action::Dropdown);
                    split.primary
                }
                RibbonItemKind::DropDown { .. } => {
                    let entries = item_entries(ui, item)?;
                    let id = ui.dropdown_button(host, label.clone(), &entries, style)?;
                    ui.set_dropdown_command(id, Some(item.command.clone()))?;
                    self.apply_command_appearance(ui, id, &item.command, &label, layout)?;
                    id
                }
                _ => {
                    let id = ui.button(host, label.clone(), Some(item.command.clone()), style)?;
                    self.apply_command_appearance(ui, id, &item.command, &label, layout)?;
                    id
                }
            };
            self.tip_nodes
                .insert(id, (Tips::Commands, item.key_tip.clone()));
            self.item_anchors.insert(item.key_tip.clone(), id);
            self.actions.insert(
                id,
                if matches!(item.kind, RibbonItemKind::DropDown { .. }) {
                    Action::Dropdown
                } else {
                    Action::Command(item.command.clone())
                },
            );
        }
        let caption = ui.label(
            panel,
            group.label.clone(),
            LayoutStyle {
                height: Length::Px(metrics.caption),
                ..Default::default()
            },
        )?;
        ui.set_ribbon_group_caption(caption)?;
        Ok(())
    }
    fn sync_key_tips(&self, ui: &mut Controls) -> Result<(), ControlError> {
        for (id, (mode, label)) in &self.tip_nodes {
            if ui.runtime.tree().node(*id).is_some() {
                let label = (*mode == self.tips && ui.is_enabled(*id)).then(|| label.clone());
                if ui.key_tip(*id) != label.as_deref() {
                    ui.set_key_tip(*id, label)?;
                }
            }
        }
        Ok(())
    }
    /// Call after `Controls::dispatch` to process tab/dropdown activations.
    /// Returns activations belonging to other parts of the application.
    pub fn process(&mut self, ui: &mut Controls) -> Result<Vec<WidgetId>, ControlError> {
        // Keyboard traversal and accessibility may transfer focus without a
        // pointer event. Closing must retain that newly selected external owner.
        if self.transient
            && !ui.popup_open()
            && ui
                .runtime
                .focused()
                .is_some_and(|id| !self.owns_node(ui, id))
        {
            self.close_transient(ui, false);
        }
        self.remember_external_focus(ui);
        let mut unhandled = Vec::new();
        let menu_was_owned = self.owned_menu_open;
        for id in ui.drain_activated() {
            match self.actions.get(&id).cloned() {
                Some(Action::Tab(tab)) => {
                    self.active = tab;
                    self.reveal(ui);
                    self.dirty = true;
                }
                Some(Action::Minimize) => {
                    let minimized = !self.minimized;
                    if minimized {
                        self.close_transient(ui, true);
                    }
                    self.set_minimized(minimized);
                    self.finish_key_tips(ui, false);
                }
                Some(Action::Dropdown) => {}
                Some(Action::Command(_)) => {
                    if self.body_owns(ui, id) {
                        self.close_transient(ui, true);
                    }
                }
                None => {
                    // Successful menu selection removes its visual node. A
                    // closed owned chain plus that activation identifies it;
                    // Escape alone emits no activation and leaves the body open.
                    if menu_was_owned && !ui.popup_open() && ui.runtime.tree().node(id).is_none() {
                        self.close_transient(ui, true);
                    }
                    unhandled.push(id);
                }
            }
        }
        self.owned_menu_open = ui.popup_anchor().is_some_and(|id| self.body_owns(ui, id));
        Ok(unhandled)
    }
    /// Call before ordinary input dispatch. Alt/F10 then tab and item KeyTips
    /// executes commands even when their groups are collapsed into overflow.
    pub fn handle_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        self.remember_external_focus(ui);
        self.owned_menu_open |= ui.popup_anchor().is_some_and(|id| self.body_owns(ui, id));
        if let InputEvent::PointerDown { position, .. } = event {
            let inside = ui
                .runtime
                .hit_test(*position)
                .is_some_and(|id| self.owns_node(ui, id));
            let in_menu = ui.popup_anchor().is_some_and(|id| self.owns_node(ui, id))
                && ui.popup_contains_point(*position);
            if !inside && !in_menu {
                self.close_transient(ui, false);
            }
            self.finish_key_tips(ui, false);
            return Ok(false);
        }
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.close_transient(ui, false);
            self.finish_key_tips(ui, false);
            return Ok(false);
        }
        if matches!(event, InputEvent::Wheel { .. }) {
            self.finish_key_tips(ui, false);
            return Ok(false);
        }
        if !self.container.is_some_and(|id| ui.is_enabled(id)) {
            self.finish_key_tips(ui, false);
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
        // An open menu owns keyboard navigation, including Escape and arrows.
        if ui.popup_open() {
            return Ok(false);
        }
        if *key == Key::Escape && self.transient {
            self.close_transient(ui, true);
            return Ok(true);
        }
        if self.tips == Tips::Hidden
            && !modifiers.alt
            && !modifiers.control
            && !modifiers.super_key
            && let Some(Action::Tab(focused)) =
                ui.runtime.focused().and_then(|id| self.actions.get(&id))
        {
            let visible = self.visible_tabs();
            if let Some(index) = visible.iter().position(|tab| tab.id == *focused) {
                let next = match key {
                    Key::ArrowLeft => Some((index + visible.len() - 1) % visible.len()),
                    Key::ArrowRight => Some((index + 1) % visible.len()),
                    Key::Home => Some(0),
                    Key::End => Some(visible.len() - 1),
                    _ => None,
                };
                if let Some(next) = next {
                    let tab = visible[next].id.clone();
                    if let Some(id) = self.tabs.get(&tab).copied().filter(|id| ui.is_enabled(*id)) {
                        ui.runtime.apply(Mutation::Focus(Some(id)))?;
                        self.active = tab;
                        self.dirty = true;
                        // Arrow navigation selects tabs but does not open a
                        // minimized body until an explicit tab activation.
                        ui.dismiss_tooltip();
                        return Ok(true);
                    }
                }
            }
        }
        if *repeat {
            return Ok(self.tips != Tips::Hidden);
        }
        if matches!(key, Key::Alt | Key::F10) && !modifiers.control && !modifiers.super_key {
            ui.dismiss_tooltip();
            if self.tips == Tips::Hidden {
                self.key_tip_return_focus = ui.runtime.focused();
                self.tips = Tips::Tabs;
                self.tip_prefix.clear();
                self.sync_key_tips(ui)?;
                if let Some(id) = self
                    .tabs
                    .get(&self.active)
                    .copied()
                    .filter(|id| ui.is_enabled(*id))
                {
                    ui.runtime.apply(Mutation::Focus(Some(id)))?;
                }
            } else {
                self.finish_key_tips(ui, true);
            }
            return Ok(true);
        }
        if self.tips == Tips::Hidden {
            return Ok(false);
        }
        if *key == Key::Escape {
            if self.tips == Tips::Commands {
                self.tips = Tips::Tabs;
                self.tip_prefix.clear();
                self.sync_key_tips(ui)?;
            } else {
                self.finish_key_tips(ui, true);
            }
            return Ok(true);
        }
        if *key == Key::Tab {
            self.finish_key_tips(ui, false);
            return Ok(false);
        }
        if modifiers.control || modifiers.super_key {
            self.finish_key_tips(ui, false);
            return Ok(false);
        }
        let Key::Character(character) = key else {
            return Ok(false);
        };
        self.tip_prefix.push_str(&character.to_uppercase());
        if self.tips == Tips::Tabs {
            if let Ok(index) = self.tip_prefix.parse::<usize>()
                && index > 0
                && index <= 9
                && let Some(command) = self.model.quick_access.get(index - 1)
            {
                ui.commands_mut().activate(command);
                self.finish_key_tips(ui, true);
                return Ok(true);
            }
            let visible = self.visible_tabs();
            if let Some(tab) = visible
                .iter()
                .find(|tab| tab.key_tip.eq_ignore_ascii_case(&self.tip_prefix))
            {
                let id = tab.id.clone();
                self.dirty |= self.active != id;
                self.active = id;
                self.reveal(ui);
                self.tips = Tips::Commands;
                self.tip_prefix.clear();
                self.sync_key_tips(ui)?;
                if let Some(id) = self
                    .tabs
                    .get(&self.active)
                    .copied()
                    .filter(|id| ui.is_enabled(*id))
                {
                    ui.runtime.apply(Mutation::Focus(Some(id)))?;
                }
            } else if !visible
                .iter()
                .any(|tab| tab.key_tip.to_uppercase().starts_with(&self.tip_prefix))
            {
                self.tip_prefix.clear();
            }
        } else if let Some(tab) = self.model.tabs.iter().find(|tab| tab.id == self.active) {
            let items: Vec<_> = tab.groups.iter().flat_map(|group| &group.items).collect();
            if let Some(item) = items
                .iter()
                .find(|item| item.key_tip.eq_ignore_ascii_case(&self.tip_prefix))
            {
                let item = (*item).clone();
                if matches!(item.kind, RibbonItemKind::DropDown { .. }) {
                    if ui.commands().enabled(&item.command) {
                        let entries = item_entries(ui, &item)?;
                        let group = tab
                            .groups
                            .iter()
                            .find(|group| {
                                group
                                    .items
                                    .iter()
                                    .any(|candidate| candidate.key_tip == item.key_tip)
                            })
                            .unwrap();
                        let anchor = self
                            .item_anchors
                            .get(&item.key_tip)
                            .or_else(|| self.group_anchors.get(&group.id))
                            .copied();
                        if let Some(anchor) = anchor.filter(|id| ui.is_enabled(*id)) {
                            self.finish_key_tips(ui, false);
                            ui.popup_menu_for_command(anchor, item.command.clone(), &entries)?;
                            self.owned_menu_open = true;
                        }
                    }
                } else {
                    let activated = ui.commands_mut().activate(&item.command);
                    self.finish_key_tips(ui, true);
                    if activated {
                        self.close_transient(ui, true);
                    }
                }
            } else if !items
                .iter()
                .any(|item| item.key_tip.to_uppercase().starts_with(&self.tip_prefix))
            {
                self.tip_prefix.clear();
            }
        }
        Ok(true)
    }
    fn finish_key_tips(&mut self, ui: &mut Controls, restore: bool) {
        if self.tips != Tips::Hidden || !self.tip_prefix.is_empty() {
            self.tips = Tips::Hidden;
            self.tip_prefix.clear();
            // Clearing an existing badge cannot fail validation. Removed owners
            // are skipped; the next reflow will create fresh badge metadata.
            let _ = self.sync_key_tips(ui);
        }
        if let Some(id) = self.key_tip_return_focus.take()
            && restore
            && ui.is_enabled(id)
            && ui
                .runtime
                .tree()
                .node(id)
                .is_some_and(|node| node.props.focusable)
        {
            let _ = ui.runtime.apply(Mutation::Focus(Some(id)));
        }
    }
}

fn ribbon_dropdown(
    ui: &mut Controls,
    parent: WidgetId,
    label: impl Into<String>,
    entries: &[MenuEntry],
    style: LayoutStyle,
) -> Result<WidgetId, ControlError> {
    if entries.is_empty() {
        let id = ui.button(parent, label, None, style)?;
        ui.set_enabled(id, false)?;
        Ok(id)
    } else {
        ui.dropdown_button(parent, label, entries, style)
    }
}

fn valid_key_tip(candidate: &str, existing: &[String]) -> bool {
    !candidate.is_empty()
        && candidate.chars().all(char::is_alphanumeric)
        && existing
            .iter()
            .all(|tip| !tip.starts_with(candidate) && !candidate.starts_with(tip))
}

const COLLAPSED: f32 = 88.0;
fn group_width(group: &RibbonGroup) -> f32 {
    let mut width = 8.0;
    let mut small = 0;
    for item in &group.items {
        if matches!(item.kind, RibbonItemKind::Small) {
            if small % 3 == 0 {
                width += 132.0;
            }
            small += 1;
        } else {
            small = 0;
            width += 108.0;
        }
    }
    width
}
/// Deterministic adaptive plan. At tiny widths one menu retains every command.
pub fn layout_groups(groups: &[RibbonGroup], width: f32) -> Vec<RibbonPresentation> {
    plan_groups(
        groups,
        width,
        groups.iter().map(group_width).collect(),
        vec![COLLAPSED; groups.len()],
    )
}

fn plan_groups(
    groups: &[RibbonGroup],
    width: f32,
    mut widths: Vec<f32>,
    collapsed_widths: Vec<f32>,
) -> Vec<RibbonPresentation> {
    if groups.is_empty() {
        return Vec::new();
    }
    let gap = 6.0 * (groups.len() - 1) as f32;
    let mut collapsed = vec![false; groups.len()];
    let mut order: Vec<_> = (0..groups.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(groups[index].collapse_priority));
    for index in order {
        if widths.iter().sum::<f32>() + gap <= width {
            break;
        }
        if widths[index] > collapsed_widths[index] {
            widths[index] = collapsed_widths[index];
            collapsed[index] = true;
        }
    }
    if widths.iter().sum::<f32>() + gap > width {
        return vec![RibbonPresentation::Overflow(
            groups.iter().map(|group| group.id.clone()).collect(),
        )];
    }
    groups
        .iter()
        .enumerate()
        .map(|(index, group)| {
            if collapsed[index] {
                RibbonPresentation::Collapsed(group.id.clone())
            } else {
                RibbonPresentation::Full(group.id.clone())
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "ribbon_command_tests.rs"]
mod command_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn groups() -> Vec<RibbonGroup> {
        (0..3)
            .map(|i| RibbonGroup {
                id: format!("g{i}"),
                label: format!("Group {i}"),
                collapse_priority: i,
                items: vec![
                    RibbonItem::new("a", RibbonItemKind::Large, format!("{i}A")),
                    RibbonItem::new("b", RibbonItemKind::Small, format!("{i}B")),
                    RibbonItem::new("c", RibbonItemKind::Small, format!("{i}C")),
                ],
            })
            .collect()
    }
    #[test]
    fn three_widths_preserve_all_group_access_without_overlap() {
        let groups = groups();
        assert!(
            layout_groups(&groups, 900.0)
                .iter()
                .all(|p| matches!(p, RibbonPresentation::Full(_)))
        );
        let medium = layout_groups(&groups, 450.0);
        assert!(
            medium
                .iter()
                .any(|p| matches!(p, RibbonPresentation::Collapsed(_)))
        );
        assert_eq!(medium.len(), 3);
        assert_eq!(
            layout_groups(&groups, 160.0),
            vec![RibbonPresentation::Overflow(vec![
                "g0".into(),
                "g1".into(),
                "g2".into()
            ])]
        );
    }
    #[test]
    fn contextual_tabs_and_quick_access_are_model_owned() {
        let mut ribbon = Ribbon::new(RibbonModel {
            tabs: vec![
                RibbonTab {
                    id: "home".into(),
                    label: "Home".into(),
                    key_tip: "H".into(),
                    context: None,
                    groups: groups(),
                },
                RibbonTab {
                    id: "data".into(),
                    label: "Data".into(),
                    key_tip: "D".into(),
                    context: Some("selection".into()),
                    groups: vec![],
                },
            ],
            quick_access: vec![],
        })
        .unwrap();
        assert!(!ribbon.set_active_tab("data"));
        ribbon.set_context(Some("selection"));
        assert!(ribbon.set_active_tab("data"));
        ribbon.set_context(None);
        assert_eq!(ribbon.active_tab(), "home");
        assert!(ribbon.add_quick_access("save".into()));
        assert!(!ribbon.add_quick_access("save".into()));
        assert!(ribbon.remove_quick_access(&"save".into()));
    }
}
