use crate::view::MountedView;
use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::{LayoutStyle, Mutation, Size};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn definition(content: &str) -> rust_desktop_ui_xaml::WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
{content}
</Window>"#)).unwrap()
}
fn context(value: DataValue) -> Rc<RefCell<BindingContext>> {
    Rc::new(RefCell::new(BindingContext::new(value).unwrap()))
}
fn mount(content: &str, model: Rc<RefCell<BindingContext>>) -> (Controls, MountedView) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.update(Size::new(900.0, 700.0)).unwrap();
    let root = ui.root();
    let view = MountedView::mount_with_context(&mut ui, root, &definition(content).content, model)
        .unwrap();
    (ui, view)
}

#[test]
fn multi_binding_inherits_context_and_orders_element_dependencies() {
    let model = context(DataValue::object([
        ("Name", "Outer".into()),
        ("Person", DataValue::object([("Count", 3.0.into())])),
    ]));
    // The dependent label deliberately precedes its ElementName source.
    let (mut ui, mut view) = mount(
        r#"<StackPanel>
        <StackPanel DataContext="{Binding Person}">
            <TextBlock x:Name="Summary" Text="{MultiBinding Paths='Count;#Source.Text', Expression='concat($1, &quot; / &quot;, $0)'}"/>
        </StackPanel>
        <TextBlock x:Name="Source" Text="{Binding Name}"/>
    </StackPanel>"#,
        model.clone(),
    );
    let summary = view.find_name("Summary").unwrap();
    assert_eq!(ui.label_text(summary), Some("Outer / 3"));
    model
        .borrow_mut()
        .set_many([("Name", "Changed".into()), ("Person.Count", 7.0.into())])
        .unwrap();
    assert!(view.sync_bindings(&mut ui));
    assert_eq!(ui.label_text(summary), Some("Changed / 7"));
    assert!(view.bindings.as_ref().unwrap().diagnostics().is_empty());
}

#[test]
fn multi_binding_converter_gets_ordered_values() {
    let model = context(DataValue::object([("A", 4.0.into()), ("B", 9.0.into())]));
    let converters = Rc::new(RefCell::new(BindingConverterRegistry::default()));
    converters
        .borrow_mut()
        .register(
            "Pair",
            BindingConverter::new(|value| {
                let values = value.as_list().ok_or("Expected multiple inputs")?;
                Ok(format!(
                    "{}:{}",
                    values[0].as_number().unwrap(),
                    values[1].as_number().unwrap()
                )
                .into())
            }),
        )
        .unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let view = MountedView::mount_with_converters(
        &mut ui,
        root,
        &definition(
            r#"<TextBlock x:Name="Pair" Text="{MultiBinding Paths='B;A', Converter=Pair}"/>"#,
        )
        .content,
        model,
        converters,
    )
    .unwrap();
    assert_eq!(ui.label_text(view.find_name("Pair").unwrap()), Some("9:4"));
}

#[test]
fn failed_mount_restores_existing_editor_focus_motion_and_nodes() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let editor = ui
        .edit_box(root, "Existing", "Accepted", LayoutStyle::default())
        .unwrap();
    ui.set_text(editor, "Unsaved draft").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.update(Size::new(900.0, 700.0)).unwrap();
    ui.set_reduced_motion(false);
    let before = ui.runtime.tree().live_ids();
    let model = context(DataValue::object([("Name", "Model".into())]));
    for markup in [
        r#"<StackPanel><BusyIndicator ReducedMotion="True"/><TextBlock Text="{Binding Name, Converter=Absent}"/></StackPanel>"#,
        r#"<StackPanel><TextBlock x:Name="A" Text="{MultiBinding Paths='#B.Text', Expression='$0'}"/><TextBlock x:Name="B" Text="{MultiBinding Paths='#A.Text', Expression='$0'}"/></StackPanel>"#,
    ] {
        let result = MountedView::mount_with_context(
            &mut ui,
            root,
            &definition(markup).content,
            model.clone(),
        );
        assert!(result.is_err());
        assert_eq!(ui.runtime.tree().live_ids(), before);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert!(!ui.reduced_motion());
        assert_eq!(ui.document(editor).unwrap().text(), "Unsaved draft");
        assert_eq!(ui.accepted_value(editor), Some("Accepted"));
        assert_eq!(model.borrow().revision(), 0);
    }
    let bad =
        definition(r#"<TextBlock Text="{MultiBinding Paths='Name', Expression='unknown($0)'}"/>"#);
    let error = MountedView::mount_with_context(&mut ui, root, &bad.content, model)
        .err()
        .unwrap();
    assert!(error.starts_with("2:"), "{error}");
    assert_eq!(ui.runtime.tree().live_ids(), before);
}

#[test]
fn nullable_check_radio_items_and_selection_round_trip() {
    let rows = |reverse: bool| {
        let mut values = vec![
            DataValue::object([("key", "one".into()), ("label", "One".into())]),
            DataValue::object([("key", "two".into()), ("label", "Two".into())]),
        ];
        if reverse {
            values.reverse();
        }
        DataValue::List(values)
    };
    let model = context(DataValue::object([
        ("Checked", DataValue::Null),
        ("Items", rows(false)),
        ("Selected", "two".into()),
    ]));
    let (mut ui, mut view) = mount(
        r#"<StackPanel>
        <CheckBox x:Name="Mixed" IsThreeState="True" IsChecked="{Binding Checked}"/>
        <RadioGroup x:Name="Choices" ItemsSource="{Binding Items}" SelectedKey="{Binding Selected}" DisplayMemberPath="label" ValueMemberPath="key"/>
    </StackPanel>"#,
        model.clone(),
    );
    let check = view.find_name("Mixed").unwrap();
    let radio = view.find_name("Choices").unwrap();
    assert_eq!(ui.check_state(check), Some(CheckState::Indeterminate));
    assert_eq!(ui.radio_selected_key(radio), Some("two"));
    let selected_widget = ui.radio_item_widget(radio, "two").unwrap();
    model.borrow_mut().set("Items", rows(true)).unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.radio_selected_key(radio), Some("two"));
    assert_eq!(ui.radio_item_widget(radio, "two"), Some(selected_widget));
    ui.set_check_state(check, CheckState::Checked).unwrap();
    ui.set_radio_selected_key(radio, Some("one")).unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(
        model.borrow().get("Checked").unwrap(),
        &DataValue::Bool(true)
    );
    assert_eq!(
        model.borrow().get("Selected").unwrap(),
        &DataValue::from("one")
    );
    assert!(view.bindings.as_ref().unwrap().diagnostics().is_empty());
}

#[test]
fn range_pair_updates_atomically_in_both_directions() {
    let model = context(DataValue::object([
        ("Lower", 10.0.into()),
        ("Upper", 20.0.into()),
    ]));
    model
        .borrow_mut()
        .set_validator(Some(
            BindingValidator::from_rules(vec![CrossFieldRule {
                paths: ["Lower", "Upper"]
                    .into_iter()
                    .map(|path| BindingPath::parse(path).unwrap())
                    .collect(),
                condition: BindingExpression::parse("$0 <= $1").unwrap(),
                message: "Ordered range required".into(),
            }])
            .unwrap(),
        ))
        .unwrap();
    let (mut ui, mut view) = mount(
        r#"<RangeSlider x:Name="Range" LowerValue="{Binding Lower}" UpperValue="{Binding Upper}"/>"#,
        model.clone(),
    );
    let range = view.find_name("Range").unwrap();
    assert_eq!(ui.range_slider_values(range), Some((10.0, 20.0)));
    model
        .borrow_mut()
        .set_many([("Lower", 70.0.into()), ("Upper", 80.0.into())])
        .unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.range_slider_values(range), Some((70.0, 80.0)));
    ui.set_range_slider_values(range, 5.0, 15.0).unwrap();
    let revision = model.borrow().revision();
    view.sync_bindings(&mut ui);
    assert_eq!(model.borrow().revision(), revision + 1);
    assert_eq!(model.borrow().get("Lower").unwrap(), &DataValue::from(5.0));
    assert_eq!(model.borrow().get("Upper").unwrap(), &DataValue::from(15.0));
    assert!(view.bindings.as_ref().unwrap().diagnostics().is_empty());
}

#[test]
fn range_pair_waits_for_both_element_dependencies() {
    let model = context(DataValue::object([
        ("Lower", 70.0.into()),
        ("Upper", 80.0.into()),
    ]));
    let (mut ui, mut view) = mount(
        r#"<StackPanel>
        <RangeSlider x:Name="Range" LowerValue="{Binding Value, ElementName=Lower, Mode=OneWay}" UpperValue="{Binding Value, ElementName=Upper, Mode=OneWay}"/>
        <Slider x:Name="Upper" Value="{Binding Upper}"/>
        <Slider x:Name="Lower" Value="{Binding Lower}"/>
    </StackPanel>"#,
        model.clone(),
    );
    let range = view.find_name("Range").unwrap();
    assert_eq!(ui.range_slider_values(range), Some((70.0, 80.0)));
    model
        .borrow_mut()
        .set_many([("Lower", 10.0.into()), ("Upper", 20.0.into())])
        .unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.range_slider_values(range), Some((10.0, 20.0)));
    assert!(view.bindings.as_ref().unwrap().diagnostics().is_empty());
}

#[test]
fn async_binding_wakes_ui_and_locale_change_preserves_draft() {
    let model = context(DataValue::object([("Code", "Accepted".into())]));
    let (mut ui, mut view) = mount(
        r#"<TextBox x:Name="Editor" Text="{Binding Code}"/>"#,
        model.clone(),
    );
    let editor = view.find_name("Editor").unwrap();
    let (send, receive) = mpsc::channel();
    let receive = Arc::new(Mutex::new(receive));
    ui.set_async_editor_validator(
        editor,
        Some(AsyncEditorValidator::new(move |_| {
            receive.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    })));
    let mut localizer = Localizer::default();
    localizer
        .register(UiCulture::English, "form", "hint", "Enter code".into())
        .unwrap();
    let mut labels = LocalizationBindings::default();
    labels
        .add(editor, LocalizedProperty::Placeholder, "form", "hint", None)
        .unwrap();
    ui.set_text(editor, "Draft").unwrap();
    assert_eq!(ui.commit_edit(editor).unwrap(), CommitOutcome::Pending);
    view.sync_bindings(&mut ui);
    assert_eq!(
        model.borrow().get("Code").unwrap(),
        &DataValue::from("Accepted")
    );
    localizer.set_culture(UiCulture::English);
    labels.sync(&mut ui, &localizer).unwrap();
    ui.set_editor_validation_context(
        editor,
        model.borrow().root().clone(),
        model.borrow().revision(),
        UiCulture::English,
    )
    .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert_eq!(
        ui.editor_settings(editor).unwrap().placeholder,
        "Enter code"
    );
    send.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while wakes.load(Ordering::SeqCst) < 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    ui.poll_editor_validation().unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.accepted_value(editor), Some("Accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert_eq!(ui.commit_edit(editor).unwrap(), CommitOutcome::Pending);
    send.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while ui.accepted_value(editor) != Some("Draft") && Instant::now() < deadline {
        ui.poll_editor_validation().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(ui.accepted_value(editor), Some("Draft"));
    view.sync_bindings(&mut ui);
    assert_eq!(
        model.borrow().get("Code").unwrap(),
        &DataValue::from("Draft")
    );
}

#[test]
fn popup_only_split_disclosure_keeps_primary_and_retained_editor_independent() {
    let definition = definition(
        r#"<StackPanel>
        <SplitButton x:Name="Apply" Content="Apply" Command="Apply"/>
        <Popup x:Name="OptionsPopup" PlacementTarget="Apply">
            <TextBox x:Name="OptionsEditor" Text="Accepted" CommitOnLostFocus="False"/>
        </Popup>
        <DropDownButton x:Name="More" Content="More"/>
        <Popup PlacementTarget="More"><TextBlock Text="Retained details"/></Popup>
    </StackPanel>"#,
    );
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("Apply", "Apply");
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let root = ui.root();
    let view = MountedView::mount(&mut ui, root, &definition.content).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let split = view.splits[&view.find_name("Apply").unwrap()];
    let popup = ui.button_flyout(split.dropdown).unwrap();
    let editor = view.find_name("OptionsEditor").unwrap();
    assert_eq!(popup.container, view.find_name("OptionsPopup").unwrap());
    ui.activate(split.primary).unwrap();
    assert_eq!(ui.drain_command_invocations().len(), 1);
    assert!(!ui.flyout_is_open(popup));
    ui.activate(split.dropdown).unwrap();
    assert!(ui.flyout_is_open(popup));
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(ui.drain_command_invocations().is_empty());
    ui.set_text(editor, "Draft").unwrap();
    ui.close_flyout(popup).unwrap();
    ui.activate(split.dropdown).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert_eq!(ui.accepted_value(editor), Some("Accepted"));
    ui.activate(split.primary).unwrap();
    assert_eq!(ui.drain_command_invocations().len(), 1);
    let more = view.find_name("More").unwrap();
    ui.activate(more).unwrap();
    assert!(ui.flyout_is_open(ui.button_flyout(more).unwrap()));
    assert!(!ui.flyout_is_open(popup));
    assert!(ui.drain_command_invocations().is_empty());
}

#[test]
fn form_tick_rejects_queued_validation_after_model_context_change_without_redraw() {
    use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent};
    let model = context(DataValue::object([
        ("Code", "Accepted".into()),
        ("Other", 0.0.into()),
    ]));
    let mut form = crate::form::Form::new_with_model(
        definition(r#"<TextBox x:Name="BatchCode" Text="{Binding Code}"/>"#),
        false,
        Some(model.clone()),
    )
    .unwrap();
    form.update(Size::new(400.0, 200.0), 1.0).unwrap();
    let editor = form.find_name("BatchCode").unwrap();
    let (send, receive) = mpsc::channel();
    let receive = Arc::new(Mutex::new(receive));
    form.ui
        .set_async_editor_validator(
            editor,
            Some(AsyncEditorValidator::new(move |_| {
                receive.lock().unwrap().recv().unwrap();
                Ok(())
            })),
        )
        .unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    form.ui.set_editor_validation_waker(Some(Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    })));
    form.ui.set_text(editor, "Draft").unwrap();
    assert_eq!(form.ui.commit_edit(editor).unwrap(), CommitOutcome::Pending);
    send.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while wakes.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    model.borrow_mut().set("Other", 1.0.into()).unwrap();
    // No Form::update/redraw occurs before this timer event.
    form.event(PlatformEvent::Tick).unwrap();
    assert_eq!(form.ui.accepted_value(editor), Some("Accepted"));
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft");
    assert_eq!(
        model.borrow().get("Code").unwrap(),
        &DataValue::from("Accepted")
    );
}
