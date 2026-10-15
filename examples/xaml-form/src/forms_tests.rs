use crate::{form::Form, forms_model, generated_editors};
use rust_desktop_ui_controls::{CommitOutcome, DataValue};
use rust_desktop_ui_core::Size;
use rust_desktop_ui_platform_winit::DesktopApp;

#[test]
fn compiled_editors_form_round_trips_values_and_preserves_invalid_drafts() {
    let model = forms_model::model().unwrap();
    let mut form = Form::new_with_model(
        generated_editors::window_definition(),
        false,
        Some(model.clone()),
    )
    .unwrap();
    form.update(Size::new(1060.0, 800.0), 1.0).unwrap();
    let title = form.find_name("TitleEditor").unwrap();
    assert_eq!(form.ui.document(title).unwrap().text(), "Новая форма");
    form.ui.set_text(title, "Документ № 2").unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        model.borrow().get("Form.Title").unwrap().as_str(),
        Some("Новая форма")
    );
    assert!(matches!(
        form.ui.commit_edit(title).unwrap(),
        CommitOutcome::Committed
    ));
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        model.borrow().get("Form.Title").unwrap().as_str(),
        Some("Документ № 2")
    );
    form.ui.set_text(title, " ").unwrap();
    assert!(matches!(
        form.ui.commit_edit(title).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        model.borrow().get("Form.Title").unwrap().as_str(),
        Some("Документ № 2")
    );
    form.ui.cancel_edit(title).unwrap();
    assert_eq!(form.ui.document(title).unwrap().text(), "Документ № 2");
    model
        .borrow_mut()
        .set("Form.Title", "Из модели".into())
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.document(title).unwrap().text(), "Из модели");
}

#[test]
fn shared_selection_and_russian_temporal_display_use_typed_model_values() {
    let model = forms_model::model().unwrap();
    let mut form = Form::new_with_model(
        generated_editors::window_definition(),
        false,
        Some(model.clone()),
    )
    .unwrap();
    let list = form.find_name("TeamList").unwrap();
    let combo = form.find_name("PersonCombo").unwrap();
    let lookup = form.find_name("PersonLookup").unwrap();
    let date = form.find_name("DueDate").unwrap();
    let time = form.find_name("StartTime").unwrap();
    assert_eq!(
        form.ui
            .document(form.ui.composite_editor(date).unwrap())
            .unwrap()
            .text(),
        "15.10.2026"
    );
    assert_eq!(
        form.ui
            .document(form.ui.composite_editor(time).unwrap())
            .unwrap()
            .text(),
        "09:30"
    );
    form.ui.set_selected_keys(list, &["ivan".into()]).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        model.borrow().get("Form.Person").unwrap(),
        &DataValue::from("ivan")
    );
    assert_eq!(form.ui.selected_keys(combo).unwrap(), &["ivan"]);
    assert_eq!(form.ui.selected_keys(lookup).unwrap(), &["ivan"]);
    let note = form.find_name("Note").unwrap();
    form.ui.set_text(note, "Явное сохранение").unwrap();
    form.ui.commit_edit(note).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        model.borrow().get("Form.Note").unwrap().as_str(),
        Some("Заметка к документу")
    );
    form.view
        .bindings
        .as_mut()
        .unwrap()
        .update_sources(&mut form.ui);
    assert_eq!(
        model.borrow().get("Form.Note").unwrap().as_str(),
        Some("Явное сохранение")
    );
}

#[test]
fn grid_tracks_resize_without_overlapping_form_columns() {
    let mut form = Form::new_with_model(
        generated_editors::window_definition(),
        false,
        Some(forms_model::model().unwrap()),
    )
    .unwrap();
    for width in [1060.0, 900.0, 1200.0] {
        form.update(Size::new(width, 800.0), 1.0).unwrap();
        let list = form
            .ui
            .runtime
            .bounds(form.find_name("TeamList").unwrap())
            .unwrap();
        let title = form
            .ui
            .runtime
            .bounds(form.find_name("TitleEditor").unwrap())
            .unwrap();
        assert!(
            list.width > 200.0 && title.width > 200.0,
            "{list:?} {title:?}"
        );
        assert!(list.x + list.width <= title.x, "{list:?} {title:?}");
        assert!(title.x + title.width <= width, "{title:?}");
    }
}
