use super::*;
use rust_desktop_ui_controls::{BadgeContent, BadgeSettings, EditorSettings, Theme};
use rust_desktop_ui_core::{LayoutStyle, Length, Size};

#[test]
fn badge_supplements_description_without_replacing_name_value_tooltip_or_nodes() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let owner = ui
        .text_box(
            ui.root(),
            "Document",
            "kept text",
            LayoutStyle {
                width: Length::Px(260.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_editor_settings(
        owner,
        EditorSettings {
            placeholder: "Подсказка ввода".into(),
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_tooltip(owner, "Описание поля").unwrap();
    let view = MountedView::default();
    let mut a11y = Accessibility::default();
    ui.update(Size::new(400.0, 200.0)).unwrap();
    let before = a11y.tree(&ui, &view, "Badges", 1.5);
    let id = a11y.id(owner).unwrap();
    let old = before
        .nodes
        .iter()
        .find(|(node, _)| *node == id)
        .unwrap()
        .1
        .clone();
    ui.set_badge(
        owner,
        Some(BadgeSettings {
            content: BadgeContent::Count(u64::MAX),
            label: "Сообщения".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    ui.update(Size::new(400.0, 200.0)).unwrap();
    let after = a11y.tree(&ui, &view, "Badges", 1.5);
    assert_eq!(before.nodes.len(), after.nodes.len());
    let current = &after.nodes.iter().find(|(node, _)| *node == id).unwrap().1;
    assert_eq!(current.role(), old.role());
    assert_eq!(current.label(), old.label());
    assert_eq!(current.value(), old.value());
    assert_eq!(current.tooltip(), old.tooltip());
    let description = current.description().unwrap();
    assert!(description.contains("Подсказка ввода"));
    assert!(description.contains("Описание поля"));
    assert!(description.contains(&u64::MAX.to_string()));
    ui.set_badge(owner, None).unwrap();
    let restored = a11y.tree(&ui, &view, "Badges", 1.5);
    let restored = &restored
        .nodes
        .iter()
        .find(|(node, _)| *node == id)
        .unwrap()
        .1;
    assert_eq!(restored.description(), old.description());
}

#[test]
fn decorated_layout_owner_remains_in_native_tree_without_extra_nodes_or_focus() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let owner = ui
        .panel(
            ui.root(),
            LayoutStyle {
                width: Length::Px(260.0),
                height: Length::Px(80.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_automation_name(owner, "Сводка").unwrap();
    ui.update(Size::new(400.0, 200.0)).unwrap();
    let view = MountedView::default();
    let mut a11y = Accessibility::default();
    let before = a11y.tree(&ui, &view, "Badges", 1.0);
    let id = a11y.id(owner).unwrap();
    let prior = &before.nodes.iter().find(|(node, _)| *node == id).unwrap().1;
    assert_eq!(prior.role(), Role::GenericContainer);
    ui.set_badge(
        owner,
        Some(BadgeSettings {
            content: BadgeContent::Text("Готово".into()),
            ..Default::default()
        }),
    )
    .unwrap();
    let after = a11y.tree(&ui, &view, "Badges", 1.0);
    let current = &after.nodes.iter().find(|(node, _)| *node == id).unwrap().1;
    assert_eq!(current.role(), Role::Group);
    assert_eq!(current.label(), prior.label());
    assert_eq!(current.children(), prior.children());
    assert_eq!(before.nodes.len(), after.nodes.len());
    assert!(!current.supports_action(Action::Focus));
    assert!(current.description().unwrap().contains("Готово"));
    for settings in [
        BadgeSettings {
            visible: false,
            ..ui.badge(owner).unwrap().clone()
        },
        BadgeSettings {
            offset: rust_desktop_ui_core::Point::new(256.0, 0.0),
            ..ui.badge(owner).unwrap().clone()
        },
    ] {
        ui.set_badge(owner, Some(settings)).unwrap();
        let hidden = a11y.tree(&ui, &view, "Badges", 1.0);
        let hidden = &hidden.nodes.iter().find(|(node, _)| *node == id).unwrap().1;
        assert_eq!(hidden.role(), Role::GenericContainer);
        assert!(hidden.description().is_none());
    }
    ui.set_badge(owner, None).unwrap();
    let after = a11y.tree(&ui, &view, "Badges", 1.0);
    assert_eq!(
        after
            .nodes
            .iter()
            .find(|(node, _)| *node == id)
            .unwrap()
            .1
            .role(),
        Role::GenericContainer
    );
}
