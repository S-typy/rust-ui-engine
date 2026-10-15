use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, BindingConverter, BindingConverterRegistry, BindingValidator, CommitOutcome,
    Controls, DataValue, Theme,
};
use rust_desktop_ui_core::{Length, Size};
use std::{cell::RefCell, rc::Rc};

fn mount(
    content: &str,
    context: Rc<RefCell<BindingContext>>,
    converters: Rc<RefCell<BindingConverterRegistry>>,
) -> Result<(Controls, MountedView), String> {
    let source = format!(
        r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">{content}</Window>"#
    );
    let definition = rust_desktop_ui_xaml::parse(&source).map_err(|error| error.to_string())?;
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    let root = ui.root();
    let view = MountedView::mount_with_converters(
        &mut ui,
        root,
        &definition.content,
        context,
        converters,
    )?;
    Ok((ui, view))
}

fn context(value: DataValue) -> Rc<RefCell<BindingContext>> {
    Rc::new(RefCell::new(BindingContext::new(value).unwrap()))
}

fn converters() -> Rc<RefCell<BindingConverterRegistry>> {
    Rc::new(RefCell::new(BindingConverterRegistry::default()))
}

#[test]
fn inherited_context_and_element_name_have_distinct_source_scopes() {
    let model = context(DataValue::object([
        ("Name", "Outer".into()),
        ("Person", DataValue::object([("Name", "Inner".into())])),
    ]));
    let (mut ui, mut view) = mount(
        r#"<StackPanel>
        <TextBox x:Name="OuterEditor" Text="{Binding Name}"/>
        <StackPanel DataContext="{Binding Person}">
            <TextBox x:Name="InnerEditor" Text="{Binding Name}"/>
            <TextBlock x:Name="Mirror" Text="{Binding Text, ElementName=OuterEditor}"/>
        </StackPanel>
    </StackPanel>"#,
        model.clone(),
        converters(),
    )
    .unwrap();
    let inner = view.find_name("InnerEditor").unwrap();
    let mirror = view.find_name("Mirror").unwrap();
    assert_eq!(ui.document(inner).unwrap().text(), "Inner");
    assert_eq!(ui.label_text(mirror), Some("Outer"));
    ui.set_text(inner, "Changed").unwrap();
    ui.commit_edit(inner).unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(
        model.borrow().get("Person.Name").unwrap().as_str(),
        Some("Changed")
    );
    assert_eq!(model.borrow().get("Name").unwrap().as_str(), Some("Outer"));
}

#[test]
fn named_converters_are_registered_before_initial_transfer() {
    let model = context(DataValue::object([("Count", 3.5.into())]));
    let registry = converters();
    registry
        .borrow_mut()
        .register(
            "NumberText",
            BindingConverter::new(|value| {
                value
                    .as_number()
                    .map(|value| value.to_string().into())
                    .ok_or("Expected number".into())
            })
            .with_reverse(|value| {
                value
                    .as_str()
                    .and_then(|text| text.parse::<f64>().ok())
                    .map(DataValue::Number)
                    .ok_or("Expected numeric text".into())
            }),
        )
        .unwrap();
    let (mut ui, mut view) = mount(
        r#"<TextBox x:Name="Count" Text="{Binding Count, Converter=NumberText}"/>"#,
        model.clone(),
        registry,
    )
    .unwrap();
    let editor = view.find_name("Count").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "3.5");
    ui.set_text(editor, "12.25").unwrap();
    ui.commit_edit(editor).unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(
        model.borrow().get("Count").unwrap().as_number(),
        Some(12.25)
    );
    assert!(
        mount(
            r#"<TextBlock Text="{Binding Count, Converter=Missing}"/>"#,
            model,
            converters()
        )
        .is_err()
    );
}

#[test]
fn fallback_literals_use_the_actual_target_property_type() {
    let (ui, view) = mount(r#"<StackPanel>
        <SpinBox x:Name="Spin" Value="{Binding Missing, FallbackValue=12.5}"/>
        <DateEdit x:Name="Date" Value="{Binding Missing, FallbackValue=2028-02-29}"/>
        <TimeEdit x:Name="Time" Value="{Binding Missing, FallbackValue=13:45}"/>
        <TextBlock x:Name="Label" Visibility="{Binding Missing, FallbackValue=Collapsed}" Width="{Binding Missing, FallbackValue=Auto}"/>
    </StackPanel>"#, context(DataValue::Object(Default::default())), converters()).unwrap();
    assert_eq!(ui.spin_value(view.find_name("Spin").unwrap()), Some(12.5));
    let date = ui
        .composite_editor(view.find_name("Date").unwrap())
        .unwrap();
    let time = ui
        .composite_editor(view.find_name("Time").unwrap())
        .unwrap();
    assert_eq!(ui.document(date).unwrap().text(), "29.02.2028");
    assert_eq!(ui.document(time).unwrap().text(), "13:45");
    let props = &ui
        .runtime
        .tree()
        .node(view.find_name("Label").unwrap())
        .unwrap()
        .props;
    assert!(!props.visible);
    assert!(props.collapsed);
    assert_eq!(props.style.width, Length::Auto);
    assert_eq!(view.bindings.as_ref().unwrap().diagnostics().len(), 5);
}

#[test]
fn lookup_numeric_identity_survives_reorder_and_removal_clears_source() {
    let person = |id: f64, name: &str| {
        DataValue::object([
            ("Id", id.into()),
            ("Name", name.into()),
            ("Department", "Design".into()),
        ])
    };
    let model = context(DataValue::object([
        (
            "People",
            DataValue::List(vec![person(1.0, "Anna"), person(2.0, "Boris")]),
        ),
        ("Selected", 2.0.into()),
    ]));
    let (mut ui, mut view) = mount(r#"<LookupEdit x:Name="Lookup" ItemsSource="{Binding People}" ValueMemberPath="Id" DisplayMemberPath="Name" SelectedValue="{Binding Selected}">
        <LookupEdit.Columns><LookupColumn Field="Name" Header="Name"/><LookupColumn Field="Department" Header="Department"/></LookupEdit.Columns>
    </LookupEdit>"#, model.clone(), converters()).unwrap();
    let lookup = view.find_name("Lookup").unwrap();
    let selected_key = ui.selected_keys(lookup).unwrap()[0].clone();
    assert_eq!(
        ui.document(ui.composite_editor(lookup).unwrap())
            .unwrap()
            .text(),
        "Boris"
    );
    model
        .borrow_mut()
        .set(
            "People",
            DataValue::List(vec![person(2.0, "Boris"), person(1.0, "Anna")]),
        )
        .unwrap();
    view.sync_bindings(&mut ui);
    assert_eq!(ui.selected_keys(lookup).unwrap(), &[selected_key]);
    assert_eq!(
        model.borrow().get("Selected").unwrap().as_number(),
        Some(2.0)
    );
    model
        .borrow_mut()
        .set("People", DataValue::List(vec![person(1.0, "Anna")]))
        .unwrap();
    view.sync_bindings(&mut ui);
    assert!(ui.selected_keys(lookup).unwrap().is_empty());
    assert_eq!(model.borrow().get("Selected").unwrap(), &DataValue::Null);
    assert_eq!(
        ui.document(ui.composite_editor(lookup).unwrap())
            .unwrap()
            .text(),
        ""
    );
}

#[test]
fn element_two_way_rejects_before_either_accepted_value_changes() {
    let model = context(DataValue::object([("Name", "Anna".into())]));
    model
        .borrow_mut()
        .set_field_validator(
            "Name",
            Some(BindingValidator::new(|value| {
                if value.as_str() == Some("Forbidden") {
                    Err("Reserved name".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let (mut ui, mut view) = mount(
        r#"<StackPanel>
        <TextBox x:Name="Mirror" Text="{Binding Text, ElementName=Editor, Mode=TwoWay}"/>
        <TextBox x:Name="Editor" Text="{Binding Name}"/>
    </StackPanel>"#,
        model.clone(),
        converters(),
    )
    .unwrap();
    let mirror = view.find_name("Mirror").unwrap();
    let editor = view.find_name("Editor").unwrap();
    ui.set_text(mirror, "Forbidden").unwrap();
    assert!(matches!(
        ui.commit_edit(mirror).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.accepted_value(mirror), Some("Anna"));
    assert_eq!(ui.accepted_value(editor), Some("Anna"));
    assert_eq!(model.borrow().get("Name").unwrap().as_str(), Some("Anna"));
    ui.set_text(mirror, "Boris").unwrap();
    ui.commit_edit(mirror).unwrap();
    view.sync_bindings(&mut ui);
    view.sync_bindings(&mut ui);
    assert_eq!(model.borrow().get("Name").unwrap().as_str(), Some("Boris"));
}

#[test]
fn viewport_containers_stretch_but_explicit_height_and_alignment_are_respected() {
    for (markup, expected) in [
        (r#"<Grid x:Name="Host"/>"#, Length::Percent(1.0)),
        (r#"<Grid x:Name="Host" Height="200"/>"#, Length::Px(200.0)),
        (
            r#"<Grid x:Name="Host" VerticalAlignment="Top"/>"#,
            Length::Auto,
        ),
        (r#"<StackPanel x:Name="Host"/>"#, Length::Auto),
    ] {
        let (ui, view) = mount(
            markup,
            context(DataValue::Object(Default::default())),
            converters(),
        )
        .unwrap();
        assert_eq!(
            ui.runtime
                .tree()
                .node(view.find_name("Host").unwrap())
                .unwrap()
                .props
                .style
                .height,
            expected
        );
    }
}

#[test]
fn lookup_first_column_supplies_display_member_and_conflicting_path_is_rejected() {
    let model = context(DataValue::object([
        (
            "Items",
            DataValue::List(vec![DataValue::object([
                ("Id", "a".into()),
                ("Caption", "Alpha".into()),
                ("Other", "Detail".into()),
            ])]),
        ),
        ("Selected", "a".into()),
    ]));
    let markup = r#"<LookupEdit x:Name="Lookup" ItemsSource="{Binding Items}" ValueMemberPath="Id" SelectedValue="{Binding Selected}">
        <LookupEdit.Columns><LookupColumn Field="Caption" Header="Caption"/><LookupColumn Field="Other" Header="Other"/></LookupEdit.Columns>
    </LookupEdit>"#;
    let (ui, view) = mount(markup, model.clone(), converters()).unwrap();
    let lookup = view.find_name("Lookup").unwrap();
    assert_eq!(ui.selection_items(lookup).unwrap()[0].text, "Alpha");
    assert_eq!(
        ui.document(ui.composite_editor(lookup).unwrap())
            .unwrap()
            .text(),
        "Alpha"
    );
    assert!(
        mount(
            &markup.replace(
                "ValueMemberPath=\"Id\"",
                "ValueMemberPath=\"Id\" DisplayMemberPath=\"Other\""
            ),
            model,
            converters()
        )
        .is_err()
    );
}
