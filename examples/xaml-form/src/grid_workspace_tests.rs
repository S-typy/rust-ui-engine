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
