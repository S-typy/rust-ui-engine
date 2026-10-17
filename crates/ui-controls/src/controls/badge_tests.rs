use super::*;

fn fixture() -> (Controls, WidgetId, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parent = ui
        .panel(
            ui.root(),
            LayoutStyle {
                width: Length::Px(280.0),
                height: Length::Px(140.0),
                padding: Edges::all(24.0),
                ..Default::default()
            },
        )
        .unwrap();
    let owner = ui
        .button(
            parent,
            "Messages",
            None,
            LayoutStyle {
                width: Length::Px(120.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    (ui, parent, owner)
}
fn badge(count: u64) -> BadgeSettings {
    BadgeSettings {
        content: BadgeContent::Count(count),
        label: "Непрочитанные".into(),
        ..Default::default()
    }
}

#[test]
fn badge_follows_owner_without_layout_hit_focus_or_semantic_nodes() {
    let (mut ui, _, owner) = fixture();
    ui.runtime.apply(Mutation::Focus(Some(owner))).unwrap();
    let before = ui.runtime.bounds(owner).unwrap();
    let nodes = ui.runtime.tree().len();
    let semantic_count = ui.semantics().len();
    let actions = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == owner)
        .unwrap()
        .actions;
    ui.set_badge(owner, Some(badge(u64::MAX))).unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    let bounds = ui.badge_bounds(owner).unwrap();
    assert_eq!(ui.runtime.bounds(owner), Some(before));
    assert_eq!(ui.runtime.tree().len(), nodes);
    assert_eq!(ui.runtime.focused(), Some(owner));
    assert_eq!(
        ui.runtime
            .hit_test(Point::new(before.x + before.width - 2.0, before.y + 2.0)),
        Some(owner)
    );
    assert_ne!(
        ui.runtime
            .hit_test(Point::new(bounds.x + bounds.width - 1.0, bounds.y + 1.0)),
        Some(owner)
    );
    let semantic = ui.semantics();
    assert_eq!(semantic.len(), semantic_count);
    let semantic = semantic.iter().find(|node| node.id == owner).unwrap();
    assert_eq!(semantic.label, "Messages");
    assert_eq!(semantic.actions, actions);
    assert!(
        semantic
            .description
            .as_ref()
            .unwrap()
            .contains(&u64::MAX.to_string())
    );
    assert!(ui.scene().texts.iter().any(|run| run.text == "99+"));
    let mut props = ui.runtime.tree().node(owner).unwrap().props.clone();
    props.style.width = Length::Px(200.0);
    ui.runtime
        .apply(Mutation::SetProps { node: owner, props })
        .unwrap();
    ui.update(Size::new(500.0, 300.0)).unwrap();
    assert_eq!(ui.badge_bounds(owner).unwrap().x, bounds.x + 80.0);
    ui.set_badge(owner, None).unwrap();
    assert!(ui.badge_bounds(owner).is_none());
    assert!(
        ui.semantics()
            .iter()
            .find(|node| node.id == owner)
            .unwrap()
            .description
            .is_none()
    );
}

#[test]
fn badge_hides_with_parent_restores_and_prunes_without_stale_geometry() {
    let (mut ui, parent, owner) = fixture();
    ui.set_badge(owner, Some(badge(7))).unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    let bounds = ui.badge_bounds(owner).unwrap();
    let original = ui.runtime.tree().node(parent).unwrap().props.clone();
    let mut hidden = original.clone();
    hidden.visible = false;
    ui.runtime
        .apply(Mutation::SetProps {
            node: parent,
            props: hidden,
        })
        .unwrap();
    assert!(ui.badge_bounds(owner).is_none());
    ui.update(Size::new(400.0, 300.0)).unwrap();
    assert!(!ui.scene().texts.iter().any(|run| run.text == "7"));
    ui.runtime
        .apply(Mutation::SetProps {
            node: parent,
            props: original,
        })
        .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    assert_eq!(ui.badge_bounds(owner), Some(bounds));
    ui.runtime.apply(Mutation::Detach(owner)).unwrap();
    assert!(ui.badge_bounds(owner).is_none());
    assert!(ui.badge(owner).is_some());
    ui.remove(owner).unwrap();
    assert!(ui.badges.is_empty());
    assert!(ui.set_badge(owner, Some(badge(1))).is_err());
}

#[test]
fn badge_clip_disabled_and_scene_scaling_are_resolved_from_owner() {
    let (mut ui, _, owner) = fixture();
    ui.set_badge(
        owner,
        Some(BadgeSettings {
            content: BadgeContent::Icon(IconSymbol::Check),
            clip: BadgeClip::Owner,
            severity: BadgeSeverity::Success,
            ..Default::default()
        }),
    )
    .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    let owner_bounds = ui.runtime.bounds(owner).unwrap();
    let badge = ui.scene().rounded_rectangles.last().unwrap();
    assert_eq!(badge.clip, owner_bounds);
    let enabled_color = badge.color;
    ui.set_enabled(owner, false).unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    assert_ne!(
        ui.scene().rounded_rectangles.last().unwrap().color,
        enabled_color
    );
    assert!(
        ui.semantics()
            .iter()
            .find(|node| node.id == owner)
            .unwrap()
            .description
            .is_some()
    );
    // Coordinates stay logical; the renderer's ordinary Scene scaling also
    // scales the badge, including its clip and vector icon.
    let logical = *ui.scene().rounded_rectangles.last().unwrap();
    let mut scaled = Scene::default();
    scaled
        .append_transformed_view(
            ui.scene(),
            Point::new(0.0, 0.0),
            2.0,
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
        )
        .unwrap();
    let physical = scaled.rounded_rectangles.last().unwrap();
    assert_eq!(physical.bounds.width, logical.bounds.width * 2.0);
    assert_eq!(physical.clip.width, logical.clip.width * 2.0);
}

#[test]
fn invalid_badge_replacement_is_atomic_including_final_grapheme_joins() {
    let (mut ui, _, owner) = fixture();
    let original = badge(3);
    ui.set_badge(owner, Some(original.clone())).unwrap();
    for invalid in [
        BadgeSettings {
            offset: Point::new(f32::NAN, 0.0),
            ..original.clone()
        },
        BadgeSettings {
            max_count: 0,
            ..original.clone()
        },
        BadgeSettings {
            content: BadgeContent::Text("a".repeat(65)),
            ..original.clone()
        },
        BadgeSettings {
            label: "x".repeat(257),
            ..original.clone()
        },
        BadgeSettings {
            content: BadgeContent::Text(format!("a{}", "\u{301}".repeat(40))),
            ..original.clone()
        },
    ] {
        assert!(ui.set_badge(owner, Some(invalid)).is_err());
        assert_eq!(ui.badge(owner), Some(&original));
    }
    // A leading cluster of 32 combining scalars grows when joined to the
    // preceding description space. Validating the isolated text is insufficient.
    assert!(
        ui.set_badge(
            owner,
            Some(BadgeSettings {
                content: BadgeContent::Text("\u{301}".repeat(32)),
                ..original.clone()
            })
        )
        .is_err()
    );
    assert_eq!(ui.badge(owner), Some(&original));
}

#[test]
fn long_badge_text_is_bounded_but_accessible_content_is_complete() {
    let (mut ui, _, owner) = fixture();
    let text = "Длинное уведомление о завершении обработки документа";
    ui.set_badge(
        owner,
        Some(BadgeSettings {
            content: BadgeContent::Text(text.into()),
            severity: BadgeSeverity::Warning,
            ..Default::default()
        }),
    )
    .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    assert!(ui.badge_bounds(owner).unwrap().width <= 160.0);
    assert!(ui.scene().texts.iter().any(|run| run.text.ends_with('…')));
    assert!(
        ui.semantics()
            .iter()
            .find(|node| node.id == owner)
            .unwrap()
            .description
            .as_ref()
            .unwrap()
            .contains(text)
    );
}

#[test]
fn badge_layers_follow_the_owner_and_clipped_owners_do_not_leak_indicators() {
    let (mut ui, parent, owner) = fixture();
    ui.set_badge(
        owner,
        Some(BadgeSettings {
            content: BadgeContent::Text("BASE".into()),
            ..Default::default()
        }),
    )
    .unwrap();
    ui.commands_mut().register("menu", "Menu command");
    ui.popup_menu_entries(owner, &[MenuEntry::Item(MenuItem::new("Menu", "menu"))])
        .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    let menu = ui
        .semantics()
        .iter()
        .find(|node| node.role == Role::MenuItem)
        .unwrap()
        .id;
    ui.set_badge(
        menu,
        Some(BadgeSettings {
            content: BadgeContent::Text("OVERLAY".into()),
            anchor: BadgeAnchor::Center,
            ..Default::default()
        }),
    )
    .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    assert!(ui.base_scene().texts.iter().any(|run| run.text == "BASE"));
    assert!(
        !ui.base_scene()
            .texts
            .iter()
            .any(|run| run.text == "OVERLAY")
    );
    assert!(
        ui.overlay_scene()
            .texts
            .iter()
            .any(|run| run.text == "OVERLAY")
    );
    let mut props = ui.runtime.tree().node(parent).unwrap().props.clone();
    props.scroll = Point::new(0.0, 500.0);
    props.clip = true;
    ui.runtime
        .apply(Mutation::SetProps {
            node: parent,
            props,
        })
        .unwrap();
    ui.update(Size::new(400.0, 300.0)).unwrap();
    assert!(ui.badge_bounds(owner).is_none());
    assert!(!ui.base_scene().texts.iter().any(|run| run.text == "BASE"));
}

#[test]
fn badge_count_binding_is_exact_atomic_and_never_writes_back() {
    use crate::{
        BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings,
        DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, _, owner) = fixture();
    ui.set_badge(owner, Some(badge(3))).unwrap();
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Count",
            DataValue::from("18446744073709551615"),
        )]))
        .unwrap(),
    ));
    let mut bindings = ControlBindings::new(context.clone());
    let mut binding = ControlBinding::new(owner, BindingProperty::BadgeCount, "Count").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    assert!(bindings.add(binding.clone()).is_err());
    binding.mode = ControlBindingMode::OneWayToSource;
    assert!(bindings.add(binding.clone()).is_err());
    binding.mode = ControlBindingMode::OneWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(
        ui.badge(owner).unwrap().content,
        BadgeContent::Count(u64::MAX)
    );
    for invalid in [
        DataValue::Number(1.0),
        DataValue::from("01"),
        DataValue::from("18446744073709551616"),
        DataValue::Null,
    ] {
        context.borrow_mut().set("Count", invalid).unwrap();
        bindings.sync(&mut ui);
        assert_eq!(
            ui.badge(owner).unwrap().content,
            BadgeContent::Count(u64::MAX)
        );
        assert_eq!(bindings.diagnostics().len(), 1);
    }
    bindings.drain_diagnostics();
    context
        .borrow_mut()
        .set("Count", DataValue::from("4"))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.badge(owner).unwrap().content, BadgeContent::Count(4));
    assert!(bindings.diagnostics().is_empty());
    ui.set_badge(owner, Some(badge(7))).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        context.borrow().get("Count").unwrap(),
        &DataValue::from("4")
    );
}

#[test]
fn badge_attachment_capacity_rejects_only_the_new_attachment() {
    let (mut ui, _, owner) = fixture();
    ui.set_badge(owner, Some(badge(1))).unwrap();
    for _ in 1..MAX_BADGES {
        let id = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
        ui.set_badge(id, Some(badge(1))).unwrap();
    }
    let extra = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    assert!(ui.set_badge(extra, Some(badge(2))).is_err());
    assert!(ui.badge(extra).is_none());
    ui.set_badge(owner, Some(badge(2))).unwrap();
    assert_eq!(ui.badge(owner).unwrap().content, BadgeContent::Count(2));
    ui.set_badge(owner, None).unwrap();
    ui.set_badge(extra, Some(badge(2))).unwrap();
    assert_eq!(ui.badges.len(), MAX_BADGES);
}

#[test]
fn badge_scene_scaling_preserves_owner_anchor_clip_and_logical_input() {
    let (mut ui, _, owner) = fixture();
    let owner_bounds = ui.runtime.bounds(owner).unwrap();
    let logical_point = Point::new(owner_bounds.x + 8.0, owner_bounds.y + 8.0);
    let tree_count = ui.runtime.tree().len();
    for anchor in [
        BadgeAnchor::TopLeft,
        BadgeAnchor::TopRight,
        BadgeAnchor::BottomLeft,
        BadgeAnchor::BottomRight,
        BadgeAnchor::Center,
    ] {
        for clip in [BadgeClip::Parent, BadgeClip::Owner] {
            let settings = BadgeSettings {
                anchor,
                clip,
                offset: Point::new(2.0, 1.0),
                ..badge(7)
            };
            ui.set_badge(owner, Some(settings)).unwrap();
            ui.update(Size::new(400.0, 300.0)).unwrap();
            let badge_bounds = ui.badge_bounds(owner).unwrap();
            let logical = ui.scene().clone();
            let surface = logical
                .rounded_rectangles
                .iter()
                .find(|r| r.bounds == badge_bounds && r.stroke_width.is_none())
                .unwrap();
            let label = logical.texts.iter().find(|r| r.text == "7").unwrap();
            let (anchor_x, anchor_y) = match anchor {
                BadgeAnchor::TopLeft => (owner_bounds.x, owner_bounds.y),
                BadgeAnchor::TopRight => (owner_bounds.x + owner_bounds.width, owner_bounds.y),
                BadgeAnchor::BottomLeft => (owner_bounds.x, owner_bounds.y + owner_bounds.height),
                BadgeAnchor::BottomRight => (
                    owner_bounds.x + owner_bounds.width,
                    owner_bounds.y + owner_bounds.height,
                ),
                BadgeAnchor::Center => (
                    owner_bounds.x + owner_bounds.width * 0.5,
                    owner_bounds.y + owner_bounds.height * 0.5,
                ),
            };
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let offset = Point::new(11.0, 13.0);
                let transform = |r: Rect| {
                    Rect::new(
                        r.x * scale + offset.x,
                        r.y * scale + offset.y,
                        r.width * scale,
                        r.height * scale,
                    )
                };
                let mut physical = Scene::default();
                physical
                    .append_transformed_view(
                        &logical,
                        offset,
                        scale,
                        Rect::new(0.0, 0.0, 900.0, 700.0),
                    )
                    .unwrap();
                let shape = physical
                    .rounded_rectangles
                    .iter()
                    .find(|r| r.bounds == transform(badge_bounds) && r.stroke_width.is_none())
                    .unwrap();
                assert_eq!(shape.clip, transform(surface.clip));
                assert_eq!(shape.radius, surface.radius * scale);
                assert!(
                    (shape.bounds.x + shape.bounds.width * 0.5
                        - ((anchor_x + 2.0) * scale + offset.x))
                        .abs()
                        < 0.001
                );
                assert!(
                    (shape.bounds.y + shape.bounds.height * 0.5
                        - ((anchor_y + 1.0) * scale + offset.y))
                        .abs()
                        < 0.001
                );
                let text = physical.texts.iter().find(|r| r.text == "7").unwrap();
                assert_eq!(text.bounds, transform(label.bounds));
                assert_eq!(text.clip, transform(label.clip));
                assert_eq!(text.font_size, label.font_size * scale);
                assert_eq!(ui.runtime.bounds(owner), Some(owner_bounds));
                assert_eq!(ui.runtime.hit_test(logical_point), Some(owner));
                assert_eq!(ui.runtime.tree().len(), tree_count);
            }
        }
    }
}
