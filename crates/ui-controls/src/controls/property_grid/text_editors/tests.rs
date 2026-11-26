use super::*;
use rust_desktop_ui_data::{CollectionAdapter, DataRow, DataSource, DataTable};
use std::{cell::RefCell, rc::Rc};

fn grid(
    d: PropertyDescriptor,
    f: FieldSchema,
    initial: &[CellValue],
) -> (Controls, PropertyGrid, DataSourceHandle) {
    let rows: Vec<_> = initial
        .iter()
        .enumerate()
        .map(|(i, v)| DataRow::new(i as i64 + 1, vec![v.clone()]))
        .collect();
    let keys: Vec<_> = (1..=rows.len() as i64).collect();
    let tab = DataTable::new(TableSchema::new(vec![f]).unwrap(), rows).unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(tab)).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let grid = PropertyGrid::mount(
        &mut ui,
        root,
        source.clone(),
        &keys,
        PropertyGridSettings {
            properties: vec![d],
            ..Default::default()
        },
        &DataFormFactories::default(),
        LayoutStyle {
            width: Length::Px(550.),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(600., 460.)).unwrap();
    (ui, grid, source)
}
fn key(ui: &mut Controls, grid: &mut PropertyGrid, key: Key, control: bool) {
    let event = InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
        repeat: false,
    };
    if !grid.input(ui, &event).unwrap() {
        ui.dispatch(event).unwrap();
    }
    grid.update(ui).unwrap();
    ui.update(Size::new(600., 460.)).unwrap();
}
fn assert_secret_hidden(ui: &Controls, secrets: &[&str]) {
    let semantic = format!("{:?}", ui.semantics());
    for secret in secrets {
        assert!(!semantic.contains(secret));
        assert!(
            ui.scene()
                .texts
                .iter()
                .all(|run| !run.text.contains(secret))
        );
    }
}
#[test]
fn inspector_password_remains_protected_when_mixed_active_resting_and_read_only() {
    let old = "one-sensitive-value";
    let second = "two-sensitive-value";
    let changed = "new-sensitive-value";
    let (mut ui, mut inspector, source) = grid(
        descriptor("Password"),
        FieldSchema::new("subject", FieldType::Text),
        &[text(old), text(second)],
    );
    assert_secret_hidden(&ui, &[old, second]);
    inspector.begin_edit(&mut ui, "subject").unwrap();
    let editor = inspector.property_editor("subject").unwrap();
    assert_eq!(ui.password_value(editor).unwrap().expose_secret(), "");
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.ime_preedit("候", Some((0, 3))).unwrap();
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Pending
    );
    key(&mut ui, &mut inspector, Key::Escape, false);
    assert_eq!(ui.password_composition_active(editor), Some(false));
    assert_eq!(inspector.accepted("subject"), Some(PropertyValue::Mixed));
    key(&mut ui, &mut inspector, Key::Character("a".into()), true);
    for shortcut in ["c", "x"] {
        key(
            &mut ui,
            &mut inspector,
            Key::Character(shortcut.into()),
            true,
        );
    }
    assert!(ui.drain_clipboard_requests().is_empty());
    ui.text_input(changed).unwrap();
    assert_eq!(
        inspector.draft(&ui, "subject").unwrap(),
        Some(text(changed))
    );
    ui.update(Size::new(600., 460.)).unwrap();
    assert_secret_hidden(&ui, &[old, second, changed]);
    assert!(
        ui.drain_text_changes()
            .iter()
            .all(|ev| !format!("{ev:?}").contains(changed))
    );
    assert!(
        ui.drain_edit_events()
            .iter()
            .all(|ev| !format!("{ev:?}").contains(changed))
    );
    assert_eq!(source.borrow().value(1, "subject"), Some(&text(old)));
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(source.borrow().value(2, "subject"), Some(&text(changed)));
    ui.update(Size::new(600., 460.)).unwrap();
    assert_secret_hidden(&ui, &[old, second, changed]);
    let mut d = descriptor("Password");
    d.metadata.read_only = true;
    let (ui, _, _) = grid(
        d,
        FieldSchema::new("subject", FieldType::Text),
        &[text(changed)],
    );
    assert_secret_hidden(&ui, &[changed]);
}
#[test]
fn typed_prefix_commits_stable_key_and_escape_restores_previous_source() {
    let (mut ui, mut inspector, source) = grid(
        choices("AutoComplete"),
        FieldSchema::new("subject", FieldType::Integer),
        &[CellValue::Integer(31)],
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    let id = inspector.property_editor("subject").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, &mut inspector, Key::Character("a".into()), true);
    ui.text_input("Ту").unwrap();
    inspector.update(&mut ui).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Тула");
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&CellValue::Integer(45))
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    ui.set_text(id, "Москва").unwrap();
    if ui.popup_open() {
        key(&mut ui, &mut inspector, Key::Escape, false);
    }
    key(&mut ui, &mut inspector, Key::Escape, false);
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&CellValue::Integer(45))
    );
    assert_eq!(ui.document(id).unwrap().text(), "Тула");
}
#[test]
fn free_suggestion_and_ordered_token_draft_roundtrip_through_property_source() {
    let mut d = choices("AutoSuggest");
    d.choice_keys.clear();
    let (mut ui, mut inspector, source) = grid(
        d,
        FieldSchema::new("subject", FieldType::Text),
        &[text("Свободный адрес")],
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    let editor = inspector.property_editor("subject").unwrap();
    ui.set_text(editor, "Новый адрес").unwrap();
    inspector.commit(&mut ui, "subject").unwrap();
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&text("Новый адрес"))
    );
    let original = text(r#"[ "45", "12" ]"#);
    let (mut ui, mut inspector, source) = grid(
        choices("TokenList"),
        FieldSchema::new("subject", FieldType::Text),
        std::slice::from_ref(&original),
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Unchanged
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    let owner = inspector.property("subject").unwrap();
    ui.open_editor_popup(owner).unwrap();
    ui.toggle_combo_draft(owner, "31").unwrap();
    assert_eq!(source.borrow().value(1, "subject"), Some(&original));
    inspector.cancel(&mut ui, "subject").unwrap();
    assert_eq!(ui.combo_selection_draft(owner).unwrap(), ["45", "12"]);
    inspector.begin_edit(&mut ui, "subject").unwrap();
    ui.toggle_combo_draft(owner, "31").unwrap();
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&text(r#"["45","12","31"]"#))
    );
    for invalid in [r#"["45","45"]"#, r#"["x"]"#, r#"["12",3]"#] {
        assert!(
            inspector
                .set_draft(&mut ui, "subject", text(invalid))
                .is_err()
        );
    }
}

#[test]
fn narrow_token_cell_shows_complete_chips_and_restores_search_space_on_activation() {
    let mut d = descriptor("TokenList");
    d.choices = ["Атлас", "Дизайн", "Печать"].map(str::to_owned).into();
    d.choice_keys = ["atlas", "design", "print"].map(str::to_owned).into();
    let (mut ui, mut inspector, _) = grid(
        d,
        FieldSchema::new("subject", FieldType::Text),
        &[text(r#"["atlas","print"]"#)],
    );
    ui.edit_props(inspector.container, |n| n.style.width = Length::Px(421.))
        .unwrap();
    let size = Size::new(460., 360.);
    ui.update(size).unwrap();
    ui.update(size).unwrap();
    let owner = inspector.property("subject").unwrap();
    let editor = inspector.property_editor("subject").unwrap();
    let last = ui.combo_token(owner, "print").unwrap();
    let content = ui.runtime.tree().parent(last).unwrap();
    let tray = ui.runtime.tree().parent(content).unwrap();
    let bounds = |ui: &Controls, id| ui.runtime.bounds(id).unwrap();
    let cell = bounds(&ui, owner);
    let full_chip = bounds(&ui, last);
    let rest = bounds(&ui, tray);
    assert!(
        full_chip.x + full_chip.width <= rest.x + rest.width + 0.5,
        "second chip {full_chip:?} clipped inside {rest:?}"
    );
    assert!(
        full_chip.x + full_chip.width - rest.x > (cell.width - 54.) * 0.7,
        "fixture must exercise the old clipping boundary"
    );
    assert!(
        ui.scene()
            .texts
            .iter()
            .any(|run| run.text == "Печать" && run.clip.width >= run.bounds.width - 0.5)
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    ui.update(size).unwrap();
    ui.update(size).unwrap();
    assert!(bounds(&ui, tray).width < rest.width && bounds(&ui, editor).width >= 28.);
    ui.set_text(editor, "Печ").unwrap();
    ui.open_editor_popup(owner).unwrap();
    ui.update(size).unwrap();
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    assert_eq!(
        ui.selection_items(list)
            .unwrap()
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>(),
        ["print"]
    );
    inspector.cancel(&mut ui, "subject").unwrap();
    ui.update(size).unwrap();
    ui.update(size).unwrap();
    assert!((bounds(&ui, tray).width - rest.width).abs() < 0.5);
}

#[test]
fn searchable_property_lookup_filters_by_id_and_accepts_keyboard_selection() {
    let (mut ui, mut inspector, source) = grid(
        choices("SearchLookup"),
        FieldSchema::new("subject", FieldType::Integer),
        &[CellValue::Integer(31)],
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    let owner = inspector.property("subject").unwrap();
    let editor = inspector.property_editor("subject").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    key(&mut ui, &mut inspector, Key::Character("a".into()), true);
    ui.text_input("45").unwrap();
    inspector.update(&mut ui).unwrap();
    ui.update(Size::new(600., 460.)).unwrap();
    let list = ui
        .rich
        .popup
        .as_ref()
        .filter(|p| p.owner == owner)
        .and_then(|p| p.list)
        .unwrap();
    let filtered = ui.selection_items(list).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].text, "Тула");
    key(&mut ui, &mut inspector, Key::ArrowDown, false);
    key(&mut ui, &mut inspector, Key::Enter, false);
    assert!(!ui.popup_open());
    assert_eq!(ui.selected_keys(owner).unwrap(), ["45"]);
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&CellValue::Integer(31))
    );
    key(&mut ui, &mut inspector, Key::Enter, false);
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&CellValue::Integer(45))
    );
}

#[test]
fn duplicate_captions_do_not_collapse_distinct_property_keys() {
    let mut d = choices("SearchLookup");
    d.choices[1] = "Москва".into();
    let (mut ui, mut inspector, source) = grid(
        d,
        FieldSchema::new("subject", FieldType::Integer),
        &[CellValue::Integer(45)],
    );
    inspector.begin_edit(&mut ui, "subject").unwrap();
    let owner = inspector.property("subject").unwrap();
    let editor = inspector.property_editor("subject").unwrap();
    ui.set_text(editor, "Москва").unwrap();
    assert!(matches!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Rejected(_)
    ));
    ui.set_selected_keys(owner, &["31".into()]).unwrap();
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&CellValue::Integer(31))
    );
    ui.set_selected_keys(owner, &["12".into()]).unwrap();
    assert_eq!(
        inspector.commit(&mut ui, "subject").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(
        source.borrow().value(1, "subject"),
        Some(&CellValue::Integer(12))
    );
}

fn descriptor(editor: &str) -> PropertyDescriptor {
    let mut d = PropertyDescriptor::new("subject");
    d.metadata.editor = editor.into();
    d
}
fn setup(
    d: &PropertyDescriptor,
    kind: FieldType,
    v: CellValue,
) -> (Controls, Box<dyn InspectorEditor>) {
    let f = FieldSchema::new("subject", kind);
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let e = mount(&mut ui, root, d, &f, &v).unwrap();
    ui.update(Size::new(640., 480.)).unwrap();
    (ui, e)
}
fn text(s: &str) -> CellValue {
    CellValue::Text(s.into())
}

#[test]
fn masks_distinguish_storage_and_whole_regular_expression() {
    let mut d = descriptor("MaskedText");
    d.mask = Some("00000-000-0000".into());
    let (mut ui, e) = setup(&d, FieldType::Text, text("070684900012"));
    assert_eq!(ui.document(e.focus()).unwrap().text(), "07068-490-0012");
    assert_eq!(
        e.rest_label(&text("003330001234")).as_deref(),
        Some("00333-000-1234")
    );
    assert_eq!(e.read(&ui).unwrap(), text("070684900012"));
    assert!(e.load(&mut ui, &text("x")).is_err());
    d.mask = Some(r"regex:(\d{5}|[A-Z]{2})-\d{3}-\d{4}".into());
    let (mut ui, e) = setup(&d, FieldType::Text, text("07068-490-0012"));
    ui.set_text(e.focus(), "AB-921-0000").unwrap();
    assert_eq!(e.read(&ui).unwrap(), text("AB-921-0000"));
    ui.set_text(e.focus(), "AB-921-0000x").unwrap();
    assert!(e.read(&ui).is_err());
    assert!(matches!(
        ui.commit_edit(e.focus()).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    d.mask = Some("regex:(".into());
    assert!(
        validate(
            &d,
            &FieldSchema::new("subject", FieldType::Text),
            &text("x")
        )
        .is_err()
    );
}

#[test]
fn protected_input_has_no_text_document_or_clear_caption() {
    let secret = "testing-unique-secret";
    let (mut ui, e) = setup(&descriptor("Password"), FieldType::Text, text(secret));
    assert!(ui.document(e.focus()).is_none());
    assert_eq!(e.rest_label(&text(secret)).unwrap(), "••••••");
    assert!(!ui.reveal_password(e.focus()).unwrap());
    assert!(!format!("{:?}", ui.scene()).contains(secret));
    assert_eq!(e.read(&ui).unwrap(), text(secret));
    e.load(&mut ui, &text("replacement")).unwrap();
    assert_eq!(e.read(&ui).unwrap(), text("replacement"));
}

#[test]
fn numeric_text_is_exact_and_button_and_memo_are_retained_editors() {
    let mut padded = descriptor("NumericText");
    padded.mask = Some("D4".into());
    let (ui, e) = setup(&padded, FieldType::Integer, CellValue::Integer(-7));
    assert_eq!(ui.document(e.focus()).unwrap().text(), "-0007");
    assert_eq!(e.read(&ui).unwrap(), CellValue::Integer(-7));
    assert_eq!(
        e.rest_label(&CellValue::Integer(3)).as_deref(),
        Some("0003")
    );
    let (mut ui, e) = setup(
        &descriptor("NumericText"),
        FieldType::Integer,
        CellValue::Integer(i64::MIN),
    );
    ui.set_text(e.focus(), "9223372036854775808").unwrap();
    assert!(e.read(&ui).is_err());
    ui.set_text(e.focus(), "-78").unwrap();
    assert_eq!(e.read(&ui).unwrap(), CellValue::Integer(-78));
    let (mut ui, e) = setup(&descriptor("NumericText"), FieldType::Text, text("0002"));
    assert_eq!(e.read(&ui).unwrap(), text("0002"));
    ui.set_text(e.focus(), "12a").unwrap();
    assert!(e.read(&ui).is_err());
    let (mut ui, e) = setup(&descriptor("ButtonText"), FieldType::Text, text("clear"));
    let clear = ui.rich.composites[&e.owner()].parts.buttons[0];
    ui.activate(clear).unwrap();
    assert_eq!(e.read(&ui).unwrap(), text(""));
    let (mut ui, e) = setup(&descriptor("Memo"), FieldType::Text, text("один\nдва"));
    let memo = ui.memo_parts(e.owner()).unwrap();
    let preview = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == "один…")
        .unwrap();
    assert_eq!(preview.align, rust_desktop_ui_core::TextAlign::Start);
    let arrows=ui.scene().stroke_paths.iter().filter(|path|{
        matches!(path.segments.as_slice(),[a,b] if a.end==b.start && a.start.x<a.end.x && a.start.y<a.end.y && b.end.x>b.start.x && b.end.y<b.start.y)
    }).count();
    assert_eq!(arrows, 1, "the memo popup owns the disclosure arrow");
    ui.open_memo(memo).unwrap();
    assert!(ui.memo_is_open(memo));
    ui.set_text(e.focus(), "три\nчетыре").unwrap();
    ui.cancel_memo(memo).unwrap();
    assert_eq!(e.read(&ui).unwrap(), text("один\nдва"));
}

fn choices(kind: &str) -> PropertyDescriptor {
    let mut d = descriptor(kind);
    d.choices = ["Москва", "Можайск", "Тула"].map(str::to_owned).into();
    d.choice_keys = ["12", "31", "45"].map(str::to_owned).into();
    d
}
#[test]
fn lookups_write_keys_and_radio_activation_changes_the_draft() {
    for kind in ["Lookup", "SearchLookup"] {
        let d = choices(kind);
        let (mut ui, e) = setup(&d, FieldType::Integer, CellValue::Integer(12));
        assert_eq!(
            e.rest_label(&CellValue::Integer(12)).as_deref(),
            Some("Москва")
        );
        assert_eq!(ui.document(e.focus()).unwrap().text(), "Москва");
        ui.open_editor_popup(e.owner()).unwrap();
        ui.set_text(e.focus(), "45").unwrap();
        ui.update(Size::new(640., 480.)).unwrap();
        let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
        let items = ui.selection_items(list).unwrap();
        if kind == "SearchLookup" {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].key, "45");
        } else {
            assert!(items.is_empty());
        }
        ui.set_text(e.focus(), "за пределами справочника").unwrap();
        assert!(e.read(&ui).is_err());
        e.load(&mut ui, &CellValue::Integer(31)).unwrap();
        assert_eq!(ui.document(e.focus()).unwrap().text(), "Можайск");
        assert_eq!(e.read(&ui).unwrap(), CellValue::Integer(31));
    }
    let (mut ui, e) = setup(
        &choices("ListBox"),
        FieldType::Integer,
        CellValue::Integer(31),
    );
    let radio = ui.radio_item_widget(e.owner(), "45").unwrap();
    ui.activate(radio).unwrap();
    assert_eq!(e.read(&ui).unwrap(), CellValue::Integer(45));
    assert!(e.inline());
}

#[test]
fn completion_selects_suffix_and_link_emits_only_activated_address() {
    let (mut ui, mut e) = setup(
        &choices("AutoComplete"),
        FieldType::Integer,
        CellValue::Integer(12),
    );
    ui.runtime.apply(Mutation::Focus(Some(e.focus()))).unwrap();
    ui.set_text(e.focus(), "Ту").unwrap();
    ui.set_editor_selection(e.focus(), "Ту".len(), "Ту".len())
        .unwrap();
    e.update(&mut ui).unwrap();
    let doc = ui.document(e.focus()).unwrap();
    assert_eq!(doc.text(), "Тула");
    assert_eq!(doc.selection().anchor, "Ту".len());
    assert_eq!(e.read(&ui).unwrap(), CellValue::Integer(45));
    let d = descriptor("Hyperlink");
    let target = "https://example.org/catalog?a=1";
    let (mut ui, mut e) = setup(&d, FieldType::Text, text(target));
    let line = ui.runtime.tree().children(e.owner()).unwrap()[0];
    let link = ui.runtime.tree().children(line).unwrap()[0];
    ui.activate(link).unwrap();
    e.update(&mut ui).unwrap();
    assert_eq!(e.take_navigation().as_deref(), Some(target));
    assert!(e.take_navigation().is_none());
    for unsafe_target in ["file:///windows", "https://a@b", "https://a\\b", "http://"] {
        assert!(e.load(&mut ui, &text(unsafe_target)).is_err());
    }
}

#[test]
fn link_caption_is_start_aligned_and_one_encoded_component_is_sent_to_the_host() {
    let mut d = descriptor("Hyperlink");
    d.uri_format = Some("https://example.org/issues?q={0}".into());
    d.metadata.read_only = true;
    let (mut ui, mut inspector, _) = grid(
        d.clone(),
        FieldSchema::new("subject", FieldType::Text),
        &[text("UI & Я")],
    );
    let host = inspector.property("subject").unwrap();
    let line = ui.runtime.tree().children(host).unwrap()[0];
    let button = ui.runtime.tree().children(line).unwrap()[0];
    assert!(inspector.begin_edit(&mut ui, "subject").is_err());
    let bounds = ui.runtime.bounds(button).unwrap();
    let cell = ui.runtime.bounds(host).unwrap();
    assert!((bounds.x - cell.x).abs() < 1. && bounds.width < cell.width * 0.7);
    ui.activate(button).unwrap();
    inspector.update(&mut ui).unwrap();
    assert_eq!(
        inspector.take_navigation_requests(),
        [(
            "subject".to_owned(),
            "https://example.org/issues?q=UI%20%26%20%D0%AF".to_owned()
        )]
    );
    for invalid in [
        "https://{0}/",
        "file:///entry/{0}",
        "https://example.org/{0}/{0}",
        "prefix{0}",
        "https://example.org/path",
    ] {
        d.uri_format = Some(invalid.to_owned());
        assert!(
            validate(
                &d,
                &FieldSchema::new("subject", FieldType::Text),
                &text("x")
            )
            .is_err()
        );
    }
}
