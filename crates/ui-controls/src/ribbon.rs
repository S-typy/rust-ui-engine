//! Command-oriented Ribbon with adaptive group collapse and keyboard KeyTips.
use crate::{
    ButtonChrome, ButtonContentLayout, ButtonIcon, ButtonStyle, CommandId, ControlError, Controls,
    MenuEntry, MenuItem, Role, SeparatorSettings, SplitButton, Theme,
};
use rust_desktop_ui_core::{
    Axis, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Mutation, WidgetId,
};
use std::collections::HashMap;

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
                for (index, item) in group.items.iter().enumerate() {
                    let layout = if matches!(item.kind, RibbonItemKind::Small) {
                        ButtonContentLayout::Leading
                    } else {
                        ButtonContentLayout::Above
                    };
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
                        .to_owned();
                    let width = widths.group(&tab.id, &group.id).items[index];
                    let size = if matches!(item.kind, RibbonItemKind::Split { .. }) {
                        ui.measure_split_button_content(
                            &label,
                            &ribbon_button_style(),
                            icon,
                            layout,
                            width,
                        )?
                    } else {
                        ui.measure_button_content(
                            &label,
                            &ribbon_button_style(),
                            icon,
                            layout,
                            width,
                        )?
                    };
                    if layout == ButtonContentLayout::Leading {
                        small = small.max(size.height.ceil());
                    } else {
                        large = large.max(size.height.ceil());
                    }
                }
            }
        }
        for (index, command) in model.quick_access.iter().enumerate() {
            let label = ui
                .commands()
                .label(command)
                .unwrap_or(&command.0)
                .to_owned();
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
        })
    }
    fn total(self) -> f32 {
        self.qat + self.tab + self.groups + 20.0 // 12px padding and two 4px gaps
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

struct MeasuredGroup {
    width: f32,
    collapsed: f32,
    items: Vec<f32>,
}

/// One measured profile feeds layout planning, popup preflight and construction.
/// Content arrangement, icon metrics and style match the actual controls.
/// KeyTips are overlays and never participate in these measurements.
struct RibbonWidths {
    qat: Vec<f32>,
    tabs: HashMap<String, f32>,
    groups: HashMap<(String, String), MeasuredGroup>,
}

impl RibbonWidths {
    fn measure(
        ui: &mut Controls,
        model: &RibbonModel,
        appearances: &HashMap<CommandId, RibbonCommandAppearance>,
        _tips: Tips,
    ) -> Result<Self, ControlError> {
        let mut measure_command = |command: &CommandId, layout, split: bool| {
            let label = ui
                .commands()
                .label(command)
                .unwrap_or(&command.0)
                .to_owned();
            let icon = presentation_icon(
                appearances
                    .get(command)
                    .and_then(|appearance| appearance.icon),
                layout,
            );
            if layout == ButtonContentLayout::Above {
                let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
                // Choose a compact width that fits two complete caption lines.
                // Extremely long text is visually ellipsized by shared paint;
                // the complete label remains semantic and in the tooltip.
                let line = ui.measure_text("Ag", 4096.0, false)?.height;
                let arrow = if split && icon.is_some() { 20.0 } else { 0.0 };
                let mut width = 72.0_f32;
                while width < 160.0 {
                    let caption_width = width - 8.0 - arrow;
                    let text = ui.measure_text(&label, caption_width, true)?;
                    if text.height <= line * 2.0 + 0.001 && text.width <= caption_width + 0.001 {
                        break;
                    }
                    width += 4.0;
                }
                let measured =
                    ui.measure_button_content(&label, &ribbon_button_style(), icon, layout, width)?;
                Ok::<f32, ControlError>(measured.width.ceil())
            } else {
                let measured = ui.measure_button_content(
                    &label,
                    &ribbon_button_style(),
                    icon,
                    layout,
                    MAX_COMMAND_WIDTH,
                )?;
                // One pixel protects fractional shaping/global clip subtraction.
                Ok((measured.width.ceil()
                    + if icon.is_some() && layout == ButtonContentLayout::IconOnly {
                        0.0
                    } else {
                        1.0
                    })
                .clamp(
                    if layout == ButtonContentLayout::IconOnly {
                        QAT_TARGET
                    } else {
                        56.0
                    },
                    MAX_COMMAND_WIDTH,
                ))
            }
        };
        let mut qat = Vec::new();
        for command in &model.quick_access {
            qat.push(measure_command(
                command,
                ButtonContentLayout::IconOnly,
                false,
            )?);
        }
        let mut groups = HashMap::new();
        for tab in &model.tabs {
            for group in &tab.groups {
                let mut items = Vec::new();
                for item in &group.items {
                    items.push(measure_command(
                        &item.command,
                        if matches!(item.kind, RibbonItemKind::Small) {
                            ButtonContentLayout::Leading
                        } else {
                            ButtonContentLayout::Above
                        },
                        matches!(item.kind, RibbonItemKind::Split { .. }),
                    )?);
                }
                let mut width = 8.0;
                let mut index = 0;
                while index < group.items.len() {
                    if matches!(group.items[index].kind, RibbonItemKind::Small) {
                        let count = group.items[index..]
                            .iter()
                            .take(3)
                            .take_while(|item| matches!(item.kind, RibbonItemKind::Small))
                            .count();
                        let column = items[index..index + count]
                            .iter()
                            .copied()
                            .fold(0.0_f32, f32::max);
                        items[index..index + count].fill(column);
                        width += column + 4.0;
                        index += count;
                    } else {
                        width += items[index] + 4.0;
                        index += 1;
                    }
                }
                groups.insert(
                    (tab.id.clone(), group.id.clone()),
                    MeasuredGroup {
                        width,
                        collapsed: width,
                        items,
                    },
                );
            }
        }
        for tab in &model.tabs {
            for group in &tab.groups {
                let text = ui.measure_text(&group.label, 4096.0, false)?;
                // The shared dropdown reserves 20px for its arrow, in addition
                // to 8px button padding on each side. A collapse never expands
                // a group: an already narrower full group stays full instead.
                let measured = groups.get_mut(&(tab.id.clone(), group.id.clone())).unwrap();
                measured.collapsed = (text.width.ceil() + 29.0)
                    .clamp(COLLAPSED, MAX_COMMAND_WIDTH)
                    .min(measured.width);
            }
        }
        let mut tabs = HashMap::new();
        for tab in &model.tabs {
            let text = ui.measure_text(&tab.label, 4096.0, false)?;
            tabs.insert(
                tab.id.clone(),
                (text.width.ceil() + 28.0).clamp(48.0, 240.0),
            );
        }
        Ok(Self { qat, tabs, groups })
    }
    fn group(&self, tab: &str, group: &str) -> &MeasuredGroup {
        &self.groups[&(tab.to_owned(), group.to_owned())]
    }
    fn qat_capacity(&self, available: f32) -> usize {
        let total = self.qat.iter().sum::<f32>() + 4.0 * self.qat.len().saturating_sub(1) as f32;
        if total <= available {
            return self.qat.len();
        }
        let mut used = QAT_TARGET; // Menu button plus a gap after each visible command.
        self.qat
            .iter()
            .take_while(|&&width| {
                used += width + 4.0;
                used <= available
            })
            .count()
    }
    fn layout_groups(&self, tab: &RibbonTab, available: f32) -> Vec<RibbonPresentation> {
        plan_groups(
            &tab.groups,
            (available - SURFACE_PADDING * 2.0).max(0.0),
            tab.groups
                .iter()
                .map(|group| self.group(&tab.id, &group.id).width)
                .collect(),
            tab.groups
                .iter()
                .map(|group| self.group(&tab.id, &group.id).collapsed)
                .collect(),
        )
    }
}

struct RibbonMeasurements {
    metrics: RibbonMetrics,
    widths: RibbonWidths,
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
}

impl Ribbon {
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
        Ok(self.preflight_metrics(ui, width)?.metrics.total())
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
                if matches!(item.kind, RibbonItemKind::Split { .. }) {
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
        let widths = RibbonWidths::measure(ui, &self.model, &self.command_appearances, self.tips)?;
        self.validate_popup_counts(width, &widths)?;
        let available = (width - 12.0).max(0.0);
        let capacity = widths.qat_capacity(available);
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
                    RibbonPresentation::Full(id) => {
                        if let Some(group) = tab.groups.iter().find(|group| group.id == id) {
                            for item in &group.items {
                                if let RibbonItemKind::Split { menu } = &item.kind {
                                    let entries: Vec<_> = menu
                                        .iter()
                                        .map(|command| {
                                            MenuEntry::Item(MenuItem::new(
                                                ui.commands().label(command).unwrap_or(&command.0),
                                                command.clone(),
                                            ))
                                        })
                                        .collect();
                                    if !entries.is_empty() {
                                        ui.preflight_menu(&entries)?;
                                    }
                                }
                            }
                        }
                        Vec::new()
                    }
                    RibbonPresentation::Collapsed(id) => tab
                        .groups
                        .iter()
                        .find(|group| group.id == id)
                        .map(|group| group_menu(ui, group))
                        .unwrap_or_default()
                        .into_iter()
                        .map(MenuEntry::Item)
                        .collect(),
                    RibbonPresentation::Overflow(_) => tab
                        .groups
                        .iter()
                        .flat_map(|group| group_menu(ui, group))
                        .map(MenuEntry::Item)
                        .collect(),
                };
                if !entries.is_empty() {
                    ui.preflight_menu(&entries)?;
                }
            }
        }
        Ok(RibbonMeasurements {
            metrics: RibbonMetrics::measure(ui, &self.model, &widths, &self.command_appearances)?,
            widths,
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
    fn validate_popup_counts(&self, width: f32, widths: &RibbonWidths) -> Result<(), ControlError> {
        let check = |count: usize, name: &str| {
            if count > crate::MAX_MENU_ITEMS {
                Err(ControlError::Invalid(format!(
                    "Ribbon {name} would contain {count} menu entries; the maximum is {}. Reduce menu commands or use a wider Ribbon.",
                    crate::MAX_MENU_ITEMS,
                )))
            } else {
                Ok(())
            }
        };
        let available = (width - 12.0).max(0.0);
        let capacity = widths.qat_capacity(available);
        check(
            self.model.quick_access.len().saturating_sub(capacity),
            "QAT overflow",
        )?;
        let visible = self.visible_tabs();
        let tab = visible
            .iter()
            .find(|tab| tab.id == self.active)
            .or(visible.first());
        if let Some(tab) = tab {
            let group_count = |group: &RibbonGroup| {
                group.items.iter().fold(0_usize, |count, item| {
                    count.saturating_add(1).saturating_add(match &item.kind {
                        RibbonItemKind::Split { menu } => menu.len(),
                        _ => 0,
                    })
                })
            };
            for presentation in widths.layout_groups(tab, available) {
                match presentation {
                    RibbonPresentation::Full(id) => {
                        if let Some(group) = tab.groups.iter().find(|group| group.id == id) {
                            for item in &group.items {
                                if let RibbonItemKind::Split { menu } = &item.kind {
                                    check(menu.len(), "Split secondary menu")?;
                                }
                            }
                        }
                    }
                    RibbonPresentation::Collapsed(id) => {
                        if let Some(group) = tab.groups.iter().find(|group| group.id == id) {
                            check(group_count(group), "collapsed group")?;
                        }
                    }
                    RibbonPresentation::Overflow(_) => {
                        check(
                            tab.groups.iter().fold(0_usize, |count, group| {
                                count.saturating_add(group_count(group))
                            }),
                            "All commands",
                        )?;
                    }
                }
            }
        }
        Ok(())
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
        self.ensure_active();
        self.width = width;
        self.theme = Some(ui.theme());
        let style = LayoutStyle {
            kind: LayoutKind::Stack(Axis::Vertical),
            height: Length::Px(metrics.total()),
            flex_shrink: 0.0,
            gap: 4.0,
            padding: Edges::all(6.0),
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
        let qat = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(metrics.qat),
                flex_shrink: 0.0,
                gap: 4.0,
                ..Default::default()
            },
        )?;
        ui.set_role(qat, Role::Group);
        let capacity = measured.widths.qat_capacity(available);
        for (index, command) in self.model.quick_access.iter().take(capacity).enumerate() {
            let label = ui
                .commands()
                .label(command)
                .unwrap_or(&command.0)
                .to_owned();
            let id = ui.button(
                qat,
                label.clone(),
                Some(command.clone()),
                LayoutStyle {
                    width: Length::Px(measured.widths.qat[index]),
                    height: Length::Px(metrics.qat),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            self.apply_command_appearance(ui, id, command, &label, ButtonContentLayout::IconOnly)?;
            if index < 9 {
                self.tip_nodes
                    .insert(id, (Tips::Tabs, (index + 1).to_string()));
            }
            self.actions.insert(id, Action::Command(command.clone()));
        }
        if self.model.quick_access.len() > capacity {
            let items: Vec<_> = self
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
            let id = ui.dropdown_button(
                qat,
                "",
                &items,
                LayoutStyle {
                    width: Length::Px(available.min(QAT_TARGET)),
                    height: Length::Px(metrics.qat),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_automation_name(id, "More")?;
            ui.set_button_style(id, ribbon_button_style())?;
            ui.set_button_chrome(id, ButtonChrome::Flat)?;
            self.actions.insert(id, Action::Dropdown);
        }
        let visible: Vec<_> = self.visible_tabs().into_iter().cloned().collect();
        let tab_gaps = visible.len().saturating_sub(1) as f32;
        let tab_gap = if tab_gaps > 0.0 {
            4.0_f32.min(available / tab_gaps)
        } else {
            0.0
        };
        let natural_tabs: f32 = visible
            .iter()
            .map(|tab| measured.widths.tabs[&tab.id])
            .sum();
        let tab_scale = if natural_tabs > 0.0 {
            ((available - tab_gap * tab_gaps) / natural_tabs).clamp(0.0, 1.0)
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
                            let entries: Vec<_> = group_menu(ui, group)
                                .into_iter()
                                .map(MenuEntry::Item)
                                .collect();
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
                        }
                    }
                    RibbonPresentation::Overflow(_) => {
                        let items: Vec<_> = tab
                            .groups
                            .iter()
                            .flat_map(|group| group_menu(ui, group))
                            .map(MenuEntry::Item)
                            .collect();
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
    fn apply_command_appearance(
        &self,
        ui: &mut Controls,
        id: WidgetId,
        command: &CommandId,
        label: &str,
        layout: ButtonContentLayout,
    ) -> Result<(), ControlError> {
        let appearance = self.command_appearances.get(command);
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
            let layout = if matches!(item.kind, RibbonItemKind::Small) {
                ButtonContentLayout::Leading
            } else {
                ButtonContentLayout::Above
            };
            let (host, height, width) = if matches!(item.kind, RibbonItemKind::Small) {
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
                RibbonItemKind::Split { menu } => {
                    let entries: Vec<_> = menu
                        .iter()
                        .map(|id| {
                            MenuEntry::Item(MenuItem::new(
                                ui.commands().label(id).unwrap_or(&id.0),
                                id.clone(),
                            ))
                        })
                        .collect();
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
                    ui.set_split_button_presentation(split, ButtonContentLayout::Above)?;
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
                _ => {
                    let id = ui.button(host, label.clone(), Some(item.command.clone()), style)?;
                    self.apply_command_appearance(ui, id, &item.command, &label, layout)?;
                    id
                }
            };
            self.tip_nodes
                .insert(id, (Tips::Commands, item.key_tip.clone()));
            self.actions
                .insert(id, Action::Command(item.command.clone()));
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
        let mut unhandled = Vec::new();
        for id in ui.drain_activated() {
            match self.actions.get(&id).cloned() {
                Some(Action::Tab(tab)) => {
                    self.active = tab;
                    self.dirty = true;
                }
                // Controls owns dropdown activation and popup lifecycle.
                Some(Action::Dropdown) => {}
                Some(Action::Command(_)) => {}
                None => unhandled.push(id),
            }
        }
        Ok(unhandled)
    }
    /// Call before ordinary input dispatch. Alt/F10 then tab and item KeyTips
    /// executes commands even when their groups are collapsed into overflow.
    pub fn handle_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if matches!(
            event,
            InputEvent::WindowFocused(false)
                | InputEvent::PointerDown { .. }
                | InputEvent::Wheel { .. }
        ) {
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
                ui.commands_mut().activate(&item.command);
                self.finish_key_tips(ui, true);
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
fn group_menu(ui: &Controls, group: &RibbonGroup) -> Vec<MenuItem> {
    group
        .items
        .iter()
        .flat_map(|item| {
            let entry = MenuItem::new(
                format!(
                    "{} / {}",
                    group.label,
                    ui.commands()
                        .label(&item.command)
                        .unwrap_or(&item.command.0)
                ),
                item.command.clone(),
            );
            let mut entries = vec![entry];
            if let RibbonItemKind::Split { menu } = &item.kind {
                entries.extend(menu.iter().map(|id| {
                    MenuItem::new(
                        format!(
                            "{} / {}",
                            group.label,
                            ui.commands().label(id).unwrap_or(&id.0)
                        ),
                        id.clone(),
                    )
                }));
            }
            entries
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
