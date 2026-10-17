use super::tiles_layout::TileGeometry;
use super::*;
use rust_desktop_ui_core::Modifiers;

fn model(count: usize) -> TileModel {
    TileModel::new(
        Vec::new(),
        (0..count)
            .map(|i| TileItem::new(i.to_string(), format!("Tile {i}")))
            .collect(),
    )
    .unwrap()
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(900.0, 700.0)).unwrap();
}
fn fixture(count: usize) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let owner = ui
        .tile_collection(
            ui.root(),
            "Tiles",
            model(count),
            TileSettings::default(),
            LayoutStyle {
                width: Length::Px(620.0),
                height: Length::Px(260.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap()
        .container;
    frame(&mut ui);
    (ui, owner)
}
fn key(ui: &mut Controls, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    })
    .unwrap();
    frame(ui);
}

#[test]
fn tiles_shelves_are_bounded_disjoint_and_retain_clipped_anchor() {
    let mut items = model(35).items().to_vec();
    for (i, item) in items.iter_mut().enumerate() {
        item.size = match i % 4 {
            0 => TileSize::Large,
            1 => TileSize::Medium,
            2 => TileSize::Wide,
            _ => TileSize::Small,
        };
        item.group = if i < 18 { "first" } else { "second" }.into();
    }
    let model = TileModel::new(
        vec![
            TileGroup::new("first", "First"),
            TileGroup::new("second", "Second"),
        ],
        items,
    )
    .unwrap();
    for width in [1.0, 130.0, 620.0, 1000.0] {
        let g = TileGeometry::new(&model, &TileSettings::default(), width).unwrap();
        for (i, a) in g.rects.iter().enumerate() {
            assert!(a.width > 0.0 && a.height > 0.0 && a.x + a.width <= g.width);
            for b in &g.rects[i + 1..] {
                assert!(a.intersection(*b).is_none());
            }
        }
        let anchor = g.anchor(&model, g.shelves[3].y + 13.0).unwrap();
        let resized = TileGeometry::new(&model, &TileSettings::default(), 480.0).unwrap();
        let position = resized.positions[&anchor.key];
        let rect = resized.rects[resized.order[position]];
        assert_eq!(resized.restore(Some(&anchor)), rect.y + 13.0);
    }
    assert!(
        TileGeometry::new(
            &model,
            &TileSettings {
                cell_height: f32::NAN,
                ..Default::default()
            },
            500.0
        )
        .is_err()
    );
}
#[test]
fn tiles_hundred_thousand_only_realize_visible_and_never_transfer_identity() {
    let (mut ui, owner) = fixture(100_000);
    let first = ui.tile_item_widget(owner, "0").unwrap();
    let lease = ui.tile_item_lease(owner, "0").unwrap();
    let retained = ui.tile_item_widget(owner, "4").unwrap();
    ui.scroll_to(owner, 60.0).unwrap();
    frame(&mut ui);
    assert_eq!(ui.tile_item_widget(owner, "4"), Some(retained));
    for key in ["50000", "99999", "10", "0"] {
        ui.focus_tile(owner, key).unwrap();
        frame(&mut ui);
        let stats = ui.tile_stats(owner).unwrap();
        assert!(stats.realized <= 28, "{stats:?}");
        assert!(ui.controls.len() < 220);
        assert_eq!(stats.created - stats.released, stats.realized as u64);
    }
    assert_eq!(ui.resolve_tile_lease(owner, lease), None);
    assert!(!ui.runtime.tree().contains(first));
    assert_ne!(ui.tile_item_widget(owner, "0"), Some(first));
    ui.perform_action(first, SemanticAction::Activate, None)
        .unwrap();
    assert!(ui.drain_tile_events().is_empty());
}
#[test]
fn tiles_actual_resize_preserves_selection_anchor_and_visible_widget() {
    let (mut ui, owner) = fixture(300);
    ui.set_tile_selected_keys(owner, vec!["40".into()]).unwrap();
    let y = ui.tile_item_bounds(owner, "40").unwrap().y;
    ui.scroll_to(owner, y + 17.0).unwrap();
    frame(&mut ui);
    let row = ui.tile_item_widget(owner, "40").unwrap();
    ui.edit_props(owner, |p| p.style.width = Length::Px(350.0))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.tile_selected_keys(owner).unwrap(), ["40"]);
    assert_eq!(ui.tile_item_widget(owner, "40"), Some(row));
    assert_eq!(
        ui.scroll_offset(owner).unwrap().y,
        ui.tile_item_bounds(owner, "40").unwrap().y + 17.0
    );
    let actual = ui.runtime.bounds(row).unwrap();
    let viewport = ui.runtime.bounds(owner).unwrap();
    assert_eq!(actual.y, viewport.y - 17.0);
    assert!(actual.width > 0.0 && actual.x + actual.width <= viewport.x + viewport.width);
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    assert_eq!(ui.tile_item_widget(owner, "40"), Some(row));
}
#[test]
fn tiles_selection_and_invoke_are_distinct_and_keyboard_repeat_is_suppressed() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("open", "Open");
    let mut item = TileItem::new("a", "Alpha");
    item.command = Some("open".into());
    let owner = ui
        .tile_collection(
            ui.root(),
            "Tiles",
            TileModel::new(Vec::new(), vec![item]).unwrap(),
            Default::default(),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(200.0),
                ..Default::default()
            },
        )
        .unwrap()
        .container;
    frame(&mut ui);
    let row = ui.tile_item_widget(owner, "a").unwrap();
    let invoke = ui.tile_invoke_widget(owner, "a").unwrap();
    ui.perform_action(row, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(ui.tile_selected_keys(owner).unwrap(), ["a"]);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    ui.perform_action(invoke, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    ui.focus_tile(owner, "a").unwrap();
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: true,
    })
    .unwrap();
    assert!(ui.commands_mut().drain_invocations().is_empty());
    let sem = ui.semantics();
    assert_eq!(
        sem.iter().find(|n| n.id == row).unwrap().position_in_set,
        Some(1)
    );
    assert_eq!(
        sem.iter().find(|n| n.id == invoke).unwrap().role,
        Role::Button
    );
}
#[test]
fn tiles_reorder_validators_and_disabled_items_are_atomic() {
    let (mut ui, owner) = fixture(20);
    ui.focus_tile(owner, "1").unwrap();
    let row = ui.tile_item_widget(owner, "1").unwrap();
    key(
        &mut ui,
        Key::ArrowRight,
        Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(ui.tile_model(owner).unwrap().items()[2].item.key, "1");
    assert_eq!(ui.tile_item_widget(owner, "1"), Some(row));
    let before = ui.tile_model(owner).unwrap().clone();
    let nodes = ui.runtime.tree().preorder();
    let offset = ui.scroll_offset(owner);
    ui.set_tile_model_validator(
        owner,
        Some(TileModelValidator::new(|_| Err("Locked order".into()))),
    )
    .unwrap();
    assert!(ui.move_tile(owner, "1", "", Some("0")).is_err());
    assert_eq!(ui.tile_model(owner), Some(&before));
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.scroll_offset(owner), offset);
    ui.set_tile_selection_validator(
        owner,
        Some(SelectionValidator::new(|_| Err("Denied".into()))),
    )
    .unwrap();
    let button = ui.tile_invoke_widget(owner, "1").unwrap();
    ui.activate(button).unwrap();
    assert!(ui.tile_selected_keys(owner).unwrap().is_empty());
    assert_eq!(ui.tile_error(owner), Some("Denied"));
    let mut settings = ui.tile_settings(owner).unwrap().clone();
    settings.is_read_only = true;
    ui.set_tile_settings(owner, settings).unwrap();
    assert!(ui.invoke_tile(owner, "1").is_err());
    frame(&mut ui);
    assert!(
        !ui.semantics()
            .iter()
            .find(|s| s.id == button)
            .unwrap()
            .actions
            .contains(&SemanticAction::Activate)
    );
}
#[test]
fn tiles_template_updates_same_key_without_stale_text_and_bad_candidate_is_atomic() {
    let (mut ui, owner) = fixture(5);
    let row = ui.tile_item_widget(owner, "0").unwrap();
    let template = ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle::default(),
            kind: ListTemplateKind::Text {
                source: ListItemTextSource::Text,
                style: TextStyle::default(),
            },
        },
    };
    ui.set_tile_item_template(owner, Some(template.clone()))
        .unwrap();
    frame(&mut ui);
    let descendants = ui
        .runtime
        .tree()
        .preorder()
        .into_iter()
        .filter(|id| ui.flyout_descendant(*id, row))
        .collect::<Vec<_>>();
    let mut items = ui.tile_model(owner).unwrap().items().to_vec();
    items[0].item.text = "Updated".into();
    ui.set_tile_model(owner, TileModel::new(Vec::new(), items).unwrap())
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.tile_item_widget(owner, "0"), Some(row));
    assert_eq!(
        descendants,
        ui.runtime
            .tree()
            .preorder()
            .into_iter()
            .filter(|id| ui.flyout_descendant(*id, row))
            .collect::<Vec<_>>()
    );
    assert!(
        descendants
            .iter()
            .any(|id| ui.controls.get(id).is_some_and(|c| c.label == "Updated"))
    );
    let nodes = ui.runtime.tree().preorder();
    let mut bad = template;
    bad.root.layout.gap = f32::NAN;
    assert!(ui.set_tile_item_template(owner, Some(bad)).is_err());
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    let mut baditems = ui.tile_model(owner).unwrap().items().to_vec();
    baditems[0].item.text = format!("A{}", "\u{301}".repeat(100));
    let old = ui.tile_model(owner).unwrap().clone();
    assert!(
        ui.set_tile_model(owner, TileModel::new(Vec::new(), baditems).unwrap())
            .is_err()
    );
    assert_eq!(ui.tile_model(owner), Some(&old));
    assert_eq!(ui.runtime.tree().preorder(), nodes);
}
#[test]
fn tiles_removed_nested_template_and_header_reject_before_publication() {
    for (remove_header, reparent) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let mut item = TileItem::new("a", "Alpha");
        item.group = "g".into();
        let owner = ui
            .tile_collection(
                ui.root(),
                "Tiles",
                TileModel::new(vec![TileGroup::new("g", "Group")], vec![item]).unwrap(),
                Default::default(),
                LayoutStyle {
                    width: Length::Px(400.0),
                    height: Length::Px(250.0),
                    ..Default::default()
                },
            )
            .unwrap()
            .container;
        frame(&mut ui);
        let row = ui.tile_item_widget(owner, "a").unwrap();
        let lease = ui.tile_item_lease(owner, "a");
        let previous = ui.tile_model(owner).unwrap().clone();
        let target = ui
            .controls
            .iter()
            .find(|(id, c)| {
                if remove_header {
                    c.label == "Group"
                } else {
                    c.label == "Alpha" && ui.flyout_descendant(**id, row)
                }
            })
            .map(|(id, _)| *id)
            .unwrap();
        if reparent {
            let other = ui
                .transparent_panel(ui.root(), LayoutStyle::default())
                .unwrap();
            ui.runtime
                .apply(Mutation::Reparent {
                    node: target,
                    parent: other,
                })
                .unwrap();
        } else {
            ui.remove(target).unwrap();
        }
        let tree = ui.runtime.tree().preorder();
        let mut replacement = previous.items().to_vec();
        replacement[0].item.text = "Changed".into();
        assert!(
            ui.set_tile_model(
                owner,
                TileModel::new(previous.groups().to_vec(), replacement).unwrap()
            )
            .is_err()
        );
        assert_eq!(ui.tile_model(owner), Some(&previous));
        assert_eq!(ui.tile_item_lease(owner, "a"), lease);
        assert_eq!(ui.runtime.tree().preorder(), tree);
        ui.remove(owner).unwrap();
        frame(&mut ui);
        assert!(ui.tile_model(owner).is_none());
    }
}
#[test]
fn tiles_binding_source_veto_and_nullable_keys_are_atomic() {
    use crate::{
        BindingContext, BindingProperty, BindingValidator, ControlBinding, ControlBindingMode,
        ControlBindings, DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, owner) = fixture(8);
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Selected", DataValue::Null)])).unwrap(),
    ));
    context
        .borrow_mut()
        .set_field_validator(
            "Selected",
            Some(BindingValidator::new(|value| {
                if *value == DataValue::String("3".into()) {
                    Err("Denied".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let mut bindings = ControlBindings::new(context.clone());
    let mut binding = ControlBinding::new(owner, BindingProperty::SelectedKey, "Selected").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.invoke_tile(owner, "2").unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        context.borrow().get("Selected").unwrap(),
        &DataValue::String("2".into())
    );
    let events = ui.drain_tile_events();
    assert_eq!(events.len(), 2);
    assert!(ui.invoke_tile(owner, "3").is_err());
    bindings.sync(&mut ui);
    assert!(ui.drain_tile_events().is_empty());
    assert_eq!(ui.tile_selected_keys(owner).unwrap(), ["2"]);
    context
        .borrow_mut()
        .set("Selected", DataValue::Null)
        .unwrap();
    bindings.sync(&mut ui);
    assert!(ui.tile_selected_keys(owner).unwrap().is_empty());
    let encoded = tile_items_to_value(ui.tile_model(owner).unwrap().items());
    assert_eq!(
        tile_items_from_value(&encoded).unwrap(),
        ui.tile_model(owner).unwrap().items()
    );
}

#[test]
fn tiles_home_end_skip_disabled_boundary_items_and_cursor_ring_is_not_covered() {
    let (mut ui, owner) = fixture(8);
    let mut rows = ui.tile_model(owner).unwrap().items().to_vec();
    rows[0].item.enabled = false;
    rows[7].item.enabled = false;
    ui.set_tile_model(owner, TileModel::new(Vec::new(), rows).unwrap())
        .unwrap();
    ui.focus_tile(owner, "3").unwrap();
    key(&mut ui, Key::Home, Modifiers::default());
    assert_eq!(ui.tile_cursor(owner), Some("1"));
    key(&mut ui, Key::End, Modifiers::default());
    assert_eq!(ui.tile_cursor(owner), Some("6"));
    let row = ui.tile_item_widget(owner, "6").unwrap();
    let button = ui.tile_invoke_widget(owner, "6").unwrap();
    let a = ui.runtime.bounds(row).unwrap();
    let b = ui.runtime.bounds(button).unwrap();
    assert_eq!(ui.runtime.focused(), Some(row));
    assert!(
        a.x < b.x && a.y < b.y && b.x + b.width < a.x + a.width && b.y + b.height < a.y + a.height
    );
    assert!(ui.tile_selected_keys(owner).unwrap().is_empty());
    assert!(ui.drain_tile_events().is_empty());
}
