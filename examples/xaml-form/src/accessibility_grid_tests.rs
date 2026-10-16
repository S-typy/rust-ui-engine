use super::super::Accessibility;
use super::*;
use crate::view::MountedView;
use rust_desktop_ui_controls::{DataSourceHandle, GridControlSettings, Theme};
use rust_desktop_ui_core::{LayoutStyle, Length, Mutation, Size};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataQuery, DataRow, DataSource, DataTable, FieldSchema,
    FieldType, SortDescriptor, SortDirection, TableSchema,
};
use std::{cell::RefCell, rc::Rc};

const VIEWPORT: Size = Size::new(620.0, 260.0);

struct Fixture {
    ui: Controls,
    view: MountedView,
    accessibility: Accessibility,
    grid: WidgetId,
    source: DataSourceHandle,
}

fn key(index: usize) -> i64 {
    match index {
        0 => i64::MIN,
        1 => i64::MAX,
        2 => 9_007_199_254_740_993,
        _ => index as i64,
    }
}

fn row_key(key: i64) -> RowKey {
    RowKey(u64::from_ne_bytes(key.to_ne_bytes()))
}

impl Fixture {
    fn new(count: usize) -> Self {
        let mut locked = FieldSchema::new("Locked", FieldType::Text);
        locked.read_only = true;
        let schema = TableSchema::new(vec![
            FieldSchema::new("Name", FieldType::Text),
            FieldSchema::new("Amount", FieldType::Integer),
            locked,
        ])
        .unwrap();
        let table = DataTable::new(
            schema,
            (0..count)
                .map(|index| {
                    DataRow::new(
                        key(index),
                        vec![
                            CellValue::Text(format!("Row {index}")),
                            CellValue::Integer(index as i64),
                            CellValue::Text("Fixed".into()),
                        ],
                    )
                })
                .collect(),
        )
        .unwrap();
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table)).unwrap(),
        ));
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let grid = ui
            .grid_control(
                ui.root(),
                Rc::clone(&source),
                GridControlSettings::default(),
                LayoutStyle {
                    width: Length::Px(VIEWPORT.width),
                    height: Length::Px(220.0),
                    ..Default::default()
                },
            )
            .unwrap();
        Self {
            ui,
            grid,
            source,
            view: MountedView::default(),
            accessibility: Accessibility::default(),
        }
    }

    fn tree(&mut self) -> accesskit::TreeUpdate {
        self.ui.update(VIEWPORT).unwrap();
        self.accessibility
            .tree(&self.ui, &self.view, "Data grid", 1.5)
    }

    fn id(&self, row: i64, column: Option<u64>) -> NodeId {
        self.accessibility.grids.ids[&Key {
            widget: self.grid,
            part: match column {
                Some(column) => Part::Cell(row_key(row), ColumnId(column)),
                None => Part::Row(row_key(row)),
            },
        }]
    }

    fn action(&mut self, target: NodeId, action: Action, data: Option<ActionData>) {
        self.accessibility
            .action(
                &mut self.ui,
                ActionRequest {
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: target,
                    action,
                    data,
                },
            )
            .unwrap();
    }
}

fn node(tree: &accesskit::TreeUpdate, id: NodeId) -> &Node {
    &tree
        .nodes
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .unwrap()
        .1
}

#[test]
fn editable_display_cells_describe_the_real_editor_and_respect_read_only() {
    let mut fixture = Fixture::new(2);
    let tree = fixture.tree();
    let amount = fixture.id(key(0), Some(1));
    let locked = fixture.id(key(0), Some(2));
    assert_eq!(node(&tree, amount).role(), Role::Cell);
    assert_eq!(node(&tree, amount).value(), Some("0"));
    assert_eq!(
        node(&tree, amount).description(),
        Some("F2 — редактировать значение")
    );
    assert_eq!(node(&tree, locked).description(), None);
    fixture.ui.set_grid_read_only(fixture.grid, true).unwrap();
    let tree = fixture.tree();
    assert_eq!(node(&tree, amount).description(), None);
    assert!(!node(&tree, amount).supports_action(Action::SetValue));
}

#[test]
fn set_value_limit_rejection_cancels_session_without_failing_the_host() {
    let mut fixture = Fixture::new(2);
    fixture.tree();
    fixture
        .ui
        .begin_grid_edit(fixture.grid, key(0), "Amount")
        .unwrap();
    let editor = fixture.ui.grid_editor(fixture.grid).unwrap();
    fixture.ui.cancel_grid_edit(fixture.grid).unwrap();
    fixture.ui.set_max_length(editor, Some(3)).unwrap();
    fixture.tree();
    let cell = fixture.id(key(0), Some(1));
    let revision = fixture.source.borrow().revision();
    fixture.action(
        cell,
        Action::SetValue,
        Some(ActionData::Value("1234".into())),
    );
    assert_eq!(fixture.ui.grid_editor(fixture.grid), None);
    assert_eq!(fixture.ui.document(editor).unwrap().text(), "0");
    assert_eq!(fixture.source.borrow().revision(), revision);
    assert_eq!(
        fixture.source.borrow().value(key(0), "Amount"),
        Some(&CellValue::Integer(0))
    );
    fixture.action(
        cell,
        Action::SetValue,
        Some(ActionData::Value("123".into())),
    );
    assert_eq!(
        fixture.source.borrow().value(key(0), "Amount"),
        Some(&CellValue::Integer(123))
    );
    assert_eq!(fixture.ui.grid_editor(fixture.grid), None);
}

#[test]
fn stale_set_value_cannot_open_an_oversized_binary_editor() {
    let mut fixture = Fixture::new(2);
    fixture.tree();
    let cell = fixture.id(key(0), Some(0));
    let payload = vec![0xab; 600 * 1024];
    let table = DataTable::new(
        TableSchema::new(vec![
            FieldSchema::new("Name", FieldType::Binary),
            FieldSchema::new("Amount", FieldType::Integer),
            FieldSchema::new("Locked", FieldType::Text),
        ])
        .unwrap(),
        vec![DataRow::new(
            key(0),
            vec![
                CellValue::Binary(payload.clone()),
                CellValue::Integer(0),
                CellValue::Text("Fixed".into()),
            ],
        )],
    )
    .unwrap();
    fixture
        .source
        .borrow_mut()
        .replace_adapter(CollectionAdapter::new(table), DataQuery::default())
        .unwrap();
    let revision = fixture.source.borrow().revision();
    fixture.action(
        cell,
        Action::SetValue,
        Some(ActionData::Value("0x00".into())),
    );
    assert_eq!(fixture.ui.grid_editor(fixture.grid), None);
    assert_eq!(fixture.source.borrow().revision(), revision);
    assert_eq!(
        fixture.source.borrow().value(key(0), "Name"),
        Some(&CellValue::Binary(payload))
    );
}

#[test]
fn grid_projects_headers_visible_rows_typed_text_and_read_only_cells() {
    let mut fixture = Fixture::new(10_000);
    let tree = fixture.tree();
    let grid = node(&tree, fixture.accessibility.id(fixture.grid).unwrap());
    assert_eq!(grid.role(), Role::Grid);
    assert_eq!(grid.row_count(), Some(10_000));
    assert_eq!(grid.column_count(), Some(3));
    assert!(grid.supports_action(Action::ScrollDown));
    let row = node(&tree, fixture.id(i64::MIN, None));
    assert_eq!(row.role(), Role::Row);
    assert_eq!(row.row_index(), Some(0));
    let cell = node(&tree, fixture.id(i64::MIN, Some(0)));
    assert_eq!(cell.role(), Role::Cell);
    assert_eq!(cell.label(), Some("Row 0"));
    assert_eq!(cell.value(), Some("Row 0"));
    assert_eq!(cell.column_index(), Some(0));
    assert!(cell.supports_action(Action::SetValue));
    let locked = node(&tree, fixture.id(i64::MIN, Some(2)));
    assert!(locked.is_read_only());
    assert!(!locked.supports_action(Action::SetValue));
    assert_eq!(
        tree.nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::ColumnHeader)
            .count(),
        3
    );
    assert!(fixture.accessibility.grids.ids.len() < 80);
    let ids: HashSet<_> = tree.nodes.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids.len(), tree.nodes.len());
    for (_, node) in &tree.nodes {
        assert!(node.children().iter().all(|child| ids.contains(child)));
    }
}

#[test]
fn extreme_signed_keys_have_distinct_stable_ids_after_sort_and_selection() {
    let mut fixture = Fixture::new(3);
    fixture.tree();
    let ids: Vec<_> = (0..3)
        .map(|index| fixture.id(key(index), Some(0)))
        .collect();
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 3);
    fixture.action(ids[2], Action::Click, None);
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), Some(key(2)));
    let tree = fixture.tree();
    assert_eq!(tree.focus, ids[2]);
    assert_eq!(node(&tree, ids[2]).is_selected(), Some(true));
    fixture
        .source
        .borrow_mut()
        .set_query(DataQuery {
            sort: vec![SortDescriptor {
                field: "Amount".into(),
                direction: SortDirection::Descending,
            }],
            filter: String::new(),
        })
        .unwrap();
    let tree = fixture.tree();
    for (index, expected) in ids.iter().enumerate() {
        assert_eq!(fixture.id(key(index), Some(0)), *expected);
    }
    assert_eq!(node(&tree, ids[2]).row_index(), Some(0));
}

#[test]
fn virtual_focus_and_set_value_preserve_the_real_editor_focus_and_invalid_draft() {
    let mut fixture = Fixture::new(3);
    fixture.tree();
    let amount = fixture.id(i64::MIN, Some(1));
    fixture.action(
        amount,
        Action::SetValue,
        Some(ActionData::Value("42".into())),
    );
    assert_eq!(
        fixture.source.borrow().value(i64::MIN, "Amount"),
        Some(&CellValue::Integer(42))
    );
    assert!(fixture.ui.grid_editor(fixture.grid).is_none());
    fixture.tree();
    fixture.action(
        amount,
        Action::SetValue,
        Some(ActionData::Value("invalid".into())),
    );
    let editor = fixture.ui.grid_editor(fixture.grid).unwrap();
    let tree = fixture.tree();
    assert_eq!(tree.focus, fixture.accessibility.id(editor).unwrap());
    assert_eq!(node(&tree, tree.focus).role(), Role::TextInput);
    assert_eq!(
        node(&tree, tree.focus).invalid(),
        Some(accesskit::Invalid::True)
    );
    assert_eq!(fixture.ui.document(editor).unwrap().text(), "invalid");
    fixture.action(
        amount,
        Action::SetValue,
        Some(ActionData::Value("55".into())),
    );
    fixture.action(fixture.id(i64::MAX, Some(0)), Action::Click, None);
    fixture.action(
        fixture.accessibility.id(fixture.grid).unwrap(),
        Action::ScrollDown,
        None,
    );
    assert_eq!(fixture.ui.document(editor).unwrap().text(), "invalid");
    assert_eq!(fixture.ui.grid_editor(fixture.grid), Some(editor));
    assert_eq!(
        fixture.source.borrow().value(i64::MIN, "Amount"),
        Some(&CellValue::Integer(42))
    );
    let tree = fixture.tree();
    assert_eq!(tree.focus, fixture.accessibility.id(editor).unwrap());
}

#[test]
fn read_only_disabled_and_malformed_requests_do_not_start_cell_edits() {
    let mut fixture = Fixture::new(3);
    fixture.tree();
    let editable = fixture.id(i64::MIN, Some(0));
    let locked = fixture.id(i64::MIN, Some(2));
    fixture.action(
        locked,
        Action::SetValue,
        Some(ActionData::Value("Changed".into())),
    );
    fixture.action(
        editable,
        Action::SetValue,
        Some(ActionData::NumericValue(12.0)),
    );
    fixture.action(NodeId(u64::MAX), Action::Click, None);
    assert!(fixture.ui.grid_editor(fixture.grid).is_none());
    fixture.ui.set_grid_read_only(fixture.grid, true).unwrap();
    fixture.action(
        editable,
        Action::SetValue,
        Some(ActionData::Value("Changed".into())),
    );
    let tree = fixture.tree();
    assert!(node(&tree, editable).is_read_only());
    assert!(!node(&tree, editable).supports_action(Action::SetValue));
    fixture.ui.set_grid_read_only(fixture.grid, false).unwrap();
    fixture.ui.set_enabled(fixture.grid, false).unwrap();
    fixture.action(editable, Action::Click, None);
    fixture.action(
        editable,
        Action::SetValue,
        Some(ActionData::Value("Changed".into())),
    );
    let tree = fixture.tree();
    assert!(node(&tree, editable).is_disabled());
    assert!(!node(&tree, editable).supports_action(Action::Focus));
    assert!(fixture.ui.grid_editor(fixture.grid).is_none());
    assert_eq!(
        fixture.source.borrow().value(i64::MIN, "Name"),
        Some(&CellValue::Text("Row 0".into()))
    );
}

#[test]
fn scrolling_reclaims_virtual_nodes_and_stale_ids_never_retarget_rows() {
    let mut fixture = Fixture::new(2000);
    fixture.tree();
    let stale = fixture.id(i64::MIN, Some(0));
    for offset in [500.0, 1000.0, 5000.0, 10000.0, 20000.0] {
        fixture
            .ui
            .grid_scroll_to(fixture.grid, 0.0, offset)
            .unwrap();
        fixture.tree();
        assert!(fixture.accessibility.grids.ids.len() < 80);
        fixture.action(stale, Action::Click, None);
        assert_eq!(fixture.ui.grid_selected_key(fixture.grid), None);
    }
    fixture.ui.grid_scroll_to(fixture.grid, 0.0, 0.0).unwrap();
    fixture.tree();
    let fresh = fixture.id(i64::MIN, Some(0));
    assert_ne!(fresh, stale);
    fixture.action(stale, Action::Click, None);
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), None);
    fixture.action(fresh, Action::Click, None);
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), Some(i64::MIN));
}

#[test]
fn filtering_hidden_and_removed_grid_requests_are_ignored() {
    let mut fixture = Fixture::new(3);
    fixture.tree();
    let stale = fixture.id(i64::MIN, Some(0));
    // Reject a disappeared query row even before the next projected tree.
    fixture
        .source
        .borrow_mut()
        .set_query(DataQuery {
            filter: "Row 1".into(),
            ..Default::default()
        })
        .unwrap();
    fixture.action(stale, Action::Click, None);
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), None);
    fixture.tree();
    let visible = fixture.id(i64::MAX, Some(0));
    let mut props = fixture
        .ui
        .runtime
        .tree()
        .node(fixture.grid)
        .unwrap()
        .props
        .clone();
    props.visible = false;
    fixture
        .ui
        .runtime
        .apply(Mutation::SetProps {
            node: fixture.grid,
            props,
        })
        .unwrap();
    fixture.ui.update(VIEWPORT).unwrap();
    fixture.action(visible, Action::Click, None);
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), None);
    fixture.tree();
    assert!(fixture.accessibility.grids.ids.is_empty());
    fixture.ui.remove(fixture.grid).unwrap();
    fixture.action(
        visible,
        Action::SetValue,
        Some(ActionData::Value("Changed".into())),
    );
}

#[test]
fn actual_cell_editor_focus_is_not_replaced_by_the_grid_selection() {
    let mut fixture = Fixture::new(3);
    fixture.tree();
    fixture.action(fixture.id(i64::MIN, Some(0)), Action::Click, None);
    fixture
        .ui
        .begin_grid_edit(fixture.grid, i64::MIN, "Name")
        .unwrap();
    let editor = fixture.ui.grid_editor(fixture.grid).unwrap();
    let tree = fixture.tree();
    assert_eq!(tree.focus, fixture.accessibility.id(editor).unwrap());
    let outside = fixture
        .ui
        .edit_box(
            fixture.ui.root(),
            "Outside",
            "Draft",
            LayoutStyle::default(),
        )
        .unwrap();
    fixture
        .ui
        .perform_action(
            outside,
            rust_desktop_ui_controls::SemanticAction::Focus,
            None,
        )
        .unwrap();
    let tree = fixture.tree();
    assert_eq!(tree.focus, fixture.accessibility.id(outside).unwrap());
}

#[test]
fn grid_group_actions_summary_and_key_collision_never_retarget_stale_virtual_ids() {
    use rust_desktop_ui_controls::{GridPresentationSettings, GridRowKind, GridSummary};
    use rust_desktop_ui_data::{Aggregate, AggregateFunction, DataWrite};
    let mut fixture = Fixture::new(3);
    fixture
        .ui
        .set_grid_selected_key(fixture.grid, Some(key(0)))
        .unwrap();
    fixture
        .ui
        .set_grid_presentation(
            fixture.grid,
            GridPresentationSettings {
                group_fields: vec!["Name".into()],
                summaries: vec![GridSummary {
                    title: "Rows".into(),
                    aggregate: Aggregate {
                        function: AggregateFunction::CountRows,
                        expression: None,
                    },
                }],
                ..Default::default()
            },
        )
        .unwrap();
    let tree = fixture.tree();
    let group = fixture
        .ui
        .grid_frame(fixture.grid)
        .unwrap()
        .semantics
        .rows
        .iter()
        .find(|r| {
            matches!(
                fixture.ui.grid_row_kind(fixture.grid, r.key),
                Some(GridRowKind::Group { .. })
            )
        })
        .unwrap()
        .key;
    let group_id = fixture.accessibility.grids.ids[&Key {
        widget: fixture.grid,
        part: Part::Row(group),
    }];
    assert_eq!(node(&tree, group_id).is_expanded(), Some(true));
    assert_eq!(node(&tree, group_id).level(), Some(0));
    assert!(tree.nodes.iter().any(|(_, n)| n.label() == Some("Rows: 3")));
    fixture.action(group_id, Action::Focus, None);
    let focused = fixture.tree();
    assert_eq!(focused.focus, group_id);
    assert!(node(&focused, group_id).supports_action(Action::Collapse));
    assert_eq!(node(&focused, group_id).is_expanded(), Some(true));
    fixture.action(group_id, Action::Collapse, None);
    let collapsed = fixture.tree();
    assert_eq!(collapsed.focus, group_id);
    assert!(node(&collapsed, group_id).supports_action(Action::Expand));
    assert_eq!(node(&collapsed, group_id).is_expanded(), Some(false));
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), Some(key(0)));
    fixture.action(group_id, Action::Expand, None);
    fixture.tree();
    let collision = i64::from_ne_bytes(group.0.to_ne_bytes());
    fixture
        .source
        .borrow_mut()
        .write_batch(&[DataWrite::Insert(DataRow::new(
            collision,
            vec![
                CellValue::Text("AAA".into()),
                CellValue::Integer(7),
                CellValue::Text("Fixed".into()),
            ],
        ))])
        .unwrap();
    fixture.action(group_id, Action::Focus, None);
    assert_eq!(fixture.ui.grid_selected_key(fixture.grid), Some(key(0)));
    let rebuilt = fixture.tree();
    let record = fixture.accessibility.grids.ids[&Key {
        widget: fixture.grid,
        part: Part::Row(group),
    }];
    assert_ne!(record, group_id);
    assert_eq!(node(&rebuilt, record).label(), Some("AAA · 7 · Fixed"));
}

#[test]
fn grid_headers_and_cells_follow_pinned_visual_order_and_cards_omit_table_headers() {
    use rust_desktop_ui_controls::{GridPresentationSettings, GridViewMode};
    let mut fixture = Fixture::new(3);
    fixture
        .ui
        .pin_grid_column(fixture.grid, "Amount", true)
        .unwrap();
    let tree = fixture.tree();
    let header = fixture.accessibility.grids.ids[&Key {
        widget: fixture.grid,
        part: Part::HeaderRow,
    }];
    let labels: Vec<_> = node(&tree, header)
        .children()
        .iter()
        .map(|id| node(&tree, *id).label().unwrap())
        .collect();
    assert_eq!(labels, ["Amount", "Name", "Locked"]);
    let row = fixture.id(key(0), None);
    let cells = node(&tree, row).children();
    assert_eq!(
        cells
            .iter()
            .map(|id| node(&tree, *id).column_index().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(node(&tree, row).label(), Some("0 · Row 0 · Fixed"));
    fixture
        .ui
        .set_grid_presentation(
            fixture.grid,
            GridPresentationSettings {
                view: GridViewMode::Cards,
                ..Default::default()
            },
        )
        .unwrap();
    let tree = fixture.tree();
    assert!(!fixture.accessibility.grids.ids.contains_key(&Key {
        widget: fixture.grid,
        part: Part::HeaderRow
    }));
    let row = fixture.id(key(0), None);
    assert_eq!(node(&tree, row).children().len(), 3);
}

#[test]
fn grid_virtual_set_value_uses_typed_numeric_recipe_without_float_conversion() {
    use rust_desktop_ui_controls::DataFormFactories;
    let mut fixture = Fixture::new(2);
    fixture
        .ui
        .set_grid_editor_factory(
            fixture.grid,
            "Amount",
            "Auto",
            &DataFormFactories::default(),
        )
        .unwrap();
    fixture.tree();
    let cell = fixture.id(key(0), Some(1));
    fixture.action(
        cell,
        Action::SetValue,
        Some(ActionData::Value("9007199254740993".into())),
    );
    assert_eq!(
        fixture.source.borrow().value(key(0), "Amount"),
        Some(&CellValue::Integer(9_007_199_254_740_993))
    );
    assert!(fixture.ui.grid_editor(fixture.grid).is_none());
}
