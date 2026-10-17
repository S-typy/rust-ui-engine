use super::*;
use rust_desktop_ui_core::Modifiers;
fn bitmap() -> Bitmap {
    Bitmap::new_rgba8(
        3,
        2,
        vec![
            12, 34, 56, 0, 20, 40, 60, 128, 2, 4, 6, 255, 3, 6, 9, 10, 4, 8, 12, 20, 5, 10, 15, 30,
        ],
    )
    .unwrap()
}
fn fixture() -> (Controls, ImageEditorParts) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let p = ui
        .image_editor(
            ui.root(),
            Some(bitmap()),
            Default::default(),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    (ui, p)
}
#[test]
fn editing_is_draft_until_explicit_apply_and_cancel_preserves_pixels() {
    let (mut ui, p) = fixture();
    let accepted = ui.image_editor_value(p.container).unwrap().unwrap().clone();
    ui.rotate_image_editor(p.container, QuarterTurn::Clockwise)
        .unwrap();
    assert_eq!(ui.image_editor_value(p.container), Some(Some(&accepted)));
    assert_eq!(
        ui.image_editor_draft(p.container).unwrap().unwrap().width(),
        2
    );
    assert!(ui.take_image_editor_events().is_empty());
    ui.cancel_image_editor(p.container).unwrap();
    assert_eq!(ui.image_editor_draft(p.container), Some(Some(&accepted)));
    ui.set_image_editor_draft(p.container, None).unwrap();
    assert!(ui.apply_image_editor(p.container).unwrap());
    assert!(matches!(
        ui.take_image_editor_events().as_slice(),
        [ImageEditorEvent::Applied { value: None, .. }]
    ));
    assert_eq!(ui.image_editor_value(p.container), Some(None));
}
#[test]
fn validator_and_external_conflict_preserve_session_and_required_parts() {
    let (mut ui, p) = fixture();
    let ids = ui.runtime.tree().live_ids();
    ui.rotate_image_editor(p.container, QuarterTurn::Clockwise)
        .unwrap();
    let draft = ui.image_editor_draft(p.container).unwrap().cloned();
    ui.set_image_editor_validator(
        p.container,
        Some(ImageEditorValidator::new(|_| Err("Too large".into()))),
    )
    .unwrap();
    assert!(ui.apply_image_editor(p.container).is_err());
    assert_eq!(ui.image_editor_draft(p.container).unwrap(), draft.as_ref());
    assert_eq!(ui.runtime.tree().live_ids(), ids);
    ui.set_image_editor_validator(p.container, None).unwrap();
    assert!(ui.set_image_editor_value(p.container, None, false).is_err());
    assert!(ui.remove(p.viewport).is_err());
    ui.runtime
        .apply(Mutation::Reparent {
            node: p.status,
            parent: ui.root(),
        })
        .unwrap();
    assert!(ui.set_image_editor_draft(p.container, None).is_err());
    assert_eq!(ui.image_editor_draft(p.container).unwrap(), draft.as_ref());
}
#[test]
fn load_tokens_are_owner_bound_cancelled_on_edit_hide_remove_and_stale_before_payload() {
    let (mut ui, p) = fixture();
    let first = ui.begin_image_editor_load(p.container).unwrap();
    let second = ui.begin_image_editor_load(p.container).unwrap();
    assert!(first.is_cancelled());
    assert!(
        !ui.complete_image_editor_load(&first, Err("x".repeat(10000)))
            .unwrap()
    );
    ui.rotate_image_editor(p.container, QuarterTurn::Clockwise)
        .unwrap();
    assert!(second.is_cancelled());
    let third = ui.begin_image_editor_load(p.container).unwrap();
    ui.edit_props(p.container, |p| p.visible = false).unwrap();
    assert!(!ui.complete_image_editor_load(&third, Ok(bitmap())).unwrap());
    assert!(third.is_cancelled());
    ui.edit_props(p.container, |p| p.visible = true).unwrap();
    let fourth = ui.begin_image_editor_load(p.container).unwrap();
    ui.remove(p.container).unwrap();
    assert!(fourth.is_cancelled());
    assert!(
        !ui.complete_image_editor_load(&fourth, Ok(bitmap()))
            .unwrap()
    );
}
#[test]
fn malformed_load_and_read_only_leave_accepted_and_draft_intact() {
    let (mut ui, p) = fixture();
    let value = ui.image_editor_value(p.container).unwrap().cloned();
    let token = ui.begin_image_editor_load(p.container).unwrap();
    ui.complete_image_editor_load(&token, Err("Invalid PNG".into()))
        .unwrap();
    assert_eq!(ui.image_editor_value(p.container).unwrap(), value.as_ref());
    assert_eq!(ui.image_editor_draft(p.container).unwrap(), value.as_ref());
    assert_eq!(ui.image_editor_error(p.container), Some("Invalid PNG"));
    ui.set_image_editor_read_only(p.container, true).unwrap();
    assert!(
        ui.rotate_image_editor(p.container, QuarterTurn::Clockwise)
            .is_err()
    );
    assert!(ui.begin_image_editor_load(p.container).is_err());
    ui.set_image_editor_viewport(
        p.container,
        ImageViewport {
            fit: false,
            zoom: 2.0,
            pan: Point::new(10.0, 20.0),
        },
    )
    .unwrap();
}
#[test]
fn actual_layout_and_render_mapping_remain_visible_at_narrow_sizes() {
    let (mut ui, p) = fixture();
    for size in [Size::new(800.0, 500.0), Size::new(360.0, 420.0)] {
        ui.update(size).unwrap();
        let b = ui.runtime.bounds(p.viewport).unwrap();
        let clip = ui.runtime.clip(p.viewport).unwrap();
        assert!(b.height > 200.0, "{b:?}");
        assert!(b.intersection(clip).unwrap().height >= b.height - 1.0);
        let (mapping, _) = ui.image_editor_mapping(p.container).unwrap();
        assert!(mapping.width <= b.width + 0.1 && mapping.height <= b.height + 0.1);
        assert!(ui.scene().images.iter().any(|i| i.bounds == mapping));
        for (button, _) in &ui.image_editors.entries[&p.container].buttons {
            let r = ui.runtime.bounds(*button).unwrap();
            let c = ui.runtime.clip(*button).unwrap();
            assert!(
                r.width > 75.0
                    && r.height > 30.0
                    && r.intersection(c)
                        .is_some_and(|visible| visible.height >= r.height - 0.1),
                "button {button:?} {r:?} clip={c:?} size={size:?} owner={:?} toolbar={:?}",
                ui.runtime.bounds(p.container),
                ui.runtime
                    .bounds(ui.image_editors.entries[&p.container].parents[0].0)
            );
        }
    }
}
#[test]
fn pointer_pan_capture_cancel_and_readonly_viewer_navigation() {
    let (mut ui, p) = fixture();
    ui.set_image_editor_read_only(p.container, true).unwrap();
    let b = ui.runtime.bounds(p.viewport).unwrap();
    let at = Point::new(b.x + 30.0, b.y + 30.0);
    ui.dispatch(InputEvent::PointerDown {
        position: at,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(at.x + 25.0, at.y + 35.0),
    })
    .unwrap();
    assert_eq!(
        ui.image_editor_viewport(p.container).unwrap().pan,
        Point::new(25.0, 35.0)
    );
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(
        ui.image_editor_viewport(p.container).unwrap().pan,
        Point::default()
    );
    assert!(!ui.image_editor_dirty(p.container).unwrap());
}
#[test]
fn event_backpressure_rejects_apply_before_accepted_changes() {
    let (mut ui, p) = fixture();
    let old = ui.image_editor_value(p.container).unwrap().cloned();
    ui.set_image_editor_draft(p.container, None).unwrap();
    for _ in 0..256 {
        ui.image_editors
            .events
            .push_back(ImageEditorEvent::Applied {
                owner: p.container,
                value: None,
            });
    }
    assert!(ui.apply_image_editor(p.container).is_err());
    assert_eq!(ui.image_editor_value(p.container).unwrap(), old.as_ref());
    assert!(ui.image_editor_dirty(p.container).unwrap());
}

#[test]
fn native_capture_loss_and_invalid_recipe_cancel_owned_gesture_immediately() {
    let (mut ui, p) = fixture();
    let b = ui.runtime.bounds(p.viewport).unwrap();
    let at = Point::new(b.x + 30.0, b.y + 30.0);
    let begin = |ui: &mut Controls| {
        ui.dispatch(InputEvent::PointerDown {
            position: at,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(at.x + 25.0, at.y + 35.0),
        })
        .unwrap();
    };
    begin(&mut ui);
    ui.cancel_pointer_interaction().unwrap();
    assert_eq!(
        ui.image_editor_viewport(p.container).unwrap().pan,
        Point::default()
    );
    assert!(ui.image_editors.drag.is_none());
    assert_eq!(ui.runtime.captured(), None);
    begin(&mut ui);
    let token = ui.begin_image_editor_load(p.container).unwrap();
    ui.runtime
        .apply(Mutation::Reparent {
            node: p.status,
            parent: ui.root(),
        })
        .unwrap();
    assert!(ui.update(Size::new(800.0, 500.0)).is_err());
    assert!(token.is_cancelled());
    assert!(ui.image_editors.drag.is_none());
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(
        ui.image_editor_viewport(p.container).unwrap().pan,
        Point::default()
    );
}
#[test]
fn fit_to_manual_navigation_clamps_large_image_scale_and_dialog_blur_keeps_load() {
    let (mut ui, p) = fixture();
    let large = Bitmap::new_rgba8(8192, 1, vec![0; 8192 * 4]).unwrap();
    ui.set_image_editor_value(p.container, Some(large), true)
        .unwrap();
    ui.update(Size::new(360.0, 420.0)).unwrap();
    assert!(
        ui.image_editor_mapping(p.container).unwrap().1 < 0.05,
        "mapping={:?},owner={:?}",
        ui.image_editor_mapping(p.container),
        ui.runtime.bounds(p.container)
    );
    let token = ui.begin_image_editor_load(p.container).unwrap();
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(!token.is_cancelled());
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(p.viewport))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert!(ui.image_editor_viewport(p.container).unwrap().zoom >= 0.05);
    ui.complete_image_editor_load(&token, Ok(bitmap())).unwrap();
    assert_eq!(
        ui.image_editor_draft(p.container)
            .flatten()
            .unwrap()
            .width(),
        3
    );
}

#[test]
fn successful_replacement_during_drag_cancels_only_its_owner_and_restores_entire_view() {
    let (mut ui, p) = fixture();
    let original = ui.image_editor_viewport(p.container).unwrap();
    let b = ui.runtime.bounds(p.viewport).unwrap();
    let at = Point::new(b.x + 20.0, b.y + 20.0);
    ui.dispatch(InputEvent::PointerDown {
        position: at,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(at.x + 30.0, at.y + 15.0),
    })
    .unwrap();
    ui.set_image_editor_value(p.container, None, true).unwrap();
    assert!(ui.image_editors.drag.is_none());
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.image_editor_viewport(p.container), Some(original));
    assert_eq!(ui.image_editor_value(p.container), Some(None));
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(at.x + 60.0, at.y + 30.0),
    })
    .unwrap();
    assert_eq!(ui.image_editor_viewport(p.container), Some(original));
}

#[test]
fn readonly_viewer_retains_only_view_commands_and_cannot_become_editor() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let p = ui
        .image_viewer(
            ui.root(),
            Some(bitmap()),
            LayoutStyle {
                width: Length::Px(320.0),
                height: Length::Px(340.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert!(ui.set_image_editor_read_only(p.container, false).is_err());
    assert!(!ui.runtime.tree().node(p.load).unwrap().props.visible);
    assert!(!ui.runtime.tree().node(p.apply).unwrap().props.visible);
    assert!(ui.runtime.tree().node(p.save).unwrap().props.visible);
    assert!(ui.runtime.bounds(p.viewport).unwrap().height > 220.0);
}

#[test]
fn retained_image_editor_is_idle_without_state_or_geometry_changes() {
    let (mut ui, _) = fixture();
    let frame = ui.update(Size::new(800.0, 500.0)).unwrap();
    assert_eq!(frame.layout_passes, 0);
    assert_eq!(frame.paint_passes, 0);
}

#[test]
fn command_availability_uses_authoritative_control_state() {
    let (mut ui, p) = fixture();
    assert!(!ui.is_enabled(p.apply));
    let undo = ui.image_editors.entries[&p.container]
        .buttons
        .iter()
        .find(|(_, a)| matches!(a, Action::Undo))
        .unwrap()
        .0;
    assert!(!ui.is_enabled(undo));
    assert!(ui.is_enabled(p.load));
    ui.rotate_image_editor(p.container, QuarterTurn::Clockwise)
        .unwrap();
    assert!(ui.is_enabled(p.apply));
    assert!(ui.is_enabled(undo));
    ui.set_image_editor_read_only(p.container, true).unwrap();
    assert!(!ui.is_enabled(p.apply));
    assert!(!ui.is_enabled(p.load));
    assert!(!ui.is_enabled(undo));
    ui.update(Size::new(800.0, 500.0)).unwrap();
    assert!(!ui.is_enabled(p.apply));
}

#[test]
fn pan_and_same_size_bitmap_replacement_invalidate_the_rendered_scene() {
    let (mut ui, p) = fixture();
    ui.set_image_editor_viewport(
        p.container,
        ImageViewport {
            zoom: 1.0,
            pan: Point::default(),
            fit: false,
        },
    )
    .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    let before = ui.scene().images[0].bounds;
    ui.set_image_editor_viewport(
        p.container,
        ImageViewport {
            zoom: 1.0,
            pan: Point::new(12.0, 7.0),
            fit: false,
        },
    )
    .unwrap();
    let frame = ui.update(Size::new(800.0, 500.0)).unwrap();
    assert!(frame.paint_passes > 0);
    assert_ne!(ui.scene().images[0].bounds, before);
    let replacement = Bitmap::new_rgba8(3, 2, vec![200; 24]).unwrap();
    let id = replacement.id();
    ui.set_image_editor_value(p.container, Some(replacement), false)
        .unwrap();
    let frame = ui.update(Size::new(800.0, 500.0)).unwrap();
    assert!(frame.paint_passes > 0);
    assert_eq!(ui.scene().images[0].bitmap.id(), id);
}

#[test]
fn image_chart_integration_keeps_owner_capture_and_rollbacks_separate() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    ui.edit_props(root, |p| {
        p.style.kind = LayoutKind::Grid;
        p.style.grid_columns = vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)];
        p.style.grid_rows = vec![GridTrack::Fr(1.0)];
    })
    .unwrap();
    let style = LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Percent(1.0),
        ..Default::default()
    };
    let image = ui
        .image_editor(root, Some(bitmap()), Default::default(), style.clone())
        .unwrap();
    let model = ChartModel::new(
        vec![ChartSeries {
            id: 1,
            label: "Series".into(),
            points: vec![
                ChartPoint {
                    id: 1,
                    x: 0.0,
                    y: Some(0.0),
                },
                ChartPoint {
                    id: 2,
                    x: 1.0,
                    y: Some(1.0),
                },
            ],
        }],
        ChartSettings::default(),
    )
    .unwrap();
    let chart = ui.chart(root, model.clone(), style).unwrap();
    ui.update(Size::new(1400.0, 650.0)).unwrap();
    let mark = ui.chart_geometry(chart).unwrap().unwrap().points[0];
    let viewport = ui.runtime.bounds(image.viewport).unwrap();
    let center = Point::new(
        viewport.x + viewport.width / 2.0,
        viewport.y + viewport.height / 2.0,
    );
    let original = ui.image_editor_viewport(image.container).unwrap();
    let down = |p| InputEvent::PointerDown {
        position: p,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    };
    let up = |p| InputEvent::PointerUp {
        position: p,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    };
    ui.dispatch(down(mark.position)).unwrap();
    assert_eq!(ui.runtime.captured(), Some(chart));
    ui.set_image_editor_value(image.container, Some(bitmap()), true)
        .unwrap();
    assert_eq!(ui.runtime.captured(), Some(chart));
    ui.cancel_image_gesture().unwrap();
    assert_eq!(ui.runtime.captured(), Some(chart));
    ui.dispatch(InputEvent::PointerMoved { position: center })
        .unwrap();
    ui.dispatch(up(center)).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.image_editor_viewport(image.container), Some(original));
    assert_eq!(ui.chart_model(chart).unwrap().selected(), None);
    ui.dispatch(down(center)).unwrap();
    assert_eq!(ui.runtime.captured(), Some(image.viewport));
    ui.set_chart_model(chart, model).unwrap();
    ui.cancel_chart_interaction().unwrap();
    assert_eq!(ui.runtime.captured(), Some(image.viewport));
    ui.dispatch(InputEvent::PointerMoved {
        position: mark.position,
    })
    .unwrap();
    ui.dispatch(up(mark.position)).unwrap();
    assert_eq!(ui.image_editor_viewport(image.container), Some(original));
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.chart_model(chart).unwrap().selected(), None);
    assert!(ui.take_chart_selection_changes().is_empty());
}
#[test]
fn image_input_background_refuses_gesture_and_chart_focus_is_unaffected() {
    let (mut ui, p) = fixture();
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    let b = ui.runtime.bounds(p.viewport).unwrap();
    let before = ui.image_editor_viewport(p.container);
    assert!(
        !ui.image_editor_before_input(&InputEvent::PointerDown {
            position: Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0),
            button: PointerButton::Primary,
            modifiers: Default::default()
        })
        .unwrap()
    );
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.image_editor_viewport(p.container), before);
}
#[test]
fn image_input_outside_menu_closes_before_own_capture() {
    let (mut ui, p) = fixture();
    ui.commands_mut().register("MenuAction", "Menu action");
    ui.popup_menu(p.load, &[MenuItem::new("Menu action", "MenuAction")])
        .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    let b = ui.runtime.bounds(p.viewport).unwrap();
    let point = Point::new(b.x + b.width * 0.75, b.y + b.height * 0.75);
    assert_eq!(ui.runtime.hit_test(point), Some(p.viewport));
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.captured(), Some(p.viewport));
    ui.cancel_pointer_interaction().unwrap();
}
