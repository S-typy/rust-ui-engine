use super::*;

fn fixture() -> (Controls, EditorControl) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let combo = ui
        .combo_box(
            root,
            "Tags",
            vec![
                SelectionItem::new("a", "Alpha"),
                SelectionItem::new("b", "Beta"),
                SelectionItem::new("c", "Gamma"),
            ],
            ComboBoxSettings {
                editable: true,
                read_only: false,
            },
            LayoutStyle {
                width: Length::Px(320.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_combo_selection_settings(
        combo.container,
        ComboSelectionSettings {
            mode: SelectionMode::Multiple,
            presentation: ComboSelectionPresentation::Tokens,
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    (ui, combo)
}

#[test]
fn short_token_caption_fits_its_painted_text_bounds_after_mount_and_rename() {
    let (mut ui, combo) = fixture();
    for text in ["Автор", "Owner", "Редактор"] {
        ui.set_selection_items(combo.container, vec![SelectionItem::new("a", text)])
            .unwrap();
        ui.set_selected_keys(combo.container, &["a".into()])
            .unwrap();
        ui.update(Size::new(640.0, 480.0)).unwrap();
        let token = ui.combo_token(combo.container, "a").unwrap();
        let content = ui.button_content(token).unwrap();
        let label = ui.runtime.tree().children(content).unwrap()[0];
        let bounds = ui.runtime.bounds(label).unwrap();
        let measured = ui.measure_text(text, 4096.0, false).unwrap().width;
        assert!(
            bounds.width - 16.0 >= measured,
            "{text}: bounds={bounds:?}, text={measured}"
        );
        assert!(ui.runtime.bounds(token).unwrap().width <= 196.0);
    }
}

#[test]
fn checked_combo_draft_cancel_and_commit_publish_accepted_keys_once() {
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into()])
        .unwrap();
    ui.drain_selection_changes();
    ui.open_editor_popup(combo.container).unwrap();
    ui.toggle_combo_draft(combo.container, "b").unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert_eq!(
        ui.combo_selection_draft(combo.container).unwrap(),
        ["a", "b"]
    );
    ui.cancel_combo_selection(combo.container).unwrap();
    assert!(ui.drain_selection_changes().is_empty());
    ui.toggle_combo_draft(combo.container, "c").unwrap();
    assert_eq!(
        ui.commit_combo_selection(combo.container).unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a", "c"]);
    assert_eq!(
        ui.drain_selection_changes()
            .iter()
            .filter(|e| e.id == combo.container)
            .count(),
        1
    );
    assert_eq!(
        ui.commit_combo_selection(combo.container).unwrap(),
        CommitOutcome::Unchanged
    );
    assert!(ui.drain_selection_changes().is_empty());
}

#[test]
fn invalid_visible_source_and_retained_parts_fail_before_any_selection_mutation() {
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into()])
        .unwrap();
    ui.open_editor_popup(combo.container).unwrap();
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let rows = ui.rich.lists[&list].rows.clone();
    let source = ui.selection_items(combo.container).unwrap().to_vec();
    let bad = format!("x{}", "\u{0301}".repeat(100));
    let mut candidate = source.clone();
    candidate[2].text = bad;
    ui.drain_selection_changes();
    assert!(ui.set_selection_items(combo.container, candidate).is_err());
    assert_eq!(ui.selection_items(combo.container).unwrap(), source);
    assert_eq!(ui.selection_items(list).unwrap(), source);
    assert_eq!(ui.rich.lists[&list].rows, rows);
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert!(ui.drain_selection_changes().is_empty());
    let token = ui.combo_token(combo.container, "a").unwrap();
    let content = ui.button_content(token).unwrap();
    assert!(ui.remove(content).is_err());
    ui.runtime.apply(Mutation::Detach(content)).unwrap();
    assert!(
        ui.set_selected_keys(combo.container, &["b".into()])
            .is_err()
    );
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert!(ui.drain_selection_changes().is_empty());
}

#[test]
fn popup_search_preserves_cursor_identity_and_escape_cancels_composition_first() {
    use rust_desktop_ui_core::Modifiers;
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into()])
        .unwrap();
    ui.open_editor_popup(combo.container).unwrap();
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    ui.rich.lists.get_mut(&list).unwrap().cursor = Some(2);
    ui.set_text(combo.editor, "a").unwrap();
    ui.open_editor_popup(combo.container).unwrap();
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let state = &ui.rich.lists[&list];
    assert_eq!(state.items[state.cursor.unwrap()].key, "c");
    ui.toggle_combo_draft(combo.container, "b").unwrap();
    ui.runtime
        .apply(Mutation::Focus(Some(combo.editor)))
        .unwrap();
    ui.ime_preedit("候", Some((0, 3))).unwrap();
    let escape = InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    };
    ui.dispatch(escape.clone()).unwrap();
    assert!(ui.document(combo.editor).unwrap().composition().is_none());
    assert!(ui.rich.popup.is_some());
    assert_eq!(
        ui.combo_selection_draft(combo.container).unwrap(),
        ["a", "b"]
    );
    ui.dispatch(escape).unwrap();
    assert!(ui.rich.popup.is_none());
    assert_eq!(ui.combo_selection_draft(combo.container).unwrap(), ["a"]);
}

#[test]
fn selected_values_binding_publishes_commits_once_and_rejects_query_binding() {
    use crate::{
        BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings,
        DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, combo) = fixture();
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Tags", DataValue::List(vec!["a".into()])),
            ("Query", "bad".into()),
        ]))
        .unwrap(),
    ));
    let mut bindings = ControlBindings::new(context.clone());
    let mut link =
        ControlBinding::new(combo.container, BindingProperty::SelectedValues, "Tags").unwrap();
    link.mode = ControlBindingMode::TwoWay;
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.toggle_combo_draft(combo.container, "b").unwrap();
    bindings.sync(&mut ui);
    assert_eq!(context.borrow().revision(), 0);
    ui.commit_combo_selection(combo.container).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(context.borrow().revision(), 1);
    assert_eq!(
        context.borrow().get("Tags").unwrap(),
        &DataValue::List(vec!["a".into(), "b".into()])
    );
    bindings.sync(&mut ui);
    assert_eq!(context.borrow().revision(), 1);
    let mut invalid = ControlBindings::new(context);
    invalid
        .add(ControlBinding::new(combo.container, BindingProperty::Text, "Query").unwrap())
        .unwrap();
    assert!(invalid.initialize(&mut ui).is_err());
    assert_eq!(ui.document(combo.editor).unwrap().text(), "");
}

#[test]
fn failed_search_popup_candidate_preserves_old_surface_cursor_keys_and_node_count() {
    let (mut ui, combo) = fixture();
    let mut items: Vec<_> = (0..100)
        .map(|i| SelectionItem::new(i.to_string(), format!("Item {i}")))
        .collect();
    items.push(SelectionItem::new(
        "broken",
        format!("Broken x{}", "\u{0301}".repeat(100)),
    ));
    ui.set_selection_items(combo.container, items).unwrap();
    ui.set_selected_keys(combo.container, &["0".into()])
        .unwrap();
    ui.open_editor_popup(combo.container).unwrap();
    let surface = ui.rich.popup.as_ref().unwrap().surface;
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let nodes = ui.controls.len();
    let rows = ui.rich.lists[&list].rows.clone();
    ui.toggle_combo_draft(combo.container, "1").unwrap();
    let nodes_with_draft = ui.controls.len();
    assert!(nodes_with_draft >= nodes);
    ui.set_text(combo.editor, "Broken").unwrap();
    assert!(ui.open_editor_popup(combo.container).is_err());
    assert_eq!(ui.controls.len(), nodes_with_draft);
    assert_eq!(ui.rich.popup.as_ref().unwrap().surface, surface);
    assert_eq!(ui.rich.popup.as_ref().unwrap().query, "");
    assert_eq!(ui.rich.lists[&list].rows, rows);
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["0"]);
    assert_eq!(
        ui.combo_selection_draft(combo.container).unwrap(),
        ["0", "1"]
    );
    assert_eq!(ui.runtime.focused(), Some(combo.editor));
    ui.cancel_combo_selection(combo.container).unwrap();
    assert_eq!(ui.rich.popup.as_ref().unwrap().surface, surface);
    assert_eq!(ui.selected_keys(list).unwrap(), ["0"]);
    ui.toggle_combo_draft(combo.container, "2").unwrap();
    ui.cancel_combo_selection(combo.container).unwrap();
    assert_eq!(ui.selected_keys(list).unwrap(), ["0"]);
}

#[test]
fn validation_preserves_pending_keys_and_token_removal_has_one_accepted_event() {
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into(), "b".into()])
        .unwrap();
    let token_a = ui.combo_token(combo.container, "a").unwrap();
    ui.chain_selection_validator(
        combo.container,
        SelectionValidator::new(|keys| {
            if keys.contains(&"a".into()) {
                Ok(())
            } else {
                Err("Keep Alpha".into())
            }
        }),
    )
    .unwrap();
    ui.drain_selection_changes();
    ui.activate_combo(token_a).unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a", "b"]);
    assert_eq!(ui.combo_selection_draft(combo.container).unwrap(), ["b"]);
    assert_eq!(ui.validation_error(combo.editor), Some("Keep Alpha"));
    assert!(ui.drain_selection_changes().is_empty());
    ui.cancel_combo_selection(combo.container).unwrap();
    let token_a = ui.combo_token(combo.container, "a").unwrap();
    let token_b = ui.combo_token(combo.container, "b").unwrap();
    ui.activate_combo(token_b).unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert_eq!(ui.combo_token(combo.container, "a"), Some(token_a));
    assert_eq!(ui.drain_selection_changes().len(), 1);
}

#[test]
fn source_reorders_retain_tokens_and_deleted_or_disabled_keys_cannot_return() {
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into()])
        .unwrap();
    let token_a = ui.combo_token(combo.container, "a").unwrap();
    ui.open_editor_popup(combo.container).unwrap();
    ui.toggle_combo_draft(combo.container, "b").unwrap();
    ui.set_selection_items(
        combo.container,
        vec![
            SelectionItem::new("c", "Gamma"),
            SelectionItem::new("b", "Beta"),
            SelectionItem::new("a", "Alpha"),
        ],
    )
    .unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert_eq!(ui.combo_token(combo.container, "a"), Some(token_a));
    assert_eq!(
        ui.combo_selection_draft(combo.container).unwrap(),
        ["a", "b"]
    );
    let mut b = SelectionItem::new("b", "Beta");
    b.enabled = false;
    ui.set_selection_items(combo.container, vec![SelectionItem::new("c", "Gamma"), b])
        .unwrap();
    assert!(ui.selected_keys(combo.container).unwrap().is_empty());
    assert!(
        ui.combo_selection_draft(combo.container)
            .unwrap()
            .is_empty()
    );
    assert!(
        ui.set_selected_keys(combo.container, &["b".into()])
            .is_err()
    );
    assert!(
        ui.set_selected_keys(combo.container, &["a".into()])
            .is_err()
    );
    ui.commit_combo_selection(combo.container).unwrap();
    assert!(ui.selected_keys(combo.container).unwrap().is_empty());
}

#[test]
fn invalid_source_replacement_and_long_caption_selection_are_atomic() {
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into()])
        .unwrap();
    ui.chain_selection_validator(
        combo.container,
        SelectionValidator::new(|keys| {
            if keys.is_empty() {
                Err("Required".into())
            } else {
                Ok(())
            }
        }),
    )
    .unwrap();
    let before = ui.selection_items(combo.container).unwrap().to_vec();
    let token = ui.combo_token(combo.container, "a");
    assert!(
        ui.set_selection_items(combo.container, vec![SelectionItem::new("b", "Beta")])
            .is_err()
    );
    assert_eq!(ui.selection_items(combo.container).unwrap(), before);
    assert_eq!(ui.combo_token(combo.container, "a"), token);
    ui.set_selection_items(
        combo.container,
        vec![
            SelectionItem::new("a", "Alpha"),
            SelectionItem::new("x", "x".repeat(4097)),
        ],
    )
    .unwrap();
    assert!(
        ui.set_selected_keys(combo.container, &["x".into()])
            .is_err()
    );
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert_eq!(ui.combo_token(combo.container, "a"), token);
}

#[test]
fn token_keyboard_navigation_removal_and_read_only_share_one_editor_session() {
    use rust_desktop_ui_core::Modifiers;
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into(), "b".into()])
        .unwrap();
    ui.runtime
        .apply(Mutation::Focus(Some(combo.editor)))
        .unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    let key = |ui: &mut Controls, key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap()
    };
    key(&mut ui, Key::ArrowLeft);
    assert_eq!(ui.runtime.focused(), ui.combo_token(combo.container, "b"));
    key(&mut ui, Key::Home);
    assert_eq!(ui.runtime.focused(), ui.combo_token(combo.container, "a"));
    key(&mut ui, Key::Delete);
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["b"]);
    ui.set_composite_read_only(combo.container, true).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    let token = ui.combo_token(combo.container, "b").unwrap();
    ui.activate_combo(token).unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["b"]);
    assert_eq!(ui.composite_editor(combo.container), Some(combo.editor));
}

#[test]
fn popup_keyboard_checked_state_escape_and_blur_keep_accepted_separate() {
    use rust_desktop_ui_core::Modifiers;
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["a".into()])
        .unwrap();
    ui.open_editor_popup(combo.container).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    let key = |ui: &mut Controls, key, modifiers| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        })
        .unwrap()
    };
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    key(
        &mut ui,
        Key::Space,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(
        ui.combo_selection_draft(combo.container).unwrap(),
        ["a", "b"]
    );
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let state = &ui.rich.lists[&list];
    assert_eq!(state.items[state.cursor.unwrap()].key, "b");
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    let state = &ui.rich.lists[&list];
    assert_eq!(state.items[state.cursor.unwrap()].key, "c");
    key(&mut ui, Key::Escape, Modifiers::default());
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a"]);
    assert!(ui.rich.popup.is_none());
    ui.open_editor_popup(combo.container).unwrap();
    ui.toggle_combo_draft(combo.container, "c").unwrap();
    let other = ui
        .button(ui.root(), "Other", None, LayoutStyle::default())
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(other))).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["a", "c"]);
    assert!(ui.rich.popup.is_none());
}

#[test]
fn token_presentation_keyboard_and_accepted_order_share_the_requested_key_sequence() {
    let (mut ui, combo) = fixture();
    ui.set_selected_keys(combo.container, &["c".into(), "a".into(), "b".into()])
        .unwrap();
    let tokens: Vec<_> = ["c", "a", "b"]
        .map(|key| ui.combo_token(combo.container, key).unwrap())
        .into();
    let host = ui.runtime.tree().parent(tokens[0]).unwrap();
    assert_eq!(ui.runtime.tree().children(host).unwrap(), tokens);
    ui.set_selection_items(
        combo.container,
        vec![
            SelectionItem::new("b", "B"),
            SelectionItem::new("c", "C"),
            SelectionItem::new("a", "A"),
        ],
    )
    .unwrap();
    assert_eq!(ui.selected_keys(combo.container).unwrap(), ["c", "a", "b"]);
    assert_eq!(ui.runtime.tree().children(host).unwrap(), tokens);
    ui.set_selected_keys(combo.container, &["b".into(), "a".into(), "c".into()])
        .unwrap();
    assert_eq!(
        ui.runtime.tree().children(host).unwrap(),
        [tokens[2], tokens[1], tokens[0]]
    );
    ui.runtime.apply(Mutation::Focus(Some(tokens[2]))).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowRight,
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(tokens[1]));
}

#[test]
fn token_extent_is_bounded_and_owner_removal_releases_all_sidecars() {
    let (mut ui, combo) = fixture();
    let items: Vec<_> = (0..256)
        .map(|i| SelectionItem::new(i.to_string(), format!("{i} {}", "длинное имя ".repeat(20))))
        .collect();
    let keys: Vec<_> = items.iter().map(|i| i.key.clone()).collect();
    ui.set_selection_items(combo.container, items).unwrap();
    ui.set_selected_keys(combo.container, &keys).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    let bounds = ui.runtime.bounds(combo.container).unwrap();
    let editor = ui.runtime.bounds(combo.editor).unwrap();
    assert!(editor.width >= 24.0 && editor.x + editor.width <= bounds.x + bounds.width);
    let mut extra = keys.clone();
    extra.push("missing".into());
    assert!(ui.set_selected_keys(combo.container, &extra).is_err());
    ui.open_editor_popup(combo.container).unwrap();
    ui.remove(combo.container).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    assert!(ui.combo_selection_settings(combo.container).is_none());
    assert!(ui.combo_token(combo.container, "0").is_none());
    assert!(ui.rich.popup.is_none());
}
