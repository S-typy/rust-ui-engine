use rust_desktop_ui_core::{Color, Point, Rect, Scene, TextAlign, TextRun};

use crate::{ColumnId, GridError, RowKey, SortDirection, TreeDataSource, TreeGrid, VisibleRow};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridPalette {
    pub background: Color,
    pub alternate_row: Color,
    pub header: Color,
    pub text: Color,
    pub muted: Color,
    pub line: Color,
    pub selection: Color,
    pub selection_text: Color,
    pub focus: Color,
    pub error: Color,
}

impl Default for GridPalette {
    fn default() -> Self {
        Self {
            background: Color::rgb(255, 255, 255),
            alternate_row: Color::rgb(246, 248, 251),
            header: Color::rgb(226, 233, 242),
            text: Color::rgb(28, 35, 46),
            muted: Color::rgb(100, 111, 127),
            line: Color::rgb(146, 155, 165),
            selection: Color::rgb(208, 227, 250),
            selection_text: Color::rgb(20, 49, 85),
            focus: Color::rgb(45, 106, 190),
            error: Color::rgb(190, 45, 45),
        }
    }
}

impl GridPalette {
    /// Paint the bottom rule and the visible trailing edge, including a frozen-column seam.
    pub fn paint_cell_edges(self, scene: &mut Scene, cell: Rect, clip: Rect) {
        line(
            scene,
            Rect::new(
                (cell.x + cell.width).min(clip.x + clip.width) - 1.,
                cell.y,
                1.,
                cell.height,
            ),
            clip,
            self.line,
        );
        bottom_rule(scene, cell, clip, self.line);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleColumn {
    pub id: ColumnId,
    pub index: usize,
    pub bounds: Rect,
    pub clip: Rect,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridHitKind {
    Header(ColumnId),
    ResizeColumn(ColumnId),
    Expander(RowKey),
    Cell { row: RowKey, column: ColumnId },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridHit {
    pub bounds: Rect,
    pub kind: GridHitKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GridSemanticCell {
    pub column: ColumnId,
    pub label: String,
    pub bounds: Rect,
    pub editable: bool,
    pub editing: bool,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GridSemanticRow {
    pub key: RowKey,
    pub index: usize,
    pub depth: usize,
    pub parent: Option<RowKey>,
    pub position_in_set: usize,
    pub set_size: usize,
    pub bounds: Rect,
    pub selected: bool,
    pub focused: bool,
    pub expanded: Option<bool>,
    pub busy: bool,
    pub cells: Vec<GridSemanticCell>,
}

/// Virtual accessibility payload. An OS adapter gives these stable RowKey and
/// ColumnId values its own namespace and publishes only materialized rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridSemantics {
    pub sort: Vec<crate::SortOrder>,
    pub row_count: usize,
    pub column_count: usize,
    pub rows: Vec<GridSemanticRow>,
    pub focused: Option<RowKey>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GridFrameStats {
    pub logical_rows: usize,
    pub index_segments: usize,
    pub materialized_rows: usize,
    pub visible_columns: usize,
    pub cells_read: usize,
}

#[derive(Debug, Clone, Default)]
pub struct GridFrame {
    pub scene: Scene,
    pub header_bounds: Option<Rect>,
    pub rows: Vec<VisibleRow>,
    pub columns: Vec<VisibleColumn>,
    pub hits: Vec<GridHit>,
    /// Visible filter buttons, excluding the resize grip and clipped column lanes.
    pub header_filters: Vec<(ColumnId, Rect)>,
    pub semantics: GridSemantics,
    pub stats: GridFrameStats,
}

impl GridFrame {
    pub fn hit_test(&self, point: Point) -> Option<GridHitKind> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.bounds.contains(point.x, point.y))
            .map(|hit| hit.kind)
    }

    /// Clipped visible cell bounds, useful for placing the host's text editor.
    pub fn cell_bounds(&self, row: RowKey, column: ColumnId) -> Option<Rect> {
        self.hits
            .iter()
            .find_map(|hit| (hit.kind == GridHitKind::Cell { row, column }).then_some(hit.bounds))
    }
}

/// Stateless viewport materializer. It never creates retained UI nodes.
pub struct TreeGridView;

impl TreeGridView {
    pub fn paint(
        grid: &TreeGrid,
        source: &impl TreeDataSource,
        bounds: Rect,
    ) -> Result<GridFrame, GridError> {
        let mut frame = Self::paint_content(grid, source, bounds)?;
        frame.paint_header_feedback(grid, bounds);
        Ok(frame)
    }

    pub(crate) fn paint_content(
        grid: &TreeGrid,
        source: &impl TreeDataSource,
        bounds: Rect,
    ) -> Result<GridFrame, GridError> {
        if !bounds.is_valid() {
            return Err(GridError::InvalidGeometry("invalid grid bounds"));
        }
        let mut frame = GridFrame::default();
        frame.stats.logical_rows = grid.row_count();
        frame.stats.index_segments = grid.index_segment_count();
        frame.semantics.row_count = grid.row_count();
        frame.semantics.column_count = grid.columns.len();
        frame.semantics.focused = grid.focused;
        frame.semantics.sort.clone_from(&grid.query().sort);
        if bounds.width == 0.0 || bounds.height == 0.0 {
            return Ok(frame);
        }
        frame.scene.fill(bounds, grid.palette.background);
        frame.columns = visible_columns(grid, bounds);
        frame.stats.visible_columns = frame.columns.len();
        let header_height = grid.config.header_height.min(bounds.height);
        let body = Rect::new(
            bounds.x,
            bounds.y + header_height,
            bounds.width,
            bounds.height - header_height,
        );
        if body.height > 0.0 {
            let first = grid.first_at_offset(grid.scroll_y);
            let bottom = grid.scroll_y + f64::from(body.height);
            let edge = grid.first_at_offset(bottom);
            let end = if grid.record_top(edge) < bottom {
                edge.saturating_add(grid.record_lanes())
            } else {
                edge
            };
            let buffer = grid.config.overscan.saturating_mul(grid.record_lanes());
            let start = first.saturating_sub(buffer);
            let end = end.saturating_add(buffer).min(grid.row_count());
            if end.saturating_sub(start) > rust_desktop_ui_core::MAX_REALIZED_ITEMS {
                return Err(GridError::Capacity);
            }
            for index in start..end {
                let Some(row) = grid.row(source, index)? else {
                    continue;
                };
                let y = body.y + (grid.record_top(index) - grid.scroll_y) as f32;
                let row_bounds = Rect::new(body.x, y, body.width, grid.record_height(index));
                paint_row(grid, source, &row, row_bounds, body, &mut frame);
                frame.rows.push(row);
            }
        }
        frame.stats.materialized_rows = frame.rows.len();
        // Header is emitted last and remains fixed during vertical scrolling.
        let header = Rect::new(bounds.x, bounds.y, bounds.width, header_height);
        frame.header_bounds = Some(header);
        frame.scene.fill(header, grid.palette.header);
        bottom_rule(&mut frame.scene, header, header, grid.palette.line);
        for visible in &frame.columns {
            let column = &grid.columns[visible.index];
            let cell = grid.track_cell(
                column.id,
                visible.bounds.x,
                bounds.y,
                visible.bounds.width,
                header_height,
            );
            let Some(clip) = cell
                .intersection(visible.clip)
                .and_then(|rect| rect.intersection(header))
            else {
                continue;
            };
            frame.scene.fill(clip, grid.palette.header);
            let mut trailing = cell.x + cell.width - 5.;
            let mid = cell.y + cell.height / 2.;
            if let Some(index) = grid.query().sort.iter().position(|s| s.column == column.id) {
                let direction = grid.query().sort[index].direction;
                let sign = if direction == SortDirection::Ascending {
                    1.
                } else {
                    -1.
                };
                let x = trailing - 8.;
                symbol(
                    &mut frame.scene,
                    &[
                        (x - 4., mid + 2. * sign),
                        (x, mid - 2. * sign),
                        (x + 4., mid + 2. * sign),
                    ],
                    clip,
                    grid.palette.text,
                );
                trailing -= 16.;
                if grid.query().sort.len() > 1 {
                    let rank = (index + 1).to_string();
                    let w = 7. * rank.len() as f32 + 2.;
                    trailing -= w;
                    text(
                        &mut frame.scene,
                        Rect::new(trailing, cell.y, w, cell.height),
                        clip,
                        rank,
                        11.,
                        grid.palette.muted,
                        500,
                    );
                }
            }
            if let Some(active) = source.header_filter(column.id)
                && trailing - cell.x >= 38.
            {
                trailing -= 22.;
                let button = Rect::new(trailing, cell.y, 22., cell.height);
                if let Some(hit) = button.intersection(clip) {
                    let x = trailing + 11.;
                    symbol(
                        &mut frame.scene,
                        &[
                            (x - 5.5, mid - 4.5),
                            (x + 5.5, mid - 4.5),
                            (x + 1., mid + 0.5),
                            (x + 1., mid + 5.),
                            (x - 1.5, mid + 3.5),
                            (x - 1.5, mid + 0.5),
                            (x - 5.5, mid - 4.5),
                        ],
                        hit,
                        if active {
                            grid.palette.focus
                        } else {
                            grid.palette.muted
                        },
                    );
                    frame.header_filters.push((column.id, hit));
                }
            }
            let title = inset(
                Rect::new(cell.x, cell.y, (trailing - cell.x).max(0.), cell.height),
                7.,
            );
            if let Some(title_clip) = title.intersection(clip) {
                text(
                    &mut frame.scene,
                    title,
                    title_clip,
                    column.title.clone(),
                    grid.config.font_size,
                    grid.palette.text,
                    500,
                );
            }
            frame.hits.push(GridHit {
                bounds: clip,
                kind: GridHitKind::Header(column.id),
            });
            let edge = Rect::new(cell.x + cell.width - 4.0, cell.y, 4.0, cell.height);
            if let Some(edge) = edge.intersection(clip) {
                frame.hits.push(GridHit {
                    bounds: edge,
                    kind: GridHitKind::ResizeColumn(column.id),
                });
            }
            grid.palette.paint_cell_edges(&mut frame.scene, cell, clip);
        }
        if grid.pinned_width > 0.0 && grid.pinned_width < f64::from(bounds.width) {
            frame.scene.fill(
                Rect::new(
                    bounds.x + grid.pinned_width as f32 - 1.0,
                    bounds.y,
                    1.0,
                    bounds.height,
                ),
                grid.palette.line,
            );
        }
        Ok(frame)
    }
}

fn visible_columns(grid: &TreeGrid, bounds: Rect) -> Vec<VisibleColumn> {
    if let Some(tracks) = &grid.tracks {
        return tracks.visible(grid, bounds);
    }
    let mut columns = Vec::new();
    let pinned_width = (grid.pinned_width as f32).min(bounds.width);
    let regular_clip = Rect::new(
        bounds.x + pinned_width,
        bounds.y,
        bounds.width - pinned_width,
        bounds.height,
    );
    let first = grid
        .column_ends
        .partition_point(|end| *end <= grid.scroll_x);
    for position in first..grid.regular.len() {
        let index = grid.regular[position];
        let column = &grid.columns[index];
        let previous = if position == 0 {
            0.0
        } else {
            grid.column_ends[position - 1]
        };
        let x = regular_clip.x + (previous - grid.scroll_x) as f32;
        if x >= bounds.x + bounds.width {
            break;
        }
        let cell = Rect::new(x, bounds.y, column.width, bounds.height);
        if let Some(clip) = cell.intersection(regular_clip) {
            columns.push(VisibleColumn {
                id: column.id,
                index,
                bounds: cell,
                clip,
                pinned: false,
            });
        }
    }
    let mut x = bounds.x;
    for &index in &grid.pinned {
        if x >= bounds.x + bounds.width {
            break;
        }
        let column = &grid.columns[index];
        let cell = Rect::new(x, bounds.y, column.width, bounds.height);
        if let Some(clip) = cell.intersection(bounds) {
            columns.push(VisibleColumn {
                id: column.id,
                index,
                bounds: cell,
                clip,
                pinned: true,
            });
        }
        x += column.width;
    }
    columns
}

fn paint_row(
    grid: &TreeGrid,
    source: &impl TreeDataSource,
    row: &VisibleRow,
    bounds: Rect,
    body: Rect,
    frame: &mut GridFrame,
) {
    let Some(row_clip) = bounds.intersection(body) else {
        return;
    };
    let Some(key) = row.key else {
        let label = row.error.as_ref().map_or_else(
            || {
                if row.loading {
                    "Загрузка…".into()
                } else {
                    "Ожидание загрузки…".into()
                }
            },
            |error| format!("Ошибка: {error}"),
        );
        text(
            &mut frame.scene,
            inset(bounds, 8.0 + row.depth as f32 * 16.0),
            row_clip,
            label,
            grid.config.font_size,
            if row.error.is_some() {
                grid.palette.error
            } else {
                grid.palette.muted
            },
            400,
        );
        return;
    };
    let selected = grid.selected.contains(&key);
    frame.scene.fill(
        row_clip,
        if selected {
            grid.palette.selection
        } else if row.index.is_multiple_of(2) {
            grid.palette.background
        } else {
            grid.palette.alternate_row
        },
    );
    let (position_in_set, set_size) = grid.sibling_position(source, key).unwrap_or((0, 0));
    // Cover any space left after fields reach their maximum widths. Merged
    // cells and focus borders are painted over this shared rule.
    bottom_rule(&mut frame.scene, bounds, row_clip, grid.palette.line);
    let mut semantics = GridSemanticRow {
        key,
        index: row.index,
        depth: row.depth,
        parent: row.parent,
        position_in_set: position_in_set + 1,
        set_size,
        bounds: row_clip,
        selected,
        focused: grid.focused == Some(key),
        expanded: grid
            .has_children(source, key)
            .then(|| grid.is_expanded(key)),
        busy: grid.is_loading(key),
        cells: Vec::new(),
    };
    for visible in &frame.columns {
        let column = &grid.columns[visible.index];
        let cell = grid.track_cell(
            column.id,
            visible.bounds.x,
            bounds.y,
            visible.bounds.width,
            bounds.height,
        );
        let Some(clip) = cell
            .intersection(visible.clip)
            .and_then(|rect| rect.intersection(body))
        else {
            continue;
        };
        let editor = grid
            .editor
            .as_ref()
            .filter(|edit| edit.row == key && edit.column == column.id);
        let value = if let Some(editor) = editor {
            editor.value.clone()
        } else {
            source.cell(key, column.id)
        };
        frame.stats.cells_read += usize::from(editor.is_none());
        frame.hits.push(GridHit {
            bounds: clip,
            kind: GridHitKind::Cell {
                row: key,
                column: column.id,
            },
        });
        let mut padding = 8.0;
        if !grid.flat_presentation && column.id == grid.tree_column() {
            padding += row.depth as f32 * 16.0 + 18.0;
            if grid.has_children(source, key) {
                let marker = Rect::new(
                    cell.x + 4.0 + row.depth as f32 * 16.0,
                    cell.y,
                    18.0,
                    cell.height,
                );
                if let Some(marker_clip) = marker.intersection(clip) {
                    let x = marker.x + marker.width * 0.5;
                    let y = marker.y + marker.height * 0.5;
                    let points = if grid.is_expanded(key) {
                        [(x - 3.5, y - 1.75), (x, y + 1.75), (x + 3.5, y - 1.75)]
                    } else {
                        [(x - 1.75, y - 3.5), (x + 1.75, y), (x - 1.75, y + 3.5)]
                    };
                    symbol(
                        &mut frame.scene,
                        &points,
                        marker_clip,
                        if selected {
                            grid.palette.selection_text
                        } else {
                            grid.palette.muted
                        },
                    );
                    frame.hits.push(GridHit {
                        bounds: marker_clip,
                        kind: GridHitKind::Expander(key),
                    });
                }
            }
        }
        if editor.is_some() {
            frame.scene.fill(clip, grid.palette.background);
        }
        text(
            &mut frame.scene,
            inset(cell, padding),
            clip,
            value.clone(),
            grid.config.font_size,
            if selected {
                grid.palette.selection_text
            } else {
                grid.palette.text
            },
            400,
        );
        grid.palette.paint_cell_edges(&mut frame.scene, cell, clip);
        if grid.focused == Some(key) && grid.focused_column == Some(column.id) {
            let color = if editor.is_some_and(|edit| edit.error.is_some()) {
                grid.palette.error
            } else {
                grid.palette.focus
            };
            border(&mut frame.scene, cell, clip, color);
        }
        semantics.cells.push(GridSemanticCell {
            column: column.id,
            label: value,
            bounds: clip,
            editable: column.editable && source.editable(key, column.id),
            editing: editor.is_some(),
            validation_error: editor.and_then(|edit| edit.error.clone()),
        });
    }
    frame.semantics.rows.push(semantics);
}

fn inset(rect: Rect, left: f32) -> Rect {
    let left = left.min(rect.width);
    Rect::new(
        rect.x + left,
        rect.y + 3.0,
        (rect.width - left - 5.0).max(0.0),
        (rect.height - 6.0).max(0.0),
    )
}

fn text(
    scene: &mut Scene,
    bounds: Rect,
    clip: Rect,
    value: String,
    size: f32,
    color: Color,
    weight: u16,
) {
    if bounds.width > 0.0 && bounds.height > 0.0 {
        scene.text(TextRun {
            sensitive: false,
            bounds,
            clip,
            text: value,
            font_size: size,
            color,
            weight,
            align: TextAlign::Start,
            wrap: false,
        });
    }
}

fn line(scene: &mut Scene, bounds: Rect, clip: Rect, color: Color) {
    if let Some(bounds) = bounds.intersection(clip) {
        scene.fill(bounds, color);
    }
}

fn symbol(scene: &mut Scene, vertices: &[(f32, f32)], clip: Rect, color: Color) {
    let segments: Vec<_> = vertices
        .windows(2)
        .map(|w| rust_desktop_ui_core::StrokeSegment {
            start: Point::new(w[0].0, w[0].1),
            end: Point::new(w[1].0, w[1].1),
        })
        .collect();
    scene.stroke_path(&segments, clip, 1.4, color);
}

fn bottom_rule(scene: &mut Scene, rect: Rect, clip: Rect, color: Color) {
    line(
        scene,
        Rect::new(rect.x, rect.y + rect.height - 1., rect.width, 1.),
        clip,
        color,
    );
}

fn border(scene: &mut Scene, rect: Rect, clip: Rect, color: Color) {
    for bounds in [
        Rect::new(rect.x, rect.y, rect.width, 1.0),
        Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
        Rect::new(rect.x, rect.y, 1.0, rect.height),
        Rect::new(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height),
    ] {
        line(scene, bounds, clip, color);
    }
}
