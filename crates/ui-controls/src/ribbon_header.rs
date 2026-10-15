//! Optional platform-independent title row. Window hosts own native chrome.
use super::*;
use crate::TextStyle;
use rust_desktop_ui_core::{Align, Rect};
use rust_desktop_ui_text::TextDocument;

/// Integrates the title and QAT in one row above Ribbon tabs. Insets reserve
/// host-owned content, such as native caption buttons, from the actual edges
/// of the Ribbon. All dimensions are logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonHeader {
    pub title: String,
    /// Minimum row height; measured text and QAT may require a taller row.
    pub height: f32,
    pub leading_inset: f32,
    pub trailing_inset: f32,
}

impl RibbonHeader {
    fn validate(&self) -> Result<(), ControlError> {
        if !self.height.is_finite()
            || self.height <= 0.0
            || self.height > 512.0
            || [self.leading_inset, self.trailing_inset]
                .into_iter()
                .any(|value| !value.is_finite() || !(0.0..=16_384.0).contains(&value))
            || self.title.len() > 4096
        {
            return Err(ControlError::Invalid(
                "Ribbon header requires a height in (0, 512], insets in [0, 16384], and a title of at most 4096 UTF-8 bytes".into(),
            ));
        }
        Ok(())
    }
}

/// Current layout in window-client logical coordinates. The host can treat
/// the header as draggable after excluding `interactive` and its own native
/// caption-button and resize regions. No native window APIs are called here.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonHeaderRegions {
    pub bounds: Rect,
    /// QAT including its overflow button and gaps; never the title itself.
    pub interactive: Vec<Rect>,
}

pub(super) struct HeaderMeasurements {
    width: f32,
    pub height: f32,
    leading: f32,
    trailing: f32,
    pub qat_available: f32,
    qat_width: f32,
    gap: f32,
    title_width: f32,
    title_height: f32,
    caption: String,
}

fn title_style() -> TextStyle {
    TextStyle {
        font_size: Some(14.0),
        wrap: false,
        ..Default::default()
    }
}

impl HeaderMeasurements {
    pub fn measure(
        ui: &mut Controls,
        header: &RibbonHeader,
        width: f32,
        widths: &RibbonWidths,
        qat_height: f32,
    ) -> Result<Self, ControlError> {
        header.validate()?;
        // Native caption controls retain their reserved area at narrow widths.
        let trailing = header.trailing_inset.min(width);
        let leading = header.leading_inset.min(width - trailing);
        let available = (width - leading - trailing).max(0.0);
        let title_height = ui
            .measure_styled_text("Ag", 4096.0, title_style())?
            .height
            .ceil();
        // Keep a useful title when possible, but retain QAT/overflow access as
        // the window narrows. KeyTips can still activate hidden QAT commands.
        let qat_available = if widths.qat.is_empty() {
            0.0
        } else if available >= QAT_TARGET + 12.0 + 96.0 {
            available - 12.0 - 96.0
        } else {
            available
        };
        let capacity = widths.qat_capacity(qat_available);
        let has_overflow = capacity < widths.qat.len();
        let count = capacity + usize::from(has_overflow);
        let qat_width = (widths.qat.iter().take(capacity).sum::<f32>()
            + if has_overflow { QAT_TARGET } else { 0.0 }
            + 4.0 * count.saturating_sub(1) as f32)
            .min(qat_available);
        let gap = if qat_width > 0.0 {
            12.0_f32.min((available - qat_width).max(0.0))
        } else {
            0.0
        };
        let title_width = (available - qat_width - gap).max(0.0);
        let caption = fit_title(ui, &header.title, (title_width - 16.0).max(0.0))?;
        Ok(Self {
            width,
            height: header.height.max(qat_height).max(title_height + 12.0),
            leading,
            trailing,
            qat_available,
            qat_width,
            gap,
            title_width,
            title_height,
            caption,
        })
    }
}

fn fit_title(ui: &mut Controls, title: &str, width: f32) -> Result<String, ControlError> {
    let width = width.min(16_384.0);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if ui.measure_styled_text(&title, width, title_style())?.width <= width {
        return Ok(title);
    }
    if ui.measure_styled_text("…", width, title_style())?.width > width {
        return Ok(String::new());
    }
    let boundaries = TextDocument::new(title.clone()).grapheme_boundaries();
    let mut low = 0;
    let mut high = boundaries.len().saturating_sub(1);
    let mut best = "…".to_owned();
    while low <= high {
        let middle = low + (high - low) / 2;
        let candidate = format!("{}…", title[..boundaries[middle]].trim_end());
        if ui
            .measure_styled_text(&candidate, width, title_style())?
            .width
            <= width
        {
            best = candidate;
            low = middle + 1;
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    Ok(best)
}

impl Ribbon {
    /// Changes the optional title row. Invalid settings leave the previous
    /// settings and mounted tree intact; `update` applies valid changes.
    pub fn set_header(&mut self, header: Option<RibbonHeader>) -> Result<(), ControlError> {
        if let Some(header) = &header {
            header.validate()?;
        }
        if self.header != header {
            self.header = header;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn header(&self) -> Option<&RibbonHeader> {
        self.header.as_ref()
    }

    /// Read after Ribbon and Controls layout updates. Empty or unmounted
    /// headers have no regions. Interactive bounds include disabled commands
    /// so their hit area cannot accidentally start a native window drag.
    pub fn header_regions(&self, ui: &Controls) -> Option<RibbonHeaderRegions> {
        self.header.as_ref()?;
        let bounds = ui.runtime.bounds(self.header_row?)?;
        let interactive = self
            .header_qat
            .and_then(|id| ui.runtime.bounds(id))
            .and_then(|qat| qat.intersection(bounds))
            .into_iter()
            .collect();
        Some(RibbonHeaderRegions {
            bounds,
            interactive,
        })
    }

    pub(super) fn mount_header(
        &mut self,
        ui: &mut Controls,
        container: WidgetId,
        measured: &HeaderMeasurements,
        widths: &RibbonWidths,
        qat_height: f32,
    ) -> Result<(), ControlError> {
        let row = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Px(measured.width),
                height: Length::Px(measured.height),
                flex_shrink: 0.0,
                align: Align::Center,
                gap: measured.gap,
                margin: Edges {
                    left: -6.0,
                    right: -6.0,
                    ..Default::default()
                },
                padding: Edges {
                    left: measured.leading,
                    right: measured.trailing,
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        self.header_row = Some(row);
        if !self.model.quick_access.is_empty() {
            self.header_qat = Some(self.mount_quick_access(
                ui,
                row,
                measured.qat_width,
                measured.qat_available,
                qat_height,
                widths,
            )?);
        }
        let title = ui.label(
            row,
            &measured.caption,
            LayoutStyle {
                width: Length::Px(measured.title_width),
                height: Length::Px(measured.title_height),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        ui.set_text_style(title, title_style())?;
        ui.set_automation_name(title, &self.header.as_ref().unwrap().title)?;
        Ok(())
    }

    pub(super) fn mount_quick_access(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        width: f32,
        available: f32,
        height: f32,
        widths: &RibbonWidths,
    ) -> Result<WidgetId, ControlError> {
        let qat = ui.transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Px(width),
                height: Length::Px(height),
                flex_shrink: 0.0,
                gap: 4.0,
                ..Default::default()
            },
        )?;
        ui.set_role(qat, Role::Group);
        let capacity = widths.qat_capacity(available);
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
                    width: Length::Px(widths.qat[index]),
                    height: Length::Px(height),
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
                    height: Length::Px(height),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            ui.set_automation_name(id, "More")?;
            ui.set_button_style(id, ribbon_button_style())?;
            ui.set_button_chrome(id, ButtonChrome::Flat)?;
            self.actions.insert(id, Action::Dropdown);
        }
        Ok(qat)
    }
}
