use super::*;
use crate::{MenuItem, Theme};
use rust_desktop_ui_core::{
    DrawCommand, Key, LayoutKind, LayoutStyle, Length, Modifiers, Mutation, Point, PointerButton,
};

fn layout(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}

fn fixture() -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .button(ui.root(), "Button", None, layout(100.0, 36.0))
        .unwrap();
    ui.set_tooltip(id, "Helpful tooltip").unwrap();
    ui.update(Size::new(400.0, 260.0)).unwrap();
    (ui, id)
}

fn center(ui: &Controls, id: WidgetId) -> Point {
    let bounds = ui.runtime.bounds(id).unwrap();
    Point::new(
        bounds.x + bounds.width * 0.5,
        bounds.y + bounds.height * 0.5,
    )
}

fn hover(ui: &mut Controls, id: WidgetId) -> Instant {
    ui.dispatch(InputEvent::PointerMoved {
        position: center(ui, id),
    })
    .unwrap();
    ui.tooltip_deadline().unwrap()
}

fn show(ui: &mut Controls, id: WidgetId) {
    let deadline = hover(ui, id);
    assert!(ui.tooltip_tick(deadline).unwrap());
    assert!(ui.tooltip_bounds().is_some());
}

#[test]
fn hover_deadline_is_stable_overlay_is_last_and_never_takes_focus_or_hit_tests() {
    let (mut ui, id) = fixture();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    let count = ui.runtime.tree().preorder().len();
    let deadline = hover(&mut ui, id);
    assert!(
        !ui.tooltip_tick(deadline - Duration::from_millis(1))
            .unwrap()
    );
    assert_eq!(ui.tooltip_bounds(), None);
    ui.dispatch(InputEvent::PointerMoved {
        position: center(&ui, id),
    })
    .unwrap();
    assert_eq!(ui.tooltip_deadline(), Some(deadline));
    assert!(ui.tooltip_tick(deadline).unwrap());
    assert_eq!(ui.tooltip_deadline(), None);
    assert!(!ui.tooltip_tick(deadline + Duration::from_secs(3)).unwrap());
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(ui.runtime.tree().preorder().len(), count);
    assert_eq!(ui.runtime.hovered(), Some(id));
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert_eq!(semantic.tooltip.as_deref(), Some("Helpful tooltip"));
    let DrawCommand::Text(index) = *ui.scene().commands().last().unwrap() else {
        panic!("tooltip text must be the last overlay draw");
    };
    assert_eq!(ui.scene().texts[index].text, "Helpful tooltip");
    let stats = ui.update(Size::new(400.0, 260.0)).unwrap();
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(stats.paint_passes, 0);
    ui.dismiss_tooltip();
    assert_eq!(ui.tooltip_bounds(), None);
    assert_eq!(ui.tooltip_deadline(), None);
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(ui.runtime.hovered(), Some(id));
    hover(&mut ui, id);
    ui.dismiss_tooltip();
    assert_eq!(ui.tooltip_deadline(), None);
    assert_eq!(ui.runtime.focused(), Some(id));
}

#[test]
fn tooltip_text_limit_is_utf8_bytes_and_rejection_preserves_visible_state() {
    let (mut ui, id) = fixture();
    let at_limit = "Ж".repeat(2048);
    ui.set_tooltip(id, at_limit.clone()).unwrap();
    show(&mut ui, id);
    let bounds = ui.tooltip_bounds();
    assert!(ui.set_tooltip(id, format!("{at_limit}a")).is_err());
    assert_eq!(ui.tooltip(id), Some(at_limit.as_str()));
    assert_eq!(ui.tooltip_bounds(), bounds);
    ui.set_tooltip(id, " \n\t ").unwrap();
    assert_eq!(ui.tooltip(id), None);
    assert_eq!(ui.tooltip_deadline(), None);
    assert_eq!(ui.tooltip_bounds(), None);
    ui.remove(id).unwrap();
    assert!(ui.set_tooltip(id, "stale").is_err());
    assert_eq!(ui.tooltip(id), None);
}

#[test]
fn input_dismisses_pending_and_visible_tooltips_without_rearming_on_idle() {
    for visible in [false, true] {
        for event in [
            InputEvent::PointerLeft,
            InputEvent::WindowFocused(false),
            InputEvent::KeyDown {
                key: Key::Escape,
                modifiers: Modifiers::default(),
                repeat: false,
            },
            InputEvent::KeyUp {
                key: Key::Other,
                modifiers: Modifiers::default(),
            },
            InputEvent::Wheel {
                position: Point::new(20.0, 20.0),
                delta: Point::new(0.0, 1.0),
                modifiers: Modifiers::default(),
            },
            InputEvent::PointerDown {
                position: Point::new(20.0, 20.0),
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
        ] {
            let (mut ui, id) = fixture();
            let deadline = hover(&mut ui, id);
            if visible {
                ui.tooltip_tick(deadline).unwrap();
            }
            ui.dispatch(event.clone()).unwrap();
            assert_eq!(ui.tooltip_bounds(), None, "{event:?}");
            assert_eq!(ui.tooltip_deadline(), None, "{event:?}");
            ui.tooltip_tick(deadline + Duration::from_secs(1)).unwrap();
            assert_eq!(ui.tooltip_bounds(), None, "{event:?}");
        }
    }
}

#[test]
fn unavailable_removed_or_captured_anchor_cancels_and_cannot_resurrect() {
    for unavailable in 0..4 {
        let (mut ui, id) = fixture();
        show(&mut ui, id);
        match unavailable {
            0 => ui.set_enabled(id, false).unwrap(),
            1 => {
                let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
                props.visible = false;
                ui.runtime
                    .apply(Mutation::SetProps { node: id, props })
                    .unwrap();
            }
            2 => ui.runtime.apply(Mutation::Remove(id)).unwrap(),
            _ => ui
                .runtime
                .apply(Mutation::CapturePointer(Some(id)))
                .unwrap(),
        }
        ui.update(Size::new(400.0, 260.0)).unwrap();
        assert_eq!(ui.tooltip_bounds(), None);
        assert_eq!(ui.tooltip_deadline(), None);
        assert!(
            !ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Helpful tooltip")
        );
    }
}

#[test]
fn opening_popup_hides_tooltip_and_popup_hover_cannot_rearm_it() {
    let (mut ui, id) = fixture();
    ui.commands_mut().register("action", "Action");
    show(&mut ui, id);
    ui.popup_menu(id, &[MenuItem::new("Action", "action")])
        .unwrap();
    assert_eq!(ui.tooltip_bounds(), None);
    ui.dispatch(InputEvent::PointerMoved {
        position: center(&ui, id),
    })
    .unwrap();
    assert_eq!(ui.tooltip_deadline(), None);
    ui.close_popup().unwrap();
    ui.update(Size::new(400.0, 260.0)).unwrap();
    assert_eq!(ui.tooltip_deadline(), None);
}

#[test]
fn tooltip_uses_theme_palette_and_clamps_wrapped_text_to_small_viewport() {
    for theme in [
        Theme::Light,
        Theme::Dark,
        Theme::Compact,
        Theme::FluentLight,
        Theme::FluentDark,
    ] {
        let (mut ui, id) = fixture();
        ui.set_theme(theme).unwrap();
        ui.set_tooltip(
            id,
            "A long tooltip with multiple words that wraps into several lines",
        )
        .unwrap();
        ui.update(Size::new(150.0, 80.0)).unwrap();
        show(&mut ui, id);
        let bounds = ui.tooltip_bounds().unwrap();
        let viewport = Rect::new(0.0, 0.0, 150.0, 80.0);
        assert_eq!(bounds.intersection(viewport), Some(bounds));
        assert!(
            ui.scene()
                .rounded_rectangles
                .iter()
                .any(|rect| rect.bounds == bounds
                    && rect.stroke_width.is_none()
                    && rect.color == theme.tokens().surface)
        );
        let text = ui.scene().texts.last().unwrap();
        assert!(text.wrap);
        assert_eq!(text.color, theme.tokens().text);
        assert_eq!(text.clip.intersection(bounds), Some(text.clip));
        assert!(text.bounds.height < bounds.height);
    }
}

#[test]
fn changing_hover_anchor_restarts_delay_and_hit_descendant_uses_parent_tooltip() {
    let (mut ui, first) = fixture();
    let parent = ui
        .panel(
            ui.root(),
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..layout(130.0, 40.0)
            },
        )
        .unwrap();
    let child = ui
        .button(parent, "Child", None, layout(100.0, 36.0))
        .unwrap();
    ui.set_tooltip(parent, "Parent tooltip").unwrap();
    ui.update(Size::new(400.0, 260.0)).unwrap();
    show(&mut ui, first);
    let deadline = hover(&mut ui, child);
    assert_eq!(ui.tooltip_bounds(), None);
    assert!(
        !ui.tooltip_tick(deadline - Duration::from_millis(1))
            .unwrap()
    );
    assert!(ui.tooltip_tick(deadline).unwrap());
    assert_eq!(ui.scene().texts.last().unwrap().text, "Parent tooltip");
    assert_eq!(ui.runtime.hovered(), Some(child));
}

#[test]
fn focus_alone_does_not_schedule_and_blur_requires_new_hover() {
    let (mut ui, id) = fixture();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.update(Size::new(400.0, 260.0)).unwrap();
    assert_eq!(ui.tooltip_deadline(), None);
    show(&mut ui, id);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: center(&ui, id),
    })
    .unwrap();
    assert_eq!(ui.tooltip_deadline(), None);
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    assert_eq!(ui.tooltip_deadline(), None);
    show(&mut ui, id);
}

#[test]
fn committed_text_and_ime_input_hide_without_requiring_a_key_event() {
    for input in 0..3 {
        let (mut ui, anchor) = fixture();
        let editor = ui
            .text_box(ui.root(), "Editor", "", layout(180.0, 36.0))
            .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.update(Size::new(400.0, 260.0)).unwrap();
        show(&mut ui, anchor);
        match input {
            0 => ui.text_input("typed").unwrap(),
            1 => ui.ime_preedit("composing", Some((0, 0))).unwrap(),
            _ => ui.ime_commit("committed").unwrap(),
        }
        assert_eq!(ui.tooltip_bounds(), None);
        assert_eq!(ui.tooltip_deadline(), None);
        ui.update(Size::new(400.0, 260.0)).unwrap();
        assert!(
            !ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Helpful tooltip")
        );
    }
}
