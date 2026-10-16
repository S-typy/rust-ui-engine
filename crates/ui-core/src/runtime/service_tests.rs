use super::*;
use crate::{
    DragCancelReason, DragPayload, DragPhase, DropEffect, DropEffects, FocusScope, KeyChord,
    Modifiers, TemplateNode,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Layout;
impl LayoutEngine for Layout {
    fn compute(&mut self, tree: &UiTree, size: Size) -> Result<LayoutSnapshot, LayoutError> {
        let order = tree.preorder();
        Ok(LayoutSnapshot {
            boxes: order
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    (
                        *id,
                        if index == 0 {
                            Rect::new(0.0, 0.0, size.width, size.height)
                        } else {
                            Rect::new(index as f32 * 30.0, 0.0, 25.0, 25.0)
                        },
                    )
                })
                .collect(),
            measured_nodes: order.len(),
            arranged_nodes: order.len(),
        })
    }
}
fn frame(ui: &mut UiRuntime) -> FrameStats {
    ui.update(&mut Layout, Size::new(300.0, 100.0)).unwrap()
}
fn focusable() -> NodeProps {
    NodeProps {
        focusable: true,
        ..Default::default()
    }
}
fn key(key: Key, control: bool, repeat: bool) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
        repeat,
    }
}
fn chord() -> KeyChord {
    KeyChord::new(
        Key::Character("s".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    )
}
fn payload() -> DragPayload {
    DragPayload {
        format: "application/test-item".into(),
        data: vec![1, 2, 3],
    }
}
fn setup() -> (UiRuntime, WidgetId, WidgetId) {
    let mut tree = UiTree::new();
    let first = tree.insert(tree.root(), focusable()).unwrap();
    let second = tree.insert(tree.root(), focusable()).unwrap();
    let mut ui = UiRuntime::new(tree);
    frame(&mut ui);
    (ui, first, second)
}
fn over(ui: &mut UiRuntime, x: f32) {
    assert!(
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(x, 10.0)
        })
        .errors
        .is_empty()
    );
}
fn release(ui: &mut UiRuntime, x: f32) {
    assert!(
        ui.dispatch(InputEvent::PointerUp {
            position: Point::new(x, 10.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default()
        })
        .errors
        .is_empty()
    );
}

#[test]
fn detached_components_keep_handlers_but_leave_layout_paint_hit_and_focus() {
    let (mut ui, first, second) = setup();
    let hits = Rc::new(RefCell::new(0));
    let target = hits.clone();
    ui.on(first, move |_, _| *target.borrow_mut() += 1).unwrap();
    ui.apply_batch(&[
        Mutation::Focus(Some(first)),
        Mutation::CapturePointer(Some(first)),
    ])
    .unwrap();
    ui.apply(Mutation::Detach(first)).unwrap();
    assert!(ui.tree.contains(first));
    assert!(!ui.tree.is_attached(first));
    assert_eq!(ui.focused(), None);
    assert_eq!(ui.captured(), None);
    assert_eq!(ui.bounds(first), None);
    assert_eq!(ui.layout_bounds(first), None);
    assert_ne!(ui.hit_test(Point::new(35.0, 10.0)), Some(first));
    let result = frame(&mut ui);
    assert_eq!(result.measured_nodes, 2);
    assert_eq!(result.painted_nodes, 2);
    ui.dispatch(key(Key::Enter, false, false));
    assert_eq!(*hits.borrow(), 0);
    ui.apply(Mutation::Reparent {
        node: first,
        parent: ui.tree.root(),
    })
    .unwrap();
    frame(&mut ui);
    ui.apply(Mutation::Focus(Some(first))).unwrap();
    ui.dispatch(key(Key::Enter, false, false));
    assert_eq!(*hits.borrow(), 1);
    assert_eq!(ui.tree.preorder(), vec![ui.tree.root(), second, first]);
}

#[test]
fn disposing_detached_root_invalidates_descendants_and_drops_handlers_once() {
    struct CountDrop(Rc<RefCell<usize>>);
    impl Drop for CountDrop {
        fn drop(&mut self) {
            *self.0.borrow_mut() += 1;
        }
    }
    let (mut ui, first, _) = setup();
    let child = ui.insert(first, NodeProps::default()).unwrap();
    let drops = Rc::new(RefCell::new(0));
    let marker = CountDrop(drops.clone());
    ui.on(child, move |_, _| {
        let _ = &marker;
    })
    .unwrap();
    ui.apply(Mutation::Detach(first)).unwrap();
    assert_eq!(*drops.borrow(), 0);
    ui.apply(Mutation::Remove(first)).unwrap();
    assert!(!ui.tree.contains(child));
    assert_eq!(*drops.borrow(), 1);
    assert!(ui.apply(Mutation::Remove(first)).is_err());
    assert_eq!(*drops.borrow(), 1);
}

#[test]
fn failed_batch_preserves_tree_dirty_state_focus_capture_handlers_and_drag() {
    let (mut ui, first, second) = setup();
    ui.apply(Mutation::Focus(Some(first))).unwrap();
    ui.begin_drag(first, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    frame(&mut ui);
    ui.take_drag_events();
    let before = ui.tree.preorder();
    let result = ui.apply_batch(&[
        Mutation::Remove(first),
        Mutation::SetProps {
            node: second,
            props: NodeProps {
                translation: Point::new(f32::NAN, 0.0),
                ..Default::default()
            },
        },
    ]);
    assert!(result.is_err());
    assert_eq!(ui.tree.preorder(), before);
    assert!(ui.tree.pending_dirty().is_empty());
    assert_eq!(ui.focused(), Some(first));
    assert_eq!(ui.captured(), Some(first));
    assert!(ui.drag_session().is_some());
    assert!(ui.take_drag_events().is_empty());
    assert_eq!(frame(&mut ui), FrameStats::default());
}

#[test]
fn successful_batch_coalesces_layout_and_nested_rejection_does_not_partially_apply() {
    let (mut ui, first, second) = setup();
    let old = ui.tree.node(first).unwrap().props.clone();
    let mut changed = old.clone();
    changed.style.width = crate::Length::Px(42.0);
    ui.apply_batch(&[
        Mutation::SetProps {
            node: first,
            props: changed.clone(),
        },
        Mutation::Reparent {
            node: second,
            parent: first,
        },
    ])
    .unwrap();
    assert_eq!(frame(&mut ui).layout_passes, 1);
    assert_eq!(frame(&mut ui), FrameStats::default());
    assert!(
        ui.apply(Mutation::Batch(vec![
            Mutation::SetProps {
                node: first,
                props: old
            },
            Mutation::Batch(vec![Mutation::Reparent {
                node: first,
                parent: second
            }]),
        ]))
        .is_err()
    );
    assert_eq!(ui.tree.node(first).unwrap().props, changed);
    assert!(!ui.needs_update());
}

#[test]
fn atomic_callback_batch_preserves_route_and_later_fifo_operations() {
    let (mut ui, first, second) = setup();
    ui.on(first, move |_, context| {
        context.enqueue(Mutation::Batch(vec![
            Mutation::Remove(first),
            Mutation::Focus(Some(first)),
        ]));
        context.enqueue(Mutation::Focus(Some(second)));
    })
    .unwrap();
    ui.apply(Mutation::Focus(Some(first))).unwrap();
    let report = ui.dispatch(key(Key::Enter, false, false));
    assert_eq!(report.errors.len(), 1);
    assert!(ui.tree.contains(first));
    assert_eq!(ui.focused(), Some(second));
}

#[test]
fn instantiate_mounts_named_structure_and_failed_parent_does_not_allocate_ids() {
    let (mut ui, _, _) = setup();
    let template = SubtreeTemplate::new(vec![
        TemplateNode {
            name: Some("panel".into()),
            parent: None,
            props: NodeProps::default(),
        },
        TemplateNode {
            name: Some("field".into()),
            parent: Some(0),
            props: focusable(),
        },
    ])
    .unwrap();
    let before = ui.tree.preorder();
    assert!(ui.instantiate(UiTree::new().root(), &template).is_err());
    assert_eq!(ui.tree.preorder(), before);
    assert!(!ui.needs_update());
    let a = ui.instantiate(ui.tree.root(), &template).unwrap();
    let b = ui.instantiate(ui.tree.root(), &template).unwrap();
    assert_ne!(a.named("field"), b.named("field"));
    assert_eq!(ui.tree.parent(a.named("field").unwrap()), Some(a.root()));
    assert_eq!(frame(&mut ui).layout_passes, 1);
}

#[test]
fn nearest_shortcut_scope_wins_and_repeats_do_not_fall_through() {
    let (mut ui, first, _) = setup();
    let child = ui.insert(first, focusable()).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    for owner in [ui.tree.root(), first] {
        let log = log.clone();
        ui.on_shortcut(owner, chord(), false, move |event, _| {
            log.borrow_mut().push(event.owner)
        })
        .unwrap();
    }
    ui.apply(Mutation::Focus(Some(child))).unwrap();
    let report = ui.dispatch(key(Key::Character("S".into()), true, false));
    assert_eq!(report.shortcut, Some(first));
    assert!(report.default_prevented);
    assert_eq!(*log.borrow(), vec![first]);
    let report = ui.dispatch(key(Key::Character("s".into()), true, true));
    assert!(report.default_prevented);
    assert_eq!(report.shortcut, None);
    assert_eq!(*log.borrow(), vec![first]);
    ui.remove_shortcut(first, &chord());
    assert_eq!(
        ui.dispatch(key(Key::Character("s".into()), true, false))
            .shortcut,
        Some(ui.tree.root())
    );
}

#[test]
fn isolated_shortcut_scope_blocks_parent_and_routed_prevention_blocks_all_shortcuts() {
    let (mut ui, first, _) = setup();
    let root = ui.tree.root();
    let count = Rc::new(RefCell::new(0));
    let log = count.clone();
    ui.on_shortcut(root, chord(), true, move |_, _| *log.borrow_mut() += 1)
        .unwrap();
    ui.set_focus_scope(
        first,
        Some(FocusScope {
            isolate_shortcuts: true,
            cycle_tab: false,
        }),
    )
    .unwrap();
    ui.apply(Mutation::Focus(Some(first))).unwrap();
    ui.dispatch(key(Key::Character("s".into()), true, false));
    assert_eq!(*count.borrow(), 0);
    ui.set_focus_scope(first, None).unwrap();
    ui.on(first, |_, context| context.prevent_default())
        .unwrap();
    ui.dispatch(key(Key::Character("s".into()), true, false));
    assert_eq!(*count.borrow(), 0);
    ui.on(first, |_, context| context.stop_propagation())
        .unwrap();
    ui.dispatch(key(Key::Character("s".into()), true, false));
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn tab_scope_wraps_only_eligible_descendants_and_explicit_focus_can_leave() {
    let (mut ui, group, outside) = setup();
    let a = ui.insert(group, focusable()).unwrap();
    let b = ui.insert(group, focusable()).unwrap();
    let mut props = ui.tree.node(group).unwrap().props.clone();
    props.tab_stop = false;
    ui.apply(Mutation::SetProps { node: group, props }).unwrap();
    ui.set_focus_scope(
        group,
        Some(FocusScope {
            cycle_tab: true,
            isolate_shortcuts: false,
        }),
    )
    .unwrap();
    ui.apply(Mutation::Focus(Some(b))).unwrap();
    ui.dispatch(key(Key::Tab, false, false));
    assert_eq!(ui.focused(), Some(a));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(ui.focused(), Some(b));
    ui.apply(Mutation::Focus(Some(outside))).unwrap();
    assert_eq!(ui.focused(), Some(outside));
}

#[test]
fn shortcut_callback_mutations_are_deferred_and_removed_scope_cannot_revive() {
    let (mut ui, first, second) = setup();
    ui.on_shortcut(first, chord(), false, move |_, context| {
        context.enqueue(Mutation::Remove(first))
    })
    .unwrap();
    ui.apply(Mutation::Focus(Some(first))).unwrap();
    assert_eq!(
        ui.dispatch(key(Key::Character("s".into()), true, false))
            .shortcut,
        Some(first)
    );
    assert!(!ui.tree.contains(first));
    let replacement = ui.insert(ui.tree.root(), focusable()).unwrap();
    assert_ne!(replacement, first);
    ui.apply(Mutation::Focus(Some(replacement))).unwrap();
    assert_eq!(
        ui.dispatch(key(Key::Character("s".into()), true, false))
            .shortcut,
        None
    );
    assert!(ui.tree.contains(second));
}

#[test]
fn drag_negotiates_real_hit_while_routed_pointer_remains_captured_then_drops_once() {
    let (mut ui, source, target) = setup();
    let log = Rc::new(RefCell::new(Vec::new()));
    let observed = log.clone();
    ui.on_drop_target(target, move |event, payload| {
        assert_eq!(payload.data, vec![1, 2, 3]);
        observed.borrow_mut().push(event.phase);
        Some(DropEffect::Copy)
    })
    .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    let report = ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(65.0, 10.0),
    });
    assert_eq!(report.target, Some(source));
    assert_eq!(ui.drag_session().unwrap().target, Some(target));
    assert_eq!(ui.drag_session().unwrap().effect, Some(DropEffect::Copy));
    release(&mut ui, 65.0);
    release(&mut ui, 65.0);
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|phase| **phase == DragPhase::Dropped)
            .count(),
        1
    );
    assert_eq!(ui.captured(), None);
    assert!(ui.drag_session().is_none());
    assert_eq!(
        ui.take_drag_events()
            .iter()
            .filter(|event| event.phase == DragPhase::Dropped)
            .count(),
        1
    );
}

#[test]
fn unsupported_effect_rejects_and_pointer_leave_clears_acceptance() {
    let (mut ui, source, target) = setup();
    ui.on_drop_target(target, |_, _| Some(DropEffect::Move))
        .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    over(&mut ui, 65.0);
    assert_eq!(ui.drag_session().unwrap().effect, None);
    release(&mut ui, 65.0);
    assert!(
        ui.take_drag_events()
            .iter()
            .any(|event| event.phase == DragPhase::Cancelled(DragCancelReason::Rejected))
    );
    ui.on_drop_target(target, |_, _| Some(DropEffect::Copy))
        .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    over(&mut ui, 65.0);
    ui.dispatch(InputEvent::PointerLeft);
    assert_eq!(ui.drag_session().unwrap().target, None);
    assert_eq!(ui.drag_session().unwrap().effect, None);
    assert!(
        ui.take_drag_events()
            .iter()
            .any(|event| event.phase == DragPhase::Leave)
    );
}

#[test]
fn pointer_left_does_not_reenter_a_drag_target_after_geometry_refresh() {
    let (mut ui, source, target) = setup();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let recorded = phases.clone();
    ui.on_drop_target(target, move |event, _| {
        recorded.borrow_mut().push(event.phase);
        Some(DropEffect::Copy)
    })
    .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    over(&mut ui, 65.0);
    ui.dispatch(InputEvent::PointerLeft);
    assert_eq!(ui.drag_session().unwrap().target, None);
    phases.borrow_mut().clear();
    let mut props = ui.tree.node(target).unwrap().props.clone();
    props.translation.x = 1.0;
    ui.apply(Mutation::SetProps {
        node: target,
        props,
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.drag_session().unwrap().target, None);
    assert!(phases.borrow().is_empty());
    over(&mut ui, 65.0);
    assert_eq!(ui.drag_session().unwrap().target, Some(target));
}

#[test]
fn blur_escape_capture_change_detach_and_remove_cancel_without_drop() {
    for cause in 0..6 {
        let (mut ui, source, target) = setup();
        ui.on_drop_target(target, |_, _| Some(DropEffect::Copy))
            .unwrap();
        ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
            .unwrap();
        over(&mut ui, 65.0);
        let expected = match cause {
            0 => {
                ui.dispatch(InputEvent::WindowFocused(false));
                DragCancelReason::WindowBlur
            }
            1 => {
                ui.dispatch(key(Key::Escape, false, false));
                DragCancelReason::Escape
            }
            2 => {
                ui.apply(Mutation::CapturePointer(Some(target))).unwrap();
                DragCancelReason::CaptureChanged
            }
            3 => {
                ui.apply(Mutation::Detach(source)).unwrap();
                DragCancelReason::SourceUnavailable
            }
            4 => {
                ui.apply(Mutation::Remove(source)).unwrap();
                DragCancelReason::SourceUnavailable
            }
            _ => {
                ui.apply(Mutation::Remove(target)).unwrap();
                DragCancelReason::TargetUnavailable
            }
        };
        assert!(ui.drag_session().is_none());
        let events = ui.take_drag_events();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.phase == DragPhase::Cancelled(expected))
                .count(),
            1,
            "cause {cause}"
        );
        assert!(!events.iter().any(|event| event.phase == DragPhase::Dropped));
        if cause == 2 {
            assert_eq!(ui.captured(), Some(target));
        } else {
            assert_eq!(ui.captured(), None);
        }
    }
}

#[test]
fn removal_during_pointer_up_routing_cancels_before_target_drop() {
    let (mut ui, source, target) = setup();
    ui.on_drop_target(target, |event, _| {
        assert_ne!(event.phase, DragPhase::Dropped);
        Some(DropEffect::Copy)
    })
    .unwrap();
    ui.on(source, move |event, context| {
        if matches!(event.input, InputEvent::PointerUp { .. }) {
            context.enqueue(Mutation::Remove(target));
        }
    })
    .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    over(&mut ui, 65.0);
    release(&mut ui, 65.0);
    assert!(ui.drag_session().is_none());
    assert!(
        ui.take_drag_events()
            .iter()
            .any(|event| event.phase == DragPhase::Cancelled(DragCancelReason::TargetUnavailable))
    );
}

#[test]
fn failed_batch_does_not_emit_drag_callbacks_and_successful_batch_cancels_once() {
    let (mut ui, source, target) = setup();
    let log = Rc::new(RefCell::new(Vec::new()));
    let observed = log.clone();
    ui.on_drop_target(target, move |event, _| {
        observed.borrow_mut().push(event.phase);
        Some(DropEffect::Copy)
    })
    .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
        .unwrap();
    over(&mut ui, 65.0);
    log.borrow_mut().clear();
    assert!(
        ui.apply_batch(&[Mutation::Remove(source), Mutation::Focus(Some(source))])
            .is_err()
    );
    assert!(log.borrow().is_empty());
    assert!(ui.drag_session().is_some());
    ui.apply_batch(&[Mutation::Detach(source), Mutation::Focus(Some(target))])
        .unwrap();
    assert_eq!(
        *log.borrow(),
        vec![DragPhase::Cancelled(DragCancelReason::SourceUnavailable)]
    );
    assert_eq!(ui.focused(), Some(target));
}

#[test]
fn invalid_drag_begin_is_atomic_and_stale_cancel_does_not_touch_new_session() {
    let (mut ui, source, _) = setup();
    assert!(
        ui.begin_drag(
            source,
            DragPayload {
                format: "".into(),
                data: vec![]
            },
            DropEffects::COPY,
            Point::default()
        )
        .is_err()
    );
    assert_eq!(ui.captured(), None);
    let old = ui
        .begin_drag(source, payload(), DropEffects::COPY, Point::default())
        .unwrap();
    ui.cancel_drag(old).unwrap();
    let new = ui
        .begin_drag(source, payload(), DropEffects::COPY, Point::default())
        .unwrap();
    assert_ne!(old, new);
    assert!(ui.cancel_drag(old).is_err());
    assert_eq!(ui.drag_session().unwrap().id, new);
    assert_eq!(ui.captured(), Some(source));
}

#[test]
fn shortcut_probe_observes_scopes_without_invoking_handlers() {
    let (mut ui, first, _) = setup();
    let event = key(Key::Character("s".into()), true, false);
    let called = Rc::new(RefCell::new(0));
    let log = called.clone();
    ui.on_shortcut(ui.tree.root(), chord(), false, move |_, _| {
        *log.borrow_mut() += 1
    })
    .unwrap();
    ui.apply(Mutation::Focus(Some(first))).unwrap();
    assert!(ui.has_shortcut(&event));
    assert_eq!(*called.borrow(), 0);
    ui.set_focus_scope(
        first,
        Some(FocusScope {
            isolate_shortcuts: true,
            cycle_tab: false,
        }),
    )
    .unwrap();
    assert!(!ui.has_shortcut(&event));
    ui.apply(Mutation::Detach(first)).unwrap();
    assert!(ui.has_shortcut(&event));
    ui.dispatch(InputEvent::WindowFocused(false));
    assert!(!ui.has_shortcut(&event));
    assert!(!ui.has_shortcut(&InputEvent::KeyUp {
        key: Key::Character("s".into()),
        modifiers: Modifiers::default()
    }));
    assert_eq!(*called.borrow(), 0);
}

#[test]
fn detached_mutations_and_noop_reparent_do_not_schedule_visible_work() {
    let (mut ui, first, second) = setup();
    ui.apply(Mutation::Reparent {
        node: second,
        parent: ui.tree.root(),
    })
    .unwrap();
    assert_eq!(frame(&mut ui), FrameStats::default());
    ui.apply(Mutation::Detach(first)).unwrap();
    frame(&mut ui);
    let child = ui.insert(first, NodeProps::default()).unwrap();
    let mut props = ui.tree.node(child).unwrap().props.clone();
    props.style.width = crate::Length::Px(99.0);
    ui.apply(Mutation::SetProps { node: child, props }).unwrap();
    ui.apply(Mutation::SetPaint {
        node: child,
        paint: Paint {
            background: Some(crate::Color::rgb(1, 2, 3)),
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(frame(&mut ui), FrameStats::default());
    ui.apply(Mutation::Remove(child)).unwrap();
    assert_eq!(frame(&mut ui), FrameStats::default());
}

#[test]
fn moving_target_under_stationary_drag_refreshes_negotiation_before_release() {
    let (mut ui, source, target) = setup();
    ui.on_drop_target(target, |_, _| Some(DropEffect::Copy))
        .unwrap();
    ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(65.0, 10.0))
        .unwrap();
    over(&mut ui, 65.0);
    assert_eq!(ui.drag_session().unwrap().target, Some(target));
    let mut props = ui.tree.node(target).unwrap().props.clone();
    props.translation.x = 120.0;
    ui.apply(Mutation::SetProps {
        node: target,
        props,
    })
    .unwrap();
    assert_eq!(frame(&mut ui).layout_passes, 0);
    assert_eq!(ui.drag_session().unwrap().target, None);
    assert_eq!(ui.drag_session().unwrap().effect, None);
    assert!(
        ui.take_drag_events()
            .iter()
            .any(|event| event.phase == DragPhase::Leave)
    );
}

#[test]
fn pointer_up_mutations_cannot_drop_using_the_targets_previous_geometry() {
    for layout_change in [false, true] {
        let (mut ui, source, target) = setup();
        let mut props = ui.tree.node(target).unwrap().props.clone();
        if layout_change {
            props.style.width = crate::Length::Px(10.0);
        } else {
            props.translation.x = 200.0;
        }
        ui.on(source, move |event, context| {
            if matches!(event.input, InputEvent::PointerUp { .. }) {
                context.enqueue(Mutation::SetProps {
                    node: target,
                    props: props.clone(),
                });
            }
        })
        .unwrap();
        ui.on_drop_target(target, |event, _| {
            assert_ne!(event.phase, DragPhase::Dropped);
            Some(DropEffect::Copy)
        })
        .unwrap();
        ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
            .unwrap();
        over(&mut ui, 65.0);
        release(&mut ui, 65.0);
        let reason = if layout_change {
            DragCancelReason::GeometryUnavailable
        } else {
            DragCancelReason::Rejected
        };
        assert!(
            ui.take_drag_events()
                .iter()
                .any(|event| event.phase == DragPhase::Cancelled(reason))
        );
        assert_eq!(ui.captured(), None);
    }
}

#[test]
fn target_disposal_delivers_terminal_callback_before_dropping_registration() {
    for atomic in [false, true] {
        let (mut ui, source, target) = setup();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let log = observed.clone();
        ui.on_drop_target(target, move |event, _| {
            log.borrow_mut().push(event.phase);
            Some(DropEffect::Copy)
        })
        .unwrap();
        ui.begin_drag(source, payload(), DropEffects::COPY, Point::new(35.0, 10.0))
            .unwrap();
        over(&mut ui, 65.0);
        observed.borrow_mut().clear();
        assert!(
            ui.apply_batch(&[Mutation::Remove(target), Mutation::Focus(Some(target))])
                .is_err()
        );
        assert!(observed.borrow().is_empty());
        if atomic {
            ui.apply_batch(&[Mutation::Remove(target)]).unwrap();
        } else {
            ui.apply(Mutation::Remove(target)).unwrap();
        }
        assert_eq!(
            *observed.borrow(),
            vec![DragPhase::Cancelled(DragCancelReason::TargetUnavailable)]
        );
        assert!(ui.drag_session().is_none());
        assert!(!ui.drop_handlers.contains_key(&target));
    }
}
