use super::*;
use rust_desktop_ui_core::{Modifiers, VirtualOverscan};

fn items(count: usize) -> Vec<SelectionItem> {
    (0..count)
        .map(|i| SelectionItem::new(i.to_string(), format!("Row {i}")))
        .collect()
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}
fn setup(count: usize) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let list = ui
        .list_box(
            ui.root(),
            "List",
            items(count),
            SelectionSettings::default(),
            LayoutStyle {
                width: Length::Px(300.0),
                height: Length::Px(160.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap()
        .container;
    frame(&mut ui);
    (ui, list)
}
fn scroll(ui: &mut Controls, list: WidgetId, y: f32) {
    ui.scroll_to(list, y).unwrap();
    frame(ui);
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
fn hundred_thousand_rows_bound_live_widgets_and_reuse_only_identity_slots() {
    let (mut ui, list) = setup(100_000);
    let original = ui.list_row_widget(list, "0").unwrap();
    let lease = ui.list_row_lease(list, "0").unwrap();
    let retained = ui.list_row_widget(list, "3").unwrap();
    scroll(&mut ui, list, 36.0);
    assert_eq!(ui.list_row_widget(list, "3"), Some(retained));
    for index in [100, 2000, 90_000, 40_000, 99_990, 0] {
        scroll(&mut ui, list, index as f32 * 32.0 + 4.0);
        let stats = ui.list_virtualization_stats(list).unwrap();
        assert!(stats.realized_rows <= 10, "{stats:?}");
        // Before the first arrange, the provisional viewport is 320 px.
        assert!(stats.allocated_slots <= 12, "{stats:?}");
        assert!(ui.controls.len() < 16);
        assert_eq!(stats.realized_rows, stats.range.materialized.len());
        assert_eq!(
            stats.created_rows - stats.released_rows,
            stats.realized_rows as u64
        );
    }
    assert_eq!(ui.list_resolve_lease(list, lease), None);
    assert!(!ui.controls.contains_key(&original));
    assert_ne!(ui.list_row_widget(list, "0"), Some(original));
    assert!(ui.list_virtualization_stats(list).unwrap().created_rows > 40);
}

#[test]
fn measured_heights_position_actual_rows_and_drive_keyboard_paging() {
    let (mut ui, list) = setup(200);
    ui.set_list_item_height(list, "0", Some(80.0)).unwrap();
    ui.set_list_item_height(list, "1", Some(48.0)).unwrap();
    frame(&mut ui);
    let first = ui
        .runtime
        .bounds(ui.list_row_widget(list, "0").unwrap())
        .unwrap();
    let second = ui
        .runtime
        .bounds(ui.list_row_widget(list, "1").unwrap())
        .unwrap();
    let third = ui
        .runtime
        .bounds(ui.list_row_widget(list, "2").unwrap())
        .unwrap();
    assert_eq!(first.height, 80.0);
    assert_eq!(second.height, 48.0);
    assert_eq!(second.y - first.y, 80.0);
    assert_eq!(third.y - second.y, 48.0);
    ui.perform_action(list, SemanticAction::Focus, None)
        .unwrap();
    key(&mut ui, Key::Home);
    key(&mut ui, Key::PageDown);
    assert_eq!(ui.selected_keys(list).unwrap(), ["3"]);
    key(&mut ui, Key::PageUp);
    assert_eq!(ui.selected_keys(list).unwrap(), ["0"]);
    key(&mut ui, Key::End);
    assert_eq!(ui.selected_keys(list).unwrap(), ["199"]);
    let last = ui
        .runtime
        .bounds(ui.list_row_widget(list, "199").unwrap())
        .unwrap();
    let viewport = ui.runtime.bounds(list).unwrap();
    assert!(last.y >= viewport.y && last.y + last.height <= viewport.y + viewport.height);
}

#[test]
fn key_anchor_survives_insert_reorder_measurement_and_removal() {
    let (mut ui, list) = setup(100);
    scroll(&mut ui, list, 4.0 + 50.0 * 32.0 + 7.5);
    let original = ui.list_row_widget(list, "50").unwrap();
    let mut next = items(100);
    next.insert(0, SelectionItem::new("inserted", "New"));
    ui.set_selection_items(list, next.clone()).unwrap();
    assert_eq!(ui.scroll_offset(list).unwrap().y, 4.0 + 51.0 * 32.0 + 7.5);
    assert_eq!(ui.list_row_widget(list, "50"), Some(original));
    ui.set_list_item_height(list, "2", Some(80.0)).unwrap();
    assert_eq!(
        ui.scroll_offset(list).unwrap().y,
        4.0 + 51.0 * 32.0 + 48.0 + 7.5
    );
    next.swap(51, 21);
    ui.set_selection_items(list, next.clone()).unwrap();
    assert_eq!(
        ui.scroll_offset(list).unwrap().y,
        4.0 + 21.0 * 32.0 + 48.0 + 7.5
    );
    assert_eq!(ui.list_row_widget(list, "50"), Some(original));
    next.remove(21);
    ui.set_selection_items(list, next).unwrap();
    assert_eq!(
        ui.list_virtualization_stats(list)
            .unwrap()
            .range
            .visible
            .start,
        21
    );
    assert_eq!(
        ui.scroll_offset(list).unwrap().y,
        4.0 + 21.0 * 32.0 + 48.0 + 7.5
    );
    assert_eq!(ui.list_row_widget(list, "50"), None);
}

#[test]
fn shrinking_anchor_stays_in_same_key_even_after_f32_conversion() {
    let (mut ui, list) = setup(100_000);
    scroll(&mut ui, list, 4.0 + 90_000.0 * 32.0 + 27.5);
    ui.set_list_item_height(list, "90000", Some(1.0)).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.list_virtualization_stats(list)
            .unwrap()
            .range
            .visible
            .start,
        90_000
    );
    assert_eq!(
        ui.scroll_offset(list).unwrap().y,
        4.0 + 90_000.0 * 32.0 + 0.75
    );
    let row = ui
        .runtime
        .bounds(ui.list_row_widget(list, "90000").unwrap())
        .unwrap();
    assert_eq!(row.height, 1.0);
    let next = ui
        .runtime
        .bounds(ui.list_row_widget(list, "90001").unwrap())
        .unwrap();
    assert_eq!(next.y - row.y, 1.0);
}

#[test]
fn retained_key_keeps_editor_draft_undo_selection_and_composition() {
    let (mut ui, list) = setup(100);
    let row = ui.list_row_widget(list, "3").unwrap();
    let editor = ui
        .edit_box(row, "Embedded", "accepted", LayoutStyle::default())
        .unwrap();
    ui.perform_action(editor, SemanticAction::Focus, None)
        .unwrap();
    ui.text_input("draft").unwrap();
    let Kind::TextBox(document) = &mut ui.controls.get_mut(&editor).unwrap().kind else {
        unreachable!()
    };
    document.set_selection(1, 4).unwrap();
    let text = document.text().to_owned();
    let selection = document.selection();
    assert!(document.can_undo());
    ui.ime_preedit("候", Some((0, 3))).unwrap();
    scroll(&mut ui, list, 36.0);
    let mut data = items(100);
    data[3].text = "Changed caption".into();
    data.swap(3, 4);
    ui.set_selection_items(list, data).unwrap();
    assert_eq!(ui.list_row_widget(list, "3"), Some(row));
    let document = ui.document(editor).unwrap();
    assert_eq!(document.text(), text);
    assert_eq!(document.selection(), selection);
    assert!(document.can_undo());
    assert!(document.composition().is_some());
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
}

#[test]
fn evicted_key_destroys_subtree_and_stale_input_cannot_reach_new_row() {
    let (mut ui, list) = setup(1000);
    let row = ui.list_row_widget(list, "2").unwrap();
    let lease = ui.list_row_lease(list, "2").unwrap();
    let editor = ui
        .edit_box(row, "Private draft", "secret", LayoutStyle::default())
        .unwrap();
    ui.perform_action(editor, SemanticAction::Focus, None)
        .unwrap();
    ui.text_input(" pending").unwrap();
    ui.ime_preedit("候", Some((0, 3))).unwrap();
    scroll(&mut ui, list, 20_000.0);
    assert!(ui.document(editor).is_none());
    assert_eq!(ui.list_resolve_lease(list, lease), None);
    assert_eq!(ui.runtime.focused(), Some(list));
    ui.perform_action(editor, SemanticAction::Focus, None)
        .unwrap();
    assert_eq!(ui.runtime.focused(), Some(list));
    assert!(
        ui.semantics()
            .iter()
            .all(|node| !node.label.contains("Private draft"))
    );
    scroll(&mut ui, list, 0.0);
    let fresh = ui.list_row_widget(list, "2").unwrap();
    assert_ne!(fresh, row);
    assert!(ui.runtime.tree().children(fresh).unwrap().is_empty());
    assert!(ui.document(fresh).is_none());
}

#[test]
fn geometry_and_budget_errors_are_atomic() {
    let (mut ui, list) = setup(100_000);
    scroll(&mut ui, list, 3200.0);
    ui.set_selected_keys(list, &["100".into()]).unwrap();
    let settings = ui.list_virtualization_settings(list).unwrap();
    let stats = ui.list_virtualization_stats(list).unwrap();
    let row = ui.list_row_widget(list, "100").unwrap();
    let scroll = ui.scroll_offset(list);
    for height in [f32::NAN, 0.0, 1.1, 4096.25] {
        assert!(ui.set_list_item_height(list, "100", Some(height)).is_err());
    }
    for candidate in [
        ListVirtualizationSettings {
            default_row_height: 64.0,
            ..settings
        },
        ListVirtualizationSettings {
            max_realized_rows: 2,
            ..settings
        },
        ListVirtualizationSettings {
            overscan: VirtualOverscan {
                before: 1025,
                after: 0,
            },
            ..settings
        },
    ] {
        assert!(ui.set_list_virtualization(list, candidate).is_err());
    }
    assert!(
        ui.set_list_item_height(list, "missing", Some(32.0))
            .is_err()
    );
    assert_eq!(ui.list_virtualization_settings(list), Some(settings));
    assert_eq!(ui.list_virtualization_stats(list), Some(stats));
    assert_eq!(ui.list_row_widget(list, "100"), Some(row));
    assert_eq!(ui.scroll_offset(list), scroll);
    assert_eq!(ui.selected_keys(list).unwrap(), ["100"]);
}

#[test]
fn invalid_new_visible_text_and_excess_columns_leave_source_and_visuals_unchanged() {
    let (mut ui, list) = setup(30);
    let rows = ui.rich.lists[&list].rows.clone();
    let original = ui.selection_items(list).unwrap().to_vec();
    let mut invalid = original.clone();
    invalid[0].text = format!("A{}", "\u{301}".repeat(40));
    assert!(ui.set_selection_items(list, invalid).is_err());
    let mut invalid = original.clone();
    invalid[0].columns = vec![String::new(); 32];
    assert!(ui.set_selection_items(list, invalid).is_err());
    assert_eq!(ui.selection_items(list), Some(original.as_slice()));
    assert_eq!(ui.rich.lists[&list].rows, rows);
}

#[test]
fn removing_source_items_discards_measurements_and_empty_source_has_no_visuals() {
    let (mut ui, list) = setup(100);
    ui.set_list_item_height(list, "50", Some(48.0)).unwrap();
    assert_eq!(ui.list_virtualization_stats(list).unwrap().measured_rows, 1);
    ui.set_selection_items(list, items(20)).unwrap();
    assert_eq!(ui.list_virtualization_stats(list).unwrap().measured_rows, 0);
    ui.set_selection_items(list, Vec::new()).unwrap();
    frame(&mut ui);
    let stats = ui.list_virtualization_stats(list).unwrap();
    assert!(stats.range.visible.is_empty());
    assert_eq!(stats.realized_rows, 0);
    assert_eq!(ui.scroll_offset(list).unwrap().y, 0.0);
    ui.remove(list).unwrap();
    assert!(ui.list_virtualization_stats(list).is_none());
}

#[test]
fn explicit_measurement_equal_to_old_default_survives_estimate_change() {
    let (mut ui, list) = setup(100);
    ui.set_list_item_height(list, "20", Some(32.0)).unwrap();
    let settings = ui.list_virtualization_settings(list).unwrap();
    ui.set_list_virtualization(
        list,
        ListVirtualizationSettings {
            default_row_height: 48.0,
            ..settings
        },
    )
    .unwrap();
    assert_eq!(ui.list_item_height(list, "20"), Some(32.0));
    assert_eq!(ui.list_item_height(list, "21"), Some(48.0));
    assert_eq!(ui.list_virtualization_stats(list).unwrap().measured_rows, 1);
    ui.set_list_item_height(list, "20", None).unwrap();
    assert_eq!(ui.list_item_height(list, "20"), Some(48.0));
    assert_eq!(ui.list_virtualization_stats(list).unwrap().measured_rows, 0);
}

#[test]
fn popup_candidate_preflight_never_mutates_live_source_or_visuals() {
    let (mut ui, list) = setup(100);
    scroll(&mut ui, list, 320.0);
    let stats = ui.list_virtualization_stats(list).unwrap();
    let rows = ui.rich.lists[&list].rows.clone();
    let accepted = ui.selection_items(list).unwrap().to_vec();
    let offset = ui.scroll_offset(list);
    let mut next = accepted.clone();
    next[10].text = "Renamed".into();
    ui.preflight_virtual_list_items(list, &next).unwrap();
    next[10].text = format!("A{}", "\u{301}".repeat(40));
    assert!(ui.preflight_virtual_list_items(list, &next).is_err());
    assert_eq!(ui.selection_items(list), Some(accepted.as_slice()));
    assert_eq!(ui.rich.lists[&list].rows, rows);
    assert_eq!(ui.list_virtualization_stats(list), Some(stats));
    assert_eq!(ui.scroll_offset(list), offset);
}
