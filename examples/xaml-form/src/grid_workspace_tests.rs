use super::*;
use rust_desktop_ui_controls::Theme;
use rust_desktop_ui_core::Size;
use rust_desktop_ui_data::CellValue;

#[test]
fn compiled_workspace_clicks_mount_editors_in_their_cells_and_write_through() {
    use rust_desktop_ui_controls::GridRowKind;
    use rust_desktop_ui_core::{InputEvent, Key, Point, PointerButton};
    use rust_desktop_ui_treegrid::ColumnId;

    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("Light", "Light");
    ui.commands_mut().register("Dark", "Dark");
    let sources = demo::register(&mut ui).unwrap();
    let root = ui.root();
    let view = MountedView::mount_with_context(
        &mut ui,
        root,
        &crate::generated_grid_workspace::window_definition().content,
        demo::model().unwrap(),
    )
    .unwrap();
    let _state = demo::State::install(&mut ui, &view, sources.clone())
        .unwrap()
        .unwrap();
    let grid = view.find_name("WorkspaceGrid").unwrap();
    let size = Size::new(1640., 920.);
    ui.update(size).unwrap();
    let frame = ui.grid_frame(grid).unwrap();
    let row = frame
        .semantics
        .rows
        .iter()
        .find(|r| {
            matches!(ui.grid_row_kind(grid, r.key), Some(GridRowKind::Record(_)))
                && frame
                    .cell_bounds(r.key, ColumnId(3))
                    .is_some_and(|b| b.height >= 22.)
        })
        .unwrap()
        .key;
    let GridRowKind::Record(record) = ui.grid_row_kind(grid, row).unwrap() else {
        unreachable!()
    };
    let click = |ui: &mut Controls, position| {
        for event in [
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            },
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            },
        ] {
            ui.dispatch(event).unwrap();
            ui.update(size).unwrap();
        }
    };
    let click_column = |ui: &mut Controls, column| {
        let b = ui
            .grid_frame(grid)
            .unwrap()
            .cell_bounds(row, ColumnId(column))
            .unwrap();
        let position = Point::new(b.x + b.width / 2., b.y + b.height / 2.);
        click(ui, position);
    };
    let key = |ui: &mut Controls, key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
        ui.update(size).unwrap();
    };
    for column in [0, 5] {
        click_column(&mut ui, column);
        assert!(ui.grid_editor(grid).is_none());
    }
    for column in [1, 2, 3, 4, 6, 7] {
        click_column(&mut ui, column);
        let editor = ui.grid_editor(grid).expect("one click opens a cell editor");
        let mut host = editor;
        while let Some(parent) = ui.runtime.tree().parent(host) {
            if parent == grid {
                break;
            }
            host = parent;
        }
        let b = ui.runtime.bounds(host).unwrap();
        let cell = ui
            .grid_frame(grid)
            .unwrap()
            .cell_bounds(row, ColumnId(column))
            .unwrap();
        assert!(
            (b.x - cell.x).abs() < 1.
                && (b.y - cell.y).abs() < 1.
                && (b.width - cell.width).abs() < 1.
                && (b.height - cell.height).abs() < 1.,
            "column {column}: host {b:?}, cell {cell:?}"
        );
        if column == 7 {
            assert!(ui.check_state(editor).is_some());
        } else {
            assert!(ui.document(editor).is_some());
        }
        key(&mut ui, Key::Escape);
        assert!(ui.grid_editor(grid).is_none());
    }
    click_column(&mut ui, 4);
    key(&mut ui, Key::Tab);
    assert_eq!(
        ui.grid_focused_column(grid),
        Some(ColumnId(6)),
        "skip the calculated total"
    );
    assert!(ui.grid_editor(grid).is_some());
    key(&mut ui, Key::Escape);

    click_column(&mut ui, 3);
    let editor = ui.grid_editor(grid).unwrap();
    ui.set_text(editor, "704").unwrap();
    click_column(&mut ui, 2);
    assert_eq!(
        sources.master.borrow().value(record, "Quantity"),
        Some(&CellValue::Integer(704))
    );
    assert_eq!(ui.grid_focused_column(grid), Some(ColumnId(2)));
    let editor = ui.grid_editor(grid).unwrap();
    ui.set_text(editor, "Проверка строки").unwrap();
    key(&mut ui, Key::Enter);
    assert_eq!(
        sources.master.borrow().value(record, "Title"),
        Some(&CellValue::Text("Проверка строки".into()))
    );
    for (column, label, field, expected) in [
        (
            6,
            "23",
            "Due",
            CellValue::Date(rust_desktop_ui_data::DateValue::new(2026, 10, 23).unwrap()),
        ),
        (1, "Поставка", "Category", CellValue::Text("SUP".into())),
    ] {
        click_column(&mut ui, column);
        let b = ui
            .grid_frame(grid)
            .unwrap()
            .cell_bounds(row, ColumnId(column))
            .unwrap();
        click(
            &mut ui,
            Point::new(b.x + b.width - 14., b.y + b.height / 2.),
        );
        let bounds = ui
            .scene()
            .texts
            .iter()
            .rev()
            .find(|t| t.text == label)
            .expect("visible popup option")
            .bounds;
        click(
            &mut ui,
            Point::new(bounds.x + 4., bounds.y + bounds.height / 2.),
        );
        assert!(
            ui.grid_editor(grid).is_some(),
            "selecting an option keeps the grid session"
        );
        key(&mut ui, Key::Enter);
        assert_eq!(
            sources.master.borrow().value(record, field),
            Some(&expected)
        );
    }
}

#[test]
fn grid_workspace_demo_modes_recipes_conflict_and_binding_preserve_logical_key() {
    let definition =
        rust_desktop_ui_xaml::parse(include_str!("../GridWorkspaceWindow.xaml")).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("Light", "Light");
    ui.commands_mut().register("Dark", "Dark");
    let sources = demo::register(&mut ui).unwrap();
    let context = demo::model().unwrap();
    let root = ui.root();
    let mut view =
        MountedView::mount_with_context(&mut ui, root, &definition.content, context.clone())
            .unwrap();
    let mut state = demo::State::install(&mut ui, &view, sources.clone())
        .unwrap()
        .unwrap();
    let grid = view.find_name("WorkspaceGrid").unwrap();
    ui.update(Size::new(1640., 920.)).unwrap();
    let frame = ui.grid_frame(grid).unwrap();
    let bounds_of = |id| frame.columns.iter().find(|c| c.id.0 == id).unwrap().clip;
    let due = bounds_of(6);
    let active = bounds_of(7);
    assert!((due.x + due.width - active.x).abs() < 0.01);
    for title in ["Документ", "Состояние"] {
        assert_eq!(
            ui.scene()
                .texts
                .iter()
                .filter(|run| run.text == title)
                .count(),
            1
        );
    }
    ui.update(Size::new(1320.0, 820.0)).unwrap();
    assert_eq!(ui.grid_selected_key(grid), Some(i64::MIN));
    assert!(ui.grid_frame(grid).unwrap().semantics.rows.len() < 40);
    let size = Size::new(1320., 820.);
    for cmd in ["AddFormat", "RaiseFormat", "ToggleFormat"] {
        state
            .command(&mut ui, &format!("GridWorkspace{cmd}"))
            .unwrap();
    }
    let rules = &ui.grid_presentation(grid).unwrap().format_rules;
    assert_eq!(rules.len(), 4);
    assert!(!rules[2].enabled);
    state.command(&mut ui, "GridWorkspaceRemoveFormat").unwrap();
    assert_eq!(ui.grid_presentation(grid).unwrap().format_rules.len(), 3);
    // Build and mount the same preview used by the form without printing a job.
    state.command(&mut ui, "GridWorkspacePreview").unwrap();
    assert!(
        ui.label_text(view.find_name("WorkspaceStatus").unwrap())
            .unwrap()
            .contains("Предпросмотр")
    );
    let before = ui.grid_summaries(grid).unwrap().to_vec();
    context
        .borrow_mut()
        .set("Criteria", "false()".into())
        .unwrap();
    view.bindings.as_mut().unwrap().sync(&mut ui);
    assert_eq!(ui.grid_criteria(grid).as_deref(), Some("false()"));
    assert_ne!(ui.grid_summaries(grid).unwrap(), before);
    ui.set_grid_criteria_enabled(grid, false).unwrap();
    view.bindings.as_mut().unwrap().sync(&mut ui);
    assert_eq!(
        context.borrow().get("RuleEnabled").unwrap(),
        &rust_desktop_ui_controls::DataValue::Bool(false)
    );
    assert_eq!(ui.grid_summaries(grid).unwrap(), before);
    ui.set_grid_criteria(grid, "true()").unwrap();
    ui.set_grid_criteria_enabled(grid, true).unwrap();
    view.bindings.as_mut().unwrap().sync(&mut ui);
    ui.begin_grid_edit(grid, i64::MIN, "Category").unwrap();
    assert_eq!(
        ui.document(ui.grid_editor(grid).unwrap()).unwrap().text(),
        "Разработка"
    );
    ui.set_grid_edit_value(grid, CellValue::Text("SUP".into()))
        .unwrap();
    assert!(ui.commit_grid_edit(grid).unwrap());
    assert_eq!(
        sources.master.borrow().value(i64::MIN, "Category"),
        Some(&CellValue::Text("SUP".into()))
    );
    ui.update(size).unwrap();
    assert_eq!(ui.grid_summaries(grid).unwrap(), before);
    let formula = view.find_name("WorkspaceFormula").unwrap();
    let original = ui.document(formula).unwrap().text().to_owned();
    ui.set_text(formula, r#"integer("1")"#).unwrap();
    state.command(&mut ui, "GridWorkspaceFormula").unwrap();
    ui.update(size).unwrap();
    assert_eq!(
        ui.grid_summaries(grid).unwrap()[1],
        CellValue::Integer(10000)
    );
    ui.set_text(formula, r#"field("LineTotal")"#).unwrap();
    state.command(&mut ui, "GridWorkspaceFormula").unwrap();
    ui.update(size).unwrap();
    assert_eq!(
        ui.grid_summaries(grid).unwrap()[1],
        CellValue::Integer(10000)
    );
    assert!(
        ui.label_text(view.find_name("WorkspaceStatus").unwrap())
            .unwrap()
            .contains("отклонена")
    );
    ui.set_text(formula, &original).unwrap();
    state.command(&mut ui, "GridWorkspaceFormula").unwrap();
    ui.update(size).unwrap();
    assert_eq!(ui.grid_summaries(grid).unwrap(), before);
    for command in [
        "Cards", "Table", "Reorder", "Pin", "Date", "Date", "Details", "Quantity",
    ] {
        state
            .command(&mut ui, &format!("GridWorkspace{command}"))
            .unwrap();
        ui.update(Size::new(1320.0, 820.0)).unwrap();
    }
    let editor = ui.grid_editor(grid).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "9007199254740993");
    ui.set_text(editor, "9007199254740995").unwrap();
    assert!(ui.commit_grid_edit(grid).unwrap());
    assert_eq!(
        sources.master.borrow().value(i64::MIN, "Quantity"),
        Some(&CellValue::Integer(9_007_199_254_740_995))
    );
    state.command(&mut ui, "GridWorkspaceConflict").unwrap();
    assert!(!ui.commit_grid_edit(grid).unwrap());
    assert_eq!(
        ui.document(ui.grid_editor(grid).unwrap()).unwrap().text(),
        "42"
    );
    state.command(&mut ui, "GridWorkspaceCancel").unwrap();
    assert_eq!(
        sources.master.borrow().value(i64::MIN, "Quantity"),
        Some(&CellValue::Integer(101))
    );
    context
        .borrow_mut()
        .set("SelectedKey", i64::MAX.to_string().into())
        .unwrap();
    view.bindings.as_mut().unwrap().sync(&mut ui);
    assert_eq!(ui.grid_selected_key(grid), Some(i64::MAX));
    state.command(&mut ui, "GridWorkspaceGroup").unwrap();
    ui.update(Size::new(1320.0, 820.0)).unwrap();
    state.refresh(&mut ui).unwrap();
    assert!(
        ui.label_text(view.find_name("WorkspaceSummary").unwrap())
            .unwrap()
            .contains("10000")
    );
}

#[test]
fn extended_grid_schema_and_aggregate_failure_preflight_without_mutating_tree_or_source() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let sources = demo::register(&mut ui).unwrap();
    let root = ui.root();
    let before = ui.runtime.tree().live_ids();
    let query = sources.master.borrow().query().clone();
    for metadata in [
        "<GridControl.Summaries><GridSummary Function='Sum' FieldName='Title'/></GridControl.Summaries>",
        "<GridControl.Detail><GridDetail DataSource='WorkspaceDetails' MasterField='Title' ForeignField='OwnerId'><GridColumn FieldName='Caption'/></GridDetail></GridControl.Detail>",
        "<GridControl.Editors><GridEditor FieldName='Title' Factory='UnknownRecipe'/></GridControl.Editors>",
        "<GridControl.CellTemplates><GridCellTemplate FieldName='Missing'/></GridControl.CellTemplates>",
    ] {
        let definition=rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui'><GridControl DataSource='WorkspaceRecords'>{metadata}</GridControl></Window>")).unwrap();
        assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), before);
        assert_eq!(sources.master.borrow().query(), &query);
    }
}

#[test]
fn compiled_record_form_commands_share_the_draft_and_the_detail_source() {
    let definition =
        rust_desktop_ui_xaml::parse(include_str!("../GridWorkspaceWindow.xaml")).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("Light", "Светлая");
    ui.commands_mut().register("Dark", "Тёмная");
    let sources = demo::register(&mut ui).unwrap();
    let root = ui.root();
    let view =
        MountedView::mount_with_context(&mut ui, root, &definition.content, demo::model().unwrap())
            .unwrap();
    let mut state = demo::State::install(&mut ui, &view, sources.clone())
        .unwrap()
        .unwrap();
    let size = Size::new(1320., 900.);
    let tabs = view.find_name("WorkspaceOperationTabs").unwrap();
    ui.set_expander_expanded(view.find_name("WorkspaceOperations").unwrap(), true)
        .unwrap();
    ui.update(size).unwrap();
    let grid = view.find_name("WorkspaceGrid").unwrap();
    let title = view.find_name("WorkspaceRowTitle").unwrap();
    state.command(&mut ui, "GridWorkspaceNewRow").unwrap();
    let key = ui.grid_row_draft(grid).unwrap().row.key;
    state.command(&mut ui, "GridWorkspaceSaveRow").unwrap();
    assert_eq!(ui.grid_row_issue(grid).unwrap().field, "Title");
    assert!(sources.master.borrow().table().row(key).is_none());
    ui.set_editor_value(title, "Новая запись").unwrap();
    state.command(&mut ui, "GridWorkspaceSaveRow").unwrap();
    assert!(ui.grid_row_draft(grid).is_none());
    ui.set_grid_selected_key(grid, Some(key)).unwrap();
    state.command(&mut ui, "GridWorkspaceEditRow").unwrap();
    ui.set_editor_value(title, "Временный текст").unwrap();
    state.command(&mut ui, "GridWorkspaceCancelRow").unwrap();
    assert_eq!(
        sources.master.borrow().value(key, "Title"),
        Some(&CellValue::Text("Новая запись".into()))
    );
    state.command(&mut ui, "GridWorkspaceDeleteRow").unwrap();
    assert!(sources.master.borrow().table().row(key).is_none());
    ui.set_grid_selected_key(grid, Some(i64::MIN)).unwrap();
    ui.set_selected_tab(tabs, 3).unwrap();
    ui.update(size).unwrap();
    state.command(&mut ui, "GridWorkspaceOpenItems").unwrap();
    let host = view.find_name("WorkspaceItemsHost").unwrap();
    let child = ui
        .open_grid_relation(grid, i64::MIN, "Items", host, Default::default())
        .unwrap();
    ui.begin_grid_row_edit(child, 0).unwrap();
    ui.set_grid_draft_cell(child, "Quantity", CellValue::Integer(17))
        .unwrap();
    ui.commit_grid_record(child).unwrap();
    assert_eq!(
        sources.detail.borrow().value(0, "Quantity"),
        Some(&CellValue::Integer(17))
    );
    ui.set_grid_selected_key(child, Some(0)).unwrap();
    state.command(&mut ui, "GridWorkspaceNotes").unwrap();
    state.command(&mut ui, "GridWorkspacePayments").unwrap();
    state.command(&mut ui, "GridWorkspaceOpenItems").unwrap();
    assert_eq!(ui.grid_selected_key(child), Some(0));
    ui.set_selected_tab(tabs, 2).unwrap();
    ui.update(size).unwrap();
    let tree = view.find_name("WorkspaceTopicsGrid").unwrap();
    ui.load_grid_branch(tree, 1).unwrap();
    let tree_data = ui.grid_source(tree).unwrap();
    assert!(tree_data.borrow().table().row(10).is_some());
}

#[test]
fn schema_design_mount_and_sqlite_page_publish_share_typed_fields() {
    use rust_desktop_ui_controls::{GridContextTarget, GridPageView};
    use rust_desktop_ui_data::*;
    let schema = TableSchema::new(vec![
        FieldSchema::new("N", FieldType::Integer),
        FieldSchema::new("Name", FieldType::Text),
    ])
    .unwrap();
    let form =
        rust_desktop_ui_xaml::design_grid_form("Orders", &schema, &["Name".into(), "N".into()])
            .unwrap();
    assert!(
        rust_desktop_ui_xaml::design_grid_form("Orders", &schema, &["missing".into()]).is_err()
    );
    assert!(
        rust_desktop_ui_xaml::design_grid_form("Orders", &schema, &["N".into(), "N".into()])
            .is_err()
    );
    let conn = SqliteConnection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE Orders (K INTEGER PRIMARY KEY, N INTEGER, Name TEXT); INSERT INTO Orders VALUES (1,2,'One'),(2,9,'Two')").unwrap();
    let mut adapter = SqliteAdapter::from_connection(
        conn,
        SqliteTableConfig::new("Orders", "K", schema.clone()).unwrap(),
    )
    .unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    ui.register_data_source(
        "Orders",
        std::rc::Rc::new(std::cell::RefCell::new(
            DataSource::new(CollectionAdapter::new(adapter.load().unwrap())).unwrap(),
        )),
    )
    .unwrap();
    let desc = rust_desktop_ui_xaml::parse(&form).unwrap();
    let _view =
        MountedView::mount_with_context(&mut ui, root, &desc.content, demo::model().unwrap())
            .unwrap();
    let pages = PagedDataSource::new(
        schema,
        Default::default(),
        PageCacheSettings {
            page_size: 1,
            ..Default::default()
        },
        QueryCapabilities::complete(),
    )
    .unwrap();
    let mut grid = GridPageView::mount(&mut ui, root, pages, Default::default()).unwrap();
    ui.set_grid_sort(
        grid.id(),
        vec![SortDescriptor {
            field: "N".into(),
            direction: SortDirection::Descending,
        }],
    )
    .unwrap();
    let req = grid.take_input(&mut ui).unwrap().pop().unwrap();
    let (page, summary) = adapter
        .query_page_with_totals(
            &req.query,
            req.offset as usize,
            req.limit,
            &[Aggregate {
                function: AggregateFunction::Sum,
                expression: Some(QueryExpression::field("N")),
            }],
        )
        .unwrap();
    assert_eq!(page.execution, QueryExecution::Pushdown);
    assert_eq!(page.table.rows()[0].key, 2);
    grid.source()
        .completion_sender()
        .try_complete(
            &req,
            Ok(PageResponse {
                total: PageTotal::Known(2),
                has_more: true,
                table: page.table,
                snapshot_revision: 7,
            }),
        )
        .unwrap();
    grid.poll(&mut ui).unwrap();
    assert!(
        grid.accept_totals(req.token, 7, vec![("Total".into(), summary[0].clone())])
            .unwrap()
    );
    assert_eq!(grid.totals()[0].1, CellValue::Integer(11));
    assert_eq!(
        ui.grid_source(grid.id()).unwrap().borrow().table().rows()[0].key,
        2
    );
    assert!(
        !ui.open_grid_context(grid.id(), GridContextTarget::Record(2))
            .unwrap()
    );
    let base = QueryRequest {
        filter: Some(Criteria::Compare {
            left: QueryExpression::field("N"),
            operator: ComparisonOperator::Greater,
            right: QueryExpression::literal(CellValue::Integer(3)),
        }),
        ..Default::default()
    };
    grid.query(&mut ui, base.clone()).unwrap();
    ui.set_grid_filter(grid.id(), "Two").unwrap();
    let find = grid.take_input(&mut ui).unwrap().pop().unwrap();
    assert!(matches!(find.query.filter, Some(Criteria::All(_))));
    ui.set_grid_filter(grid.id(), "").unwrap();
    let cleared = grid.take_input(&mut ui).unwrap().pop().unwrap();
    assert_eq!(*cleared.query, base);
}

#[test]
fn grouped_workspace_header_gestures_sort_real_records_and_groups() {
    use rust_desktop_ui_core::{InputEvent, Point, PointerButton};
    use rust_desktop_ui_treegrid::{ColumnId, GridHitKind, SortDirection};
    let definition =
        rust_desktop_ui_xaml::parse(include_str!("../GridWorkspaceWindow.xaml")).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("Light", "Light");
    ui.commands_mut().register("Dark", "Dark");
    let sources = demo::register(&mut ui).unwrap();
    let root = ui.root();
    let view =
        MountedView::mount_with_context(&mut ui, root, &definition.content, demo::model().unwrap())
            .unwrap();
    let _state = demo::State::install(&mut ui, &view, sources)
        .unwrap()
        .unwrap();
    let grid = view.find_name("WorkspaceGrid").unwrap();
    let size = Size::new(1640., 920.);
    ui.update(size).unwrap();
    let click = |ui: &mut Controls, point| {
        for event in [
            InputEvent::PointerDown {
                position: point,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            },
            InputEvent::PointerUp {
                position: point,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            },
        ] {
            ui.dispatch(event).unwrap();
        }
        ui.update(size).unwrap();
    };
    let records = |ui: &Controls| {
        ui.grid_frame(grid)
            .unwrap()
            .semantics
            .rows
            .iter()
            .filter(|r| {
                matches!(
                    ui.grid_row_kind(grid, r.key),
                    Some(rust_desktop_ui_controls::GridRowKind::Record(_))
                )
            })
            .filter_map(|r| {
                r.cells
                    .iter()
                    .find(|c| c.column == ColumnId(0))
                    .map(|c| c.label.clone())
            })
            .collect::<Vec<_>>()
    };
    let asc = records(&ui);
    assert!(
        asc.len() >= 2 && asc.windows(2).all(|v| v[0] < v[1]),
        "{asc:?}"
    );
    let frame = ui.grid_frame(grid).unwrap();
    assert_eq!(frame.semantics.sort[0].direction, SortDirection::Ascending);
    let r = frame
        .hits
        .iter()
        .find(|h| h.kind == GridHitKind::Header(ColumnId(0)))
        .unwrap()
        .bounds;
    click(&mut ui, Point::new(r.x + 9., r.y + r.height / 2.));
    let frame = ui.grid_frame(grid).unwrap();
    assert_eq!(frame.semantics.sort[0].direction, SortDirection::Descending);
    let desc = records(&ui);
    assert!(
        desc.len() >= 2 && desc.windows(2).all(|v| v[0] > v[1]),
        "{desc:?}"
    );
    let group = |ui: &Controls| {
        ui.grid_frame(grid)
            .unwrap()
            .semantics
            .rows
            .iter()
            .find(|r| r.depth == 0)
            .unwrap()
            .key
    };
    let first = group(&ui);
    let panel = ui.runtime.bounds(grid).unwrap();
    click(&mut ui, Point::new(panel.x + 15., panel.y + 14.));
    assert_ne!(group(&ui), first);
    click(&mut ui, Point::new(panel.x + 15., panel.y + 14.));
    assert_eq!(group(&ui), first);
}
