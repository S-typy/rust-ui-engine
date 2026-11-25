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
                    title: source.presentation.role_text("Header", &c.title),
                    width: a.width,
                    min_width: c.min_width,
                    max_width: c.max_width,
                    pinned: a.pinned,
                    editable: !self.settings.is_read_only && !c.read_only,
                }
            })
            .collect();
        let options = &source.presentation;
        let column_id = |name: &str| {
            self.settings
                .columns
                .iter()
                .position(|c| c.field == name)
                .map(|i| ColumnId(i as u64))
        };
        let stacks: HashMap<_, _> = options
            .stacked_fields
            .iter()
            .map(|(f, r)| {
                (
                    column_id(f).expect("validated field"),
                    column_id(r).expect("validated root"),
                )
            })
            .collect();
        let right: HashSet<_> = options
            .right_pinned_fields
            .iter()
            .filter_map(|f| column_id(f))
            .collect();
        let mut track_sizes = HashMap::new();
        for col in &columns {
            let track = stacks.get(&col.id).copied().unwrap_or(col.id);
            *track_sizes
                .entry((
                    if right.contains(&col.id) {
                        2
                    } else if col.pinned {
                        0
                    } else {
                        1
                    },
                    track,
                ))
                .or_insert(0usize) += 1;
        }
        let lines = track_sizes.values().max().copied().unwrap_or(1) as f32;
        let config = if source.presentation.view == GridViewMode::Cards {
            GridConfig {
                header_height: 0.0,
                row_height: 12.0 + columns.len() as f32 * 28.0_f32.max(self.editor_height),
                ..Default::default()
            }
        } else {
            GridConfig {
                header_height: GridConfig::default().header_height * lines,
                row_height: 28.0_f32.max(self.editor_height) * lines,
                ..Default::default()
            }
        };
        let mut model = TreeGrid::with_config(columns, config).map_err(error)?;
        if options.view == GridViewMode::Table {
            model.set_column_tracks(stacks, right).map_err(error)?;
        }
        model.flat_presentation = source.presentation.group_fields.is_empty()
            && source.detail.is_none()
            && source.parent_field.is_none();
        model.refresh(source).map_err(error)?;
        let expanded: Vec<_> = source
            .projection
            .children
            .keys()
            .copied()
            .filter(|key| {
                if let Some(row) = source.projection.synthetic.get(key) {
                    self.source.projection.identities.get(&row.identity).map_or(
                        matches!(
                            row.identity,
                            projection::SyntheticIdentity::Group(_)
                                | projection::SyntheticIdentity::Branch(_, _)
                        ),
                        |old| self.model.is_expanded(*old),
                    )
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
        model.retain_selected_records(self.model.selected().iter().copied().filter(|r| {
            self.source.table.row(data_key(*r)).is_some()
                && source.table.row(data_key(*r)).is_some()
        }));
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
    pub(super) fn validate_arrangement(
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
        let panel = self.source.presentation.panel_height(bounds);
        let band = if self.source.presentation.bands.is_empty()
            || self.source.presentation.view == GridViewMode::Cards
        {
            0.0
        } else {
            (24.0 * (1 + self.source.presentation.band_rows.len()) as f32).min(bounds.height)
        };
        let footer = if self.source.presentation.summary_titles().next().is_none() {
            0.0
        } else {
            (28.0 * self.source.presentation.footer_lines() as f32)
                .min((bounds.height - panel - band).max(0.0))
        };
        Rect::new(
            bounds.x + self.selector_width(bounds),
            bounds.y + panel + band + self.filter_height(bounds),
            (bounds.width - self.selector_width(bounds)).max(0.0),
            (bounds.height
                - panel
                - band
                - footer
                - self.filter_height(bounds)
                - self.pinned_height())
            .max(0.0),
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
                let lane_width = bounds.width / self.model.record_lanes() as f32;
                let card = Rect::new(
                    bounds.x + (row.index % self.model.record_lanes()) as f32 * lane_width + 4.0,
                    bounds.y
                        + (self.model.record_top(row.index) - self.model.scroll_offset().1) as f32
                        + 3.0,
                    (lane_width - 8.0).max(0.0),
                    (self.model.config().row_height - 6.0).max(0.0),
                );
                let clip = card
                    .intersection(bounds)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                row.bounds = clip;
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
        // A template and its cell share the same clip, even in a stacked or frozen column.
        let runs: HashMap<_, _> = frame
            .scene
            .texts
            .iter()
            .enumerate()
            .map(|(index, run)| (clip_key(run.clip), index))
            .collect();
        customize::decorate_cells(self.source, frame, &runs);
        appearance::paint(self.source, frame);
        for row in &frame.semantics.rows {
            if self.source.projection.synthetic.contains_key(&row.key) {
                continue;
            }
            for cell in &row.cells {
                let rect = cell.bounds;
                let index = runs.get(&clip_key(rect)).copied();
                let template = self.cell_templates.get(&cell.column);
                let field = &self.source.fields[cell.column.0 as usize];
                if options.view == GridViewMode::Table
                    && !self.source.choice_labels.contains_key(field)
                {
                    use rust_desktop_ui_data::FieldType;
                    let kind = self
                        .source
                        .table
                        .schema()
                        .field(field)
                        .map(|schema| schema.kind);
                    match kind {
                        Some(FieldType::Integer | FieldType::Real | FieldType::Decimal) => {
                            if let Some(index) = index {
                                frame.scene.texts[index].align = TextAlign::End;
                            }
                        }
                        Some(FieldType::Bool)
                            if template.is_none()
                                && !cell.editing
                                && !options.merge_fields.contains(field) =>
                        {
                            let checked = match self.source.table.value(data_key(row.key), field) {
                                Some(CellValue::Bool(value)) => Some(*value),
                                Some(CellValue::Null) => None,
                                _ => continue,
                            };
                            if let Some(index) = index {
                                frame.scene.texts[index].text.clear();
                            }
                            // Null has no text run. Position every state using the same logical track.
                            if let Some(column) =
                                frame.columns.iter().find(|col| col.id == cell.column)
                            {
                                let y = bounds.y
                                    + self.model.config().header_height
                                    + (self.model.record_top(row.index)
                                        - self.model.scroll_offset().1)
                                        as f32;
                                let track = self.model.track_cell(
                                    cell.column,
                                    column.bounds.x,
                                    y,
                                    column.bounds.width,
                                    self.model.record_height(row.index),
                                );
                                paint_check(&mut frame.scene, track, rect, palette, checked);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                let Some(index) = index else { continue };
                let run = &mut frame.scene.texts[index];
                if options.view == GridViewMode::Table && run.text != cell.label {
                    continue; // A merged cell already hides its repeated value.
                }
                let Some(template) = template else { continue };
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
            let Some(synthetic) = self.source.projection.synthetic.get(&row.key) else {
                continue;
            };
            let row_y = bounds.y
                + self.model.config().header_height
                + (self.model.record_top(row.index) - self.model.scroll_offset().1) as f32;
            let row_rect = Rect::new(
                bounds.x,
                row_y,
                bounds.width,
                self.model.record_height(row.index),
            );
            if matches!(
                synthetic.identity,
                projection::SyntheticIdentity::Footer(..)
            ) {
                let Some(rect) = row.bounds.intersection(bounds) else {
                    continue;
                };
                frame.scene.fill(rect, palette.header);
                if options.view == GridViewMode::Table {
                    palette.paint_cell_edges(&mut frame.scene, row_rect, rect);
                }
                let area = Rect::new(rect.x + 6., rect.y, (rect.width - 12.).max(0.), rect.height);
                draw_text(
                    &mut frame.scene,
                    area,
                    rect,
                    synthetic.label.clone(),
                    palette.text,
                    600,
                    13.,
                );
                for (i, cell) in row.cells.iter_mut().enumerate() {
                    cell.label = if i == 0 {
                        synthetic.label.clone()
                    } else {
                        String::new()
                    };
                    cell.editable = false;
                }
                continue;
            }
            if matches!(
                synthetic.identity,
                projection::SyntheticIdentity::Group(_)
                    | projection::SyntheticIdentity::Branch(_, _)
            ) {
                let rect = row
                    .bounds
                    .intersection(bounds)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                frame.scene.fill(rect, palette.header);
                if options.view == GridViewMode::Table {
                    palette.paint_cell_edges(&mut frame.scene, row_rect, rect);
                }
                // This full-width header replaces the base row's disclosure
                // geometry, including when its original tree column is hidden.
                frame
                    .hits
                    .retain(|h| !matches!(h.kind, GridHitKind::Expander(key) if key == row.key));
                let indent = (row.depth as f32 * 16.0).min(rect.width);
                let disclosure = Rect::new(
                    rect.x + indent,
                    rect.y,
                    26.0_f32.min(rect.width - indent),
                    rect.height,
                );
                frame.hits.push(GridHit {
                    bounds: disclosure,
                    kind: GridHitKind::Expander(row.key),
                });
                let icon = Rect::new(
                    rect.x + indent + 5.0,
                    row_y + (row_rect.height - 16.0) * 0.5,
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
                let area = Rect::new(
                    rect.x + indent + 26.,
                    row_y,
                    (rect.width - indent - 34.).max(0.),
                    row_rect.height,
                );
                if area.width >= 440.
                    && let Some(&(end, start)) =
                        self.source.projection.group_label_parts.get(&row.key)
                {
                    let trailing_width = (area.width * 0.55).min(580.);
                    let caption = Rect::new(
                        area.x,
                        area.y,
                        area.width - trailing_width - 8.,
                        area.height,
                    );
                    let totals = Rect::new(
                        area.x + area.width - trailing_width,
                        area.y,
                        trailing_width,
                        area.height,
                    );
                    if let Some(clip) = caption.intersection(rect) {
                        draw_text(
                            &mut frame.scene,
                            caption,
                            clip,
                            synthetic.label[..end].into(),
                            palette.text,
                            600,
                            14.,
                        );
                    }
                    if let Some(clip) = totals.intersection(rect) {
                        let mut run =
                            TextRun::new(&synthetic.label[start..], totals, palette.muted, 13.);
                        run.clip = clip;
                        run.align = TextAlign::End;
                        frame.scene.text(run);
                    }
                } else {
                    draw_text(
                        &mut frame.scene,
                        area,
                        rect,
                        synthetic.label.clone(),
                        palette.text,
                        600,
                        14.,
                    );
                }
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
        let panel = self.source.presentation.panel_height(bounds);
        if panel > 0.0 {
            let area = Rect::new(bounds.x, bounds.y, bounds.width, panel);
            scene.fill(area, palette.header);
            let fields = &self.source.presentation.group_fields;
            if fields.is_empty() {
                draw_text(
                    scene,
                    area,
                    area,
                    "Перетащите заголовок для группировки".into(),
                    palette.text,
                    400,
                    13.0,
                );
            }
            for (index, field) in fields.iter().enumerate() {
                let chip = self.source.presentation.group_slot(bounds, index);
                let label = self
                    .settings
                    .columns
                    .iter()
                    .find(|c| &c.field == field)
                    .map_or(field.as_str(), |c| c.title.as_str());
                let face = Rect::new(
                    chip.x + 3.,
                    chip.y + 3.,
                    (chip.width - 6.).max(0.),
                    (chip.height - 6.).max(0.),
                );
                scene.rounded_fill(face, chip, 5., palette.background);
                scene.rounded_stroke(face, chip, 5., 1., palette.line);
                let caption = Rect::new(
                    face.x + 6.,
                    face.y + 2.,
                    (face.width - 28.).max(0.),
                    (face.height - 4.).max(0.),
                );
                draw_text(
                    scene,
                    caption,
                    caption,
                    label.to_owned(),
                    palette.text,
                    500,
                    13.,
                );
                let x = face.x + face.width - 12.;
                let y = face.y + face.height * 0.5;
                let sign = if self.source.group_descending(field) {
                    -1.
                } else {
                    1.
                };
                scene.stroke_path(
                    &[
                        rust_desktop_ui_core::StrokeSegment {
                            start: Point::new(x - 4., y + 2. * sign),
                            end: Point::new(x, y - 2. * sign),
                        },
                        rust_desktop_ui_core::StrokeSegment {
                            start: Point::new(x, y - 2. * sign),
                            end: Point::new(x + 4., y + 2. * sign),
                        },
                    ],
                    face,
                    1.4,
                    palette.text,
                );
            }
        }
        if self.source.presentation.view == GridViewMode::Table {
            let options = &self.source.presentation;
            let bands_top = bounds.y + panel;
            let height = (24. * (1 + options.band_rows.len()) as f32).min(bounds.height - panel);
            let band_area = Rect::new(bounds.x, bands_top, bounds.width, height);
            if !options.bands.is_empty() {
                scene.fill(band_area, palette.header);
            }
            for (level, band) in std::iter::once(&options.bands)
                .chain(&options.band_rows)
                .enumerate()
                .flat_map(|(level, row)| row.iter().map(move |b| (level, b)))
            {
                // Adjacent fields share one caption, including across a pinned seam.
                let band_height = 24. * options.band_span(level, band) as f32;
                let mut columns: Vec<_> = frame
                    .columns
                    .iter()
                    .filter(|column| {
                        band.fields
                            .contains(&self.settings.columns[column.id.0 as usize].field)
                    })
                    .collect();
                columns.sort_by(|a, b| a.clip.x.total_cmp(&b.clip.x));
                let mut fragments: Vec<(Rect, f32)> = Vec::new();
                for column in columns {
                    let Some(visible) = column.bounds.intersection(column.clip) else {
                        continue;
                    };
                    if visible.width <= 0.0 {
                        continue;
                    }
                    let rect = Rect::new(
                        visible.x,
                        bounds.y + panel + level as f32 * 24.0,
                        visible.width,
                        (bounds.height - panel - level as f32 * 24.0).clamp(0.0, band_height),
                    );
                    let field_end = column.bounds.x + column.bounds.width;
                    if let Some((last, end)) = fragments.last_mut()
                        && rect.x <= last.x + last.width + 0.01
                    {
                        last.width = last.width.max(rect.x + rect.width - last.x);
                        *end = end.max(field_end);
                    } else {
                        fragments.push((rect, field_end));
                    }
                }
                for (rect, end) in fragments {
                    scene.fill(rect, palette.header);
                    let band_cell = Rect::new(rect.x, rect.y, end - rect.x, band_height);
                    palette.paint_cell_edges(scene, band_cell, rect);
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
                        palette.muted,
                        500,
                        13.0,
                    );
                }
            }
            if !options.bands.is_empty() && height > 0. {
                let rule = height.min(1.);
                for y in [bands_top, bands_top + height - rule] {
                    scene.fill(Rect::new(bounds.x, y, bounds.width, rule), palette.line);
                }
            }
        }
        if self.source.presentation.summary_titles().next().is_some() {
            let options = &self.source.presentation;
            let lines = options.footer_lines();
            let height = (28.0 * lines as f32).min(bounds.height);
            let rect = Rect::new(
                bounds.x,
                bounds.y + bounds.height - height,
                bounds.width,
                height,
            );
            scene.fill(rect, palette.header);
            if options.view == GridViewMode::Table {
                scene.fill(
                    Rect::new(rect.x, rect.y, rect.width, 1_f32.min(rect.height)),
                    palette.line,
                );
            }
            let line = height / lines as f32;
            let mut cells: Vec<(Rect, Vec<String>)> = Vec::new();
            for (i, (title, value)) in options
                .summary_titles()
                .zip(&self.source.projection.totals)
                .enumerate()
            {
                let area = if let Some((_, field)) = options
                    .summary_columns
                    .iter()
                    .find(|(index, _)| *index == i)
                {
                    let Some(column) = frame
                        .columns
                        .iter()
                        .find(|c| &self.settings.columns[c.id.0 as usize].field == field)
                    else {
                        continue;
                    };
                    Rect::new(column.clip.x, rect.y, column.clip.width, line)
                } else {
                    Rect::new(rect.x, rect.y + height - line, rect.width, line)
                };
                let text = options.role_text(
                    "Summary",
                    &format!("{title}: {}", options.display_value(value)),
                );
                if let Some((_, parts)) = cells.iter_mut().find(|(bounds, _)| *bounds == area) {
                    parts.push(text);
                } else {
                    cells.push((area, vec![text]));
                }
            }
            for (area, parts) in cells {
                let inset = area.width.min(8.);
                draw_text(
                    scene,
                    Rect::new(area.x + inset, area.y, area.width - inset, area.height),
                    area,
                    parts.join("  ·  "),
                    palette.text,
                    600,
                    13.0,
                );
            }
        }
    }
}
pub(super) fn clip_key(r: Rect) -> [u32; 4] {
    [
        r.x.to_bits(),
        r.y.to_bits(),
        r.width.to_bits(),
        r.height.to_bits(),
    ]
}

/// A display indicator uses the cell's logical center, including when scrolled partly out of view.
pub(super) fn paint_check(
    scene: &mut Scene,
    cell: Rect,
    clip: Rect,
    palette: GridPalette,
    checked: Option<bool>,
) {
    let mark = Rect::new(
        cell.x + (cell.width - 14.) * 0.5,
        cell.y + (cell.height - 14.) * 0.5,
        14.,
        14.,
    );
    let Some(clip) = clip.intersection(cell) else {
        return;
    };
    let on = checked == Some(true);
    if on {
        scene.rounded_fill(mark, clip, 3., palette.focus);
    }
    scene.rounded_stroke(
        mark,
        clip,
        3.,
        1.,
        if on { palette.focus } else { palette.line },
    );
    match checked {
        Some(true) => {
            crate::primitives::paint_icon(scene, IconSymbol::Check, mark, clip, palette.background)
        }
        None => {
            let dash = Rect::new(mark.x + 3., mark.y + 6., 8., 2.);
            scene.rounded_fill(dash, clip, 1., palette.muted);
        }
        Some(false) => {}
    }
}

pub(super) fn draw_text(
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
    pub fn grid_cell_accessible_label(
        &self,
        id: WidgetId,
        column: ColumnId,
        value: &str,
    ) -> String {
        self.grids
            .get(&id)
            .and_then(|g| g.cell_templates.get(&column))
            .map_or_else(
                || value.into(),
                |t| format!("{}{}{}", t.prefix, value, t.suffix),
            )
    }
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
        Ok(source.revision() == grid.source.revision
            && !grid.source.detail_changed()?
            && !grid.source.choices_changed()?
            && unbound::revisions(&grid.source.unbound)? == grid.source.value_revisions)
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
    /// Replace a formula after type checking the entire candidate presentation.
    pub fn set_grid_calculated_expression(
        &mut self,
        id: WidgetId,
        field: &str,
        text: &str,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let mut next = grid.source.clone();
        let index = grid
            .settings
            .columns
            .iter()
            .position(|c| c.field == field && c.expression.is_some())
            .ok_or_else(|| error("Unknown calculated column"))?;
        let formula = next
            .formulas
            .iter_mut()
            .find(|(f, _)| f == field)
            .expect("declared formula");
        formula.1 = text.into();
        self.install_grid_presentation(
            id,
            next,
            self.grid_arrangement(id).expect("registered grid"),
        )?;
        self.grids.get_mut(&id).unwrap().settings.columns[index].expression = Some(text.into());
        Ok(())
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
        source: SnapshotSource,
        arrangement: Vec<GridColumnArrangement>,
    ) -> Result<(), ControlError> {
        self.install_grid_profile(id, source, arrangement, None)
    }
    pub(super) fn install_grid_profile(
        &mut self,
        id: WidgetId,
        mut source: SnapshotSource,
        arrangement: Vec<GridColumnArrangement>,
        query: Option<DataQuery>,
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
        source.reload_with_query(query.clone())?;
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
        for band in source
            .presentation
            .bands
            .iter()
            .chain(source.presentation.band_rows.iter().flatten())
        {
            self.measure_text(&band.title, 4096.0, false)?;
        }
        for title in source.presentation.summary_titles() {
            self.measure_text(title, 4096.0, false)?;
        }
        let grid = self.grids.get_mut(&id).unwrap();
        if let Some(query) = query
            && source.relation.is_none()
            && source.formulas.is_empty()
            && source.unbound.is_empty()
        {
            let mut adapter = source.handle.try_borrow_mut().map_err(error)?;
            adapter.set_query(query).map_err(error)?;
            source.revision = adapter.revision();
        }
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
        if expanded
            && self.grids.get(&id).is_some_and(|g| {
                g.source
                    .lazy_loaded
                    .as_ref()
                    .is_some_and(|keys| !keys.contains(&data_key(row)))
            })
        {
            return self.load_grid_branch(id, data_key(row));
        }
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
