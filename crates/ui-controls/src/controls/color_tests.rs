use super::*;
use crate::{
    BindingContext, BindingProperty, BindingValidator, ControlBinding, ControlBindingMode,
    ControlBindings, DataValue,
};
use rust_desktop_ui_core::Modifiers;
use std::{cell::RefCell, rc::Rc};

fn frame(ui: &mut Controls) {
    ui.update(Size::new(900.0, 850.0)).unwrap();
}
fn fixture(brush: bool) -> (Controls, ColorEditor) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let style = LayoutStyle {
        width: Length::Px(400.0),
        height: Length::Px(36.0),
        flex_shrink: 0.0,
        ..Default::default()
    };
    let parts = if brush {
        ui.brush_edit(
            ui.root(),
            "Brush",
            Some(BrushValue::Solid(Srgba8::WHITE)),
            Default::default(),
            style,
        )
    } else {
        ui.color_edit(
            ui.root(),
            "Color",
            Some(Srgba8::WHITE),
            Default::default(),
            style,
        )
    }
    .unwrap();
    frame(&mut ui);
    (ui, parts)
}
fn escape(ui: &mut Controls) {
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn gradient() -> BrushValue {
    BrushValue::Linear(
        LinearGradient::new(
            23.123456789,
            vec![
                GradientStop::new(0.0, Srgba8::new(255, 13, 7, 0)),
                GradientStop::new(0.5, Srgba8::BLACK),
                GradientStop::new(0.5, Srgba8::WHITE),
                GradientStop::new(1.0, Srgba8::new(1, 2, 3, 155)),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn strict_binding_roundtrip_distinguishes_null_transparency_and_exact_stops() {
    for alpha in 0..=255 {
        for channel in 0..=255 {
            let color = Some(Srgba8::new(channel, 23, 255 - channel, alpha));
            assert_eq!(color_from_value(&color_to_value(color)).unwrap(), color);
        }
    }
    assert_ne!(
        color_to_value(None),
        color_to_value(Some(Srgba8::TRANSPARENT))
    );
    for value in [
        None,
        Some(BrushValue::Solid(Srgba8::TRANSPARENT)),
        Some(gradient()),
    ] {
        assert_eq!(
            brush_from_value(&brush_to_value(value.as_ref())).unwrap(),
            value
        );
    }
    for n in [f64::NAN, f64::INFINITY, -1.0, 256.0, 1.5] {
        let v = DataValue::object([
            ("r", DataValue::Number(n)),
            ("g", 0.0.into()),
            ("b", 0.0.into()),
            ("a", 255.0.into()),
        ]);
        assert!(color_from_value(&v).is_err());
    }
    assert!(
        color_from_value(&DataValue::object([
            ("r", 0.0.into()),
            ("g", 0.0.into()),
            ("b", 0.0.into()),
            ("x", 0.0.into())
        ]))
        .is_err()
    );
    let mut value = brush_to_value(Some(&gradient()));
    if let DataValue::Object(ref mut map) = value {
        map.insert("extra".into(), DataValue::Null);
    }
    assert!(brush_from_value(&value).is_err());
}
#[test]
fn invalid_constructor_and_values_leave_tree_accepted_draft_and_history_unchanged() {
    let (mut ui, p) = fixture(false);
    let ids = ui.runtime.tree().live_ids();
    for settings in [
        ColorEditorSettings {
            palette: vec![Srgba8::BLACK; 129],
            ..Default::default()
        },
        ColorEditorSettings {
            recent_limit: 33,
            ..Default::default()
        },
        ColorEditorSettings {
            popup_size: Size::new(f32::NAN, 400.0),
            ..Default::default()
        },
    ] {
        assert!(
            ui.color_edit(
                ui.root(),
                "Bad",
                Some(Srgba8::BLACK),
                settings,
                Default::default()
            )
            .is_err()
        );
        assert_eq!(ui.runtime.tree().live_ids(), ids);
    }
    assert!(
        ui.color_edit(
            ui.root(),
            "Required",
            None,
            ColorEditorSettings {
                allow_empty: false,
                ..Default::default()
            },
            Default::default()
        )
        .is_err()
    );
    assert!(ui.set_brush_value(p.container, Some(gradient())).is_err());
    ui.set_text(p.editor, "#00FA1122").unwrap();
    assert_eq!(ui.commit_edit(p.editor).unwrap(), CommitOutcome::Committed);
    assert_eq!(
        ui.color_value(p.container),
        Some(Some(Srgba8::new(250, 17, 34, 0)))
    );
    assert_eq!(ui.drain_brush_changes().len(), 1);
    ui.set_text(p.editor, "#broken").unwrap();
    let before = ui.document(p.editor).unwrap().selection();
    assert!(matches!(
        ui.commit_edit(p.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.document(p.editor).unwrap().selection(), before);
    assert_eq!(ui.accepted_value(p.editor), Some("#00FA1122"));
    ui.cancel_edit(p.editor).unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "#00FA1122");
}
#[test]
fn popup_preview_apply_cancel_and_implicit_focus_do_not_publish_partial_values() {
    let (mut ui, p) = fixture(false);
    ui.runtime.apply(Mutation::Focus(Some(p.editor))).unwrap();
    frame(&mut ui);
    ui.set_text(p.editor, "#00112233").unwrap();
    ui.open_color_popup(p.container).unwrap();
    frame(&mut ui);
    assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
    assert!(ui.drain_brush_changes().is_empty());
    let popup = ui.color_popup(p.container).unwrap().clone();
    let ids = ui.runtime.tree().live_ids();
    ui.set_color_draft(p.container, Srgba8::new(10, 20, 30, 0))
        .unwrap();
    assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
    assert!(ui.color_recent(p.container).unwrap().is_empty());
    escape(&mut ui);
    assert!(!ui.color_popup_is_open(p.container));
    assert_eq!(ui.document(p.editor).unwrap().text(), "#00112233");
    ui.open_color_popup(p.container).unwrap();
    ui.set_color_draft(p.container, Srgba8::new(10, 20, 30, 0))
        .unwrap();
    assert_eq!(
        ui.apply_color_popup(p.container).unwrap(),
        CommitOutcome::Committed
    );
    frame(&mut ui);
    assert_eq!(ui.document(p.editor).unwrap().text(), "#000A141E");
    assert_eq!(ui.drain_brush_changes().len(), 1);
    assert_eq!(
        ui.color_recent(p.container),
        Some([Srgba8::new(10, 20, 30, 0)].as_slice())
    );
    ui.set_theme(Theme::FluentDark).unwrap();
    ui.update(Size::new(600.0, 700.0)).unwrap();
    assert_eq!(ui.color_popup(p.container).unwrap().hex, popup.hex);
    assert_eq!(ui.runtime.tree().live_ids(), ids);
}
#[test]
fn invalid_channels_and_source_validator_keep_popup_pending_and_accepted_atomic() {
    let (mut ui, p) = fixture(false);
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Color",
            color_to_value(Some(Srgba8::WHITE)),
        )]))
        .unwrap(),
    ));
    model
        .borrow_mut()
        .set_field_validator(
            "Color",
            Some(BindingValidator::new(|value| {
                if color_from_value(value)?.is_some_and(|c| c.r > 200 && c != Srgba8::WHITE) {
                    Err("Red exceeds 200".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let mut bindings = ControlBindings::new(model.clone());
    let mut binding = ControlBinding::new(p.container, BindingProperty::Value, "Color").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.open_color_popup(p.container).unwrap();
    let popup = ui.color_popup(p.container).unwrap().clone();
    ui.set_text(popup.channels[0].editor, "999").unwrap();
    frame(&mut ui);
    assert!(matches!(
        ui.apply_color_popup(p.container).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert!(ui.color_popup_is_open(p.container));
    assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
    ui.set_color_draft(p.container, Srgba8::rgb(240, 1, 2))
        .unwrap();
    assert!(matches!(
        ui.apply_color_popup(p.container).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Color").unwrap(),
        &color_to_value(Some(Srgba8::WHITE))
    );
    assert!(ui.drain_brush_changes().is_empty());
    ui.set_color_draft(p.container, Srgba8::new(100, 1, 2, 0))
        .unwrap();
    assert_eq!(
        ui.apply_color_popup(p.container).unwrap(),
        CommitOutcome::Committed
    );
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Color").unwrap(),
        &color_to_value(Some(Srgba8::new(100, 1, 2, 0)))
    );
}
#[test]
fn external_dirty_replacement_has_explicit_conflict_and_equal_value_preserves_input() {
    let (mut ui, p) = fixture(false);
    ui.set_text(p.editor, "#FF010203").unwrap();
    ui.set_color_value(p.container, Some(Srgba8::WHITE))
        .unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "#FF010203");
    assert!(
        ui.set_color_value(p.container, Some(Srgba8::BLACK))
            .is_err()
    );
    assert_eq!(ui.document(p.editor).unwrap().text(), "#FF010203");
    ui.set_brush_value_with_policy(
        p.container,
        Some(BrushValue::Solid(Srgba8::BLACK)),
        ColorReplacePolicy::CancelPending,
    )
    .unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "#FF000000");
    assert!(ui.drain_brush_changes().is_empty());
    ui.open_color_popup(p.container).unwrap();
    ui.set_color_draft(p.container, Srgba8::WHITE).unwrap();
    assert!(ui.set_color_value(p.container, None).is_err());
    ui.set_brush_value_with_policy(p.container, None, ColorReplacePolicy::CancelPending)
        .unwrap();
    assert_eq!(ui.color_value(p.container), Some(None));
    assert!(!ui.color_popup_is_open(p.container));
}
#[test]
fn gradient_stop_keys_survive_sort_and_duplicate_edges_and_failures_are_atomic() {
    let (mut ui, p) = fixture(true);
    ui.set_brush_value(p.container, Some(gradient())).unwrap();
    ui.open_color_popup(p.container).unwrap();
    let stops = ui.gradient_stops(p.container).unwrap().to_vec();
    let first = stops[0].id;
    ui.set_gradient_stop(p.container, first, 0.8, Srgba8::new(10, 20, 30, 0))
        .unwrap();
    assert_eq!(ui.selected_gradient_stop(p.container), Some(first));
    assert_eq!(ui.gradient_stops(p.container).unwrap()[2].id, first);
    let before = ui.brush_draft(p.container).unwrap().cloned();
    let keys = ui.gradient_stops(p.container).unwrap().to_vec();
    for offset in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(
            ui.set_gradient_stop(p.container, first, offset, Srgba8::WHITE)
                .is_err()
        );
        assert_eq!(ui.brush_draft(p.container).unwrap(), before.as_ref());
        assert_eq!(ui.gradient_stops(p.container).unwrap(), keys);
    }
    let added = ui
        .add_gradient_stop(p.container, 0.5, Srgba8::rgb(1, 2, 3))
        .unwrap();
    assert_eq!(
        ui.gradient_stops(p.container)
            .unwrap()
            .iter()
            .rfind(|s| s.offset == 0.5)
            .unwrap()
            .id,
        added
    );
    ui.set_gradient_angle(p.container, 360.0).unwrap();
    ui.remove_gradient_stop(p.container, added).unwrap();
    assert_eq!(
        ui.apply_color_popup(p.container).unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(ui.drain_brush_changes().len(), 1);
    assert!(
        matches!(ui.brush_value(p.container),Some(Some(BrushValue::Linear(g)))if g.angle()==360.0)
    );
}
#[test]
fn escape_ime_first_disable_outside_and_required_part_lifecycle() {
    let (mut ui, p) = fixture(false);
    ui.open_color_popup(p.container).unwrap();
    frame(&mut ui);
    let popup = ui.color_popup(p.container).unwrap().clone();
    ui.runtime.apply(Mutation::Focus(Some(popup.hex))).unwrap();
    ui.ime_preedit("A", Some((0, 1))).unwrap();
    escape(&mut ui);
    assert!(ui.color_popup_is_open(p.container));
    assert!(ui.document(popup.hex).unwrap().composition().is_none());
    ui.set_color_draft(p.container, Srgba8::BLACK).unwrap();
    ui.set_enabled(p.container, false).unwrap();
    frame(&mut ui);
    assert!(!ui.color_popup_is_open(p.container));
    assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
    assert!(ui.drain_brush_changes().is_empty());
    ui.set_enabled(p.container, true).unwrap();
    frame(&mut ui);
    ui.open_color_popup(p.container).unwrap();
    frame(&mut ui);
    ui.dispatch(InputEvent::PointerDown {
        position: Point::new(850.0, 800.0),
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    frame(&mut ui);
    assert!(!ui.color_popup_is_open(p.container));
    assert!(ui.remove(p.editor).is_err());
    assert!(ui.remove(popup.channels[0].editor).is_err());
    ui.remove(p.container).unwrap();
    frame(&mut ui);
    assert!(!ui.runtime.tree().contains(popup.flyout.container));
}
#[test]
fn readonly_inner_editor_blocks_popup_and_brush_preview_bitmap_is_bounded() {
    let (mut ui, p) = fixture(false);
    ui.set_read_only(p.editor, true).unwrap();
    assert!(ui.open_color_popup(p.container).is_err());
    ui.set_read_only(p.editor, false).unwrap();
    ui.open_color_popup(p.container).unwrap();
    ui.set_color_draft(p.container, Srgba8::BLACK).unwrap();
    ui.set_read_only(p.editor, true).unwrap();
    frame(&mut ui);
    assert!(!ui.color_popup_is_open(p.container));
    let (mut ui, p) = fixture(true);
    ui.set_brush_value(p.container, Some(gradient())).unwrap();
    ui.open_color_popup(p.container).unwrap();
    frame(&mut ui);
    let scene = ui.scene();
    assert!(scene.images.len() >= 2);
    for image in &scene.images {
        assert_eq!(image.bitmap.width(), 256);
        assert_eq!(image.bitmap.height(), 64);
    }
    let image = ui.color_editors.entries[&p.container]
        .preview_bitmap
        .clone();
    frame(&mut ui);
    assert_eq!(ui.color_editors.entries[&p.container].preview_bitmap, image);
}

#[test]
fn owned_recipe_changes_fail_before_pending_value_fields_history_or_tree_mutation() {
    for kind in 0..4 {
        let (mut ui, p) = fixture(false);
        ui.open_color_popup(p.container).unwrap();
        ui.set_color_draft(p.container, Srgba8::BLACK).unwrap();
        let popup = ui.color_popup(p.container).unwrap().clone();
        match kind {
            0 => {
                let mut settings = ui
                    .numeric_settings(popup.channels[0].container)
                    .unwrap()
                    .clone();
                settings.maximum = DecimalValue::new(10, 0).unwrap();
                ui.set_numeric_settings(popup.channels[0].container, settings)
                    .unwrap();
            }
            1 => {
                let mut settings = ui.editor_settings(popup.hex).unwrap().clone();
                settings.max_length = Some(10);
                ui.set_editor_settings(popup.hex, settings).unwrap();
            }
            2 => {
                ui.set_editor_validator(
                    popup.hex,
                    Some(EditorValidator::new(|_| Err("reject".into()))),
                )
                .unwrap();
            }
            _ => {
                ui.chain_selection_validator(
                    popup.stops.container,
                    SelectionValidator::new(|_| Err("reject".into())),
                )
                .unwrap();
            }
        }
        let ids = ui.runtime.tree().live_ids();
        let draft = ui.brush_draft(p.container).unwrap().cloned();
        let fields = [
            popup.hex,
            popup.channels[0].editor,
            popup.offset,
            popup.angle,
        ]
        .map(|id| {
            (
                id,
                ui.document(id).unwrap().text().to_owned(),
                ui.document(id).unwrap().selection(),
                ui.document(id).unwrap().can_undo(),
            )
        });
        assert!(
            ui.set_color_draft(p.container, Srgba8::rgb(200, 50, 25))
                .is_err()
        );
        assert_eq!(ui.brush_draft(p.container).unwrap(), draft.as_ref());
        assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
        assert_eq!(ui.runtime.tree().live_ids(), ids);
        assert!(ui.color_recent(p.container).unwrap().is_empty());
        assert!(ui.drain_brush_changes().is_empty());
        for (id, text, selection, undo) in fields {
            assert_eq!(ui.document(id).unwrap().text(), text);
            assert_eq!(ui.document(id).unwrap().selection(), selection);
            assert_eq!(ui.document(id).unwrap().can_undo(), undo);
        }
    }
}
#[test]
fn live_element_source_validator_and_dirty_source_reject_before_target_commit() {
    use crate::{BindingEndpoint, BindingSource};
    let (mut ui, source) = fixture(false);
    let target = ui
        .color_edit(
            ui.root(),
            "Target",
            Some(Srgba8::WHITE),
            Default::default(),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([] as [(&str, DataValue); 0])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(model);
    let mut binding =
        ControlBinding::new(target.container, BindingProperty::Value, "Unused").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    binding.source = BindingSource::Element(BindingEndpoint {
        widget: source.container,
        property: BindingProperty::Value,
    });
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.set_color_validator(
        source.container,
        Some(BrushValidator::new(|v| {
            if v == Some(&BrushValue::Solid(Srgba8::BLACK)) {
                Err("Black rejected at source".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.set_text(target.editor, "#FF000000").unwrap();
    assert!(matches!(
        ui.commit_edit(target.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.color_value(target.container), Some(Some(Srgba8::WHITE)));
    ui.set_text(source.editor, "#FF112233").unwrap();
    ui.set_text(target.editor, "#FF445566").unwrap();
    assert!(matches!(
        ui.commit_edit(target.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.color_value(target.container), Some(Some(Srgba8::WHITE)));
    assert_eq!(ui.document(source.editor).unwrap().text(), "#FF112233");
    ui.cancel_edit(source.editor).unwrap();
    assert_eq!(
        ui.commit_edit(target.editor).unwrap(),
        CommitOutcome::Committed
    );
    bindings.sync(&mut ui);
    assert_eq!(
        ui.color_value(source.container),
        ui.color_value(target.container)
    );
}
#[test]
fn hostile_validation_diagnostic_and_event_backpressure_do_not_lose_accepted_values() {
    let (mut ui, p) = fixture(false);
    ui.set_color_validator(
        p.container,
        Some(BrushValidator::new(|_| {
            Err(format!("A{}", "\u{0301}".repeat(4500)))
        })),
    )
    .unwrap();
    ui.open_color_popup(p.container).unwrap();
    ui.set_color_draft(p.container, Srgba8::BLACK).unwrap();
    assert!(matches!(
        ui.apply_color_popup(p.container).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    frame(&mut ui);
    assert_eq!(
        ui.color_error(p.container),
        Some("Invalid color or brush value.")
    );
    assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
    ui.cancel_color_popup(p.container).unwrap();
    ui.set_color_validator(p.container, None).unwrap();
    for i in 0..256 {
        ui.set_text(p.editor, if i % 2 == 0 { "#FF000000" } else { "#FFFFFFFF" })
            .unwrap();
        assert_eq!(ui.commit_edit(p.editor).unwrap(), CommitOutcome::Committed);
    }
    ui.set_text(p.editor, "#FF123456").unwrap();
    assert!(matches!(
        ui.commit_edit(p.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.color_value(p.container), Some(Some(Srgba8::WHITE)));
    assert_eq!(ui.drain_brush_changes().len(), 256);
    assert_eq!(ui.commit_edit(p.editor).unwrap(), CommitOutcome::Committed);
}

#[test]
fn subnormal_gradient_numbers_fit_fields_and_round_trip_without_loss() {
    let (mut ui, parts) = fixture(true);
    let tiny = f64::from_bits(1);
    let value = BrushValue::Linear(
        LinearGradient::new(
            tiny,
            vec![
                GradientStop::new(tiny, Srgba8::BLACK),
                GradientStop::new(1.0, Srgba8::WHITE),
            ],
        )
        .unwrap(),
    );
    ui.set_brush_value(parts.container, Some(value.clone()))
        .unwrap();
    ui.open_color_popup(parts.container).unwrap();
    let popup = ui.color_popup(parts.container).unwrap().clone();
    for field in [popup.angle, popup.offset] {
        let text = ui.document(field).unwrap().text();
        assert!(text.len() <= 24);
        assert_eq!(text.parse::<f64>().unwrap().to_bits(), tiny.to_bits());
    }
    assert_eq!(
        ui.apply_color_popup(parts.container).unwrap(),
        CommitOutcome::Unchanged
    );
    assert_eq!(ui.brush_value(parts.container), Some(Some(&value)));
}

#[test]
fn already_closed_flyout_cannot_publish_between_dispatch_and_next_frame() {
    let (mut ui, parts) = fixture(false);
    ui.open_color_popup(parts.container).unwrap();
    ui.set_color_draft(parts.container, Srgba8::BLACK).unwrap();
    let popup = ui.color_popup(parts.container).unwrap().clone();
    ui.close_flyout(popup.flyout).unwrap();
    assert!(
        ui.set_color_draft(parts.container, Srgba8::rgb(3, 4, 5))
            .is_err()
    );
    assert_eq!(
        ui.apply_color_popup(parts.container).unwrap(),
        CommitOutcome::Unchanged
    );
    assert_eq!(ui.color_value(parts.container), Some(Some(Srgba8::WHITE)));
    assert!(ui.drain_brush_changes().is_empty());
    assert!(ui.color_recent(parts.container).unwrap().is_empty());
}

#[test]
fn readonly_prevents_accepting_preexisting_inline_draft() {
    let (mut ui, parts) = fixture(false);
    ui.set_text(parts.editor, "#FF112233").unwrap();
    ui.set_color_read_only(parts.container, true).unwrap();
    assert_eq!(
        ui.commit_edit(parts.editor).unwrap(),
        CommitOutcome::Unchanged
    );
    assert_eq!(ui.color_value(parts.container), Some(Some(Srgba8::WHITE)));
    assert_eq!(ui.document(parts.editor).unwrap().text(), "#FF112233");
    assert!(!ui.runtime.tree().node(parts.button).unwrap().props.enabled);
    assert!(ui.drain_brush_changes().is_empty());
    ui.set_color_read_only(parts.container, false).unwrap();
    assert_eq!(
        ui.commit_edit(parts.editor).unwrap(),
        CommitOutcome::Committed
    );
}

#[test]
fn color_owned_part_reparent_rejects_before_value_or_draft_publication() {
    for which in 0..4 {
        let (mut ui, parts) = fixture(true);
        ui.open_color_popup(parts.container).unwrap();
        ui.set_color_draft(parts.container, Srgba8::rgb(30, 40, 50))
            .unwrap();
        let popup = ui.color_popup(parts.container).unwrap().clone();
        let accepted = ui.brush_value(parts.container).unwrap().cloned();
        let draft = ui.brush_draft(parts.container).unwrap().cloned();
        let recent = ui.color_recent(parts.container).unwrap().to_vec();
        let part = match which {
            0 => parts.editor,
            1 => popup.angle,
            2 => popup.channels[0].editor,
            _ => popup.stops.content,
        };
        let original_parent = ui.runtime.tree().parent(part).unwrap();
        let other = ui
            .transparent_panel(ui.root(), LayoutStyle::default())
            .unwrap();
        ui.runtime
            .apply(Mutation::Reparent {
                node: part,
                parent: other,
            })
            .unwrap();
        let tree = ui.runtime.tree().preorder();
        assert!(ui.set_color_draft(parts.container, Srgba8::BLACK).is_err());
        assert!(
            ui.set_brush_value_with_policy(
                parts.container,
                Some(BrushValue::Solid(Srgba8::BLACK)),
                ColorReplacePolicy::CancelPending
            )
            .is_err()
        );
        assert_eq!(ui.brush_value(parts.container).unwrap(), accepted.as_ref());
        assert_eq!(ui.brush_draft(parts.container).unwrap(), draft.as_ref());
        assert_eq!(ui.color_recent(parts.container).unwrap(), recent);
        assert_eq!(ui.runtime.tree().preorder(), tree);
        assert!(ui.drain_brush_changes().is_empty());
        ui.runtime
            .apply(Mutation::Reparent {
                node: part,
                parent: original_parent,
            })
            .unwrap();
        ui.cancel_color_popup(parts.container).unwrap();
    }
}

#[test]
fn color_popup_measured_scroll_wheel_and_tab_reveal_gradient_fields() {
    for size in [Size::new(900.0, 640.0), Size::new(640.0, 420.0)] {
        let (mut ui, parts) = fixture(true);
        ui.update(size).unwrap();
        ui.open_color_popup(parts.container).unwrap();
        ui.set_brush_draft(
            parts.container,
            Some(BrushValue::Linear(
                LinearGradient::new(
                    0.0,
                    vec![
                        GradientStop::new(0.0, Srgba8::BLACK),
                        GradientStop::new(1.0, Srgba8::WHITE),
                    ],
                )
                .unwrap(),
            )),
        )
        .unwrap();
        ui.update(size).unwrap();
        let popup = ui.color_popup(parts.container).unwrap().clone();
        let before = ui.scroll_offset(popup.scroll.viewport).unwrap().y;
        let view = ui.runtime.bounds(popup.scroll.viewport).unwrap();
        ui.dispatch(InputEvent::Wheel {
            position: Point::new(view.x + 3.0, view.y + 15.0),
            delta: Point::new(0.0, 480.0),
            modifiers: Modifiers::default(),
        })
        .unwrap();
        ui.update(size).unwrap();
        assert!(ui.scroll_offset(popup.scroll.viewport).unwrap().y > before);
        ui.scroll_to(popup.scroll.viewport, 0.0).unwrap();
        ui.runtime
            .apply(Mutation::Focus(Some(popup.offset)))
            .unwrap();
        ui.update(size).unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        ui.update(size).unwrap();
        assert_eq!(ui.runtime.focused(), Some(popup.angle));
        let angle = ui.runtime.bounds(popup.angle).unwrap();
        let clip = ui.runtime.clip(popup.angle).unwrap();
        assert_eq!(angle.intersection(clip), Some(angle));
        let footer = ui.runtime.bounds(popup.apply).unwrap();
        assert!(angle.y + angle.height <= footer.y);
        ui.set_text(popup.angle, "45").unwrap();
        ui.update(size).unwrap();
        assert!(
            matches!(ui.brush_draft(parts.container),Some(Some(BrushValue::Linear(g))) if g.angle()==45.0)
        );
        assert_eq!(
            ui.brush_value(parts.container),
            Some(Some(&BrushValue::Solid(Srgba8::WHITE)))
        );
    }
}

#[test]
fn color_popup_reopen_reveals_the_same_retained_hex_after_wheel_without_focus_change() {
    let (mut ui, parts) = fixture(true);
    let size = Size::new(900.0, 640.0);
    ui.update(size).unwrap();
    ui.open_color_popup(parts.container).unwrap();
    ui.set_brush_draft(
        parts.container,
        Some(BrushValue::Linear(
            LinearGradient::new(
                0.0,
                vec![
                    GradientStop::new(0.0, Srgba8::BLACK),
                    GradientStop::new(1.0, Srgba8::WHITE),
                ],
            )
            .unwrap(),
        )),
    )
    .unwrap();
    ui.update(size).unwrap();
    let popup = ui.color_popup(parts.container).unwrap().clone();
    assert_eq!(ui.runtime.focused(), Some(popup.hex));
    let view = ui.runtime.bounds(popup.scroll.viewport).unwrap();
    ui.dispatch(InputEvent::Wheel {
        position: Point::new(view.x + 3.0, view.y + 15.0),
        delta: Point::new(0.0, 480.0),
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.update(size).unwrap();
    assert_eq!(ui.runtime.focused(), Some(popup.hex));
    assert!(ui.scroll_offset(popup.scroll.viewport).unwrap().y > 0.0);
    // Accepted gradient keeps the same overflowing geometry on reopen.
    ui.apply_color_popup(parts.container).unwrap();
    ui.open_color_popup(parts.container).unwrap();
    ui.update(size).unwrap();
    let reopened = ui.color_popup(parts.container).unwrap();
    assert_eq!(reopened.hex, popup.hex);
    assert_eq!(ui.runtime.focused(), Some(popup.hex));
    let bounds = ui.runtime.bounds(popup.hex).unwrap();
    assert_eq!(
        bounds.intersection(ui.runtime.clip(popup.hex).unwrap()),
        Some(bounds)
    );
}

#[test]
fn color_popup_channels_and_swatches_fit_before_the_scrollbar_strip() {
    for size in [Size::new(900.0, 640.0), Size::new(640.0, 420.0)] {
        let (mut ui, parts) = fixture(true);
        ui.update(size).unwrap();
        ui.open_color_popup(parts.container).unwrap();
        ui.set_brush_draft(parts.container, Some(gradient()))
            .unwrap();
        ui.update(size).unwrap();
        let popup = ui.color_popup(parts.container).unwrap().clone();
        let viewport = ui.runtime.bounds(popup.scroll.viewport).unwrap();
        let right = viewport.x + viewport.width - 10.0;
        for channel in &popup.channels {
            ui.bring_into_view(channel.container).unwrap();
            ui.update(size).unwrap();
            for id in std::iter::once(channel.container)
                .chain(std::iter::once(channel.editor))
                .chain(channel.buttons[..2].iter().copied())
            {
                let bounds = ui.runtime.bounds(id).unwrap();
                let clip = ui.runtime.clip(id).unwrap();
                assert!(bounds.width > 0.0 && bounds.height > 0.0);
                assert!(
                    bounds.x >= viewport.x && bounds.x + bounds.width <= right,
                    "channel {id:?}: {bounds:?}, usable right={right}, viewport={viewport:?}"
                );
                assert_eq!(bounds.intersection(clip), Some(bounds));
            }
        }
        for id in popup.palette {
            ui.bring_into_view(id).unwrap();
            ui.update(size).unwrap();
            let bounds = ui.runtime.bounds(id).unwrap();
            assert!(bounds.width > 0.0 && bounds.height > 0.0);
            assert!(
                bounds.x >= viewport.x && bounds.x + bounds.width <= right,
                "swatch {id:?}: {bounds:?}, usable right={right}"
            );
            assert_eq!(
                bounds.intersection(ui.runtime.clip(id).unwrap()),
                Some(bounds)
            );
        }
    }
}
