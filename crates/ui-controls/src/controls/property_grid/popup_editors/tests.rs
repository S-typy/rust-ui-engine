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
