//! Table, band and vertical-card presentation using shared TreeGrid identities.
use super::*;
use rust_desktop_ui_treegrid::{GridConfig, GridHit, GridHitKind};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridCellTemplate {
    pub prefix: String,
    pub suffix: String,
    pub icon: Option<IconSymbol>,
    pub emphasis: bool,
}

impl GridState {
    pub(super) fn make_model(
        &self,
        source: &mut SnapshotSource,
        arrangement: &[GridColumnArrangement],
    ) -> Result<TreeGrid, ControlError> {
        let columns: Vec<_> = arrangement
            .iter()
            .filter(|a| a.visible)
            .map(|a| {
                let index = self
                    .settings
                    .columns
                    .iter()
                    .position(|c| c.field == a.field)
                    .expect("validated arrangement");
                let c = &self.settings.columns[index];
                GridColumn {
                    id: ColumnId(index as u64),
                    title: c.title.clone(),
                    width: a.width,
                    min_width: c.min_width,
                    max_width: c.max_width,
                    pinned: a.pinned,
                    editable: !self.settings.is_read_only && !c.read_only,
                }
            })
            .collect();
        let config = if source.presentation.view == GridViewMode::Cards {
            GridConfig {
                header_height: 0.0,
                row_height: 12.0 + columns.len() as f32 * 28.0_f32.max(self.editor_height),
                ..Default::default()
            }
        } else {
            GridConfig {
                row_height: 28.0_f32.max(self.editor_height),
                ..Default::default()
            }
        };
        let mut model = TreeGrid::with_config(columns, config).map_err(error)?;
        model.flat_presentation =
            source.presentation.group_fields.is_empty() && source.detail.is_none();
        model.refresh(source).map_err(error)?;
        let expanded: Vec<_> = source
            .projection
            .children
            .keys()
            .copied()
            .filter(|key| {
                if let Some(row) = source.projection.synthetic.get(key) {
                    self.source
                        .projection
                        .identities
                        .get(&row.identity)
                        .map_or(source.projection.groups.contains(key), |old| {
                            self.model.is_expanded(*old)
                        })
                } else {
                    self.model.is_expanded(*key)
                }
            })
            .collect();
        model.set_known_expanded(source, &expanded).map_err(error)?;
        model.set_query_presentation(source.query());
        let bounds = self.bounds.unwrap_or(Rect::new(0.0, 0.0, 480.0, 320.0));
        model
            .set_viewport(Size::new(bounds.width, bounds.height))
            .map_err(error)?;
        if let Some(key) = self
            .selected_key
            .filter(|k| source.position(row_key(*k)).is_some())
        {
            model
                .assign_selected_key(source, Some(row_key(key)))
                .map_err(error)?;
            if let Some(column) = self
                .model
                .focused_column()
                .filter(|id| model.columns().iter().any(|c| c.id == *id))
                && model.visible_index(source, row_key(key)).is_some()
            {
                model
                    .focus_cell(source, row_key(key), column, SelectionModifiers::default())
                    .map_err(error)?;
            }
        }
        let mut anchor = self.model.capture_scroll_anchor(&self.source);
        if let Some(key) = anchor.key
            && let Some(old) = self.source.projection.synthetic.get(&key)
        {
            anchor.key = source.projection.identities.get(&old.identity).copied();
        }
        anchor.within_row = anchor.within_row.min(f64::from(config.row_height) - 0.001);
        model.restore_scroll_anchor(source, anchor).map_err(error)?;
        Ok(model)
    }
    pub(super) fn current_arrangement(&self) -> Vec<GridColumnArrangement> {
        let result: Vec<_> = self
            .model
            .columns()
            .iter()
            .map(|c| GridColumnArrangement {
                field: self.settings.columns[c.id.0 as usize].field.clone(),
                visible: true,
                width: c.width,
                pinned: c.pinned,
            })
            .collect();
        let mut visible = result.into_iter();
        self.arrangement
            .iter()
            .map(|entry| {
                if entry.visible {
                    visible.next().expect("known visible column")
                } else {
                    entry.clone()
                }
            })
            .collect()
    }
    fn validate_arrangement(
        &self,
        arrangement: &[GridColumnArrangement],
    ) -> Result<(), ControlError> {
        if arrangement.len() != self.settings.columns.len()
            || !arrangement.iter().any(|a| a.visible)
        {
            return Err(error(
                "Grid arrangement must name every column and keep at least one visible",
            ));
        }
        let mut seen = HashSet::new();
        for entry in arrangement {
            let c = self
                .settings
                .columns
                .iter()
                .find(|c| c.field == entry.field)
                .ok_or_else(|| error("Unknown arranged field"))?;
            if !seen.insert(&entry.field)
                || !entry.width.is_finite()
                || entry.width < c.min_width
                || entry.width > c.max_width
            {
                return Err(error("Duplicate field or invalid arrangement width"));
            }
        }
        Ok(())
    }
    pub(super) fn presentation_bounds(&self, bounds: Rect) -> Rect {
        let band = if self.source.presentation.bands.is_empty()
            || self.source.presentation.view == GridViewMode::Cards
        {
            0.0
        } else {
            24.0_f32.min(bounds.height)
        };
        let footer = if self.source.presentation.summaries.is_empty() {
            0.0
        } else {
            28.0_f32.min((bounds.height - band).max(0.0))
        };
        Rect::new(
            bounds.x,
            bounds.y + band,
            bounds.width,
            (bounds.height - band - footer).max(0.0),
        )
    }
    pub(super) fn decorate_frame(&self, bounds: Rect, frame: &mut GridFrame) {
        self.painter().decorate_frame(bounds, frame);
    }
    pub(super) fn decorate_outer(&self, bounds: Rect, scene: &mut Scene) {
        self.painter().decorate_outer(bounds, &self.frame, scene);
    }
    fn painter(&self) -> PresentationPainter<'_> {
        PresentationPainter {
            settings: &self.settings,
            source: &self.source,
            model: &self.model,
            cell_templates: &self.cell_templates,
            editor_height: self.editor_height,
        }
    }
}
struct PresentationPainter<'a> {
    settings: &'a GridControlSettings,
    source: &'a SnapshotSource,
    model: &'a TreeGrid,
    cell_templates: &'a HashMap<ColumnId, GridCellTemplate>,
    editor_height: f32,
}
impl PresentationPainter<'_> {
    pub(super) fn decorate_frame(&self, bounds: Rect, frame: &mut GridFrame) {
        let options = &self.source.presentation;
        let palette = self.model.palette;
        if options.view == GridViewMode::Cards {
            frame.scene.clear();
            frame.scene.fill(bounds, palette.background);
            frame.hits.clear();
            let columns = self.model.columns();
            frame.columns = columns
                .iter()
                .enumerate()
                .map(|(index, c)| rust_desktop_ui_treegrid::VisibleColumn {
                    id: c.id,
                    index,
                    bounds,
                    clip: bounds,
                    pinned: false,
                })
                .collect();
            let line_height = 28.0_f32.max(self.editor_height);
            frame.stats.visible_columns = columns.len();
            for row in &mut frame.semantics.rows {
                let card = Rect::new(
                    bounds.x + 4.0,
                    bounds.y
                        + (row.index as f64 * f64::from(self.model.config().row_height)
                            - self.model.scroll_offset().1) as f32
                        + 3.0,
                    (bounds.width - 8.0).max(0.0),
                    (self.model.config().row_height - 6.0).max(0.0),
                );
                let clip = card
                    .intersection(bounds)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                frame.scene.rounded_fill(
                    card,
                    clip,
                    6.0,
                    if row.selected {
                        palette.selection
                    } else {
                        palette.alternate_row
                    },
                );
                let mut cells = Vec::new();
                for (index, column) in columns.iter().enumerate() {
                    let rect = Rect::new(
                        card.x + 8.0,
                        card.y + 3.0 + index as f32 * line_height,
                        (card.width - 16.0).max(0.0),
                        line_height,
                    );
                    let visible = rect
                        .intersection(bounds)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                    let value = self.source.cell(row.key, column.id);
                    let text = format!("{}: {}", column.title, value);
                    draw_text(
                        &mut frame.scene,
                        rect,
                        visible,
                        text,
                        palette.text,
                        400,
                        14.0,
                    );
                    if visible.width > 0.0 && visible.height > 0.0 {
                        frame.hits.push(GridHit {
                            bounds: visible,
                            kind: GridHitKind::Cell {
                                row: row.key,
                                column: column.id,
                            },
                        });
                    }
                    cells.push(rust_desktop_ui_treegrid::GridSemanticCell {
                        column: column.id,
                        label: value,
                        bounds: visible,
                        editable: column.editable && self.source.editable(row.key, column.id),
                        editing: self
                            .model
                            .editor()
                            .is_some_and(|e| e.row == row.key && e.column == column.id),
                        validation_error: None,
                    });
                }
                row.cells = cells;
                if row.expanded.is_some() {
                    let expander = Rect::new(card.x + card.width - 24.0, card.y, 24.0, 24.0)
                        .intersection(bounds)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                    frame.hits.push(GridHit {
                        bounds: expander,
                        kind: GridHitKind::Expander(row.key),
                    });
                    draw_text(
                        &mut frame.scene,
                        expander,
                        expander,
                        if row.expanded == Some(true) {
                            "−"
                        } else {
                            "+"
                        }
                        .into(),
                        palette.text,
                        600,
                        16.0,
                    );
                }
            }
        }
        for row in &frame.semantics.rows {
            if self.source.projection.synthetic.contains_key(&row.key) {
                continue;
            }
            for cell in &row.cells {
                let Some(template) = self.cell_templates.get(&cell.column) else {
                    continue;
                };
                // Replace only the original cell text, retaining its painter
                // order and the row's zebra/selection/focus/grid-line geometry.
                // Painting an opaque cell rectangle here would cover all of it.
                let rect = cell.bounds;
                let Some(run) = frame.scene.texts.iter_mut().find(|run| {
                    run.clip == rect
                        && (run.text == cell.label || options.view == GridViewMode::Cards)
                }) else {
                    continue;
                };
                let icon_space = if template.icon.is_some() { 22.0 } else { 0.0 };
                let icon_bounds = Rect::new(
                    run.bounds.x,
                    run.bounds.y + (run.bounds.height - 16.0) * 0.5,
                    16.0,
                    16.0,
                );
                run.bounds.x += icon_space;
                run.bounds.width = (run.bounds.width - icon_space).max(0.0);
                let value = format!("{}{}{}", template.prefix, cell.label, template.suffix);
                run.text = if options.view == GridViewMode::Cards {
                    format!(
                        "{}: {value}",
                        self.settings.columns[cell.column.0 as usize].title
                    )
                } else {
                    value
                };
                run.weight = if template.emphasis { 600 } else { 400 };
                if let Some(icon) = template.icon {
                    crate::primitives::paint_icon(
                        &mut frame.scene,
                        icon,
                        icon_bounds,
                        rect,
                        if row.selected {
                            palette.selection_text
                        } else {
                            palette.text
                        },
                    );
                }
            }
        }
        for row in &mut frame.semantics.rows {
            if let Some(synthetic) = self.source.projection.synthetic.get(&row.key)
                && matches!(synthetic.identity, projection::SyntheticIdentity::Group(_))
            {
                let rect = row
                    .bounds
                    .intersection(bounds)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                frame.scene.fill(rect, palette.header);
                // This full-width header replaces the base row's disclosure
                // geometry, including when its original tree column is hidden.
                frame
                    .hits
                    .retain(|h| !matches!(h.kind, GridHitKind::Expander(key) if key == row.key));
                let disclosure = Rect::new(rect.x, rect.y, 26.0_f32.min(rect.width), rect.height);
                frame.hits.push(GridHit {
                    bounds: disclosure,
                    kind: GridHitKind::Expander(row.key),
                });
                let row_y = bounds.y
                    + self.model.config().header_height
                    + (row.index as f64 * f64::from(self.model.config().row_height)
                        - self.model.scroll_offset().1) as f32;
                let icon = Rect::new(
                    rect.x + 5.0,
                    row_y + (self.model.config().row_height - 16.0) * 0.5,
                    16.0,
                    16.0,
                );
                crate::primitives::paint_icon(
                    &mut frame.scene,
                    if row.expanded == Some(true) {
                        IconSymbol::ChevronDown
                    } else {
                        IconSymbol::ChevronRight
                    },
                    icon,
                    rect,
                    palette.text,
                );
                draw_text(
                    &mut frame.scene,
                    Rect::new(
                        rect.x + 26.0,
                        row_y,
                        (rect.width - 32.0).max(0.0),
                        self.model.config().row_height,
                    ),
                    rect,
                    synthetic.label.clone(),
                    palette.text,
                    600,
                    14.0,
                );
                for (index, cell) in row.cells.iter_mut().enumerate() {
                    cell.label = if index == 0 {
                        synthetic.label.clone()
                    } else {
                        String::new()
                    };
                    cell.editable = false;
                }
            }
        }
    }
    fn decorate_outer(&self, bounds: Rect, frame: &GridFrame, scene: &mut Scene) {
        let palette = self.model.palette;
        if self.source.presentation.view == GridViewMode::Table {
            for band in &self.source.presentation.bands {
                // Use clipped visible column geometry, merging only adjacent
                // fragments in the same pinned/scrolling coordinate region.
                let mut columns: Vec<_> = frame
                    .columns
                    .iter()
                    .filter(|column| {
                        band.fields
                            .contains(&self.settings.columns[column.id.0 as usize].field)
                    })
                    .collect();
                columns.sort_by(|a, b| a.clip.x.total_cmp(&b.clip.x));
                let mut fragments: Vec<(Rect, bool)> = Vec::new();
                for column in columns {
                    let Some(visible) = column.bounds.intersection(column.clip) else {
                        continue;
                    };
                    if visible.width <= 0.0 {
                        continue;
                    }
                    let rect = Rect::new(
                        visible.x,
                        bounds.y,
                        visible.width,
                        24.0_f32.min(bounds.height),
                    );
                    if let Some((last, pinned)) = fragments.last_mut()
                        && *pinned == column.pinned
                        && (last.x + last.width - rect.x).abs() < 0.5
                    {
                        last.width = rect.x + rect.width - last.x;
                    } else {
                        fragments.push((rect, column.pinned));
                    }
                }
                for (rect, _) in fragments {
                    scene.fill(rect, palette.header);
                    let text = Rect::new(
                        rect.x + 6.0,
                        rect.y,
                        (rect.width - 12.0).max(0.0),
                        rect.height,
                    );
                    draw_text(
                        scene,
                        text,
                        rect,
                        band.title.clone(),
                        palette.text,
                        600,
                        13.0,
                    );
                }
            }
        }
        if !self.source.presentation.summaries.is_empty() {
            let height = 28.0_f32.min(bounds.height);
            let rect = Rect::new(
                bounds.x,
                bounds.y + bounds.height - height,
                bounds.width,
                height,
            );
            scene.fill(rect, palette.header);
            let text = self
                .source
                .presentation
                .summaries
                .iter()
                .zip(&self.source.projection.totals)
                .map(|(s, v)| format!("{}: {}", s.title, v.display()))
                .collect::<Vec<_>>()
                .join("  ·  ");
            draw_text(scene, rect, rect, text, palette.text, 600, 13.0);
        }
    }
}
fn draw_text(
    scene: &mut Scene,
    bounds: Rect,
    clip: Rect,
    text: String,
    color: Color,
    weight: u16,
    size: f32,
) {
    scene.text(TextRun {
        sensitive: false,
        bounds,
        clip,
        text,
        color,
        weight,
        font_size: size,
        align: TextAlign::Start,
        wrap: false,
    });
}

/// Every row/header/card cell reserves an area rather than an intrinsic text
/// line. Resolve its vertical alignment from shaping, without rasterizing a
/// second copy or changing the original clip and painter-order references.
pub(super) fn center_scene_text(
    scene: &mut Scene,
    engine: &mut TextEngine,
) -> Result<(), ControlError> {
    for run in &mut scene.texts {
        let mut measure = run.clone();
        measure.bounds = Rect::new(0.0, 0.0, run.bounds.width, 0.0);
        measure.clip = Rect::new(0.0, 0.0, 0.0, 0.0);
        let height = engine.prepare(&measure, 1.0).map_err(error)?.height;
        run.bounds.y += (run.bounds.height - height).max(0.0) * 0.5;
        run.bounds.height = height;
    }
    Ok(())
}

impl Controls {
    pub fn grid_presentation(&self, id: WidgetId) -> Option<&GridPresentationSettings> {
        self.grids.get(&id).map(|g| &g.source.presentation)
    }
    pub fn grid_row_kind(&self, id: WidgetId, row: RowKey) -> Option<GridRowKind> {
        self.grids.get(&id)?.source.row_kind(row)
    }
    /// Whether virtual row identities still refer to the current accepted
    /// snapshots. This is a read-only check with no provider I/O.
    pub fn grid_snapshot_is_current(&self, id: WidgetId) -> Result<bool, ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let source = grid
            .source
            .handle
            .try_borrow()
            .map_err(|_| error("Grid source is already borrowed"))?;
        Ok(source.revision() == grid.source.revision && !grid.source.detail_changed()?)
    }
    pub fn grid_summaries(&self, id: WidgetId) -> Option<&[CellValue]> {
        self.grids
            .get(&id)
            .map(|g| g.source.projection.totals.as_slice())
    }
    pub fn grid_arrangement(&self, id: WidgetId) -> Option<Vec<GridColumnArrangement>> {
        let grid = self.grids.get(&id)?;
        Some(grid.current_arrangement())
    }
    pub fn set_grid_presentation(
        &mut self,
        id: WidgetId,
        settings: GridPresentationSettings,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        settings.validate(&grid.settings.columns)?;
        let mut source = grid.source.clone();
        source.presentation = settings;
        self.install_grid_presentation(id, source, self.grid_arrangement(id).unwrap())
    }
    pub fn set_grid_detail(
        &mut self,
        id: WidgetId,
        detail: Option<GridDetailSettings>,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let mut source = grid.source.clone();
        source.detail = detail;
        self.install_grid_presentation(id, source, self.grid_arrangement(id).unwrap())
    }
    pub fn arrange_grid_columns(
        &mut self,
        id: WidgetId,
        arrangement: Vec<GridColumnArrangement>,
    ) -> Result<(), ControlError> {
        let source = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?
            .source
            .clone();
        self.install_grid_presentation(id, source, arrangement)
    }
    pub(super) fn install_grid_presentation(
        &mut self,
        id: WidgetId,
        mut source: SnapshotSource,
        arrangement: Vec<GridColumnArrangement>,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error(
                "Finish the active cell edit before changing grid presentation",
            ));
        }
        grid.validate_arrangement(&arrangement)?;
        source.reload()?;
        let mut model = grid.make_model(&mut source, &arrangement)?;
        let bounds = grid.bounds.unwrap_or(Rect::new(0.0, 0.0, 480.0, 320.0));
        let mut frame = model.paint(&source, bounds).map_err(error)?;
        let painter = PresentationPainter {
            settings: &grid.settings,
            source: &source,
            model: &model,
            cell_templates: &grid.cell_templates,
            editor_height: grid.editor_height,
        };
        painter.decorate_frame(bounds, &mut frame);
        let mut scene = frame.scene.clone();
        painter.decorate_outer(bounds, &frame, &mut scene);
        // Validate the final decorated runs and use their real shaped line
        // height, including emphasized labels, before publishing the model.
        center_scene_text(&mut scene, &mut self.engine)?;
        for band in &source.presentation.bands {
            self.measure_text(&band.title, 4096.0, false)?;
        }
        for summary in &source.presentation.summaries {
            self.measure_text(&summary.title, 4096.0, false)?;
        }
        let grid = self.grids.get_mut(&id).unwrap();
        grid.source = source;
        grid.model = model;
        grid.arrangement = arrangement;
        grid.cancel_pointer();
        grid.error = None;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_grid_expanded(
        &mut self,
        id: WidgetId,
        row: RowKey,
        expanded: bool,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error(
                "Finish the active cell edit before changing expansion",
            ));
        }
        if expanded && !grid.model.is_expanded(row) && grid.model.expanded_count() >= 4096 {
            return Err(error(
                "Grid presentation supports at most 4096 expanded rows",
            ));
        }
        grid.model
            .set_expanded(&mut grid.source, row, expanded)
            .map_err(error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_grid_cell_template(
        &mut self,
        id: WidgetId,
        field: &str,
        template: Option<GridCellTemplate>,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error(
                "Finish the active cell edit before changing templates",
            ));
        }
        let column = ColumnId(
            grid.settings
                .columns
                .iter()
                .position(|c| c.field == field)
                .ok_or_else(|| error("Unknown grid field"))? as u64,
        );
        if let Some(t) = &template {
            if t.prefix.len() > 256 || t.suffix.len() > 256 {
                return Err(error(
                    "Cell template prefix/suffix are limited to 256 bytes",
                ));
            }
            self.measure_text(&t.prefix, 4096.0, false)?;
            self.measure_text(&t.suffix, 4096.0, false)?;
            let values: Vec<_> = self.grids[&id]
                .frame
                .semantics
                .rows
                .iter()
                .flat_map(|r| &r.cells)
                .filter(|c| c.column == column)
                .map(|c| {
                    let value = format!("{}{}{}", t.prefix, c.label, t.suffix);
                    if self.grids[&id].source.presentation.view == GridViewMode::Cards {
                        format!(
                            "{}: {value}",
                            self.grids[&id].settings.columns[column.0 as usize].title
                        )
                    } else {
                        value
                    }
                })
                .collect();
            for text in values {
                self.measure_text(&text, 4096.0, false)?;
            }
        }
        let grid = self.grids.get_mut(&id).unwrap();
        if let Some(t) = template {
            grid.cell_templates.insert(column, t);
        } else {
            grid.cell_templates.remove(&column);
        }
        self.scene_dirty = true;
        Ok(())
    }
    /// Focus a materialized presentation row. Group/detail focus never changes
    /// the selected master record key and never opens a cell editor.
    pub fn grid_focus_row(
        &mut self,
        id: WidgetId,
        row: RowKey,
        column: Option<ColumnId>,
    ) -> Result<(), ControlError> {
        self.sync_grids()?;
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error("Finish the active cell edit before changing focus"));
        }
        let column = column.unwrap_or(grid.model.tree_column());
        if let Some(GridRowKind::Record(key)) = grid.source.row_kind(row) {
            let field = grid
                .source
                .fields
                .get(column.0 as usize)
                .ok_or_else(|| error("Unknown grid column"))?
                .clone();
            return self.grid_focus_cell(id, key, Some(&field));
        }
        grid.model
            .focus_cell(&grid.source, row, column, SelectionModifiers::default())
            .map_err(error)?;
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        self.scene_dirty = true;
        Ok(())
    }
}
