use super::*;

#[derive(Clone, Copy)]
pub(in crate::controls) enum ValueMark {
    Plain,
    Check(CheckState),
    Switch(CheckState),
    Swatch(Srgba8),
    Percent(u8),
    Choice,
}

impl ValueMark {
    /// Draw inside the value button, retaining its keyboard, focus and accessibility route.
    pub(in crate::controls) fn paint(
        self,
        scene: &mut Scene,
        mut text: Rect,
        clip: Rect,
        theme: Theme,
        foreground: Color,
    ) -> Rect {
        let Some(clip) = text.intersection(clip) else {
            return text;
        };
        let chrome = theme.chrome();
        let accent = theme.tokens().focus;
        let middle = text.y + text.height * 0.5;
        let end = text.x + text.width;
        match self {
            Self::Check(_) | Self::Swatch(_) if text.width < 44. || text.height < 16. => (),
            Self::Check(state) => {
                let b = Rect::new(text.x, middle - 8., 16., 16.);
                if state == CheckState::Unchecked {
                    shape_border(scene, b, clip, 3., 1., foreground);
                } else {
                    scene.rounded_fill(b, clip, 3., accent);
                    if state == CheckState::Checked {
                        paint_checkmark(scene, b, clip, theme.on_accent());
                    } else {
                        fill_clipped(
                            scene,
                            Rect::new(b.x + 4., middle - 1., 8., 2.),
                            clip,
                            theme.on_accent(),
                        );
                    }
                }
                text.x += 23.;
                text.width -= 23.;
            }
            Self::Swatch(color) => {
                let b = Rect::new(text.x, middle - 8., 20., 16.);
                if color.a < 255 {
                    fill_clipped(scene, b, clip, Color::rgb(255, 255, 255));
                    for row in 0..4 {
                        for col in (row % 2..5).step_by(2) {
                            fill_clipped(
                                scene,
                                Rect::new(b.x + col as f32 * 4., b.y + row as f32 * 4., 4., 4.),
                                clip,
                                Color::rgb(185, 185, 185),
                            );
                        }
                    }
                }
                fill_clipped(scene, b, clip, color.into());
                shape_border(scene, b, clip, 0., 1., chrome.control_bottom_border);
                text.x += 27.;
                text.width -= 27.;
            }
            Self::Switch(state) if text.width >= 90. => {
                let b = Rect::new(end - 32., middle - 9., 32., 18.);
                let (x, knob) = match state {
                    CheckState::Checked => {
                        scene.rounded_fill(b, clip, 9., accent);
                        (end - 15., theme.on_accent())
                    }
                    CheckState::Unchecked | CheckState::Indeterminate => {
                        shape_border(scene, b, clip, 9., 1., foreground);
                        (
                            b.x + if state == CheckState::Unchecked {
                                3.
                            } else {
                                10.
                            },
                            foreground,
                        )
                    }
                };
                scene.rounded_fill(Rect::new(x, middle - 6., 12., 12.), clip, 6., knob);
                text.width -= 39.;
            }
            Self::Percent(value) if text.width >= 96. => {
                let label_width = (theme.tokens().font_size * 3.8).min(text.width * 0.55);
                let bar = Rect::new(
                    text.x + label_width,
                    middle - 2.,
                    text.width - label_width,
                    4.,
                );
                scene.rounded_fill(bar, clip, 2., chrome.control_bottom_border);
                if value != 0 {
                    scene.rounded_fill(
                        Rect::new(
                            bar.x,
                            bar.y,
                            bar.width * f32::from(value) / 100.,
                            bar.height,
                        ),
                        clip,
                        2.,
                        accent,
                    );
                }
                text.width = label_width;
            }
            Self::Choice if text.width >= 40. => {
                crate::primitives::paint_icon(
                    scene,
                    IconSymbol::ChevronDown,
                    Rect::new(end - 12., middle - 6., 12., 12.),
                    clip,
                    foreground,
                );
                text.width -= 18.;
            }
            _ => (),
        }
        text
    }
}

#[derive(Clone, Copy)]
pub(in crate::controls) enum InspectorPart {
    Row {
        value_host: Option<WidgetId>,
        alternate: bool,
    },
    Name,
    Category {
        owner: WidgetId,
        depth: usize,
    },
    Viewport,
    Value(ValueMark),
    BooleanCell,
    ReadOnlyText {
        host: WidgetId,
    },
}

pub(super) fn vertical_stack() -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Stack(Axis::Vertical),
        width: Length::Percent(1.),
        ..Default::default()
    }
}

pub(super) fn show(ui: &mut Controls, id: WidgetId, visible: bool) -> Result<(), ControlError> {
    let props = &ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| invalid("Inspector part was removed"))?
        .props;
    if props.visible != visible || props.collapsed == visible {
        ui.edit_props(id, |p| {
            p.visible = visible;
            p.collapsed = !visible;
        })?;
    }
    Ok(())
}

pub(super) fn action_icon(
    ui: &mut Controls,
    row: WidgetId,
    icon: IconSymbol,
    name: &str,
) -> Result<WidgetId, ControlError> {
    let id = ui.button(
        row,
        "",
        None,
        LayoutStyle {
            height: Length::Px(28.),
            ..button_style(26.)
        },
    )?;
    ui.set_button_chrome(id, ButtonChrome::Flat)?;
    ui.set_button_icon(id, Some(ButtonIcon::new(icon)))?;
    ui.set_automation_name(id, name)?;
    ui.set_tooltip(id, name)?;
    Ok(id)
}

pub(super) fn toolbar(
    ui: &mut Controls,
    parent: WidgetId,
) -> Result<(WidgetId, WidgetId, WidgetId), ControlError> {
    let bar = ui.transparent_panel(
        parent,
        LayoutStyle {
            kind: LayoutKind::Flex(Axis::Horizontal),
            padding: Edges::all(4.),
            gap: 4.,
            width: Length::Percent(1.),
            ..Default::default()
        },
    )?;
    let categories = ui.toggle(bar, "Группы", None, true, button_style(32.))?;
    let sorted = ui.toggle(bar, "А—Я", None, false, button_style(32.))?;
    for (id, title, glyph) in [
        (categories, "По категориям", IconSymbol::GroupedList),
        (sorted, "По алфавиту", IconSymbol::FlatList),
    ] {
        ui.set_button_icon(id, Some(ButtonIcon::new(glyph)))?;
        ui.set_button_content_layout(id, ButtonContentLayout::IconOnly)?;
        ui.set_button_chrome(id, ButtonChrome::Flat)?;
        ui.set_automation_name(id, title)?;
        ui.set_tooltip(id, title)?;
    }
    let search = ui.edit_box(
        bar,
        "Поиск свойств",
        "",
        LayoutStyle {
            flex_grow: 1.,
            height: Length::Px(32.),
            ..Default::default()
        },
    )?;
    ui.set_max_length(search, Some(256))?;
    ui.set_placeholder(search, "Поиск свойств")?;
    Ok((search, categories, sorted))
}

impl PropertyGrid {
    /// Update the scroll range from arranged row groups after a resize or filter.
    pub fn update_scroll_extent(&self, ui: &mut Controls) -> Result<(), ControlError> {
        let Some(scroll) = &self.viewport else {
            return Ok(());
        };
        let Some(bounds) = ui.runtime.bounds(scroll.viewport) else {
            return Ok(());
        };
        let height = ui
            .runtime
            .tree()
            .children(scroll.content)
            .unwrap_or_default()
            .iter()
            .filter(|id| {
                ui.runtime
                    .tree()
                    .node(**id)
                    .is_some_and(|n| !n.props.collapsed)
            })
            .filter_map(|id| ui.runtime.layout_bounds(*id))
            .fold(bounds.height, |height, r| height.max(r.y + r.height));
        ui.set_scroll_measured_extent(scroll.viewport, Size::new(bounds.width, height))
    }
    /// Row viewport of a scrollable inspector.
    pub fn row_viewport(&self) -> Option<WidgetId> {
        self.viewport.as_ref().map(|s| s.viewport)
    }

    pub(super) fn select(
        &mut self,
        ui: &mut Controls,
        index: Option<usize>,
    ) -> Result<bool, ControlError> {
        let Some((title, hint)) = self.details else {
            return Ok(false);
        };
        if index == self.selection {
            return Ok(false);
        }
        for (item, weight) in [(self.selection, 400), (index, 600)] {
            if let Some(i) = item {
                let id = self.properties[i].name_label;
                let mut style = ui.text_style(id).unwrap_or_default();
                style.font_weight = Some(weight);
                ui.set_text_style(id, style)?;
            }
        }
        self.selection = index;
        let selected = index.map(|i| &self.properties[i]);
        ui.set_label(
            title,
            selected.map_or("", |p| {
                p.descriptor
                    .metadata
                    .label
                    .as_deref()
                    .unwrap_or(&p.schema.title)
            }),
        )?;
        ui.set_label(hint, selected.map_or("", |p| &p.descriptor.description))?;
        Ok(true)
    }

    pub(super) fn sync_selection(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        if self.details.is_none() {
            return Ok(false);
        }
        let focus = ui.runtime.focused();
        if focus != self.last_focus {
            self.last_focus = focus;
            if let Some(id) = focus
                && let Some(i) = self
                    .properties
                    .iter()
                    .position(|p| ui.flyout_descendant(id, p.row) && ui.effective_enabled(p.row))
            {
                return self.select(ui, Some(i));
            }
        }
        if self
            .selection
            .is_some_and(|i| ui.effective_enabled(self.properties[i].row))
        {
            return Ok(false);
        }
        let first = self
            .properties
            .iter()
            .position(|p| ui.effective_enabled(p.row));
        self.select(ui, first)
    }

    pub(super) fn activate(&mut self, ui: &mut Controls, index: usize) -> Result<(), ControlError> {
        self.select(ui, Some(index))?;
        for (i, p) in self.properties.iter_mut().enumerate() {
            if p.active != (i == index) {
                p.active = i == index;
                present(ui, p)?;
            }
        }
        Ok(())
    }

    pub(super) fn restripe(&self, ui: &mut Controls) {
        let hosts = self.flat.then_some(self.flat_host).into_iter().chain(
            self.groups
                .iter()
                .filter(|_| !self.flat)
                .map(|(_, group)| group.content),
        );
        for host in hosts {
            let mut odd = false;
            for id in ui.runtime.tree().children(host).unwrap_or_default() {
                if ui
                    .runtime
                    .tree()
                    .node(*id)
                    .is_some_and(|n| !n.props.collapsed)
                    && let Some(InspectorPart::Row { alternate, .. }) = ui
                        .controls
                        .get_mut(id)
                        .and_then(|c| c.inspector_part.as_mut())
                {
                    *alternate = odd;
                    odd = !odd;
                }
            }
        }
    }

    pub(super) fn arrange(
        &mut self,
        ui: &mut Controls,
        flat: bool,
        query: String,
    ) -> Result<(), ControlError> {
        if flat == self.flat && query == self.filter {
            return Ok(());
        }
        if flat != self.flat {
            let order = if flat {
                let mut rows: Vec<_> = self.properties.iter().collect();
                rows.sort_by_cached_key(|p| {
                    p.descriptor
                        .metadata
                        .label
                        .as_ref()
                        .unwrap_or(&p.schema.title)
                        .to_lowercase()
                });
                rows.into_iter().map(|p| (p.row, self.flat_host)).collect()
            } else {
                self.grouped_order.clone()
            };
            ui.runtime.apply(Mutation::Batch(
                order
                    .iter()
                    .map(|&(node, parent)| Mutation::Reparent { node, parent })
                    .collect(),
            ))?;
            let moved: HashMap<_, _> = order.into_iter().collect();
            for (id, parent) in &mut self.parents {
                if let Some(new) = moved.get(id) {
                    *parent = *new;
                }
            }
            show(ui, self.flat_host, flat)?;
        }
        if self.filter.is_empty() && !query.is_empty() {
            self.saved_expansion = self
                .groups
                .iter()
                .map(|(_, g)| (g.container, ui.expander_expanded(g.container) == Some(true)))
                .collect();
        } else if query.is_empty() {
            for (id, expanded) in self.saved_expansion.drain(..) {
                ui.set_expander_expanded(id, expanded)?;
            }
        }
        let mut paths = HashSet::new();
        let mut any = false;
        for p in &self.properties {
            let d = &p.descriptor;
            let path: Vec<_> = std::iter::once(&d.category)
                .chain(&d.groups)
                .cloned()
                .collect();
            let matches = d.metadata.visible
                && (query.is_empty()
                    || path
                        .iter()
                        .chain([
                            &p.schema.name,
                            d.metadata.label.as_ref().unwrap_or(&p.schema.title),
                        ])
                        .any(|part| part.to_lowercase().contains(&query)));
            show(ui, p.row, matches)?;
            if matches {
                any = true;
                for n in 1..=path.len() {
                    paths.insert(path[..n].to_vec());
                }
            }
        }
        for (path, g) in &self.groups {
            let visible = !flat && paths.contains(path);
            show(ui, g.container, visible)?;
            if visible && !query.is_empty() {
                ui.set_expander_expanded(g.container, true)?;
            }
        }
        show(ui, self.empty, !any)?;
        self.flat = flat;
        self.filter = query;
        self.restripe(ui);
        Ok(())
    }
}

impl Controls {
    pub(in crate::controls) fn readable_label(&self, id: WidgetId, enabled: bool) -> bool {
        enabled
            || matches!(self.controls.get(&id).and_then(|c| c.inspector_part),
            Some(InspectorPart::ReadOnlyText {host}) if self.is_enabled(host))
    }

    pub(in crate::controls) fn paint_inspector_part(
        &mut self,
        id: WidgetId,
        b: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(part) = self.controls.get(&id).and_then(|c| c.inspector_part) else {
            return Ok(false);
        };
        let chrome = self.theme.chrome();
        let line = chrome.divider;
        match part {
            InspectorPart::Value(_)
            | InspectorPart::BooleanCell
            | InspectorPart::Name
            | InspectorPart::ReadOnlyText { .. } => {
                return Ok(false);
            }
            InspectorPart::Viewport => {
                fill_clipped(&mut self.scene, b, clip, chrome.card_surface);
                shape_border(&mut self.scene, b, clip, 0., 1., chrome.surface_border);
                return Ok(true);
            }
            InspectorPart::Row {
                value_host,
                alternate,
            } => {
                let color = if alternate {
                    self.theme.inspector_alternate_row()
                } else {
                    chrome.card_surface
                };
                fill_clipped(&mut self.scene, b, clip, color);
                if let Some(host) = value_host.and_then(|host| self.runtime.bounds(host))
                    && host.x > b.x + 1.
                    && host.x < b.x + b.width
                {
                    fill_clipped(
                        &mut self.scene,
                        Rect::new(host.x - 1., b.y, 1., b.height),
                        clip,
                        line,
                    );
                }
            }
            InspectorPart::Category { owner, depth } => {
                let state = self.button_visual_state(id).unwrap_or_default();
                let expanded = self.expander_expanded(owner) == Some(true);
                let tokens = self.theme.tokens();
                let background = if state.hovered {
                    chrome.control_hover
                } else {
                    tokens.canvas
                };
                fill_clipped(&mut self.scene, b, clip, background);
                let x = b.x + 8. + (depth as f32) * 12.;
                let glyph = Rect::new(x, b.y + (b.height - 12.) / 2., 12., 12.);
                let color = if state.enabled {
                    tokens.text
                } else {
                    chrome.disabled_text
                };
                crate::primitives::paint_icon(
                    &mut self.scene,
                    if expanded {
                        IconSymbol::ChevronDown
                    } else {
                        IconSymbol::ChevronRight
                    },
                    glyph,
                    clip,
                    color,
                );
                let title = self.controls[&id].label.clone();
                let mut run = TextRun::new(
                    title,
                    Rect::new(x + 20., b.y, (b.x + b.width - x - 24.).max(0.), b.height),
                    color,
                    13.,
                );
                run.weight = if depth == 0 { 550 } else { 450 };
                let h = self
                    .engine
                    .prepare(&run, 1.)
                    .map_err(|e| ControlError::Text(e.to_string()))?
                    .height;
                run.bounds.y += ((b.height - h) * 0.5).max(0.);
                let Some(visible) = b.intersection(clip) else {
                    return Ok(true);
                };
                run.clip = visible;
                self.scene.text(run);
                if state.focused {
                    shape_border(&mut self.scene, b, clip, 0., 1., tokens.focus);
                }
            }
        }
        fill_clipped(
            &mut self.scene,
            Rect::new(b.x, b.y + b.height - 1., b.width, 1.),
            clip,
            line,
        );
        Ok(true)
    }
}
