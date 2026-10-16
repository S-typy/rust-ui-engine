use super::*;
use std::{cell::Cell, rc::Rc};
fn create(width: f32) -> (Controls, EditorControl, Rc<Cell<usize>>) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let calls = Rc::new(Cell::new(0));
    ui.commands_mut().register("Choose", "Choose");
    let counter = calls.clone();
    ui.commands_mut()
        .on_execute(&"Choose".into(), move || counter.set(counter.get() + 1));
    let parts = ui
        .button_edit(
            ui.root(),
            "File",
            "accepted",
            vec![
                EditorButton {
                    label: "Choose file".into(),
                    icon: Some(IconSymbol::FolderOpen),
                    action: EditorButtonAction::Command("Choose".into()),
                },
                EditorButton {
                    label: "Clear file".into(),
                    icon: Some(IconSymbol::Close),
                    action: EditorButtonAction::Clear,
                },
            ],
            LayoutStyle {
                width: Length::Px(width),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, parts, calls)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}

#[test]
fn composite_popup_semantics_do_not_replace_step_icons_with_extra_arrows() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .date_time_edit(
            ui.root(),
            "Document date",
            None,
            DateTimeEditSettings::default(),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    // One disclosure chevron, minus and plus, and no arrow inside the text.
    // The path topology distinguishes the icons, not only their count.
    let paths = &ui.scene().stroke_paths;
    assert_eq!(paths.len(), 3);
    assert_eq!(
        paths.iter().map(|p| p.segments.len()).collect::<Vec<_>>(),
        [2, 1, 2]
    );
    assert_eq!(ui.popup_expanded(parts.editor), Some(false));
}

#[test]
fn embedded_content_centers_both_axes_and_retains_explicit_alignment() {
    let (mut ui, parts, _) = create(320.0);
    let button = parts.buttons[0];
    let mut settings = ui.editor_button_settings(button).unwrap();
    settings.width = 60.0;
    ui.set_editor_button_settings(button, settings).unwrap();
    for alignment in [None, Some(rust_desktop_ui_core::Align::End)] {
        let content = ui
            .label(
                button,
                "ID",
                LayoutStyle {
                    width: Length::Px(20.0),
                    height: Length::Px(16.0),
                    horizontal_alignment: alignment,
                    vertical_alignment: alignment,
                    ..Default::default()
                },
            )
            .unwrap();
        let old = ui.button_content(button);
        ui.set_editor_button_content(button, Some(content)).unwrap();
        frame(&mut ui);
        if let Some(old) = old {
            assert_eq!(ui.runtime.tree().parent(old), None);
        }
        let actual = ui.runtime.bounds(content).unwrap();
        let owner = ui.runtime.bounds(button).unwrap();
        let fraction = if alignment.is_none() { 0.5 } else { 1.0 };
        assert!(
            (actual.x + actual.width * fraction - owner.x - owner.width * fraction).abs() < 0.5,
            "horizontal: {actual:?} in {owner:?}"
        );
        assert!(
            (actual.y + actual.height * fraction - owner.y - owner.height * fraction).abs() < 0.5,
            "vertical: {actual:?} in {owner:?}"
        );
    }
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key: key.clone(),
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    ui.dispatch(InputEvent::KeyUp {
        key,
        modifiers: Default::default(),
    })
    .unwrap();
}

#[test]
fn readonly_button_policy_agrees_with_activation_semantics_and_keyboard_availability() {
    let (mut ui, p, calls) = create(240.0);
    ui.set_composite_read_only(p.container, true).unwrap();
    for button in &p.buttons {
        assert!(!ui.is_enabled(*button));
        ui.activate(*button).unwrap();
    }
    assert_eq!(calls.get(), 0);
    for button in &p.buttons {
        ui.set_editor_button_settings(
            *button,
            EditorButtonSettings {
                enabled_when_read_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    }
    frame(&mut ui);
    assert!(ui.is_enabled(p.buttons[0]));
    assert!(!ui.is_enabled(p.buttons[1]));
    let node = ui
        .semantics()
        .into_iter()
        .find(|n| n.id == p.buttons[0])
        .unwrap();
    assert_eq!(node.label, "Choose file");
    assert!(!node.disabled);
    assert!(node.actions.contains(&SemanticAction::Activate));
    ui.perform_action(p.buttons[0], SemanticAction::Activate, None)
        .unwrap();
    ui.activate(p.buttons[1]).unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(ui.document(p.editor).unwrap().text(), "accepted");
    for settings in [
        EditorButtonSettings {
            enabled: false,
            ..Default::default()
        },
        EditorButtonSettings {
            visible: false,
            ..Default::default()
        },
    ] {
        ui.set_editor_button_settings(p.buttons[0], settings)
            .unwrap();
        ui.activate(p.buttons[0]).unwrap();
        assert!(!ui.is_enabled(p.buttons[0]));
    }
    assert_eq!(calls.get(), 1);
}

#[test]
fn leading_buttons_reorder_keyboard_without_committing_the_editor_session() {
    let (mut ui, p, calls) = create(240.0);
    ui.runtime.apply(Mutation::Focus(Some(p.editor))).unwrap();
    frame(&mut ui);
    ui.set_text(p.editor, "draft").unwrap();
    ui.set_editor_button_settings(
        p.buttons[0],
        EditorButtonSettings {
            placement: EditorButtonPlacement::Leading,
            width: 40.0,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert_eq!(
        ui.runtime.tree().children(p.container).unwrap(),
        &[p.buttons[0], p.editor, p.buttons[1]]
    );
    key(&mut ui, Key::Tab);
    assert_eq!(ui.runtime.focused(), Some(p.buttons[1]));
    assert_eq!(ui.accepted_value(p.editor), Some("accepted"));
    ui.runtime
        .apply(Mutation::Focus(Some(p.buttons[0])))
        .unwrap();
    frame(&mut ui);
    key(&mut ui, Key::Enter);
    assert_eq!(calls.get(), 1);
    assert_eq!(ui.document(p.editor).unwrap().text(), "draft");
    assert_eq!(ui.accepted_value(p.editor), Some("accepted"));
    ui.set_editor_button_settings(
        p.buttons[0],
        EditorButtonSettings {
            visible: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert_eq!(ui.accepted_value(p.editor), Some("accepted"));
}

#[test]
fn narrow_editor_buttons_fit_host_and_invalid_settings_are_atomic() {
    let (mut ui, p, _) = create(50.0);
    ui.set_editor_button_settings(
        p.buttons[0],
        EditorButtonSettings {
            width: 1024.0,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let owner = ui.runtime.bounds(p.container).unwrap();
    for id in p.buttons.iter().chain(std::iter::once(&p.editor)) {
        let bounds = ui.runtime.bounds(*id).unwrap();
        assert!(
            bounds.width >= 0.0
                && bounds.x >= owner.x - 0.01
                && bounds.x + bounds.width <= owner.x + owner.width + 0.01,
            "{bounds:?} {owner:?}"
        );
    }
    assert!(ui.runtime.bounds(p.editor).unwrap().width >= 23.9);
    let original = ui.editor_button_settings(p.buttons[0]).unwrap();
    for width in [0.0, -1.0, f32::NAN, 1025.0] {
        assert!(
            ui.set_editor_button_settings(p.buttons[0], EditorButtonSettings { width, ..original })
                .is_err()
        );
        assert_eq!(ui.editor_button_settings(p.buttons[0]), Some(original));
    }
    let delta = ui.update(Size::new(640.0, 480.0)).unwrap();
    assert_eq!(delta.layout_passes, 0);
}

#[test]
fn content_replacement_keeps_button_identity_and_rejects_interactive_children() {
    let (mut ui, p, calls) = create(240.0);
    ui.runtime
        .apply(Mutation::Focus(Some(p.buttons[0])))
        .unwrap();
    let old = ui
        .label(ui.root(), "Decoration", LayoutStyle::default())
        .unwrap();
    ui.set_editor_button_content(p.buttons[0], Some(old))
        .unwrap();
    let bad = ui
        .edit_box(ui.root(), "Bad", "secret", LayoutStyle::default())
        .unwrap();
    assert!(
        ui.set_editor_button_content(p.buttons[0], Some(bad))
            .is_err()
    );
    assert_eq!(ui.button_content(p.buttons[0]), Some(old));
    let next = ui.label(ui.root(), "New", LayoutStyle::default()).unwrap();
    ui.set_editor_button_content(p.buttons[0], Some(next))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(p.buttons[0]));
    assert_eq!(ui.runtime.tree().parent(old), None);
    assert!(!ui.runtime.tree().node(next).unwrap().props.hit_test);
    ui.activate(p.buttons[0]).unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(
        ui.semantics()
            .into_iter()
            .find(|n| n.id == p.buttons[0])
            .unwrap()
            .label,
        "Choose file"
    );
}

#[test]
fn automatic_composite_height_tracks_theme_and_explicit_height_remains_owned_by_caller() {
    let (mut ui, p, _) = create(240.0);
    ui.set_theme(Theme::Dark).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.runtime.bounds(p.container).unwrap().height,
        ui.theme().tokens().control_height
    );
    let mut style = ui
        .runtime
        .tree()
        .node(p.container)
        .unwrap()
        .props
        .style
        .clone();
    style.height = Length::Px(73.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: p.container,
            style,
        })
        .unwrap();
    ui.set_theme(Theme::FluentLight).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(p.container).unwrap().height, 73.0);
}
