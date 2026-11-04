// Copyright 2026 rust-ui-engine contributors. Apache-2.0.
// These tests call COM interfaces directly. No HWND, input, focus, UIA events
// or screen-reader process is created or changed.
use super::*;
use crate::{context::ActionHandlerNoMut, window_handle::WindowHandle};
use accesskit::{Node, TreeInfo, TreeUpdate};
use windows::Win32::System::Ole::{SafeArrayDestroy, SafeArrayGetUBound};

struct Realizer {
    context: std::sync::Mutex<Weak<Context>>,
    requests: std::sync::Mutex<Vec<ActionRequest>>,
}
impl ActionHandlerNoMut for Realizer {
    fn do_action(&self, request: ActionRequest) {
        let Some(ActionData::RealizeCell {
            row,
            column,
            generation,
        }) = request.data
        else {
            panic!("Unexpected action")
        };
        assert_eq!(request.action, Action::RealizeCell);
        self.requests.lock().unwrap().push(request.clone());
        let ctx = self.context.lock().unwrap().upgrade().unwrap();
        let mut grid = Node::new(Role::Grid);
        grid.set_row_count(1_000_000);
        grid.set_column_count(16_384);
        grid.set_virtualization_generation(generation);
        grid.add_action(Action::RealizeCell);
        grid.set_children(vec![NodeId(2)]);
        let mut cell = Node::new(Role::Cell);
        cell.set_row_index(row);
        cell.set_column_index(column);
        cell.set_value("Far cell");
        // A synchronous callback would deadlock if Realize held a tree lock.
        *ctx.tree.write().unwrap() = Tree::new(
            TreeUpdate {
                nodes: vec![(NodeId(1), grid), (NodeId(2), cell)],
                tree: Some(TreeInfo::new(NodeId(1))),
                tree_id: TreeId::ROOT,
                focus: NodeId(1),
            },
            false,
        );
    }
}

fn virtual_snapshot() -> TreeUpdate {
    let mut grid = Node::new(Role::Grid);
    grid.set_row_count(1_000_000);
    grid.set_column_count(16_384);
    grid.set_virtualization_generation(7);
    grid.add_action(Action::RealizeCell);
    TreeUpdate {
        nodes: vec![(NodeId(1), grid)],
        tree: Some(TreeInfo::new(NodeId(1))),
        tree_id: TreeId::ROOT,
        focus: NodeId(1),
    }
}

#[test]
fn com_virtual_far_cell_realization_is_bounded_and_reentrant_without_window() {
    let handler = Arc::new(Realizer {
        context: Default::default(),
        requests: Default::default(),
    });
    let ctx = Context::new(
        WindowHandle(HWND::default()),
        Tree::new(virtual_snapshot(), false),
        handler.clone(),
        false,
    );
    *handler.context.lock().unwrap() = Arc::downgrade(&ctx);
    let table = root(&ctx);
    let grid: IGridProvider = pattern(&table, UIA_GridPatternId);
    let container: IItemContainerProvider = pattern(&table, UIA_ItemContainerPatternId);
    unsafe {
        let placeholder = container
            .FindItemByProperty(None, UIA_PROPERTY_ID(0), &VARIANT::default())
            .unwrap();
        assert!(handler.requests.lock().unwrap().is_empty());
        assert_eq!(
            placeholder
                .GetPropertyValue(UIA_ValueValuePropertyId)
                .err()
                .unwrap()
                .code(),
            HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
        let item: IGridItemProvider = pattern(&placeholder, UIA_GridItemPatternId);
        assert_eq!((item.Row().unwrap(), item.Column().unwrap()), (0, 0));
        let virtual_item: IVirtualizedItemProvider =
            pattern(&placeholder, UIA_VirtualizedItemPatternId);
        virtual_item.Realize().unwrap();
        assert_eq!(handler.requests.lock().unwrap().len(), 1);
        let value: IValueProvider = pattern(&placeholder, UIA_ValuePatternId);
        assert_eq!(value.Value().unwrap().to_string(), "Far cell");
        virtual_item.Realize().unwrap();
        assert_eq!(
            handler.requests.lock().unwrap().len(),
            1,
            "already materialized is a no-op"
        );
        let far = grid.GetItem(999_999, 16_383).unwrap();
        let far: IGridItemProvider = pattern(&far, UIA_GridItemPatternId);
        assert_eq!(
            (far.Row().unwrap(), far.Column().unwrap()),
            (999_999, 16_383)
        );
        assert_eq!(handler.requests.lock().unwrap().len(), 2);
        drop(ctx);
        assert_eq!(
            virtual_item.Realize().unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
    }
}

#[test]
fn com_virtual_enumeration_skips_merged_aliases_and_rejects_stale_generations() {
    let mut update = virtual_snapshot();
    update.nodes[0].1.set_row_count(10);
    update.nodes[0].1.set_column_count(10);
    update.nodes[0]
        .1
        .set_virtual_cell_spans(vec![accesskit::VirtualCellSpan {
            row: 2,
            column: 0,
            rows: 7,
            columns: 10,
        }]);
    let ctx = context(update.clone());
    let table = root(&ctx);
    let container: IItemContainerProvider = pattern(&table, UIA_ItemContainerPatternId);
    unsafe {
        let mut merged = container
            .FindItemByProperty(None, UIA_PROPERTY_ID(0), &VARIANT::default())
            .unwrap();
        for _ in 0..20 {
            merged = container
                .FindItemByProperty(&merged, UIA_PROPERTY_ID(0), &VARIANT::default())
                .unwrap();
        }
        let item: IGridItemProvider = pattern(&merged, UIA_GridItemPatternId);
        assert_eq!(
            (
                item.Row().unwrap(),
                item.Column().unwrap(),
                item.RowSpan().unwrap(),
                item.ColumnSpan().unwrap()
            ),
            (2, 0, 7, 10)
        );
        let next = container
            .FindItemByProperty(&merged, UIA_PROPERTY_ID(0), &VARIANT::default())
            .unwrap();
        let item: IGridItemProvider = pattern(&next, UIA_GridItemPatternId);
        assert_eq!((item.Row().unwrap(), item.Column().unwrap()), (9, 0));
        assert_eq!(
            container
                .FindItemByProperty(None, UIA_NamePropertyId, &VARIANT::default())
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        let mut last = next.clone();
        for _ in 0..9 {
            last = container
                .FindItemByProperty(&last, UIA_PROPERTY_ID(0), &VARIANT::default())
                .unwrap();
        }
        assert!(
            container
                .FindItemByProperty(&last, UIA_PROPERTY_ID(0), &VARIANT::default())
                .is_err()
        );
        update.nodes[0].1.set_virtualization_generation(8);
        *ctx.tree.write().unwrap() = Tree::new(update, false);
        assert_eq!(
            item.Row().unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
    }
}

#[test]
fn com_virtual_disabled_actions_do_not_dispatch_and_invalid_spans_fail() {
    let mut update = virtual_snapshot();
    update.nodes[0].1.remove_action(Action::RealizeCell);
    let ctx = context(update.clone());
    let table = root(&ctx);
    let grid: IGridProvider = pattern(&table, UIA_GridPatternId);
    let container: IItemContainerProvider = pattern(&table, UIA_ItemContainerPatternId);
    unsafe {
        assert_eq!(
            grid.GetItem(100, 100).unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTENABLED as _)
        );
        let item = container
            .FindItemByProperty(None, UIA_PROPERTY_ID(0), &VARIANT::default())
            .unwrap();
        let virtual_item: IVirtualizedItemProvider = pattern(&item, UIA_VirtualizedItemPatternId);
        assert_eq!(
            virtual_item.Realize().unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTENABLED as _)
        );
        update.nodes[0]
            .1
            .set_virtual_cell_spans(vec![accesskit::VirtualCellSpan {
                row: 0,
                column: 0,
                rows: usize::MAX,
                columns: 2,
            }]);
        *ctx.tree.write().unwrap() = Tree::new(update, false);
        assert_eq!(grid.GetItem(0, 0).unwrap_err().code(), E_INVALIDARG);
    }
}

struct NoActions;
impl ActionHandlerNoMut for NoActions {
    fn do_action(&self, _: ActionRequest) {
        panic!("Read-only table query dispatched an action")
    }
}

fn snapshot() -> TreeUpdate {
    let mut grid = Node::new(Role::Grid);
    grid.set_label("Test grid");
    grid.set_row_count(3);
    grid.set_column_count(4);
    grid.set_children(vec![NodeId(2), NodeId(3), NodeId(4), NodeId(8)]);
    let mut a = Node::new(Role::ColumnHeader);
    a.set_label("A–B");
    a.set_column_index(0);
    a.set_column_span(2);
    let mut b = Node::new(Role::ColumnHeader);
    b.set_label("C");
    b.set_column_index(2);
    let mut row = Node::new(Role::Row);
    row.set_children(vec![NodeId(5), NodeId(6), NodeId(7)]);
    let mut c = Node::new(Role::Cell);
    c.set_label("Merged");
    c.set_row_index(0);
    c.set_column_index(0);
    c.set_row_span(2);
    c.set_column_span(2);
    let mut empty = Node::new(Role::GridCell);
    empty.set_row_index(0);
    empty.set_column_index(2);
    empty.set_value("");
    let mut rh = Node::new(Role::RowHeader);
    rh.set_row_index(0);
    rh.set_label("First row");
    let mut hidden = Node::new(Role::GenericContainer);
    hidden.set_hidden();
    hidden.set_children(vec![NodeId(9)]);
    let mut hc = Node::new(Role::Cell);
    hc.set_row_index(2);
    hc.set_column_index(0);
    TreeUpdate {
        nodes: vec![
            (NodeId(1), grid),
            (NodeId(2), a),
            (NodeId(3), b),
            (NodeId(4), row),
            (NodeId(5), c),
            (NodeId(6), empty),
            (NodeId(7), rh),
            (NodeId(8), hidden),
            (NodeId(9), hc),
        ],
        tree: Some(TreeInfo::new(NodeId(1))),
        tree_id: TreeId::ROOT,
        focus: NodeId(1),
    }
}

fn context(update: TreeUpdate) -> Arc<Context> {
    Context::new(
        WindowHandle(HWND::default()),
        Tree::new(update, false),
        Arc::new(NoActions),
        false,
    )
}
fn root(ctx: &Arc<Context>) -> IRawElementProviderSimple {
    let id = ctx.read_tree().state().root_id();
    ctx.get_or_create_platform_node(id).into_interface()
}
fn pattern<T: Interface>(node: &IRawElementProviderSimple, id: UIA_PATTERN_ID) -> T {
    unsafe { node.GetPatternProvider(id).unwrap().cast().unwrap() }
}
unsafe fn array_len(array: *mut SAFEARRAY) -> i32 {
    assert!(!array.is_null());
    let len = unsafe { SafeArrayGetUBound(array, 1).unwrap() + 1 };
    unsafe { SafeArrayDestroy(array).unwrap() };
    len
}

#[test]
fn com_grid_counts_getitem_spans_empty_and_headers_without_window() {
    let ctx = context(snapshot());
    let node = root(&ctx);
    let grid: IGridProvider = pattern(&node, UIA_GridPatternId);
    let table: ITableProvider = pattern(&node, UIA_TablePatternId);
    unsafe {
        assert_eq!(grid.RowCount().unwrap(), 3);
        assert_eq!(grid.ColumnCount().unwrap(), 4);
        assert_eq!(table.RowOrColumnMajor().unwrap(), RowOrColumnMajor_RowMajor);
        assert_eq!(array_len(table.GetColumnHeaders().unwrap()), 2);
        assert_eq!(array_len(table.GetRowHeaders().unwrap()), 1);
        let a = grid.GetItem(0, 0).unwrap();
        let b = grid.GetItem(1, 1).unwrap();
        assert_eq!(a, b);
        let item: IGridItemProvider = pattern(&a, UIA_GridItemPatternId);
        assert_eq!(
            (
                item.Row().unwrap(),
                item.Column().unwrap(),
                item.RowSpan().unwrap(),
                item.ColumnSpan().unwrap()
            ),
            (0, 0, 2, 2)
        );
        assert_eq!(item.ContainingGrid().unwrap(), node);
        let ti: ITableItemProvider = pattern(&a, UIA_TableItemPatternId);
        assert_eq!(array_len(ti.GetColumnHeaderItems().unwrap()), 1);
        assert_eq!(array_len(ti.GetRowHeaderItems().unwrap()), 1);
        assert!(
            grid.GetItem(0, 2).is_ok(),
            "empty materialized cell must have a provider"
        );
        for (r, c) in [(-1, 0), (0, -1), (3, 0), (0, 4), (i32::MAX, 0)] {
            assert_eq!(grid.GetItem(r, c).unwrap_err().code(), E_INVALIDARG);
        }
        assert_eq!(
            grid.GetItem(2, 0).unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
        assert!(node.GetPatternProvider(UIA_GridItemPatternId).is_err());
    }
}

#[test]
fn com_retained_provider_tracks_reorder_and_rejects_removal_or_owner_drop() {
    let ctx = context(snapshot());
    let node = root(&ctx);
    let grid: IGridProvider = pattern(&node, UIA_GridPatternId);
    let c = unsafe { grid.GetItem(0, 2).unwrap() };
    let item: IGridItemProvider = pattern(&c, UIA_GridItemPatternId);
    let mut update = snapshot();
    update
        .nodes
        .iter_mut()
        .find(|n| n.0 == NodeId(6))
        .unwrap()
        .1
        .set_row_index(2);
    *ctx.tree.write().unwrap() = Tree::new(update, false);
    unsafe {
        assert_eq!(item.Row().unwrap(), 2);
        assert_eq!(grid.GetItem(2, 2).unwrap(), c);
    }
    let mut update = snapshot();
    update.nodes.retain(|n| n.0 != NodeId(6));
    update
        .nodes
        .iter_mut()
        .find(|n| n.0 == NodeId(4))
        .unwrap()
        .1
        .set_children(vec![NodeId(5), NodeId(7)]);
    *ctx.tree.write().unwrap() = Tree::new(update, false);
    unsafe {
        assert_eq!(
            item.Row().unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
    }
    drop(ctx);
    unsafe {
        assert_eq!(
            grid.RowCount().unwrap_err().code(),
            HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
    }
}

#[test]
fn nested_grid_is_not_part_of_parent_and_invalid_geometry_has_no_pattern() {
    let mut update = snapshot();
    let mut nested = Node::new(Role::Table);
    nested.set_row_count(1);
    nested.set_column_count(1);
    nested.set_children(vec![NodeId(11)]);
    let mut cell = Node::new(Role::Cell);
    cell.set_row_index(0);
    cell.set_column_index(0);
    update.nodes[0].1.push_child(NodeId(10));
    update
        .nodes
        .extend([(NodeId(10), nested), (NodeId(11), cell)]);
    let ctx = context(update);
    let node = root(&ctx);
    let grid: IGridProvider = pattern(&node, UIA_GridPatternId);
    unsafe {
        assert!(
            grid.GetItem(0, 0).is_ok(),
            "nested duplicate coordinates must be isolated"
        );
    }
    let mut update = snapshot();
    update.nodes[0].1.set_row_count(usize::MAX);
    *ctx.tree.write().unwrap() = Tree::new(update, false);
    unsafe {
        assert!(node.GetPatternProvider(UIA_GridPatternId).is_err());
    }
    let mut update = snapshot();
    update
        .nodes
        .iter_mut()
        .find(|n| n.0 == NodeId(5))
        .unwrap()
        .1
        .set_column_span(0);
    *ctx.tree.write().unwrap() = Tree::new(update, false);
    unsafe {
        assert!(grid.GetItem(0, 0).is_err());
    }
}

#[test]
fn duplicate_coordinates_fail_instead_of_returning_arbitrary_cell() {
    let mut update = snapshot();
    let n = &mut update
        .nodes
        .iter_mut()
        .find(|n| n.0 == NodeId(6))
        .unwrap()
        .1;
    n.set_column_index(0);
    let ctx = context(update);
    let grid: IGridProvider = pattern(&root(&ctx), UIA_GridPatternId);
    unsafe {
        assert_eq!(
            grid.GetItem(0, 0).unwrap_err().code(),
            HRESULT(UIA_E_INVALIDOPERATION as _)
        );
    }
}

#[test]
fn properties_empty_table_and_dimension_change_notification_are_consistent() {
    let ctx = context(snapshot());
    let node = root(&ctx);
    unsafe {
        let value = node.GetPropertyValue(UIA_GridRowCountPropertyId).unwrap();
        assert_eq!(value.Anonymous.Anonymous.Anonymous.lVal, 3);
        let array = node
            .GetPropertyValue(UIA_TableColumnHeadersPropertyId)
            .unwrap();
        assert_eq!(array.Anonymous.Anonymous.vt, VT_ARRAY | VT_UNKNOWN);
        assert_eq!(
            SafeArrayGetUBound(array.Anonymous.Anonymous.Anonymous.parray, 1).unwrap(),
            1
        );
    }
    let before = Tree::new(snapshot(), false);
    let mut update = snapshot();
    update.nodes[0].1.set_row_count(4);
    let after = Tree::new(update, false);
    let mut old_buf = Vec::new();
    let mut new_buf = Vec::new();
    let old_node = before.state().root();
    let new_node = after.state().root();
    let old = NodeWrapper {
        node: &old_node,
        string_buffer: &mut old_buf,
    };
    let new = NodeWrapper {
        node: &new_node,
        string_buffer: &mut new_buf,
    };
    let mut events = Vec::new();
    new.enqueue_grid_property_changes(&mut events, &node, &old);
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0],QueuedEvent::PropertyChanged{property_id,..} if *property_id==UIA_GridRowCountPropertyId)
    );
    let mut empty = snapshot();
    empty.nodes.truncate(1);
    empty.nodes[0].1.set_children(vec![]);
    empty.nodes[0].1.set_row_count(0);
    empty.nodes[0].1.set_column_count(0);
    *ctx.tree.write().unwrap() = Tree::new(empty, false);
    let grid: IGridProvider = pattern(&node, UIA_GridPatternId);
    let table: ITableProvider = pattern(&node, UIA_TablePatternId);
    unsafe {
        assert_eq!(grid.RowCount().unwrap(), 0);
        assert_eq!(array_len(table.GetColumnHeaders().unwrap()), 0);
        assert!(grid.GetItem(0, 0).is_err());
    }
}
