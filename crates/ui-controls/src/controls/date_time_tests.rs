use super::*;
use crate::{
    BindingContext, BindingProperty, BindingUpdateTrigger, ControlBinding, ControlBindingMode,
    ControlBindings, DataValue,
};
use rust_desktop_ui_core::Modifiers;
use std::{cell::RefCell, rc::Rc};
fn d(y: i32, m: u8, day: u8) -> DateValue {
    DateValue::new(y, m, day).unwrap()
}
fn dt(text: &str) -> DateTimeValue {
    DateTimeValue::parse_iso(text).unwrap()
}
fn style() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(320.0),
        height: Length::Px(36.0),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(800.0, 600.0)).unwrap();
}
fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    frame(ui);
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn calendar_cell(ui: &Controls, calendar: &Calendar, date: DateValue) -> WidgetId {
    calendar
        .cells
        .iter()
        .copied()
        .find(|id| {
            ui.controls[id].automation_name.as_deref()
                == Some(date.to_display(DateFormat::Russian).as_str())
        })
        .unwrap()
}

#[test]
fn combined_iso_is_strict_and_seconds_arithmetic_crosses_leap_midnight() {
    for bad in [
        "2024-02-29 01:02:03",
        "2024-02-29T01:02",
        "2024-02-29T01:02:03Z",
        "2024-02-29T01:02:03.1",
        "1900-02-29T01:02:03",
        "2024-02-29T24:00:00",
    ] {
        assert!(DateTimeValue::parse_iso(bad).is_err(), "{bad}");
    }
    assert_eq!(
        dt("2024-02-28T23:59:59").checked_add_seconds(1),
        Some(dt("2024-02-29T00:00:00"))
    );
    assert_eq!(dt("0001-01-01T00:00:00").checked_add_seconds(-1), None);
    assert_eq!(dt("9999-12-31T23:59:59").checked_add_seconds(1), None);
    assert_eq!(
        dt("2024-02-29T00:00:00").checked_add_seconds(i64::MAX),
        None
    );
    assert!(DateRangeValue::parse_iso("2024-03-01/2024-02-29").is_err());
}
#[test]
fn hidden_seconds_survive_noop_commit_settings_and_exact_programmatic_assignment() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .time_edit(
            ui.root(),
            "Time",
            Some(TimeValue::new(12, 30, 42).unwrap()),
            TimeEditSettings::default(),
            style(),
        )
        .unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "12:30");
    assert_eq!(ui.commit_edit(c.editor).unwrap(), CommitOutcome::Unchanged);
    assert_eq!(
        ui.time_value(c.container),
        Some(Some(TimeValue::new(12, 30, 42).unwrap()))
    );
    assert_eq!(
        ui.set_time_value(c.container, Some(TimeValue::new(12, 30, 43).unwrap()))
            .unwrap(),
        CommitOutcome::Committed
    );
    ui.set_time_settings(
        c.container,
        TimeEditSettings {
            show_seconds: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "12:30:43");
    ui.set_time_settings(c.container, TimeEditSettings::default())
        .unwrap();
    ui.set_text(c.editor, "12:31").unwrap();
    ui.commit_edit(c.editor).unwrap();
    assert_eq!(
        ui.time_value(c.container),
        Some(Some(TimeValue::new(12, 31, 0).unwrap()))
    );
}
#[test]
fn combined_minute_display_keeps_seconds_when_only_date_changes_and_steps_exactly() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .date_time_edit(
            ui.root(),
            "Combined",
            Some(dt("2024-02-28T23:59:59")),
            DateTimeEditSettings {
                step_seconds: 1,
                ..Default::default()
            },
            style(),
        )
        .unwrap();
    ui.set_text(c.editor, "29.02.2024 23:59").unwrap();
    ui.commit_edit(c.editor).unwrap();
    assert_eq!(
        ui.date_time_value(c.container),
        Some(Some(dt("2024-02-29T23:59:59")))
    );
    frame(&mut ui);
    focus(&mut ui, c.editor);
    key(&mut ui, Key::ArrowUp);
    assert_eq!(
        ui.date_time_value(c.container),
        Some(Some(dt("2024-03-01T00:00:00")))
    );
}
#[test]
fn temporal_settings_reject_invalid_bounds_format_rejection_and_dirty_format_atomically() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = dt("2024-02-29T12:30:42");
    let c = ui
        .date_time_edit(
            ui.root(),
            "Combined",
            Some(original),
            DateTimeEditSettings::default(),
            style(),
        )
        .unwrap();
    ui.set_editor_validator(
        c.editor,
        Some(EditorValidator::new(|text| {
            if text.starts_with("2024-") {
                Err("format".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    assert!(
        ui.set_date_time_settings(
            c.container,
            DateTimeEditSettings {
                date_format: DateFormat::Iso,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        ui.date_time_settings(c.container).unwrap().date_format,
        DateFormat::Russian
    );
    assert_eq!(ui.date_time_value(c.container), Some(Some(original)));
    assert_eq!(ui.document(c.editor).unwrap().text(), "29.02.2024 12:30");
    assert!(
        ui.set_date_time_settings(
            c.container,
            DateTimeEditSettings {
                minimum: Some(dt("2024-03-01T00:00:00")),
                ..Default::default()
            }
        )
        .is_err()
    );
    ui.set_text(c.editor, "draft").unwrap();
    assert!(
        ui.set_date_time_settings(
            c.container,
            DateTimeEditSettings {
                show_seconds: true,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.document(c.editor).unwrap().text(), "draft");
}
#[test]
fn range_accepts_only_complete_ordered_pair_and_cancel_restores_accepted() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = DateRangeValue::new(d(2024, 2, 20), d(2024, 2, 29)).unwrap();
    let c = ui
        .date_range_edit(
            ui.root(),
            "Range",
            Some(original),
            DateRangeEditSettings::default(),
            style(),
        )
        .unwrap();
    for text in ["01.03.2024 – ", "05.03.2024 – 01.03.2024"] {
        ui.set_text(c.editor, text).unwrap();
        assert!(matches!(
            ui.commit_edit(c.editor).unwrap(),
            CommitOutcome::Invalid(_)
        ));
        assert_eq!(ui.date_range_value(c.container), Some(Some(original)));
    }
    ui.cancel_edit(c.editor).unwrap();
    assert_eq!(
        ui.document(c.editor).unwrap().text(),
        original.to_display(DateFormat::Russian)
    );
    ui.set_date_range_value(c.container, None).unwrap();
    assert_eq!(ui.date_range_value(c.container), Some(None));
}
#[test]
fn retained_calendar_views_bounds_selection_and_accessibility() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .calendar(
            ui.root(),
            "Date",
            CalendarSelection::Single(Some(d(2024, 2, 29))),
            CalendarSettings {
                minimum: Some(d(2024, 2, 20)),
                maximum: Some(d(2024, 3, 5)),
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    let count = ui.runtime.tree().preorder().len();
    let ids = c.cells.clone();
    assert!(!ui.is_enabled(c.previous));
    assert!(ui.is_enabled(c.next));
    assert!(!ui.is_enabled(calendar_cell(&ui, &c, d(2024, 2, 19))));
    let selected = calendar_cell(&ui, &c, d(2024, 2, 29));
    assert!(
        ui.semantics()
            .iter()
            .any(|n| n.id == selected && n.selected && n.label == "29.02.2024")
    );
    ui.activate(c.header).unwrap();
    assert_eq!(ui.calendar_view(c.container), Some(CalendarView::Months));
    assert!(!ui.is_enabled(c.cells[0]));
    assert!(ui.is_enabled(c.cells[1]));
    assert!(!ui.is_enabled(c.cells[3]));
    ui.activate(c.header).unwrap();
    assert_eq!(ui.calendar_view(c.container), Some(CalendarView::Years));
    ui.set_calendar_view(c.container, CalendarView::Days)
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.tree().preorder().len(), count);
    assert_eq!(ids, c.cells);
    assert!(ui.remove(c.cells[0]).is_err());
    ui.remove(c.container).unwrap();
    frame(&mut ui);
    assert!(ui.calendar_selection(c.container).is_none());
}
#[test]
fn calendar_range_two_choices_are_atomic_under_validator_and_read_only() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .calendar(
            ui.root(),
            "Range",
            CalendarSelection::Range(None),
            CalendarSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    ui.set_calendar_display_date(c.container, d(2024, 2, 20))
        .unwrap();
    frame(&mut ui);
    ui.chain_calendar_validator(
        c.container,
        CalendarValidator::new(|value| match value {
            CalendarSelection::Range(Some(v)) if v.end.day == 29 => Err("reserved".into()),
            _ => Ok(()),
        }),
    )
    .unwrap();
    ui.activate(calendar_cell(&ui, &c, d(2024, 2, 20))).unwrap();
    assert_eq!(ui.calendar_range_anchor(c.container), Some(d(2024, 2, 20)));
    ui.activate(calendar_cell(&ui, &c, d(2024, 2, 29))).unwrap();
    assert_eq!(
        ui.calendar_selection(c.container),
        Some(CalendarSelection::Range(None))
    );
    assert_eq!(ui.calendar_range_anchor(c.container), Some(d(2024, 2, 20)));
    ui.activate(calendar_cell(&ui, &c, d(2024, 2, 28))).unwrap();
    assert_eq!(
        ui.calendar_selection(c.container),
        Some(CalendarSelection::Range(Some(
            DateRangeValue::new(d(2024, 2, 20), d(2024, 2, 28)).unwrap()
        )))
    );
    assert!(
        ui.set_calendar_selection(c.container, CalendarSelection::Single(None))
            .is_err()
    );
    ui.set_calendar_settings(
        c.container,
        CalendarSettings {
            is_read_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let cell = calendar_cell(&ui, &c, d(2024, 2, 21));
    let node = ui.semantics().into_iter().find(|n| n.id == cell).unwrap();
    assert!(node.read_only);
    assert!(!node.actions.contains(&SemanticAction::Activate));
}
#[test]
fn calendar_keyboard_pages_preserve_day_and_bounds() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .calendar(
            ui.root(),
            "Date",
            CalendarSelection::Single(Some(d(2024, 1, 31))),
            CalendarSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    let cell = calendar_cell(&ui, &c, d(2024, 1, 31));
    focus(&mut ui, cell);
    key(&mut ui, Key::PageDown);
    assert_eq!(ui.calendar_display_date(c.container), Some(d(2024, 2, 29)));
    key(&mut ui, Key::Enter);
    assert_eq!(
        ui.calendar_selection(c.container),
        Some(CalendarSelection::Single(Some(d(2024, 2, 29))))
    );
}
#[test]
fn range_popup_retains_parts_draft_focus_and_explicit_apply_cancel() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = DateRangeValue::new(d(2024, 2, 1), d(2024, 2, 5)).unwrap();
    let c = ui
        .date_range_edit(
            ui.root(),
            "Range",
            Some(original),
            DateRangeEditSettings::default(),
            style(),
        )
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, c.editor);
    ui.open_editor_popup(c.container).unwrap();
    let calendar = ui.temporal_calendar(c.container).unwrap().clone();
    let count = ui.runtime.tree().preorder().len();
    ui.activate(calendar_cell(&ui, &calendar, d(2024, 2, 10)))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.date_range_value(c.container), Some(Some(original)));
    assert!(ui.document(c.editor).unwrap().text().ends_with(" – "));
    assert!(matches!(
        ui.apply_temporal_popup(c.container).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    ui.activate(calendar_cell(&ui, &calendar, d(2024, 2, 15)))
        .unwrap();
    assert_eq!(ui.date_range_value(c.container), Some(Some(original)));
    assert_eq!(
        ui.apply_temporal_popup(c.container).unwrap(),
        CommitOutcome::Committed
    );
    assert!(!ui.temporal_popup_is_open(c.container));
    let accepted = DateRangeValue::new(d(2024, 2, 10), d(2024, 2, 15)).unwrap();
    assert_eq!(ui.date_range_value(c.container), Some(Some(accepted)));
    ui.open_editor_popup(c.container).unwrap();
    assert_eq!(
        ui.temporal_calendar(c.container).unwrap().cells,
        calendar.cells
    );
    assert_eq!(ui.runtime.tree().preorder().len(), count);
    ui.activate(calendar_cell(&ui, &calendar, d(2024, 2, 20)))
        .unwrap();
    key(&mut ui, Key::Escape);
    assert!(!ui.temporal_popup_is_open(c.container));
    assert_eq!(
        ui.document(c.editor).unwrap().text(),
        accepted.to_display(DateFormat::Russian)
    );
    ui.remove(c.container).unwrap();
    frame(&mut ui);
    assert!(!ui.popup_open());
    assert!(!ui.runtime.tree().contains(calendar.container));
}
#[test]
fn temporal_and_calendar_bindings_preserve_typed_values_and_live_formats() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Time", "12:30:42".into()),
            ("Combined", "2024-02-29T12:30:43".into()),
            (
                "Range",
                DataValue::object([("start", "2024-02-20".into()), ("end", "2024-02-29".into())]),
            ),
        ]))
        .unwrap(),
    ));
    let t = ui
        .time_edit(
            ui.root(),
            "Time",
            None,
            TimeEditSettings::default(),
            style(),
        )
        .unwrap();
    let dtc = ui
        .date_time_edit(
            ui.root(),
            "Combined",
            None,
            DateTimeEditSettings::default(),
            style(),
        )
        .unwrap();
    let r = ui
        .date_range_edit(
            ui.root(),
            "Range",
            None,
            DateRangeEditSettings::default(),
            style(),
        )
        .unwrap();
    let calendar = ui
        .calendar(
            ui.root(),
            "Calendar",
            CalendarSelection::Range(None),
            CalendarSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let mut bindings = ControlBindings::new(Rc::clone(&data));
    for (id, path, trigger) in [
        (t.container, "Time", BindingUpdateTrigger::LostFocus),
        (dtc.container, "Combined", BindingUpdateTrigger::LostFocus),
        (r.container, "Range", BindingUpdateTrigger::LostFocus),
        (
            calendar.container,
            "Range",
            BindingUpdateTrigger::PropertyChanged,
        ),
    ] {
        let mut b = ControlBinding::new(id, BindingProperty::Value, path).unwrap();
        b.mode = ControlBindingMode::TwoWay;
        b.trigger = trigger;
        bindings.add(b).unwrap();
    }
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(
        ui.time_value(t.container),
        Some(Some(TimeValue::new(12, 30, 42).unwrap()))
    );
    assert_eq!(
        ui.date_time_value(dtc.container),
        Some(Some(dt("2024-02-29T12:30:43")))
    );
    assert_eq!(
        ui.calendar_selection(calendar.container),
        Some(CalendarSelection::Range(
            ui.date_range_value(r.container).unwrap()
        ))
    );
    ui.set_date_time_settings(
        dtc.container,
        DateTimeEditSettings {
            date_format: DateFormat::Iso,
            show_seconds: true,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_text(dtc.editor, "2024-03-01 12:30:44").unwrap();
    ui.commit_edit(dtc.editor).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        data.borrow().get("Combined").unwrap().as_str(),
        Some("2024-03-01T12:30:44")
    );
    data.borrow_mut().set("Time", "12:30:45".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        ui.time_value(t.container),
        Some(Some(TimeValue::new(12, 30, 45).unwrap()))
    );
    let next = DateRangeValue::new(d(2024, 3, 1), d(2024, 3, 2)).unwrap();
    ui.set_calendar_selection(calendar.container, CalendarSelection::Range(Some(next)))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        data.borrow().get("Range").unwrap().as_object().unwrap()["start"].as_str(),
        Some("2024-03-01")
    );
    bindings.sync(&mut ui);
    assert_eq!(ui.date_range_value(r.container), Some(Some(next)));
}

#[test]
fn calendar_month_grid_uses_available_height_and_does_not_keep_hidden_day_rows() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let c = ui
        .calendar(
            ui.root(),
            "Date",
            CalendarSelection::Single(Some(d(2024, 2, 29))),
            CalendarSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    let day_height = ui.runtime.bounds(c.cells[0]).unwrap().height;
    ui.set_calendar_view(c.container, CalendarView::Months)
        .unwrap();
    frame(&mut ui);
    assert!(
        ui.runtime.bounds(c.cells[0]).unwrap().height > day_height,
        "day={day_height} month={:?} root={:?} last={:?}",
        ui.runtime.bounds(c.cells[0]),
        ui.runtime.bounds(c.container),
        ui.runtime.bounds(c.cells[11])
    );
    let outer = ui.runtime.bounds(c.container).unwrap();
    let last = ui.runtime.bounds(c.cells[11]).unwrap();
    assert!(last.y + last.height <= outer.y + outer.height);
    assert!(ui.runtime.bounds(c.cells[12]).is_none());
}
#[test]
fn combined_popup_navigation_preserves_draft_and_external_value_replaces_calendar_cursor() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = dt("2024-02-29T12:30:42");
    let c = ui
        .date_time_edit(
            ui.root(),
            "Combined",
            Some(original),
            DateTimeEditSettings::default(),
            style(),
        )
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, c.editor);
    ui.set_text(c.editor, "unfinished").unwrap();
    ui.open_editor_popup(c.container).unwrap();
    let calendar = ui.temporal_calendar(c.container).unwrap().clone();
    ui.activate(calendar.header).unwrap();
    assert_eq!(ui.document(c.editor).unwrap().text(), "unfinished");
    ui.set_date_time_value(c.container, Some(dt("2025-03-02T10:20:59")))
        .unwrap();
    assert_eq!(
        ui.calendar_display_date(calendar.container),
        Some(d(2025, 3, 2))
    );
    ui.expand_popup(c.container).unwrap();
    assert!(ui.temporal_popup_is_open(c.container));
    ui.collapse_popup(c.container).unwrap();
    assert!(!ui.temporal_popup_is_open(c.container));
    ui.open_editor_popup(c.container).unwrap();
    ui.activate(calendar_cell(&ui, &calendar, d(2025, 3, 3)))
        .unwrap();
    ui.apply_temporal_popup(c.container).unwrap();
    assert_eq!(
        ui.date_time_value(c.container),
        Some(Some(dt("2025-03-03T10:20:59")))
    );
}
#[test]
fn date_popup_uses_month_and_year_views_without_committing_header_navigation() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = d(2024, 2, 29);
    let c = ui
        .date_edit(
            ui.root(),
            "Date",
            Some(original),
            DateEditSettings::default(),
            style(),
        )
        .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let surface = ui.rich.popup.as_ref().unwrap().surface;
    let calendar = ui
        .runtime
        .tree()
        .children(surface)
        .unwrap()
        .iter()
        .copied()
        .find(|id| ui.calendar_selection(*id).is_some())
        .unwrap();
    ui.set_calendar_view(calendar, CalendarView::Years).unwrap();
    assert_eq!(ui.date_value(c.container), Some(Some(original)));
    assert!(ui.popup_open());
    ui.set_calendar_view(calendar, CalendarView::Months)
        .unwrap();
    assert_eq!(ui.date_value(c.container), Some(Some(original)));
}
#[test]
fn temporal_popup_async_acceptance_closes_and_disabled_owner_discards_completion() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = dt("2024-02-29T12:30:42");
    let c = ui
        .date_time_edit(
            ui.root(),
            "Combined",
            Some(original),
            DateTimeEditSettings::default(),
            style(),
        )
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, c.editor);
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(gate));
    let (wake, completed) = mpsc::channel();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        let _ = wake.send(());
    })));
    ui.set_async_editor_validator(
        c.editor,
        Some(crate::AsyncEditorValidator::new(move |_| {
            gate.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    ui.open_editor_popup(c.container).unwrap();
    let calendar = ui.temporal_calendar(c.container).unwrap().clone();
    ui.activate(calendar_cell(&ui, &calendar, d(2024, 3, 1)))
        .unwrap();
    assert_eq!(
        ui.apply_temporal_popup(c.container).unwrap(),
        CommitOutcome::Pending
    );
    assert!(ui.temporal_popup_is_open(c.container));
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(2)).unwrap();
    frame(&mut ui);
    assert!(!ui.temporal_popup_is_open(c.container));
    let accepted = dt("2024-03-01T12:30:42");
    assert_eq!(ui.date_time_value(c.container), Some(Some(accepted)));
    ui.open_editor_popup(c.container).unwrap();
    ui.activate(calendar_cell(&ui, &calendar, d(2024, 3, 2)))
        .unwrap();
    assert_eq!(
        ui.apply_temporal_popup(c.container).unwrap(),
        CommitOutcome::Pending
    );
    ui.set_enabled(c.container, false).unwrap();
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(2)).unwrap();
    frame(&mut ui);
    assert!(!ui.temporal_popup_is_open(c.container));
    assert_eq!(ui.date_time_value(c.container), Some(Some(accepted)));
}
#[test]
fn element_calendar_source_rejects_out_of_bounds_before_target_editor_acceptance() {
    use crate::{BindingEndpoint, BindingSource};
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = DateRangeValue::new(d(2024, 2, 20), d(2024, 2, 29)).unwrap();
    let calendar = ui
        .calendar(
            ui.root(),
            "Calendar",
            CalendarSelection::Range(Some(original)),
            CalendarSettings {
                minimum: Some(d(2024, 2, 1)),
                maximum: Some(d(2024, 3, 31)),
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    let target = ui
        .date_range_edit(
            ui.root(),
            "Range",
            None,
            DateRangeEditSettings::default(),
            style(),
        )
        .unwrap();
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([] as [(&str, DataValue); 0])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(data);
    let mut link = ControlBinding::new(target.container, BindingProperty::Value, "Unused").unwrap();
    link.mode = ControlBindingMode::TwoWay;
    link.trigger = BindingUpdateTrigger::LostFocus;
    link.source = BindingSource::Element(BindingEndpoint {
        widget: calendar.container,
        property: BindingProperty::Value,
    });
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.set_calendar_settings(
        calendar.container,
        CalendarSettings {
            minimum: Some(d(2024, 2, 20)),
            maximum: Some(d(2024, 2, 29)),
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_text(target.editor, "01.03.2024 – 02.03.2024")
        .unwrap();
    assert!(matches!(
        ui.commit_edit(target.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.date_range_value(target.container), Some(Some(original)));
    assert_eq!(
        ui.calendar_selection(calendar.container),
        Some(CalendarSelection::Range(Some(original)))
    );
}

#[test]
fn direct_inner_editor_read_only_closes_temporal_popup_without_accepting_draft() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let original = dt("2024-02-29T12:30:42");
    let c = ui
        .date_time_edit(
            ui.root(),
            "Combined",
            Some(original),
            DateTimeEditSettings::default(),
            style(),
        )
        .unwrap();
    frame(&mut ui);
    ui.open_editor_popup(c.container).unwrap();
    let calendar = ui.temporal_calendar(c.container).unwrap().clone();
    ui.activate(calendar_cell(&ui, &calendar, d(2024, 3, 1)))
        .unwrap();
    ui.set_read_only(c.editor, true).unwrap();
    assert!(ui.apply_temporal_popup(c.container).is_err());
    frame(&mut ui);
    assert!(!ui.temporal_popup_is_open(c.container));
    assert_eq!(ui.date_time_value(c.container), Some(Some(original)));
    assert_eq!(ui.document(c.editor).unwrap().text(), "01.03.2024 12:30");
    ui.open_editor_popup(c.container).unwrap();
    assert!(!ui.temporal_popup_is_open(c.container));
    ui.set_read_only(c.editor, false).unwrap();
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(calendar.container).unwrap();
    assert!(bounds.height > 200.0);
    let day = calendar_cell(&ui, &calendar, d(2024, 3, 1));
    let cell = ui.runtime.bounds(day).unwrap();
    assert!(cell.height > 20.0 && cell.width > 20.0);
    let run = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == "1" && run.bounds.intersection(cell).is_some())
        .unwrap();
    assert_eq!(run.align, TextAlign::Center);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(!ui.temporal_popup_is_open(c.container));
    assert_eq!(ui.date_time_value(c.container), Some(Some(original)));
}
