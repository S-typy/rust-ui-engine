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
