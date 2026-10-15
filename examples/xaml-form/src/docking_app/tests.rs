use super::*;
use rust_desktop_ui_core::{Key, Modifiers};

fn application(float: Option<&str>) -> DockApplication {
    DockApplication::new(
        crate::generated_docking::window_definition(),
        RunOptions::default(),
        false,
        float,
    )
    .unwrap()
}
fn pane(name: &str) -> DockPaneId {
    DockPaneId::new(name).unwrap()
}
fn frame(app: &mut DockApplication, width: f32) -> Vec<WindowSpec> {
    let specs = app.windows().unwrap();
    for spec in &specs {
        app.window_mut(spec.key)
            .unwrap()
            .update(Size::new(width, 800.0), 1.5)
            .unwrap();
    }
    specs
}

#[test]
fn native_window_lifecycle_keeps_retiring_view_until_host_acknowledges_close() {
    let mut app = application(Some("Properties"));
    let specs = frame(&mut app, 1200.0);
    assert_eq!(specs.len(), 2);
    let floating = specs.iter().find(|s| s.key != WindowKey::MAIN).unwrap();
    assert_eq!(floating.owner, Some(WindowKey::MAIN));
    assert!(!floating.activate);
    assert!(app.window(floating.key).unwrap().title_bar().is_none());
    assert!(!app.window(floating.key).unwrap().scene().texts.is_empty());
    app.close_requested(floating.key).unwrap();
    app.close_requested(floating.key).unwrap();
    assert_eq!(app.windows().unwrap().len(), 1);
    assert!(app.window(floating.key).is_some());
    assert_eq!(
        app.state.borrow().layout.window_for(&pane("Properties")),
        Some(DockWindowId::MAIN)
    );
    app.window_closed(floating.key);
    assert!(app.window(floating.key).is_none());
    app.close_requested(WindowKey::MAIN).unwrap();
    assert!(app.windows().unwrap().is_empty());
}

#[test]
fn float_redock_resize_and_theme_preserve_document_session_and_widget_identity() {
    let mut app = application(None);
    frame(&mut app, 1200.0);
    let id = pane("Document");
    let (editor, original, draft, selection, a11y_root) = {
        let mut state = app.state.borrow_mut();
        let form = state.panes.get_mut(&id).unwrap();
        let editor = form.editor();
        let original = form.ui.accepted_value(editor).unwrap().to_owned();
        form.ui
            .runtime
            .apply(Mutation::Focus(Some(editor)))
            .unwrap();
        form.ui.text_input(" draft").unwrap();
        form.ui
            .dispatch(InputEvent::KeyDown {
                key: Key::ArrowLeft,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                repeat: false,
            })
            .unwrap();
        let result = (
            editor,
            original,
            form.text().to_owned(),
            form.ui.document(editor).unwrap().selection(),
            form.accessible.root_id(),
        );
        state.focused = Some((DockWindowId::MAIN, id.clone()));
        state
            .request(DockWindowId::MAIN, DockRequest::Float(id.clone()))
            .unwrap();
        result
    };
    let specs = frame(&mut app, 650.0);
    let floating = specs
        .iter()
        .find(|spec| spec.key != WindowKey::MAIN)
        .unwrap()
        .key;
    assert_eq!(app.window(floating).unwrap().ime_target(), Some(editor));
    assert_eq!(app.window(WindowKey::MAIN).unwrap().ime_target(), None);
    app.state
        .borrow_mut()
        .command(DockWindowId::MAIN, "Dark")
        .unwrap();
    frame(&mut app, 900.0);
    app.close_requested(floating).unwrap();
    // Host retirement must not commit or cancel an editor now owned by MAIN.
    app.window_mut(floating)
        .unwrap()
        .event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    app.window_mut(floating)
        .unwrap()
        .event(PlatformEvent::ImeCancel)
        .unwrap();
    app.window_closed(floating);
    frame(&mut app, 1200.0);
    let state = app.state.borrow();
    let form = &state.panes[&id];
    assert_eq!(form.editor(), editor);
    assert_eq!(form.text(), draft);
    assert_eq!(form.ui.accepted_value(editor), Some(original.as_str()));
    assert_eq!(form.ui.document(editor).unwrap().selection(), selection);
    assert!(form.ui.document(editor).unwrap().can_undo());
    assert_eq!(form.accessible.root_id(), a11y_root);
    assert_eq!(form.ui.runtime.focused(), Some(editor));
    assert_eq!(state.focused, Some((DockWindowId::MAIN, id)));
}

#[test]
fn hidden_and_closed_panes_leave_accessibility_but_keep_content_for_restore() {
    let mut app = application(None);
    frame(&mut app, 1200.0);
    app.windows
        .get_mut(&WindowKey::MAIN)
        .unwrap()
        .compose_accessibility(1.5);
    let owners = &app.windows[&WindowKey::MAIN].action_owners;
    assert!(owners.values().any(|p| p == &pane("Document")));
    assert!(!owners.values().any(|p| p == &pane("Notes")));
    let id = pane("Properties");
    let editor = app.state.borrow().panes[&id].editor();
    app.state
        .borrow_mut()
        .request(DockWindowId::MAIN, DockRequest::Close(id.clone()))
        .unwrap();
    frame(&mut app, 1200.0);
    app.windows
        .get_mut(&WindowKey::MAIN)
        .unwrap()
        .compose_accessibility(1.5);
    assert!(
        !app.windows[&WindowKey::MAIN]
            .action_owners
            .values()
            .any(|p| p == &id)
    );
    app.state
        .borrow_mut()
        .command(DockWindowId::MAIN, "DockRestore")
        .unwrap();
    frame(&mut app, 1200.0);
    assert_eq!(app.state.borrow().panes[&id].editor(), editor);
    assert!(!app.state.borrow().layout.is_closed(&id));
}

#[test]
fn reset_restores_tree_and_all_pane_arenas_remain_distinct() {
    let mut app = application(Some("Properties"));
    frame(&mut app, 1200.0);
    let ids: Vec<_> = app
        .state
        .borrow()
        .panes
        .values()
        .flat_map(|form| form.view.editors.iter().copied())
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len());
    app.state
        .borrow_mut()
        .request(DockWindowId::MAIN, DockRequest::Close(pane("Output")))
        .unwrap();
    app.state
        .borrow_mut()
        .command(DockWindowId::MAIN, "DockReset")
        .unwrap();
    assert_eq!(frame(&mut app, 300.0).len(), 1);
    let state = app.state.borrow();
    assert_eq!(state.layout.snapshot(), state.initial);
    assert_eq!(
        ids,
        state
            .panes
            .values()
            .flat_map(|form| form.view.editors.iter().copied())
            .collect::<Vec<_>>()
    );
}

#[test]
fn closing_current_pane_selects_a_live_command_target() {
    let mut app = application(None);
    let mut state = app.state.borrow_mut();
    state.active = pane("Properties");
    state
        .request(DockWindowId::MAIN, DockRequest::Close(pane("Properties")))
        .unwrap();
    assert!(!state.layout.is_closed(&state.active));
    state.command(DockWindowId::MAIN, "DockFloat").unwrap();
    assert_eq!(state.layout.floating().len(), 1);
    drop(state);
    assert_eq!(frame(&mut app, 1200.0).len(), 2);
}

#[test]
fn unknown_startup_pane_and_wrong_shell_fail_before_native_window_creation() {
    assert!(
        DockApplication::new(
            crate::generated_docking::window_definition(),
            RunOptions::default(),
            false,
            Some("Unknown")
        )
        .is_err()
    );
    assert!(
        DockApplication::new(
            crate::generated::window_definition(),
            RunOptions::default(),
            false,
            None
        )
        .is_err()
    );
    let mut definition = crate::generated_docking::window_definition();
    if let ElementKind::StackPanel(panel) = &mut definition.content.kind {
        panel.children.last_mut().unwrap().common.width = Some(400.0);
    }
    assert!(DockApplication::new(definition, RunOptions::default(), false, None).is_err());
}

#[test]
fn rejected_stale_gesture_keeps_the_workspace_alive_and_unchanged() {
    let app = application(None);
    let mut state = app.state.borrow_mut();
    let before = state.layout.snapshot();
    state
        .request(
            DockWindowId::MAIN,
            DockRequest::Move {
                pane: pane("Document"),
                placement: DockPlacement::Tab {
                    group: DockGroupId(u64::MAX),
                    index: 0,
                },
            },
        )
        .unwrap();
    assert_eq!(state.layout.snapshot(), before);
    assert!(state.alive);
}

#[test]
fn restore_all_sixty_four_panes_does_not_exceed_tree_depth() {
    let panes = (0..64)
        .map(|index| {
            format!("<DockPane Id=\"Pane{index}\"><TextBox Text=\"Value{index}\" /></DockPane>")
        })
        .collect::<String>();
    let definition = xaml::parse(&format!("<Window xmlns=\"urn:rust-ui-engine:ui\" Padding=\"0\"><DockWorkspace><DockGroup>{panes}</DockGroup></DockWorkspace></Window>")).unwrap();
    let app = DockApplication::new(definition, RunOptions::default(), false, None).unwrap();
    let mut state = app.state.borrow_mut();
    for index in 0..64 {
        state.layout.close(&pane(&format!("Pane{index}"))).unwrap();
    }
    assert!(state.layout.root(DockWindowId::MAIN).is_none());
    state.command(DockWindowId::MAIN, "DockRestore").unwrap();
    assert!(state.layout.snapshot().closed.is_empty());
    assert_eq!(
        state.layout.window_panes(DockWindowId::MAIN).unwrap().len(),
        64
    );
    let DockNode::Tabs { panes, .. } = state.layout.root(DockWindowId::MAIN).unwrap() else {
        panic!("restored panes must share a bounded tab group")
    };
    assert_eq!(panes.len(), 64);
    for index in 0..64 {
        assert_eq!(
            state.panes[&pane(&format!("Pane{index}"))].text(),
            format!("Value{index}")
        );
    }
}
