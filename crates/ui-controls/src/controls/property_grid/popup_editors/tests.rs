use super::*;
use rust_desktop_ui_images::{QuarterTurn, encode_bitmap_png};

#[test]
fn responsive_brush_raster_spans_the_value_cell_across_relayouts() {
    use rust_desktop_ui_data::{CollectionAdapter, DataRow, DataSource, DataTable};
    use std::rc::Rc;
    let table = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("brush", FieldType::Text)]).unwrap(),
        vec![DataRow::new(
            1,
            vec![CellValue::Text("linear(0;0=#A41424;1=#1594BB)".into())],
        )],
    )
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ));
    let mut descriptor = PropertyDescriptor::new("brush");
    descriptor.metadata.editor = "Brush".into();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut grid = PropertyGrid::mount(
        &mut ui,
        root,
        source,
        &[1],
        PropertyGridSettings {
            properties: vec![descriptor],
            label_fraction: Some(0.4),
            ..Default::default()
        },
        &DataFormFactories::default(),
        LayoutStyle {
            width: Length::Percent(1.),
            ..Default::default()
        },
    )
    .unwrap();
    let owner = grid.property("brush").unwrap();
    let parts = ui.color_editor_parts(owner).unwrap();
    for width in [420., 760.] {
        let size = Size::new(width, 280.);
        ui.update(size).unwrap();
        grid.update(&mut ui).unwrap();
        ui.update(size).unwrap();
        let cell = ui.runtime.bounds(grid.properties[0].host).unwrap();
        let swatch = ui.runtime.bounds(parts.preview).unwrap();
        let ramp_id = ui.color_editors.entries[&owner]
            .bitmap
            .as_ref()
            .unwrap()
            .id();
        let ramp = ui
            .scene()
            .images
            .iter()
            .find(|image| image.bitmap.id() == ramp_id)
            .unwrap();
        let painted = ramp
            .bounds
            .intersection(ramp.clip)
            .and_then(|b| b.intersection(cell))
            .unwrap();
        assert!(
            cell.width > width * 0.45,
            "cell {cell:?} for viewport {width}"
        );
        assert!(
            swatch.width > cell.width * 0.7,
            "swatch {swatch:?}, cell {cell:?}"
        );
        assert!(
            painted.width > cell.width * 0.7 && painted.height >= 10.,
            "paint {painted:?}, cell {cell:?}"
        );
        let pixels = ramp.bitmap.pixels();
        assert_ne!(&pixels[..4], &pixels[pixels.len() - 4..]);
    }
}

#[test]
fn attached_image_and_segmented_date_buttons_paint_one_disclosure_each() {
    for (name, field, value) in [
        ("Image", FieldType::Binary, CellValue::Binary(Vec::new())),
        (
            "DatePicker",
            FieldType::Date,
            CellValue::Date("2026-11-26".parse().unwrap()),
        ),
    ] {
        let (ui, edit) = fixture(name, field, value);
        let button = edit.focus();
        assert!(ui.button_flyout(button).is_some() && ui.button_icon(button).is_none());
        let bounds = ui.runtime.bounds(button).unwrap();
        let chevrons = ui
            .scene()
            .stroke_paths
            .iter()
            .filter(|path| {
                let [a, b] = path.segments.as_slice() else {
                    return false;
                };
                a.end == b.start
                    && a.end.y > a.start.y
                    && b.end.y < b.start.y
                    && a.end.x > a.start.x
                    && b.end.x > b.start.x
                    && bounds.contains(a.start.x, a.start.y)
                    && bounds.contains(b.end.x, b.end.y)
            })
            .count();
        assert_eq!(chevrons, 1, "{name} button {bounds:?}");
    }
}

#[track_caller]
fn click(ui: &mut Controls, id: WidgetId) {
    ui.update(Size::new(900., 800.)).unwrap();
    let b = ui.runtime.bounds(id).unwrap();
    assert!(
        b.width > 0. && b.height > 0.,
        "{} has bounds {b:?}",
        ui.controls[&id].label
    );
    let position = Point::new(b.x + b.width / 2., b.y + b.height / 2.);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}

#[test]
fn sparkline_renders_source_segments_and_keeps_the_last_valid_series_while_typing() {
    let original = CellValue::Text("[0, 6, -2]".into());
    let (mut ui, mut field) = fixture("Sparkline", FieldType::Text, original.clone());
    let chart = ui
        .controls
        .keys()
        .copied()
        .find(|id| ui.chart_model(*id).is_some())
        .unwrap();
    let before = ui.chart_geometry(chart).unwrap().unwrap();
    assert_eq!(before.segments.len(), 2);
    assert!(before.segments[0].end.y < before.segments[0].start.y);
    assert!(before.segments[1].end.y > before.segments[1].start.y);
    field.set_active(&mut ui, true).unwrap();
    ui.set_text(field.focus(), "[1,broken]").unwrap();
    field.update(&mut ui).unwrap();
    assert!(field.read(&ui).is_err());
    assert_eq!(ui.chart_model(chart).unwrap().series()[0].points.len(), 3);
    ui.set_text(field.focus(), "[4,4]").unwrap();
    field.update(&mut ui).unwrap();
    assert_eq!(field.read(&ui).unwrap(), CellValue::Text("[4,4]".into()));
    let flat = ui.chart_geometry(chart).unwrap().unwrap();
    assert_eq!(flat.segments[0].start.y, flat.segments[0].end.y);
    field.load(&mut ui, &original).unwrap();
    assert_eq!(field.read(&ui).unwrap(), original);
    assert!(
        sparkline::model("[]").unwrap().series()[0]
            .points
            .is_empty()
    );
    assert!(sparkline::model("[1e999]").is_err());
    assert!(sparkline::model(&serde_json::to_string(&vec![0; 513]).unwrap()).is_err());
}

fn fixture(kind: &str, field: FieldType, value: CellValue) -> (Controls, Box<dyn InspectorEditor>) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let schema = FieldSchema::new("field", field);
    let mut descriptor = PropertyDescriptor::new("field");
    descriptor.metadata.editor = kind.into();
    validate(&descriptor, &schema, &value).unwrap();
    let root = ui.root();
    let editor = mount(&mut ui, root, &descriptor, &schema, &value).unwrap();
    ui.update(Size::new(900., 800.)).unwrap();
    (ui, editor)
}

#[test]
fn palettes_apply_and_discard_without_normalizing_unedited_source_colors() {
    for kind in ["Color", "Brush"] {
        let original = CellValue::Text("#abcDEF".into());
        let (mut ui, editor) = fixture(kind, FieldType::Text, original.clone());
        assert_eq!(editor.read(&ui).unwrap(), original);
        ui.open_color_popup(editor.owner()).unwrap();
        let changed = if kind == "Brush" {
            parse_brush("linear(22.5;0=#000000;0.7=#11FFFFFF;1=#FFFFFF)", true).unwrap()
        } else {
            BrushValue::Solid(Srgba8::new(255, 205, 50, 23))
        };
        ui.set_brush_draft(editor.owner(), Some(changed.clone()))
            .unwrap();
        assert!(editor.read(&ui).is_err());
        ui.cancel_color_popup(editor.owner()).unwrap();
        assert_eq!(editor.read(&ui).unwrap(), original);
        ui.open_color_popup(editor.owner()).unwrap();
        ui.set_brush_draft(editor.owner(), Some(changed.clone()))
            .unwrap();
        ui.apply_color_popup(editor.owner()).unwrap();
        let CellValue::Text(text) = editor.read(&ui).unwrap() else {
            panic!("expected paint")
        };
        assert_eq!(parse_brush(&text, kind == "Brush").unwrap(), changed);
        editor.load(&mut ui, &original).unwrap();
        assert_eq!(editor.read(&ui).unwrap(), original);
    }
    for s in [
        "linear(-1;0=#000000;1=#FFFFFF)",
        "linear(0;1=#000000;0=#FFFFFF)",
        "linear(0;0=#000000)",
        "bad",
    ] {
        assert!(parse_brush(s, true).is_err(), "{s}");
    }
}

#[test]
fn color_property_quick_palette_navigates_and_accepts_the_visible_swatch() {
    let (mut ui, editor) = fixture("Color", FieldType::Text, CellValue::Text("#ffffff".into()));
    let owner = editor.owner();
    ui.open_color_popup(owner).unwrap();
    let popup = ui.color_popup(owner).unwrap().clone();
    let page = ui.runtime.bounds(popup.flyout.container).unwrap();
    let first = ui.runtime.bounds(popup.palette[0]).unwrap();
    let below = ui.runtime.bounds(popup.palette[8]).unwrap();
    let more = ui.runtime.bounds(popup.more_colors.unwrap()).unwrap();
    assert!(page.width < 300. && page.height < 240.);
    assert_eq!(first.x, below.x);
    assert!(below.y >= first.y + first.height && more.y + more.height <= page.y + page.height);
    assert_eq!(ui.runtime.focused(), Some(popup.palette[0]));
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        ui.update(Size::new(900., 800.)).unwrap();
        let semantics = ui.semantics();
        let current = semantics
            .iter()
            .find(|node| node.id == popup.palette[0])
            .unwrap();
        assert!(current.selected && current.focused && current.role == Role::ListItem);
        assert_eq!(current.label, "#FFFFFFFF");
        assert!(semantics.iter().all(|node| node.id != popup.hex));
        let b = ui.runtime.bounds(popup.palette[0]).unwrap();
        assert!(ui.scene().rectangles.iter().any(
            |r| r.color == theme.tokens().focus && r.bounds == Rect::new(b.x, b.y, b.width, 1.)
        ));
    }
    for (key, index) in [(Key::ArrowDown, 8), (Key::ArrowRight, 9)] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        assert_eq!(ui.runtime.focused(), Some(popup.palette[index]));
        assert_eq!(ui.color_value(owner), Some(Some(Srgba8::WHITE)));
    }
    for (shift, target) in [
        (false, popup.more_colors.unwrap()),
        (true, popup.palette[9]),
    ] {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
            repeat: false,
        })
        .unwrap();
        assert_eq!(ui.runtime.focused(), Some(target));
    }
    let selected = ui.color_editor_settings(owner).unwrap().palette[9];
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert!(!ui.color_popup_is_open(owner));
    assert_eq!(ui.color_value(owner), Some(Some(selected)));
    assert_eq!(ui.color_recent(owner), Some([selected].as_slice()));

    ui.open_color_popup(owner).unwrap();
    assert_eq!(ui.runtime.focused(), Some(popup.palette[9]));
    click(&mut ui, popup.more_colors.unwrap());
    assert_eq!(ui.runtime.focused(), Some(popup.hex));
    ui.set_text(popup.hex, "#bad").unwrap();
    click(&mut ui, popup.apply);
    assert!(ui.color_popup_is_open(owner));
    assert_eq!(ui.color_value(owner), Some(Some(selected)));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert!(!ui.color_popup_is_open(owner));
    assert_eq!(ui.color_recent(owner), Some([selected].as_slice()));
}

#[test]
fn nullable_color_preserves_transparent_rgb_and_clears_only_on_empty_selection() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut schema = FieldSchema::new("paint", FieldType::Text);
    schema.nullable = true;
    let mut descriptor = PropertyDescriptor::new("paint");
    descriptor.metadata.editor = "Color".into();
    let root = ui.root();
    let editor = mount(&mut ui, root, &descriptor, &schema, &CellValue::Null).unwrap();
    ui.update(Size::new(900., 800.)).unwrap();
    let owner = editor.owner();
    ui.open_color_popup(owner).unwrap();
    let popup = ui.color_popup(owner).unwrap().clone();
    click(&mut ui, popup.more_colors.unwrap());
    ui.set_text(popup.hex, "#001234AB").unwrap();
    click(&mut ui, popup.apply);
    assert_eq!(
        editor.read(&ui).unwrap(),
        CellValue::Text("#001234AB".into())
    );
    ui.open_color_popup(owner).unwrap();
    assert_eq!(ui.runtime.focused(), Some(popup.recent[0]));
    click(&mut ui, popup.empty);
    assert!(!ui.color_popup_is_open(owner));
    assert_eq!(editor.read(&ui).unwrap(), CellValue::Null);
}

#[test]
fn property_spectrum_edits_a_preview_preserving_alpha_and_cancel_releases_the_pointer() {
    let original = CellValue::Text("#80404040".into());
    let (mut ui, editor) = fixture("Color", FieldType::Text, original.clone());
    let owner = editor.owner();
    ui.open_color_popup(owner).unwrap();
    let popup = ui.color_popup(owner).unwrap().clone();
    click(&mut ui, popup.more_colors.unwrap());
    let plane = popup.saturation_value.unwrap();
    let hue = popup.hue.unwrap();
    let plane_bounds = ui.runtime.bounds(plane).unwrap();
    let hue_bounds = ui.runtime.bounds(hue).unwrap();
    let cyan = Point::new(
        hue_bounds.x + hue_bounds.width * 0.5,
        hue_bounds.y + hue_bounds.height * 0.5,
    );
    let center = Point::new(
        plane_bounds.x + plane_bounds.width * 0.5,
        plane_bounds.y + plane_bounds.height * 0.5,
    );
    let beyond_top_right = Point::new(
        plane_bounds.x + plane_bounds.width + 24.,
        plane_bounds.y - 12.,
    );
    let down = |position| InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let up = |position| InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };

    ui.dispatch(down(cyan)).unwrap();
    ui.dispatch(up(cyan)).unwrap();
    assert_eq!(ui.document(popup.hex).unwrap().text(), "#80404040");
    ui.dispatch(down(center)).unwrap();
    assert_eq!(ui.runtime.captured(), Some(plane));
    ui.dispatch(InputEvent::PointerMoved {
        position: beyond_top_right,
    })
    .unwrap();
    assert_eq!(ui.document(popup.hex).unwrap().text(), "#8000FFFF");
    assert_eq!(
        ui.color_value(owner),
        Some(Some("#80404040".parse().unwrap()))
    );
    ui.dispatch(up(beyond_top_right)).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.document(popup.hex).unwrap().text(), "#8003FFFF");

    ui.set_text(popup.hex, "#40123456").unwrap();
    ui.update(Size::new(900., 800.)).unwrap();
    assert!(
        ui.semantics()
            .into_iter()
            .any(|node| node.id == hue && node.label.contains("210°"))
    );
    ui.dispatch(down(center)).unwrap();
    assert!(ui.document(popup.hex).unwrap().text().starts_with("#40"));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert!(!ui.color_popup_is_open(owner));
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(editor.read(&ui).unwrap(), original);
}

fn paint_property(kind: &str, objects: &[i64]) -> (Controls, PropertyGrid, DataSourceHandle) {
    use rust_desktop_ui_data::{CollectionAdapter, DataRow, DataSource, DataTable};
    use std::rc::Rc;
    let schema = TableSchema::new(vec![FieldSchema::new("paint", FieldType::Text)]).unwrap();
    let rows = objects
        .iter()
        .map(|&id| DataRow::new(id, vec![CellValue::Text("#abcDEF".into())]))
        .collect();
    let table = DataTable::new(schema, rows).unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ));
    let mut descriptor = PropertyDescriptor::new("paint");
    descriptor.metadata.editor = kind.into();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let grid = PropertyGrid::mount(
        &mut ui,
        root,
        source.clone(),
        objects,
        PropertyGridSettings {
            properties: vec![descriptor],
            ..Default::default()
        },
        &DataFormFactories::default(),
        LayoutStyle {
            width: Length::Percent(1.),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(900., 800.)).unwrap();
    (ui, grid, source)
}

#[test]
fn accepting_a_single_paint_popup_posts_once_and_dismissal_preserves_the_source() {
    for (kind, choice) in [
        ("Color", "#123456"),
        ("Brush", "linear(45;0=#000000;1=#FFFFFF)"),
    ] {
        let (mut ui, mut grid, source) = paint_property(kind, &[1]);
        let owner = grid.property("paint").unwrap();
        let original = CellValue::Text("#abcDEF".into());
        let brush = parse_brush(choice, kind == "Brush").unwrap();
        let expected = CellValue::Text(encode_brush(&brush));
        grid.begin_edit(&mut ui, "paint").unwrap();
        ui.open_color_popup(owner).unwrap();
        assert_eq!(
            ui.apply_color_popup(owner).unwrap(),
            CommitOutcome::Unchanged
        );
        grid.update(&mut ui).unwrap();
        assert_eq!(source.borrow().value(1, "paint"), Some(&original));
        assert!(grid.take_changes().is_empty());
        grid.begin_edit(&mut ui, "paint").unwrap();
        for close_outside in [false, true] {
            ui.open_color_popup(owner).unwrap();
            ui.set_brush_draft(owner, Some(brush.clone())).unwrap();
            let popup = ui.color_popup(owner).unwrap().clone();
            if close_outside {
                ui.close_flyout(popup.flyout).unwrap();
                ui.update(Size::new(900., 800.)).unwrap();
            } else {
                if let Some(more) = popup.more_colors {
                    click(&mut ui, more);
                }
                click(&mut ui, popup.cancel);
            }
            grid.update(&mut ui).unwrap();
            assert!(!ui.color_popup_is_open(owner));
            assert_eq!(source.borrow().value(1, "paint"), Some(&original));
            assert!(grid.take_changes().is_empty());
        }
        ui.open_color_popup(owner).unwrap();
        ui.set_brush_draft(owner, Some(brush)).unwrap();
        let popup = ui.color_popup(owner).unwrap().clone();
        if let Some(more) = popup.more_colors {
            click(&mut ui, more);
        }
        click(&mut ui, popup.apply);
        grid.update(&mut ui).unwrap();
        assert_eq!(source.borrow().value(1, "paint"), Some(&expected));
        let changes = grid.take_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].value, expected);
        grid.update(&mut ui).unwrap();
        assert!(grid.take_changes().is_empty());
    }
}

#[test]
fn mixed_colors_require_a_choice_before_bulk_apply() {
    let (mut ui, mut grid, source) = paint_property("Color", &[1, 2]);
    let original = CellValue::Text("#abcDEF".into());
    let other = CellValue::Text("#660000FF".into());
    source
        .borrow_mut()
        .set_cell(2, "paint", other.clone())
        .unwrap();
    grid.refresh(&mut ui).unwrap();
    let owner = grid.property("paint").unwrap();
    let button = grid.property_editor("paint").unwrap();
    assert_eq!(ui.controls[&button].label, "Разные значения");
    grid.begin_edit(&mut ui, "paint").unwrap();
    ui.open_color_popup(owner).unwrap();
    ui.cancel_color_popup(owner).unwrap();
    assert_eq!(
        grid.commit(&mut ui, "paint").unwrap(),
        PropertyCommit::Unchanged
    );
    assert_eq!(source.borrow().value(1, "paint"), Some(&original));
    assert_eq!(source.borrow().value(2, "paint"), Some(&other));
    assert!(grid.take_changes().is_empty());

    ui.open_color_popup(owner).unwrap();
    ui.set_color_draft(owner, "#00123456".parse().unwrap())
        .unwrap();
    ui.apply_color_popup(owner).unwrap();
    grid.update(&mut ui).unwrap();
    assert_eq!(ui.controls[&button].label, "#00123456");
    assert_eq!(source.borrow().value(2, "paint"), Some(&other));
    assert_eq!(
        grid.commit(&mut ui, "paint").unwrap(),
        PropertyCommit::Saved
    );
    let chosen = CellValue::Text("#00123456".into());
    assert_eq!(source.borrow().value(1, "paint"), Some(&chosen));
    assert_eq!(source.borrow().value(2, "paint"), Some(&chosen));
    assert_eq!(grid.take_changes().len(), 1);
}

#[test]
fn read_only_color_remains_visible_without_a_write_action() {
    use rust_desktop_ui_data::{CollectionAdapter, DataRow, DataSource, DataTable};
    let mut field = FieldSchema::new("paint", FieldType::Text);
    field.read_only = true;
    let table = DataTable::new(
        TableSchema::new(vec![field]).unwrap(),
        vec![DataRow::new(1, vec![CellValue::Text("#80223344".into())])],
    )
    .unwrap();
    let source = std::rc::Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ));
    let mut descriptor = PropertyDescriptor::new("paint");
    descriptor.metadata.editor = "Color".into();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut grid = PropertyGrid::mount(
        &mut ui,
        root,
        source,
        &[1],
        PropertyGridSettings {
            properties: vec![descriptor],
            ..Default::default()
        },
        &DataFormFactories::default(),
        LayoutStyle::default(),
    )
    .unwrap();
    ui.update(Size::new(700., 400.)).unwrap();
    let owner = grid.property("paint").unwrap();
    let button = grid.property_editor("paint").unwrap();
    assert_eq!(ui.controls[&button].label, "#80223344");
    assert!(!ui.is_enabled(button));
    assert!(grid.properties[0].menu_button.is_none());
    assert!(grid.begin_edit(&mut ui, "paint").is_err());
    assert!(ui.open_color_popup(owner).is_err());
    assert!(!ui.color_popup_is_open(owner));
}

#[test]
fn paint_popup_keeps_bulk_and_exact_color_drafts_explicit_and_reports_writer_conflicts() {
    let original = CellValue::Text("#abcDEF".into());
    let changed = CellValue::Text("#FF654321".into());
    let (mut ui, mut grid, source) = paint_property("Color", &[1, 2]);
    let owner = grid.property("paint").unwrap();
    grid.begin_edit(&mut ui, "paint").unwrap();
    ui.open_color_popup(owner).unwrap();
    ui.set_color_draft(owner, "#654321".parse().unwrap())
        .unwrap();
    ui.apply_color_popup(owner).unwrap();
    grid.update(&mut ui).unwrap();
    assert_eq!(source.borrow().value(1, "paint"), Some(&original));
    assert_eq!(source.borrow().value(2, "paint"), Some(&original));
    assert_eq!(
        grid.commit(&mut ui, "paint").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(source.borrow().value(2, "paint"), Some(&changed));

    let (mut ui, mut grid, source) = paint_property("Color", &[1]);
    let owner = grid.property("paint").unwrap();
    grid.begin_edit(&mut ui, "paint").unwrap();
    ui.open_color_popup(owner).unwrap();
    let popup = ui.color_popup(owner).unwrap().clone();
    click(&mut ui, popup.more_colors.unwrap());
    ui.set_text(popup.hex, "#654321").unwrap();
    ui.update(Size::new(900., 800.)).unwrap();
    assert_eq!(source.borrow().value(1, "paint"), Some(&original));
    assert_eq!(
        ui.brush_draft(owner),
        Some(Some(&BrushValue::Solid("#654321".parse().unwrap())))
    );
    let external = CellValue::Text("#112233".into());
    source
        .borrow_mut()
        .set_cell(1, "paint", external.clone())
        .unwrap();
    ui.apply_color_popup(owner).unwrap();
    grid.update(&mut ui).unwrap();
    assert_eq!(source.borrow().value(1, "paint"), Some(&external));
    assert_eq!(grid.draft(&ui, "paint").unwrap(), Some(changed));
    assert!(grid.error().is_some());
    assert!(grid.take_changes().is_empty());
}

#[test]
fn calculator_uses_exact_decimal_and_font_picker_writes_family() {
    let source = CellValue::Decimal("0.1".parse().unwrap());
    let (mut ui, edit) = fixture("Calculator", FieldType::Decimal, source.clone());
    let id = edit.owner();
    ui.open_numeric_calculator(id).unwrap();
    for key in [
        CalculatorKey::Operation(NumericOperation::Add),
        CalculatorKey::Digit(0),
        CalculatorKey::DecimalSeparator,
        CalculatorKey::Digit(2),
        CalculatorKey::Equals,
    ] {
        ui.press_numeric_calculator(id, key).unwrap();
    }
    assert!(edit.read(&ui).is_err());
    ui.apply_numeric_calculator(id).unwrap();
    assert_eq!(
        edit.read(&ui).unwrap(),
        CellValue::Decimal("0.3".parse().unwrap())
    );
    edit.load(&mut ui, &source).unwrap();
    assert_eq!(edit.read(&ui).unwrap(), source);
    let missing = CellValue::Text("Unavailable Inspector Font XYZ".into());
    let (mut ui, font) = fixture("Font", FieldType::Text, missing.clone());
    assert_eq!(font.read(&ui).unwrap(), missing);
    ui.set_text(font.focus(), "unmatched draft family").unwrap();
    assert!(font.read(&ui).is_err());
    font.load(&mut ui, &missing).unwrap();
    assert_eq!(font.read(&ui).unwrap(), missing);
    let family = ui.font_families().unwrap()[0].clone();
    ui.set_selected_keys(font.owner(), std::slice::from_ref(&family))
        .unwrap();
    assert_eq!(font.read(&ui).unwrap(), CellValue::Text(family));
}

#[test]
fn image_editor_retains_encoded_source_and_applies_rotated_pixels_only_on_accept() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="3" height="2"><rect width="2" height="2" fill="#4848ff"/></svg>"##;
    let original = CellValue::Binary(svg.to_vec());
    let (mut ui, mut field) = fixture("Image", FieldType::Binary, original.clone());
    assert_eq!(field.read(&ui).unwrap(), original);
    let image = ui
        .controls
        .keys()
        .copied()
        .find(|id| ui.image_editor_parts(*id).is_some())
        .unwrap();
    click(&mut ui, field.focus());
    field.update(&mut ui).unwrap();
    ui.rotate_image_editor(image, QuarterTurn::Clockwise)
        .unwrap();
    assert!(field.read(&ui).is_err());
    ui.close_flyout(ui.button_flyout(field.focus()).unwrap())
        .unwrap();
    field.update(&mut ui).unwrap();
    assert_eq!(field.read(&ui).unwrap(), original);
    click(&mut ui, field.focus());
    field.update(&mut ui).unwrap();
    ui.rotate_image_editor(image, QuarterTurn::Clockwise)
        .unwrap();
    let finish = ui
        .controls
        .iter()
        .find(|(_, c)| c.label == "Готово")
        .unwrap()
        .0
        .to_owned();
    click(&mut ui, finish);
    field.update(&mut ui).unwrap();
    let CellValue::Binary(result) = field.read(&ui).unwrap() else {
        panic!("expected encoded image")
    };
    let bitmap = image::decode(&result).unwrap().unwrap();
    assert_eq!((bitmap.width(), bitmap.height()), (2, 3));
    assert_eq!(result, encode_bitmap_png(&bitmap).unwrap());
    field.load(&mut ui, &original).unwrap();
    assert_eq!(field.read(&ui).unwrap(), original);
}

#[test]
fn date_picker_requires_a_valid_gregorian_date_and_preserves_cancel() {
    let original = CellValue::Date("2024-02-29".parse().unwrap());
    let (mut ui, mut field) = fixture("DatePicker", FieldType::Date, original.clone());
    let flyout = ui.button_flyout(field.owner()).unwrap();
    let year = ui
        .controls
        .keys()
        .copied()
        .find(|id| ui.spin_settings(*id).is_some_and(|s| s.maximum == 9999.))
        .unwrap();
    let apply = ui
        .controls
        .iter()
        .find(|(_, c)| c.label == "Выбрать")
        .map(|(id, _)| *id)
        .unwrap();
    click(&mut ui, field.focus());
    field.update(&mut ui).unwrap();
    ui.set_spin_value(year, 2023.).unwrap();
    click(&mut ui, apply);
    field.update(&mut ui).unwrap();
    assert!(ui.flyout_is_open(flyout));
    ui.close_flyout(flyout).unwrap();
    field.update(&mut ui).unwrap();
    assert_eq!(field.read(&ui).unwrap(), original);
    click(&mut ui, field.focus());
    field.update(&mut ui).unwrap();
    ui.set_spin_value(year, 2028.).unwrap();
    click(&mut ui, apply);
    field.update(&mut ui).unwrap();
    assert_eq!(
        field.read(&ui).unwrap(),
        CellValue::Date("2028-02-29".parse().unwrap())
    );
}
