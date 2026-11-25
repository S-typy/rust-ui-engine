use super::*;

#[derive(Clone, Copy)]
pub(in crate::controls) enum InspectorPart {
    Row { split: f32, alternate: bool },
    Category { owner: WidgetId, depth: usize },
    Viewport,
    Value,
    ReadOnlyText { host: WidgetId },
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
    let id = ui.button(row, "", None, button_style(30.))?;
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
    let categories = ui.toggle(bar, "Группы", None, true, button_style(72.))?;
    let sorted = ui.toggle(bar, "А—Я", None, false, button_style(48.))?;
    for (id, title) in [(categories, "По категориям"), (sorted, "По алфавиту")]
    {
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

    pub(super) fn activate(&mut self, ui: &mut Controls, index: usize) -> Result<(), ControlError> {
        for (i, p) in self.properties.iter_mut().enumerate() {
            if p.active != (i == index) {
                p.active = i == index;
                present(ui, p)?;
            }
        }
        Ok(())
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
        let line = self.theme.grid_line();
        let chrome = self.theme.chrome();
        match part {
            InspectorPart::Value | InspectorPart::ReadOnlyText { .. } => return Ok(false),
            InspectorPart::Viewport => {
                fill_clipped(&mut self.scene, b, clip, chrome.card_surface);
                shape_border(&mut self.scene, b, clip, 0., 1., line);
                return Ok(true);
            }
            InspectorPart::Row { split, alternate } => {
                let color = if alternate {
                    self.theme.grid_alternate_row()
                } else {
                    chrome.card_surface
                };
                fill_clipped(&mut self.scene, b, clip, color);
                if split > 0. && split < b.width {
                    fill_clipped(
                        &mut self.scene,
                        Rect::new(b.x + split, b.y, 1., b.height),
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
                run.weight = 600;
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
