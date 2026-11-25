use super::*;
use rust_desktop_ui_controls::Theme;
use rust_desktop_ui_core::Size;
use rust_desktop_ui_data::CellValue;

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
