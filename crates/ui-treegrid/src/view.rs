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
            line: Color::rgb(211, 220, 232),
            selection: Color::rgb(208, 227, 250),
            selection_text: Color::rgb(20, 49, 85),
            focus: Color::rgb(45, 106, 190),
            error: Color::rgb(190, 45, 45),
        }
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
    pub rows: Vec<VisibleRow>,
    pub columns: Vec<VisibleColumn>,
    pub hits: Vec<GridHit>,
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
        if !bounds.is_valid() {
            return Err(GridError::InvalidGeometry("invalid grid bounds"));
        }
        let mut frame = GridFrame::default();
        frame.stats.logical_rows = grid.row_count();
        frame.stats.index_segments = grid.index_segment_count();
        frame.semantics.row_count = grid.row_count();
        frame.semantics.column_count = grid.columns.len();
        frame.semantics.focused = grid.focused;
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
            let first = (grid.scroll_y / f64::from(grid.config.row_height)).floor() as usize;
            let end = ((grid.scroll_y + f64::from(body.height)) / f64::from(grid.config.row_height))
                .ceil() as usize;
            let start = first.saturating_sub(grid.config.overscan);
            let end = end
                .saturating_add(grid.config.overscan)
                .min(grid.row_count());
            for index in start..end {
                let Some(row) = grid.row(source, index)? else {
                    continue;
                };
                let y = body.y
                    + (index as f64 * f64::from(grid.config.row_height) - grid.scroll_y) as f32;
                let row_bounds = Rect::new(body.x, y, body.width, grid.config.row_height);
                paint_row(grid, source, &row, row_bounds, body, &mut frame);
                frame.rows.push(row);
            }
        }
        frame.stats.materialized_rows = frame.rows.len();
        // Header is emitted last and remains fixed during vertical scrolling.
        let header = Rect::new(bounds.x, bounds.y, bounds.width, header_height);
        frame.scene.fill(header, grid.palette.header);
        for visible in &frame.columns {
            let column = &grid.columns[visible.index];
            let cell = Rect::new(visible.bounds.x, bounds.y, column.width, header_height);
            let Some(clip) = cell
                .intersection(visible.clip)
                .and_then(|rect| rect.intersection(header))
            else {
                continue;
            };
            frame.scene.fill(clip, grid.palette.header);
            let suffix = grid
                .query()
                .sort
                .iter()
                .find(|sort| sort.column == column.id)
                .map_or("", |sort| {
                    if sort.direction == SortDirection::Ascending {
                        " ↑"
                    } else {
                        " ↓"
                    }
                });
            text(
                &mut frame.scene,
                inset(cell, 8.0),
                clip,
                format!("{}{suffix}", column.title),
                grid.config.font_size,
                grid.palette.text,
                600,
            );
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
            line(
                &mut frame.scene,
                Rect::new(cell.x + cell.width - 1.0, cell.y, 1.0, cell.height),
                clip,
                grid.palette.line,
            );
        }
        if grid.pinned_width > 0.0 && grid.pinned_width < f64::from(bounds.width) {
            frame.scene.fill(
                Rect::new(
                    bounds.x + grid.pinned_width as f32 - 1.0,
                    bounds.y,
                    1.0,
                    bounds.height,
                ),
                grid.palette.focus,
            );
        }
        Ok(frame)
    }
}

fn visible_columns(grid: &TreeGrid, bounds: Rect) -> Vec<VisibleColumn> {
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
    let mut semantics = GridSemanticRow {
        key,
        index: row.index,
        depth: row.depth,
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
        let cell = Rect::new(visible.bounds.x, bounds.y, column.width, bounds.height);
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
        if column.id == grid.tree_column() {
            padding += row.depth as f32 * 16.0 + 18.0;
            if grid.has_children(source, key) {
                let marker = Rect::new(
                    cell.x + 4.0 + row.depth as f32 * 16.0,
                    cell.y,
                    18.0,
                    cell.height,
                );
                if let Some(marker_clip) = marker.intersection(clip) {
                    text(
                        &mut frame.scene,
                        marker,
                        marker_clip,
                        if grid.is_expanded(key) { "-" } else { "+" }.into(),
                        grid.config.font_size,
                        grid.palette.text,
                        600,
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
        line(
            &mut frame.scene,
            Rect::new(cell.x + cell.width - 1.0, cell.y, 1.0, cell.height),
            clip,
            grid.palette.line,
        );
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
    line(
        &mut frame.scene,
        Rect::new(bounds.x, bounds.y + bounds.height - 1.0, bounds.width, 1.0),
        body,
        grid.palette.line,
    );
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
