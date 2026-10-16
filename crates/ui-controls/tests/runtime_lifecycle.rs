//! Public-API integration: control sidecars remain consistent with core lifecycle.
use std::{cell::RefCell, rc::Rc};

use rust_desktop_ui_controls::{
    Controls, GridControlSettings, SelectionItem, SelectionSettings, SpinBoxSettings, Theme,
};
use rust_desktop_ui_core::{
    Axis, DragCancelReason, DragPayload, DragPhase, DropEffect, DropEffects, FocusScope,
    InputEvent, Key, KeyChord, LayoutKind, LayoutStyle, Length, Modifiers, Mutation, Point,
    PointerButton, Size, WidgetId,
};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    TableSchema,
};

fn size() -> Size {
    Size::new(640.0, 480.0)
}
fn frame(ui: &mut Controls) {
    ui.update(size()).unwrap();
}
fn panel(ui: &mut Controls) -> WidgetId {
    ui.panel(
        ui.root(),
        LayoutStyle {
            kind: LayoutKind::Stack(Axis::Vertical),
            width: Length::Px(400.0),
            height: Length::Px(300.0),
            gap: 8.0,
            ..Default::default()
        },
    )
    .unwrap()
}
fn leaf() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(240.0),
        height: Length::Px(36.0),
        ..Default::default()
    }
}
fn key(ui: &mut Controls, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    })
    .unwrap();
}
fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}
fn center(ui: &Controls, id: WidgetId) -> Point {
    let rect = ui.runtime.bounds(id).unwrap();
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}
fn down(ui: &mut Controls, position: Point) {
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn up(ui: &mut Controls, position: Point) {
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn payload() -> DragPayload {
    DragPayload {
        format: "application/component-item".into(),
        data: b"item-42".to_vec(),
    }
}

#[test]
fn detached_editor_retains_draft_selection_and_undo_without_implicit_commit() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let editor = ui.edit_box(group, "Name", "original", leaf()).unwrap();
    let button = ui.button(group, "Apply", None, leaf()).unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    key(&mut ui, Key::End, Modifiers::default());
    ui.text_input(" draft").unwrap();
    key(
        &mut ui,
        Key::ArrowLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    key(
        &mut ui,
        Key::ArrowLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    let before_text = ui.document(editor).unwrap().text().to_owned();
    let before_selection = ui.document(editor).unwrap().selection();
    assert!(ui.document(editor).unwrap().can_undo());
    assert_eq!(ui.accepted_value(editor), Some("original"));
    ui.runtime.apply(Mutation::Detach(group)).unwrap();
    frame(&mut ui);
    assert!(ui.runtime.tree().contains(editor));
    assert!(!ui.runtime.tree().is_attached(editor));
    assert_eq!(ui.runtime.focused(), None);
    assert_eq!(ui.runtime.bounds(editor), None);
    assert_eq!(ui.document(editor).unwrap().text(), before_text);
    assert_eq!(ui.document(editor).unwrap().selection(), before_selection);
    assert_eq!(ui.accepted_value(editor), Some("original"));
    assert!(
        ui.semantics()
            .iter()
            .all(|node| ![group, editor, button].contains(&node.id))
    );
    assert!(ui.scene().texts.iter().all(|run| run.text != before_text));
    assert!(!ui.is_enabled(button));
    ui.activate(button).unwrap();
    assert!(ui.drain_activated().is_empty());
    ui.runtime
        .apply(Mutation::Reparent {
            node: group,
            parent: ui.root(),
        })
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().selection(), before_selection);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    key(&mut ui, Key::Character("z".into()), ctrl());
    assert_eq!(ui.document(editor).unwrap().text(), "original");
    ui.remove(group).unwrap();
    assert!(ui.document(editor).is_none());
    assert!(!ui.runtime.tree().contains(editor));
}

#[test]
fn local_shortcut_consumes_editor_key_and_button_default_activation() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let editor = ui.edit_box(group, "Name", "unchanged", leaf()).unwrap();
    let button = ui.button(group, "Apply", None, leaf()).unwrap();
    let calls = Rc::new(RefCell::new(Vec::new()));
    for owner in [ui.runtime.tree().root(), group] {
        let calls = calls.clone();
        ui.runtime
            .on_shortcut(
                owner,
                KeyChord::new(Key::Character("a".into()), ctrl()),
                false,
                move |event, _| calls.borrow_mut().push(event.owner),
            )
            .unwrap();
    }
    ui.runtime
        .set_focus_scope(
            group,
            Some(FocusScope {
                cycle_tab: true,
                isolate_shortcuts: false,
            }),
        )
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    key(&mut ui, Key::Character("a".into()), ctrl());
    assert_eq!(*calls.borrow(), vec![group]);
    assert!(ui.document(editor).unwrap().selection().is_empty());
    let enter_calls = Rc::new(RefCell::new(0));
    let target = enter_calls.clone();
    ui.runtime
        .on_shortcut(
            group,
            KeyChord::new(Key::Enter, Modifiers::default()),
            false,
            move |_, _| *target.borrow_mut() += 1,
        )
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(button))).unwrap();
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(*enter_calls.borrow(), 1);
    assert!(ui.drain_activated().is_empty());
    key(&mut ui, Key::Tab, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(editor));
    ui.runtime.apply(Mutation::Detach(group)).unwrap();
    frame(&mut ui);
    key(&mut ui, Key::Character("a".into()), ctrl());
    assert_eq!(*calls.borrow(), vec![group, ui.runtime.tree().root()]);
}

#[test]
fn dragging_control_negotiates_and_drops_once_without_clicking_source() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let source = ui.button(group, "Item", None, leaf()).unwrap();
    let target = ui.button(group, "Basket", None, leaf()).unwrap();
    let drops = Rc::new(RefCell::new(0));
    let observed = drops.clone();
    ui.runtime
        .on_drop_target(target, move |event, data| {
            assert_eq!(data.data, b"item-42");
            if event.phase == DragPhase::Dropped {
                *observed.borrow_mut() += 1;
            }
            Some(DropEffect::Copy)
        })
        .unwrap();
    frame(&mut ui);
    let start = center(&ui, source);
    let end = center(&ui, target);
    down(&mut ui, start);
    ui.runtime
        .begin_drag(source, payload(), DropEffects::COPY, start)
        .unwrap();
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    up(&mut ui, end);
    up(&mut ui, end);
    assert_eq!(*drops.borrow(), 1);
    assert!(ui.drain_activated().is_empty());
    down(&mut ui, start);
    ui.runtime
        .begin_drag(source, payload(), DropEffects::COPY, start)
        .unwrap();
    up(&mut ui, start);
    assert!(ui.drain_activated().is_empty());
    assert_eq!(*drops.borrow(), 1);
    assert_eq!(ui.runtime.captured(), None);
}

#[test]
fn escape_cancels_drag_without_reverting_editor_draft_and_blur_clears_capture() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let editor = ui.edit_box(group, "Name", "accepted", leaf()).unwrap();
    let target = ui.button(group, "Basket", None, leaf()).unwrap();
    ui.runtime
        .on_drop_target(target, |event, _| {
            assert_ne!(event.phase, DragPhase::Dropped);
            Some(DropEffect::Copy)
        })
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    key(&mut ui, Key::End, Modifiers::default());
    ui.text_input(" draft").unwrap();
    let start = center(&ui, editor);
    let end = center(&ui, target);
    ui.runtime
        .begin_drag(editor, payload(), DropEffects::COPY, start)
        .unwrap();
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    key(&mut ui, Key::Escape, Modifiers::default());
    assert!(ui.runtime.drag_session().is_none());
    assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
    assert!(
        ui.runtime
            .take_drag_events()
            .iter()
            .any(|event| event.phase == DragPhase::Cancelled(DragCancelReason::Escape))
    );
    ui.runtime
        .begin_drag(editor, payload(), DropEffects::COPY, start)
        .unwrap();
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(ui.runtime.drag_session().is_none());
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.runtime.focused(), None);
}

#[test]
fn detach_remount_preserves_control_handlers_and_dispose_never_revives_them() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let button = ui.button(group, "Run", None, leaf()).unwrap();
    let deliveries = Rc::new(RefCell::new(0));
    let target = deliveries.clone();
    ui.runtime
        .on(button, move |_, _| *target.borrow_mut() += 1)
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(button))).unwrap();
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(*deliveries.borrow(), 1);
    ui.runtime.apply(Mutation::Detach(group)).unwrap();
    frame(&mut ui);
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(*deliveries.borrow(), 1);
    ui.runtime
        .apply(Mutation::Reparent {
            node: group,
            parent: ui.root(),
        })
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(button))).unwrap();
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(*deliveries.borrow(), 2);
    ui.remove(group).unwrap();
    let replacement = panel(&mut ui);
    let replacement_button = ui.button(replacement, "Replacement", None, leaf()).unwrap();
    frame(&mut ui);
    assert_ne!(replacement_button, button);
    ui.runtime
        .apply(Mutation::Focus(Some(replacement_button)))
        .unwrap();
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(*deliveries.borrow(), 2);
}

#[test]
fn list_and_spin_share_shortcut_scopes_before_their_own_navigation() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let list = ui
        .list_box(
            group,
            "Items",
            vec![
                SelectionItem::new("a", "First"),
                SelectionItem::new("b", "Second"),
            ],
            SelectionSettings::default(),
            LayoutStyle {
                height: Length::Px(100.0),
                ..leaf()
            },
        )
        .unwrap();
    let spin = ui
        .spin_box(
            group,
            "Count",
            2.0,
            SpinBoxSettings {
                minimum: 0.0,
                maximum: 10.0,
                step: 1.0,
            },
            leaf(),
        )
        .unwrap();
    ui.set_selected_keys(list.container, &["a".into()]).unwrap();
    let calls = Rc::new(RefCell::new(0));
    for navigation in [Key::ArrowDown, Key::ArrowUp] {
        let target = calls.clone();
        ui.runtime
            .on_shortcut(
                group,
                KeyChord::new(navigation, Modifiers::default()),
                false,
                move |_, _| *target.borrow_mut() += 1,
            )
            .unwrap();
    }
    frame(&mut ui);
    ui.runtime
        .apply(Mutation::Focus(Some(list.viewport)))
        .unwrap();
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    assert_eq!(ui.selected_keys(list.container).unwrap(), &["a".to_owned()]);
    ui.runtime
        .apply(Mutation::Focus(Some(spin.editor)))
        .unwrap();
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(ui.spin_value(spin.container), Some(2.0));
    assert_eq!(*calls.borrow(), 2);
    ui.runtime
        .remove_shortcut(group, &KeyChord::new(Key::ArrowUp, Modifiers::default()));
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(ui.spin_value(spin.container), Some(3.0));
}

#[test]
fn consumed_character_shortcut_suppresses_only_its_following_text_event() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = panel(&mut ui);
    let editor = ui.edit_box(group, "Name", "", leaf()).unwrap();
    ui.runtime
        .on_shortcut(
            group,
            KeyChord::new(Key::Character("q".into()), Modifiers::default()),
            false,
            |_, _| {},
        )
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    key(&mut ui, Key::Character("q".into()), Modifiers::default());
    ui.text_input("q").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "");
    // A nonmatching text event is independent and must not be discarded.
    key(&mut ui, Key::Character("q".into()), Modifiers::default());
    ui.text_input("x").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "x");
    // KeyUp ends the association with its earlier KeyDown.
    key(&mut ui, Key::Character("q".into()), Modifiers::default());
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Character("q".into()),
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.text_input("q").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "xq");
    // IME commits are a separate path, not keyboard text fallback.
    key(&mut ui, Key::Character("q".into()), Modifiers::default());
    ui.ime_commit("ж").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "xqж");
    ui.set_text(editor, "programmatic").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "programmatic");
}

#[test]
fn runtime_removal_prunes_control_state_but_detach_preserves_scroll_and_draft() {
    for deferred in [false, true] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let group = panel(&mut ui);
        let scroll = ui.scroll_view(group, leaf(), 600.0).unwrap();
        let editor = ui
            .edit_box(scroll.content, "Draft", "original", leaf())
            .unwrap();
        let trigger = ui.button(ui.root(), "Remove", None, leaf()).unwrap();
        frame(&mut ui);
        ui.set_text(editor, "retained draft").unwrap();
        ui.scroll_to(scroll.viewport, 50.0).unwrap();
        frame(&mut ui);
        let offset = ui.scroll_offset(scroll.viewport);
        ui.runtime.apply(Mutation::Detach(group)).unwrap();
        frame(&mut ui);
        assert_eq!(ui.document(editor).unwrap().text(), "retained draft");
        assert_eq!(ui.scroll_offset(scroll.viewport), offset);
        ui.runtime
            .apply(Mutation::Reparent {
                node: group,
                parent: ui.root(),
            })
            .unwrap();
        frame(&mut ui);
        assert_eq!(ui.scroll_offset(scroll.viewport), offset);
        if deferred {
            ui.runtime
                .on(trigger, move |event, context| {
                    if matches!(
                        event.input,
                        InputEvent::KeyDown {
                            key: Key::Enter,
                            ..
                        }
                    ) {
                        context.enqueue(Mutation::Remove(group));
                    }
                })
                .unwrap();
            ui.runtime.apply(Mutation::Focus(Some(trigger))).unwrap();
            key(&mut ui, Key::Enter, Modifiers::default());
            assert!(
                ui.document(editor).is_none(),
                "dispatch must prune immediately"
            );
        } else {
            ui.runtime.apply_batch(&[Mutation::Remove(group)]).unwrap();
        }
        // A stale ScrollViewer sidecar used to fail this frame with Unknown node.
        frame(&mut ui);
        assert!(ui.document(editor).is_none());
        assert!(ui.scroll_offset(scroll.viewport).is_none());
        assert!(ui.runtime.tree().contains(trigger));
        let replacement = panel(&mut ui);
        assert_ne!(replacement, group);
        frame(&mut ui);
    }
}

#[test]
fn shortcut_removal_disconnects_grid_notifications_before_the_next_frame() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let schema = TableSchema::new(vec![FieldSchema::new("value", FieldType::Integer)]).unwrap();
    let adapter = CollectionAdapter::new(
        DataTable::new(schema, vec![DataRow::new(1, vec![CellValue::Integer(10)])]).unwrap(),
    );
    let collection = adapter.handle();
    let source = Rc::new(RefCell::new(DataSource::new(adapter).unwrap()));
    let grid = ui
        .grid_control(
            ui.root(),
            source.clone(),
            GridControlSettings::default(),
            leaf(),
        )
        .unwrap();
    let wakes = Rc::new(RefCell::new(0));
    let observer = wakes.clone();
    ui.set_data_waker(Some(Rc::new(move || *observer.borrow_mut() += 1)))
        .unwrap();
    frame(&mut ui);
    collection
        .update_cell(1, "value", CellValue::Integer(11))
        .unwrap();
    assert_eq!(*wakes.borrow(), 1);
    ui.runtime
        .on_shortcut(
            ui.runtime.tree().root(),
            KeyChord::new(Key::Enter, ctrl()),
            false,
            move |_, context| {
                context.enqueue(Mutation::Remove(grid));
            },
        )
        .unwrap();
    key(&mut ui, Key::Enter, ctrl());
    assert!(ui.grid_source(grid).is_none());
    *wakes.borrow_mut() = 0;
    collection
        .update_cell(1, "value", CellValue::Integer(12))
        .unwrap();
    assert_eq!(
        *wakes.borrow(),
        0,
        "removed grid must release its wake subscription"
    );
    frame(&mut ui);
}

#[test]
fn radio_tab_wrap_uses_the_active_cycle_scope_instead_of_external_controls() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let scope = panel(&mut ui);
    let group = ui.radio_group();
    let selected = ui
        .radio_button(scope, "First", group, true, leaf())
        .unwrap();
    let other = ui
        .radio_button(scope, "Second", group, false, leaf())
        .unwrap();
    let outside = ui.button(ui.root(), "Outside", None, leaf()).unwrap();
    ui.runtime
        .set_focus_scope(
            scope,
            Some(FocusScope {
                cycle_tab: true,
                isolate_shortcuts: false,
            }),
        )
        .unwrap();
    frame(&mut ui);
    for shift in [false, true] {
        ui.runtime.apply(Mutation::Focus(Some(other))).unwrap();
        assert!(!ui.runtime.navigation_order().contains(&outside));
        key(
            &mut ui,
            Key::Tab,
            Modifiers {
                shift,
                ..Default::default()
            },
        );
        assert_eq!(ui.runtime.focused(), Some(selected));
    }
}

#[test]
fn tab_header_wrap_stays_in_the_active_cycle_scope() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let scope = panel(&mut ui);
    let tabs = ui
        .tab_control(
            scope,
            &["First", "Second"],
            LayoutStyle {
                width: Length::Px(300.0),
                height: Length::Px(200.0),
                ..Default::default()
            },
        )
        .unwrap();
    let outside = ui.button(ui.root(), "Outside", None, leaf()).unwrap();
    ui.runtime
        .set_focus_scope(
            scope,
            Some(FocusScope {
                cycle_tab: true,
                isolate_shortcuts: false,
            }),
        )
        .unwrap();
    frame(&mut ui);
    for shift in [false, true] {
        ui.runtime
            .apply(Mutation::Focus(Some(tabs.tabs[1])))
            .unwrap();
        assert!(!ui.runtime.navigation_order().contains(&outside));
        key(
            &mut ui,
            Key::Tab,
            Modifiers {
                shift,
                ..Default::default()
            },
        );
        assert_eq!(ui.runtime.focused(), Some(tabs.tabs[0]));
    }
}
