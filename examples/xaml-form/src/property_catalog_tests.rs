use crate::{form::Form, generated_property_catalog};
use rust_desktop_ui_controls::{PropertyCommit, PropertyValue};
use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Point, PointerButton, Rect, Size};
use rust_desktop_ui_data::CellValue;
use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent};

mod color;
mod column_resize;
mod custom;

fn pointer_down(form: &mut Form, position: Point) {
    form.event(PlatformEvent::Input(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
}

fn pointer_up(form: &mut Form, position: Point) {
    form.event(PlatformEvent::Input(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
}

fn click(form: &mut Form, position: Point) {
    pointer_down(form, position);
    pointer_up(form, position);
}

fn press_key(form: &mut Form, key: Key) {
    press_modified_key(form, key, Modifiers::default());
}

fn press_modified_key(form: &mut Form, key: Key, modifiers: Modifiers) {
    form.event(PlatformEvent::Input(InputEvent::KeyDown {
        key: key.clone(),
        modifiers,
        repeat: false,
    }))
    .unwrap();
    form.event(PlatformEvent::Input(InputEvent::KeyUp { key, modifiers }))
        .unwrap();
}

fn reveal_spin_row(form: &mut Form, field: &str, size: Size, scale: f32) -> Rect {
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
        .unwrap();
    form.update(size, scale).unwrap();
    form.update(size, scale).unwrap();
    let grid = form.view.property_grids.values().next().unwrap();
    let row = grid.property_row(field).unwrap();
    let viewport = grid.row_viewport().unwrap();
    let row_y = form.ui.runtime.bounds(row).unwrap().y;
    let viewport_y = form.ui.runtime.bounds(viewport).unwrap().y;
    form.ui
        .scroll_to(viewport, (row_y - viewport_y - 60.).max(0.))
        .unwrap();
    form.update(size, scale).unwrap();
    let grid = form.view.property_grids.values().next().unwrap();
    let preview = form
        .ui
        .runtime
        .bounds(grid.property_actions(field).unwrap().0)
        .unwrap();
    let row_before = form.ui.runtime.bounds(row).unwrap();
    click(
        form,
        Point::new(preview.x + 14., preview.y + preview.height / 2.),
    );
    form.update(size, scale).unwrap();
    let grid = form.view.property_grids.values().next().unwrap();
    let editor = grid.property_editor(field).unwrap();
    assert_eq!(
        form.ui.runtime.focused(),
        Some(editor),
        "{field}: first click"
    );
    assert_eq!(
        form.ui.runtime.bounds(row),
        Some(row_before),
        "{field}: row moved"
    );
    assert!(
        form.ui
            .runtime
            .bounds(grid.property_actions(field).unwrap().1)
            .is_none(),
        "{field}: the Apply check occupies part of the row"
    );
    row_before
}

fn spin_arrows(form: &Form, field: &str, row: Rect) -> [Rect; 2] {
    let grid = form.view.property_grids.values().next().unwrap();
    let parts = form
        .ui
        .editor_control_parts(grid.property(field).unwrap())
        .unwrap();
    let mut buttons: Vec<_> = parts
        .buttons
        .iter()
        .filter_map(|id| form.ui.runtime.bounds(*id))
        .collect();
    buttons.sort_by(|a, b| a.y.total_cmp(&b.y));
    let [up, down]: [Rect; 2] = buttons
        .try_into()
        .expect("expected only the two spin directions");
    assert!((up.x - down.x).abs() < 1. && (up.width - down.width).abs() < 1.);
    assert!(up.height > 0. && down.height > 0. && up.y + up.height <= down.y + 0.5);
    assert!(down.y + down.height <= row.y + row.height + 0.5);
    assert!(up.x > row.x + row.width * 0.75 && up.width <= row.height + 1.);
    [up, down]
}

#[test]
fn documents_spin_steps_the_draft_and_enter_updates_both_documents() {
    let mut form = Form::new(crate::generated_properties::window_definition(), false).unwrap();
    let size = Size::new(700., 760.);
    let row = reveal_spin_row(&mut form, "edition", size, 1.5);
    let [up, down] = spin_arrows(&form, "edition", row);
    let center = |b: Rect| Point::new(b.x + b.width / 2., b.y + b.height / 2.);
    let owner = form.view.names["DocumentProperties"];
    let source = form.ui.data_source("InspectionObjects").unwrap();
    for (direction, value) in [(up, 4), (down, 3), (up, 4)] {
        click(&mut form, center(direction));
        form.update(size, 1.5).unwrap();
        assert_eq!(
            form.view.property_grids[&owner]
                .draft(&form.ui, "edition")
                .unwrap(),
            Some(CellValue::Integer(value))
        );
        for key in [1, 2] {
            assert_eq!(
                source.borrow().value(key, "edition"),
                Some(&CellValue::Integer(3))
            );
        }
    }
    press_key(&mut form, Key::Enter);
    for key in [1, 2] {
        assert_eq!(
            source.borrow().value(key, "edition"),
            Some(&CellValue::Integer(4))
        );
    }

    // Opening a mixed value and clicking away must never broadcast its editor seed.
    source
        .borrow_mut()
        .set_cell(2, "edition", CellValue::Integer(9))
        .unwrap();
    form.view
        .property_grids
        .get_mut(&owner)
        .unwrap()
        .refresh(&mut form.ui)
        .unwrap();
    form.update(size, 1.5).unwrap();
    let grid = &form.view.property_grids[&owner];
    assert_eq!(grid.accepted("edition"), Some(PropertyValue::Mixed));
    let preview = form
        .ui
        .runtime
        .bounds(grid.property_actions("edition").unwrap().0)
        .unwrap();
    let search = form.ui.runtime.bounds(grid.toolbar_parts().0).unwrap();
    click(&mut form, center(preview));
    click(&mut form, center(search));
    assert_eq!(
        source.borrow().value(1, "edition"),
        Some(&CellValue::Integer(4))
    );
    assert_eq!(
        source.borrow().value(2, "edition"),
        Some(&CellValue::Integer(9))
    );
}

#[test]
#[ignore = "requires a hardware GPU adapter"]
fn render_property_spin_rows() {
    use rust_desktop_ui_images::{PixelCrop, crop_bitmap, encode_bitmap_png};
    let out = std::env::var_os("PROPERTY_GRID_FRAME_DIR").map(std::path::PathBuf::from);
    let mut gpu =
        pollster::block_on(rust_desktop_ui_render_wgpu::OffscreenRenderer::new()).unwrap();
    eprintln!("GPU {:?}", gpu.adapter_info());
    for (field, definition) in [
        ("edition", crate::generated_properties::window_definition()),
        ("price", generated_property_catalog::window_definition()),
    ] {
        let size = Size::new(700., 760.);
        let scale = 1.5;
        let mut form = Form::new(definition, false).unwrap();
        let row = reveal_spin_row(&mut form, field, size, scale);
        spin_arrows(&form, field, row);
        let in_row = |text: &str| {
            form.ui
                .scene()
                .texts
                .iter()
                .find(|run| {
                    run.text == text
                        && run
                            .bounds
                            .intersection(run.clip)
                            .is_some_and(|ink| ink.intersection(row).is_some())
                })
                .unwrap_or_else(|| panic!("{field}: missing visible text {text}"))
        };
        let label = in_row(if field == "edition" {
            "Версия"
        } else {
            "Цена экземпляра"
        });
        let value = in_row(if field == "edition" { "3" } else { "49,95" });
        assert!(
            label.weight >= 550 && value.weight >= 550,
            "{field}: active row typography"
        );
        if field == "price" {
            let currency = in_row("₽");
            let offset = currency.bounds.x - value.bounds.x;
            assert!(
                offset > 0. && offset < value.font_size * 5.,
                "currency detached from digits: {offset}"
            );
        }
        let bitmap = gpu
            .render(
                form.ui.scene(),
                [(size.width * scale) as u32, (size.height * scale) as u32],
                scale,
            )
            .unwrap();
        // Include a little of each neighbour to see the divider and row height.
        let x = (row.x * scale).floor() as u32;
        let right = ((row.x + row.width) * scale).ceil() as u32;
        let y = ((row.y - 12.) * scale).floor() as u32;
        let bottom = ((row.y + row.height + 12.) * scale).ceil() as u32;
        let band = crop_bitmap(
            &bitmap,
            PixelCrop {
                x,
                y,
                width: right - x,
                height: bottom - y,
            },
        )
        .unwrap();
        let first_y = (row.y * scale).ceil() as u32;
        let last_y = ((row.y + row.height) * scale).floor() as u32;
        for y in first_y..last_y {
            let accent = (x..right)
                .filter(|x| {
                    let i = ((y * bitmap.width() + x) * 4) as usize;
                    let rgb = &bitmap.pixels()[i..i + 3];
                    rgb[2].saturating_sub(rgb[0]) > 95 && rgb[2] > 110
                })
                .count();
            assert!(
                accent < (right - x) as usize / 5,
                "{field}: full-width blue focus line"
            );
        }
        if let Some(dir) = &out {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(
                dir.join(format!("spin-{field}.png")),
                encode_bitmap_png(&band).unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn catalog_first_press_places_caret_and_posts_text_or_check_to_object() {
    let mut form = Form::new(generated_property_catalog::window_definition(), false).unwrap();
    let size = Size::new(760., 650.);
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
        .unwrap();
    form.update(size, 1.25).unwrap();
    form.update(size, 1.25).unwrap();
    let source = form.ui.data_source("EditorCatalogData").unwrap();
    let owner = form.view.names["EditorCatalog"];
    let grid = &form.view.property_grids[&owner];
    let title = grid.property_editor("title").unwrap();
    let preview = grid.property_actions("title").unwrap().0;
    let search = grid.toolbar_parts().0;
    let checkbox = grid.property_editor("approved").unwrap();
    let check_row = grid.property_row("approved").unwrap();
    let viewport = grid.row_viewport().unwrap();
    let CellValue::Text(original) = source.borrow().value(1, "title").unwrap().clone() else {
        panic!("expected a text property");
    };
    let bounds = form.ui.runtime.bounds(preview).unwrap();
    let inside_title = Point::new(bounds.x + 28., bounds.y + bounds.height / 2.);
    pointer_down(&mut form, inside_title);
    assert_eq!(form.ui.runtime.focused(), Some(title));
    let caret = form.ui.document(title).unwrap().selection();
    assert!(caret.is_empty() && caret.focus > 0 && caret.focus < original.len());
    pointer_up(&mut form, inside_title);
    form.event(PlatformEvent::Text("★".into())).unwrap();
    let mut edited = original.clone();
    edited.insert(caret.focus, '★');
    assert_eq!(form.ui.document(title).unwrap().text(), edited);
    assert_eq!(
        source.borrow().value(1, "title"),
        Some(&CellValue::Text(original))
    );
    press_key(&mut form, Key::Enter);
    assert_eq!(
        source.borrow().value(1, "title"),
        Some(&CellValue::Text(edited.clone()))
    );

    form.update(size, 1.25).unwrap();
    click(&mut form, inside_title);
    form.event(PlatformEvent::Text("?".into())).unwrap();
    assert_ne!(form.ui.document(title).unwrap().text(), edited);
    press_key(&mut form, Key::Escape);
    assert_eq!(
        source.borrow().value(1, "title"),
        Some(&CellValue::Text(edited.clone()))
    );

    form.update(size, 1.25).unwrap();
    click(&mut form, inside_title);
    form.event(PlatformEvent::Text("!".into())).unwrap();
    let draft = form.ui.document(title).unwrap().text().to_owned();
    assert_ne!(draft, edited);
    let search_bounds = form.ui.runtime.bounds(search).unwrap();
    click(
        &mut form,
        Point::new(
            search_bounds.x + 16.,
            search_bounds.y + search_bounds.height / 2.,
        ),
    );
    assert_eq!(
        source.borrow().value(1, "title"),
        Some(&CellValue::Text(draft))
    );

    let row_y = form.ui.runtime.bounds(check_row).unwrap().y;
    let viewport_y = form.ui.runtime.bounds(viewport).unwrap().y;
    form.ui
        .scroll_to(viewport, row_y - viewport_y - 30.)
        .unwrap();
    form.update(size, 1.25).unwrap();
    let bounds = form.ui.runtime.bounds(checkbox).unwrap();
    let inside_checkbox = Point::new(bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.);
    assert_eq!(form.ui.runtime.hit_test(inside_checkbox), Some(checkbox));
    assert_eq!(
        source.borrow().value(1, "approved"),
        Some(&CellValue::Bool(false))
    );
    click(&mut form, inside_checkbox);
    assert_eq!(
        source.borrow().value(1, "approved"),
        Some(&CellValue::Bool(true))
    );
}

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
    use rust_desktop_ui_xaml::Theme;
    let out = std::env::var_os("PROPERTY_GRID_FRAME_DIR").map(std::path::PathBuf::from);
    let mut gpu =
        pollster::block_on(rust_desktop_ui_render_wgpu::OffscreenRenderer::new()).unwrap();
    eprintln!("GPU {:?}", gpu.adapter_info());
    for (name, top, theme, width, scale) in [
        ("inputs", "title", Theme::Light, 760., 1.25),
        ("inplace-text", "title", Theme::Light, 760., 1.25),
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
        if name == "inplace-text" {
            form.event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
                .unwrap();
            let grid = &form.view.property_grids[&owner];
            let preview = grid.property_actions(top).unwrap().0;
            let editor = grid.property_editor(top).unwrap();
            let rect = form.ui.runtime.bounds(preview).unwrap();
            let position = Point::new(rect.x + 28., rect.y + rect.height / 2.);
            click(&mut form, position);
            form.update(size, scale).unwrap();
            assert_eq!(form.ui.runtime.focused(), Some(editor));
            assert!(form.ui.caret_bounds().is_some());
        }
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
                    click(&mut form, position);
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
