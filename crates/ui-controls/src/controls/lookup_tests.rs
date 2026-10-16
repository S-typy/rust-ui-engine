use super::*;
use crate::{AsyncEditorValidator, EditorValidationStatus};
use rust_desktop_ui_core::Modifiers;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

fn data() -> Vec<SelectionItem> {
    [
        ("a", "Анна", "Москва"),
        ("b", "Борис", "Казань"),
        ("c", "Anna", "London"),
    ]
    .into_iter()
    .map(|(key, text, city)| {
        let mut item = SelectionItem::new(key, text);
        item.columns.push(city.into());
        item
    })
    .collect()
}
fn setup(items: Vec<SelectionItem>) -> (Controls, EditorControl) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .lookup_box(
            ui.root(),
            "Reference",
            items,
            vec!["Name".into(), "City".into()],
            LayoutStyle {
                width: Length::Px(360.0),
                height: Length::Px(40.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, parts)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}
fn popup(ui: &Controls, owner: WidgetId) -> WidgetId {
    ui.rich
        .popup
        .as_ref()
        .filter(|p| p.owner == owner)
        .unwrap()
        .list
        .unwrap()
}
fn query(ui: &mut Controls, parts: EditorControl, text: &str) {
    ui.set_text(parts.editor, text).unwrap();
    if ui.rich.popup.is_none() {
        ui.open_editor_popup(parts.container).unwrap();
    }
    frame(ui);
}
fn visible_keys(ui: &Controls, owner: WidgetId) -> Vec<String> {
    ui.selection_items(popup(ui, owner))
        .unwrap()
        .iter()
        .map(|i| i.key.clone())
        .collect()
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(ui);
}

#[test]
fn search_columns_modes_and_unicode_comparison_are_explicit() {
    let (mut ui, c) = setup(data());
    query(&mut ui, c.clone(), "МОС");
    assert_eq!(visible_keys(&ui, c.container), ["a"]);
    ui.set_lookup_search_settings(
        c.container,
        LookupSearchSettings {
            columns: vec![0],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(visible_keys(&ui, c.container).is_empty());
    query(&mut ui, c.clone(), "АН");
    assert_eq!(visible_keys(&ui, c.container), ["a"]);
    ui.set_lookup_search_settings(
        c.container,
        LookupSearchSettings {
            columns: vec![0],
            comparison: LookupSearchComparison::Ordinal,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(visible_keys(&ui, c.container).is_empty());
    ui.set_lookup_search_settings(
        c.container,
        LookupSearchSettings {
            columns: vec![1],
            mode: LookupSearchMode::StartsWith,
            ..Default::default()
        },
    )
    .unwrap();
    query(&mut ui, c.clone(), "зань");
    assert!(visible_keys(&ui, c.container).is_empty());
    query(&mut ui, c.clone(), "каз");
    assert_eq!(visible_keys(&ui, c.container), ["b"]);
    assert_eq!(ui.selected_keys(c.container).unwrap(), [] as [String; 0]);
}

#[test]
fn search_policy_validation_and_failure_do_not_change_open_popup() {
    let (mut ui, c) = setup(data());
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let list = popup(&ui, c.container);
    let rows = ui.rich.lists[&list].rows.clone();
    for columns in [vec![0, 0], vec![2], vec![32]] {
        assert!(
            ui.set_lookup_search_settings(
                c.container,
                LookupSearchSettings {
                    columns,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert_eq!(popup(&ui, c.container), list);
    assert_eq!(ui.rich.lists[&list].rows, rows);
    assert_eq!(
        ui.lookup_search_settings(c.container),
        Some(LookupSearchSettings::default())
    );
}

#[test]
fn ambiguous_typed_caption_does_not_choose_a_key_but_existing_key_survives() {
    let mut items = data();
    items[1].text = items[0].text.clone();
    let (mut ui, c) = setup(items);
    ui.set_text(c.editor, "Анна").unwrap();
    assert!(matches!(
        ui.commit_edit(c.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
    ui.set_selected_keys(c.container, &["b".into()]).unwrap();
    assert_eq!(ui.commit_edit(c.editor).unwrap(), CommitOutcome::Unchanged);
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["b"]);
    let mut next = ui.selection_items(c.container).unwrap().to_vec();
    next.reverse();
    ui.set_selection_items(c.container, next).unwrap();
    assert_eq!(ui.commit_edit(c.editor).unwrap(), CommitOutcome::Unchanged);
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["b"]);
}

#[test]
fn disabled_reference_keys_and_custom_text_are_never_accepted() {
    let mut items = data();
    items[1].enabled = false;
    let (mut ui, c) = setup(items);
    assert!(ui.set_selected_keys(c.container, &["b".into()]).is_err());
    for text in ["Борис", "Not a record"] {
        ui.set_text(c.editor, text).unwrap();
        assert!(matches!(
            ui.commit_edit(c.editor).unwrap(),
            CommitOutcome::Invalid(_)
        ));
        assert!(ui.selected_keys(c.container).unwrap().is_empty());
    }
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    let mut next = data();
    next[0].enabled = false;
    ui.set_selection_items(c.container, next).unwrap();
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
    assert_eq!(ui.accepted_value(c.editor), Some(""));
}

#[test]
fn source_replacement_preserves_query_composition_navigation_and_retained_rows() {
    let (mut ui, c) = setup(data());
    query(&mut ui, c.clone(), "а");
    key(&mut ui, Key::ArrowDown);
    let list = popup(&ui, c.container);
    let cursor = ui.rich.lists[&list].cursor.unwrap();
    let cursor_key = ui.selection_items(list).unwrap()[cursor].key.clone();
    let row = ui.list_row_widget(list, &cursor_key).unwrap();
    ui.ime_preedit("候", Some((0, 3))).unwrap();
    let mut next = data();
    next.reverse();
    ui.set_selection_items(c.container, next).unwrap();
    assert_eq!(popup(&ui, c.container), list);
    assert_eq!(ui.list_row_widget(list, &cursor_key), Some(row));
    let cursor = ui.rich.lists[&list].cursor.unwrap();
    assert_eq!(ui.selection_items(list).unwrap()[cursor].key, cursor_key);
    assert_eq!(ui.document(c.editor).unwrap().text(), "а");
    assert!(ui.document(c.editor).unwrap().composition().is_some());
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
}

#[test]
fn source_preflight_or_validator_failure_preserves_accepted_and_popup_state() {
    let (mut ui, c) = setup(data());
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let list = popup(&ui, c.container);
    let rows = ui.rich.lists[&list].rows.clone();
    let old = ui.selection_items(c.container).unwrap().to_vec();
    let mut next = old.clone();
    next[1].text = format!("A{}", "\u{301}".repeat(40));
    assert!(ui.set_selection_items(c.container, next).is_err());
    assert_eq!(ui.selection_items(c.container), Some(old.as_slice()));
    assert_eq!(ui.rich.lists[&list].rows, rows);
    ui.chain_selection_validator(
        c.container,
        SelectionValidator::new(|keys| {
            if keys.is_empty() {
                Err("Required".into())
            } else {
                Ok(())
            }
        }),
    )
    .unwrap();
    assert!(
        ui.set_selection_items(c.container, vec![old[1].clone()])
            .is_err()
    );
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
    assert_eq!(ui.selection_items(c.container), Some(old.as_slice()));
    assert_eq!(ui.rich.lists[&list].rows, rows);
    assert_eq!(ui.accepted_value(c.editor), Some("Анна"));
}

#[test]
fn multiple_lookup_search_uses_column_policy_and_preserves_offscreen_pending_keys() {
    let (mut ui, c) = setup(data());
    ui.set_combo_selection_settings(
        c.container,
        ComboSelectionSettings {
            mode: SelectionMode::Multiple,
            presentation: ComboSelectionPresentation::Tokens,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    ui.open_editor_popup(c.container).unwrap();
    ui.toggle_combo_draft(c.container, "b").unwrap();
    ui.set_lookup_search_settings(
        c.container,
        LookupSearchSettings {
            columns: vec![1],
            ..Default::default()
        },
    )
    .unwrap();
    query(&mut ui, c.clone(), "London");
    assert_eq!(visible_keys(&ui, c.container), ["c"]);
    assert_eq!(ui.combo_selection_draft(c.container).unwrap(), ["a", "b"]);
    ui.toggle_combo_draft(c.container, "c").unwrap();
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
    ui.cancel_combo_selection(c.container).unwrap();
    assert_eq!(ui.combo_selection_draft(c.container).unwrap(), ["a"]);
    ui.toggle_combo_draft(c.container, "b").unwrap();
    assert_eq!(
        ui.commit_combo_selection(c.container).unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a", "b"]);
}

#[test]
fn source_change_invalidates_async_caption_request_before_reusing_caption_for_new_key() {
    let (mut ui, c) = setup(data());
    let (send, recv) = mpsc::channel();
    let (done, completed) = mpsc::channel();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        done.send(()).unwrap();
    })));
    let recv = Arc::new(Mutex::new(recv));
    ui.set_async_editor_validator(
        c.editor,
        Some(AsyncEditorValidator::new(move |_| {
            recv.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    ui.set_text(c.editor, "Анна").unwrap();
    assert_eq!(ui.commit_edit(c.editor).unwrap(), CommitOutcome::Pending);
    let mut next = data();
    next[0].key = "replacement".into();
    ui.set_selection_items(c.container, next).unwrap();
    assert_eq!(
        ui.editor_validation_status(c.editor),
        EditorValidationStatus::Idle
    );
    send.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(3)).unwrap();
    ui.poll_editor_validation().unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some(""));
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
    assert_eq!(ui.document(c.editor).unwrap().text(), "Анна");
}

#[test]
fn accepted_caption_normalizes_single_line_and_hidden_baseline_is_preflighted() {
    let mut items = data();
    items[0].text = "Ан\nна".into();
    let (mut ui, c) = setup(items.clone());
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("Анна"));
    ui.set_text(c.editor, "Unfinished").unwrap();
    items[0].text = format!("A{}", "\u{301}".repeat(40));
    assert!(ui.set_selection_items(c.container, items).is_err());
    assert_eq!(ui.accepted_value(c.editor), Some("Анна"));
    assert_eq!(ui.document(c.editor).unwrap().text(), "Unfinished");
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
}

#[test]
fn unicode_search_has_no_accent_folding_or_implicit_normalization() {
    let labels = ["Ёжик", "İstanbul", "نموذج", "é", "e\u{301}", "Straße"];
    let records = labels
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let mut item = SelectionItem::new(i.to_string(), s);
            item.columns.push(String::new());
            item
        })
        .collect();
    let (mut ui, c) = setup(records);
    for (needle, expected) in [
        ("ёж", vec!["0"]),
        ("еж", vec![]),
        ("i\u{307}s", vec!["1"]),
        ("نم", vec!["2"]),
        ("é", vec!["3"]),
        ("e\u{301}", vec!["4"]),
        ("STRASSE", vec![]),
    ] {
        query(&mut ui, c.clone(), needle);
        assert_eq!(visible_keys(&ui, c.container), expected, "{needle}");
    }
}

#[test]
fn readonly_lookup_blocks_user_changes_but_accepts_valid_application_source_updates() {
    let (mut ui, c) = setup(data());
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    ui.set_composite_read_only(c.container, true).unwrap();
    ui.open_editor_popup(c.container).unwrap();
    assert!(ui.rich.popup.is_none());
    ui.perform_action(c.editor, SemanticAction::Focus, None)
        .unwrap();
    ui.text_input("other").unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("Анна"));
    assert_eq!(ui.document(c.editor).unwrap().text(), "Анна");
    let mut next = data();
    next[0].text = "Анна новая".into();
    ui.set_selection_items(c.container, next).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("Анна новая"));
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
}

#[test]
fn changing_search_columns_preflights_newly_visible_rows_before_publishing_settings() {
    let mut items = data();
    items[0].text = "good".into();
    items[0].columns[0] = "hidden".into();
    items[1].text = format!("A{}", "\u{301}".repeat(40));
    items[1].columns[0] = "good".into();
    let (mut ui, c) = setup(items);
    let settings = LookupSearchSettings {
        columns: vec![0],
        ..Default::default()
    };
    ui.set_lookup_search_settings(c.container, settings.clone())
        .unwrap();
    query(&mut ui, c.clone(), "good");
    let list = popup(&ui, c.container);
    let rows = ui.rich.lists[&list].rows.clone();
    assert!(
        ui.set_lookup_search_settings(
            c.container,
            LookupSearchSettings {
                columns: vec![1],
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.lookup_search_settings(c.container), Some(settings));
    assert_eq!(popup(&ui, c.container), list);
    assert_eq!(ui.rich.lists[&list].rows, rows);
    assert_eq!(visible_keys(&ui, c.container), ["a"]);
}

#[test]
fn multi_source_changes_prune_disabled_keys_without_losing_query_or_cursor() {
    let mut records = data();
    for (item, name) in records.iter_mut().zip(["Alpha", "Beta", "Gamma"]) {
        item.text = name.into();
    }
    let (mut ui, c) = setup(records.clone());
    ui.set_combo_selection_settings(
        c.container,
        ComboSelectionSettings {
            mode: SelectionMode::Multiple,
            presentation: ComboSelectionPresentation::Tokens,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    query(&mut ui, c.clone(), "a");
    key(&mut ui, Key::ArrowDown);
    let list = popup(&ui, c.container);
    assert_eq!(
        ui.selection_items(list).unwrap()[ui.rich.lists[&list].cursor.unwrap()].key,
        "b"
    );
    ui.toggle_combo_draft(c.container, "b").unwrap();
    assert_eq!(
        ui.selection_items(list).unwrap()[ui.rich.lists[&list].cursor.unwrap()].key,
        "b"
    );
    key(&mut ui, Key::ArrowDown);
    records.reverse();
    records[1].enabled = false;
    ui.set_selection_items(c.container, records).unwrap();
    assert_eq!(popup(&ui, c.container), list);
    assert_eq!(ui.document(c.editor).unwrap().text(), "a");
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
    assert_eq!(ui.combo_selection_draft(c.container).unwrap(), ["a"]);
    assert_eq!(
        ui.selection_items(list).unwrap()[ui.rich.lists[&list].cursor.unwrap()].key,
        "c"
    );
}

#[path = "lookup_binding_tests.rs"]
mod binding_tests;
