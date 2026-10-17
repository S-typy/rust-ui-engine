use super::*;
use rust_desktop_ui_core::{Key, Modifiers, PointerButton};
fn id(s: &str) -> DocumentId {
    DocumentId::new(s).unwrap()
}
fn app() -> DockApplication {
    let def = xaml::parse(include_str!("../../../DocumentWindow.xaml")).unwrap();
    let mut app = DockApplication::new(def, RunOptions::default(), false, None).unwrap();
    app.enable_documents().unwrap();
    app
}
fn frame(app: &mut DockApplication) {
    for spec in app.windows().unwrap() {
        app.window_mut(spec.key)
            .unwrap()
            .update(Size::new(1200., 800.), 1.5)
            .unwrap();
    }
}
fn command(app: &mut DockApplication, cmd: &str) {
    app.state
        .borrow_mut()
        .command(DockWindowId::MAIN, cmd)
        .unwrap();
    frame(app);
}
fn edit(app: &mut DockApplication, key: &str, value: &str) {
    let mut state = app.state.borrow_mut();
    let f = state.panes.get_mut(&id(key)).unwrap();
    let editor = f.view.editors[0];
    f.ui.set_text(editor, value).unwrap();
    state.refresh_documents().unwrap();
}
fn reply(app: &mut DockApplication, answer: platform::DialogButton) {
    let window = app.window_mut(WindowKey::MAIN).unwrap();
    let request = window.take_dialog_request().expect("dialog requested");
    window
        .dialog_completed(platform::DialogCompletion {
            id: request.id,
            result: Ok(platform::DialogOutcome::Button(answer)),
        })
        .unwrap();
}
#[test]
fn document_identity_and_draft_survive_all_three_presentations() {
    let mut app = app();
    frame(&mut app);
    let (identity, selection) = {
        let mut s = app.state.borrow_mut();
        let f = s.panes.get_mut(&id("Plan")).unwrap();
        let editor = f.view.editors[0];
        f.ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        f.ui.text_input(" draft").unwrap();
        f.ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowLeft,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
            repeat: false,
        })
        .unwrap();
        (editor, f.ui.document(editor).unwrap().selection())
    };
    let draft = app.state.borrow().panes[&id("Plan")]
        .ui
        .document(identity)
        .unwrap()
        .text()
        .to_owned();
    for cmd in ["DocWindowed", "DocMdi", "DocTabbed", "DocMdi"] {
        command(&mut app, cmd);
        let s = app.state.borrow();
        let f = &s.panes[&id("Plan")];
        assert_eq!(f.view.editors[0], identity);
        assert_eq!(f.ui.document(identity).unwrap().text(), draft);
        assert_eq!(f.ui.document(identity).unwrap().selection(), selection);
        assert!(
            s.documents
                .as_ref()
                .unwrap()
                .controller
                .document(&id("Plan"))
                .unwrap()
                .dirty()
        );
    }
    let window = app.windows.get(&WindowKey::MAIN).unwrap();
    let geo = window.mdi_geometry.as_ref().unwrap();
    assert_eq!(geo.frames.len(), 3);
    for f in &geo.frames {
        assert!(f.content.width > 0. && f.content.height > 0.);
        assert!(
            window
                .ui
                .runtime
                .bounds(window.mdi.title_widget(&f.pane).unwrap())
                .is_some()
        );
    }
}
#[test]
fn close_all_native_prompt_cancel_and_save_failure_preserve_every_document() {
    let mut app = app();
    frame(&mut app);
    edit(&mut app, "Plan", "Plan dirty");
    edit(&mut app, "Notes", "Notes dirty");
    command(&mut app, "DocCloseAll");
    reply(&mut app, platform::DialogButton::No);
    assert!(app.state.borrow().layout.snapshot().closed.is_empty());
    reply(&mut app, platform::DialogButton::Cancel);
    assert!(app.state.borrow().layout.snapshot().closed.is_empty());
    command(&mut app, "DocFailSave");
    command(&mut app, "DocCloseAll");
    reply(&mut app, platform::DialogButton::Yes);
    {
        let mut s = app.state.borrow_mut();
        s.documents.as_mut().unwrap().save.as_mut().unwrap().0 = Instant::now();
    }
    frame(&mut app);
    assert!(app.state.borrow().layout.snapshot().closed.is_empty());
    assert!(
        app.state
            .borrow()
            .layout_status
            .contains("Ошибка сохранения")
    );
    assert!(
        !app.state
            .borrow()
            .documents
            .as_ref()
            .unwrap()
            .controller
            .close_pending()
    );
}
#[test]
fn app_close_waits_for_all_consents_and_floating_close_keeps_cancelled_document() {
    let mut app = app();
    frame(&mut app);
    edit(&mut app, "Plan", "Keep");
    app.close_requested(WindowKey::MAIN).unwrap();
    assert!(!app.windows().unwrap().is_empty());
    reply(&mut app, platform::DialogButton::Cancel);
    assert!(!app.windows().unwrap().is_empty());
    command(&mut app, "DocWindowed");
    let key = WindowKey(app.state.borrow().layout.window_for(&id("Plan")).unwrap().0);
    app.close_requested(key).unwrap();
    let request = app.window_mut(key).unwrap().take_dialog_request().unwrap();
    app.window_mut(key)
        .unwrap()
        .dialog_completed(platform::DialogCompletion {
            id: request.id,
            result: Ok(platform::DialogOutcome::Button(
                platform::DialogButton::Cancel,
            )),
        })
        .unwrap();
    assert!(!app.state.borrow().layout.is_closed(&id("Plan")));
    app.close_requested(WindowKey::MAIN).unwrap();
    reply(&mut app, platform::DialogButton::No);
    assert!(app.windows().unwrap().is_empty());
}
#[test]
fn mdi_header_drag_changes_only_geometry_and_escape_restores_uncommitted_preview() {
    let mut app = app();
    frame(&mut app);
    command(&mut app, "DocMdi");
    let window = app.windows.get_mut(&WindowKey::MAIN).unwrap();
    let geo = window.mdi_geometry.as_ref().unwrap();
    let f = geo.frames.last().unwrap();
    let key = f.pane.clone();
    let start = Point::new(f.header.x + 40., f.header.y + 16.);
    let before = window
        .state
        .borrow()
        .documents
        .as_ref()
        .unwrap()
        .mdi
        .stored_bounds(&key)
        .unwrap();
    let down = InputEvent::PointerDown {
        position: start,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    window.event(PlatformEvent::Input(down.clone())).unwrap();
    window
        .event(PlatformEvent::Input(InputEvent::PointerMoved {
            position: Point::new(start.x + 35., start.y + 20.),
        }))
        .unwrap();
    window
        .event(PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        }))
        .unwrap();
    assert_eq!(
        window
            .state
            .borrow()
            .documents
            .as_ref()
            .unwrap()
            .mdi
            .stored_bounds(&key),
        Some(before)
    );
    window.event(PlatformEvent::Input(down)).unwrap();
    window
        .event(PlatformEvent::Input(InputEvent::PointerMoved {
            position: Point::new(start.x + 35., start.y + 20.),
        }))
        .unwrap();
    window
        .event(PlatformEvent::Input(InputEvent::PointerUp {
            position: Point::new(start.x + 35., start.y + 20.),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }))
        .unwrap();
    let after = window
        .state
        .borrow()
        .documents
        .as_ref()
        .unwrap()
        .mdi
        .stored_bounds(&key)
        .unwrap();
    assert_eq!(after.x, before.x + 35.);
    assert_eq!(after.y, before.y + 20.);
}

#[test]
fn stale_dialog_after_external_placement_change_cancels_without_closing() {
    let mut app = app();
    frame(&mut app);
    edit(&mut app, "Plan", "Keep");
    command(&mut app, "DocCloseAll");
    let request = app
        .window_mut(WindowKey::MAIN)
        .unwrap()
        .take_dialog_request()
        .unwrap();
    app.state
        .borrow_mut()
        .layout
        .float_pane(&id("Plan"), Rect::new(100., 100., 640., 440.))
        .unwrap();
    app.window_mut(WindowKey::MAIN)
        .unwrap()
        .dialog_completed(platform::DialogCompletion {
            id: request.id,
            result: Ok(platform::DialogOutcome::Button(platform::DialogButton::No)),
        })
        .unwrap();
    let state = app.state.borrow();
    assert!(state.layout.snapshot().closed.is_empty());
    assert!(!state.documents.as_ref().unwrap().controller.close_pending());
    assert!(state.layout_status.contains("Контекст"));
    assert!(state.alive);
}
#[test]
fn presentation_transfer_rejects_active_composition_without_losing_preedit() {
    let mut app = app();
    frame(&mut app);
    {
        let mut s = app.state.borrow_mut();
        let form = s.panes.get_mut(&id("Plan")).unwrap();
        let editor = form.view.editors[0];
        form.ui
            .runtime
            .apply(Mutation::Focus(Some(editor)))
            .unwrap();
        form.event(PlatformEvent::ImePreedit("compose".into(), None))
            .unwrap();
        assert_eq!(form.ime_composition_active(), Some(true));
    }
    let before = app.state.borrow().layout.snapshot();
    command(&mut app, "DocWindowed");
    let s = app.state.borrow();
    assert_eq!(s.layout.snapshot(), before);
    assert_eq!(
        s.documents.as_ref().unwrap().controller.presentation(),
        DocumentPresentation::Tabbed
    );
    assert_eq!(s.panes[&id("Plan")].ime_composition_active(), Some(true));
}

#[test]
fn delayed_save_completion_requests_redraw_without_external_input() {
    let mut app = app();
    frame(&mut app);
    edit(&mut app, "Plan", "Dirty");
    command(&mut app, "DocCloseAll");
    reply(&mut app, platform::DialogButton::Yes);
    app.state
        .borrow_mut()
        .documents
        .as_mut()
        .unwrap()
        .save
        .as_mut()
        .unwrap()
        .0 = Instant::now();
    let response = app
        .window_mut(WindowKey::MAIN)
        .unwrap()
        .event(PlatformEvent::Tick)
        .unwrap();
    assert!(response.redraw);
    assert!(app.state.borrow().layout.snapshot().closed.len() == 3);
    assert!(
        !app.state
            .borrow()
            .documents
            .as_ref()
            .unwrap()
            .controller
            .close_pending()
    );
}
