use super::*;
use rust_desktop_ui_data::{
    CellValue, ComparisonOperator, Criteria, FieldSchema, FieldType, QueryExpression, TableSchema,
    parse_criteria_text,
};
fn schema() -> TableSchema {
    TableSchema::new(vec![
        FieldSchema::new("Count", FieldType::Integer),
        FieldSchema::new("Name", FieldType::Text),
    ])
    .unwrap()
}
fn fixture() -> (Controls, QueryBuilderParts) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let p = ui
        .query_builder(
            ui.root(),
            schema(),
            QueryBuilderValue::Criteria(Criteria::Constant(true)),
            Default::default(),
            LayoutStyle {
                width: Length::Px(900.0),
                height: Length::Px(520.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(900.0, 520.0)).unwrap();
    (ui, p)
}
fn button(ui: &Controls, label: &str) -> WidgetId {
    ui.controls
        .iter()
        .find_map(|(&id, c)| (c.label == label).then_some(id))
        .unwrap()
}
#[test]
fn visual_and_text_produce_identical_typed_criteria_apply_once() {
    let (mut ui, p) = fixture();
    ui.set_text(p.operand, "9007199254740993").unwrap();
    let replace = button(&ui, "Заменить выбранный узел");
    ui.activate(replace).unwrap();
    let expected = QueryBuilderValue::Criteria(Criteria::Compare {
        left: QueryExpression::field("Count"),
        operator: ComparisonOperator::Equal,
        right: QueryExpression::Literal(CellValue::Integer(9007199254740993)),
    });
    assert_eq!(ui.query_builder_draft(p.container), Some(&expected));
    assert_eq!(
        ui.query_builder_value(p.container),
        Some(&QueryBuilderValue::Criteria(Criteria::Constant(true)))
    );
    ui.set_query_builder_mode(p.container, QueryBuilderMode::Text)
        .unwrap();
    assert!(ui.apply_query_builder(p.container).unwrap());
    assert!(!ui.apply_query_builder(p.container).unwrap());
    assert_eq!(ui.take_query_builder_applied().len(), 1);
    assert_eq!(ui.query_builder_value(p.container), Some(&expected));
    assert_eq!(ui.query_builder_parts(p.container).unwrap().text, p.text);
}
#[test]
fn invalid_text_cancel_conflict_validator_and_readonly_preserve_accepted() {
    let (mut ui, p) = fixture();
    ui.set_query_builder_text(p.container, r#"eq(field("Count"),text("bad"))"#)
        .unwrap();
    assert!(ui.query_builder_error(p.container).is_some());
    assert!(ui.apply_query_builder(p.container).is_err());
    let accepted = ui.query_builder_value(p.container).unwrap().clone();
    assert!(
        ui.set_query_builder_value(p.container, accepted.clone(), false)
            .is_err()
    );
    ui.cancel_query_builder(p.container).unwrap();
    assert_eq!(ui.query_builder_text(p.container), Some("true()"));
    ui.set_query_builder_validator(
        p.container,
        Some(QueryBuilderValidator::new(|_| Err("Rejected".into()))),
    )
    .unwrap();
    ui.set_query_builder_text(p.container, "false()").unwrap();
    assert!(ui.apply_query_builder(p.container).is_err());
    assert_eq!(ui.query_builder_value(p.container), Some(&accepted));
    assert!(ui.take_query_builder_applied().is_empty());
    ui.set_query_builder_read_only(p.container, true).unwrap();
    assert!(ui.apply_query_builder(p.container).is_err());
    ui.cancel_query_builder(p.container).unwrap();
}
#[test]
fn groups_retain_controls_and_unsupported_visual_recipe_is_diagnostic() {
    let (mut ui, p) = fixture();
    let and = button(&ui, "И");
    ui.activate(and).unwrap();
    assert!(
        matches!(ui.query_builder_draft(p.container),Some(QueryBuilderValue::Criteria(Criteria::All(v)))if v.len()==2)
    );
    let not = button(&ui, "НЕ");
    ui.activate(not).unwrap();
    let ids = (p.tree, p.text, p.operand);
    ui.set_query_builder_mode(p.container, QueryBuilderMode::Text)
        .unwrap();
    let complex = r#"eq(add(field("Count"),integer("1")),integer("2"))"#;
    ui.set_query_builder_text(p.container, complex).unwrap();
    assert!(
        ui.set_query_builder_mode(p.container, QueryBuilderMode::Visual)
            .is_err()
    );
    assert_eq!(ui.query_builder_text(p.container), Some(complex));
    assert_eq!(
        ui.query_builder_value(p.container),
        Some(&QueryBuilderValue::Criteria(Criteria::Constant(true)))
    );
    ui.update(Size::new(750.0, 500.0)).unwrap();
    ui.set_theme(Theme::FluentDark).unwrap();
    assert_eq!(ids, (p.tree, p.text, p.operand));
    ui.cancel_query_builder(p.container).unwrap();
}
#[test]
fn required_parts_and_shape_limits_reject_before_mutation() {
    let (mut ui, p) = fixture();
    let before = ui.query_builder_value(p.container).unwrap().clone();
    assert!(ui.remove(p.operand).is_err());
    let other = ui.transparent_panel(ui.root(), Default::default()).unwrap();
    ui.runtime
        .apply(Mutation::Reparent {
            node: p.operand,
            parent: other,
        })
        .unwrap();
    assert!(
        ui.set_query_builder_value(
            p.container,
            QueryBuilderValue::Criteria(Criteria::Constant(false)),
            true
        )
        .is_err()
    );
    assert_eq!(ui.query_builder_value(p.container), Some(&before));
    ui.runtime
        .apply(Mutation::Reparent {
            node: p.operand,
            parent: ui.runtime.tree().parent(p.text).unwrap(),
        })
        .unwrap();
    assert!(
        ui.set_query_builder_text(p.container, &"x".repeat(16385))
            .is_err()
    );
    assert_eq!(ui.query_builder_text(p.container), Some("true()"));
}
#[test]
fn actual_apply_pointer_rejection_is_nonfatal_and_geometry_visible() {
    let (mut ui, p) = fixture();
    ui.set_query_builder_text(p.container, "broken(").unwrap();
    let b = ui.runtime.bounds(p.apply).unwrap();
    assert!(b.width > 50.0 && b.height >= 32.0);
    assert_eq!(b.intersection(ui.runtime.clip(p.apply).unwrap()), Some(b));
    let position = Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0);
    for event in [
        InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        },
        InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        },
    ] {
        ui.dispatch(event).unwrap();
    }
    assert!(ui.query_builder_error(p.container).is_some());
    assert!(ui.take_query_builder_applied().is_empty());
    assert_eq!(
        parse_criteria_text(&schema(), "true()").unwrap(),
        Criteria::Constant(true)
    );
}

#[test]
fn keyboard_escape_cancels_composition_before_draft_and_control_enter_applies() {
    let (mut ui, p) = fixture();
    ui.runtime.apply(Mutation::Focus(Some(p.text))).unwrap();
    ui.set_query_builder_text(p.container, "false()").unwrap();
    ui.ime_preedit("draft", None).unwrap();
    let escape = InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Default::default(),
        repeat: false,
    };
    ui.dispatch(escape.clone()).unwrap();
    assert_eq!(ui.query_builder_text(p.container), Some("false()"));
    assert!(ui.document(p.text).unwrap().composition().is_none());
    ui.dispatch(escape).unwrap();
    assert_eq!(ui.query_builder_text(p.container), Some("true()"));
    ui.set_query_builder_text(p.container, "false()").unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: rust_desktop_ui_core::Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.take_query_builder_applied().len(), 1);
}
#[test]
fn source_validator_remains_active_after_application_validator_changes() {
    use crate::{
        BindingContext, BindingProperty, BindingValidator, ControlBinding, ControlBindingMode,
        ControlBindings, DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, p) = fixture();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Filter", "true()".into())])).unwrap(),
    ));
    model
        .borrow_mut()
        .set_field_validator(
            "Filter",
            Some(BindingValidator::new(|v| {
                if *v == DataValue::String("false()".into()) {
                    Err("No false query".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let mut bindings = ControlBindings::new(model.clone());
    let mut binding = ControlBinding::new(p.container, BindingProperty::Value, "Filter").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.set_query_builder_validator(p.container, None).unwrap();
    ui.set_query_builder_text(p.container, "false()").unwrap();
    assert!(ui.apply_query_builder(p.container).is_err());
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Filter").unwrap(),
        &DataValue::String("true()".into())
    );
    assert_eq!(
        ui.query_builder_value(p.container),
        Some(&QueryBuilderValue::Criteria(Criteria::Constant(true)))
    );
}

#[test]
fn owned_composition_blocks_replacement_modes_and_visual_apply_until_explicit_cancel() {
    let (mut ui, p) = fixture();
    let accepted = ui.query_builder_value(p.container).unwrap().clone();
    for editor in [p.text, p.operand] {
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.ime_preedit("pending composition", None).unwrap();
        let composition = ui.document(editor).unwrap().composition().cloned();
        assert!(
            ui.set_query_builder_value(p.container, accepted.clone(), false)
                .is_err()
        );
        assert!(
            ui.set_query_builder_draft(p.container, accepted.clone())
                .is_err()
        );
        assert!(ui.set_query_builder_text(p.container, "false()").is_err());
        assert!(
            ui.set_query_builder_mode(p.container, QueryBuilderMode::Text)
                .is_err()
        );
        assert!(ui.apply_query_builder(p.container).is_err());
        let replace = button(&ui, "Заменить выбранный узел");
        ui.activate(replace).unwrap();
        assert_eq!(
            ui.document(editor).unwrap().composition(),
            composition.as_ref()
        );
        assert_eq!(ui.query_builder_value(p.container), Some(&accepted));
        assert_eq!(ui.query_builder_text(p.container), Some("true()"));
        assert!(ui.take_query_builder_applied().is_empty());
        ui.cancel_query_builder(p.container).unwrap();
        assert!(ui.document(editor).unwrap().composition().is_none());
    }
    ui.runtime.apply(Mutation::Focus(Some(p.text))).unwrap();
    ui.ime_preedit("discard explicitly", None).unwrap();
    ui.set_query_builder_value(p.container, accepted, true)
        .unwrap();
    assert!(ui.document(p.text).unwrap().composition().is_none());
}

#[test]
fn inspector_tracks_fill_available_width_and_show_wrapped_error() {
    for width in [700.0, 900.0, 1100.0] {
        let (mut ui, p) = fixture();
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            ui.set_theme(theme).unwrap();
            ui.edit_props(p.container, |props| props.style.width = Length::Px(width))
                .unwrap();
            ui.set_query_builder_text(p.container, r#"eq(field("Count"),text("bad"))"#)
                .unwrap();
            // Controls' root has an eight-DIP inset on each side.
            ui.update(Size::new(width + 16.0, 536.0)).unwrap();
            for id in [p.field.container, p.operator.container, p.operand, p.text] {
                let b = ui.runtime.bounds(id).unwrap();
                assert!(
                    b.width >= (width - 8.0) * 0.59,
                    "width={width}, editor={b:?}"
                );
                assert_eq!(b.intersection(ui.runtime.clip(id).unwrap()), Some(b));
            }
            let replace = button(&ui, "Заменить выбранный узел");
            let b = ui.runtime.bounds(replace).unwrap();
            let needed = ui
                .measure_text("Заменить выбранный узел", 4096.0, false)
                .unwrap();
            assert!(needed.width + 16.0 <= b.width);
            let status = ui.runtime.bounds(p.status).unwrap();
            let message = ui.query_builder_error(p.container).unwrap().to_owned();
            let needed = ui
                .measure_text(&message, status.width - 16.0, true)
                .unwrap();
            assert!(
                status.height + 0.01 >= needed.height,
                "status={status:?}, text={needed:?}"
            );
            assert_eq!(
                status.intersection(ui.runtime.clip(p.status).unwrap()),
                Some(status)
            );
        }
    }
}
