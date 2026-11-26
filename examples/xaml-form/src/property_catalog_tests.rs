use crate::{form::Form, generated_property_catalog};
use rust_desktop_ui_controls::{PropertyCommit, PropertyValue};
use rust_desktop_ui_core::Size;
use rust_desktop_ui_data::CellValue;
use rust_desktop_ui_platform_winit::DesktopApp;

mod custom;

#[test]
fn catalog_editors_roundtrip_all_source_values_and_rollback_invalid_updates() {
    let mut form = Form::new(generated_property_catalog::window_definition(), false).unwrap();
    let source = form.ui.data_source("EditorCatalogData").unwrap();
    let fields = source.borrow().table().schema().fields().to_vec();
    assert_eq!(fields.len(), 33);
    let ui = &mut form.ui;
    let grid = form
        .view
        .property_grids
        .get_mut(&form.view.names["EditorCatalog"])
        .unwrap();
    let price = grid.property_actions("price").unwrap().0;
    let budget = grid.property_actions("budget").unwrap().0;
    assert_eq!(ui.label_text(price), Some("49,95 ₽"));
    assert_eq!(ui.label_text(budget), Some("1 245,75 ₽"));
    for field in &fields {
        let name = &field.name;
        let original = source.borrow().value(1, name).unwrap().clone();
        assert_eq!(
            grid.accepted(name),
            Some(PropertyValue::Uniform(original.clone()))
        );
        if name == "progress" {
            assert!(grid.begin_edit(ui, name).is_err());
            continue;
        }
        grid.begin_edit(ui, name)
            .unwrap_or_else(|e| panic!("{name} begin: {e}"));
        assert_eq!(
            grid.draft(ui, name)
                .unwrap_or_else(|e| panic!("{name} read: {e}")),
            Some(original.clone()),
            "{name}"
        );
        assert_eq!(
            grid.commit(ui, name)
                .unwrap_or_else(|e| panic!("{name}: {e}")),
            PropertyCommit::Unchanged,
            "{name}"
        );
        assert_eq!(source.borrow().value(1, name), Some(&original));
    }
    let text = |s: &str| CellValue::Text(s.to_owned());
    for (name, value) in [
        ("serial", text("00071")),
        ("reference", text("12345-678-9012")),
        ("password", text("different-test-draft")),
        ("subject", CellValue::Integer(30)),
        ("notes", text("Первая строка\nВторая")),
        ("budget", CellValue::Decimal("35.75".parse().unwrap())),
        ("accent", text("#D47126")),
        ("interval", text("15..85")),
        ("rating", CellValue::Real(4.)),
        ("size", CellValue::Integer(48)),
        ("barcode", text("ATLAS-17")),
        ("rights", CellValue::Integer(5)),
        ("contributors", text(r#"["Юлия","Лев"]"#)),
    ] {
        let before = source.borrow().value(1, name).unwrap().clone();
        grid.set_draft(ui, name, value.clone())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        grid.cancel(ui, name).unwrap();
        assert_eq!(source.borrow().value(1, name), Some(&before));
        grid.set_draft(ui, name, value.clone()).unwrap();
        assert_eq!(
            grid.commit(ui, name).unwrap(),
            PropertyCommit::Saved,
            "{name}"
        );
        assert_eq!(source.borrow().value(1, name), Some(&value), "{name}");
    }
    for (name, value) in [
        ("reference", text("123")),
        ("subject", CellValue::Integer(99)),
        ("interval", text("90..80")),
        ("accent", text("invalid")),
    ] {
        let old = grid.accepted(name);
        assert!(grid.set_draft(ui, name, value).is_err(), "{name}");
        assert_eq!(grid.accepted(name), old);
    }
    ui.update(Size::new(760., 730.)).unwrap();
    assert!(
        !ui.scene()
            .texts
            .iter()
            .any(|t| t.text.contains("different-test-draft"))
    );
    assert!(
        !ui.label_text(grid.property_actions("password").unwrap().0)
            .unwrap()
            .contains("different-test-draft")
    );
}

#[test]
#[ignore = "requires a hardware GPU adapter"]
fn render_property_editor_catalog() {
    use rust_desktop_ui_core::{InputEvent, Modifiers, Point, PointerButton};
    use rust_desktop_ui_platform_winit::PlatformEvent;
    use rust_desktop_ui_xaml::Theme;
    let out = std::env::var_os("PROPERTY_GRID_FRAME_DIR").map(std::path::PathBuf::from);
    let mut gpu =
        pollster::block_on(rust_desktop_ui_render_wgpu::OffscreenRenderer::new()).unwrap();
    eprintln!("GPU {:?}", gpu.adapter_info());
    for (name, top, theme, width, scale) in [
        ("inputs", "title", Theme::Light, 760., 1.25),
        ("ranges", "opacity", Theme::Light, 760., 1.25),
        ("ranges-dark", "opacity", Theme::Dark, 760., 1.25),
        ("palette", "accent", Theme::Light, 760., 1.25),
        ("brush", "background", Theme::Dark, 760., 1.25),
        ("date-picker", "deadline", Theme::Light, 760., 1.25),
        ("image", "cover", Theme::Light, 760., 1.25),
        ("calculator", "budget", Theme::Light, 760., 1.25),
        ("structured", "rights", Theme::Light, 660., 1.),
        ("narrow", "interval", Theme::Light, 460., 1.5),
    ] {
        let mut definition = generated_property_catalog::window_definition();
        definition.theme = theme;
        let mut form = Form::new(definition, false).unwrap();
        let height = 650.;
        let size = Size::new(width, height);
        form.update(size, scale).unwrap();
        form.update(size, scale).unwrap();
        let owner = form.view.names["EditorCatalog"];
        let grid = form.view.property_grids.get_mut(&owner).unwrap();
        let row = grid.property_row(top).unwrap();
        let viewport = grid.row_viewport().unwrap();
        let distance =
            form.ui.runtime.bounds(row).unwrap().y - form.ui.runtime.bounds(viewport).unwrap().y;
        let inset = if top == "title" { distance } else { 30. };
        form.ui
            .scroll_to(viewport, (distance - inset).max(0.))
            .unwrap();
        form.update(size, scale).unwrap();
        if matches!(
            name,
            "palette" | "brush" | "date-picker" | "image" | "calculator"
        ) {
            let grid = form.view.property_grids.get_mut(&owner).unwrap();
            grid.begin_edit(&mut form.ui, top).unwrap();
            let field = grid.property(top).unwrap();
            let edit = grid.property_editor(top).unwrap();
            form.update(size, scale).unwrap();
            match name {
                "palette" | "brush" => form.ui.open_color_popup(field).unwrap(),
                "calculator" => form.ui.open_numeric_calculator(field).unwrap(),
                _ => {
                    let b = form.ui.runtime.bounds(edit).unwrap();
                    let position = Point::new(b.x + b.width / 2., b.y + b.height / 2.);
                    let button = PointerButton::Primary;
                    let modifiers = Modifiers::default();
                    for e in [
                        InputEvent::PointerDown {
                            position,
                            button,
                            modifiers,
                        },
                        InputEvent::PointerUp {
                            position,
                            button,
                            modifiers,
                        },
                    ] {
                        form.event(PlatformEvent::Input(e)).unwrap();
                    }
                }
            }
            form.update(size, scale).unwrap();
            assert!(form.ui.popup_open(), "{name} did not open");
        }
        let row = form.ui.runtime.bounds(row).unwrap();
        assert!(
            row.intersection(form.ui.runtime.bounds(viewport).unwrap())
                .is_some(),
            "{name} row outside viewport"
        );
        let bitmap = gpu
            .render(
                form.ui.scene(),
                [
                    (width * scale).round() as u32,
                    (height * scale).round() as u32,
                ],
                scale,
            )
            .unwrap();
        if let Some(dir) = &out {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(
                dir.join(format!("{name}.png")),
                rust_desktop_ui_images::encode_bitmap_png(&bitmap).unwrap(),
            )
            .unwrap();
        }
    }
}
