use rust_desktop_ui_core::{Rect, Size};
use rust_desktop_ui_treegrid::*;

fn node(key: u64, parent: Option<u64>) -> TreeNode {
    TreeNode::new(RowKey(key), parent.map(RowKey), format!("Node {key}"))
}
fn grid(source: &mut impl TreeDataSource) -> TreeGrid {
    let mut grid = TreeGrid::new(vec![
        GridColumn::new(ColumnId(0), "Name", 240.0),
        GridColumn::new(ColumnId(1), "Value", 120.0),
    ])
    .unwrap();
    grid.refresh(source).unwrap();
    grid.set_viewport(Size::new(400.0, 116.0)).unwrap();
    grid
}
fn visible(grid: &TreeGrid, source: &impl TreeDataSource) -> Vec<RowKey> {
    (0..grid.row_count())
        .filter_map(|i| grid.row(source, i).unwrap().and_then(|r| r.key))
        .collect()
}
#[test]
fn complete_graph_validation_rejects_disconnected_cycles_missing_duplicates_and_depth() {
    for records in [
        vec![node(1, None), node(1, None)],
        vec![node(1, Some(2))],
        vec![node(1, None), node(2, Some(3)), node(3, Some(2))],
        (1..=257)
            .map(|key| node(key, (key > 1).then_some(key - 1)))
            .collect(),
    ] {
        assert!(IndexedTreeSource::new(records).is_err());
    }
    let mut parent = node(1, None);
    parent.children = TreeChildren::Unloaded;
    assert!(IndexedTreeSource::new(vec![parent, node(2, Some(1))]).is_err());
    let source =
        IndexedTreeSource::new(vec![node(3, Some(1)), node(2, None), node(1, None)]).unwrap();
    assert_eq!(source.child_at(None, 0), Some(RowKey(2)));
    assert_eq!(source.child_at(Some(RowKey(1)), 0), Some(RowKey(3)));
    assert_eq!(source.position(RowKey(3)), Some(0));
}
#[test]
fn invalid_replace_move_and_noop_leave_accepted_source_intact() {
    let mut source = IndexedTreeSource::new(vec![
        node(1, None),
        node(2, Some(1)),
        node(3, Some(2)),
        node(4, None),
    ])
    .unwrap();
    let snapshot = source.nodes().to_vec();
    for (rows, target) in [
        (vec![RowKey(1)], TreeMoveTarget::Into(Some(RowKey(3)))),
        (vec![RowKey(1), RowKey(2)], TreeMoveTarget::Into(None)),
        (
            vec![RowKey(2), RowKey(2)],
            TreeMoveTarget::Before(RowKey(4)),
        ),
        (vec![RowKey(2)], TreeMoveTarget::Before(RowKey(2))),
        (vec![RowKey(99)], TreeMoveTarget::Into(None)),
    ] {
        assert!(source.move_rows(&rows, target).is_err());
        assert_eq!(source.nodes(), snapshot);
        assert_eq!(source.revision(), 0);
    }
    assert!(source.replace(vec![node(4, Some(999))]).is_err());
    assert_eq!(source.nodes(), snapshot);
    assert!(!source.replace(snapshot).unwrap());
    assert_eq!(source.revision(), 0);
}
#[test]
fn moving_multiple_siblings_preserves_identity_subtrees_and_requested_order() {
    let mut source = IndexedTreeSource::new(vec![
        node(1, None),
        node(2, None),
        node(3, None),
        node(4, Some(1)),
        node(5, Some(4)),
    ])
    .unwrap();
    assert!(
        source
            .move_rows(
                &[RowKey(3), RowKey(2)],
                TreeMoveTarget::Into(Some(RowKey(4)))
            )
            .unwrap()
    );
    assert_eq!(
        (0..3)
            .map(|i| source.child_at(Some(RowKey(4)), i).unwrap())
            .collect::<Vec<_>>(),
        [RowKey(5), RowKey(3), RowKey(2)]
    );
    assert_eq!(source.parent(RowKey(4)), Some(RowKey(1)));
    assert_eq!(source.revision(), 1);
    assert!(
        source
            .move_rows(&[RowKey(4)], TreeMoveTarget::Into(None))
            .unwrap()
    );
    assert_eq!(source.parent(RowKey(5)), Some(RowKey(4)));
    assert_eq!(source.child_at(None, 1), Some(RowKey(4)));
}
#[test]
fn expansion_collapse_lazy_completion_and_external_reorder_keep_top_key_fraction() {
    let mut records: Vec<_> = (1..=100).map(|k| node(k, None)).collect();
    records.extend([node(101, Some(1)), node(102, Some(1))]);
    let mut source = IndexedTreeSource::new(records).unwrap();
    let mut model = grid(&mut source);
    model.scroll_to(0.0, 49.0 * 28.0 + 7.0).unwrap();
    let anchor = model.capture_scroll_anchor(&source);
    assert_eq!(anchor.key, Some(RowKey(50)));
    model.set_expanded(&mut source, RowKey(1), true).unwrap();
    assert_eq!(
        model.capture_scroll_anchor(&source),
        TreeScrollAnchor {
            fallback_index: 51,
            ..anchor
        }
    );
    model.set_expanded(&mut source, RowKey(1), false).unwrap();
    assert_eq!(model.capture_scroll_anchor(&source), anchor);
    model
        .paint(&source, Rect::new(0.0, 0.0, 400.0, 116.0))
        .unwrap();
    let mut records = source.nodes().to_vec();
    records.insert(0, node(999, None));
    source.replace(records).unwrap();
    model.refresh(&mut source).unwrap();
    assert_eq!(
        model.capture_scroll_anchor(&source),
        TreeScrollAnchor {
            fallback_index: 50,
            ..anchor
        }
    );
}
#[test]
fn hidden_anchor_chooses_nearest_visible_ancestor_and_deleted_uses_old_position() {
    let mut records = vec![node(1, None), node(101, Some(1)), node(102, Some(101))];
    records.extend((2..=20).map(|k| node(k, None)));
    let mut source = IndexedTreeSource::new(records).unwrap();
    let mut model = grid(&mut source);
    model.set_expanded(&mut source, RowKey(1), true).unwrap();
    model.set_expanded(&mut source, RowKey(101), true).unwrap();
    model.scroll_to(0.0, 2.0 * 28.0 + 3.0).unwrap();
    model.set_expanded(&mut source, RowKey(1), false).unwrap();
    assert_eq!(model.capture_scroll_anchor(&source).key, Some(RowKey(1)));
    assert_eq!(model.scroll_offset().1, 3.0);
    model.scroll_to(0.0, 5.0 * 28.0 + 3.0).unwrap();
    let anchor = model.capture_scroll_anchor(&source);
    let next = source
        .nodes()
        .iter()
        .filter(|n| Some(n.key) != anchor.key)
        .cloned()
        .collect();
    source.replace(next).unwrap();
    model.refresh(&mut source).unwrap();
    model.restore_scroll_anchor(&source, anchor).unwrap();
    assert_eq!(model.scroll_offset().1, 5.0 * 28.0 + 3.0);
}
#[test]
fn indexed_lazy_install_rejects_stale_foreign_invalid_results_before_data_mutation() {
    let mut unloaded = node(1, None);
    unloaded.children = TreeChildren::Unloaded;
    let mut source = IndexedTreeSource::new(vec![unloaded.clone(), node(2, None)]).unwrap();
    let mut model = grid(&mut source);
    let request = model
        .set_expanded(&mut source, RowKey(1), true)
        .unwrap()
        .unwrap();
    assert!(
        model
            .complete_indexed_children(&mut source, request, vec![node(9, Some(999))])
            .is_err()
    );
    assert_eq!(source.revision(), 0);
    assert_eq!(source.child_count(Some(RowKey(1))), None);
    assert!(model.pending_requests().contains(&request));
    let mut other_source = IndexedTreeSource::new(vec![unloaded]).unwrap();
    let mut other = grid(&mut other_source);
    assert!(
        !other
            .complete_indexed_children(&mut other_source, request, vec![node(9, Some(1))])
            .unwrap()
    );
    assert!(
        model
            .complete_indexed_children(&mut source, request, vec![node(9, Some(1))])
            .unwrap()
    );
    assert_eq!(visible(&model, &source), [RowKey(1), RowKey(9), RowKey(2)]);
    assert_eq!(source.revision(), 1);
    assert!(
        !model
            .complete_indexed_children(&mut source, request, vec![node(10, Some(1))])
            .unwrap()
    );
    model.set_expanded(&mut source, RowKey(1), false).unwrap();
    model.set_expanded(&mut source, RowKey(1), true).unwrap();
    assert_eq!(visible(&model, &source), [RowKey(1), RowKey(9), RowKey(2)]);
}
#[test]
fn model_move_is_preflighted_and_columns_navigate_in_display_order() {
    let mut source =
        IndexedTreeSource::new(vec![node(1, None), node(2, None), node(3, Some(1))]).unwrap();
    let mut model = grid(&mut source);
    model
        .select(&source, RowKey(1), SelectionModifiers::default())
        .unwrap();
    assert!(
        model
            .move_nodes(
                &mut source,
                &[RowKey(1)],
                TreeMoveTarget::Into(Some(RowKey(3)))
            )
            .is_err()
    );
    assert_eq!(source.revision(), 0);
    model
        .move_nodes(
            &mut source,
            &[RowKey(2)],
            TreeMoveTarget::Into(Some(RowKey(1))),
        )
        .unwrap();
    assert_eq!(
        model.selected(),
        &std::collections::HashSet::from([RowKey(1)])
    );
    model.set_expanded(&mut source, RowKey(1), true).unwrap();
    assert_eq!(visible(&model, &source), [RowKey(1), RowKey(3), RowKey(2)]);
    model.navigate_column(&source, true).unwrap();
    assert_eq!(model.focused_column(), Some(ColumnId(1)));
    assert_eq!(
        model.selected(),
        &std::collections::HashSet::from([RowKey(1)])
    );
    model.navigate_column(&source, false).unwrap();
    assert_eq!(model.focused_column(), Some(ColumnId(0)));
    assert_eq!(
        model.selected(),
        &std::collections::HashSet::from([RowKey(1)])
    );
}
#[test]
fn moving_a_deep_subtree_rejects_depth_overflow_in_preview() {
    let mut nodes: Vec<_> = (1..=255)
        .map(|k| node(k, (k > 1).then_some(k - 1)))
        .collect();
    nodes.extend([node(1000, None), node(1001, Some(1000))]);
    let source = IndexedTreeSource::new(nodes).unwrap();
    assert!(
        source
            .validate_node_move(&[RowKey(1000)], TreeMoveTarget::Into(Some(RowKey(255))))
            .is_err()
    );
}

#[test]
fn selection_checkpoints_reject_foreign_and_stale_models_and_restore_navigation() {
    let mut source =
        IndexedTreeSource::new(vec![node(1, None), node(2, None), node(3, Some(1))]).unwrap();
    let mut model = grid(&mut source);
    model
        .select(&source, RowKey(1), SelectionModifiers::default())
        .unwrap();
    let snapshot = model.selection_snapshot();
    model
        .focus_cell(
            &source,
            RowKey(2),
            ColumnId(1),
            SelectionModifiers::default(),
        )
        .unwrap();
    model.restore_selection_snapshot(snapshot.clone()).unwrap();
    assert_eq!(model.focused(), Some(RowKey(1)));
    assert_eq!(
        model.selected(),
        &std::collections::HashSet::from([RowKey(1)])
    );
    let mut foreign = grid(&mut source);
    assert!(
        foreign
            .restore_selection_snapshot(snapshot.clone())
            .is_err()
    );
    model.refresh(&mut source).unwrap();
    assert!(model.restore_selection_snapshot(snapshot).is_err());
    model.assign_selected_key(&source, Some(RowKey(3))).unwrap();
    assert_eq!(
        model.selected(),
        &std::collections::HashSet::from([RowKey(3)])
    );
    assert!(!model.is_expanded(RowKey(1)));
}

#[test]
fn excessive_visible_frame_returns_capacity_instead_of_allocating_all_rows() {
    let mut source =
        IndexedTreeSource::new((1..=5000).map(|key| node(key, None)).collect()).unwrap();
    let mut model = grid(&mut source);
    assert!(matches!(
        model.paint(&source, Rect::new(0.0, 0.0, 400.0, 150_000.0)),
        Err(GridError::Capacity)
    ));
}

#[test]
fn malformed_immediate_children_become_retryable_failure_without_pending_worker() {
    struct Source;
    impl TreeDataSource for Source {
        fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
            parent.is_none().then_some(1)
        }
        fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
            (parent.is_none() && index == 0).then_some(RowKey(1))
        }
        fn parent(&self, _: RowKey) -> Option<RowKey> {
            None
        }
        fn position(&self, row: RowKey) -> Option<usize> {
            (row == RowKey(1)).then_some(0)
        }
        fn cell(&self, _: RowKey, _: ColumnId) -> String {
            "Root".into()
        }
        fn request_children(&mut self, _: RowKey, _: RequestId) -> Result<ChildrenLoad, String> {
            Ok(ChildrenLoad::Ready(vec![RowKey(1)]))
        }
    }
    let mut source = Source;
    let mut model = grid(&mut source);
    assert!(model.set_expanded(&mut source, RowKey(1), true).is_err());
    assert!(model.pending_requests().is_empty());
    assert!(model.is_expanded(RowKey(1)));
    let placeholder = model.row(&source, 1).unwrap().unwrap();
    assert!(placeholder.error.is_some());
    assert!(!placeholder.loading);
    assert!(model.set_expanded(&mut source, RowKey(1), true).is_err());
    assert!(model.pending_requests().is_empty());
}
