//! Measured, reversible item reductions precede group collapse.
use super::*;

impl RibbonItemSize {
    pub(super) fn layout(self) -> ButtonContentLayout {
        match self {
            Self::Large => ButtonContentLayout::Above,
            Self::Small => ButtonContentLayout::Leading,
            Self::IconOnly => ButtonContentLayout::IconOnly,
        }
    }
}

#[derive(Clone)]
pub(super) struct MeasuredGroup {
    pub width: f32,
    pub collapsed: f32,
    pub items: Vec<f32>,
    pub sizes: Vec<RibbonItemSize>,
}

pub(super) struct RibbonWidths {
    pub qat: Vec<f32>,
    pub tabs: HashMap<String, f32>,
    groups: HashMap<(String, String), Vec<MeasuredGroup>>,
    selected: HashMap<(String, String), usize>,
}

fn measure_command(
    ui: &mut Controls,
    command: &CommandId,
    layout: ButtonContentLayout,
    split: bool,
    dropdown: bool,
    appearance: Option<&RibbonCommandAppearance>,
) -> Result<f32, ControlError> {
    let label = ui
        .commands()
        .label(command)
        .unwrap_or(&command.0)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let icon = presentation_icon(appearance.and_then(|appearance| appearance.icon), layout);
    if layout == ButtonContentLayout::Above {
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let line = ui.measure_text("Ag", 4096.0, false)?.height;
        let arrow = if dropdown || split && icon.is_some() {
            20.0
        } else {
            0.0
        };
        let mut width = 72.0_f32;
        while width < 160.0 {
            let available = width - 8.0 - arrow;
            let text = ui.measure_text(&label, available, true)?;
            if text.height <= line * 2.0 + 0.001 && text.width <= available + 0.001 {
                break;
            }
            width += 4.0;
        }
        return Ok(width);
    }
    let measured = if split {
        ui.measure_split_button_content(
            &label,
            &ribbon_button_style(),
            icon,
            layout,
            MAX_COMMAND_WIDTH,
        )?
    } else {
        ui.measure_button_content(
            &label,
            &ribbon_button_style(),
            icon,
            layout,
            MAX_COMMAND_WIDTH,
        )?
    };
    Ok((measured.width.ceil()
        + if dropdown { 20.0 } else { 0.0 }
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

impl RibbonWidths {
    pub fn measure(
        ui: &mut Controls,
        model: &RibbonModel,
        appearances: &HashMap<CommandId, RibbonCommandAppearance>,
        limits: &HashMap<(String, String), SizeLimits>,
    ) -> Result<Self, ControlError> {
        let mut qat = Vec::new();
        for command in &model.quick_access {
            qat.push(measure_command(
                ui,
                command,
                ButtonContentLayout::IconOnly,
                false,
                false,
                appearances.get(command),
            )?);
        }
        let mut groups = HashMap::new();
        for tab in &model.tabs {
            for group in &tab.groups {
                let collapsed = (ui.measure_text(&group.label, 4096.0, false)?.width.ceil() + 29.0)
                    .clamp(COLLAPSED, MAX_COMMAND_WIDTH);
                let mut profiles = Vec::new();
                for tier in [
                    RibbonItemSize::Large,
                    RibbonItemSize::Small,
                    RibbonItemSize::IconOnly,
                ] {
                    let mut items = Vec::new();
                    let mut sizes = Vec::new();
                    for item in &group.items {
                        let preferred = if matches!(item.kind, RibbonItemKind::Small) {
                            RibbonItemSize::Small
                        } else {
                            RibbonItemSize::Large
                        };
                        let limits = limits
                            .get(&(tab.id.clone(), item.key_tip.clone()))
                            .copied()
                            .unwrap_or(SizeLimits {
                                preferred,
                                minimum: RibbonItemSize::IconOnly,
                            });
                        let mut size = limits.preferred.min(tier).max(limits.minimum);
                        if size == RibbonItemSize::IconOnly
                            && appearances
                                .get(&item.command)
                                .and_then(|appearance| appearance.icon)
                                .is_none()
                        {
                            size = RibbonItemSize::Small;
                        }
                        items.push(measure_command(
                            ui,
                            &item.command,
                            size.layout(),
                            item.kind.is_split(),
                            matches!(item.kind, RibbonItemKind::DropDown { .. }),
                            appearances.get(&item.command),
                        )?);
                        sizes.push(size);
                    }
                    let mut width = 8.0;
                    let mut index = 0;
                    while index < items.len() {
                        if sizes[index] != RibbonItemSize::Large {
                            let count = sizes[index..]
                                .iter()
                                .take(3)
                                .take_while(|size| **size != RibbonItemSize::Large)
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
                    profiles.push(MeasuredGroup {
                        width,
                        collapsed: collapsed.min(width),
                        items,
                        sizes,
                    });
                }
                groups.insert((tab.id.clone(), group.id.clone()), profiles);
            }
        }
        let mut tabs = HashMap::new();
        for tab in &model.tabs {
            let text = ui.measure_styled_text(
                &tab.label,
                4096.0,
                crate::TextStyle {
                    font_weight: Some(600),
                    wrap: false,
                    ..Default::default()
                },
            )?;
            tabs.insert(
                tab.id.clone(),
                (text.width.ceil() + 28.0).clamp(48.0, 240.0),
            );
        }
        Ok(Self {
            qat,
            tabs,
            groups,
            selected: HashMap::new(),
        })
    }
    pub fn profiles(&self, tab: &str, group: &str) -> &[MeasuredGroup] {
        &self.groups[&(tab.to_owned(), group.to_owned())]
    }
    pub fn group(&self, tab: &str, group: &str) -> &MeasuredGroup {
        let key = (tab.to_owned(), group.to_owned());
        &self.groups[&key][self.selected.get(&key).copied().unwrap_or(0)]
    }
    pub fn qat_capacity(&self, available: f32) -> usize {
        let total = self.qat.iter().sum::<f32>() + 4.0 * self.qat.len().saturating_sub(1) as f32;
        if total <= available {
            return self.qat.len();
        }
        let mut used = QAT_TARGET;
        self.qat
            .iter()
            .take_while(|&&width| {
                used += width + 4.0;
                used <= available
            })
            .count()
    }
    pub fn adapt(&mut self, tab: &RibbonTab, available: f32) {
        let available = (available - SURFACE_PADDING * 2.0).max(0.0);
        let gap = 6.0 * tab.groups.len().saturating_sub(1) as f32;
        let mut widths: Vec<_> = tab
            .groups
            .iter()
            .map(|group| self.group(&tab.id, &group.id).width)
            .collect();
        let mut order: Vec<_> = (0..tab.groups.len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(tab.groups[index].collapse_priority));
        for tier in [1, 2] {
            for &index in &order {
                if widths.iter().sum::<f32>() + gap <= available {
                    return;
                }
                let key = (tab.id.clone(), tab.groups[index].id.clone());
                let candidate = self.groups[&key][tier].width;
                // Do not adopt a smaller named size that actually uses more width.
                if candidate < widths[index] {
                    widths[index] = candidate;
                    self.selected.insert(key, tier);
                }
            }
        }
    }
    pub fn layout_groups(&self, tab: &RibbonTab, available: f32) -> Vec<RibbonPresentation> {
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
