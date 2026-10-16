use super::*;
use rust_desktop_ui_core::Modifiers;
fn items() -> Vec<SelectionItem> {
    (0..100)
        .map(|index| {
            let mut item = SelectionItem::new(format!("k{index}"), format!("Item {index}"));
            item.columns = vec![format!("Detail {index}")];
            item.enabled = index != 2;
            item
        })
        .collect()
}
fn setup(mode: SelectionMode) -> (Controls, ListBox) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let list = ui
        .list_box(
            root,
            "Values",
            items(),
            SelectionSettings {
                mode,
                read_only: false,
            },
            LayoutStyle {
                width: Length::Px(300.0),
                height: Length::Px(180.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, list)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 400.0)).unwrap();
}
fn template() -> ListItemTemplate {
    ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                gap: 2.0,
                ..Default::default()
            },
            kind: ListTemplateKind::Stack {
                orientation: Axis::Vertical,
                children: vec![
                    ListTemplateNode {
                        layout: LayoutStyle {
                            height: Length::Px(22.0),
                            ..Default::default()
                        },
                        kind: ListTemplateKind::Text {
                            source: ListItemTextSource::Text,
                            style: TextStyle {
                                font_weight: Some(600),
                                wrap: false,
                                ..Default::default()
                            },
                        },
                    },
                    ListTemplateNode {
                        layout: LayoutStyle {
                            height: Length::Px(20.0),
                            ..Default::default()
                        },
                        kind: ListTemplateKind::Text {
                            source: ListItemTextSource::Column(0),
                            style: TextStyle {
                                tone: TextTone::Secondary,
                                wrap: false,
                                ..Default::default()
                            },
                        },
                    },
                ],
            },
        },
    }
}
fn action(ui: &mut Controls, list: ListBox, key: &str, action: SemanticAction) {
    let row = ui.list_row_widget(list.container, key).unwrap();
    ui.perform_action(row, action, None).unwrap();
    frame(ui);
}
fn key(ui: &mut Controls, list: ListBox, key: Key) {
    ui.runtime
        .apply(Mutation::Focus(Some(list.container)))
        .unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(ui);
}
#[test]
fn checkbox_selection_toggles_without_modifiers_and_arrow_only_moves_cursor() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_item_presentation(list.container, ListItemPresentation::CheckBox)
        .unwrap();
    action(&mut ui, list, "k0", SemanticAction::Toggle);
    action(&mut ui, list, "k1", SemanticAction::Activate);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k0", "k1"]);
    action(&mut ui, list, "k0", SemanticAction::Toggle);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1"]);
    key(&mut ui, list, Key::ArrowDown);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1"]);
    key(&mut ui, list, Key::ArrowDown);
    assert_eq!(ui.rich.lists[&list.container].cursor, Some(3));
    key(&mut ui, list, Key::Space);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1", "k3"]);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: true,
    })
    .unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1", "k3"]);
    let disabled = ui.list_row_widget(list.container, "k2").unwrap();
    ui.perform_action(disabled, SemanticAction::Toggle, None)
        .unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1", "k3"]);
}
#[test]
fn radio_and_checkbox_project_list_hierarchy_selection_position_and_valid_actions() {
    let (mut ui, list) = setup(SelectionMode::Single);
    ui.set_list_item_presentation(list.container, ListItemPresentation::RadioButton)
        .unwrap();
    action(&mut ui, list, "k1", SemanticAction::Activate);
    let row = ui.list_row_widget(list.container, "k1").unwrap();
    let node = ui.semantics().into_iter().find(|n| n.id == row).unwrap();
    assert_eq!(node.role, Role::ListItem);
    assert!(node.selected);
    assert_eq!(node.checked, None);
    assert_eq!(node.position_in_set, Some(2));
    assert_eq!(node.set_size, Some(100));
    assert!(!node.actions.contains(&SemanticAction::Toggle));
    ui.perform_action(row, SemanticAction::Toggle, None)
        .unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1"]);
    key(&mut ui, list, Key::ArrowDown);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k3"]);
    ui.set_list_item_presentation(list.container, ListItemPresentation::Plain)
        .unwrap();
    ui.set_selection_settings(
        list.container,
        SelectionSettings {
            mode: SelectionMode::Multiple,
            read_only: false,
        },
    )
    .unwrap();
    ui.set_list_item_presentation(list.container, ListItemPresentation::CheckBox)
        .unwrap();
    frame(&mut ui);
    let row = ui.list_row_widget(list.container, "k3").unwrap();
    let node = ui.semantics().into_iter().find(|n| n.id == row).unwrap();
    assert_eq!(node.checked, Some(true));
    assert!(node.actions.contains(&SemanticAction::Toggle));
    ui.set_selection_settings(
        list.container,
        SelectionSettings {
            mode: SelectionMode::Multiple,
            read_only: true,
        },
    )
    .unwrap();
    let node = ui.semantics().into_iter().find(|n| n.id == row).unwrap();
    assert!(node.read_only);
    assert!(!node.actions.contains(&SemanticAction::Toggle));
    assert!(!node.actions.contains(&SemanticAction::Activate));
}
#[test]
fn same_key_template_children_update_without_replacing_identity_or_selection() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_virtualization(
        list.container,
        ListVirtualizationSettings {
            default_row_height: 56.0,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_list_item_template(list.container, Some(template()))
        .unwrap();
    ui.set_selected_keys(list.container, &["k1".into()])
        .unwrap();
    frame(&mut ui);
    let row = ui.list_row_widget(list.container, "k1").unwrap();
    let content = ui.list_item_content(list.container, "k1").unwrap();
    let children = ui.runtime.tree().children(content).unwrap().to_vec();
    let mut next = items();
    next.swap(0, 1);
    next[0].text = "New title".into();
    next[0].columns[0] = "Updated detail".into();
    ui.set_selection_items(list.container, next).unwrap();
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    assert_eq!(ui.list_row_widget(list.container, "k1"), Some(row));
    assert_eq!(ui.list_item_content(list.container, "k1"), Some(content));
    assert_eq!(ui.runtime.tree().children(content).unwrap(), children);
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k1"]);
    assert!(ui.scene().texts.iter().any(|r| r.text == "New title"));
    assert!(ui.scene().texts.iter().any(|r| r.text == "Updated detail"));
    assert!(!ui.scene().texts.iter().any(|r| r.text == "Detail 1"));
    let node = ui.semantics().into_iter().find(|n| n.id == row).unwrap();
    assert_eq!(node.position_in_set, Some(1));
    let mut next = ui.selection_items(list.container).unwrap().to_vec();
    next[0].columns.clear();
    ui.set_selection_items(list.container, next).unwrap();
    frame(&mut ui);
    assert_eq!(ui.list_item_content(list.container, "k1"), Some(content));
    assert!(!ui.scene().texts.iter().any(|r| r.text == "Updated detail"));
}
#[test]
fn item_template_descendants_never_intercept_pointer_or_take_focus() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_virtualization(
        list.container,
        ListVirtualizationSettings {
            default_row_height: 56.0,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_list_item_presentation(list.container, ListItemPresentation::CheckBox)
        .unwrap();
    ui.set_list_item_template(list.container, Some(template()))
        .unwrap();
    frame(&mut ui);
    let row = ui.list_row_widget(list.container, "k0").unwrap();
    let content = ui.list_item_content(list.container, "k0").unwrap();
    for id in ui.runtime.tree().preorder() {
        if ui.flyout_descendant(id, content) {
            let props = &ui.runtime.tree().node(id).unwrap().props;
            assert!(!props.focusable);
            assert!(!props.hit_test);
        }
    }
    let bounds = ui.runtime.bounds(row).unwrap();
    let point = Point::new(bounds.x + 60.0, bounds.y + 16.0);
    assert_eq!(ui.runtime.hit_test(point), Some(row));
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: rust_desktop_ui_core::PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position: point,
        button: rust_desktop_ui_core::PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k0"]);
}
#[test]
fn invalid_template_presentation_and_height_preserve_hosts_selection_and_cursor() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_item_template(list.container, Some(template()))
        .unwrap();
    action(&mut ui, list, "k1", SemanticAction::Activate);
    let nodes = ui.runtime.tree().preorder();
    let state = ui.rich.lists[&list.container].clone();
    let content = ui.list_item_content(list.container, "k1");
    let mut invalid = template();
    invalid.root.layout.width = Length::Px(f32::NAN);
    assert!(
        ui.set_list_item_template(list.container, Some(invalid))
            .is_err()
    );
    let invalid = ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle::default(),
            kind: ListTemplateKind::Text {
                source: ListItemTextSource::Literal(format!("A{}", "\u{301}".repeat(100))),
                style: TextStyle::default(),
            },
        },
    };
    assert!(
        ui.set_list_item_template(list.container, Some(invalid))
            .is_err()
    );
    assert!(
        ui.set_list_item_presentation(list.container, ListItemPresentation::RadioButton)
            .is_err()
    );
    assert!(
        ui.set_list_item_height(list.container, "k1", Some(f32::INFINITY))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.selected_keys(list.container).unwrap(), state.keys);
    assert_eq!(ui.rich.lists[&list.container].cursor, state.cursor);
    assert_eq!(ui.rich.lists[&list.container].anchor, state.anchor);
    assert_eq!(ui.list_item_content(list.container, "k1"), content);
}
#[test]
fn measured_heights_and_recycled_slots_never_transfer_old_template_widgets() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_item_template(list.container, Some(template()))
        .unwrap();
    ui.set_list_item_height(list.container, "k0", Some(100.0))
        .unwrap();
    ui.set_list_item_height(list.container, "k1", Some(60.0))
        .unwrap();
    frame(&mut ui);
    let old = ui.list_item_content(list.container, "k0").unwrap();
    let oldrow = ui.list_row_widget(list.container, "k0").unwrap();
    let lease = ui.list_row_lease(list.container, "k0").unwrap();
    assert_eq!(ui.runtime.bounds(oldrow).unwrap().height, 100.0);
    ui.scroll_to(list.viewport, 2000.0).unwrap();
    frame(&mut ui);
    assert!(ui.list_item_content(list.container, "k0").is_none());
    assert!(!ui.runtime.tree().contains(old));
    assert!(ui.list_resolve_lease(list.container, lease).is_none());
    ui.scroll_to(list.viewport, 0.0).unwrap();
    frame(&mut ui);
    assert_ne!(ui.list_item_content(list.container, "k0"), Some(old));
    assert_ne!(ui.list_row_widget(list.container, "k0"), Some(oldrow));
    assert_eq!(ui.list_item_height(list.container, "k0"), Some(100.0));
    let row = ui.list_row_widget(list.container, "k0").unwrap();
    ui.set_list_item_template(list.container, None).unwrap();
    frame(&mut ui);
    assert!(ui.list_item_content(list.container, "k0").is_none());
    assert_eq!(ui.list_row_widget(list.container, "k0"), Some(row));
}

#[test]
fn template_data_failure_and_structure_limits_leave_the_old_view_intact() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_item_template(list.container, Some(template()))
        .unwrap();
    action(&mut ui, list, "k1", SemanticAction::Activate);
    let nodes = ui.runtime.tree().preorder();
    let state = ui.rich.lists[&list.container].clone();
    let mut next = items();
    next[1].columns[0] = format!("A{}", "\u{301}".repeat(33));
    assert!(ui.set_selection_items(list.container, next).is_err());
    let leaf = ListTemplateNode {
        layout: LayoutStyle::default(),
        kind: ListTemplateKind::Text {
            source: ListItemTextSource::Text,
            style: TextStyle::default(),
        },
    };
    let wide = ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle::default(),
            kind: ListTemplateKind::Stack {
                orientation: Axis::Vertical,
                children: vec![leaf.clone(); 32],
            },
        },
    };
    assert!(
        ui.set_list_item_template(list.container, Some(wide))
            .is_err()
    );
    let mut deep = leaf;
    for _ in 0..8 {
        deep = ListTemplateNode {
            layout: LayoutStyle::default(),
            kind: ListTemplateKind::Stack {
                orientation: Axis::Vertical,
                children: vec![deep],
            },
        };
    }
    assert!(
        ui.set_list_item_template(list.container, Some(ListItemTemplate { root: deep }))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.selection_items(list.container).unwrap(), state.items);
    assert_eq!(ui.selected_keys(list.container).unwrap(), state.keys);
    assert_eq!(ui.rich.lists[&list.container].cursor, state.cursor);
    assert_eq!(ui.rich.lists[&list.container].anchor, state.anchor);
}

#[test]
fn keyboard_focus_follows_checkbox_cursor_across_virtual_pages_without_toggling() {
    let (mut ui, list) = setup(SelectionMode::Multiple);
    ui.set_list_item_presentation(list.container, ListItemPresentation::CheckBox)
        .unwrap();
    ui.set_list_item_template(list.container, Some(template()))
        .unwrap();
    ui.set_list_item_height(list.container, "k0", Some(96.0))
        .unwrap();
    action(&mut ui, list, "k0", SemanticAction::Activate);
    key(&mut ui, list, Key::End);
    let row = ui.list_row_widget(list.container, "k99").unwrap();
    assert_eq!(ui.runtime.focused(), Some(row));
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k0"]);
    let node = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == row)
        .unwrap();
    assert!(node.focused);
    assert_eq!(node.checked, Some(false));
    assert_eq!(node.position_in_set, Some(100));
    assert_eq!(node.set_size, Some(100));
    assert!(ui.runtime.bounds(row).unwrap().y >= ui.runtime.bounds(list.container).unwrap().y);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k0", "k99"]);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Home,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.runtime.focused(),
        ui.list_row_widget(list.container, "k0")
    );
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k0", "k99"]);
    ui.set_list_item_presentation(list.container, ListItemPresentation::Plain)
        .unwrap();
    ui.set_selection_settings(
        list.container,
        SelectionSettings {
            mode: SelectionMode::Multiple,
            read_only: true,
        },
    )
    .unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.runtime.focused(),
        ui.list_row_widget(list.container, "k99")
    );
    assert_eq!(ui.selected_keys(list.container).unwrap(), ["k0", "k99"]);
}
