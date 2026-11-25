//! Application context commands, measured rows, pinned records and scrollbar markers.
use super::*;
use rust_desktop_ui_treegrid::GridHitKind;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridContextTarget {
    Column(String),
    Group(RowKey),
    Record(i64),
}
#[derive(Default)]
pub(super) struct Customization {
    pub(super) menus: [Vec<MenuItem>; 3],
    context: Option<GridContextTarget>,
    pins: Vec<i64>,
    marks: Vec<i64>,
    matches: Vec<i64>,
    last_query: Option<(String, (u64, u64, u64))>,
}
impl GridState {
    pub(super) fn pinned_height(&self) -> f32 {
        self.custom
            .pins
            .iter()
            .filter(|k| self.source.positions.contains_key(k))
            .count() as f32
            * 24.
    }
    fn pin_regions(&self, b: Rect) -> Vec<(i64, Rect)> {
        let top = b.y + b.height
            - self.source.presentation.footer_lines() as f32 * 28.
            - self.pinned_height();
        self.custom
            .pins
            .iter()
            .filter(|k| self.source.positions.contains_key(k))
            .enumerate()
            .filter_map(|(i, k)| {
                Some((
                    *k,
                    Rect::new(b.x, top + i as f32 * 24., b.width, 24.).intersection(b)?,
                ))
            })
            .collect()
    }
    pub(super) fn update_search_marks(&mut self) {
        if self.search.is_empty() {
            self.custom.matches.clear();
            self.custom.last_query = None;
            return;
        }
        let stamp = self.model.structure_revision();
        if self
            .custom
            .last_query
            .as_ref()
            .is_some_and(|(q, s)| q == &self.search && *s == stamp)
        {
            return;
        }
        self.custom.matches = self
            .source
            .keys
            .iter()
            .copied()
            .filter(|key| {
                self.model.columns().iter().any(|c| {
                    self.source
                        .cell(row_key(*key), c.id)
                        .to_lowercase()
                        .contains(&self.search)
                })
            })
            .take(256)
            .collect();
        self.custom.last_query = Some((self.search.clone(), stamp));
    }
    fn marker_regions(&self) -> Vec<(i64, Rect)> {
        if self.custom.pins.is_empty()
            && self.custom.marks.is_empty()
            && self.custom.matches.is_empty()
            && self.records.issue.is_none()
            && self.selected_key.is_none()
        {
            return vec![];
        }
        let Some(bar) = self.scroll_bars.iter().find(|b| !b.horizontal) else {
            return Vec::new();
        };
        let mut keys = self.custom.marks.clone();
        keys.extend(&self.custom.matches);
        if self.records.issue.is_some()
            && let Some(d) = &self.records.draft
        {
            keys.push(d.row.key)
        }
        if let Some(key) = self.selected_key {
            keys.push(key)
        }
        keys.into_iter()
            .filter_map(|key| {
                let i = self.model.visible_index(&self.source, row_key(key))?;
                let t = self.model.record_top(i) / self.model.content_height().max(1.);
                Some((
                    key,
                    Rect::new(
                        bar.track.x,
                        bar.track.y + (bar.track.height - 3.).max(0.) * t as f32,
                        bar.track.width,
                        3.,
                    ),
                ))
            })
            .collect()
    }
    pub(super) fn paint_navigation(&self, b: Rect, scene: &mut Scene) {
        let palette = self.model.palette;
        for (key, rect) in self.pin_regions(b) {
            scene.fill(rect, palette.header);
            let fields = self
                .model
                .columns()
                .iter()
                .take(4)
                .map(|c| self.source.cell(row_key(key), c.id))
                .collect::<Vec<_>>()
                .join(" · ");
            views::draw_text(
                scene,
                rect,
                rect,
                format!("  ◆  {fields}"),
                palette.text,
                600,
                13.,
            );
        }
        for (_, r) in self.marker_regions() {
            scene.fill(r, palette.focus)
        }
    }
    pub(super) fn measure_visible_rows(
        &mut self,
        frame: &GridFrame,
        engine: &mut TextEngine,
    ) -> Result<bool, ControlError> {
        let options = &self.source.presentation;
        if options.view != GridViewMode::Table
            || (options.wrap_fields.is_empty() && options.stacked_fields.is_empty())
        {
            return Ok(false);
        }
        let anchor = self.model.capture_scroll_anchor(&self.source);
        let mut changed = false;
        for row in &frame.semantics.rows {
            if let Some(synthetic) = self.source.projection.synthetic.get(&row.key) {
                if matches!(
                    synthetic.identity,
                    projection::SyntheticIdentity::Group(_)
                        | projection::SyntheticIdentity::Branch(_, _)
                        | projection::SyntheticIdentity::Footer(..)
                ) {
                    let height = self.model.config().row_height.min(28.);
                    changed |= self.model.measure_row(row.index, height).map_err(error)?;
                }
                continue;
            }
            let mut height = self.model.config().row_height;
            for col in self.model.columns().iter().filter(|c| {
                options
                    .wrap_fields
                    .contains(&self.source.fields[c.id.0 as usize])
            }) {
                let width = (col.width - 14.).max(1.);
                let value = self.source.cell(row.key, col.id);
                let mut run = TextRun::new(
                    value,
                    Rect::new(0., 0., width, 0.),
                    self.model.palette.text,
                    self.model.config().font_size,
                );
                run.wrap = true;
                height = height.max(
                    (engine.prepare(&run, 1.).map_err(error)?.height + 8.)
                        .ceil()
                        .min(4096.),
                );
            }
            changed |= self.model.measure_row(row.index, height).map_err(error)?;
        }
        if changed {
            self.model
                .restore_scroll_anchor(&self.source, anchor)
                .map_err(error)?
        }
        Ok(changed)
    }
}
pub(super) fn decorate_cells(
    s: &SnapshotSource,
    frame: &mut GridFrame,
    runs: &HashMap<[u32; 4], usize>,
) {
    let p = &s.presentation;
    if p.view != GridViewMode::Table || (p.wrap_fields.is_empty() && p.merge_fields.is_empty()) {
        return;
    }
    let mut previous = HashMap::new();
    for row in &frame.semantics.rows {
        if s.projection.synthetic.contains_key(&row.key) {
            previous.clear();
            continue;
        }
        for cell in &row.cells {
            let field = &s.fields[cell.column.0 as usize];
            let r = cell.bounds;
            let Some(&index) = runs.get(&views::clip_key(r)) else {
                continue;
            };
            let run = &mut frame.scene.texts[index];
            if run.text != cell.label {
                continue;
            }
            if p.wrap_fields.contains(field) {
                run.wrap = true;
            }
            if p.merge_fields.contains(field) {
                let typed = s.table.value(data_key(row.key), field);
                let repeat = previous.insert(
                    cell.column,
                    (row.index, r.y + r.height, typed, row.selected),
                );
                if repeat.is_some_and(|(i, bottom, val, selected)| {
                    i + 1 == row.index
                        && (r.y - bottom).abs() < 0.01
                        && val == typed
                        && selected == row.selected
                }) && !cell.editing
                {
                    run.text.clear();
                    // Keep separate semantic cells and full logical hit targets.
                    // The shared edge is covered inside the column, leaving its dividers.
                    if r.width > 2. && r.height > 1. {
                        let color = frame
                            .scene
                            .rectangles
                            .iter()
                            .rev()
                            .find(|rect| {
                                rect.bounds.contains(r.x + 1., r.y + 1.) && rect.bounds.height > 2.
                            })
                            .map(|rect| rect.color);
                        if let Some(color) = color {
                            frame
                                .scene
                                .fill(Rect::new(r.x + 1., r.y - 1., r.width - 2., 1.), color)
                        }
                    }
                }
            }
        }
    }
}
impl Controls {
    pub fn set_grid_context_menus(
        &mut self,
        id: WidgetId,
        column: Vec<MenuItem>,
        group: Vec<MenuItem>,
        row: Vec<MenuItem>,
    ) -> Result<(), ControlError> {
        for menu in [&column, &group, &row] {
            if menu.len() > 64
                || menu
                    .iter()
                    .any(|i| self.commands.label(&i.command).is_none())
            {
                return Err(error("Invalid Grid context menu command"));
            }
        }
        self.grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?
            .custom
            .menus = [column, group, row];
        Ok(())
    }
    pub fn grid_context_target(&self, id: WidgetId) -> Option<&GridContextTarget> {
        self.grids.get(&id)?.custom.context.as_ref()
    }
    pub fn open_grid_context(
        &mut self,
        id: WidgetId,
        target: GridContextTarget,
    ) -> Result<bool, ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let index = match &target {
            GridContextTarget::Column(f) if g.source.fields.contains(f) => 0,
            GridContextTarget::Group(r)
                if matches!(g.source.row_kind(*r), Some(GridRowKind::Group { .. })) =>
            {
                1
            }
            GridContextTarget::Record(k) if g.source.positions.contains_key(k) => 2,
            _ => return Err(error("Unknown grid context target")),
        };
        let entries = g.custom.menus[index].clone();
        if entries.is_empty() {
            return Ok(false);
        }
        self.popup_menu(id, &entries)?;
        self.grids.get_mut(&id).unwrap().custom.context = Some(target);
        Ok(true)
    }
    pub(super) fn grid_custom_input(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let g = &self.grids[&id];
        let Some(b) = g.bounds else { return Ok(false) };
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some((key, _)) = g
                .pin_regions(b)
                .into_iter()
                .chain(g.marker_regions())
                .find(|(_, r)| r.contains(position.x, position.y))
        {
            self.grid_focus_cell(id, key, None)?;
            return Ok(true);
        }
        let target = match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Secondary,
                modifiers,
            } if !modifiers.alt => match g.frame.hit_test(*position) {
                Some(GridHitKind::Header(c)) => Some(GridContextTarget::Column(
                    g.source.fields[c.0 as usize].clone(),
                )),
                Some(GridHitKind::Cell { row, .. } | GridHitKind::Expander(row)) => {
                    target_for_row(g, row)
                }
                _ => None,
            },
            InputEvent::KeyDown {
                key: Key::F10,
                modifiers,
                ..
            } if modifiers.shift => {
                if modifiers.control {
                    g.model
                        .focused_column()
                        .map(|c| GridContextTarget::Column(g.source.fields[c.0 as usize].clone()))
                } else {
                    g.model.focused().and_then(|r| target_for_row(g, r))
                }
            }
            _ => None,
        };
        if let Some(target) = target {
            self.open_grid_context(id, target)
        } else {
            Ok(false)
        }
    }
    pub fn set_grid_navigation_rows(
        &mut self,
        id: WidgetId,
        pinned: Vec<i64>,
        markers: Vec<i64>,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        if pinned.len() > 8
            || markers.len() > 256
            || [&pinned, &markers].iter().any(|v| {
                let mut seen = HashSet::new();
                v.iter()
                    .any(|k| g.source.table.row(*k).is_none() || !seen.insert(*k))
            })
        {
            return Err(error("Invalid pinned rows or markers"));
        }
        g.custom.pins = pinned;
        g.custom.marks = markers;
        self.scene_dirty = true;
        Ok(())
    }
}
fn target_for_row(g: &GridState, row: RowKey) -> Option<GridContextTarget> {
    match g.source.row_kind(row)? {
        GridRowKind::Group { .. } => Some(GridContextTarget::Group(row)),
        GridRowKind::Record(k) => Some(GridContextTarget::Record(k)),
        _ => None,
    }
}
