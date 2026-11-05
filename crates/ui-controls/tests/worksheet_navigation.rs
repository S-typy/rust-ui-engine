use rust_desktop_ui_controls::{Controls, Theme, spreadsheet::Spreadsheet};
use rust_desktop_ui_core::*;
use rust_desktop_ui_data::workbook::*;

#[test]
fn wheel_direction_both_axes_and_zero_delta_preserve_the_document() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut sheet = Spreadsheet::mount(
        &mut ui,
        root,
        Workbook::new("Scroll").unwrap(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1000.0, 650.0)).unwrap();
    sheet.refresh(&mut ui).unwrap();
    let document = sheet.session().workbook().clone();
    let canvas = ui.runtime.bounds(sheet.parts.canvas).unwrap();
    let position = Point::new(canvas.x + 100.0, canvas.y + 100.0);
    let first = |ui: &Controls, sheet: &Spreadsheet| sheet.semantics(ui).unwrap().cells[0].address;
    assert_eq!(first(&ui, &sheet), CellAddress::new(0, 0).unwrap());
    for (delta, shift, row, column) in [
        (Point::new(0.0, 84.0), false, 3, 0),
        (Point::new(0.0, 84.0), true, 3, 3),
        (Point::new(84.0, -84.0), false, 0, 6),
        (Point::new(-84.0, 0.0), false, 0, 3),
    ] {
        assert!(
            sheet
                .input(
                    &mut ui,
                    &InputEvent::Wheel {
                        position,
                        delta,
                        modifiers: Modifiers {
                            shift,
                            ..Default::default()
                        }
                    }
                )
                .unwrap()
        );
        sheet.refresh(&mut ui).unwrap();
        assert_eq!(first(&ui, &sheet), CellAddress::new(row, column).unwrap());
        assert_eq!(sheet.session().workbook(), &document);
    }
    ui.set_text(sheet.parts.formula, "=SUM(").unwrap();
    for delta in [Point::ZERO, Point::new(0.0, f32::NAN)] {
        assert!(
            !sheet
                .input(
                    &mut ui,
                    &InputEvent::Wheel {
                        position,
                        delta,
                        modifiers: Modifiers::default()
                    }
                )
                .unwrap()
        );
        assert_eq!(ui.document(sheet.parts.formula).unwrap().text(), "=SUM(");
        assert_eq!(sheet.session().workbook(), &document);
    }
}
