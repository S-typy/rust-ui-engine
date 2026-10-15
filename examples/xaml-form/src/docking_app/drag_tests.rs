use super::*;
use rust_desktop_ui_core::Modifiers;

fn pane(name: &str) -> DockPaneId {
    DockPaneId::new(name).unwrap()
}
fn center(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn frame(app: &mut DockApplication) {
    for spec in app.windows().unwrap() {
        let main = spec.key == WindowKey::MAIN;
        let size = if main {
            Size::new(1200.0, 800.0)
        } else {
            Size::new(640.0, 440.0)
        };
        let scale = if main { 1.25 } else { 2.0 };
        let origin = if main {
            Point::new(-1600.0, 120.0)
        } else {
            Point::new(100.0 + spec.key.0 as f32 * 1400.0, 160.0)
        };
        let window = app.window_mut(spec.key).unwrap();
        window.desktop_geometry_changed(WindowGeometry {
            client_origin: origin,
            client_size: size,
            outer_position: Point::new(origin.x / scale, origin.y / scale),
            scale,
        });
        window.update(size, scale).unwrap();
    }
}

fn send(
    app: &mut DockApplication,
    source: WindowKey,
    target: Option<WindowKey>,
    screen: Point,
    kind: u8,
) {
    let geometry = app.state.borrow().native_geometry[&DockWindowId(source.0)];
    let position = geometry.screen_to_client(screen).unwrap();
    let event = match kind {
        0 => InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        1 => InputEvent::PointerMoved { position },
        _ => InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    };
    let window = app.window_mut(source).unwrap();
    window.desktop_pointer_changed(DesktopPointer {
        screen_position: screen,
        hit_window: target,
    });
    window.event(PlatformEvent::Input(event)).unwrap();
}

fn start(app: &mut DockApplication, source: WindowKey, id: &DockPaneId) {
    let window = &app.windows[&source];
    let tab = window.dock.tab_widget(id).unwrap();
    let position = center(window.ui.runtime.bounds(tab).unwrap());
    let screen = app.state.borrow().native_geometry[&DockWindowId(source.0)]
        .client_to_screen(position)
        .unwrap();
    send(app, source, Some(source), screen, 0);
    assert_eq!(app.windows[&source].ui.runtime.captured(), Some(tab));
}

fn destination(app: &DockApplication, target: WindowKey) -> Point {
    let window = &app.windows[&target];
    let geometry = window.geometry.as_ref().unwrap();
    let local = geometry
        .groups
        .first()
        .map_or(center(geometry.bounds), |group| center(group.content));
    app.state.borrow().native_geometry[&DockWindowId(target.0)]
        .client_to_screen(local)
        .unwrap()
}

#[test]
fn captured_drag_between_native_windows_converts_dpi_and_preserves_document_identity() {
    let mut app = super::super::tests::application(Some("Properties"));
    frame(&mut app);
    let target = *app
        .windows
        .keys()
        .find(|key| **key != WindowKey::MAIN)
        .unwrap();
    let id = pane("Document");
    let editor = app.state.borrow().panes[&id].editor();
    app.state
        .borrow_mut()
        .panes
        .get_mut(&id)
        .unwrap()
        .ui
        .set_text(editor, "retained text")
        .unwrap();
    start(&mut app, WindowKey::MAIN, &id);
    let before = app.state.borrow().layout.snapshot();
    let screen = destination(&app, target);
    // CursorLeft is expected during native capture, and must not end the drag.
    app.window_mut(WindowKey::MAIN)
        .unwrap()
        .event(PlatformEvent::Input(InputEvent::PointerLeft))
        .unwrap();
    send(&mut app, WindowKey::MAIN, Some(target), screen, 1);
    frame(&mut app);
    assert_eq!(app.state.borrow().layout.snapshot(), before);
    assert!(app.windows[&WindowKey::MAIN].dock.drop_preview().is_none());
    assert!(app.windows[&target].dock.drop_preview().is_some());
    send(&mut app, WindowKey::MAIN, Some(target), screen, 2);
    assert!(app.state.borrow().drag.is_none());
    assert_eq!(
        app.state.borrow().layout.window_for(&id),
        Some(DockWindowId(target.0))
    );
    assert_eq!(
        app.state.borrow().layout.floating().len(),
        1,
        "no intermediate Float"
    );
    frame(&mut app);
    assert!(app.windows[&target].dock.drop_preview().is_none());
    assert_eq!(app.state.borrow().panes[&id].editor(), editor);
    assert_eq!(app.state.borrow().panes[&id].text(), "retained text");
    // A duplicate release cannot move the pane again.
    let after = app.state.borrow().layout.snapshot();
    send(&mut app, WindowKey::MAIN, Some(target), screen, 2);
    assert_eq!(app.state.borrow().layout.snapshot(), after);
}

#[test]
fn cross_window_escape_blur_capture_loss_and_target_close_cancel_without_float() {
    for cancel in 0..4 {
        let mut app = super::super::tests::application(Some("Properties"));
        frame(&mut app);
        let target = *app
            .windows
            .keys()
            .find(|key| **key != WindowKey::MAIN)
            .unwrap();
        let id = pane("Document");
        start(&mut app, WindowKey::MAIN, &id);
        let screen = destination(&app, target);
        send(&mut app, WindowKey::MAIN, Some(target), screen, 1);
        let before = app.state.borrow().layout.snapshot();
        match cancel {
            0 => {
                app.window_mut(WindowKey::MAIN)
                    .unwrap()
                    .event(PlatformEvent::Input(InputEvent::KeyDown {
                        key: Key::Escape,
                        modifiers: Modifiers::default(),
                        repeat: false,
                    }))
                    .unwrap();
            }
            1 => {
                app.window_mut(WindowKey::MAIN)
                    .unwrap()
                    .event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
                    .unwrap();
            }
            2 => {
                app.window_mut(WindowKey::MAIN)
                    .unwrap()
                    .pointer_capture_lost()
                    .unwrap();
            }
            _ => {
                app.close_requested(target).unwrap();
                app.window_closed(target);
            }
        }
        send(&mut app, WindowKey::MAIN, None, screen, 2);
        frame(&mut app);
        assert!(app.state.borrow().drag.is_none());
        assert!(
            app.windows[&WindowKey::MAIN]
                .ui
                .runtime
                .captured()
                .is_none()
        );
        assert_eq!(
            app.state.borrow().layout.window_for(&id),
            Some(DockWindowId::MAIN)
        );
        if cancel < 3 {
            assert_eq!(app.state.borrow().layout.snapshot(), before);
        } else {
            assert!(app.state.borrow().layout.floating().is_empty());
        }
    }
}

#[test]
fn native_hit_result_and_client_chrome_prevent_docking_into_obscured_or_ineligible_target() {
    let mut app = super::super::tests::application(Some("Properties"));
    frame(&mut app);
    let target = *app
        .windows
        .keys()
        .find(|key| **key != WindowKey::MAIN)
        .unwrap();
    start(&mut app, WindowKey::MAIN, &pane("Document"));
    let screen = destination(&app, target);
    send(&mut app, WindowKey::MAIN, None, screen, 1);
    frame(&mut app);
    assert!(
        app.state.borrow().drag.as_ref().unwrap().target.is_none(),
        "topmost foreign window wins over geometric overlap"
    );
    assert!(app.windows[&target].dock.drop_preview().is_none());
    // Dropping onto target's footer cancels rather than silently floating.
    let footer = app.state.borrow().native_geometry[&DockWindowId(target.0)]
        .client_to_screen(Point::new(100.0, 435.0))
        .unwrap();
    let before = app.state.borrow().layout.snapshot();
    send(&mut app, WindowKey::MAIN, Some(target), footer, 2);
    assert_eq!(app.state.borrow().layout.snapshot(), before);
}

#[test]
fn floating_pane_can_return_to_empty_main_or_move_to_another_float() {
    let definition = xaml::parse(r#"<Window xmlns="urn:rust-ui-engine:ui" Padding="0"><DockWorkspace><DockGroup><DockPane Id="A"><TextBox Text="value" /></DockPane><DockPane Id="B"><TextBox /></DockPane></DockGroup></DockWorkspace></Window>"#).unwrap();
    let mut app =
        DockApplication::new(definition, RunOptions::default(), false, Some("A")).unwrap();
    app.state
        .borrow_mut()
        .request(DockWindowId::MAIN, DockRequest::Float(pane("B")))
        .unwrap();
    frame(&mut app);
    let first = WindowKey(app.state.borrow().layout.window_for(&pane("A")).unwrap().0);
    let second = WindowKey(app.state.borrow().layout.window_for(&pane("B")).unwrap().0);
    start(&mut app, first, &pane("A"));
    let screen = destination(&app, second);
    send(&mut app, first, Some(second), screen, 1);
    send(&mut app, first, Some(second), screen, 2);
    app.window_closed(first);
    frame(&mut app);
    assert_eq!(app.state.borrow().layout.floating().len(), 1);
    start(&mut app, second, &pane("A"));
    let screen = destination(&app, WindowKey::MAIN);
    send(&mut app, second, Some(WindowKey::MAIN), screen, 1);
    frame(&mut app);
    assert!(app.windows[&WindowKey::MAIN].dock.drop_preview().is_some());
    send(&mut app, second, Some(WindowKey::MAIN), screen, 2);
    assert_eq!(
        app.state.borrow().layout.window_for(&pane("A")),
        Some(DockWindowId::MAIN)
    );
    assert_eq!(app.state.borrow().panes[&pane("A")].text(), "value");
}

#[test]
fn release_between_native_resize_and_layout_does_not_use_stale_target_geometry() {
    let mut app = super::super::tests::application(Some("Properties"));
    frame(&mut app);
    let target = *app
        .windows
        .keys()
        .find(|key| **key != WindowKey::MAIN)
        .unwrap();
    start(&mut app, WindowKey::MAIN, &pane("Document"));
    let screen = destination(&app, target);
    send(&mut app, WindowKey::MAIN, Some(target), screen, 1);
    let before = app.state.borrow().layout.snapshot();
    let mut geometry = app.state.borrow().native_geometry[&DockWindowId(target.0)];
    geometry.client_size.width += 200.0;
    app.window_mut(target)
        .unwrap()
        .desktop_geometry_changed(geometry);
    assert!(app.state.borrow().drag.as_ref().unwrap().target.is_none());
    send(&mut app, WindowKey::MAIN, Some(target), screen, 2);
    assert_eq!(app.state.borrow().layout.snapshot(), before);
}

#[test]
fn reset_discards_old_split_drag_and_queued_input_until_new_geometry_is_ready() {
    let mut app = super::super::tests::application(None);
    frame(&mut app);
    let window = &app.windows[&WindowKey::MAIN];
    let point = center(window.geometry.as_ref().unwrap().splitters[0].bounds);
    let native = app.state.borrow().native_geometry[&DockWindowId::MAIN];
    let screen = native.client_to_screen(point).unwrap();
    send(&mut app, WindowKey::MAIN, Some(WindowKey::MAIN), screen, 0);
    let moved = native
        .client_to_screen(Point::new(point.x + 100.0, point.y))
        .unwrap();
    send(&mut app, WindowKey::MAIN, Some(WindowKey::MAIN), moved, 1);
    assert_ne!(
        app.state.borrow().layout.snapshot(),
        app.state.borrow().initial
    );
    app.state
        .borrow_mut()
        .command(DockWindowId::MAIN, "DockReset")
        .unwrap();
    let expected = app.state.borrow().layout.snapshot();
    send(&mut app, WindowKey::MAIN, Some(WindowKey::MAIN), moved, 1);
    send(&mut app, WindowKey::MAIN, Some(WindowKey::MAIN), moved, 2);
    assert!(app.windows[&WindowKey::MAIN].geometry.is_none());
    assert!(
        app.windows[&WindowKey::MAIN]
            .ui
            .runtime
            .captured()
            .is_none()
    );
    frame(&mut app);
    assert_eq!(app.state.borrow().layout.snapshot(), expected);
    assert!(!app.windows[&WindowKey::MAIN].dock.is_interacting());
}
