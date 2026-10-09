use std::{cell::Cell, collections::HashMap};

use rust_desktop_ui_core::{Point, Rect, Size};
use rust_desktop_ui_treegrid::*;

struct IndexedRows {
    count: usize,
    reads: Cell<usize>,
    lookups: Cell<usize>,
}

impl IndexedRows {
    fn new(count: usize) -> Self {
        Self {
            count,
            reads: Cell::new(0),
            lookups: Cell::new(0),
        }
    }
}

impl TreeDataSource for IndexedRows {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        Some(if parent.is_none() { self.count } else { 0 })
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        self.lookups.set(self.lookups.get() + 1);
        (parent.is_none() && index < self.count).then_some(RowKey(index as u64 + 1))
    }
    fn parent(&self, _row: RowKey) -> Option<RowKey> {
        None
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        (row.0 > 0 && row.0 <= self.count as u64).then_some(row.0.saturating_sub(1) as usize)
    }
    fn cell(&self, row: RowKey, column: ColumnId) -> String {
        self.reads.set(self.reads.get() + 1);
        format!("Книга {} / {}", row.0, column.0)
    }
}

fn columns(count: usize) -> Vec<GridColumn> {
    (0..count)
        .map(|index| {
            let mut column =
                GridColumn::new(ColumnId(index as u64), format!("Колонка {index}"), 140.0);
            column.pinned = index == 0;
            column.editable = true;
            column
        })
        .collect()
}

fn bounds() -> Rect {
    Rect::new(20.0, 60.0, 900.0, 600.0)
}

#[test]
fn million_logical_rows_and_wide_columns_materialize_only_the_viewport() {
    for count in [1_000, 100_000, 1_000_000] {
        let mut source = IndexedRows::new(count);
        let mut grid = TreeGrid::new(columns(1_000)).unwrap();
        grid.refresh(&mut source).unwrap();
        assert_eq!(grid.index_segment_count(), 1);
        grid.set_viewport(Size::new(900.0, 600.0)).unwrap();
        grid.scroll_to(55_000.0, (count / 2) as f64 * 28.0).unwrap();
        let frame = grid.paint(&source, bounds()).unwrap();
        assert_eq!(frame.stats.logical_rows, count);
        assert!(
            frame.stats.materialized_rows <= 26,
            "{}",
            frame.stats.materialized_rows
        );
        assert!(frame.stats.visible_columns <= 8);
        assert!(source.reads.get() <= 26 * 8);
        assert!(source.lookups.get() <= 26);
        assert!(
            frame
                .rows
                .iter()
                .filter_map(|row| row.key)
                .all(|key| key.0 > 1)
        );
        assert_eq!(
            frame
                .columns
                .iter()
                .find(|column| column.pinned)
                .unwrap()
                .bounds
                .x,
            20.0
        );
        assert!(
            frame
                .columns
                .iter()
                .filter(|column| !column.pinned)
                .all(|column| column.clip.x >= 160.0)
        );
        assert!(frame.scene.texts.iter().all(|text| text.clip.is_valid()));
        assert!(frame.semantics.rows.len() < frame.rows.len());
        assert_eq!(frame.semantics.row_count, count);
        assert_eq!(frame.semantics.column_count, 1_000);
    }
}

#[derive(Default)]
struct TreeSource {
    roots: Vec<RowKey>,
    root_count_override: Option<usize>,
    children: HashMap<RowKey, Option<Vec<RowKey>>>,
    parents: HashMap<RowKey, RowKey>,
    cells: HashMap<(RowKey, ColumnId), String>,
    requested: Vec<RequestId>,
    cancelled: Vec<RequestId>,
    immediate: Option<Vec<RowKey>>,
    queries: Vec<GridQuery>,
}

impl TreeSource {
    fn sample() -> Self {
        let mut source = Self {
            roots: vec![RowKey(1), RowKey(2)],
            ..Self::default()
        };
        source
            .children
            .insert(RowKey(1), Some(vec![RowKey(3), RowKey(4)]));
        source.children.insert(RowKey(3), Some(vec![RowKey(5)]));
        source.children.insert(RowKey(2), None);
        source.parents.extend([
            (RowKey(3), RowKey(1)),
            (RowKey(4), RowKey(1)),
            (RowKey(5), RowKey(3)),
        ]);
        source
    }
}

impl TreeDataSource for TreeSource {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        parent.map_or(
            Some(self.root_count_override.unwrap_or(self.roots.len())),
            |key| {
                self.children
                    .get(&key)
                    .map_or(Some(0), |children| children.as_ref().map(Vec::len))
            },
        )
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        match parent {
            None => self.roots.get(index).copied(),
            Some(key) => self.children.get(&key)?.as_ref()?.get(index).copied(),
        }
    }
    fn parent(&self, row: RowKey) -> Option<RowKey> {
        self.parents.get(&row).copied()
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        let rows = match self.parent(row) {
            None => &self.roots,
            Some(parent) => self.children.get(&parent)?.as_ref()?,
        };
        rows.iter().position(|key| *key == row)
    }
    fn cell(&self, row: RowKey, column: ColumnId) -> String {
        self.cells
            .get(&(row, column))
            .cloned()
            .unwrap_or_else(|| format!("Узел {}", row.0))
    }
    fn editable(&self, _row: RowKey, _column: ColumnId) -> bool {
        true
    }
    fn validate_edit(&self, _row: RowKey, _column: ColumnId, value: &str) -> Result<(), String> {
        if value.trim().is_empty() {
            Err("Название обязательно".into())
        } else {
            Ok(())
        }
    }
    fn set_cell(&mut self, row: RowKey, column: ColumnId, value: &str) -> Result<(), String> {
        self.cells.insert((row, column), value.into());
        Ok(())
    }
    fn apply_query(&mut self, query: &GridQuery) -> Result<(), String> {
        self.queries.push(query.clone());
        self.roots = [RowKey(1), RowKey(2)]
            .into_iter()
            .filter(|row| query.filter.is_empty() || row.0.to_string().contains(&query.filter))
            .collect();
        if query
            .sort
            .first()
            .is_some_and(|sort| sort.direction == SortDirection::Descending)
        {
            self.roots.reverse();
        }
        Ok(())
    }
    fn request_children(
        &mut self,
        _parent: RowKey,
        request: RequestId,
    ) -> Result<ChildrenLoad, String> {
        self.requested.push(request);
        Ok(self
            .immediate
            .take()
            .map_or(ChildrenLoad::Pending, ChildrenLoad::Ready))
    }
    fn cancel_request(&mut self, request: RequestId) {
        self.cancelled.push(request);
    }
}

fn fixture() -> (TreeGrid, TreeSource) {
    let mut source = TreeSource::sample();
    let mut grid = TreeGrid::new(columns(4)).unwrap();
    grid.refresh(&mut source).unwrap();
    grid.set_viewport(Size::new(900.0, 600.0)).unwrap();
    (grid, source)
}

fn keys(grid: &TreeGrid, source: &impl TreeDataSource) -> Vec<Option<RowKey>> {
    (0..grid.row_count())
        .map(|index| grid.row(source, index).unwrap().unwrap().key)
        .collect()
}

#[test]
fn hierarchy_collapse_preserves_stable_selection_and_nested_expansion() {
    let (mut grid, mut source) = fixture();
    grid.set_expanded(&mut source, RowKey(1), true).unwrap();
    grid.set_expanded(&mut source, RowKey(3), true).unwrap();
    assert_eq!(
        keys(&grid, &source),
        [1, 3, 5, 4, 2].map(|key| Some(RowKey(key)))
    );
    assert_eq!(grid.row(&source, 2).unwrap().unwrap().depth, 2);
    grid.select(&source, RowKey(5), SelectionModifiers::default())
        .unwrap();
    grid.set_expanded(&mut source, RowKey(1), false).unwrap();
    assert_eq!(keys(&grid, &source), vec![Some(RowKey(1)), Some(RowKey(2))]);
    assert!(grid.is_selected(RowKey(5)));
    assert_eq!(grid.focused(), Some(RowKey(1)));
    grid.set_expanded(&mut source, RowKey(1), true).unwrap();
    assert_eq!(grid.visible_index(&source, RowKey(5)), Some(2));
    assert!(grid.is_expanded(RowKey(3)));
}

#[test]
fn failed_lazy_reindex_rolls_back_cached_keys_and_keeps_request_retryable() {
    let (mut grid, mut source) = fixture();
    let request = grid
        .set_expanded(&mut source, RowKey(2), true)
        .unwrap()
        .unwrap();
    assert_eq!(grid.row_count(), 3);
    source.root_count_override = Some(usize::MAX);
    assert!(matches!(
        grid.complete_children(&source, request, Ok(vec![RowKey(6)])),
        Err(GridError::Capacity)
    ));
    assert_eq!(grid.row_count(), 3);
    assert!(grid.is_loading(RowKey(2)));
    assert_eq!(grid.visible_index(&source, RowKey(6)), None);
    source.root_count_override = None;
    assert!(
        grid.complete_children(&source, request, Ok(vec![RowKey(7)]))
            .unwrap()
    );
    assert_eq!(
        keys(&grid, &source),
        [Some(RowKey(1)), Some(RowKey(2)), Some(RowKey(7))]
    );
    assert_eq!(grid.visible_index(&source, RowKey(6)), None);
    assert!(!grid.is_loading(RowKey(2)));
}

#[test]
fn lazy_loading_cancels_and_rejects_stale_foreign_and_previous_query_responses() {
    let (mut grid, mut source) = fixture();
    let first = grid
        .set_expanded(&mut source, RowKey(2), true)
        .unwrap()
        .unwrap();
    assert!(grid.row(&source, 2).unwrap().unwrap().loading);
    grid.set_expanded(&mut source, RowKey(2), false).unwrap();
    assert_eq!(source.cancelled, vec![first]);
    assert!(
        !grid
            .complete_children(&source, first, Ok(vec![RowKey(6)]))
            .unwrap()
    );
    let second = grid
        .set_expanded(&mut source, RowKey(2), true)
        .unwrap()
        .unwrap();
    assert_ne!(first, second);
    let (mut other, mut other_source) = fixture();
    let foreign = other
        .set_expanded(&mut other_source, RowKey(2), true)
        .unwrap()
        .unwrap();
    assert!(
        !grid
            .complete_children(&source, foreign, Ok(vec![RowKey(9)]))
            .unwrap()
    );
    assert!(
        grid.complete_children(&source, second, Ok(vec![RowKey(6), RowKey(7)]))
            .unwrap()
    );
    assert_eq!(
        keys(&grid, &source),
        [1, 2, 6, 7].map(|key| Some(RowKey(key)))
    );
    assert_eq!(grid.visible_index(&source, RowKey(7)), Some(3));
    assert!(grid.pending_requests().is_empty());
    grid.refresh(&mut source).unwrap();
    let third = grid
        .set_expanded(&mut source, RowKey(2), true)
        .unwrap()
        .unwrap();
    grid.apply_query(
        &mut source,
        GridQuery {
            filter: "1".into(),
            ..GridQuery::default()
        },
    )
    .unwrap();
    assert!(source.cancelled.contains(&third));
    assert!(
        !grid
            .complete_children(&source, third, Ok(vec![RowKey(8)]))
            .unwrap()
    );
    assert_eq!(grid.row_count(), 1);
}

#[test]
fn malformed_lazy_results_do_not_install_duplicate_or_cyclic_keys() {
    let (mut grid, mut source) = fixture();
    let request = grid
        .set_expanded(&mut source, RowKey(2), true)
        .unwrap()
        .unwrap();
    assert!(
        grid.complete_children(&source, request, Ok(vec![RowKey(6), RowKey(6)]))
            .is_err()
    );
    assert!(
        grid.complete_children(&source, request, Ok(vec![RowKey(2)]))
            .is_err()
    );
    assert!(
        grid.complete_children(&source, request, Ok(vec![RowKey(1)]))
            .is_err()
    );
    assert_eq!(grid.row_count(), 3);
    assert_eq!(grid.pending_requests(), vec![request]);
    assert!(
        grid.complete_children(&source, request, Err("Временная ошибка".into()))
            .unwrap()
    );
    let row = grid.row(&source, 2).unwrap().unwrap();
    assert_eq!(row.error.as_deref(), Some("Временная ошибка"));
    assert!(!row.loading);
    source.immediate = Some(vec![RowKey(9)]);
    grid.set_expanded(&mut source, RowKey(2), true).unwrap();
    assert_eq!(grid.row(&source, 2).unwrap().unwrap().key, Some(RowKey(9)));
}

#[test]
fn keyboard_navigation_ranges_ctrl_selection_and_scroll_are_consistent() {
    let mut source = IndexedRows::new(100_000);
    let mut grid = TreeGrid::new(columns(3)).unwrap();
    grid.refresh(&mut source).unwrap();
    grid.set_viewport(Size::new(400.0, 312.0)).unwrap();
    grid.navigate(&mut source, GridKey::Home, SelectionModifiers::default())
        .unwrap();
    grid.navigate(
        &mut source,
        GridKey::PageDown,
        SelectionModifiers {
            shift: true,
            control: false,
        },
    )
    .unwrap();
    assert_eq!(grid.focused(), Some(RowKey(11)));
    assert_eq!(grid.selected().len(), 11);
    grid.select(
        &source,
        RowKey(5),
        SelectionModifiers {
            shift: false,
            control: true,
        },
    )
    .unwrap();
    assert!(!grid.is_selected(RowKey(5)));
    grid.navigate(
        &mut source,
        GridKey::End,
        SelectionModifiers {
            shift: false,
            control: true,
        },
    )
    .unwrap();
    assert_eq!(grid.focused(), Some(RowKey(100_000)));
    assert_eq!(grid.selected().len(), 10);
    assert!(grid.scroll_offset().1 > 2_000_000.0);
    grid.navigate(&mut source, GridKey::Up, SelectionModifiers::default())
        .unwrap();
    assert_eq!(grid.selected().len(), 1);
    assert!(grid.is_selected(RowKey(99_999)));
}

#[test]
fn sorting_and_filtering_are_delegated_and_selection_uses_keys() {
    let (mut grid, mut source) = fixture();
    grid.select(&source, RowKey(1), SelectionModifiers::default())
        .unwrap();
    grid.apply_query(
        &mut source,
        GridQuery {
            sort: vec![SortOrder {
                column: ColumnId(0),
                direction: SortDirection::Descending,
            }],
            filter: String::new(),
        },
    )
    .unwrap();
    assert_eq!(keys(&grid, &source), vec![Some(RowKey(2)), Some(RowKey(1))]);
    assert!(grid.is_selected(RowKey(1)));
    assert_eq!(grid.visible_index(&source, RowKey(1)), Some(1));
    grid.apply_query(
        &mut source,
        GridQuery {
            filter: "2".into(),
            ..GridQuery::default()
        },
    )
    .unwrap();
    assert_eq!(keys(&grid, &source), vec![Some(RowKey(2))]);
    assert_eq!(source.queries.len(), 2);
    assert!(grid.is_selected(RowKey(1))); // filtering is not deletion
    assert_eq!(grid.focused(), None);
}

#[test]
fn editing_validation_cancel_commit_and_semantics_preserve_cell_focus() {
    let (mut grid, mut source) = fixture();
    grid.begin_edit(&source, RowKey(1), ColumnId(0)).unwrap();
    grid.set_edit_text("  ").unwrap();
    assert!(matches!(
        grid.commit_edit(&mut source),
        Err(GridError::Validation(_))
    ));
    assert!(grid.editor().unwrap().error.is_some());
    assert!(source.cells.is_empty());
    let frame = grid.paint(&source, bounds()).unwrap();
    let cell = frame
        .semantics
        .rows
        .iter()
        .find(|row| row.key == RowKey(1))
        .unwrap()
        .cells
        .iter()
        .find(|cell| cell.column == ColumnId(0))
        .unwrap();
    assert!(cell.editing && cell.validation_error.is_some());
    grid.cancel_edit();
    assert!(source.cells.is_empty());
    grid.begin_edit(&source, RowKey(1), ColumnId(0)).unwrap();
    grid.set_edit_text("Преступление и наказание").unwrap();
    grid.commit_edit(&mut source).unwrap();
    assert_eq!(
        source.cell(RowKey(1), ColumnId(0)),
        "Преступление и наказание"
    );
    assert_eq!(grid.focused(), Some(RowKey(1)));
    assert_eq!(grid.focused_column(), Some(ColumnId(0)));
    assert!(grid.editor().is_none());
}

#[test]
fn column_layout_roundtrips_and_rejects_partial_invalid_updates() {
    let mut grid = TreeGrid::new(columns(3)).unwrap();
    grid.resize_column(ColumnId(1), 333.5).unwrap();
    grid.reorder_column(ColumnId(2), Some(ColumnId(0))).unwrap();
    grid.pin_column(ColumnId(2), true).unwrap();
    let saved = grid.save_columns();
    let mut restored = TreeGrid::new(columns(4)).unwrap();
    restored.restore_columns(&saved).unwrap();
    assert_eq!(&restored.columns()[..3], grid.columns());
    assert_eq!(restored.columns()[3].id, ColumnId(3));
    let previous = restored.columns().to_vec();
    assert!(
        restored
            .restore_columns("treegrid-columns\t1\n1\t300\t0\n2\tNaN\t1\n")
            .is_err()
    );
    assert_eq!(restored.columns(), previous);
    assert!(restored.restore_columns("treegrid-columns\t9\n").is_err());
    assert!(restored.resize_column(ColumnId(0), f32::NAN).is_err());
    assert_eq!(restored.columns(), previous);
}

#[test]
fn pointer_hits_support_expanders_cells_header_sort_resize_and_reorder() {
    let (mut grid, mut source) = fixture();
    let frame = grid.paint(&source, bounds()).unwrap();
    let expander = frame
        .hits
        .iter()
        .find(|hit| hit.kind == GridHitKind::Expander(RowKey(1)))
        .unwrap()
        .bounds;
    grid.pointer_down(
        &mut source,
        &frame,
        Point::new(expander.x + 2.0, expander.y + 2.0),
        SelectionModifiers::default(),
    )
    .unwrap();
    assert!(grid.is_expanded(RowKey(1)));
    let frame = grid.paint(&source, bounds()).unwrap();
    let cell = frame.cell_bounds(RowKey(3), ColumnId(1)).unwrap();
    grid.pointer_down(
        &mut source,
        &frame,
        Point::new(cell.x + 20.0, cell.y + 4.0),
        SelectionModifiers::default(),
    )
    .unwrap();
    assert_eq!(grid.focused(), Some(RowKey(3)));
    let resize = frame
        .hits
        .iter()
        .find(|hit| hit.kind == GridHitKind::ResizeColumn(ColumnId(0)))
        .unwrap()
        .bounds;
    let down = Point::new(resize.x + 1.0, resize.y + 4.0);
    grid.pointer_down(&mut source, &frame, down, SelectionModifiers::default())
        .unwrap();
    assert!(grid.pointer_captured());
    grid.pointer_move(Point::new(down.x + 50.0, down.y))
        .unwrap();
    grid.pointer_up(&mut source, &frame, Point::new(down.x + 50.0, down.y))
        .unwrap();
    assert_eq!(grid.columns()[0].width, 190.0);
    assert!(!grid.pointer_captured());
    let frame = grid.paint(&source, bounds()).unwrap();
    let header = frame
        .hits
        .iter()
        .find(|hit| hit.kind == GridHitKind::Header(ColumnId(1)))
        .unwrap()
        .bounds;
    let point = Point::new(header.x + 10.0, header.y + 5.0);
    grid.pointer_down(&mut source, &frame, point, SelectionModifiers::default())
        .unwrap();
    grid.pointer_up(&mut source, &frame, point).unwrap();
    assert_eq!(source.queries.len(), 1);
    let frame = grid.paint(&source, bounds()).unwrap();
    let destination = frame
        .hits
        .iter()
        .find(|hit| hit.kind == GridHitKind::Header(ColumnId(3)))
        .unwrap()
        .bounds;
    grid.pointer_down(&mut source, &frame, point, SelectionModifiers::default())
        .unwrap();
    grid.pointer_up(
        &mut source,
        &frame,
        Point::new(destination.x + 10.0, destination.y + 5.0),
    )
    .unwrap();
    assert_eq!(
        grid.columns()
            .iter()
            .map(|column| column.id)
            .collect::<Vec<_>>(),
        vec![ColumnId(0), ColumnId(2), ColumnId(1), ColumnId(3)]
    );
    grid.reorder_column(ColumnId(1), Some(ColumnId(2))).unwrap();
    assert_eq!(grid.columns()[1].id, ColumnId(1));
}

#[test]
fn zero_viewport_invalid_geometry_and_clamped_scroll_do_not_read_cells() {
    let mut source = IndexedRows::new(100_000);
    let mut grid = TreeGrid::new(columns(3)).unwrap();
    grid.refresh(&mut source).unwrap();
    let frame = grid.paint(&source, Rect::new(0.0, 0.0, 0.0, 0.0)).unwrap();
    assert_eq!(frame.stats.materialized_rows, 0);
    assert_eq!(source.reads.get(), 0);
    assert!(grid.scroll_to(f64::NAN, 0.0).is_err());
    assert!(grid.set_viewport(Size::new(f32::INFINITY, 100.0)).is_err());
    grid.set_viewport(Size::new(900.0, 600.0)).unwrap();
    grid.scroll_to(f64::MAX, f64::MAX).unwrap();
    let frame = grid.paint(&source, bounds()).unwrap();
    assert!(
        frame
            .rows
            .iter()
            .any(|row| row.key == Some(RowKey(100_000)))
    );
    assert!(
        frame
            .scene
            .rectangles
            .iter()
            .all(|rect| rect.bounds.is_valid())
    );
}

#[test]
fn one_expanded_branch_with_a_million_children_uses_three_index_segments() {
    struct Hierarchy {
        lookups: Cell<usize>,
    }
    impl TreeDataSource for Hierarchy {
        fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
            Some(match parent {
                None => 2,
                Some(RowKey(1)) => 1_000_000,
                _ => 0,
            })
        }
        fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
            self.lookups.set(self.lookups.get() + 1);
            match parent {
                None => [RowKey(1), RowKey(2)].get(index).copied(),
                Some(RowKey(1)) if index < 1_000_000 => Some(RowKey(index as u64 + 1000)),
                _ => None,
            }
        }
        fn parent(&self, row: RowKey) -> Option<RowKey> {
            (row.0 >= 1000).then_some(RowKey(1))
        }
        fn position(&self, row: RowKey) -> Option<usize> {
            match row.0 {
                1 | 2 => Some(row.0 as usize - 1),
                1000..1_001_000 => Some(row.0 as usize - 1000),
                _ => None,
            }
        }
        fn cell(&self, row: RowKey, column: ColumnId) -> String {
            format!("{}:{}", row.0, column.0)
        }
    }
    let mut source = Hierarchy {
        lookups: Cell::new(0),
    };
    let mut grid = TreeGrid::new(columns(4)).unwrap();
    grid.refresh(&mut source).unwrap();
    grid.set_expanded(&mut source, RowKey(1), true).unwrap();
    assert_eq!(grid.row_count(), 1_000_002);
    assert_eq!(grid.index_segment_count(), 3);
    grid.set_viewport(Size::new(900.0, 600.0)).unwrap();
    grid.scroll_to(0.0, 500_000.0 * 28.0).unwrap();
    source.lookups.set(0);
    let frame = grid.paint(&source, bounds()).unwrap();
    assert!(frame.rows.iter().all(|row| row.depth == 1));
    assert!(source.lookups.get() <= 26);
    assert_eq!(
        grid.visible_index(&source, RowKey(1_000_999)),
        Some(1_000_000)
    );
    assert_eq!(grid.visible_index(&source, RowKey(2)), Some(1_000_001));
}

#[test]
fn cancelling_an_ancestor_cancels_descendant_loads_and_right_on_leaf_stays_put() {
    let (mut grid, mut source) = fixture();
    source.children.insert(RowKey(3), None);
    grid.set_expanded(&mut source, RowKey(1), true).unwrap();
    let request = grid
        .set_expanded(&mut source, RowKey(3), true)
        .unwrap()
        .unwrap();
    grid.set_expanded(&mut source, RowKey(1), false).unwrap();
    assert_eq!(source.cancelled, vec![request]);
    assert!(
        !grid
            .complete_children(&source, request, Ok(vec![RowKey(8)]))
            .unwrap()
    );
    grid.set_expanded(&mut source, RowKey(1), true).unwrap();
    grid.select(&source, RowKey(4), SelectionModifiers::default())
        .unwrap();
    grid.navigate(&mut source, GridKey::Right, SelectionModifiers::default())
        .unwrap();
    assert_eq!(grid.focused(), Some(RowKey(4)));
    grid.navigate(&mut source, GridKey::Left, SelectionModifiers::default())
        .unwrap();
    assert_eq!(grid.focused(), Some(RowKey(1)));
    grid.navigate(&mut source, GridKey::Right, SelectionModifiers::default())
        .unwrap();
    assert_eq!(grid.focused(), Some(RowKey(3)));
    let second = grid
        .set_expanded(&mut source, RowKey(3), true)
        .unwrap()
        .unwrap();
    grid.cancel_pending_requests(&mut source);
    assert!(source.cancelled.contains(&second));
    assert!(
        !grid
            .complete_children(&source, second, Ok(vec![RowKey(9)]))
            .unwrap()
    );
}

#[test]
fn inconsistent_source_cycles_are_rejected_before_expansion_changes() {
    struct Cyclic;
    impl TreeDataSource for Cyclic {
        fn child_count(&self, _parent: Option<RowKey>) -> Option<usize> {
            Some(1)
        }
        fn child_at(&self, parent: Option<RowKey>, _index: usize) -> Option<RowKey> {
            Some(if parent == Some(RowKey(1)) {
                RowKey(2)
            } else {
                RowKey(1)
            })
        }
        fn parent(&self, row: RowKey) -> Option<RowKey> {
            Some(if row == RowKey(1) {
                RowKey(2)
            } else {
                RowKey(1)
            })
        }
        fn position(&self, _row: RowKey) -> Option<usize> {
            Some(0)
        }
        fn cell(&self, _row: RowKey, _column: ColumnId) -> String {
            String::new()
        }
    }
    let mut source = Cyclic;
    let mut grid = TreeGrid::new(columns(1)).unwrap();
    grid.refresh(&mut source).unwrap();
    assert!(matches!(
        grid.set_expanded(&mut source, RowKey(1), true),
        Err(GridError::InvalidHierarchy(_))
    ));
    assert!(!grid.is_expanded(RowKey(1)));
    assert_eq!(grid.row_count(), 1);
}
