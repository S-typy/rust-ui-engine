use super::*;
use rust_desktop_ui_core::Modifiers;

fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 240.0)).unwrap();
}
fn zoom(ui: &mut Controls) -> WidgetId {
    ui.zoom_control(
        ui.root(),
        ZoomControlSettings::default(),
        LayoutStyle::default(),
    )
    .unwrap()
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn pointer(ui: &mut Controls, position: Point, down: bool) {
    ui.dispatch(if down {
        InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    } else {
        InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    })
    .unwrap();
}
fn position(ui: &Controls, id: WidgetId, x: f32) -> Point {
    let bounds = ui.runtime.bounds(id).unwrap();
    Point::new(bounds.x + x, bounds.y + bounds.height / 2.0)
}

#[test]
fn status_regions_share_space_and_clip_without_overlap_at_narrow_widths() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
    let left = ui
        .label(
            bar.left,
            "A lengthy state message",
            LayoutStyle {
                width: Length::Px(300.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    let right = ui
        .button(
            bar.right,
            "Command",
            None,
            LayoutStyle {
                width: Length::Px(160.0),
                height: Length::Px(24.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    for width in [640.0, 220.0, 80.0] {
        ui.update(Size::new(width, 180.0)).unwrap();
        let bounds = ui.runtime.bounds(bar.container).unwrap();
        let a = ui.runtime.bounds(bar.left).unwrap();
        let b = ui.runtime.bounds(bar.right).unwrap();
        assert!(a.x + a.width <= b.x + 0.01, "{width}: {a:?} {b:?}");
        assert!(b.x + b.width <= bounds.x + bounds.width + 0.01);
        assert!((bounds.height - 32.0).abs() < 0.01);
        if let Some(clip) = ui.runtime.clip(left) {
            assert!(clip.x + clip.width <= b.x + 0.01);
        }
        let _ = ui.runtime.clip(right);
    }
}

#[test]
fn status_regions_are_owned_and_subtree_removal_cleans_numeric_state() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let bar = ui
        .ribbon_status_bar(ui.root(), LayoutStyle::default())
        .unwrap();
    let progress = ui
        .progress_bar(
            bar.left,
            ProgressBarSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let zoom = ui
        .zoom_control(
            bar.right,
            ZoomControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(bar.container).unwrap().height, 36.0);
    assert!(ui.remove(bar.left).is_err());
    assert!(ui.remove(bar.right).is_err());
    let point = position(&ui, zoom, 60.0);
    pointer(&mut ui, point, true);
    ui.remove(bar.container).unwrap();
    assert!(ui.progress_settings(progress).is_none());
    assert!(ui.zoom_settings(zoom).is_none());

    assert_eq!(ui.runtime.captured(), None);
}

#[test]
fn ranges_reject_invalid_values_before_allocating_and_setters_are_atomic() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let before = ui.runtime.tree().preorder().len();
    for settings in [
        ProgressBarSettings {
            minimum: 1.0,
            maximum: 1.0,
            value: 1.0,
        },
        ProgressBarSettings {
            minimum: -f64::MAX,
            maximum: f64::MAX,
            value: 0.0,
        },
        ProgressBarSettings {
            value: f64::NAN,
            ..Default::default()
        },
        ProgressBarSettings {
            value: 101.0,
            ..Default::default()
        },
    ] {
        assert!(
            ui.progress_bar(ui.root(), settings, LayoutStyle::default())
                .is_err()
        );
    }
    assert!(
        ui.zoom_control(
            ui.root(),
            ZoomControlSettings {
                step: 0.0,
                ..Default::default()
            },
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder().len(), before);
    let progress = ui
        .progress_bar(
            ui.root(),
            ProgressBarSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let zoom = zoom(&mut ui);
    for value in [f64::NAN, f64::INFINITY, -1.0, 401.0] {
        assert!(ui.set_progress_value(progress, value).is_err());
        assert!(ui.set_zoom_value(zoom, value).is_err());
    }
    assert_eq!(ui.progress_value(progress), Some(0.0));
    assert_eq!(ui.zoom_value(zoom), Some(100.0));
    ui.set_progress_value(progress, 100.0).unwrap();
    assert_eq!(ui.progress_value(progress), Some(100.0));
}

#[test]
fn zoom_keyboard_clamps_and_home_end_keep_focus() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = zoom(&mut ui);
    frame(&mut ui);
    ui.perform_action(id, SemanticAction::Focus, None).unwrap();
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.zoom_value(id), Some(110.0));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.zoom_value(id), Some(100.0));
    key(&mut ui, Key::Home);
    key(&mut ui, Key::ArrowLeft);
    assert_eq!(ui.zoom_value(id), Some(25.0));
    key(&mut ui, Key::End);
    key(&mut ui, Key::ArrowUp);
    assert_eq!(ui.zoom_value(id), Some(400.0));
    assert_eq!(ui.runtime.focused(), Some(id));
}

#[test]
fn zoom_buttons_activate_only_on_matching_release_and_track_drags() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = zoom(&mut ui);
    frame(&mut ui);
    let plus = position(&ui, id, 154.0);
    let minus = position(&ui, id, 12.0);
    pointer(&mut ui, plus, true);
    assert_eq!(ui.zoom_value(id), Some(100.0));
    pointer(&mut ui, minus, false);
    assert_eq!(ui.zoom_value(id), Some(100.0));
    pointer(&mut ui, plus, true);
    pointer(&mut ui, plus, false);
    assert_eq!(ui.zoom_value(id), Some(110.0));
    pointer(&mut ui, minus, true);
    pointer(&mut ui, minus, false);
    assert_eq!(ui.zoom_value(id), Some(100.0));
    let track = position(&ui, id, 60.0);
    pointer(&mut ui, track, true);
    assert_eq!(ui.runtime.captured(), Some(id));
    let far = position(&ui, id, 400.0);
    ui.dispatch(InputEvent::PointerMoved { position: far })
        .unwrap();
    assert_eq!(ui.zoom_value(id), Some(400.0));
    pointer(&mut ui, far, false);
    assert_eq!(ui.runtime.captured(), None);
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn zoom_cancel_disable_hide_blur_and_model_update_prevent_late_releases() {
    for scenario in 0..5 {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = zoom(&mut ui);
        frame(&mut ui);
        let plus = position(&ui, id, 154.0);
        pointer(&mut ui, plus, true);
        assert_eq!(ui.runtime.captured(), Some(id));
        match scenario {
            0 => ui.cancel_pointer_interaction().unwrap(),
            1 => ui.set_enabled(id, false).unwrap(),
            2 => {
                ui.edit_props(id, |p| p.visible = false).unwrap();
                frame(&mut ui);
            }
            3 => ui.dispatch(InputEvent::WindowFocused(false)).unwrap(),
            4 => ui.set_zoom_value(id, 180.0).unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(ui.runtime.captured(), None);

        pointer(&mut ui, plus, false);
        assert_eq!(
            ui.zoom_value(id),
            Some(if scenario == 4 { 180.0 } else { 100.0 })
        );
    }
}

#[test]
fn status_numeric_semantics_distinguish_readonly_progress_from_slider() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
    let progress = ui
        .progress_bar(
            bar.left,
            ProgressBarSettings {
                value: 45.0,
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    let slider = ui
        .zoom_control(
            bar.right,
            ZoomControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    ui.set_automation_name(slider, "Zoom").unwrap();
    frame(&mut ui);
    let nodes = ui.semantics();
    let progress_node = nodes.iter().find(|n| n.id == progress).unwrap();
    assert_eq!(progress_node.role, Role::ProgressBar);
    assert_eq!(progress_node.value.as_deref(), Some("45"));
    assert!(progress_node.actions.is_empty());
    let node = nodes.iter().find(|n| n.id == slider).unwrap();
    assert_eq!(node.role, Role::Slider);
    assert!(node.actions.contains(&SemanticAction::SetValue));
    assert!(node.actions.contains(&SemanticAction::Focus));
    assert!(!node.actions.contains(&SemanticAction::Activate));
    assert_eq!(node.label, "Zoom");
    assert_eq!(
        nodes.iter().find(|n| n.id == bar.container).unwrap().role,
        Role::StatusBar
    );
    ui.perform_action(slider, SemanticAction::SetValue, Some("125"))
        .unwrap();
    assert_eq!(ui.zoom_value(slider), Some(125.0));
    for invalid in ["NaN", "inf", "500", "not a number"] {
        ui.perform_action(slider, SemanticAction::SetValue, Some(invalid))
            .unwrap();
        assert_eq!(ui.zoom_value(slider), Some(125.0));
    }
    ui.set_enabled(slider, false).unwrap();
    ui.perform_action(slider, SemanticAction::SetValue, Some("150"))
        .unwrap();
    assert_eq!(ui.zoom_value(slider), Some(125.0));
}

#[test]
fn zoom_ignores_pointer_press_after_window_blur() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = zoom(&mut ui);
    frame(&mut ui);
    let track = position(&ui, id, 90.0);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    pointer(&mut ui, track, true);
    pointer(&mut ui, track, false);
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.runtime.focused(), None);
    assert_eq!(ui.zoom_value(id), Some(100.0));
}

#[test]
fn tiny_status_indicators_clip_every_primitive_to_their_own_bounds() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let slider = zoom(&mut ui);
    let progress = ui
        .progress_bar(
            ui.root(),
            ProgressBarSettings {
                value: 50.0,
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
    for id in [slider, progress, bar.container] {
        for size in [Size::new(2.0, 20.0), Size::new(80.0, 1.0), Size::ZERO] {
            let bounds = Rect::new(50.0, 50.0, size.width, size.height);
            ui.scene = Scene::default();
            ui.paint_status_control(id, bounds, Rect::new(0.0, 0.0, 640.0, 240.0))
                .unwrap();
            let contained = |r: Rect| {
                assert!(r.x >= bounds.x && r.y >= bounds.y);
                assert!(r.x + r.width <= bounds.x + bounds.width + 0.001);
                assert!(r.y + r.height <= bounds.y + bounds.height + 0.001);
            };
            for r in &ui.scene.rectangles {
                contained(r.bounds);
            }
            for r in &ui.scene.rounded_rectangles {
                contained(r.clip);
            }
            for path in &ui.scene.stroke_paths {
                contained(path.clip);
            }
            for text in &ui.scene.texts {
                contained(text.clip);
            }
        }
    }
}

#[test]
fn zoom_consumed_input_routes_once_updates_hover_and_honors_preview_cancellation() {
    use rust_desktop_ui_core::EventPhase;
    use std::{cell::Cell, rc::Rc};
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = zoom(&mut ui);
    let neighbor = ui
        .button(
            ui.root(),
            "Neighbor",
            None,
            LayoutStyle {
                width: Length::Px(210.0),
                height: Length::Px(28.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    let deliveries = Rc::new(Cell::new(0));
    let seen = Rc::clone(&deliveries);
    ui.runtime
        .on(ui.root(), move |event, _| {
            if event.phase == EventPhase::Capture {
                seen.set(seen.get() + 1);
            }
        })
        .unwrap();
    let track = position(&ui, id, 60.0);
    let other = position(&ui, neighbor, 60.0);
    pointer(&mut ui, track, true);
    ui.dispatch(InputEvent::PointerMoved { position: other })
        .unwrap();
    pointer(&mut ui, other, false);
    assert_eq!(deliveries.get(), 3);
    assert_eq!(ui.runtime.hovered(), Some(neighbor));
    assert_eq!(ui.runtime.captured(), None);
    ui.runtime
        .on(ui.root(), |event, context| {
            if event.phase == EventPhase::Capture {
                context.prevent_default();
            }
        })
        .unwrap();
    ui.runtime.apply(Mutation::Focus(None)).unwrap();
    ui.set_zoom_value(id, 100.0).unwrap();
    pointer(&mut ui, track, true);
    assert_eq!(ui.runtime.focused(), None);
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.zoom_value(id), Some(100.0));
    ui.perform_action(id, SemanticAction::Focus, None).unwrap();
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.zoom_value(id), Some(100.0));
}

#[test]
fn theme_switch_and_resize_preserve_values_and_widgets() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let bar = ui
        .ribbon_status_bar(ui.root(), LayoutStyle::default())
        .unwrap();
    let progress = ui
        .progress_bar(
            bar.left,
            ProgressBarSettings {
                value: 65.0,
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    let slider = ui
        .zoom_control(
            bar.right,
            ZoomControlSettings {
                value: 175.0,
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    let light = ui.scene().clone();
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    assert_ne!(light, *ui.scene());
    ui.update(Size::new(180.0, 160.0)).unwrap();
    assert_eq!(ui.progress_value(progress), Some(65.0));
    assert_eq!(ui.zoom_value(slider), Some(175.0));
    assert!(ui.runtime.tree().node(bar.left).is_some());
}
