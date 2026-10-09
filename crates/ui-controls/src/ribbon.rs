//! Command-oriented Ribbon with adaptive group collapse and keyboard KeyTips.
use crate::{CommandId, ControlError, Controls, MenuItem, Role, Theme};
use rust_desktop_ui_core::{
    Axis, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Mutation, WidgetId,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonItemKind {
    Large,
    Small,
    Toggle,
    Split { menu: Vec<CommandId> },
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonPresentation {
    Full(String),
    Collapsed(String),
    Overflow(Vec<String>),
}
#[derive(Clone, Debug)]
enum Action {
    Tab(String),
    Menu(Vec<MenuItem>),
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
    dropdown: f32,
    qat: f32,
    tab: f32,
    groups: f32,
}

impl RibbonMetrics {
    fn measure(ui: &mut Controls, model: &RibbonModel) -> Result<Self, ControlError> {
        let compact = ui.theme() == Theme::Compact;
        let mut text_height = ui.measure_text("Ag More", 4096.0, false)?.height.ceil();
        let mut caption = 20.0_f32;
        // Measure the whole model so tab/context/overflow switches keep a stable
        // height for this theme. Group caption widths match their full layout.
        let mut labels = Vec::new();
        for tab in &model.tabs {
            labels.push(tab.label.clone());
            for group in &tab.groups {
                caption = caption.max(
                    ui.measure_text(&group.label, (group_width(group) - 24.0).max(0.0), true)?
                        .height
                        .ceil(),
                );
                for item in &group.items {
                    labels.push(
                        ui.commands()
                            .label(&item.command)
                            .unwrap_or(&item.command.0)
                            .to_owned(),
                    );
                }
            }
        }
        for command in &model.quick_access {
            labels.push(
                ui.commands()
                    .label(command)
                    .unwrap_or(&command.0)
                    .to_owned(),
            );
        }
        for label in labels {
            text_height = text_height.max(ui.measure_text(&label, 4096.0, false)?.height.ceil());
        }
        let small = (if compact { 20.0_f32 } else { 26.0 }).max(text_height + 4.0);
        let large = (if compact { 64.0_f32 } else { 84.0 }).max(small * 3.0 + 4.0);
        Ok(Self {
            caption,
            small,
            large,
            dropdown: 24.0_f32.max(text_height + 4.0),
            qat: 28.0_f32.max(text_height + 4.0),
            tab: 30.0_f32.max(text_height + 4.0),
            groups: (if compact { 94.0_f32 } else { 118.0 }).max(large + caption + 10.0),
        })
    }
    fn total(self) -> f32 {
        self.qat + self.tab + self.groups + 20.0 // 12px padding and two 4px gaps
    }
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
    tips: Tips,
    tip_prefix: String,
    dirty: bool,
    width: f32,
    theme: Option<Theme>,
    presentations: Vec<RibbonPresentation>,
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
            tips: Tips::Hidden,
            tip_prefix: String::new(),
            dirty: true,
            width: 0.0,
            theme: None,
            presentations: Vec::new(),
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
    pub fn presentations(&self) -> &[RibbonPresentation] {
        &self.presentations
    }
    pub fn container(&self) -> Option<WidgetId> {
        self.container
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
            self.active = id.into();
            self.dirty = true;
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
        self.parent = Some(parent);
        self.dirty = true;
        self.update(ui, 800.0)?;
        self.container
            .ok_or_else(|| ControlError::Invalid("Ribbon was not mounted".into()))
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
    /// Reflows only on width/theme/model changes. Commands retain their identity and checked state.
    pub fn update(&mut self, ui: &mut Controls, width: f32) -> Result<(), ControlError> {
        if !width.is_finite() || width < 0.0 {
            return Err(ControlError::Invalid("Invalid Ribbon width".into()));
        }
        if !self.dirty && self.width == width && self.theme == Some(ui.theme()) {
            return Ok(());
        }
        let Some(parent) = self.parent else {
            return Ok(());
        };
        let restore_command = ui
            .runtime
            .focused()
            .and_then(|id| self.actions.get(&id))
            .and_then(|a| {
                if let Action::Command(id) = a {
                    Some(id.clone())
                } else {
                    None
                }
            });
        ui.close_popup()?;
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
        self.presentations.clear();
        self.ensure_active();
        self.width = width;
        self.theme = Some(ui.theme());
        let metrics = RibbonMetrics::measure(ui, &self.model)?;
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
            ui.panel(parent, style)?
        };
        self.container = Some(container);
        let available = (width - 12.0).max(0.0);
        let qat = ui.panel(
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
        let capacity = ((available - 44.0).max(0.0) / 108.0).floor() as usize;
        for (index, command) in self.model.quick_access.iter().take(capacity).enumerate() {
            let label = ui
                .commands()
                .label(command)
                .unwrap_or(&command.0)
                .to_owned();
            let caption = if self.tips == Tips::Tabs && index < 9 {
                format!("{} [{}]", label, index + 1)
            } else {
                label.clone()
            };
            let id = ui.button(
                qat,
                caption,
                Some(command.clone()),
                LayoutStyle {
                    width: Length::Px(104.0),
                    height: Length::Px(metrics.qat),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_tooltip(id, label);
            self.actions.insert(id, Action::Command(command.clone()));
        }
        if self.model.quick_access.len() > capacity {
            let id = ui.button(
                qat,
                "More",
                None,
                LayoutStyle {
                    width: Length::Px(available.min(44.0)),
                    height: Length::Px(metrics.qat),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            let items = self
                .model
                .quick_access
                .iter()
                .skip(capacity)
                .map(|command| {
                    MenuItem::new(
                        ui.commands().label(command).unwrap_or(&command.0),
                        command.clone(),
                    )
                })
                .collect();
            self.actions.insert(id, Action::Menu(items));
        }
        let tab_row = ui.panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(metrics.tab),
                flex_shrink: 0.0,
                gap: 4.0,
                ..Default::default()
            },
        )?;
        ui.set_role(tab_row, Role::TabList);
        let visible: Vec<_> = self.visible_tabs().into_iter().cloned().collect();
        let tab_width = ((available - (visible.len().saturating_sub(1) as f32) * 4.0)
            / visible.len().max(1) as f32)
            .clamp(0.0, 150.0);
        for tab in visible {
            let caption = if self.tips == Tips::Tabs {
                format!("{} [{}]", tab.label, tab.key_tip)
            } else {
                tab.label.clone()
            };
            let id = ui.toggle(
                tab_row,
                caption,
                None,
                tab.id == self.active,
                LayoutStyle {
                    width: Length::Px(tab_width),
                    height: Length::Px(metrics.tab),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_role(id, Role::Tab);
            self.actions.insert(id, Action::Tab(tab.id.clone()));
            self.tabs.insert(tab.id, id);
        }
        let groups_row = ui.panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(metrics.groups),
                gap: 6.0,
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        if let Some(tab) = self
            .model
            .tabs
            .iter()
            .find(|tab| tab.id == self.active)
            .cloned()
        {
            self.presentations = layout_groups(&tab.groups, available);
            for presentation in self.presentations.clone() {
                match presentation {
                    RibbonPresentation::Full(id) => {
                        if let Some(group) = tab.groups.iter().find(|g| g.id == id) {
                            self.mount_group(ui, groups_row, group, metrics)?;
                        }
                    }
                    RibbonPresentation::Collapsed(id) => {
                        if let Some(group) = tab.groups.iter().find(|g| g.id == id) {
                            let button = ui.button(
                                groups_row,
                                format!("{}\nCommands", group.label),
                                None,
                                LayoutStyle {
                                    width: Length::Px(COLLAPSED),
                                    height: Length::Percent(1.0),
                                    flex_shrink: 0.0,
                                    ..Default::default()
                                },
                            )?;
                            self.actions
                                .insert(button, Action::Menu(group_menu(ui, group)));
                        }
                    }
                    RibbonPresentation::Overflow(_) => {
                        let button = ui.button(
                            groups_row,
                            "All commands",
                            None,
                            LayoutStyle {
                                width: Length::Px(available),
                                height: Length::Px(48.0),
                                flex_shrink: 0.0,
                                ..Default::default()
                            },
                        )?;
                        let items = tab
                            .groups
                            .iter()
                            .flat_map(|group| group_menu(ui, group))
                            .collect();
                        self.actions.insert(button, Action::Menu(items));
                    }
                }
            }
        }
        self.dirty = false;
        // Focus can be restored before layout because eligibility is a logical tree property.
        if let Some(command) = restore_command
            && let Some((&id, _)) = self
                .actions
                .iter()
                .find(|(_, action)| matches!(action,Action::Command(id) if *id==command))
        {
            let _ = ui.runtime.apply(Mutation::Focus(Some(id)));
        }
        Ok(())
    }
    fn mount_group(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        group: &RibbonGroup,
        metrics: RibbonMetrics,
    ) -> Result<(), ControlError> {
        let panel = ui.panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                width: Length::Px(group_width(group)),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                padding: Edges::all(4.0),
                gap: 2.0,
                ..Default::default()
            },
        )?;
        ui.set_role(panel, Role::Group);
        let row = ui.panel(
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
        for item in &group.items {
            let label = ui
                .commands()
                .label(&item.command)
                .unwrap_or(&item.command.0)
                .to_owned();
            let caption = if self.tips == Tips::Commands {
                format!("{} [{}]", label, item.key_tip)
            } else {
                label.clone()
            };
            let (host, height, width) = if matches!(item.kind, RibbonItemKind::Small) {
                if small_count % 3 == 0 {
                    small_column = Some(ui.panel(
                        row,
                        LayoutStyle {
                            kind: LayoutKind::Stack(Axis::Vertical),
                            width: Length::Px(128.0),
                            height: Length::Percent(1.0),
                            gap: 2.0,
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?);
                }
                small_count += 1;
                (small_column.unwrap_or(row), metrics.small, 128.0)
            } else {
                small_count = 0;
                small_column = None;
                (row, metrics.large, 104.0)
            };
            let style = LayoutStyle {
                width: Length::Px(width),
                height: Length::Px(height),
                flex_shrink: 0.0,
                ..Default::default()
            };
            let id = match &item.kind {
                RibbonItemKind::Toggle => ui.toggle(
                    host,
                    caption,
                    Some(item.command.clone()),
                    ui.commands().checked(&item.command).unwrap_or(false),
                    style,
                )?,
                RibbonItemKind::Split { menu } => {
                    let split = ui.panel(
                        host,
                        LayoutStyle {
                            kind: LayoutKind::Stack(Axis::Vertical),
                            width: Length::Px(width),
                            height: Length::Px(height),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?;
                    let primary = ui.button(
                        split,
                        caption,
                        Some(item.command.clone()),
                        LayoutStyle {
                            height: Length::Px(height - metrics.dropdown),
                            ..Default::default()
                        },
                    )?;
                    let dropdown = ui.button(
                        split,
                        "More",
                        None,
                        LayoutStyle {
                            height: Length::Px(metrics.dropdown),
                            ..Default::default()
                        },
                    )?;
                    let entries = menu
                        .iter()
                        .map(|id| {
                            MenuItem::new(ui.commands().label(id).unwrap_or(&id.0), id.clone())
                        })
                        .collect();
                    self.actions.insert(dropdown, Action::Menu(entries));
                    primary
                }
                _ => ui.button(host, caption, Some(item.command.clone()), style)?,
            };
            ui.set_tooltip(id, label);
            self.actions
                .insert(id, Action::Command(item.command.clone()));
        }
        ui.label(
            panel,
            group.label.clone(),
            LayoutStyle {
                height: Length::Px(metrics.caption),
                ..Default::default()
            },
        )?;
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
                Some(Action::Menu(items)) => {
                    ui.popup_menu(id, &items)?;
                }
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
        let InputEvent::KeyDown {
            key,
            repeat,
            modifiers,
        } = event
        else {
            return Ok(false);
        };
        if *repeat {
            return Ok(self.tips != Tips::Hidden);
        }
        if matches!(key, Key::Alt | Key::F10) && !modifiers.control && !modifiers.super_key {
            self.tips = if self.tips == Tips::Hidden {
                Tips::Tabs
            } else {
                Tips::Hidden
            };
            self.tip_prefix.clear();
            self.dirty = true;
            if let Some(&id) = self.tabs.get(&self.active) {
                ui.runtime.apply(Mutation::Focus(Some(id)))?;
            }
            return Ok(true);
        }
        if self.tips == Tips::Hidden {
            return Ok(false);
        }
        if *key == Key::Escape {
            self.tips = if self.tips == Tips::Commands {
                Tips::Tabs
            } else {
                Tips::Hidden
            };
            self.tip_prefix.clear();
            self.dirty = true;
            ui.close_popup()?;
            return Ok(true);
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
                ui.commands_mut().execute(command);
                self.tips = Tips::Hidden;
                self.tip_prefix.clear();
                self.dirty = true;
                return Ok(true);
            }
            let visible = self.visible_tabs();
            if let Some(tab) = visible
                .iter()
                .find(|tab| tab.key_tip.eq_ignore_ascii_case(&self.tip_prefix))
            {
                self.active = tab.id.clone();
                self.tips = Tips::Commands;
                self.tip_prefix.clear();
                self.dirty = true;
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
                if matches!(item.kind, RibbonItemKind::Toggle)
                    && ui.commands().enabled(&item.command)
                {
                    let checked = !ui.commands().checked(&item.command).unwrap_or(false);
                    ui.commands_mut().set_checked(&item.command, Some(checked));
                }
                ui.commands_mut().execute(&item.command);
                self.tips = Tips::Hidden;
                self.tip_prefix.clear();
                self.dirty = true;
            } else if !items
                .iter()
                .any(|item| item.key_tip.to_uppercase().starts_with(&self.tip_prefix))
            {
                self.tip_prefix.clear();
            }
        }
        Ok(true)
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
    if groups.is_empty() {
        return Vec::new();
    }
    let gap = 6.0 * (groups.len() - 1) as f32;
    let mut widths: Vec<_> = groups.iter().map(group_width).collect();
    let mut collapsed = vec![false; groups.len()];
    let mut order: Vec<_> = (0..groups.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(groups[index].collapse_priority));
    for index in order {
        if widths.iter().sum::<f32>() + gap <= width {
            break;
        }
        if widths[index] > COLLAPSED {
            widths[index] = COLLAPSED;
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
            let mut entry = MenuItem::new(
                format!(
                    "{} / {}",
                    group.label,
                    ui.commands()
                        .label(&item.command)
                        .unwrap_or(&item.command.0)
                ),
                item.command.clone(),
            );
            if matches!(item.kind, RibbonItemKind::Toggle) {
                entry = entry.toggling();
            }
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
