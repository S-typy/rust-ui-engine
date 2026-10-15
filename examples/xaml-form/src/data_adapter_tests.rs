use crate::{data_model, form::Form, generated_data, view::MountedView};
use rust_desktop_ui_controls::{Controls, DataValue, Theme};
use rust_desktop_ui_core::Size;
use rust_desktop_ui_data::CellValue;
use rust_desktop_ui_platform_winit::DesktopApp;

fn form() -> Form {
    let (context, sources) = data_model::model().unwrap();
    Form::new_with_sources(
        generated_data::window_definition(),
        false,
        Some(context),
        sources,
    )
    .unwrap()
}

#[test]
fn compiled_grids_fill_available_space_and_keep_virtual_frames_small() {
    let mut form = form();
    for size in [Size::new(1200.0, 780.0), Size::new(960.0, 640.0)] {
        form.update(size, 1.0).unwrap();
        for (name, count) in [("CollectionGrid", 5000), ("SqliteGrid", 1000)] {
            let id = form.find_name(name).unwrap();
            let bounds = form.ui.runtime.bounds(id).unwrap();
            assert!(bounds.height > 200.0, "{name}: {bounds:?}");
            assert!(bounds.y + bounds.height < size.height - 50.0);
            let frame = form.ui.grid_frame(id).unwrap();
            assert!(frame.rows.len() < 40);
            assert!(frame.rows.len() > 3);
            assert_eq!(
                frame.rows.first().unwrap().key.unwrap().0,
                data_model::FIRST_KEY as u64
            );
            let first = frame
                .cell_bounds(
                    rust_desktop_ui_treegrid::RowKey(data_model::FIRST_KEY as u64),
                    rust_desktop_ui_treegrid::ColumnId(0),
                )
                .expect("initially selected first row must be visible");
            assert!(first.height >= 27.0, "first row is clipped: {first:?}");
            assert_eq!(form.ui.grid_source(id).unwrap().borrow().len(), count);
            assert_eq!(form.ui.grid_selected_key(id), Some(data_model::FIRST_KEY));
        }
    }
}

#[test]
fn filter_and_selection_bindings_roundtrip_without_float_keys() {
    let mut form = form();
    form.update(Size::new(1200.0, 780.0), 1.0).unwrap();
    let context = form.view.bindings.as_ref().unwrap().context();
    context
        .borrow_mut()
        .set("Filter", "Клавиатура".into())
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    form.update(Size::new(1200.0, 780.0), 1.0).unwrap();
    for name in ["CollectionGrid", "SqliteGrid"] {
        let id = form.find_name(name).unwrap();
        assert_eq!(form.ui.grid_filter(id), Some("Клавиатура"));
        let source = form.ui.grid_source(id).unwrap();
        assert!(source.borrow().len() < 1000);
        let key = source.borrow().row_key(3).unwrap();
        form.ui.set_grid_selected_key(id, Some(key)).unwrap();
        form.view.sync_bindings(&mut form.ui);
        let path = if name == "CollectionGrid" {
            "CollectionKey"
        } else {
            "SqliteKey"
        };
        assert_eq!(
            context.borrow().get(path).unwrap(),
            &DataValue::String(key.to_string())
        );
    }
}

#[test]
fn sqlite_constraint_failure_keeps_original_value_and_editor_draft() {
    let mut form = form();
    form.update(Size::new(1200.0, 780.0), 1.0).unwrap();
    let id = form.find_name("SqliteGrid").unwrap();
    form.ui
        .begin_grid_edit(id, data_model::FIRST_KEY, "Quantity")
        .unwrap();
    let editor = form.ui.grid_editor(id).unwrap();
    form.ui.set_text(editor, "-5").unwrap();
    assert!(!form.ui.commit_grid_edit(id).unwrap());
    assert_eq!(form.ui.document(editor).unwrap().text(), "-5");
    assert!(form.ui.grid_error(id).is_some());
    assert_eq!(
        form.ui
            .grid_source(id)
            .unwrap()
            .borrow()
            .value(data_model::FIRST_KEY, "Quantity"),
        Some(&CellValue::Integer(1))
    );
    form.ui.set_text(editor, "42").unwrap();
    assert!(form.ui.commit_grid_edit(id).unwrap());
    let source = form.ui.grid_source(id).unwrap();
    source.borrow_mut().refresh().unwrap();
    assert_eq!(
        source.borrow().value(data_model::FIRST_KEY, "Quantity"),
        Some(&CellValue::Integer(42))
    );
}

#[test]
fn missing_source_and_field_fail_before_tree_mutation() {
    for (source, field) in [("Missing", "Product"), ("Products", "Missing")] {
        let definition = rust_desktop_ui_xaml::parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui'><StackPanel><TextBlock Text='Before'/><GridControl DataSource='{source}'><GridColumn FieldName='{field}'/></GridControl></StackPanel></Window>"
        )).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        for (name, source) in data_model::model().unwrap().1 {
            ui.register_data_source(name, source).unwrap();
        }
        let root = ui.root();
        let before = ui.runtime.tree().children(root).unwrap().len();
        assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
        assert_eq!(ui.runtime.tree().children(root).unwrap().len(), before);
    }
}

#[test]
fn both_grids_obey_read_only_binding_and_cancel_draft_explicitly() {
    let mut form = form();
    form.update(Size::new(1200.0, 780.0), 1.0).unwrap();
    let context = form.view.bindings.as_ref().unwrap().context();
    context.borrow_mut().set("ReadOnly", true.into()).unwrap();
    form.view.sync_bindings(&mut form.ui);
    for name in ["CollectionGrid", "SqliteGrid"] {
        let id = form.find_name(name).unwrap();
        assert!(form.ui.grid_settings(id).unwrap().is_read_only);
        assert!(
            form.ui
                .begin_grid_edit(id, data_model::FIRST_KEY, "Product")
                .is_err()
        );
    }
}

fn small_source() -> crate::data_model::SharedSource {
    use rust_desktop_ui_data::{
        CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType, TableSchema,
    };
    let table = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text)]).unwrap(),
        vec![
            DataRow::new(1, vec![CellValue::Text("First".into())]),
            DataRow::new(2, vec![CellValue::Text("Second".into())]),
        ],
    )
    .unwrap();
    std::rc::Rc::new(std::cell::RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ))
}

#[test]
fn initial_grid_selection_is_checked_against_query_before_any_mount_or_source_mutation() {
    for (existing, attributes) in [
        ("", "FilterText='First' SelectedValue='2'"),
        ("First", "SelectedValue='99'"),
    ] {
        let source = small_source();
        source
            .borrow_mut()
            .set_query(rust_desktop_ui_data::DataQuery {
                filter: existing.into(),
                ..Default::default()
            })
            .unwrap();
        let before_revision = source.borrow().revision();
        let before_query = source.borrow().query().clone();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.register_data_source("Rows", std::rc::Rc::clone(&source))
            .unwrap();
        let definition = rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui'><StackPanel><TextBlock Text='Before'/><GridControl DataSource='Rows' {attributes}/></StackPanel></Window>")).unwrap();
        let root = ui.root();
        let before = ui.runtime.tree().children(root).unwrap().to_vec();
        assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
        assert_eq!(ui.runtime.tree().children(root).unwrap(), before);
        assert_eq!(source.borrow().revision(), before_revision);
        assert_eq!(source.borrow().query(), &before_query);
    }
    let source = small_source();
    let definition = rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui'><GridControl DataSource='Rows' FilterText='Second' SelectedValue='2'/></Window>").unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.register_data_source("Rows", std::rc::Rc::clone(&source))
        .unwrap();
    let root = ui.root();
    MountedView::mount(&mut ui, root, &definition.content).unwrap();
    assert_eq!(source.borrow().len(), 1);
    assert_eq!(source.borrow().row_key(0), Some(2));
}

#[test]
fn manual_grid_source_binding_is_rejected_before_mount() {
    let mut definition = rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui'><StackPanel><TextBlock/><GridControl DataSource='Rows'/></StackPanel></Window>").unwrap();
    definition.content.children_mut()[1].common.bindings.insert(
        "DataSource".into(),
        rust_desktop_ui_xaml::BindingDefinition {
            path: "Source".into(),
            ..Default::default()
        },
    );
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.register_data_source("Rows", small_source()).unwrap();
    let root = ui.root();
    let before = ui.runtime.tree().children(root).unwrap().to_vec();
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().children(root).unwrap(), before);
}

#[test]
fn grids_sharing_a_source_keep_known_initial_keys_hidden_by_an_earlier_filter() {
    let definition = rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><StackPanel><GridControl x:Name='First' DataSource='Rows' FilterText='First' SelectedValue='1'/><GridControl x:Name='Second' DataSource='Rows' SelectedValue='2'/></StackPanel></Window>").unwrap();
    let mut form = Form::new_with_sources(
        definition,
        false,
        None,
        vec![("Rows".into(), small_source())],
    )
    .unwrap();
    form.update(Size::new(620.0, 700.0), 1.0).unwrap();
    let first = form.find_name("First").unwrap();
    let second = form.find_name("Second").unwrap();
    assert_eq!(form.ui.grid_selected_key(first), Some(1));
    assert_eq!(form.ui.grid_selected_key(second), Some(2));
    assert_eq!(form.ui.grid_filter(second), Some("First"));
}

#[test]
fn automatic_grid_column_limit_is_checked_before_mount() {
    use rust_desktop_ui_data::{
        CollectionAdapter, DataSource, DataTable, FieldSchema, FieldType, TableSchema,
    };
    let table = DataTable::new(
        TableSchema::new(
            (0..65)
                .map(|index| FieldSchema::new(format!("Field{index}"), FieldType::Text))
                .collect(),
        )
        .unwrap(),
        Vec::new(),
    )
    .unwrap();
    let source = std::rc::Rc::new(std::cell::RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.register_data_source("Rows", source).unwrap();
    let definition = rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui'><StackPanel><TextBlock/><GridControl DataSource='Rows'/></StackPanel></Window>").unwrap();
    let root = ui.root();
    let before = ui.runtime.tree().children(root).unwrap().to_vec();
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().children(root).unwrap(), before);
}

#[test]
fn grid_alignment_preserves_intrinsic_height_while_stretch_and_height_binding_fill_as_requested() {
    use rust_desktop_ui_controls::BindingContext;
    use rust_desktop_ui_core::Length;
    for (attributes, expected_height) in [
        ("VerticalAlignment='Top'", 320.0),
        ("VerticalAlignment='Center'", 320.0),
        ("VerticalAlignment='Bottom'", 320.0),
        ("", 700.0),
        ("Height='{Binding GridHeight}'", 180.0),
    ] {
        let definition = rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' Padding='0'><Grid><GridControl x:Name='Data' DataSource='Rows' {attributes}/></Grid></Window>")).unwrap();
        let context = std::rc::Rc::new(std::cell::RefCell::new(
            BindingContext::new(DataValue::object([(
                "GridHeight",
                DataValue::Number(180.0),
            )]))
            .unwrap(),
        ));
        let mut form = Form::new_with_sources(
            definition,
            false,
            Some(std::rc::Rc::clone(&context)),
            vec![("Rows".into(), small_source())],
        )
        .unwrap();
        form.update(Size::new(620.0, 700.0), 1.0).unwrap();
        let id = form.find_name("Data").unwrap();
        let bounds = form.ui.runtime.bounds(id).unwrap();
        assert!(
            (bounds.height - expected_height).abs() < 1.0,
            "{attributes}: {bounds:?}"
        );
        if attributes.starts_with("VerticalAlignment") {
            assert_eq!(
                form.ui.runtime.tree().node(id).unwrap().props.style.height,
                Length::Px(320.0)
            );
        }
        if attributes.contains("GridHeight") {
            context
                .borrow_mut()
                .set("GridHeight", DataValue::Null)
                .unwrap();
            form.update(Size::new(620.0, 700.0), 1.0).unwrap();
            assert!((form.ui.runtime.bounds(id).unwrap().height - 320.0).abs() < 1.0);
        }
    }
}
