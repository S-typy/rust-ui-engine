use super::*;
use rust_desktop_ui_core::Modifiers;
fn model(count: usize) -> GalleryModel {
    GalleryModel::new(
        Vec::new(),
        (0..count)
            .map(|i| GalleryItem::new(i.to_string(), format!("Образец {i}")))
            .collect(),
    )
    .unwrap()
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(1000.0, 760.0)).unwrap();
}
fn fixture(count: usize) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let g = ui
        .gallery(
            ui.root(),
            "Образцы",
            model(count),
            GallerySettings::default(),
            LayoutStyle {
                width: Length::Px(260.0),
                height: Length::Px(210.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, g.container)
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(ui);
}
fn point(ui: &Controls, id: WidgetId) -> Point {
    let r = ui.runtime.bounds(id).unwrap();
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}
#[test]
fn gallery_preview_never_accepts_and_keyboard_accepts_exactly_once() {
    let (mut ui, id) = fixture(12);
    ui.set_gallery_selected_key(id, Some("0".into())).unwrap();
    let first = ui.gallery_item_widget(id, "0").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(first))).unwrap();
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.gallery_preview_key(id), Some("1"));
    assert_eq!(ui.gallery_selected_key(id), Some("0"));
    assert!(ui.drain_gallery_selection_changes().is_empty());
    key(&mut ui, Key::Enter);
    assert_eq!(ui.gallery_selected_key(id), Some("1"));
    assert_eq!(ui.drain_gallery_selection_changes().len(), 1);
    assert!(ui.gallery_preview_key(id).is_none());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: true,
    })
    .unwrap();
    assert!(ui.drain_gallery_selection_changes().is_empty());
    key(&mut ui, Key::End);
    assert_eq!(ui.gallery_preview_key(id), Some("11"));
    assert!(ui.scroll_offset(id).unwrap().y > 0.0);
    let row = ui.gallery_item_widget(id, "11").unwrap();
    assert!(
        ui.runtime
            .bounds(row)
            .unwrap()
            .intersection(ui.runtime.clip(row).unwrap())
            .is_some()
    );
    key(&mut ui, Key::Escape);
    assert_eq!(ui.gallery_selected_key(id), Some("1"));
    assert!(ui.gallery_preview_key(id).is_none());
}
#[test]
fn gallery_pointer_preview_cancel_veto_and_source_epoch_are_atomic() {
    let (mut ui, id) = fixture(4);
    let row = ui.gallery_item_widget(id, "1").unwrap();
    let pos = point(&ui, row);
    ui.dispatch(InputEvent::PointerMoved { position: pos })
        .unwrap();
    assert_eq!(ui.gallery_preview_key(id), Some("1"));
    assert_eq!(ui.gallery_selected_key(id), None);
    ui.set_gallery_selection_validator(
        id,
        Some(SelectionValidator::new(|_| Err("Запрещено".into()))),
    )
    .unwrap();
    for event in [
        InputEvent::PointerDown {
            position: pos,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        InputEvent::PointerUp {
            position: pos,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    ] {
        ui.dispatch(event).unwrap();
    }
    assert_eq!(ui.gallery_selected_key(id), None);
    assert_eq!(ui.gallery_error(id), Some("Запрещено"));
    assert!(ui.drain_gallery_selection_changes().is_empty());
    ui.set_gallery_selection_validator(id, None).unwrap();
    ui.dispatch(InputEvent::PointerDown {
        position: pos,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.set_gallery_model(id, model(4)).unwrap();
    frame(&mut ui);
    ui.dispatch(InputEvent::PointerUp {
        position: pos,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.gallery_selected_key(id), None);
    ui.dispatch(InputEvent::PointerLeft).unwrap();
    assert_eq!(ui.gallery_preview_key(id), None);
}
#[test]
fn gallery_model_reorder_retains_identity_and_invalid_candidate_leaves_tree() {
    let (mut ui, id) = fixture(6);
    ui.set_gallery_selected_key(id, Some("2".into())).unwrap();
    let row = ui.gallery_item_widget(id, "2").unwrap();
    let mut items = model(6).items().to_vec();
    items.reverse();
    ui.set_gallery_model(id, GalleryModel::new(Vec::new(), items).unwrap())
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.gallery_item_widget(id, "2"), Some(row));
    assert_eq!(ui.gallery_selected_key(id), Some("2"));
    let old = ui.gallery_model(id).unwrap().clone();
    let nodes = ui.runtime.tree().preorder();
    let mut items = old.items().to_vec();
    items[0].item.text = "a".to_owned() + &"\u{301}".repeat(40);
    assert!(
        ui.set_gallery_model(id, GalleryModel::new(Vec::new(), items).unwrap())
            .is_err()
    );
    assert_eq!(ui.gallery_model(id), Some(&old));
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.gallery_item_widget(id, "2"), Some(row));
    assert!(
        ui.set_gallery_settings(
            id,
            GallerySettings {
                sample_width: f32::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(GalleryModel::new(Vec::new(), vec![GalleryItem::new("x", "x"); 257]).is_err());
}
#[test]
fn gallery_popup_transfer_returns_home_and_preserves_ids_selected_and_focus() {
    let (mut ui, id) = fixture(18);
    let anchor = ui
        .button(
            ui.root(),
            "Открыть",
            None,
            LayoutStyle {
                height: Length::Px(32.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.set_gallery_selected_key(id, Some("15".into())).unwrap();
    let row = ui.gallery_item_widget(id, "15").unwrap();
    let popup = ui
        .configure_gallery_popup(id, anchor, Size::new(380.0, 290.0))
        .unwrap();
    ui.open_gallery_popup(id).unwrap();
    frame(&mut ui);
    assert!(ui.flyout_is_open(popup));
    assert_eq!(ui.runtime.tree().parent(id), Some(popup.content));
    assert_eq!(ui.gallery_item_widget(id, "15"), Some(row));
    assert_eq!(ui.runtime.focused(), Some(row));
    ui.preview_gallery_item(id, "14").unwrap();
    ui.close_gallery_popup(id).unwrap();
    frame(&mut ui);
    assert!(!ui.flyout_is_open(popup));
    assert_eq!(ui.runtime.tree().parent(id), Some(ui.root()));
    assert_eq!(ui.gallery_selected_key(id), Some("15"));
    assert_eq!(ui.gallery_preview_key(id), None);
    assert_eq!(ui.runtime.focused(), Some(anchor));
    ui.open_gallery_popup(id).unwrap();
    ui.set_enabled(anchor, false).unwrap();
    frame(&mut ui);
    assert!(!ui.flyout_is_open(popup));
    assert_eq!(ui.runtime.tree().parent(id), Some(ui.root()));
    assert_ne!(ui.runtime.focused(), Some(row));
}
#[test]
fn gallery_host_preflight_and_required_part_guards_are_atomic() {
    let (mut ui, id) = fixture(4);
    let row = ui.gallery_item_widget(id, "0").unwrap();
    let parent = ui.runtime.tree().parent(id);
    let tree = ui.runtime.tree().preorder();
    assert!(
        ui.set_gallery_host(id, row, LayoutStyle::default())
            .is_err()
    );
    assert_eq!(ui.runtime.tree().parent(id), parent);
    assert_eq!(ui.runtime.tree().preorder(), tree);
    assert!(ui.remove(row).is_err());
    let content = ui.gallery_parts(id).unwrap().content;
    ui.runtime
        .apply(Mutation::Reparent {
            node: row,
            parent: ui.root(),
        })
        .unwrap();
    assert!(ui.set_gallery_selected_key(id, Some("0".into())).is_err());
    assert_eq!(ui.gallery_selected_key(id), None);
    ui.runtime
        .apply(Mutation::Reparent {
            node: row,
            parent: content,
        })
        .unwrap();
    frame(&mut ui);
}
#[test]
fn gallery_ribbon_content_and_flyout_share_real_retained_samples() {
    use crate::{Ribbon, RibbonContentSpec, RibbonGroup, RibbonModel, RibbonTab};
    let (mut ui, id) = fixture(8);
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Главная".into(),
            key_tip: "H".into(),
            context: None,
            groups: vec![RibbonGroup {
                id: "samples".into(),
                label: "Образцы".into(),
                collapse_priority: 0,
                items: Vec::new(),
            }],
        }],
        quick_access: Vec::new(),
    })
    .unwrap();
    ribbon
        .add_content(RibbonContentSpec {
            id: "gallery".into(),
            tab_id: "home".into(),
            group_id: "samples".into(),
            label: "Галерея".into(),
            key_tip: "G".into(),
            preferred_width: 400.0,
            minimum_width: 160.0,
            height: 84.0,
        })
        .unwrap();
    let root = ui.root();
    ribbon.mount_with_width(&mut ui, root, 960.0).unwrap();
    frame(&mut ui);
    let host = ribbon.content_host("gallery").unwrap();
    ui.set_gallery_selected_key(id, Some("5".into())).unwrap();
    let row = ui.gallery_item_widget(id, "5").unwrap();
    ui.set_gallery_host(
        id,
        host,
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.tree().parent(id), Some(host));
    assert_eq!(ui.gallery_item_widget(id, "5"), Some(row));
    assert_eq!(ui.gallery_selected_key(id), Some("5"));
    let anchor = ui
        .button(
            root,
            "Ещё",
            None,
            LayoutStyle {
                height: Length::Px(32.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.configure_gallery_popup(id, anchor, Size::new(420.0, 280.0))
        .unwrap();
    ui.open_gallery_popup(id).unwrap();
    frame(&mut ui);
    ui.close_gallery_popup(id).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.tree().parent(id), Some(host));
    assert_eq!(ui.gallery_item_widget(id, "5"), Some(row));
}
#[test]
fn gallery_hidden_disabled_and_removed_popup_owner_cancel_preview() {
    let (mut ui, id) = fixture(4);
    let anchor = ui
        .button(
            ui.root(),
            "Popup",
            None,
            LayoutStyle {
                height: Length::Px(32.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    let popup = ui
        .configure_gallery_popup(id, anchor, Size::new(380.0, 250.0))
        .unwrap();
    ui.open_gallery_popup(id).unwrap();
    ui.preview_gallery_item(id, "1").unwrap();
    ui.remove(anchor).unwrap();
    frame(&mut ui);
    assert!(ui.gallery_parts(id).is_none());
    assert!(!ui.runtime.tree().contains(id));
    assert!(!ui.runtime.tree().contains(popup.container));
    assert!(
        ui.runtime
            .focused()
            .is_none_or(|f| ui.runtime.tree().contains(f))
    );
}
#[test]
fn gallery_binding_publishes_only_accepted_key_and_rejects_invalid_source() {
    use crate::{
        BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings,
        DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, id) = fixture(3);
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Selected", DataValue::from("0"))])).unwrap(),
    ));
    let mut engine = ControlBindings::new(context.clone());
    let mut binding = ControlBinding::new(id, BindingProperty::SelectedKey, "Selected").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    engine.add(binding).unwrap();
    engine.initialize(&mut ui).unwrap();
    ui.preview_gallery_item(id, "1").unwrap();
    engine.sync(&mut ui);
    assert_eq!(
        context.borrow().get("Selected").unwrap(),
        &DataValue::from("0")
    );
    ui.accept_gallery_preview(id).unwrap();
    engine.sync(&mut ui);
    assert_eq!(
        context.borrow().get("Selected").unwrap(),
        &DataValue::from("1")
    );
    context
        .borrow_mut()
        .set("Selected", "unknown".into())
        .unwrap();
    engine.sync(&mut ui);
    assert_eq!(ui.gallery_selected_key(id), Some("1"));
    assert!(!engine.diagnostics().is_empty());
}

#[test]
fn gallery_groups_resize_readonly_and_semantics_preserve_accepted_identity() {
    let (mut ui, id) = fixture(9);
    let mut items = model(9).items().to_vec();
    for (n, i) in items.iter_mut().enumerate() {
        i.group = if n < 4 { "a" } else { "b" }.into();
    }
    ui.set_gallery_model(
        id,
        GalleryModel::new(
            vec![
                GalleryGroup::new("a", "Первая"),
                GalleryGroup::new("b", "Вторая"),
            ],
            items,
        )
        .unwrap(),
    )
    .unwrap();
    ui.set_gallery_selected_key(id, Some("8".into())).unwrap();
    frame(&mut ui);
    let row = ui.gallery_item_widget(id, "8").unwrap();
    for width in [500.0, 180.0, 70.0, 320.0] {
        ui.edit_props(id, |p| p.style.width = Length::Px(width))
            .unwrap();
        frame(&mut ui);
        let r = ui.runtime.bounds(row).unwrap();
        let clip = ui.runtime.clip(row).unwrap();
        assert!(r.width <= width);
        assert!(r.intersection(clip).is_some());
        assert_eq!(ui.gallery_selected_key(id), Some("8"));
    }
    let mut settings = ui.gallery_settings(id).unwrap().clone();
    settings.is_read_only = true;
    ui.set_gallery_settings(id, settings).unwrap();
    frame(&mut ui);
    ui.preview_gallery_item(id, "0").unwrap();
    assert!(ui.accept_gallery_preview(id).is_err());
    ui.activate(ui.gallery_item_widget(id, "0").unwrap())
        .unwrap();
    assert_eq!(ui.gallery_selected_key(id), Some("8"));
    assert!(ui.drain_gallery_selection_changes().is_empty());
    let semantics = ui.semantics();
    let sample = semantics.iter().find(|n| n.id == row).unwrap();
    assert!(sample.selected);
    assert!(sample.read_only);
    assert_eq!(sample.position_in_set, Some(9));
    assert!(!sample.actions.contains(&SemanticAction::Activate));
    ui.edit_props(id, |p| p.visible = false).unwrap();
    frame(&mut ui);
    assert!(ui.gallery_preview_key(id).is_none());
    ui.edit_props(id, |p| p.visible = true).unwrap();
    frame(&mut ui);
    assert_eq!(ui.gallery_item_widget(id, "8"), Some(row));
}
#[test]
fn gallery_images_templates_and_failed_source_pruning_are_atomic() {
    use rust_desktop_ui_images::{ImageFormat, ImageSource};
    let (mut ui, id) = fixture(3);
    let svg=ImageSource::from_bytes(br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#3080c0"/></svg>"##.to_vec(),ImageFormat::Svg).unwrap();
    ui.register_image_source("sample", svg).unwrap();
    let mut items = model(3).items().to_vec();
    items[0].image_resource = Some("sample".into());
    items[1].icon = Some(IconSymbol::Check);
    ui.set_gallery_model(id, GalleryModel::new(Vec::new(), items).unwrap())
        .unwrap();
    frame(&mut ui);
    ui.set_gallery_selected_key(id, Some("0".into())).unwrap();
    let row = ui.gallery_item_widget(id, "0").unwrap();
    let template = ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle::default(),
            kind: ListTemplateKind::Text {
                source: ListItemTextSource::Key,
                style: TextStyle {
                    font_weight: Some(600),
                    ..Default::default()
                },
            },
        },
    };
    ui.set_gallery_item_template(id, Some(template)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.gallery_item_widget(id, "0"), Some(row));
    ui.set_gallery_selection_validator(
        id,
        Some(SelectionValidator::new(|keys| {
            if keys.is_empty() {
                Err("Выбор обязателен".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    let old = ui.gallery_model(id).unwrap().clone();
    let nodes = ui.runtime.tree().preorder();
    let mut items = old.items().to_vec();
    items[0].item.enabled = false;
    assert!(
        ui.set_gallery_model(id, GalleryModel::new(Vec::new(), items).unwrap())
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.gallery_model(id), Some(&old));
    let mut items = old.items().to_vec();
    items[0].image_resource = Some("absent".into());
    assert!(
        ui.set_gallery_model(id, GalleryModel::new(Vec::new(), items).unwrap())
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
}
#[test]
fn gallery_retained_source_validator_expires_and_user_queue_is_bounded() {
    let (mut ui, id) = fixture(2);
    let validator = ui.gallery_source_validator(id).unwrap();
    for n in 0..256 {
        ui.preview_gallery_item(id, &(n % 2).to_string()).unwrap();
        ui.accept_gallery_preview(id).unwrap();
    }
    ui.preview_gallery_item(id, "0").unwrap();
    assert!(ui.accept_gallery_preview(id).is_err());
    assert_eq!(ui.gallery_selected_key(id), Some("1"));
    assert_eq!(ui.drain_gallery_selection_changes().len(), 256);
    ui.accept_gallery_preview(id).unwrap();
    assert_eq!(ui.drain_gallery_selection_changes().len(), 1);
    ui.remove(id).unwrap();
    assert!(validator.check(&["0".into()]).is_err());
}

#[test]
fn gallery_template_captions_keep_visible_text_geometry_below_images() {
    use rust_desktop_ui_images::{ImageFormat, ImageSource};
    let (mut ui, id) = fixture(3);
    let image = ImageSource::from_bytes(
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#3080c0"/></svg>"##.to_vec(),
        ImageFormat::Svg,
    ).unwrap();
    ui.register_image_source("caption-image", image).unwrap();
    let mut items = model(3).items().to_vec();
    items[0].image_resource = Some("caption-image".into());
    items[1].icon = Some(IconSymbol::Check);
    ui.set_gallery_model(id, GalleryModel::new(Vec::new(), items).unwrap())
        .unwrap();
    ui.set_gallery_item_template(
        id,
        Some(ListItemTemplate {
            root: ListTemplateNode {
                layout: LayoutStyle::default(),
                kind: ListTemplateKind::Text {
                    source: ListItemTextSource::Text,
                    style: TextStyle {
                        wrap: true,
                        ..Default::default()
                    },
                },
            },
        }),
    )
    .unwrap();
    let first = ui.gallery_item_widget(id, "0").unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        for width in [400.0, 260.0] {
            ui.edit_props(id, |p| p.style.width = Length::Px(width))
                .unwrap();
            frame(&mut ui);
            for index in 0..3 {
                let sample = ui.gallery_item_widget(id, &index.to_string()).unwrap();
                let text = format!("Образец {index}");
                let caption = ui
                    .runtime
                    .tree()
                    .preorder()
                    .into_iter()
                    .find(|node| {
                        ui.flyout_descendant(*node, sample)
                            && ui.label_text(*node) == Some(text.as_str())
                    })
                    .expect("retained template caption");
                let bounds = ui.runtime.bounds(caption).unwrap();
                let visible = bounds.intersection(ui.runtime.clip(caption).unwrap());
                assert!(
                    visible.is_some_and(|r| r.height >= 16.0 && r.width > 40.0),
                    "{theme:?} width={width} caption={bounds:?} clip={:?}",
                    ui.runtime.clip(caption)
                );
                if index < 2 {
                    assert!(
                        bounds.y >= ui.runtime.bounds(sample).unwrap().y + 38.0,
                        "caption must follow the 32px picture and gap: {bounds:?}"
                    );
                }
                assert!(
                    ui.scene().texts.iter().any(|run| run.text == text
                        && run
                            .bounds
                            .intersection(run.clip)
                            .is_some_and(|r| r.height >= run.font_size)),
                    "caption must reach the painted Scene: {text}"
                );
            }
            assert_eq!(ui.gallery_item_widget(id, "0"), Some(first));
        }
    }
}
