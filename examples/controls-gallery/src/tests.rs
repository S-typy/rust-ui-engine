use crate::gallery::Gallery;
use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation, Size};
use rust_desktop_ui_platform_winit::{
    DesktopApp, PlatformEvent,
    accesskit::{Action, ActionData, ActionRequest, NodeId, TreeId},
};
use rust_desktop_ui_treegrid::{ColumnId, RowKey, TreeDataSource};
use std::collections::{HashMap, HashSet};

fn gallery() -> Gallery {
    let mut app = Gallery::new(100_000, "target/gallery-test-never-written.txt".into()).unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    app
}
fn key(app: &mut Gallery, key: Key) {
    app.event(PlatformEvent::Input(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }))
    .unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
}
fn assert_connected(app: &mut Gallery) {
    let tree = app.accessibility(1.5);
    let nodes: HashMap<_, _> = tree.nodes.iter().map(|(id, n)| (*id, n)).collect();
    assert_eq!(nodes.len(), tree.nodes.len(), "duplicate semantic IDs");
    assert!(nodes.contains_key(&tree.focus));
    let mut pending = vec![NodeId(1)];
    let mut visited = HashSet::new();
    while let Some(id) = pending.pop() {
        assert!(visited.insert(id), "duplicate child or cycle");
        let node = nodes.get(&id).expect("dangling accessibility child");
        pending.extend_from_slice(node.children());
    }
    assert_eq!(visited.len(), nodes.len(), "orphan accessibility nodes");
}

#[test]
fn visible_scene_and_accessibility_stay_bounded_across_pages() {
    let mut app = gallery();
    assert_eq!(app.grid.row_count(), 100_000);
    assert!(app.frame.stats.materialized_rows < 32);
    assert!(app.frame.stats.visible_columns < 24);
    assert!(
        app.scene()
            .texts
            .iter()
            .any(|run| run.text.contains("Библиотека"))
    );
    assert_connected(&mut app);
    for page in [1, 2, 0] {
        app.ui.select_tab(app.tabs.container, page).unwrap();
        app.update(Size::new(640.0, 480.0), 1.25).unwrap();
        assert_connected(&mut app);
    }
}

#[test]
fn editor_commits_unicode_and_rejects_invalid_year_without_losing_input() {
    let mut app = gallery();
    app.grid
        .begin_edit(&app.source, RowKey(1), ColumnId(0))
        .unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    let editor = app.ui.runtime.focused().unwrap();
    app.ui.set_text(editor, "Новая книга — café 👩‍💻").unwrap();
    key(&mut app, Key::Enter);
    assert_eq!(
        app.source.cell(RowKey(1), ColumnId(0)),
        "Новая книга — café 👩‍💻"
    );
    assert!(app.grid.editor().is_none());
    app.grid
        .begin_edit(&app.source, RowKey(1), ColumnId(2))
        .unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    let editor = app.ui.runtime.focused().unwrap();
    app.ui.set_text(editor, "not a year").unwrap();
    key(&mut app, Key::Enter);
    assert!(app.grid.editor().unwrap().error.is_some());
    assert_eq!(app.ui.document(editor).unwrap().text(), "not a year");
    key(&mut app, Key::Escape);
    assert!(app.grid.editor().is_none());
}

#[test]
fn enter_during_ime_preedit_does_not_commit_a_grid_cell() {
    let mut app = gallery();
    app.grid
        .begin_edit(&app.source, RowKey(1), ColumnId(0))
        .unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    app.event(PlatformEvent::ImePreedit("候補".into(), Some((0, 6))))
        .unwrap();
    key(&mut app, Key::Enter);
    assert!(app.grid.editor().is_some());
    assert_eq!(app.ime_composition_active(), Some(true));
    app.event(PlatformEvent::ImeCommit("候補".into())).unwrap();
    assert_eq!(app.ime_composition_active(), Some(false));
    assert!(app.grid.editor().is_some());
}

#[test]
fn native_accessibility_actions_share_control_and_grid_models() {
    let mut app = gallery();
    let tree = app.accessibility(1.0);
    let textbox = tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == rust_desktop_ui_platform_winit::accesskit::Role::TextInput)
        .unwrap()
        .0;
    app.event(PlatformEvent::Accessibility(ActionRequest {
        action: Action::SetValue,
        target_tree: TreeId::ROOT,
        target_node: textbox,
        data: Some(ActionData::Value("Пушкин".into())),
    }))
    .unwrap();
    assert!(app.source.has_pending());
    let node = app.grid_node;
    app.ui.runtime.apply(Mutation::Focus(Some(node))).unwrap();
    key(&mut app, Key::End);
    assert_eq!(app.grid.focused(), Some(RowKey(100_000)));
    assert_connected(&mut app);
}

#[test]
fn advertised_accessibility_clicks_route_ribbon_menus_and_disabled_commands() {
    use rust_desktop_ui_controls::Theme;
    use rust_desktop_ui_platform_winit::accesskit::Role;

    fn click(app: &mut Gallery, role: Role, label: &str) {
        let tree = app.accessibility(1.5);
        let (target, node) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == role && node.label() == Some(label))
            .unwrap_or_else(|| panic!("missing advertised {label}"));
        assert!(
            node.supports_action(Action::Click),
            "{label} must advertise Click"
        );
        app.event(PlatformEvent::Accessibility(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: *target,
            data: None,
        }))
        .unwrap();
        app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    }

    let mut app = gallery();
    click(&mut app, Role::Tab, "Вид");
    assert!(app.scene().texts.iter().any(|run| run.text == "Тема"));
    click(&mut app, Role::Button, "Тёмная");
    assert_eq!(app.ui.theme(), Theme::Dark);
    click(&mut app, Role::Tab, "Компоненты");
    assert_eq!(app.ui.selected_tab(app.tabs.container), Some(1));
    click(&mut app, Role::Button, "Меню v");
    assert!(app.ui.popup_open());
    let tree = app.accessibility(1.5);
    let (disabled, node) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::MenuItem && node.label() == Some("Недоступно"))
        .unwrap();
    assert!(!node.supports_action(Action::Click));
    app.event(PlatformEvent::Accessibility(ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: *disabled,
        data: None,
    }))
    .unwrap();
    assert!(
        app.ui.popup_open(),
        "disabled action must not dismiss or execute the menu"
    );
    assert!(app.ui.commands_mut().drain_invocations().is_empty());
    click(&mut app, Role::MenuItem, "Избранное");
    assert!(!app.ui.popup_open());
    assert_eq!(app.ui.commands().checked(&"favorite".into()), Some(true));
    assert_connected(&mut app);
}

#[test]
fn pending_query_blocks_keyboard_command_and_accessibility_cell_editing() {
    use rust_desktop_ui_platform_winit::accesskit::Role;
    use rust_desktop_ui_treegrid::SelectionModifiers;
    let mut app = gallery();
    app.grid
        .focus_cell(
            &app.source,
            RowKey(1),
            ColumnId(0),
            SelectionModifiers::default(),
        )
        .unwrap();
    app.ui
        .runtime
        .apply(Mutation::Focus(Some(app.grid_node)))
        .unwrap();
    app.source.set_search(&mut app.grid, "война").unwrap();
    assert!(app.source.query_pending());
    // No Tick has consumed the completion: this is deterministic even if the
    // worker has already finished scanning before keyboard input arrives.
    for shortcut in [Key::F2, Key::Enter] {
        key(&mut app, shortcut);
        assert!(app.grid.editor().is_none());
        assert!(app.message.contains("Дождитесь завершения"));
    }
    app.ui.commands_mut().execute(&"edit".into());
    app.commands().unwrap();
    assert!(app.grid.editor().is_none());
    let original = app.source.cell(RowKey(1), ColumnId(0));
    let tree = app.accessibility(1.5);
    let (cell, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == Role::Cell
                && node.supports_action(Action::SetValue)
                && node.value() == Some(original.as_str())
        })
        .unwrap();
    app.event(PlatformEvent::Accessibility(ActionRequest {
        action: Action::SetValue,
        target_tree: TreeId::ROOT,
        target_node: *cell,
        data: Some(ActionData::Value(
            "Must not replace a pending-query cell".into(),
        )),
    }))
    .unwrap();
    assert_eq!(app.source.cell(RowKey(1), ColumnId(0)), original);
    assert!(app.grid.editor().is_none());
    assert!(app.message.contains("Дождитесь завершения"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.source.query_pending() {
        app.event(PlatformEvent::Tick).unwrap();
        assert!(
            std::time::Instant::now() < deadline,
            "query completion timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    key(&mut app, Key::F2);
    assert!(
        app.grid.editor().is_some(),
        "editing resumes after the query completion"
    );
}

#[test]
fn search_waits_for_cell_commit_or_cancel_without_discarding_the_draft() {
    use rust_desktop_ui_platform_winit::accesskit::Role;
    for cancel in [false, true] {
        let mut app = gallery();
        // Capture the only initial TextInput (the search field) before opening
        // the separate cell editor, so the action models another input target.
        let search = app
            .accessibility(1.5)
            .nodes
            .into_iter()
            .find(|(_, node)| node.role() == Role::TextInput)
            .unwrap()
            .0;
        app.grid
            .begin_edit(&app.source, RowKey(1), ColumnId(2))
            .unwrap();
        app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
        let editor = app.ui.runtime.focused().unwrap();
        app.ui.set_text(editor, "invalid year").unwrap();
        app.event(PlatformEvent::Accessibility(ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: search,
            data: Some(ActionData::Value("война".into())),
        }))
        .unwrap();
        assert!(!app.source.query_pending());
        assert!(app.grid.query().filter.is_empty());
        assert_eq!(app.ui.document(editor).unwrap().text(), "invalid year");
        key(&mut app, Key::Enter);
        assert!(app.grid.editor().unwrap().error.is_some());
        assert!(!app.source.query_pending());
        if cancel {
            key(&mut app, Key::Escape);
        } else {
            app.ui.set_text(editor, "2026").unwrap();
            key(&mut app, Key::Enter);
            assert_eq!(app.source.cell(RowKey(1), ColumnId(2)), "2026");
        }
        assert!(app.grid.editor().is_none());
        assert_eq!(app.grid.query().filter, "война");
        assert!(app.source.query_pending());
    }
}

#[test]
fn leaving_editor_focus_does_not_let_navigation_or_another_action_replace_the_draft() {
    use rust_desktop_ui_platform_winit::accesskit::Role;
    use rust_desktop_ui_treegrid::SelectionModifiers;
    let mut app = gallery();
    app.grid
        .focus_cell(
            &app.source,
            RowKey(1),
            ColumnId(0),
            SelectionModifiers::default(),
        )
        .unwrap();
    app.ui
        .runtime
        .apply(Mutation::Focus(Some(app.grid_node)))
        .unwrap();
    key(&mut app, Key::F2);
    let editor = app.ui.runtime.focused().unwrap();
    app.ui.set_text(editor, "Черновик без потери").unwrap();
    for next in [Key::ArrowDown, Key::ArrowLeft, Key::F2] {
        app.event(PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
            repeat: false,
        }))
        .unwrap();
        assert_eq!(app.ui.runtime.focused(), Some(app.grid_node));
        key(&mut app, next);
        assert_eq!(app.ui.runtime.focused(), Some(editor));
        assert_eq!(app.grid.focused(), Some(RowKey(1)));
        assert_eq!(app.grid.editor().unwrap().row, RowKey(1));
        assert_eq!(
            app.ui.document(editor).unwrap().text(),
            "Черновик без потери"
        );
    }
    // The Ribbon uses the same CommandRegistry path for its collapse KeyTip.
    key(&mut app, Key::Alt);
    key(&mut app, Key::Character("h".into()));
    key(&mut app, Key::Character("z".into()));
    assert_eq!(app.ui.runtime.focused(), Some(editor));
    let untouched = app.source.cell(RowKey(2), ColumnId(0));
    let tree = app.accessibility(1.5);
    let target = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Cell && node.value() == Some(untouched.as_str()))
        .unwrap()
        .0;
    app.event(PlatformEvent::Accessibility(ActionRequest {
        action: Action::SetValue,
        target_tree: TreeId::ROOT,
        target_node: target,
        data: Some(ActionData::Value("A second edit must wait".into())),
    }))
    .unwrap();
    assert_eq!(app.source.cell(RowKey(2), ColumnId(0)), untouched);
    assert_eq!(app.ui.runtime.focused(), Some(editor));
    assert_eq!(
        app.ui.document(editor).unwrap().text(),
        "Черновик без потери"
    );
    assert!(app.message.contains("Завершите или отмените"));
    key(&mut app, Key::Enter);
    assert_eq!(
        app.source.cell(RowKey(1), ColumnId(0)),
        "Черновик без потери"
    );
    assert!(app.grid.editor().is_none());
}

#[test]
fn shrinking_and_restoring_the_view_keeps_the_hidden_editor_draft_and_validation() {
    let mut app = gallery();
    app.grid
        .begin_edit(&app.source, RowKey(1), ColumnId(2))
        .unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    let editor = app.ui.runtime.focused().unwrap();
    let draft = "Неверный год 👩‍💻 café";
    app.ui.set_text(editor, draft).unwrap();
    key(&mut app, Key::Enter);
    let validation = app.grid.editor().unwrap().error.clone();
    assert!(validation.is_some());
    app.event(PlatformEvent::ImePreedit("候補".into(), Some((0, 6))))
        .unwrap();
    assert_eq!(app.ime_composition_active(), Some(true));
    // The pinned title plus author column put the year outside this width.
    app.update(Size::new(480.0, 320.0), 1.5).unwrap();
    assert!(app.frame.cell_bounds(RowKey(1), ColumnId(2)).is_none());
    assert_eq!(app.ui.document(editor).unwrap().text(), draft);
    assert!(app.ui.document(editor).unwrap().composition().is_none());
    assert_eq!(app.grid.editor().unwrap().error, validation);
    assert!(app.ui.runtime.bounds(editor).is_none());
    assert!(app.ime_target().is_none());
    assert!(app.ime_cursor().is_none());
    assert!(
        app.accessibility(1.5)
            .nodes
            .iter()
            .all(|(_, node)| node.label() != Some("Значение ячейки"))
    );
    // A command while hidden must neither replace the draft nor focus an
    // ineligible node and terminate the host with IneligibleFocus.
    app.ui.commands_mut().execute(&"edit".into());
    app.commands().unwrap();
    assert!(app.ui.runtime.focused().is_none());
    let other = app
        .ui
        .semantics()
        .iter()
        .find(|node| node.label == "Поиск по названию или автору")
        .unwrap()
        .id;
    app.ui.runtime.apply(Mutation::Focus(Some(other))).unwrap();
    app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
    assert!(app.ui.runtime.bounds(editor).is_some());
    assert_eq!(app.ui.document(editor).unwrap().text(), draft);
    assert_eq!(app.grid.editor().unwrap().error, validation);
    assert_eq!(
        app.ui.runtime.focused(),
        Some(other),
        "restoring the overlay must not steal focus"
    );
    assert!(
        app.accessibility(1.5)
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Значение ячейки"))
    );
}

#[test]
fn gallery_text_uses_real_font_fallback() {
    let mut app = gallery();
    let mut engine = rust_desktop_ui_text::TextEngine::new();
    for page in [0, 1, 2] {
        app.ui.select_tab(app.tabs.container, page).unwrap();
        app.update(Size::new(1200.0, 800.0), 1.5).unwrap();
        for text in &app.scene().texts {
            let prepared = engine.prepare(text, 1.5).unwrap();
            // These specimen lines deliberately probe optional system fonts.
            // Their coverage is reported by font_probe, not assumed on every OS.
            if text.text.starts_with("Fallback:") || text.text.contains("emoji:") {
                continue;
            }
            assert_eq!(
                prepared.missing_glyphs, 0,
                "missing glyphs in {:?}",
                text.text
            );
        }
    }
}
