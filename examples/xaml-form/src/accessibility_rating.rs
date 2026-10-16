use super::*;
use rust_desktop_ui_controls::{RatingSettings, Theme};
use rust_desktop_ui_core::{InputEvent, Key, LayoutStyle, Modifiers, Mutation, Size};

#[test]
fn rating_projects_nullable_score_range_and_native_actions_without_preview_commit() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let owner = ui
        .rating(
            ui.root(),
            "Quality",
            RatingSettings {
                step: 0.5,
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let view = MountedView::default();
    let mut accessibility = Accessibility::default();
    let tree = accessibility.tree(&ui, &view, "Rating", 1.0);
    let id = accessibility.id(owner).unwrap();
    let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
    // Native range roles support writable RangeValue; a custom description
    // preserves Rating semantics. Unknown roles are always readonly in AccessKit.
    assert_eq!(node.role(), Role::Slider);
    assert!(!node.is_read_only());
    assert_eq!(node.numeric_value(), None);
    assert_eq!(node.min_numeric_value(), Some(0.0));
    assert_eq!(node.max_numeric_value(), Some(5.0));
    assert_eq!(node.numeric_value_step(), Some(0.5));
    assert_eq!(node.role_description(), Some("Оценка"));
    assert!(node.value().unwrap().contains("Нет оценки"));
    assert!(node.supports_action(Action::Increment));
    assert!(node.supports_action(Action::SetValue));
    accessibility
        .action(
            &mut ui,
            accesskit::ActionRequest {
                action: Action::SetValue,
                target_tree: accesskit::TreeId::ROOT,
                target_node: id,
                data: Some(accesskit::ActionData::NumericValue(2.5)),
            },
        )
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(owner))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowRight,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    let tree = accessibility.tree(&ui, &view, "Rating", 1.0);
    let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
    assert_eq!(node.numeric_value(), Some(2.5));
    assert!(node.description().unwrap().contains("Предпросмотр"));
    for value in [f64::NAN, 6.0, -1.0] {
        accessibility
            .action(
                &mut ui,
                accesskit::ActionRequest {
                    action: Action::SetValue,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: id,
                    data: Some(accesskit::ActionData::NumericValue(value)),
                },
            )
            .unwrap();
        assert_eq!(ui.rating_value(owner), Some(Some(2.5)));
    }
    accessibility
        .action(
            &mut ui,
            accesskit::ActionRequest {
                action: Action::SetValue,
                target_tree: accesskit::TreeId::ROOT,
                target_node: id,
                data: Some(accesskit::ActionData::Value("null".into())),
            },
        )
        .unwrap();
    assert_eq!(ui.rating_value(owner), Some(None));
    ui.set_rating_read_only(owner, true).unwrap();
    let tree = accessibility.tree(&ui, &view, "Rating", 1.0);
    let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
    assert!(node.is_read_only());
    assert!(!node.supports_action(Action::SetValue));
    assert!(!node.supports_action(Action::Increment));
    accessibility
        .action(
            &mut ui,
            accesskit::ActionRequest {
                action: Action::Increment,
                target_tree: accesskit::TreeId::ROOT,
                target_node: id,
                data: None,
            },
        )
        .unwrap();
    assert_eq!(ui.rating_value(owner), Some(None));
}
