use super::*;
use crate::Theme;
use rust_desktop_ui_core::{Mutation, Size};

fn mount() -> (Controls, RichEditor) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let editor = RichEditor::mount(
        &mut ui,
        root,
        RichDocument::from_text("alpha beta\nsecond paragraph").unwrap(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1100.0, 700.0)).unwrap();
    (ui, editor)
}
#[test]
fn formatted_selection_and_unicode_undo_restore_together() {
    let (mut ui, mut editor) = mount();
    let id = editor.parts.editor;
    ui.set_editor_selection(id, 0, 5).unwrap();
    editor.format(&mut ui, RichFormat::Bold).unwrap();
    assert!(
        paragraph_at(editor.document(), 0).unwrap().runs[0]
            .style
            .bold
    );
    assert_eq!(
        ui.document(id).unwrap().selection(),
        Selection {
            anchor: 0,
            focus: 5
        }
    );
    assert!(editor.undo(&mut ui).unwrap());
    assert!(
        !paragraph_at(editor.document(), 0).unwrap().runs[0]
            .style
            .bold
    );
    assert_eq!(
        ui.document(id).unwrap().selection(),
        Selection {
            anchor: 0,
            focus: 5
        }
    );
    assert!(editor.redo(&mut ui).unwrap());
    assert!(
        paragraph_at(editor.document(), 0).unwrap().runs[0]
            .style
            .bold
    );
}
#[test]
fn preedit_is_not_committed_by_format_or_view_change() {
    let (mut ui, mut editor) = mount();
    let id = editor.parts.editor;
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.ime_preedit("候補", Some((0, 6))).unwrap();
    let before = editor.document().clone();
    assert!(editor.format(&mut ui, RichFormat::Bold).is_err());
    assert!(editor.set_view(&mut ui, RichEditorView::Code).is_err());
    assert!(ui.document(id).unwrap().composition().is_some());
    assert_eq!(editor.document(), &before);
    assert!(!editor.sync(&mut ui).unwrap());
}
#[test]
fn native_text_changes_preserve_unaffected_styles() {
    let (mut ui, mut editor) = mount();
    let id = editor.parts.editor;
    ui.set_editor_selection(id, 0, 5).unwrap();
    editor.format(&mut ui, RichFormat::Bold).unwrap();
    ui.set_text(id, "alpha beta🦀").unwrap();
    assert!(editor.sync(&mut ui).unwrap());
    assert!(
        paragraph_at(editor.document(), 0).unwrap().runs[0]
            .style
            .bold
    );
    assert_eq!(
        paragraph_at(editor.document(), 0).unwrap().plain_text(),
        "alpha beta🦀"
    );
    editor.undo(&mut ui).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "alpha beta");
    assert!(
        paragraph_at(editor.document(), 0).unwrap().runs[0]
            .style
            .bold
    );
}
#[test]
fn invalid_code_draft_never_replaces_document() {
    let (mut ui, mut editor) = mount();
    let before = editor.document().clone();
    editor.set_view(&mut ui, RichEditorView::Code).unwrap();
    ui.set_text(editor.parts.editor, "{broken").unwrap();
    assert!(editor.apply_code(&mut ui).is_err());
    assert_eq!(editor.document(), &before);
    assert_eq!(editor.view(), RichEditorView::Code);
    assert_eq!(ui.document(editor.parts.editor).unwrap().text(), "{broken");
    editor.set_view(&mut ui, RichEditorView::Document).unwrap();
    assert_eq!(
        ui.document(editor.parts.editor).unwrap().text(),
        "alpha beta"
    );
}
#[test]
fn code_apply_is_one_undoable_transaction() {
    let (mut ui, mut editor) = mount();
    editor.set_view(&mut ui, RichEditorView::Code).unwrap();
    let changed = RichDocument::from_text("updated").unwrap();
    ui.set_text(editor.parts.editor, &changed.to_json().unwrap())
        .unwrap();
    editor.apply_code(&mut ui).unwrap();
    assert_eq!(editor.document(), &changed);
    editor.undo(&mut ui).unwrap();
    assert_eq!(
        ui.document(editor.parts.editor).unwrap().text(),
        "alpha beta"
    );
}
#[test]
fn table_cell_navigation_and_page_paint_are_live() {
    let (mut ui, mut editor) = mount();
    let mut next = editor.document().clone();
    next.blocks.push(RichBlock::Table(RichTable {
        columns: vec![200.0],
        rows: vec![vec![Paragraph::text("cell")]],
    }));
    editor.replace_document(&mut ui, next).unwrap();
    editor.select_paragraph(&mut ui, 2).unwrap();
    assert_eq!(ui.document(editor.parts.editor).unwrap().text(), "cell");
    editor.refresh(&mut ui).unwrap();
    ui.update(Size::new(1100.0, 700.0)).unwrap();
    assert!(ui.scene().texts.iter().any(|r| r.text == "cell"));
}
#[test]
fn difference_never_splits_joined_graphemes() {
    let (range, text) = text_difference("e\u{301}👩‍💻a", "e\u{301}🦀a");
    assert_eq!(range, "e\u{301}".len().."e\u{301}👩‍💻".len());
    assert_eq!(text, "🦀");
}

#[test]
fn escape_cancels_preedit_without_business_rollback() {
    let (mut ui, mut editor) = mount();
    let id = editor.parts.editor;
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.set_text(id, "changed paragraph").unwrap();
    editor.sync(&mut ui).unwrap();
    let escape = InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Default::default(),
        repeat: false,
    };
    assert!(editor.handle_event(&mut ui, &escape).unwrap());
    assert_eq!(ui.document(id).unwrap().text(), "changed paragraph");
    ui.ime_preedit("候補", None).unwrap();
    assert!(editor.handle_event(&mut ui, &escape).unwrap());
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().text(), "changed paragraph");
}

fn click_toolbar(ui: &mut Controls, editor: &mut RichEditor, button: WidgetId) {
    use rust_desktop_ui_core::{Point, PointerButton};
    assert!(
        ui.is_enabled(button),
        "the requested toolbar action must be enabled"
    );
    ui.update(Size::new(1100.0, 700.0)).unwrap();
    let bounds = ui.runtime.bounds(button).unwrap();
    let position = Point::new(
        bounds.x + bounds.width * 0.5,
        bounds.y + bounds.height * 0.5,
    );
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    let activated = ui.drain_activated();
    assert!(activated.contains(&button));
    for id in activated {
        editor.activate(ui, id).unwrap();
    }
}

#[test]
fn toolbar_format_click_returns_focus_and_next_text_replaces_the_selection() {
    for format in [RichFormat::Bold, RichFormat::Italic, RichFormat::Underline] {
        let (mut ui, mut editor) = mount();
        let id = editor.parts.editor;
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        ui.set_editor_selection(id, 0, 5).unwrap();
        let button = match format {
            RichFormat::Bold => editor.parts.bold,
            RichFormat::Italic => editor.parts.italic,
            RichFormat::Underline => editor.parts.underline,
        };
        click_toolbar(&mut ui, &mut editor, button);
        assert_eq!(ui.runtime.focused(), Some(id));
        assert_eq!(
            ui.document(id).unwrap().selection(),
            Selection {
                anchor: 0,
                focus: 5
            }
        );
        ui.text_input("Z").unwrap();
        editor.sync(&mut ui).unwrap();
        assert_eq!(
            paragraph_at(editor.document(), 0).unwrap().plain_text(),
            "Z beta"
        );
    }
}

#[test]
fn toolbar_undo_and_redo_clicks_keep_keyboard_editing_alive() {
    for redo in [false, true] {
        let (mut ui, mut editor) = mount();
        let id = editor.parts.editor;
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        ui.set_editor_selection(id, 0, 5).unwrap();
        editor.format(&mut ui, RichFormat::Bold).unwrap();
        let button = if redo {
            editor.undo(&mut ui).unwrap();
            editor.parts.redo
        } else {
            editor.parts.undo
        };
        click_toolbar(&mut ui, &mut editor, button);
        assert_eq!(ui.runtime.focused(), Some(id));
        ui.text_input("X").unwrap();
        editor.sync(&mut ui).unwrap();
        assert_eq!(
            paragraph_at(editor.document(), 0).unwrap().plain_text(),
            "X beta"
        );
    }
}

#[test]
fn toolbar_history_actions_enable_the_opposite_action_immediately() {
    let (mut ui, mut editor) = mount();
    let id = editor.parts.editor;
    let undo = editor.parts.undo;
    let redo = editor.parts.redo;
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.set_editor_selection(id, 0, 5).unwrap();
    editor.format(&mut ui, RichFormat::Bold).unwrap();
    assert!(ui.is_enabled(undo));
    assert!(!ui.is_enabled(redo));

    // No extra RichEditor::refresh is allowed between these genuine pointer
    // activations: each accepted action must publish the next available action.
    for button in [undo, redo, undo] {
        click_toolbar(&mut ui, &mut editor, button);
        let is_formatted = button == redo;
        assert_eq!(ui.is_enabled(undo), is_formatted);
        assert_eq!(ui.is_enabled(redo), !is_formatted);
        assert_eq!(editor.can_undo(), is_formatted);
        assert_eq!(editor.can_redo(), !is_formatted);
        assert_eq!(ui.runtime.focused(), Some(id));
        assert_eq!(
            paragraph_at(editor.document(), 0).unwrap().runs[0]
                .style
                .bold,
            is_formatted
        );
    }
    ui.text_input("X").unwrap();
    editor.sync(&mut ui).unwrap();
    assert_eq!(
        paragraph_at(editor.document(), 0).unwrap().plain_text(),
        "X beta"
    );
    assert!(ui.is_enabled(undo));
    assert!(
        !ui.is_enabled(redo),
        "a new edit discards the abandoned redo branch"
    );
}

#[test]
fn invalid_code_draft_blocks_document_replacement_and_history_commands() {
    let (mut ui, mut editor) = mount();
    let id = editor.parts.editor;
    ui.set_editor_selection(id, 0, 5).unwrap();
    editor.format(&mut ui, RichFormat::Bold).unwrap();
    let accepted = editor.document().clone();
    editor.set_view(&mut ui, RichEditorView::Code).unwrap();
    ui.set_text(id, "{ invalid pending Code draft").unwrap();
    assert!(!ui.is_enabled(editor.parts.undo));
    assert!(!ui.is_enabled(editor.parts.redo));
    assert!(editor.undo(&mut ui).is_err());
    assert!(editor.redo(&mut ui).is_err());
    assert!(
        editor
            .replace_document(&mut ui, RichDocument::from_text("replacement").unwrap())
            .is_err()
    );
    assert_eq!(editor.document(), &accepted);
    assert_eq!(editor.view(), RichEditorView::Code);
    assert_eq!(
        ui.document(id).unwrap().text(),
        "{ invalid pending Code draft"
    );
}

#[test]
fn small_valid_page_still_paints_large_font_in_a_large_viewport() {
    let (mut ui, mut editor) = mount();
    let mut document = RichDocument::from_text("i").unwrap();
    document.page = PageSettings {
        width: 100.0,
        height: 300.0,
        margin: 0.0,
        ..Default::default()
    };
    let RichBlock::Paragraph(p) = &mut document.blocks[0] else {
        panic!()
    };
    p.runs[0].style.size = 192.0;
    editor.replace_document(&mut ui, document).unwrap();
    ui.update(Size::new(4000.0, 3000.0)).unwrap();
    editor.refresh(&mut ui).unwrap();
    ui.update(Size::new(4000.0, 3000.0)).unwrap();
    assert!(
        ui.scene()
            .texts
            .iter()
            .filter(|run| run.text == "i")
            .count()
            >= 2,
        "both editor text and preview text must remain painted"
    );
    assert!(
        transform_scene(&editor.layout.pages[0].scene, 4.0, 0.0, 0.0).is_err(),
        "invalid oversized typography must return an error rather than silently dropping text"
    );
}
