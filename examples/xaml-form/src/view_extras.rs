//! Adapters for retained layout, range controls and command surfaces.
use super::*;
use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::{GridTrack, Mutation, Size};
use rust_desktop_ui_xaml as x;

pub(super) struct PendingPopup {
    flyout: Flyout,
    placeholder: WidgetId,
    target: String,
    open: bool,
}

impl MountedView {
    pub(super) fn mount_extra(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        mut style: LayoutStyle,
        width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        match &element.kind {
            ElementKind::Overlay(panel) => {
                style.kind = LayoutKind::Overlay;
                style.padding = edges(panel.padding);
                let id = ui.transparent_panel(parent, style).map_err(err)?;
                if panel.surface == x::SurfaceKindDefinition::Card {
                    ui.set_surface(id, SurfaceKind::Card).map_err(err)?;
                }
                for child in &panel.children {
                    self.mount_element(
                        ui,
                        id,
                        child,
                        Axis::Vertical,
                        (width - panel.padding[0] - panel.padding[2]).max(0.0),
                        context,
                    )?;
                }
                Ok(id)
            }
            ElementKind::SplitContainer(definition) => {
                let parts = ui
                    .split_container(parent, split_settings(definition), style)
                    .map_err(err)?;
                for (host, child) in [parts.first, parts.second]
                    .into_iter()
                    .zip(&definition.children)
                {
                    let mut host_style = ui.runtime.tree().node(host).unwrap().props.style.clone();
                    host_style.kind = LayoutKind::Grid;
                    host_style.grid_columns = vec![GridTrack::Fr(1.0)];
                    host_style.grid_rows = vec![GridTrack::Fr(1.0)];
                    ui.runtime
                        .apply(Mutation::SetStyle {
                            node: host,
                            style: host_style,
                        })
                        .map_err(err)?;
                    self.mount_element(ui, host, child, Axis::Vertical, width, context)?;
                }
                Ok(parts.container)
            }
            ElementKind::Slider(definition) => ui
                .slider(parent, slider_settings(definition), style)
                .map_err(err),
            ElementKind::RangeSlider(definition) => ui
                .range_slider(parent, range_settings(definition), style)
                .map_err(err),
            ElementKind::Switch(definition) => {
                let id = ui
                    .switch(parent, &definition.content, definition.is_checked, style)
                    .map_err(err)?;
                ui.set_button_style(id, button_style(&definition.style))
                    .map_err(err)?;
                self.checks.push(id);
                Ok(id)
            }
            ElementKind::RadioGroup(definition) => {
                let items = definition
                    .items
                    .iter()
                    .map(|item| SelectionItem {
                        key: item.value.clone(),
                        text: item.display.clone(),
                        enabled: item.is_enabled,
                        columns: Vec::new(),
                    })
                    .collect();
                let group = ui
                    .radio_choice_group(parent, items, definition.selected_value.as_deref(), style)
                    .map_err(err)?;
                Ok(group.container)
            }
            ElementKind::BusyIndicator(definition) => {
                let id = ui
                    .progress_bar(
                        parent,
                        ProgressBarSettings {
                            minimum: definition.minimum,
                            maximum: definition.maximum,
                            value: definition.value,
                        },
                        style,
                    )
                    .map_err(err)?;
                ui.set_progress_indeterminate(id, true).map_err(err)?;
                if definition.reduced_motion {
                    ui.set_reduced_motion(true);
                }
                Ok(id)
            }
            ElementKind::ToolBar(definition) => {
                let staging = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            width: Length::Px(0.0),
                            height: Length::Px(0.0),
                            ..Default::default()
                        },
                    )
                    .map_err(err)?;
                let mut items = Vec::new();
                for (index, child) in definition.children.iter().enumerate() {
                    let child_width = desired_width(ui, child)?.clamp(1.0, 16384.0);
                    let child_id = self.mount_element(
                        ui,
                        staging,
                        child,
                        Axis::Horizontal,
                        child_width,
                        context,
                    )?;
                    let key = child
                        .common
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("item.{index}"));
                    let mut item = ToolBarItem::editor(key, child_id, child_width);
                    item.visible = child.common.visibility != x::Visibility::Collapsed;
                    items.push(item);
                }
                let bar = ui
                    .tool_bar(parent, &definition.key, &items, style)
                    .map_err(err)?;
                ui.remove(staging).map_err(err)?;
                Ok(bar.container)
            }
            ElementKind::MenuBar(definition) => {
                let mut items = Vec::new();
                for child in &definition.items {
                    let ElementKind::DropDownButton(menu) = &child.kind else {
                        return Err("MenuBar accepts DropDownButton items".into());
                    };
                    items.push(MenuBarItem {
                        label: menu.button.content.clone(),
                        entries: menu_entries(&menu.items, &mut self.commands),
                        enabled: child.common.is_enabled,
                    });
                }
                let bar = ui.menu_bar(parent, &items, style).map_err(err)?;
                for (id, child) in bar.items.iter().copied().zip(&definition.items) {
                    let ElementKind::DropDownButton(menu) = &child.kind else {
                        unreachable!()
                    };
                    ui.set_button_style(id, button_style(&menu.button.style))
                        .map_err(err)?;
                    ui.set_button_icon(
                        id,
                        button_icon(
                            menu.button.icon,
                            menu.button.icon_size,
                            menu.button.icon_spacing,
                        ),
                    )
                    .map_err(err)?;
                    if let Some(name) = &child.common.name {
                        self.names.insert(name.clone(), id);
                    }
                    if let Some(name) = &child.common.automation_name {
                        self.automation_names.insert(id, name.clone());
                        ui.set_automation_name(id, name).map_err(err)?;
                    }
                    if let Some(tip) = &child.common.tool_tip {
                        ui.set_tooltip(id, tip).map_err(err)?;
                    }
                    if child.common.visibility != x::Visibility::Visible {
                        let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
                        props.visible = false;
                        props.collapsed = child.common.visibility == x::Visibility::Collapsed;
                        ui.runtime
                            .apply(Mutation::SetProps { node: id, props })
                            .map_err(err)?;
                    }
                    apply_common_extras(ui, id, child)?;
                    self.roles.insert(id, Role::MenuItem);
                    self.record_bindings(
                        id,
                        child,
                        bindings::context_path(context, child.common.data_context.as_deref()),
                    );
                }
                Ok(bar.container)
            }
            ElementKind::Popup(definition) => {
                let placeholder = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            width: Length::Px(0.0),
                            height: Length::Px(0.0),
                            ..Default::default()
                        },
                    )
                    .map_err(err)?;
                let flyout = ui
                    .flyout(
                        placeholder,
                        Size::new(definition.popup_width, definition.popup_height),
                    )
                    .map_err(err)?;
                ui.set_flyout_placement(
                    flyout,
                    Size::new(definition.popup_width, definition.popup_height),
                    match definition.placement {
                        x::PopupPlacementDefinition::Below => FlyoutPlacement::Below,
                        x::PopupPlacementDefinition::Beside => FlyoutPlacement::Beside,
                    },
                )
                .map_err(err)?;
                if let Some(child) = &definition.child {
                    self.mount_element(
                        ui,
                        flyout.content,
                        child,
                        Axis::Vertical,
                        (definition.popup_width - 16.0).max(0.0),
                        context,
                    )?;
                }
                self.pending_popups.push(PendingPopup {
                    flyout,
                    placeholder,
                    target: definition.placement_target.clone(),
                    open: definition.is_open,
                });
                Ok(flyout.container)
            }
            _ => Err("Unsupported extra element".into()),
        }
    }
    pub(super) fn initialize_popups(&mut self, ui: &mut Controls) -> Result<(), String> {
        for pending in std::mem::take(&mut self.pending_popups) {
            let anchor = self
                .find_name(&pending.target)
                .ok_or_else(|| format!("Unknown Popup PlacementTarget '{}'", pending.target))?;
            let anchor = self
                .splits
                .get(&anchor)
                .map_or(anchor, |parts| parts.dropdown);
            ui.set_flyout_anchor(pending.flyout, anchor).map_err(err)?;
            if ui.can_attach_flyout(anchor) {
                ui.set_button_flyout(anchor, pending.flyout).map_err(err)?;
            }
            ui.remove(pending.placeholder).map_err(err)?;
            if pending.open {
                self.initial_popups.push(pending.flyout);
            }
        }
        Ok(())
    }
    /// Called only after the anchor's first arranged geometry is available.
    pub(super) fn open_initial_popups(&mut self, ui: &mut Controls) -> Result<bool, String> {
        if self.initial_popups.is_empty() {
            return Ok(false);
        }
        for popup in std::mem::take(&mut self.initial_popups) {
            ui.open_flyout(popup).map_err(err)?;
        }
        Ok(true)
    }
}

pub(super) fn split_settings(s: &x::SplitContainerDefinition) -> SplitterSettings {
    SplitterSettings {
        axis: axis(s.orientation),
        ratio: s.ratio,
        first_min: s.first_min_size,
        second_min: s.second_min_size,
        first_max: s.first_max_size.unwrap_or(f32::INFINITY),
        second_max: s.second_max_size.unwrap_or(f32::INFINITY),
        divider_width: s.divider_width,
        collapsed: match s.collapsed_pane {
            x::SplitPaneDefinition::None => None,
            x::SplitPaneDefinition::First => Some(SplitPane::First),
            x::SplitPaneDefinition::Second => Some(SplitPane::Second),
        },
    }
}
pub(super) fn slider_settings(s: &x::SliderDefinition) -> SliderSettings {
    SliderSettings {
        minimum: s.minimum,
        maximum: s.maximum,
        value: s.value,
        step: s.step,
        axis: axis(s.orientation),
        reversed: s.is_reversed,
        snap_to_ticks: s.snap_to_ticks,
        tick_frequency: Some(s.tick_frequency),
    }
}
fn range_settings(s: &x::RangeSliderDefinition) -> RangeSliderSettings {
    let v = slider_settings(&s.slider);
    RangeSliderSettings {
        minimum: v.minimum,
        maximum: v.maximum,
        lower: s.lower_value,
        upper: s.upper_value,
        step: v.step,
        axis: v.axis,
        reversed: v.reversed,
        snap_to_ticks: v.snap_to_ticks,
        tick_frequency: v.tick_frequency,
        crossing: match s.crossing_policy {
            x::RangeCrossingPolicyDefinition::Clamp => ThumbCrossing::Clamp,
            x::RangeCrossingPolicyDefinition::Swap => ThumbCrossing::Swap,
        },
    }
}

pub(super) fn configure_text(
    ui: &mut Controls,
    id: WidgetId,
    text: &x::TextBlockDefinition,
) -> Result<(), String> {
    ui.set_text_display(
        id,
        TextDisplaySettings {
            trimming: match text.trimming {
                x::TextTrimmingDefinition::None => TextTrimming::None,
                x::TextTrimmingDefinition::CharacterEllipsis => TextTrimming::CharacterEllipsis,
                x::TextTrimmingDefinition::WordEllipsis => TextTrimming::WordEllipsis,
            },
            max_lines: text.max_lines,
        },
    )
    .map_err(err)?;
    if !text.inlines.is_empty() {
        ui.set_text_inlines(
            id,
            text.inlines
                .iter()
                .map(|span| InlineSpan {
                    text: span.text.clone(),
                    font_size: span.font_size,
                    font_weight: span.font_weight,
                    foreground: span.foreground.map(color),
                })
                .collect(),
        )
        .map_err(err)?;
    }
    Ok(())
}
pub(super) fn apply_common_extras(
    ui: &mut Controls,
    id: WidgetId,
    element: &ElementDefinition,
) -> Result<(), String> {
    let common = &element.common;
    if common.is_link {
        ui.set_button_chrome(id, ButtonChrome::Link).map_err(err)?;
    }
    if let Some(delay) = common.repeat_delay_ms {
        ui.set_repeat_button(
            id,
            Some(RepeatButtonSettings {
                delay: std::time::Duration::from_millis(u64::from(delay)),
                interval: std::time::Duration::from_millis(u64::from(common.repeat_interval_ms)),
            }),
        )
        .map_err(err)?;
    }
    if let Some(tip) = &common.rich_tool_tip {
        let image = tip
            .image
            .as_deref()
            .map(|name| {
                ui.image_resource_asset(name)
                    .map(|asset| asset.bitmap().clone())
                    .map_err(err)
            })
            .transpose()?;
        ui.set_rich_tooltip(
            id,
            RichToolTip {
                header: tip.header.clone(),
                text: common.tool_tip.clone().unwrap_or_default(),
                icon: tip.icon.map(icon_symbol),
                image,
                image_description: tip.image_description.clone(),
            },
        )
        .map_err(err)?;
    }
    Ok(())
}
pub(super) fn container_desired_width(
    ui: &mut Controls,
    element: &ElementDefinition,
) -> Result<f32, String> {
    let horizontal = !matches!(&element.kind,ElementKind::SplitContainer(s) if s.orientation==Orientation::Vertical);
    let mut width = 0.0_f32;
    for child in element.children() {
        let value = desired_width(ui, child)? + child.common.margin[0] + child.common.margin[2];
        if horizontal {
            width += value + 4.0;
        } else {
            width = width.max(value);
        }
    }
    Ok(width.max(1.0))
}
pub(super) fn validate_extra(ui: &mut Controls, element: &ElementDefinition) -> Result<(), String> {
    let common = &element.common;
    if (common.is_link || common.repeat_delay_ms.is_some())
        && !matches!(element.kind, ElementKind::Button(_))
    {
        return Err("Link and repeat presentation require a plain Button".into());
    }
    if let Some(delay) = common.repeat_delay_ms
        && (!(1..=60000).contains(&delay) || !(16..=60000).contains(&common.repeat_interval_ms))
    {
        return Err("Invalid repeat delay or interval".into());
    }
    if let Some(tip) = &common.rich_tool_tip {
        let description = [
            tip.header.as_str(),
            common.tool_tip.as_deref().unwrap_or_default(),
            tip.image_description.as_str(),
        ]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        if description.len() > 4096
            || tip.header.len() > 1024
            || (tip.image.is_some() && tip.image_description.trim().is_empty())
        {
            return Err("Invalid rich tooltip text or image description".into());
        }
        if let Some(name) = &tip.image {
            ui.image_resource_asset(name).map_err(err)?;
        }
    }
    if let ElementKind::TextBlock(text) = &element.kind {
        if !text.inlines.is_empty() && element.common.bindings.contains_key("Text") {
            return Err("Inline content cannot also bind Text".into());
        }
        let inlines = text
            .inlines
            .iter()
            .map(|span| InlineSpan {
                text: span.text.clone(),
                font_size: span.font_size,
                font_weight: span.font_weight,
                foreground: span.foreground.map(color),
            })
            .collect::<Vec<_>>();
        validate_text_inlines(&inlines).map_err(err)?;
    }
    match &element.kind {
        ElementKind::SplitContainer(s) => {
            x::validate_split_container(s)?;
            split_settings(s).validate().map_err(err)?;
        }
        ElementKind::Slider(s) => {
            x::validate_slider(s)?;
            slider_settings(s).validate().map_err(err)?;
        }
        ElementKind::RangeSlider(s) => {
            x::validate_range_slider(s)?;
            range_settings(s).validate().map_err(err)?;
        }
        ElementKind::ToolBar(bar)
            if bar.children.is_empty()
                || bar.children.len() > 128
                || bar.key.is_empty()
                || bar.key.len() > 128
                || !bar
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) =>
        {
            return Err("Invalid ToolBar key or item count".into());
        }
        ElementKind::MenuBar(bar)
            if bar.items.is_empty()
                || bar.items.len() > 32
                || bar
                    .items
                    .iter()
                    .any(|i| !matches!(i.kind, ElementKind::DropDownButton(_))) =>
        {
            return Err("MenuBar requires 1..32 DropDownButton items".into());
        }
        ElementKind::Popup(p)
            if p.child.is_none()
                || p.placement_target.is_empty()
                || [p.popup_width, p.popup_height]
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0.0 || *v > 16384.0) =>
        {
            return Err("Invalid Popup target/content/size".into());
        }
        ElementKind::ToggleButton(c)
            if c.is_indeterminate && !c.is_three_state
                || c.is_three_state && c.command.is_some() =>
        {
            return Err("Indeterminate requires a local three-state ToggleButton".into());
        }
        ElementKind::CheckBox(c) if c.is_indeterminate && !c.is_three_state => {
            return Err("Indeterminate requires IsThreeState=True".into());
        }
        ElementKind::Switch(c) if c.is_three_state || c.is_indeterminate => {
            return Err("Switch has two states".into());
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_popup_targets(root: &ElementDefinition) -> Result<(), String> {
    let mut names = HashMap::new();
    let mut popups = Vec::new();
    let mut pending = vec![root];
    while let Some(element) = pending.pop() {
        if let Some(name) = &element.common.name {
            names.insert(name.as_str(), element);
        }
        if let ElementKind::Popup(popup) = &element.kind {
            popups.push((element, popup));
        }
        pending.extend(element.children());
    }
    for (element, popup) in popups {
        let target = names
            .get(popup.placement_target.as_str())
            .ok_or_else(|| format!("Unknown Popup PlacementTarget '{}'", popup.placement_target))?;
        let mut descendants = vec![element];
        while let Some(child) = descendants.pop() {
            if std::ptr::eq(child, *target) {
                return Err("Popup cannot use itself or its own content as PlacementTarget".into());
            }
            descendants.extend(child.children());
        }
    }
    Ok(())
}

pub(super) fn text_desired_width(
    ui: &mut Controls,
    text: &x::TextBlockDefinition,
) -> Result<f32, String> {
    let base = TextStyle {
        wrap: false,
        ..text_style(text)
    };
    if text.inlines.is_empty() {
        return Ok(ui
            .measure_styled_text(&text.text, 16384.0, base)
            .map_err(err)?
            .width
            + 16.0);
    }
    let mut width = 16.0;
    for span in &text.inlines {
        let style = TextStyle {
            font_size: span.font_size.or(base.font_size),
            font_weight: span.font_weight.or(base.font_weight),
            foreground: span.foreground.map(color).or(base.foreground),
            ..base
        };
        width += ui
            .measure_styled_text(&span.text, 16384.0, style)
            .map_err(err)?
            .width;
    }
    Ok(width.min(16384.0))
}
