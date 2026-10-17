use super::*;
use rust_desktop_ui_core::Modifiers;
use std::{cell::RefCell, rc::Rc};

fn path(count: usize) -> BreadcrumbPath {
    BreadcrumbPath {
        text: format!("route:{count}"),
        segments: (0..count)
            .map(|i| BreadcrumbSegment {
                key: format!("k{i}"),
                label: format!("Уровень {i} 🦀"),
                has_children: true,
            })
            .collect(),
    }
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(900.0, 600.0)).unwrap();
}
fn fixture(count: usize) -> (Controls, Breadcrumb) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .breadcrumb(
            ui.root(),
            "Расположение",
            path(count),
            Default::default(),
            LayoutStyle {
                width: Length::Px(600.0),
                height: Length::Px(38.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.runtime
        .apply(Mutation::Focus(Some(parts.edit_button)))
        .unwrap();
    (ui, parts)
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn request(ui: &mut Controls, p: Breadcrumb, text: &str) -> BreadcrumbNavigationRequest {
    ui.set_breadcrumb_draft(p.container, text).unwrap();
    key(ui, Key::Enter);
    ui.drain_breadcrumb_navigation_requests().pop().unwrap()
}

#[test]
fn breadcrumb_visible_segments_share_one_nonempty_row_after_mount_resize_and_edit() {
    let (mut ui, p) = fixture(4);
    for width in [900.0, 240.0, 600.0] {
        ui.edit_props(p.container, |props| props.style.width = Length::Px(width))
            .unwrap();
        frame(&mut ui);
        let owner = ui.runtime.bounds(p.container).unwrap();
        let mut count = 0;
        for i in 0..4 {
            let key = format!("k{i}");
            for id in [
                ui.breadcrumb_segment_button(p.container, &key).unwrap(),
                ui.breadcrumb_children_button(p.container, &key).unwrap(),
            ] {
                let node = ui.runtime.tree().node(id).unwrap();
                if node.props.visible && !node.props.collapsed {
                    let bounds = ui.runtime.bounds(id).expect("visible segment has bounds");
                    assert!(bounds.width > 0.0 && bounds.height > 0.0);
                    assert_eq!(bounds.y, owner.y);
                    assert!(
                        bounds.x >= owner.x
                            && bounds.x + bounds.width <= owner.x + owner.width + 0.01
                    );
                    assert!(ui.semantics().iter().any(|node| node.id == id));
                    count += 1;
                }
            }
        }
        assert!(
            count >= 2,
            "the final segment and its disclosure remain visible"
        );
        ui.begin_breadcrumb_edit(p.container).unwrap();
        frame(&mut ui);
        ui.cancel_breadcrumb(p.container).unwrap();
        frame(&mut ui);
        let last = ui.breadcrumb_segment_button(p.container, "k3").unwrap();
        assert!(ui.runtime.bounds(last).is_some());
    }
}

#[test]
fn breadcrumb_navigation_is_explicit_accepted_only_and_invalid_input_keeps_draft() {
    let (mut ui, p) = fixture(3);
    let before = ui.breadcrumb_path(p.container).unwrap().clone();
    let token = request(&mut ui, p, "new:location");
    assert_eq!(ui.breadcrumb_path(p.container), Some(&before));
    assert_eq!(
        ui.breadcrumb_status(p.container),
        Some(&BreadcrumbStatus::Pending)
    );
    assert_eq!(
        ui.complete_breadcrumb_navigation(&token, Err("Такого пути нет".into()))
            .unwrap(),
        BreadcrumbCompletion::Rejected
    );
    assert_eq!(ui.breadcrumb_draft(p.container), Some("new:location"));
    assert_eq!(ui.breadcrumb_path(p.container), Some(&before));
    assert!(ui.breadcrumb_history(p.container).unwrap().is_empty());
    key(&mut ui, Key::Escape);
    assert_eq!(ui.breadcrumb_draft(p.container), Some(before.text.as_str()));
    let token = request(&mut ui, p, "route:4");
    assert_eq!(
        ui.complete_breadcrumb_navigation(&token, Ok(path(4)))
            .unwrap(),
        BreadcrumbCompletion::Applied
    );
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(4)));
    assert_eq!(ui.breadcrumb_history(p.container).unwrap(), [path(4)]);
}

#[test]
fn breadcrumb_blur_keeps_draft_without_acceptance_and_cancels_pending() {
    let (mut ui, p) = fixture(2);
    let token = request(&mut ui, p, "pending");
    let other = ui
        .edit_box(ui.root(), "Other", "other", LayoutStyle::default())
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(other))).unwrap();
    frame(&mut ui);
    assert!(token.is_cancelled());
    assert_eq!(ui.breadcrumb_draft(p.container), Some("pending"));
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
    assert_eq!(
        ui.complete_breadcrumb_navigation(&token, Ok(path(1)))
            .unwrap(),
        BreadcrumbCompletion::Stale
    );
    assert_eq!(ui.runtime.focused(), Some(other));
}

#[test]
fn breadcrumb_children_validate_atomically_and_late_response_never_navigates() {
    let (mut ui, p) = fixture(3);
    ui.request_breadcrumb_children(p.container, "k1").unwrap();
    let old = ui.drain_breadcrumb_children_requests().pop().unwrap();
    ui.request_breadcrumb_children(p.container, "k2").unwrap();
    let current = ui.drain_breadcrumb_children_requests().pop().unwrap();
    assert!(old.is_cancelled());
    assert_eq!(
        ui.complete_breadcrumb_children(&old, Ok(vec![BreadcrumbChild::new("stale", "Old")]))
            .unwrap(),
        BreadcrumbCompletion::Stale
    );
    let count = ui.runtime.tree().len();
    assert!(
        ui.complete_breadcrumb_children(
            &current,
            Ok(vec![
                BreadcrumbChild::new("same", "First"),
                BreadcrumbChild::new("same", "Second")
            ])
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), count);
    assert!(!current.is_cancelled());
    assert!(
        ui.complete_breadcrumb_children(&current, Ok(vec![BreadcrumbChild::new("k1", "Cycle")]))
            .is_err()
    );
    let mut disabled = BreadcrumbChild::new("disabled", "Недоступен");
    disabled.enabled = false;
    ui.complete_breadcrumb_children(
        &current,
        Ok(vec![disabled, BreadcrumbChild::new("next", "Дальше")]),
    )
    .unwrap();
    frame(&mut ui);
    assert!(ui.choose_breadcrumb_item(p.container, "disabled").is_err());
    ui.choose_breadcrumb_item(p.container, "next").unwrap();
    let nav = ui.drain_breadcrumb_navigation_requests().pop().unwrap();
    assert_eq!(
        nav.target(),
        &BreadcrumbNavigationTarget::Child {
            parent: vec!["k0".into(), "k1".into(), "k2".into()],
            key: "next".into()
        }
    );
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(3)));
}

#[test]
fn breadcrumb_malformed_final_text_rejects_before_any_tree_or_model_mutation() {
    let (mut ui, p) = fixture(2);
    let token = request(&mut ui, p, "pending");
    let mut candidate = path(3);
    candidate.segments[1].label = "\u{301}".repeat(32);
    let count = ui.runtime.tree().len();
    let focus = ui.runtime.focused();
    assert!(
        ui.complete_breadcrumb_navigation(&token, Ok(candidate))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.runtime.focused(), focus);
    assert_eq!(ui.breadcrumb_draft(p.container), Some("pending"));
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
    assert!(!token.is_cancelled());
    ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
        .unwrap();
}

#[test]
fn breadcrumb_long_path_retains_segment_ids_and_restores_focus_after_overflow() {
    let (mut ui, p) = fixture(12);
    let id = ui.breadcrumb_segment_button(p.container, "k10").unwrap();
    ui.edit_props(p.container, |props| props.style.width = Length::Px(850.0))
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    let count = ui.runtime.tree().len();
    ui.edit_props(p.container, |props| props.style.width = Length::Px(180.0))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(p.overflow_button));
    assert_eq!(ui.breadcrumb_segment_button(p.container, "k10"), Some(id));
    assert_eq!(ui.runtime.tree().len(), count);
    ui.open_breadcrumb_overflow(p.container).unwrap();
    assert_eq!(ui.selection_items(p.list.container).unwrap().len(), 12);
    key(&mut ui, Key::Escape);
    ui.edit_props(p.container, |props| props.style.width = Length::Px(850.0))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(ui.breadcrumb_segment_button(p.container, "k10"), Some(id));
}

#[test]
fn breadcrumb_source_version_readonly_and_remove_cancel_tokens() {
    for reason in 0..3 {
        let (mut ui, p) = fixture(2);
        ui.request_breadcrumb_children(p.container, "k1").unwrap();
        let token = ui.drain_breadcrumb_children_requests().pop().unwrap();
        match reason {
            0 => {
                let mut settings = ui.breadcrumb_settings(p.container).unwrap();
                settings.source = BreadcrumbSourceVersion {
                    identity: 8,
                    revision: 1,
                };
                ui.set_breadcrumb_settings(p.container, settings).unwrap();
            }
            1 => ui.set_breadcrumb_read_only(p.container, true).unwrap(),
            _ => ui.remove(p.container).unwrap(),
        }
        assert!(token.is_cancelled());
        assert_eq!(
            ui.complete_breadcrumb_children(&token, Ok(vec![])).unwrap(),
            BreadcrumbCompletion::Stale
        );
    }
}

#[test]
fn breadcrumb_required_parts_and_changed_recipe_reject_before_acceptance() {
    let (mut ui, p) = fixture(2);
    let token = request(&mut ui, p, "route:3");
    for id in [
        p.editor,
        p.popup.container,
        p.list.container,
        ui.breadcrumb_segment_button(p.container, "k0").unwrap(),
    ] {
        assert!(ui.remove(id).is_err());
    }
    ui.set_editor_validator(
        p.editor,
        Some(EditorValidator::new(|_| Err("external validator".into()))),
    )
    .unwrap();
    let count = ui.runtime.tree().len();
    assert!(
        ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
}

#[test]
fn breadcrumb_reparented_owned_parts_reject_before_pending_path_or_layout_changes() {
    for case in 0..10 {
        let (mut ui, p) = fixture(2);
        let token = request(&mut ui, p, "route:3");
        let bar = ui.runtime.tree().parent(p.overflow_button).unwrap();
        let segment = ui.breadcrumb_segment_button(p.container, "k0").unwrap();
        let disclosure = ui.breadcrumb_children_button(p.container, "k0").unwrap();
        let status = ui.runtime.tree().children(p.popup.content).unwrap()[0];
        let (node, destination) = match case {
            0 => (p.editor, bar),
            1 => (bar, p.popup.content),
            2 => (segment, p.container),
            3 => (disclosure, p.editor),
            4 => (p.history_button, p.container),
            5 => (p.popup.container, p.container),
            6 => (p.popup.content, p.container),
            7 => (p.list.container, bar),
            8 => (p.list.content, bar),
            _ => (status, bar),
        };
        let parent = ui.runtime.tree().parent(node).unwrap();
        let draft = ui.breadcrumb_draft(p.container).unwrap().to_owned();
        let history = ui.breadcrumb_history(p.container).unwrap().to_vec();
        let selected = ui.selected_keys(p.list.container).unwrap().to_vec();
        ui.runtime
            .apply(Mutation::Reparent {
                node,
                parent: destination,
            })
            .unwrap();
        let ids = ui.runtime.tree().preorder();
        assert!(
            ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
                .is_err(),
            "case {case}"
        );
        assert!(!token.is_cancelled(), "case {case}");
        assert!(ui.begin_breadcrumb_edit(p.container).is_err());
        assert!(ui.sync_breadcrumbs().is_err());
        assert!(ui.sync_breadcrumb_layout().is_err());
        assert_eq!(ui.runtime.tree().preorder(), ids);
        assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
        assert_eq!(ui.breadcrumb_draft(p.container), Some(draft.as_str()));
        assert_eq!(ui.breadcrumb_history(p.container).unwrap(), history);
        assert_eq!(ui.selected_keys(p.list.container).unwrap(), selected);
        ui.runtime
            .apply(Mutation::Reparent { node, parent })
            .unwrap();
        // Reparent itself may clear focus; restoring the owned editor is an
        // explicit application action, separate from the rejected completion.
        ui.runtime.apply(Mutation::Focus(Some(p.editor))).unwrap();
        assert_eq!(
            ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
                .unwrap(),
            BreadcrumbCompletion::Applied
        );
    }
}

#[test]
fn breadcrumb_binding_publishes_only_accepted_navigation_and_rejects_dirty_replacement() {
    use crate::{
        BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings,
        DataValue,
    };
    let (mut ui, p) = fixture(2);
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "location",
            breadcrumb_path_to_value(&path(2)),
        )]))
        .unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&context));
    let mut binding = ControlBinding::new(p.container, BindingProperty::Value, "location").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    let token = request(&mut ui, p, "route:3");
    bindings.sync(&mut ui);
    assert_eq!(
        context.borrow().get("location").unwrap(),
        &breadcrumb_path_to_value(&path(2))
    );
    ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        context.borrow().get("location").unwrap(),
        &breadcrumb_path_to_value(&path(3))
    );
    ui.set_breadcrumb_draft(p.container, "unfinished").unwrap();
    context
        .borrow_mut()
        .set("location", breadcrumb_path_to_value(&path(4)))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(3)));
    assert_eq!(ui.breadcrumb_draft(p.container), Some("unfinished"));
    assert!(!bindings.diagnostics().is_empty());
    ui.cancel_breadcrumb(p.container).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(4)));
}

#[test]
fn breadcrumb_ime_escape_and_keyboard_segments_do_not_commit_text_implicitly() {
    let (mut ui, p) = fixture(3);
    let first = ui.breadcrumb_segment_button(p.container, "k0").unwrap();
    let last = ui.breadcrumb_segment_button(p.container, "k2").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(first))).unwrap();
    key(&mut ui, Key::End);
    assert_eq!(ui.runtime.focused(), Some(last));
    key(&mut ui, Key::Home);
    assert_eq!(ui.runtime.focused(), Some(first));
    key(&mut ui, Key::F2);
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    ui.set_breadcrumb_draft(p.container, "draft").unwrap();
    ui.ime_preedit("あ", Some((3, 3))).unwrap();
    key(&mut ui, Key::Enter);
    assert!(ui.drain_breadcrumb_navigation_requests().is_empty());
    key(&mut ui, Key::Escape);
    assert!(ui.document(p.editor).unwrap().composition().is_none());
    assert_eq!(ui.breadcrumb_draft(p.container), Some("draft"));
    key(&mut ui, Key::Escape);
    assert_eq!(ui.breadcrumb_draft(p.container), Some("route:3"));
    assert!(!ui.breadcrumb_is_editing(p.container));
}

#[test]
fn breadcrumb_direct_draft_change_invalidates_response_and_source_change_is_explicit() {
    let (mut ui, p) = fixture(2);
    let token = request(&mut ui, p, "submitted");
    ui.set_text(p.editor, "changed later").unwrap();
    assert_eq!(
        ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
            .unwrap(),
        BreadcrumbCompletion::Stale
    );
    let old = ui.breadcrumb_settings(p.container).unwrap();
    let mut settings = old;
    settings.source.revision += 1;
    assert!(ui.set_breadcrumb_settings(p.container, settings).is_err());
    assert_eq!(ui.breadcrumb_settings(p.container), Some(old));
    assert!(ui.set_breadcrumb_path(p.container, path(4)).is_err());
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
    ui.cancel_breadcrumb(p.container).unwrap();
    ui.set_breadcrumb_settings(p.container, settings).unwrap();
    ui.set_breadcrumb_path(p.container, path(4)).unwrap();
    assert!(ui.breadcrumb_history(p.container).unwrap().is_empty());
}

#[test]
fn breadcrumb_invalid_typed_binding_shape_and_late_malformed_children_leave_state_intact() {
    use crate::DataValue;
    let data = breadcrumb_path_to_value(&path(3));
    assert_eq!(breadcrumb_path_from_value(&data).unwrap(), path(3));
    let DataValue::Object(mut fields) = data else {
        unreachable!()
    };
    fields.insert("extra".into(), DataValue::Null);
    assert!(breadcrumb_path_from_value(&DataValue::Object(fields)).is_err());
    let (mut ui, p) = fixture(2);
    ui.request_breadcrumb_children(p.container, "k1").unwrap();
    let token = ui.drain_breadcrumb_children_requests().pop().unwrap();
    let count = ui.runtime.tree().len();
    let focus = ui.runtime.focused();
    assert!(
        ui.complete_breadcrumb_children(
            &token,
            Ok(vec![BreadcrumbChild::new(
                "bad",
                format!("a{}", "\u{301}".repeat(32))
            )])
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.runtime.focused(), focus);
    assert!(!token.is_cancelled());
    assert!(ui.breadcrumb_children(p.container).unwrap().is_empty());
    ui.complete_breadcrumb_children(&token, Ok(vec![BreadcrumbChild::new("good", "Good")]))
        .unwrap();
}

#[test]
fn breadcrumb_application_validator_and_binding_validator_run_before_tree_publication() {
    let (mut ui, p) = fixture(2);
    ui.set_breadcrumb_validator(
        p.container,
        Some(BreadcrumbValidator::new(|path| {
            if path.segments.len() > 2 {
                Err("Only two levels".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    let token = request(&mut ui, p, "route:3");
    let nodes = ui.runtime.tree().len();
    assert!(
        ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), nodes);
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
    assert!(!token.is_cancelled());
    ui.set_breadcrumb_validator(p.container, None).unwrap();
    ui.complete_breadcrumb_navigation(&token, Ok(path(3)))
        .unwrap();
}

#[test]
fn breadcrumb_external_mask_cannot_partially_publish_path_or_retained_segments() {
    let (mut ui, p) = fixture(2);
    ui.set_editor_mask(p.editor, Some(MaskSettings::new("XXXXXXX").unwrap()))
        .unwrap();
    let count = ui.runtime.tree().len();
    let first = ui.breadcrumb_segment_button(p.container, "k0").unwrap();
    let caption = ui.label_text(first).unwrap().to_owned();
    assert!(ui.set_breadcrumb_path(p.container, path(10)).is_err());
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.breadcrumb_segment_button(p.container, "k0"), Some(first));
    assert_eq!(ui.label_text(first), Some(caption.as_str()));
    assert_eq!(ui.breadcrumb_path(p.container), Some(&path(2)));
    assert_eq!(ui.breadcrumb_draft(p.container), Some("route:2"));
    assert!(ui.breadcrumb_history(p.container).unwrap().is_empty());
}
