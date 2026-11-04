use rust_desktop_ui_controls::{Controls, Theme, spreadsheet::Spreadsheet};
use rust_desktop_ui_core::{InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Size};
use rust_desktop_ui_data::{CellValue, workbook::*};
fn a() -> CellAddress {
    CellAddress::new(0, 0).unwrap()
}
fn fixture() -> (Controls, Spreadsheet) {
    let mut book = Workbook::new("Format").unwrap();
    book.apply(&[WorkbookEdit::Set {
        sheet: 0,
        address: a(),
        cell: WorkbookCell::value(CellValue::Real(12.5)),
    }])
    .unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut sheet = Spreadsheet::mount(
        &mut ui,
        root,
        book,
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1000.0, 650.0)).unwrap();
    sheet.refresh(&mut ui).unwrap();
    (ui, sheet)
}

#[test]
fn preset_application_preserves_typed_value_and_formula_editor_identity() {
    let (mut ui, mut sheet) = fixture();
    let editor = sheet.parts.formula;
    let before = sheet.session().workbook().value(0, a()).unwrap();
    ui.set_selected_keys(sheet.parts.format, &["0.00%".into()])
        .unwrap();
    let apply = sheet.parts.format_apply;
    assert!(sheet.activate(&mut ui, apply).unwrap());
    assert_eq!(sheet.session().workbook().value(0, a()).unwrap(), before);
    assert_eq!(sheet.parts.formula, editor);
    let cells = sheet.semantics(&ui).unwrap().cells;
    assert_eq!(
        cells.iter().find(|c| c.address == a()).unwrap().value,
        "1250,00%"
    );
    let generation = sheet.session().generation();
    sheet.activate(&mut ui, apply).unwrap();
    assert_eq!(sheet.session().generation(), generation);
}

#[test]
fn bad_format_or_formula_keeps_draft_and_history_unchanged() {
    let (mut ui, mut sheet) = fixture();
    let editor = sheet.parts.formula;
    let before = sheet.session().workbook().clone();
    let generation = sheet.session().generation();
    ui.set_text(editor, "=2*3").unwrap();
    assert!(sheet.set_number_format(&mut ui, &"x".repeat(257)).is_err());
    assert_eq!(sheet.session().workbook(), &before);
    assert_eq!(sheet.session().generation(), generation);
    assert_eq!(ui.document(editor).unwrap().text(), "=2*3");
    ui.set_text(editor, "=SUM(").unwrap();
    assert!(sheet.set_number_format(&mut ui, "0.00").is_err());
    assert_eq!(sheet.session().workbook(), &before);
    assert_eq!(sheet.session().generation(), generation);
    assert_eq!(ui.document(editor).unwrap().text(), "=SUM(");
    assert!(!sheet.session().can_undo());
}

#[test]
fn custom_format_remains_selectable_and_picker_fits_both_themes() {
    let (mut ui, mut sheet) = fixture();
    sheet
        .set_number_format(&mut ui, "[$-en-US]#,##0.000")
        .unwrap();
    assert_eq!(
        ui.selected_keys(sheet.parts.format).unwrap(),
        ["[$-en-US]#,##0.000"]
    );
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        for size in [Size::new(760.0, 550.0), Size::new(1200.0, 800.0)] {
            ui.update(size).unwrap();
            sheet.refresh(&mut ui).unwrap();
            for id in [
                sheet.parts.format,
                sheet.parts.format_apply,
                sheet.parts.format_code,
                sheet.parts.format_code_apply,
            ] {
                let b = ui.runtime.bounds(id).unwrap();
                let clip = ui.runtime.clip(id).unwrap();
                let visible = b.intersection(clip).unwrap();
                assert!(visible.width >= b.width - 0.1 && visible.height >= b.height - 0.1);
            }
        }
    }
}

fn input(key: Key) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }
}

#[test]
fn custom_code_draft_is_explicit_and_escape_restores_the_workbook_format() {
    let (mut ui, mut sheet) = fixture();
    let code = sheet.parts.format_code;
    let before = sheet.session().workbook().clone();
    ui.runtime.apply(Mutation::Focus(Some(code))).unwrap();
    ui.set_text(code, "0.000").unwrap();
    ui.runtime
        .apply(Mutation::Focus(Some(sheet.parts.canvas)))
        .unwrap();
    sheet.refresh(&mut ui).unwrap();
    assert_eq!(sheet.session().workbook(), &before);
    assert_eq!(ui.document(code).unwrap().text(), "0.000");
    ui.runtime.apply(Mutation::Focus(Some(code))).unwrap();
    assert!(sheet.input(&mut ui, &input(Key::Escape)).unwrap());
    assert_eq!(ui.document(code).unwrap().text(), "General");
    ui.set_text(code, "0.000").unwrap();
    assert!(sheet.input(&mut ui, &input(Key::Enter)).unwrap());
    assert_eq!(
        sheet.session().workbook().value(0, a()),
        before.value(0, a())
    );
    assert_eq!(
        sheet.session().workbook().sheets()[0]
            .cell(a())
            .unwrap()
            .style
            .number_format,
        "0.000"
    );
    assert_eq!(ui.selected_keys(sheet.parts.format).unwrap(), ["0.000"]);
    assert_eq!(
        sheet
            .semantics(&ui)
            .unwrap()
            .cells
            .iter()
            .find(|c| c.address == a())
            .unwrap()
            .value,
        "12,500"
    );
    let generation = sheet.session().generation();
    sheet.apply_format_code(&mut ui).unwrap();
    assert_eq!(sheet.session().generation(), generation);
    let undo = ui
        .semantics()
        .into_iter()
        .find(|n| n.label == "Undo")
        .unwrap()
        .id;
    sheet.activate(&mut ui, undo).unwrap();
    assert_eq!(sheet.session().workbook(), &before);
    assert_eq!(ui.document(code).unwrap().text(), "General");
}

#[test]
fn rejected_code_preserves_both_drafts_and_a_preset_explicitly_replaces_code() {
    let (mut ui, mut sheet) = fixture();
    let code = sheet.parts.format_code;
    let before = sheet.session().workbook().clone();
    ui.set_text(sheet.parts.formula, "=2*3").unwrap();
    let invalid = "я".repeat(129);
    ui.set_text(code, &invalid).unwrap();
    let apply = sheet.parts.format_code_apply;
    assert!(sheet.activate(&mut ui, apply).is_err());
    assert_eq!(sheet.session().workbook(), &before);
    assert!(!sheet.session().can_undo());
    assert_eq!(ui.document(code).unwrap().text(), invalid);
    assert_eq!(ui.document(sheet.parts.formula).unwrap().text(), "=2*3");
    ui.set_selected_keys(sheet.parts.format, &["0.00%".into()])
        .unwrap();
    let preset = sheet.parts.format_apply;
    sheet.activate(&mut ui, preset).unwrap();
    assert_eq!(ui.document(code).unwrap().text(), "0.00%");
    assert_eq!(sheet.session().entry(), "=2*3");
}

#[test]
fn format_code_composition_cannot_be_published_or_replaced_by_a_preset() {
    let (mut ui, mut sheet) = fixture();
    let code = sheet.parts.format_code;
    let before = sheet.session().workbook().clone();
    ui.runtime.apply(Mutation::Focus(Some(code))).unwrap();
    ui.ime_preedit("に", Some((0, 0))).unwrap();
    assert!(!sheet.input(&mut ui, &input(Key::Enter)).unwrap());
    assert!(sheet.apply_format_code(&mut ui).is_err());
    assert!(sheet.set_number_format(&mut ui, "0.00").is_err());
    assert_eq!(sheet.session().workbook(), &before);
    assert!(ui.document(code).unwrap().composition().is_some());
}
