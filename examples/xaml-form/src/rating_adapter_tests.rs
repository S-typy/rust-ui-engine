use super::*;
use crate::rating_model as model;
use rust_desktop_ui_controls::{BindingContext, DataValue, RatingPreview, SemanticAction, Theme};
use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation, Size};
use std::{cell::RefCell, rc::Rc};

fn parse(body: &str) -> ElementDefinition {
    rust_desktop_ui_xaml::parse(&format!(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">{body}</Window>"#)).unwrap().content
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}

#[test]
fn rating_compiled_nullable_binding_publishes_commit_and_readonly_blocks_input() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Score", DataValue::Null),
            ("Locked", false.into()),
        ]))
        .unwrap(),
    ));
    let content = parse(
        r#"<Rating x:Name="Score" Value="{Binding Score}" IsReadOnly="{Binding Locked}" Maximum="5" Step="0.5"/>"#,
    );
    let root = ui.root();
    let mut view = MountedView::mount_with_context(&mut ui, root, &content, model.clone()).unwrap();
    let id = view.names["Score"];
    ui.update(Size::new(600.0, 400.0)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowRight);
    view.sync_bindings(&mut ui);
    assert_eq!(ui.rating_preview(id), Some(RatingPreview::Value(Some(0.5))));
    assert_eq!(model.borrow().get("Score").unwrap(), &DataValue::Null);
    key(&mut ui, Key::Enter);
    view.sync_bindings(&mut ui);
    assert_eq!(
        model.borrow().get("Score").unwrap(),
        &DataValue::Number(0.5)
    );
    model.borrow_mut().set("Locked", true.into()).unwrap();
    view.sync_bindings(&mut ui);
    ui.perform_action(id, SemanticAction::SetValue, Some("4"))
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(0.5)));
    model
        .borrow_mut()
        .set("Score", DataValue::Number(0.0))
        .unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.rating_value(id), Some(Some(0.0)));
    model.borrow_mut().set("Score", DataValue::Null).unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.rating_value(id), Some(None));
}

#[test]
fn rating_bound_nonnullable_initializes_from_model_and_invalid_definition_is_atomic() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Score", DataValue::Number(2.345))])).unwrap(),
    ));
    let mut content =
        parse(r#"<Rating x:Name="Score" Value="{Binding Score}" AllowEmpty="False"/>"#);
    let root = ui.root();
    let view = MountedView::mount_with_context(&mut ui, root, &content, model.clone()).unwrap();
    assert_eq!(ui.rating_value(view.names["Score"]), Some(Some(2.345)));
    let before = ui.runtime.tree().live_ids();
    let ElementKind::Rating(rating) = &mut content.kind else {
        panic!()
    };
    rating.maximum = 0;
    assert!(MountedView::mount_with_context(&mut ui, root, &content, model).is_err());
    assert_eq!(ui.runtime.tree().live_ids(), before);
    let fallback = parse(r#"<Rating Value="{Binding Missing, FallbackValue=null}"/>"#);
    assert!(MountedView::mount(&mut ui, root, &fallback).is_ok());
}

#[test]
fn rating_demo_commands_distinguish_null_zero_and_rejection_and_drain_events() {
    let definition = rust_desktop_ui_xaml::parse(include_str!("../RatingWindow.xaml")).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    model::register(&mut ui);
    ui.commands_mut().register("Light", "Light");
    ui.commands_mut().register("Dark", "Dark");
    let root = ui.root();
    let context = model::model().unwrap();
    let mut view =
        MountedView::mount_with_context(&mut ui, root, &definition.content, context.clone())
            .unwrap();
    let mut state = model::State::install(&mut ui, &view).unwrap().unwrap();
    ui.update(Size::new(960.0, 800.0)).unwrap();
    let id = view.names["QualityScore"];
    assert_eq!(ui.rating_value(id), Some(Some(2.5)));
    for (command, expected) in [
        ("RatingDemoEmpty", None),
        ("RatingDemoZero", Some(0.0)),
        ("RatingDemoFraction", Some(2.5)),
    ] {
        assert!(state.command(&mut ui, command).unwrap());
        view.sync_bindings(&mut ui);
        state.refresh(&mut ui).unwrap();
        assert_eq!(ui.rating_value(id), Some(expected));
    }
    state.command(&mut ui, "RatingDemoReject").unwrap();
    state.refresh(&mut ui).unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(2.5)));
    assert!(
        ui.label_text(view.names["RatingError"])
            .unwrap()
            .contains("не должна превышать")
    );
    state.command(&mut ui, "RatingDemoFocus").unwrap();
    key(&mut ui, Key::ArrowRight);
    state.refresh(&mut ui).unwrap();
    assert!(
        ui.label_text(view.names["RatingPreview"])
            .unwrap()
            .contains("значение ещё не принято")
    );
    assert_eq!(
        context.borrow().get("Score").unwrap(),
        &DataValue::Number(2.5)
    );
    key(&mut ui, Key::Enter);
    view.sync_bindings(&mut ui);
    state.refresh(&mut ui).unwrap();
    assert_eq!(
        context.borrow().get("Score").unwrap(),
        &DataValue::Number(3.0)
    );
    assert_eq!(
        ui.label_text(view.names["RatingActivity"]),
        Some("Подтверждений: 1")
    );
    assert!(ui.drain_rating_changed().is_empty());
}
