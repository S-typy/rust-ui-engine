//! Input and presentation for column filters, find, cell ranges and record drops.
use super::header_filter::{Action, FilterPopup};
use super::*;
use rust_desktop_ui_treegrid::GridHitKind;
use std::time::{Duration, Instant};

pub(super) enum Panel {
    Text { target: TextTarget, draft: String },
    Facet(FilterPopup),
}
pub(super) enum TextTarget {
    Find,
    Column(String),
    Criteria,
}
pub(super) struct Gesture {
    origin: (i64, String),
    point: Point,
    append: bool,
    external: bool,
    rows: Option<Vec<i64>>,
}
#[derive(Default)]
pub(super) struct Workflow {
    auto_filter: bool,
    selector: bool,
    pub(super) panel: Option<Panel>,
    quick: Option<Instant>,
    pub(super) gesture: Option<Gesture>,
    drop_order: Option<String>,
    allow_copy: bool,
    external_requested: bool,
    external_move: bool,
    export: Option<Vec<rust_desktop_ui_data::DataRow>>,
}
impl GridState {
    pub(super) fn filter_height(&self, b: Rect) -> f32 {
        if self.workflow.auto_filter && self.source.presentation.view == GridViewMode::Table {
            (26.0 * self.model.config().header_height
                / rust_desktop_ui_treegrid::GridConfig::default().header_height)
                .min(b.height)
        } else {
            0.
        }
    }
    pub(super) fn selector_width(&self, b: Rect) -> f32 {
        if self.workflow.selector && self.source.presentation.view == GridViewMode::Table {
            26_f32.min(b.width)
        } else {
            0.
        }
    }
    fn filter_strip(&self, b: Rect) -> Rect {
        let body = self.presentation_bounds(b);
        let h = self.filter_height(b);
        Rect::new(body.x, body.y - h, body.width, h)
    }
    fn filter_cells(&self, b: Rect) -> impl Iterator<Item = (ColumnId, Rect, Rect)> + '_ {
        let strip = self.filter_strip(b);
        self.frame.columns.iter().filter_map(move |column| {
            let cell = self.model.track_cell(
                column.id,
                column.bounds.x,
                strip.y,
                column.bounds.width,
                strip.height,
            );
            let lane = Rect::new(column.clip.x, strip.y, column.clip.width, strip.height);
            Some((
                column.id,
                cell,
                cell.intersection(lane)?.intersection(strip)?,
            ))
        })
    }
    fn panel_area(&self, b: Rect) -> Rect {
        Rect::new(b.x, b.y, b.width.min(450.), b.height.min(244.))
    }
    fn selection_header(&self, b: Rect) -> Rect {
        let body = self.presentation_bounds(b);
        Rect::new(
            b.x,
            body.y,
            self.selector_width(b),
            self.model.config().header_height.min(body.height),
        )
    }
    fn hit_cell(&self, p: Point) -> Option<(i64, String)> {
        let GridHitKind::Cell { row, column } = self.frame.hit_test(p)? else {
            return None;
        };
        if self.source.projection.synthetic.contains_key(&row) {
            return None;
        }
        Some((data_key(row), self.source.fields[column.0 as usize].clone()))
    }
    pub(super) fn paint_workflow(&self, b: Rect, scene: &mut Scene) {
        let palette = self.model.palette;
        let label = |scene: &mut Scene, r: Rect, text: String| {
            views::draw_text(scene, r, r, text, palette.text, 400, 13.)
        };
        let strip = self.filter_strip(b);
        if strip.height > 0. {
            scene.fill(strip, palette.header);
            palette.paint_cell_edges(scene, strip, strip);
            for (id, cell, r) in self.filter_cells(b) {
                let field = &self.source.fields[id.0 as usize];
                let val = self
                    .source
                    .filters
                    .inputs
                    .get(field)
                    .map(String::as_str)
                    .unwrap_or("Фильтр…");
                label(scene, r, format!("  {val}"));
                palette.paint_cell_edges(scene, cell, r);
            }
        }
        let selector = self.selection_header(b);
        if selector.width > 0. {
            scene.fill(selector, palette.header);
            label(scene, selector, "  ☑".into());
            palette.paint_cell_edges(scene, selector, selector);
        }
        for row in &self.frame.semantics.rows {
            if self.source.projection.synthetic.contains_key(&row.key) {
                continue;
            }
            if selector.width > 0. {
                let r = Rect::new(selector.x, row.bounds.y, selector.width, row.bounds.height);
                if let Some(r) = r.intersection(b) {
                    scene.fill(r, palette.header);
                    let y = selector.y
                        + selector.height
                        + (self.model.record_top(row.index) - self.model.scroll_offset().1) as f32;
                    let cell = Rect::new(r.x, y, r.width, self.model.record_height(row.index));
                    palette.paint_cell_edges(scene, cell, r);
                    label(
                        scene,
                        r,
                        self.source
                            .presentation
                            .role_text("Indicator", if row.selected { "☑" } else { "☐" }),
                    );
                }
            }
            for cell in &row.cells {
                let r = cell.bounds;
                if self.range_contains(
                    data_key(row.key),
                    &self.source.fields[cell.column.0 as usize],
                ) {
                    scene.fill(Rect::new(r.x, r.y, r.width, 1.), palette.focus);
                    scene.fill(Rect::new(r.x, r.y, 1., r.height), palette.focus);
                }
                if !self.search.is_empty() && cell.label.to_lowercase().contains(&self.search) {
                    scene.fill(
                        Rect::new(
                            r.x,
                            r.y + (r.height - 3.).max(0.),
                            r.width,
                            2_f32.min(r.height),
                        ),
                        palette.focus,
                    );
                }
            }
        }
    }
    fn filter_anchor(&self, b: Rect, field: &str) -> Rect {
        let target = self
            .source
            .fields
            .iter()
            .position(|f| f == field)
            .map(|i| ColumnId(i as u64));
        self.frame
            .header_filters
            .iter()
            .find(|(id, _)| Some(*id) == target)
            .map(|(_, r)| *r)
            .or_else(|| {
                self.frame
                    .hits
                    .iter()
                    .find(|h| matches!(h.kind,GridHitKind::Header(c) if Some(c)==target))
                    .map(|h| h.bounds)
            })
            .unwrap_or(Rect::new(b.x, b.y, b.width.min(20.), 0.))
    }
    pub(super) fn paint_filter_panel(&self, b: Rect, scene: &mut Scene) {
        let palette = self.model.palette;
        let Some(panel) = &self.workflow.panel else {
            return;
        };
        let area = self.panel_area(b);
        let header = Rect::new(area.x, area.y, area.width, 30_f32.min(area.height));
        match panel {
            Panel::Text { target, draft } => {
                scene.fill(header, palette.header);
                let field = match target {
                    TextTarget::Find => "Поиск",
                    TextTarget::Column(f) => f,
                    TextTarget::Criteria => "Условие",
                };
                views::draw_text(
                    scene,
                    header,
                    header,
                    format!("  {field}: {draft} ▏"),
                    palette.text,
                    400,
                    13.,
                );
            }
            Panel::Facet(popup) => {
                popup.paint(scene, b, self.filter_anchor(b, &popup.field), palette)
            }
        }
    }
}
impl Controls {
    pub fn set_grid_workflow_rows(
        &mut self,
        id: WidgetId,
        auto_filter: bool,
        selector: bool,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        g.workflow.auto_filter = auto_filter;
        g.workflow.selector = selector;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn configure_grid_row_drop(
        &mut self,
        id: WidgetId,
        order_field: Option<&str>,
        allow_copy: bool,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        if let Some(f) = order_field
            && !g.settings.columns.iter().any(|c| {
                c.field == f
                    && !c.read_only
                    && g.source.table.schema().field(f).is_some_and(|f| {
                        f.kind == rust_desktop_ui_data::FieldType::Integer && !f.read_only
                    })
            })
        {
            return Err(error("Invalid order field"));
        }
        g.workflow.drop_order = order_field.map(str::to_owned);
        g.workflow.allow_copy = allow_copy;
        Ok(())
    }
    /// Allow deletion after a confirmed native move only when every stored field is exported.
    pub fn allow_grid_external_move(
        &mut self,
        id: WidgetId,
        allow: bool,
    ) -> Result<(), ControlError> {
        self.grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?
            .workflow
            .external_move = allow;
        Ok(())
    }
    pub fn take_grid_external_drag(&mut self) -> Option<(WidgetId, String, bool)> {
        let id = self
            .grids
            .iter()
            .find_map(|(id, g)| g.workflow.external_requested.then_some(*id))?;
        self.grids.get_mut(&id)?.workflow.external_requested = false;
        let text = self.copy_grid_selection(id, false).ok()?.tsv;
        let g = self.grids.get(&id)?;
        let data = g.source.handle.try_borrow().ok()?;
        let fields = g.model.display_column_order();
        let mut allow = g.workflow.external_move
            && !g.settings.is_read_only
            && self.effective_enabled(id)
            && g.transfer.ranges.is_empty()
            && data.write_capabilities().delete
            && data.write_capabilities().atomic_batch
            && fields.len() == data.table().schema().fields().len()
            && fields
                .iter()
                .zip(data.table().schema().fields())
                .all(|(c, f)| {
                    g.source.fields[c.0 as usize] == f.name
                        && !g.source.choice_labels.contains_key(&f.name)
                });
        let rows = if allow {
            g.visual_keys()
                .ok()?
                .into_iter()
                .filter(|k| g.model.is_selected(row_key(*k)) || g.selected_key == Some(*k))
                .filter_map(|k| data.table().row(k).cloned())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        allow &= !rows.is_empty() && rows.len() <= rust_desktop_ui_data::MAX_DATA_WRITE_OPERATIONS;
        drop(data);
        self.grids.get_mut(&id)?.workflow.export = Some(if allow { rows } else { Vec::new() });
        Some((id, text, allow))
    }
    pub fn finish_grid_external_drag(
        &mut self,
        id: WidgetId,
        moved: bool,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown drag source"))?;
        let rows = g.workflow.export.take().unwrap_or_default();
        if moved && !rows.is_empty() {
            let ops = rows
                .into_iter()
                .map(|expected| rust_desktop_ui_data::DataWrite::Delete { expected })
                .collect::<Vec<_>>();
            self.apply_grid_batch(id, &ops, None)?;
        }
        Ok(())
    }
    /// Negotiate and apply an external text drop at the actual grid cell.
    pub fn grid_text_drop(&mut self, point: Point, text: Option<&str>) -> bool {
        let Some(id) = self.runtime.hit_test(point) else {
            return false;
        };
        let Some(g) = self.grids.get(&id) else {
            return false;
        };
        if !self.effective_enabled(id)
            || g.settings.is_read_only
            || g.session.is_some()
            || g.records.draft.is_some()
            || g.workflow.export.is_some()
        {
            return false;
        }
        let Some((key, field)) = g.hit_cell(point) else {
            return false;
        };
        let Some(column) = g
            .settings
            .columns
            .iter()
            .position(|c| c.field == field && !c.read_only)
        else {
            return false;
        };
        if !g.source.editable(row_key(key), ColumnId(column as u64))
            || !g.source.handle.try_borrow().is_ok_and(|s| {
                s.write_capabilities().update
                    && s.write_capabilities().atomic_batch
                    && s.table().schema().field(&field).is_some()
            })
        {
            return false;
        }
        if let Some(text) = text {
            match self.paste_grid_tsv(id, (key, &field), text, false) {
                Ok(_) => true,
                Err(err) => {
                    self.grids.get_mut(&id).unwrap().error = Some(err.to_string());
                    self.scene_dirty = true;
                    false
                }
            }
        } else {
            true
        }
    }
    fn open_header_filter(&mut self, id: WidgetId, column: ColumnId) -> Result<bool, ControlError> {
        let grid = &self.grids[&id];
        if grid.source.header_filter(column).is_none() {
            return Ok(false);
        }
        let field = grid.source.fields[column.0 as usize].clone();
        let popup = FilterPopup::new(&grid.source, field)?;
        self.grids.get_mut(&id).unwrap().workflow.panel = Some(Panel::Facet(popup));
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        self.scene_dirty = true;
        Ok(true)
    }
    pub fn open_grid_find(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.open_grid_text_panel(id, TextTarget::Find)
    }
    fn open_grid_text_panel(
        &mut self,
        id: WidgetId,
        target: TextTarget,
    ) -> Result<(), ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if g.session.is_some() {
            return Err(error("Finish the active editor first"));
        }
        let draft = match &target {
            TextTarget::Find => g.search.clone(),
            TextTarget::Column(f) => g.source.filters.inputs.get(f).cloned().unwrap_or_default(),
            TextTarget::Criteria => self.grid_criteria(id).unwrap_or_default(),
        };
        self.grids.get_mut(&id).unwrap().workflow.panel = Some(Panel::Text { target, draft });
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        self.scene_dirty = true;
        Ok(())
    }
    pub(in crate::controls) fn grid_workflow_text(
        &mut self,
        text: &str,
    ) -> Result<bool, ControlError> {
        let Some(id) = self
            .runtime
            .focused()
            .filter(|id| self.grids.contains_key(id))
        else {
            return Ok(false);
        };
        if text.is_empty() || text.chars().any(char::is_control) {
            return Ok(false);
        }
        let g = self.grids.get_mut(&id).unwrap();
        if g.session.is_some() || g.chooser.is_some() || g.records.draft.is_some() {
            return Ok(false);
        }
        if let Some(panel) = &mut g.workflow.panel {
            match panel {
                Panel::Text { draft, .. } if draft.len() + text.len() <= 4096 => {
                    draft.push_str(text)
                }
                Panel::Facet(popup) => popup.text(text),
                _ => {}
            }
            self.scene_dirty = true;
            return Ok(true);
        }
        let now = Instant::now();
        if g.workflow
            .quick
            .is_none_or(|last| now.duration_since(last) > Duration::from_secs(1))
        {
            g.search.clear();
        }
        if g.search.len() + text.len() <= 4096 {
            g.search.push_str(&text.to_lowercase());
        }
        g.workflow.quick = Some(now);
        self.find_grid_match(id, true, true)?;
        self.scene_dirty = true;
        Ok(true)
    }
    pub(in crate::controls) fn grid_workflow_paste(
        &mut self,
        text: &str,
    ) -> Result<bool, ControlError> {
        let Some(id) = self
            .runtime
            .focused()
            .filter(|id| self.grids.contains_key(id))
        else {
            return Ok(false);
        };
        let g = &self.grids[&id];
        if g.workflow.panel.is_some() {
            return self.grid_workflow_text(text);
        }
        if let (Some(key), Some(col)) = (g.selected_key, g.model.focused_column()) {
            let field = g.source.fields[col.0 as usize].clone();
            if let Err(err) = self.paste_grid_tsv(id, (key, &field), text, false) {
                self.grids.get_mut(&id).unwrap().error = Some(err.to_string());
                self.scene_dirty = true;
            }
        }
        Ok(true)
    }
    pub(super) fn grid_range_after_navigation(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
    ) -> Result<(), ControlError> {
        let g = &self.grids[&id];
        let (Some(key), Some(col)) = (g.selected_key, g.model.focused_column()) else {
            return Ok(());
        };
        let field = g.source.fields[col.0 as usize].clone();
        let mods = match event {
            InputEvent::KeyDown { modifiers, .. } | InputEvent::PointerDown { modifiers, .. } => {
                modifiers
            }
            _ => return Ok(()),
        };
        if mods.shift
            && let Some((k, f)) = g.transfer.anchor.clone()
        {
            if g.model.visible_index(&g.source, row_key(k)).is_some() {
                self.select_grid_range(id, (k, &f), (key, &field), mods.control)?;
            }
        } else if !mods.shift {
            let t = &mut self.grids.get_mut(&id).unwrap().transfer;
            t.anchor = Some((key, field));
            if !mods.control {
                t.clear_ranges();
            }
        }
        Ok(())
    }
    pub(super) fn grid_workflow_input(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let g = &self.grids[&id];
        let Some(b) = g.bounds else { return Ok(false) };
        if g.workflow.panel.is_some() {
            let visible = self.runtime.clip(id).and_then(|clip| b.intersection(clip));
            if let Some(visible) = visible {
                return self.grid_panel_input(id, event, visible);
            }
            self.grids.get_mut(&id).unwrap().workflow.panel = None;
            return Ok(false);
        }
        if let InputEvent::KeyDown { key, modifiers, .. } = event {
            match key {
                Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("f") => {
                    self.open_grid_text_panel(
                        id,
                        if modifiers.shift {
                            TextTarget::Criteria
                        } else {
                            TextTarget::Find
                        },
                    )?;
                    return Ok(true);
                }
                Key::ArrowUp if modifiers.alt && !modifiers.control => {
                    if let Some(col) = g.model.focused_column() {
                        self.toggle_grid_header_sort(id, col, modifiers.shift)?;
                        return Ok(true);
                    }
                }
                Key::ArrowDown if modifiers.alt && !modifiers.control => {
                    if let Some(col) = g.model.focused_column() {
                        return self.open_header_filter(id, col);
                    }
                }
                Key::F3 => {
                    self.find_grid_match(id, !modifiers.shift, false)?;
                    return Ok(true);
                }
                Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("a") => {
                    self.select_all_grid_records(id)?;
                    return Ok(true);
                }
                Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("c") => {
                    if let Ok(t) = self.copy_grid_selection(id, modifiers.alt) {
                        self.clipboard.push(ClipboardRequest::CopyTable {
                            text: t.tsv,
                            html: t.html,
                        });
                    }
                    return Ok(true);
                }
                Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("v") => {
                    self.clipboard.push(ClipboardRequest::Paste);
                    return Ok(true);
                }
                Key::Escape if g.workflow.gesture.is_some() => {
                    self.grids.get_mut(&id).unwrap().workflow.gesture = None;
                    self.runtime.apply(Mutation::CapturePointer(None))?;
                    return Ok(true);
                }
                Key::ArrowRight
                    if g.source.parent_field.is_some() && g.relations.loader.is_some() =>
                {
                    if let Some(key) = g.selected_key {
                        self.load_grid_branch(id, key)?;
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(GridHitKind::Expander(row)) = g.frame.hit_test(*position)
            && g.source
                .lazy_loaded
                .as_ref()
                .is_some_and(|keys| !keys.contains(&data_key(row)))
        {
            self.load_grid_branch(id, data_key(row))?;
            return Ok(true);
        }
        if let InputEvent::PointerDown {
            position: p,
            button,
            modifiers,
        } = event
        {
            if *button == PointerButton::Secondary
                && let Some(GridHitKind::Header(col)) = g.frame.hit_test(*p)
            {
                return self.open_header_filter(id, col);
            }
            if *button == PointerButton::Primary {
                if let Some((col, _)) = g
                    .frame
                    .header_filters
                    .iter()
                    .find(|(_, r)| r.contains(p.x, p.y))
                {
                    return self.open_header_filter(id, *col);
                }
                let filter_field = g
                    .filter_cells(b)
                    .find(|(_, _, r)| r.contains(p.x, p.y))
                    .map(|(id, _, _)| g.source.fields[id.0 as usize].clone());
                if let Some(field) = filter_field {
                    self.open_grid_text_panel(id, TextTarget::Column(field))?;
                    return Ok(true);
                }
                if g.selector_width(b) > 0.
                    && p.x < b.x + g.selector_width(b)
                    && p.y >= g.selection_header(b).y
                {
                    if g.selection_header(b).contains(p.x, p.y) {
                        self.select_all_grid_records(id)?;
                    } else if let Some(row) = g.frame.semantics.rows.iter().find(|r| {
                        p.y >= r.bounds.y
                            && p.y < r.bounds.y + r.bounds.height
                            && !g.source.projection.synthetic.contains_key(&r.key)
                    }) {
                        let row = row.key;
                        let g = self.grids.get_mut(&id).unwrap();
                        let current =
                            (!g.model.is_selected(row) || modifiers.shift).then(|| data_key(row));
                        g.validate_selection(current)?;
                        g.model
                            .select(
                                &g.source,
                                row,
                                SelectionModifiers {
                                    control: true,
                                    shift: modifiers.shift,
                                },
                            )
                            .map_err(error)?;
                        g.set_current(current);
                    }
                    self.runtime.apply(Mutation::Focus(Some(id)))?;
                    self.scene_dirty = true;
                    return Ok(true);
                }
                if let Some(origin) = g.hit_cell(*p) {
                    let rows = if modifiers.alt {
                        Some(if g.model.is_selected(row_key(origin.0)) {
                            g.model
                                .selected()
                                .iter()
                                .filter(|r| g.source.positions.contains_key(&data_key(**r)))
                                .map(|r| data_key(*r))
                                .collect()
                        } else {
                            vec![origin.0]
                        })
                    } else {
                        None
                    };
                    if modifiers.alt
                        && !g.model.is_selected(row_key(origin.0))
                        && !g.range_contains(origin.0, &origin.1)
                    {
                        self.grid_focus_cell(id, origin.0, Some(&origin.1))?;
                    }
                    self.grids.get_mut(&id).unwrap().workflow.gesture = Some(Gesture {
                        origin,
                        point: *p,
                        append: modifiers.control,
                        external: modifiers.alt && modifiers.control,
                        rows,
                    });
                    if modifiers.alt {
                        self.runtime.apply(Mutation::Focus(Some(id)))?;
                        self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                        return Ok(true);
                    }
                }
            }
        }
        if let InputEvent::PointerMoved { position } = event
            && let Some(gesture) = &self.grids[&id].workflow.gesture
            && gesture.external
            && (gesture.point.x - position.x).abs() + (gesture.point.y - position.y).abs() > 4.
        {
            let g = self.grids.get_mut(&id).unwrap();
            g.workflow.external_requested = true;
            g.workflow.gesture = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
            return Ok(true);
        }
        if let InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(gesture) = self.grids.get_mut(&id).unwrap().workflow.gesture.take()
        {
            self.runtime.apply(Mutation::CapturePointer(None))?;
            if (position.x - gesture.point.x).abs() + (position.y - gesture.point.y).abs() > 4. {
                if let Some(keys) = gesture.rows {
                    if let Some(to) = self
                        .runtime
                        .hit_test(*position)
                        .filter(|t| self.grids.contains_key(t))
                    {
                        let target = &self.grids[&to];
                        let before = target.hit_cell(*position).map(|c| c.0);
                        let result =
                            if Rc::ptr_eq(&self.grids[&id].source.handle, &target.source.handle) {
                                if let Some(order) = target.workflow.drop_order.clone() {
                                    self.move_grid_rows(to, &keys, before, &order)
                                } else {
                                    Ok(false)
                                }
                            } else if target.workflow.allow_copy {
                                self.copy_grid_rows_to(id, to, &keys)
                            } else {
                                Ok(false)
                            };
                        if let Err(err) = result {
                            self.grids.get_mut(&id).unwrap().error = Some(err.to_string());
                        }
                    }
                } else if let Some(to) = self.grids[&id].hit_cell(*position) {
                    self.select_grid_range(
                        id,
                        (gesture.origin.0, &gesture.origin.1),
                        (to.0, &to.1),
                        gesture.append,
                    )?;
                }
                self.scene_dirty = true;
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn select_all_grid_records(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        g.model.select_all(&g.source).map_err(error)?;
        g.transfer.clear_ranges();
        self.scene_dirty = true;
        Ok(())
    }
    fn grid_panel_input(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
        b: Rect,
    ) -> Result<bool, ControlError> {
        let g = self.grids.get_mut(&id).unwrap();
        let action = match g.workflow.panel.as_ref().unwrap() {
            Panel::Facet(popup) => {
                let anchor = g.filter_anchor(b, &popup.field);
                let Some(Panel::Facet(popup)) = &mut g.workflow.panel else {
                    unreachable!()
                };
                popup.input(event, b, anchor)
            }
            Panel::Text { .. } => match event {
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                }
                | InputEvent::PointerDown { .. }
                | InputEvent::WindowFocused(false) => Action::Close,
                InputEvent::KeyDown {
                    key: Key::Enter, ..
                } => Action::Apply,
                InputEvent::KeyDown {
                    key: Key::Backspace,
                    ..
                } => {
                    if let Some(Panel::Text { draft, .. }) = &mut g.workflow.panel {
                        draft.pop();
                    }
                    Action::Stay
                }
                _ => Action::Stay,
            },
        };
        let clear = matches!(action, Action::Clear);
        if matches!(action, Action::Close) {
            g.workflow.panel = None;
        }
        if matches!(action, Action::Clear | Action::Apply)
            && let Some(mut panel) = g.workflow.panel.take()
        {
            let result = match &panel {
                Panel::Text {
                    target: TextTarget::Find,
                    draft,
                } => self
                    .set_grid_search(id, draft)
                    .and_then(|()| self.find_grid_match(id, true, false).map(|_| ())),
                Panel::Text {
                    target: TextTarget::Column(f),
                    draft,
                } => self.set_grid_auto_filter(id, f, draft),
                Panel::Text {
                    target: TextTarget::Criteria,
                    draft,
                } => self.set_grid_criteria(id, draft),
                Panel::Facet(popup) => self.apply_header_filter(id, popup, clear),
            };
            if let Err(err) = result {
                let g = self.grids.get_mut(&id).unwrap();
                if let Panel::Facet(popup) = &mut panel {
                    popup.error = Some(err.to_string());
                } else {
                    g.error = Some(err.to_string());
                }
                g.workflow.panel = Some(panel);
            }
        }
        self.scene_dirty = true;
        Ok(true)
    }
}
