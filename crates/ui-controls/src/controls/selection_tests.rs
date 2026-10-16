use super::*;
use rust_desktop_ui_core::Modifiers;
use std::{cell::Cell, rc::Rc};

fn style(height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(300.0),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}
fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    frame(ui);
}
fn key(ui: &mut Controls, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    })
    .unwrap();
}
fn items() -> Vec<SelectionItem> {
    vec![
        SelectionItem::new("a", "Alpha"),
        SelectionItem::new("b", "Beta"),
        SelectionItem::new("c", "Gamma"),
    ]
}
fn list(mode: SelectionMode) -> (Controls, ListBox) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let list = ui
        .list_box(
            ui.root(),
            "List",
            items(),
            SelectionSettings {
                mode,
                read_only: false,
            },
            style(100.0),
        )
        .unwrap();
    frame(&mut ui);
    (ui, list)
}
fn combo(editable: bool) -> (Controls, EditorControl) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .combo_box(
            ui.root(),
            "Choice",
            items(),
            ComboBoxSettings {
                editable,
                read_only: false,
            },
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    (ui, c)
}
fn spin() -> (Controls, EditorControl) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .spin_box(
            ui.root(),
            "Amount",
            2.0,
            SpinBoxSettings {
                minimum: 0.0,
                maximum: 10.0,
                step: 0.5,
            },
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    (ui, c)
}

#[test]
fn stable_keys_survive_reorder_and_captions_without_spurious_events() {
    let (mut ui, list) = list(SelectionMode::Single);
    ui.set_selected_keys(list.container, &["b".into()]).unwrap();
    ui.drain_selection_changes();
    let replacement = vec![
        SelectionItem::new("c", "New gamma"),
        SelectionItem::new("b", "Renamed beta"),
        SelectionItem::new("a", "Alpha"),
    ];
    ui.set_selection_items(list.container, replacement).unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["b"]);
    assert!(ui.drain_selection_changes().is_empty());
    ui.set_selection_items(list.container, vec![SelectionItem::new("a", "Alpha")])
        .unwrap();
    assert!(ui.selected_keys(list.container).unwrap().is_empty());
    assert_eq!(
        ui.drain_selection_changes(),
        vec![SelectionChanged {
            id: list.container,
            keys: vec![]
        }]
    );
}
#[test]
fn selection_validation_is_atomic_and_redundant_writes_do_not_emit() {
    let (mut ui, list) = list(SelectionMode::Single);
    ui.set_selected_keys(list.container, &["b".into()]).unwrap();
    ui.drain_selection_changes();
    assert!(!ui.set_selected_keys(list.container, &["b".into()]).unwrap());
    assert!(
        ui.set_selected_keys(list.container, &["missing".into()])
            .is_err()
    );
    assert!(
        ui.set_selected_keys(list.container, &["a".into(), "b".into()])
            .is_err()
    );
    assert!(
        ui.set_selection_items(
            list.container,
            vec![
                SelectionItem::new("x", "One"),
                SelectionItem::new("x", "Two")
            ]
        )
        .is_err()
    );
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["b"]);
    assert_eq!(ui.selection_items(list.container).unwrap().len(), 3);
    assert!(ui.drain_selection_changes().is_empty());
}
#[test]
fn multiselect_control_shift_and_disabled_rows_follow_stable_order() {
    let (mut ui, list) = list(SelectionMode::Multiple);
    let mut data = items();
    data[1].enabled = false;
    ui.set_selection_items(list.container, data).unwrap();
    focus(&mut ui, list.container);
    key(&mut ui, Key::Home, Modifiers::default());
    key(
        &mut ui,
        Key::End,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["a", "c"]);
    key(
        &mut ui,
        Key::Space,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["a"]);
    assert!(
        ui.set_selection_settings(
            list.container,
            SelectionSettings {
                mode: SelectionMode::Single,
                read_only: false
            }
        )
        .is_ok()
    );
}
#[test]
fn multiselect_to_single_rejects_without_truncating_selection() {
    let (mut ui, list) = list(SelectionMode::Multiple);
    ui.set_selected_keys(list.container, &["c".into(), "a".into()])
        .unwrap();
    assert!(
        ui.set_selection_settings(list.container, SelectionSettings::default())
            .is_err()
    );
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["a", "c"]);
    assert_eq!(
        ui.selection_settings(list.container).unwrap().mode,
        SelectionMode::Multiple
    );
}
#[test]
fn list_navigation_scrolls_selected_row_into_view_and_typeahead_matches() {
    let (mut ui, list) = list(SelectionMode::Single);
    ui.set_selection_items(
        list.container,
        (0..40)
            .map(|i| SelectionItem::new(i.to_string(), format!("Item {i:02}")))
            .collect(),
    )
    .unwrap();
    frame(&mut ui);
    focus(&mut ui, list.container);
    key(&mut ui, Key::End, Modifiers::default());
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["39"]);
    assert!(
        ui.runtime
            .tree()
            .node(list.container)
            .unwrap()
            .props
            .scroll
            .y
            > 1000.0
    );
    key(&mut ui, Key::Home, Modifiers::default());
    assert_eq!(
        ui.runtime
            .tree()
            .node(list.container)
            .unwrap()
            .props
            .scroll
            .y,
        4.0
    );
    ui.set_selection_items(list.container, items()).unwrap();
    key(&mut ui, Key::Character("g".into()), Modifiers::default());
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["c"]);
}
#[test]
fn list_read_only_and_disabled_ancestor_block_user_selection_but_allow_application() {
    let (mut ui, list) = list(SelectionMode::Single);
    ui.set_selection_settings(
        list.container,
        SelectionSettings {
            read_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    focus(&mut ui, list.container);
    key(&mut ui, Key::End, Modifiers::default());
    assert!(ui.selected_keys(list.container).unwrap().is_empty());
    ui.set_selected_keys(list.container, &["b".into()]).unwrap();
    ui.set_selection_settings(list.container, SelectionSettings::default())
        .unwrap();
    ui.set_enabled(list.container, false).unwrap();
    let row = ui.rich.lists[&list.container].rows[2];
    ui.activate(row).unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["b"]);
}
#[test]
fn selection_source_validator_rejects_before_keys_or_display_change() {
    let (mut ui, c) = combo(false);
    ui.chain_selection_validator(
        c.container,
        SelectionValidator::new(|keys| {
            if keys == ["b"] {
                Err("Unavailable".into())
            } else {
                Ok(())
            }
        }),
    )
    .unwrap();
    assert!(ui.set_selected_keys(c.container, &["b".into()]).is_err());
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
    assert_eq!(ui.accepted_value(c.editor), Some(""));
    assert!(ui.drain_selection_changes().is_empty());
    ui.open_editor_popup(c.container).unwrap();
    let popup_list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let row = ui.rich.lists[&popup_list].rows[1];
    ui.activate(row).unwrap();
    assert!(ui.popup_open());
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
}
#[test]
fn combo_duplicate_captions_keep_explicit_key_and_emit_once() {
    let (mut ui, c) = combo(false);
    ui.set_selection_items(
        c.container,
        vec![
            SelectionItem::new("one", "Same"),
            SelectionItem::new("two", "Same"),
        ],
    )
    .unwrap();
    ui.drain_selection_changes();
    ui.set_selected_keys(c.container, &["two".into()]).unwrap();
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["two"]);
    assert_eq!(ui.accepted_value(c.editor), Some("Same"));
    assert_eq!(
        ui.drain_selection_changes(),
        vec![SelectionChanged {
            id: c.container,
            keys: vec!["two".into()]
        }]
    );
    ui.set_selected_keys(c.container, &["one".into()]).unwrap();
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["one"]);
    assert_eq!(ui.drain_selection_changes().len(), 1);
}
#[test]
fn combo_keyboard_popup_preview_does_not_accept_until_enter_and_escape_cancels() {
    let (mut ui, c) = combo(false);
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    ui.drain_selection_changes();
    focus(&mut ui, c.editor);
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
    key(&mut ui, Key::Escape, Modifiers::default());
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(c.editor));
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    key(&mut ui, Key::End, Modifiers::default());
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["c"]);
    assert_eq!(ui.drain_selection_changes().len(), 1);
    assert!(!ui.popup_open());
}
#[test]
fn editable_combo_accepts_custom_text_and_clears_identity() {
    let (mut ui, c) = combo(true);
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    ui.drain_selection_changes();
    ui.set_text(c.editor, "Custom").unwrap();
    assert_eq!(ui.commit_edit(c.editor).unwrap(), CommitOutcome::Committed);
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
    assert_eq!(ui.drain_selection_changes().len(), 1);
}
#[test]
fn lookup_filters_all_columns_and_rejects_unlisted_accepted_values() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut a = SelectionItem::new("a", "Alice");
    a.columns = vec!["Engineering".into()];
    let mut b = SelectionItem::new("b", "Bob");
    b.columns = vec!["Finance".into()];
    let c = ui
        .lookup_box(
            ui.root(),
            "Person",
            vec![a, b],
            vec!["Name".into(), "Team".into()],
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    ui.set_text(c.editor, "fin").unwrap();
    ui.open_editor_popup(c.container).unwrap();
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    assert_eq!(ui.selection_items(list).unwrap()[0].key, "b");
    assert_eq!(ui.selection_items(list).unwrap().len(), 1);
    assert!(matches!(
        ui.commit_edit(c.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.accepted_value(c.editor), Some(""));
    let row = ui.rich.lists[&list].rows[0];
    ui.activate(row).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("Bob"));
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["b"]);
}
#[test]
fn popup_is_clamped_and_closes_on_blur_disable_and_owner_removal() {
    let (mut ui, c) = combo(false);
    ui.open_editor_popup(c.container).unwrap();
    ui.update(Size::new(180.0, 90.0)).unwrap();
    let bounds = ui
        .runtime
        .bounds(ui.rich.popup.as_ref().unwrap().surface)
        .unwrap();
    assert!(
        bounds.x >= 0.0
            && bounds.y >= 0.0
            && bounds.x + bounds.width <= 180.01
            && bounds.y + bounds.height <= 90.01
    );
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(!ui.popup_open());
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    ui.open_editor_popup(c.container).unwrap();
    ui.set_enabled(c.container, false).unwrap();
    frame(&mut ui);
    assert!(!ui.popup_open());
    ui.set_enabled(c.container, true).unwrap();
    ui.open_editor_popup(c.container).unwrap();
    ui.remove(c.container).unwrap();
    frame(&mut ui);
    assert!(!ui.popup_open());
}
#[test]
fn combo_item_refresh_updates_caption_and_rejects_atomically_on_validation() {
    let (mut ui, c) = combo(false);
    ui.set_selected_keys(c.container, &["b".into()]).unwrap();
    ui.set_editor_validator(
        c.editor,
        Some(EditorValidator::new(|text| {
            if text == "Blocked" {
                Err("No".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.drain_edit_events();
    ui.drain_selection_changes();
    let mut data = items();
    data[1].text = "Blocked".into();
    assert!(ui.set_selection_items(c.container, data).is_err());
    assert_eq!(ui.selection_items(c.container).unwrap()[1].text, "Beta");
    assert_eq!(ui.accepted_value(c.editor), Some("Beta"));
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.drain_selection_changes().is_empty());
}
#[test]
fn numeric_draft_accept_cancel_and_loss_of_focus_keep_valid_baseline() {
    let (mut ui, c) = spin();
    let next = ui.button(ui.root(), "Next", None, style(34.0)).unwrap();
    frame(&mut ui);
    focus(&mut ui, c.editor);
    ui.set_text(c.editor, "NaN").unwrap();
    focus(&mut ui, next);
    assert_eq!(ui.spin_value(c.container), Some(2.0));
    assert!(ui.validation_error(c.editor).is_some());
    focus(&mut ui, c.editor);
    key(&mut ui, Key::Escape, Modifiers::default());
    assert_eq!(ui.document(c.editor).unwrap().text(), "2");
    ui.set_text(c.editor, "7.5").unwrap();
    focus(&mut ui, next);
    assert_eq!(ui.spin_value(c.container), Some(7.5));
    assert_eq!(
        ui.drain_value_changes(),
        vec![ValueChanged {
            id: c.container,
            value: EditorValue::Number(7.5)
        }]
    );
}
#[test]
fn numeric_settings_reject_nonfinite_inverted_and_invalid_existing_value_atomically() {
    let (mut ui, c) = spin();
    let before = ui.spin_settings(c.container).unwrap().clone();
    for s in [
        SpinBoxSettings {
            step: f64::NAN,
            ..before.clone()
        },
        SpinBoxSettings {
            minimum: 5.0,
            ..before.clone()
        },
        SpinBoxSettings {
            maximum: -1.0,
            ..before.clone()
        },
        SpinBoxSettings {
            step: 0.0,
            ..before.clone()
        },
    ] {
        assert!(ui.set_spin_settings(c.container, s).is_err());
        assert_eq!(ui.spin_settings(c.container), Some(&before));
    }
    assert!(matches!(
        ui.set_spin_value(c.container, f64::INFINITY).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.spin_value(c.container), Some(2.0));
}
#[test]
fn numeric_steps_are_clamped_and_value_events_ignore_equal_numeric_text() {
    let (mut ui, c) = spin();
    focus(&mut ui, c.editor);
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(ui.spin_value(c.container), Some(2.5));
    ui.set_spin_value(c.container, 10.0).unwrap();
    ui.drain_value_changes();
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert!(ui.drain_value_changes().is_empty());
    ui.set_text(c.editor, "10.000").unwrap();
    ui.commit_edit(c.editor).unwrap();
    assert!(ui.drain_value_changes().is_empty());
}
#[test]
fn composite_read_only_blocks_buttons_and_keyboard_but_allows_application_value() {
    let (mut ui, c) = spin();
    ui.set_composite_read_only(c.container, true).unwrap();
    focus(&mut ui, c.editor);
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    ui.activate(c.buttons[1]).unwrap();
    assert_eq!(ui.spin_value(c.container), Some(2.0));
    ui.set_spin_value(c.container, 4.0).unwrap();
    assert_eq!(ui.spin_value(c.container), Some(4.0));
}
#[test]
fn chained_application_validators_preserve_typed_numeric_validation() {
    let (mut ui, c) = spin();
    ui.set_editor_validator(
        c.editor,
        Some(EditorValidator::new(|s| {
            if s == "3" {
                Err("three".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.chain_editor_validator(
        c.editor,
        EditorValidator::new(|s| if s == "4" { Err("four".into()) } else { Ok(()) }),
    )
    .unwrap();
    for value in ["NaN", "3", "4"] {
        ui.set_text(c.editor, value).unwrap();
        assert!(matches!(
            ui.commit_edit(c.editor).unwrap(),
            CommitOutcome::Invalid(_)
        ));
    }
    assert_eq!(ui.spin_value(c.container), Some(2.0));
}
#[test]
fn gregorian_dates_validate_century_leap_years_and_strict_formats() {
    assert!(DateValue::parse_iso("1900-02-29").is_err());
    assert!(DateValue::parse_iso("2000-02-29").is_ok());
    assert!(DateValue::parse_iso("2100-02-29").is_err());
    assert!(DateValue::parse_iso("2026-04-31").is_err());
    assert!(DateValue::parse_iso("2026-4-01").is_err());
    assert!(DateValue::parse_iso("0000-01-01").is_err());
    let date = DateValue::new(2024, 2, 29).unwrap();
    assert_eq!(date.to_display(DateFormat::Russian), "29.02.2024");
    assert_eq!(
        DateValue::parse_display("29.02.2024", DateFormat::Russian).unwrap(),
        date
    );
}
#[test]
fn date_nullable_display_switch_and_invalid_settings_are_atomic() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            None,
            DateEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    assert_eq!(ui.date_value(c.container), Some(None));
    let date = DateValue::new(2024, 2, 29).unwrap();
    ui.set_date_value(c.container, Some(date)).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("29.02.2024"));
    ui.drain_value_changes();
    ui.set_date_settings(
        c.container,
        DateEditSettings {
            format: DateFormat::Iso,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("2024-02-29"));
    assert!(ui.drain_value_changes().is_empty());
    assert!(
        ui.set_date_settings(
            c.container,
            DateEditSettings {
                minimum: Some(DateValue::new(2025, 1, 1).unwrap()),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.date_value(c.container), Some(Some(date)));
}
#[test]
fn date_calendar_keyboard_handles_leap_day_and_escape_preserves_value() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let date = DateValue::new(2024, 2, 28).unwrap();
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            Some(date),
            DateEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.date_value(c.container), Some(Some(date)));
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(
        ui.date_value(c.container),
        Some(Some(DateValue::new(2024, 2, 29).unwrap()))
    );
    ui.open_editor_popup(c.container).unwrap();
    key(&mut ui, Key::PageDown, Modifiers::default());
    key(&mut ui, Key::Escape, Modifiers::default());
    assert_eq!(
        ui.date_value(c.container),
        Some(Some(DateValue::new(2024, 2, 29).unwrap()))
    );
    assert!(!ui.popup_open());
}
#[test]
fn date_calendar_disables_out_of_range_cells() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let min = DateValue::new(2026, 10, 10).unwrap();
    let max = DateValue::new(2026, 10, 20).unwrap();
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            Some(min),
            DateEditSettings {
                minimum: Some(min),
                maximum: Some(max),
                ..Default::default()
            },
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let disabled = ui
        .rich
        .actions
        .iter()
        .find_map(|(&id, a)| {
            matches!(a,selection::RichAction::Date{value,..}if value.day==9).then_some(id)
        })
        .unwrap();
    assert!(!ui.is_enabled(disabled));
    ui.activate(disabled).unwrap();
    assert_eq!(ui.date_value(c.container), Some(Some(min)));
}
#[test]
fn time_parser_nullable_seconds_precision_and_steps_are_validated() {
    for text in ["24:00", "12:60", "12:30:60", "1:30", "12:3", "-1:00"] {
        assert!(TimeValue::parse(text).is_err());
    }
    assert_eq!(TimeValue::parse("09:30").unwrap().to_iso(), "09:30:00");
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .time_edit(
            ui.root(),
            "Time",
            None,
            TimeEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    assert_eq!(ui.time_value(c.container), Some(None));
    assert_eq!(
        ui.set_time_value(c.container, Some(TimeValue::new(9, 30, 1).unwrap()))
            .unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(
        ui.time_value(c.container),
        Some(Some(TimeValue::new(9, 30, 1).unwrap()))
    );
    ui.set_time_value(c.container, Some(TimeValue::new(23, 59, 0).unwrap()))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, c.editor);
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(
        ui.time_value(c.container),
        Some(Some(TimeValue::new(23, 59, 0).unwrap()))
    );
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    assert_eq!(
        ui.time_value(c.container),
        Some(Some(TimeValue::new(23, 58, 0).unwrap()))
    );
}
#[test]
fn time_settings_accept_hidden_seconds_and_preserve_dirty_draft() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .time_edit(
            ui.root(),
            "Time",
            Some(TimeValue::new(9, 0, 0).unwrap()),
            TimeEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    assert!(
        ui.set_time_settings(
            c.container,
            TimeEditSettings {
                step_seconds: 1,
                ..Default::default()
            }
        )
        .is_ok()
    );
    ui.set_text(c.editor, "invalid").unwrap();
    ui.set_time_settings(
        c.container,
        TimeEditSettings {
            step_seconds: 120,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "invalid");
    assert!(
        ui.set_time_settings(
            c.container,
            TimeEditSettings {
                show_seconds: true,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn embedded_command_and_clear_use_existing_commands_and_editor_session() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("pick");
    ui.commands_mut()
        .register_toggle(command.clone(), "Pick", false);
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    ui.commands_mut()
        .on_execute(&command, move || counter.set(counter.get() + 1));
    let c = ui
        .button_edit(
            ui.root(),
            "File",
            "file.txt",
            vec![
                EditorButton {
                    label: "Pick".into(),
                    icon: Some(IconSymbol::FolderOpen),
                    action: EditorButtonAction::Command(command.clone()),
                },
                EditorButton {
                    label: "Clear".into(),
                    icon: Some(IconSymbol::Close),
                    action: EditorButtonAction::Clear,
                },
            ],
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    ui.activate(c.buttons[0]).unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(ui.commands().checked(&command), Some(true));
    ui.activate(c.buttons[1]).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some(""));
    assert_eq!(ui.drain_edit_events().len(), 1);
    ui.set_composite_read_only(c.container, true).unwrap();
    ui.activate(c.buttons[0]).unwrap();
    assert_eq!(calls.get(), 1);
}
#[test]
fn open_selection_popup_does_not_cause_perpetual_layout_work() {
    let (mut ui, c) = combo(false);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    frame(&mut ui);
    let stats = ui.update(Size::new(640.0, 480.0)).unwrap();
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(stats.paint_passes, 0);
}

#[test]
fn a_hundred_thousand_rows_keep_a_bounded_retained_tree_and_reach_the_last_key() {
    let (mut ui, list) = list(SelectionMode::Single);
    ui.set_selection_items(
        list.container,
        (0..100_000)
            .map(|i| SelectionItem::new(i.to_string(), format!("Row {i}")))
            .collect(),
    )
    .unwrap();
    frame(&mut ui);
    assert!(ui.rich.lists[&list.container].rows.len() < 20);
    assert!(ui.controls.len() < 30);
    focus(&mut ui, list.container);
    key(&mut ui, Key::End, Modifiers::default());
    frame(&mut ui);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["99999"]);
    assert!(ui.rich.lists[&list.container].row_start > 99_980);
    assert!(ui.controls.len() < 30);
    let row = ui.rich.lists[&list.container].rows.last().copied().unwrap();
    assert!(ui.runtime.bounds(row).is_some());
    assert!(
        ui.semantics()
            .iter()
            .any(|n| n.role == Role::ListItem && n.selected && n.label.contains("99999"))
    );
}

#[test]
fn lookup_column_widths_are_applied_to_headers_and_rows_and_editability_can_change() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut item = SelectionItem::new("a", "Alice");
    item.columns = vec!["Engineering".into()];
    let c = ui
        .lookup_box(
            ui.root(),
            "Person",
            vec![item],
            vec!["Name".into(), "Team".into()],
            style(34.0),
        )
        .unwrap();
    let columns = vec![
        LookupColumn {
            header: "Name".into(),
            width: Some(120.0),
        },
        LookupColumn {
            header: "Team".into(),
            width: None,
        },
    ];
    ui.set_lookup_columns(c.container, columns.clone()).unwrap();
    assert!(
        ui.set_lookup_columns(
            c.container,
            vec![LookupColumn {
                header: "Invalid".into(),
                width: Some(f32::NAN)
            }]
        )
        .is_err()
    );
    assert_eq!(ui.lookup_columns(c.container), Some(columns.as_slice()));
    ui.set_combo_settings(
        c.container,
        ComboBoxSettings {
            editable: false,
            read_only: false,
        },
    )
    .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let row = ui.rich.lists[&list].rows[0];
    let first = ui.runtime.tree().children(row).unwrap()[0];
    assert!((ui.runtime.bounds(first).unwrap().width - 120.0).abs() < 0.01);
    assert_eq!(ui.is_read_only(c.editor), Some(true));
    ui.activate(row).unwrap();
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["a"]);
}

#[test]
fn successful_combo_item_rename_refreshes_display_without_selection_notification() {
    let (mut ui, c) = combo(false);
    ui.set_selected_keys(c.container, &["b".into()]).unwrap();
    ui.drain_selection_changes();
    let mut data = items();
    data[1].text = "New name".into();
    ui.set_selection_items(c.container, data).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("New name"));
    assert_eq!(ui.selected_keys(c.container).unwrap(), ["b"]);
    assert!(ui.drain_selection_changes().is_empty());
}

#[test]
fn ime_composition_is_not_accepted_or_replaced_by_spin_or_combo_keys() {
    let (mut ui, c) = combo(true);
    focus(&mut ui, c.editor);
    ui.ime_preedit("候補", None).unwrap();
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    assert!(!ui.popup_open());
    assert!(ui.document(c.editor).unwrap().composition().is_some());
    assert_eq!(ui.accepted_value(c.editor), Some(""));
    let (mut ui, c) = spin();
    focus(&mut ui, c.editor);
    ui.ime_preedit("3", None).unwrap();
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(ui.spin_value(c.container), Some(2.0));
    assert!(ui.document(c.editor).unwrap().composition().is_some());
}

#[test]
fn popup_internal_focus_does_not_commit_lookup_search_or_discard_its_editor() {
    let (mut ui, c) = combo(true);
    focus(&mut ui, c.editor);
    ui.set_text(c.editor, "Be").unwrap();
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let row = ui.rich.lists[&list].rows[0];
    focus(&mut ui, row);
    assert_eq!(ui.accepted_value(c.editor), Some(""));
    assert!(ui.popup_open());
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.accepted_value(c.editor), Some("Beta"));
    assert!(!ui.popup_open());
}

#[test]
fn replacing_a_selection_popup_with_a_menu_leaves_exactly_one_overlay() {
    let (mut ui, c) = combo(false);
    ui.commands_mut().register("save", "Save");
    let anchor = ui.button(ui.root(), "Menu", None, style(34.0)).unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    ui.popup_menu(anchor, &[MenuItem::new("Save", "save")])
        .unwrap();
    assert!(ui.rich.popup.is_none());
    assert_eq!(ui.popup_depth(), 1);
    assert!(ui.popup.is_some());
    ui.open_editor_popup(c.container).unwrap();
    assert!(ui.popup.is_none());
    assert!(ui.rich.popup.is_some());
}

#[test]
fn date_format_rejection_by_application_validator_preserves_all_settings_and_values() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let date = DateValue::new(2026, 10, 15).unwrap();
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            Some(date),
            DateEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    ui.set_editor_validator(
        c.editor,
        Some(EditorValidator::new(|s| {
            if s.contains('-') {
                Err("Expected localized format".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    assert!(
        ui.set_date_settings(
            c.container,
            DateEditSettings {
                format: DateFormat::Iso,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        ui.date_settings(c.container).unwrap().format,
        DateFormat::Russian
    );
    assert_eq!(ui.accepted_value(c.editor), Some("15.10.2026"));
    assert_eq!(ui.date_value(c.container), Some(Some(date)));
    assert!(ui.drain_value_changes().is_empty());
}

#[test]
fn readonly_list_semantics_preserve_selection_without_offering_activation() {
    let (mut ui, list) = list(SelectionMode::Single);
    ui.set_selected_keys(list.container, &["b".into()]).unwrap();
    ui.set_selection_settings(
        list.container,
        SelectionSettings {
            read_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let semantics = ui.semantics();
    let row = semantics
        .iter()
        .find(|n| n.role == Role::ListItem && n.selected)
        .unwrap();
    assert!(row.read_only);
    assert!(!row.actions.contains(&SemanticAction::Activate));
}

#[test]
fn fluent_list_rows_have_one_outer_frame_left_aligned_labels_and_visible_selection() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let (mut ui, list) = list(SelectionMode::Single);
        ui.set_theme(theme).unwrap();
        ui.set_selected_keys(list.container, &["b".into()]).unwrap();
        frame(&mut ui);
        let row = ui.rich.lists[&list.container].rows[1];
        let bounds = ui.runtime.bounds(row).unwrap();
        assert!(
            ui.scene()
                .rounded_rectangles
                .iter()
                .any(|r| r.bounds == bounds
                    && r.color == theme.tokens().selected
                    && r.stroke_width.is_none())
        );
        let run = ui.scene().texts.iter().find(|r| r.text == "Beta").unwrap();
        assert_eq!(run.align, TextAlign::Start);
        assert!(
            !ui.scene()
                .rounded_rectangles
                .iter()
                .any(|r| r.bounds == bounds && r.stroke_width.is_some())
        );
        let viewport = ui.runtime.bounds(list.container).unwrap();
        assert!(
            ui.scene()
                .rounded_rectangles
                .iter()
                .any(|r| r.bounds == viewport && r.stroke_width == Some(1.0))
        );
    }
}

#[test]
fn composite_editor_paints_one_outer_frame_and_shared_focus_and_validation_chrome() {
    let (mut ui, c) = spin();
    focus(&mut ui, c.editor);
    let outer = ui.runtime.bounds(c.container).unwrap();
    let editor = ui.runtime.bounds(c.editor).unwrap();
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == outer && r.stroke_width.is_some())
    );
    assert!(
        !ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == editor && r.stroke_width.is_some())
    );
    for button in &c.buttons {
        let bounds = ui.runtime.bounds(*button).unwrap();
        assert!(
            !ui.scene()
                .rounded_rectangles
                .iter()
                .any(|r| r.bounds == bounds && r.stroke_width.is_some())
        );
    }
    ui.set_text(c.editor, "invalid").unwrap();
    ui.commit_edit(c.editor).unwrap();
    frame(&mut ui);
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == outer
                && r.stroke_width.is_some()
                && r.color == ui.theme().tokens().error)
    );
}

#[test]
fn required_composite_parts_reject_removal_atomically_and_optional_buttons_are_pruned() {
    let (mut ui, c) = spin();
    let count = ui.runtime.tree().preorder().len();
    assert!(matches!(ui.remove(c.editor), Err(ControlError::Invalid(_))));
    assert_eq!(ui.runtime.tree().preorder().len(), count);
    ui.activate(c.buttons[1]).unwrap();
    assert_eq!(ui.spin_value(c.container), Some(2.5));
    ui.remove(c.buttons[0]).unwrap();
    assert_eq!(
        ui.editor_control_parts(c.container).unwrap().buttons,
        vec![c.buttons[1]]
    );
    assert!(!ui.rich.actions.contains_key(&c.buttons[0]));
    frame(&mut ui);
    ui.remove(c.container).unwrap();
    frame(&mut ui);
    assert!(ui.editor_control_parts(c.container).is_none());
    assert!(!ui.rich.actions.contains_key(&c.buttons[1]));
    let (mut ui, l) = list(SelectionMode::Single);
    let count = ui.runtime.tree().preorder().len();
    assert!(ui.remove(l.content).is_err());
    assert_eq!(ui.runtime.tree().preorder().len(), count);
    focus(&mut ui, l.container);
    key(&mut ui, Key::End, Modifiers::default());
    assert_eq!(ui.selected_keys(l.container).unwrap(), ["c"]);
}

#[test]
fn removing_composite_ancestor_releases_popup_and_all_actions_without_stale_parts() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parent = ui.panel(ui.root(), style(100.0)).unwrap();
    let c = ui
        .combo_box(
            parent,
            "Choice",
            items(),
            ComboBoxSettings::default(),
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    ui.remove(parent).unwrap();
    assert!(!ui.popup_open());
    assert!(ui.rich.composites.is_empty());
    assert!(ui.rich.lists.is_empty());
    assert!(ui.rich.actions.is_empty());
    frame(&mut ui);
}

#[test]
fn collection_refresh_preserves_editable_draft_preedit_history_and_live_popup() {
    let (mut ui, c) = combo(true);
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    focus(&mut ui, c.editor);
    key(
        &mut ui,
        Key::Character("a".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    ui.text_input("Be").unwrap();
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    ui.ime_preedit("候補", None).unwrap();
    let selection = ui.document(c.editor).unwrap().selection();
    let undo = ui.document(c.editor).unwrap().can_undo();
    let mut data = items();
    data.push(SelectionItem::new("d", "Bernard"));
    ui.set_selection_items(c.container, data).unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "Be");
    assert!(ui.document(c.editor).unwrap().composition().is_some());
    assert_eq!(ui.document(c.editor).unwrap().selection(), selection);
    assert_eq!(ui.document(c.editor).unwrap().can_undo(), undo);
    assert_eq!(ui.accepted_value(c.editor), Some("Alpha"));
    assert!(ui.popup_open());
    let popup_list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    assert_eq!(ui.selection_items(popup_list).unwrap().len(), 2);
}

#[test]
fn selected_caption_rename_keeps_dirty_document_but_updates_the_cancel_baseline() {
    let (mut ui, c) = combo(true);
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    focus(&mut ui, c.editor);
    ui.set_text(c.editor, "Draft").unwrap();
    ui.ime_preedit("候補", None).unwrap();
    let mut data = items();
    data[0].text = "Renamed".into();
    ui.set_selection_items(c.container, data).unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "Draft");
    assert!(ui.document(c.editor).unwrap().composition().is_some());
    assert_eq!(ui.accepted_value(c.editor), Some("Renamed"));
    ui.cancel_edit(c.editor).unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "Renamed");
    ui.set_editor_value(c.editor, "Custom accepted").unwrap();
    ui.set_selection_items(c.container, items()).unwrap();
    assert_eq!(ui.accepted_value(c.editor), Some("Custom accepted"));
}

#[test]
fn rejected_calendar_keyboard_and_pointer_choices_keep_popup_and_report_validation() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let date = DateValue::new(2026, 10, 15).unwrap();
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            Some(date),
            DateEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    ui.set_editor_validator(
        c.editor,
        Some(EditorValidator::new(|text| {
            if text == "16.10.2026" {
                Err("Unavailable day".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    key(&mut ui, Key::Enter, Modifiers::default());
    assert!(ui.popup_open());
    assert_eq!(ui.date_value(c.container), Some(Some(date)));
    assert_eq!(ui.validation_error(c.editor), Some("Unavailable day"));
    assert!(matches!(
        ui.drain_edit_events().as_slice(),
        [EditEvent::ValidationFailed { .. }]
    ));
    frame(&mut ui);
    let row = ui
        .rich
        .actions
        .iter()
        .find_map(|(&id, a)| {
            matches!(a,selection::RichAction::Date{value,..}if value.day==16).then_some(id)
        })
        .unwrap();
    ui.activate(row).unwrap();
    assert!(ui.popup_open());
    assert_eq!(ui.date_value(c.container), Some(Some(date)));
    assert!(matches!(
        ui.drain_edit_events().as_slice(),
        [EditEvent::ValidationFailed { .. }]
    ));
}

#[test]
fn external_date_update_moves_open_calendar_cursor_so_enter_cannot_restore_old_value() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let date = DateValue::new(2026, 10, 15).unwrap();
    let next = DateValue::new(2027, 2, 20).unwrap();
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            Some(date),
            DateEditSettings::default(),
            style(34.0),
        )
        .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    ui.set_date_value(c.container, Some(next)).unwrap();
    assert_eq!(ui.rich.popup.as_ref().unwrap().month, Some(next));
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.date_value(c.container), Some(Some(next)));
    assert!(!ui.popup_open());
}

#[test]
fn composite_semantics_expose_display_value_and_readonly_has_no_expand_action() {
    let (mut ui, c) = combo(false);
    ui.set_selected_keys(c.container, &["a".into()]).unwrap();
    ui.set_composite_read_only(c.container, true).unwrap();
    frame(&mut ui);
    let semantics = ui.semantics();
    let parent = semantics.iter().find(|n| n.id == c.container).unwrap();
    assert_eq!(parent.role, Role::ComboBox);
    assert_eq!(parent.value.as_deref(), Some("Alpha"));
    assert_eq!(parent.label, "Choice");
    assert!(parent.read_only);
    assert!(!parent.actions.contains(&SemanticAction::Expand));
    assert!(!parent.actions.contains(&SemanticAction::SetValue));
    assert!(parent.has_popup);
}
