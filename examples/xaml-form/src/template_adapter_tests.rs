use crate::{form::Form, view::MountedView};
use rust_desktop_ui_controls::{BindingContext, Controls, DataValue, Role, Theme};
use rust_desktop_ui_core::{Mutation, Size, WidgetId};
use rust_desktop_ui_platform_winit::DesktopApp;
use rust_desktop_ui_xaml::{ControlTemplateDefinition, ElementDefinition, ElementKind};
use std::{cell::RefCell, rc::Rc};

const SIZE: Size = Size::new(640.0, 480.0);

#[test]
fn compiled_item_templates_and_multiple_combo_bind_only_accepted_keys() {
    use rust_desktop_ui_controls::{SelectionValidator, SemanticAction};
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Tags", DataValue::List(vec!["a".into()])),
            ("Free", "custom value".into()),
        ]))
        .unwrap(),
    ));
    let markup = parse(
        r#"<Window.Resources><DataTemplate x:Key="Row"><StackPanel Orientation="Horizontal" Spacing="6"><Icon Symbol="Copy" Width="16"/><TextBlock Text="{Binding Text}"/><TextBlock Text="{Binding Key}"/></StackPanel></DataTemplate></Window.Resources>
    <StackPanel><ListBox x:Name="Checked" SelectionMode="Multiple" ItemPresentation="CheckBox" ItemTemplate="{StaticResource Row}" Height="100"><ListBoxItem Value="a" Content="Alpha"/><ListBoxItem Value="b" Content="Beta"/></ListBox>
    <ComboBox x:Name="Tags" IsEditable="True" SelectionMode="Multiple" SelectionPresentation="Tokens" SelectedValues="{Binding Tags}" ItemTemplate="{StaticResource Row}"><ListBoxItem Value="a" Content="Alpha"/><ListBoxItem Value="b" Content="Beta"/></ComboBox>
    <ComboBox x:Name="Free" IsEditable="True" Text="{Binding Free}"/></StackPanel>"#,
    );
    let mut form = Form::new_with_model(markup, false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let tags = form.find_name("Tags").unwrap();
    let list = form.find_name("Checked").unwrap();
    let free = form.find_name("Free").unwrap();
    assert!(form.ui.list_item_content(list, "a").is_some());
    assert_eq!(form.ui.selected_keys(tags).unwrap(), ["a"]);
    assert!(form.ui.combo_item_template(tags).is_some());
    assert_eq!(
        form.ui
            .accepted_value(form.ui.composite_editor(free).unwrap()),
        Some("custom value")
    );
    let token = form.ui.combo_token(tags, "a").unwrap();
    form.ui
        .chain_selection_validator(
            tags,
            SelectionValidator::new(|keys| {
                if keys.is_empty() {
                    Err("Select at least one".into())
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
    form.ui
        .perform_action(token, SemanticAction::Activate, None)
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.selected_keys(tags).unwrap(), ["a"]);
    assert_eq!(
        context.borrow().get("Tags").unwrap(),
        &DataValue::List(vec!["a".into()])
    );
    form.ui.cancel_combo_selection(tags).unwrap();
    context
        .borrow_mut()
        .set("Tags", DataValue::List(vec!["a".into(), "b".into()]))
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    let token = form.ui.combo_token(tags, "b").unwrap();
    let before = context.borrow().revision();
    form.ui
        .perform_action(token, SemanticAction::Activate, None)
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(
        context.borrow().get("Tags").unwrap(),
        &DataValue::List(vec!["a".into()])
    );
    assert_eq!(context.borrow().revision(), before + 1);
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(context.borrow().revision(), before + 1);
}

#[test]
fn lookup_compiled_search_policy_and_multiple_values_share_combo_engine() {
    use rust_desktop_ui_controls::{LookupSearchComparison, LookupSearchMode, SelectionMode};
    let markup = parse(
        r#"<LookupEdit x:Name="Lookup" SearchColumns="0" SearchMode="StartsWith" SearchComparison="Ordinal" SelectionMode="Multiple" SelectionPresentation="Tokens" SelectedValues="a,b"><ListBoxItem Value="a" Content="Alpha"/><ListBoxItem Value="b" Content="Beta"/></LookupEdit>"#,
    );
    let mut form = Form::new_with_model(markup, false, None).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Lookup").unwrap();
    assert_eq!(
        form.ui.combo_selection_settings(owner).unwrap().mode,
        SelectionMode::Multiple
    );
    let search = form.ui.lookup_search_settings(owner).unwrap();
    assert_eq!(search.columns, [0]);
    assert_eq!(search.mode, LookupSearchMode::StartsWith);
    assert_eq!(search.comparison, LookupSearchComparison::Ordinal);
    assert_eq!(form.ui.selected_keys(owner).unwrap(), ["a", "b"]);
    assert!(form.ui.combo_token(owner, "a").is_some());
}

#[test]
fn ribbon_retained_xaml_editors_keep_identity_draft_and_bindings_across_presentations() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Font", "Segoe UI".into()),
            ("Title", "Document".into()),
        ]))
        .unwrap(),
    ));
    let markup = parse(
        r#"<StackPanel><Ribbon CustomizeQuickAccess="True">
        <Ribbon.Backstage><Backstage Header="File" KeyTip="F">
            <BackstagePage Id="Info" Header="Info" KeyTip="I"><TextBox x:Name="Title" Text="{Binding Title}" CommitOnLostFocus="False"/></BackstagePage>
            <BackstageCommand Command="Confirm" KeyTip="C"/>
        </Backstage></Ribbon.Backstage>
        <RibbonTab Id="Home" Header="Home" KeyTip="H"><RibbonGroup Id="Text" Header="Text">
            <Button Command="Confirm" KeyTip="C"/>
            <RibbonContent Id="Font" Header="Font" KeyTip="FF" Width="180" MinWidth="120" Height="84">
                <TextBox x:Name="Font" Text="{Binding Font}" CommitOnLostFocus="False"/>
            </RibbonContent>
        </RibbonGroup></RibbonTab>
        <RibbonTab Id="View" Header="View" KeyTip="V"><RibbonGroup Id="View" Header="View"><Button Command="Confirm" KeyTip="C"/></RibbonGroup></RibbonTab>
    </Ribbon><TextBlock x:Name="TitlePreview" Text="{Binding Text, ElementName=Title}"/><TextBox x:Name="Message"/></StackPanel>"#,
    );
    let mut form = Form::new_with_model(markup, false, Some(context.clone())).unwrap();
    form.update(Size::new(960.0, 640.0), 1.0).unwrap();
    let font = form.find_name("Font").unwrap();
    let title = form.find_name("Title").unwrap();
    let slot_host = form
        .view
        .ribbon
        .as_ref()
        .unwrap()
        .ribbon
        .content_host("Font")
        .unwrap();
    let page_host = form
        .view
        .ribbon
        .as_ref()
        .unwrap()
        .ribbon
        .backstage_page_host("Info")
        .unwrap();
    form.ui
        .perform_action(font, rust_desktop_ui_controls::SemanticAction::Focus, None)
        .unwrap();
    form.ui.set_text(font, "Font draft").unwrap();
    for width in [280.0, 960.0, 420.0, 960.0] {
        form.update(Size::new(width, 640.0), 1.0).unwrap();
        assert_eq!(form.find_name("Font"), Some(font));
        assert_eq!(form.ui.document(font).unwrap().text(), "Font draft");
        assert_eq!(form.ui.accepted_value(font), Some("Segoe UI"));
        assert_eq!(
            form.view
                .ribbon
                .as_ref()
                .unwrap()
                .ribbon
                .content_host("Font"),
            Some(slot_host)
        );
    }
    for tab in ["View", "Home"] {
        assert!(
            form.view
                .ribbon
                .as_mut()
                .unwrap()
                .ribbon
                .set_active_tab(tab)
        );
        form.update(Size::new(960.0, 640.0), 1.0).unwrap();
        assert_eq!(form.ui.document(font).unwrap().text(), "Font draft");
    }
    for minimized in [true, false] {
        form.view
            .ribbon
            .as_mut()
            .unwrap()
            .ribbon
            .set_minimized(minimized);
        form.update(Size::new(960.0, 640.0), 1.0).unwrap();
        assert_eq!(form.ui.document(font).unwrap().text(), "Font draft");
    }
    form.view
        .ribbon
        .as_mut()
        .unwrap()
        .ribbon
        .open_backstage(&mut form.ui)
        .unwrap();
    form.update(Size::new(960.0, 640.0), 1.0).unwrap();
    form.ui.set_text(title, "Title draft").unwrap();
    form.view
        .ribbon
        .as_mut()
        .unwrap()
        .ribbon
        .close_backstage(&mut form.ui)
        .unwrap();
    form.update(Size::new(960.0, 640.0), 1.0).unwrap();
    form.view
        .ribbon
        .as_mut()
        .unwrap()
        .ribbon
        .open_backstage(&mut form.ui)
        .unwrap();
    form.update(Size::new(960.0, 640.0), 1.0).unwrap();
    assert_eq!(
        form.view
            .ribbon
            .as_ref()
            .unwrap()
            .ribbon
            .backstage_page_host("Info"),
        Some(page_host)
    );
    assert_eq!(form.find_name("Title"), Some(title));
    assert_eq!(form.ui.document(title).unwrap().text(), "Title draft");
    assert_eq!(form.ui.accepted_value(title), Some("Document"));
    form.ui.commit_edit(title).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Title").unwrap(),
        &DataValue::from("Title draft")
    );
    context
        .borrow_mut()
        .set("Font", "Model font".into())
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.document(font).unwrap().text(), "Model font");
}

#[test]
fn ribbon_retained_content_mount_preflights_commands_names_and_bindings_atomically() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("Confirm", "Confirm");
    ui.update(Size::new(960.0, 640.0)).unwrap();
    for body in [
        "<Button Content='Missing' Command='Missing'/>",
        "<TextBlock Text='{Binding Text, ElementName=Absent}'/>",
        "<MaskEdit Mask='00' Value='AB'/>",
    ] {
        let definition = parse(&format!(
            "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='Content' Header='Content'><RibbonContent Id='Content' Header='Content' KeyTip='C'>{body}</RibbonContent></RibbonGroup></RibbonTab></Ribbon>"
        ));
        let before = ui.runtime.tree().live_ids();
        let root = ui.root();
        assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), before);
    }
}

#[test]
fn mask_xaml_binds_raw_storage_without_literals_and_rejects_invalid_mount_atomically() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Reference", "AB123".into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(parse(r#"<MaskEdit x:Name="Reference" Mask="LL-0009" Value="{Binding Reference}" AllowEmpty="False" PromptChar="_"/>"#), false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let editor = form.find_name("Reference").unwrap();
    assert_eq!(form.ui.document(editor).unwrap().text(), "AB-123_");
    assert_eq!(form.ui.masked_value(editor).as_deref(), Some("AB123"));
    form.ui.set_masked_value(editor, "ЖЯ4567").unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Reference").unwrap(),
        &DataValue::from("ЖЯ4567")
    );
    assert_eq!(form.ui.document(editor).unwrap().text(), "ЖЯ-4567");
    context
        .borrow_mut()
        .set("Reference", "12345".into())
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.document(editor).unwrap().text(), "ЖЯ-4567");
    let invalid = parse(r#"<StackPanel><TextBlock Text="Must not mount"/><MaskEdit Mask="00" Value="AB"/></StackPanel>"#).content;
    let before = form.ui.runtime.tree().live_ids();
    let parent = form.ui.root();
    assert!(MountedView::mount(&mut form.ui, parent, &invalid).is_err());
    assert_eq!(form.ui.runtime.tree().live_ids(), before);
    assert_eq!(form.ui.masked_value(editor).as_deref(), Some("ЖЯ4567"));
}

#[test]
fn numeric_xaml_binding_and_calculator_keep_exact_decimal_storage() {
    use rust_desktop_ui_controls::{CalculatorKey as K, NumericOperation};
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Amount",
            "9007199254740993.25".into(),
        )]))
        .unwrap(),
    ));
    let mut form=Form::new_with_model(parse(r#"<NumericEdit x:Name="Exact" Value="{Binding Amount}" Minimum="0" Maximum="99999999999999999" Increment="0.01" Culture="en-US" Precision="2"/>"#),false,Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Exact").unwrap();
    let editor = form.ui.composite_editor(owner).unwrap();
    assert_eq!(
        form.ui.document(editor).unwrap().text(),
        "9007199254740993.25"
    );
    form.ui.set_text(editor, "9007199254740993.26").unwrap();
    form.ui.commit_edit(editor).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Amount").unwrap(),
        &DataValue::from("9007199254740993.26")
    );
    context.borrow_mut().set("Amount", DataValue::Null).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.numeric_value(owner), Some(None));
    context.borrow_mut().set("Amount", "0.1".into()).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.document(editor).unwrap().text(), "0.10");
    context
        .borrow_mut()
        .set("Amount", DataValue::Number(1.0))
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.document(editor).unwrap().text(), "0.10");
    context.borrow_mut().set("Amount", "0.1".into()).unwrap();
    form.view.sync_bindings(&mut form.ui);
    form.ui.set_text(editor, "1.25").unwrap();
    form.ui.open_numeric_calculator(owner).unwrap();
    form.ui.press_numeric_calculator(owner, K::Clear).unwrap();
    form.ui
        .press_numeric_calculator(owner, K::Digit(9))
        .unwrap();
    form.ui.cancel_numeric_calculator(owner).unwrap();
    assert_eq!(form.ui.document(editor).unwrap().text(), "1.25");
    assert_eq!(form.ui.accepted_value(editor), Some("0.10"));
    form.ui.open_numeric_calculator(owner).unwrap();
    for key in [
        K::Clear,
        K::Digit(0),
        K::DecimalSeparator,
        K::Digit(1),
        K::Operation(NumericOperation::Add),
        K::Digit(0),
        K::DecimalSeparator,
        K::Digit(2),
    ] {
        form.ui.press_numeric_calculator(owner, key).unwrap();
    }
    form.ui.apply_numeric_calculator(owner).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        form.ui.numeric_value(owner).unwrap().unwrap().to_string(),
        "0.3"
    );
    assert_eq!(
        context.borrow().get("Amount").unwrap(),
        &DataValue::from("0.3")
    );
    assert_eq!(form.ui.document(editor).unwrap().text(), "0.30");
    assert!(!form.ui.numeric_calculator_is_open(owner));
}

#[test]
fn typed_editor_fallbacks_use_storage_types_and_mask_fallback_remains_raw() {
    let definition = parse(
        r#"<StackPanel>
        <NumericEdit x:Name="Amount" Value="{Binding MissingAmount, Mode=OneWay, FallbackValue='0.1'}"/>
        <NumericEdit x:Name="Empty" Value="{Binding MissingEmpty, Mode=OneWay, FallbackValue='Null'}"/>
        <DateTimeEdit x:Name="Meeting" Value="{Binding MissingMeeting, Mode=OneWay, FallbackValue='2024-02-29T12:30:59'}"/>
        <DateRangeEdit x:Name="Range" Value="{Binding MissingRange, Mode=OneWay, FallbackValue='2024-02-29/2024-03-02'}"/>
        <Calendar x:Name="Calendar" SelectionMode="Range" Value="{Binding MissingCalendar, Mode=OneWay, FallbackValue='2024-02-29/2024-03-02'}"/>
        <MaskEdit x:Name="Code" Mask="LL-000" Value="{Binding MissingCode, Mode=OneWay, FallbackValue='AB123'}"/>
    </StackPanel>"#,
    );
    let mut form = Form::new(definition, false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(
        form.ui
            .numeric_value(form.find_name("Amount").unwrap())
            .unwrap()
            .unwrap()
            .to_string(),
        "0.1"
    );
    assert_eq!(
        form.ui.numeric_value(form.find_name("Empty").unwrap()),
        Some(None)
    );
    assert_eq!(
        form.ui
            .date_time_value(form.find_name("Meeting").unwrap())
            .unwrap()
            .unwrap()
            .to_iso(),
        "2024-02-29T12:30:59"
    );
    assert_eq!(
        form.ui
            .date_range_value(form.find_name("Range").unwrap())
            .unwrap()
            .unwrap()
            .to_iso(),
        "2024-02-29/2024-03-02"
    );
    assert!(matches!(
        form.ui
            .calendar_selection(form.find_name("Calendar").unwrap()),
        Some(rust_desktop_ui_controls::CalendarSelection::Range(Some(_)))
    ));
    assert_eq!(
        form.ui
            .masked_value(form.find_name("Code").unwrap())
            .as_deref(),
        Some("AB123")
    );
}

#[test]
fn password_xaml_mounts_empty_and_ordinary_model_or_diagnostics_cannot_read_secret() {
    let mut form = Form::new(parse(r#"<PasswordBox x:Name="Protected" AutomationName="Account password" AllowReveal="False" MaxLength="64"/>"#), false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Protected").unwrap();
    assert!(form.ui.password_value(owner).unwrap().is_empty());
    assert!(!form.view.editors.contains(&owner));
    assert_eq!(form.view.roles[&owner], Role::PasswordBox);
    form.ui
        .set_password_value(owner, "PRIVATE_PASSWORD_938")
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert!(form.ui.document(owner).is_none());
    assert!(form.ui.accepted_value(owner).is_none());
    let semantics = form.ui.semantics();
    let node = semantics.iter().find(|node| node.id == owner).unwrap();
    assert_eq!(node.role, Role::PasswordBox);
    assert_eq!(node.label, "Account password");
    assert!(node.value.is_none());
    assert!(!format!("{semantics:?}").contains("PRIVATE_PASSWORD_938"));
    assert!(!format!("{:?}", form.ui.password_value(owner)).contains("PRIVATE_PASSWORD_938"));
    assert!(form.ui.drain_text_changes().is_empty());
    assert!(form.ui.drain_edit_events().is_empty());
    let mut invalid = parse("<PasswordBox/>");
    invalid.content.common.bindings = parse("<TextBox Text='{Binding Secret}'/>")
        .content
        .common
        .bindings;
    let before = form.ui.runtime.tree().live_ids();
    let root = form.ui.root();
    assert!(MountedView::mount(&mut form.ui, root, &invalid.content).is_err());
    assert_eq!(form.ui.runtime.tree().live_ids(), before);
}

#[test]
fn embedded_editor_templates_keep_session_scopes_and_bindings_after_replacement() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Reference", "accepted".into()),
            ("Caption", "Decoration".into()),
        ]))
        .unwrap(),
    ));
    let definition = parse(
        r#"<Window.Resources><DataTemplate x:Key="ButtonFace"><StackPanel Orientation="Horizontal"><TextBlock x:Name="PrivateCaption" Text="{Binding Caption}"/></StackPanel></DataTemplate></Window.Resources><ButtonEdit x:Name="Reference" Text="{Binding Reference}"><ButtonEdit.Buttons><EditorButton Content="Choose document" Command="ChooseReference" Placement="Leading" Width="120" IsEnabledWhenReadOnly="True" ContentTemplate="{StaticResource ButtonFace}"/><EditorButton Content="Second action" Command="ChooseReference" ContentTemplate="{StaticResource ButtonFace}"/></ButtonEdit.Buttons></ButtonEdit>"#,
    );
    let mut form = Form::new_with_model(definition, false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Reference").unwrap();
    let parts = form.ui.editor_control_parts(owner).unwrap().clone();
    let button = parts.buttons[0];
    assert_eq!(
        form.ui.runtime.tree().children(owner).unwrap(),
        &[button, parts.editor, parts.buttons[1]]
    );
    assert_eq!(form.ui.document(parts.editor).unwrap().text(), "accepted");
    assert!(form.find_name("PrivateCaption").is_none());
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(parts.editor)))
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    form.ui.set_text(parts.editor, "draft").unwrap();
    let replacement = parse(r#"<StackPanel Orientation="Horizontal"><Icon Symbol="Check" Width="16" Height="16"/><TextBlock Text="{Binding Caption}"/></StackPanel>"#).content;
    for _ in 0..2 {
        let old = form.ui.button_content(button).unwrap();
        form.view
            .replace_editor_button_template(&mut form.ui, button, &replacement)
            .unwrap();
        form.update(SIZE, 1.0).unwrap();
        assert_eq!(form.ui.runtime.focused(), Some(parts.editor));
        assert_eq!(form.ui.document(parts.editor).unwrap().text(), "draft");
        assert_eq!(form.ui.accepted_value(parts.editor), Some("accepted"));
        assert!(!form.ui.runtime.tree().contains(old));
        let content = form.ui.button_content(button).unwrap();
        let bounds = form.ui.runtime.bounds(content).unwrap();
        let button_bounds = form.ui.runtime.bounds(button).unwrap();
        assert!(
            ((bounds.y + bounds.height * 0.5) - (button_bounds.y + button_bounds.height * 0.5))
                .abs()
                < 0.5
        );
    }
    context
        .borrow_mut()
        .set("Caption", "Updated decoration".into())
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert!(
        form.ui
            .semantics()
            .iter()
            .any(|node| node.label == "Updated decoration")
    );
    let previous = form.ui.button_content(button);
    let invalid = parse(r#"<TextBlock Text="{Binding Path=Text, ElementName=Missing}"/>"#).content;
    assert!(
        form.view
            .replace_editor_button_template(&mut form.ui, button, &invalid)
            .is_err()
    );
    assert_eq!(form.ui.button_content(button), previous);
    assert_eq!(form.ui.document(parts.editor).unwrap().text(), "draft");
    form.ui.commit_edit(parts.editor).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Reference").unwrap(),
        &DataValue::from("draft")
    );
    assert_eq!(
        form.ui
            .semantics()
            .into_iter()
            .find(|node| node.id == button)
            .unwrap()
            .label,
        "Choose document"
    );
}

#[test]
fn embedded_editor_icon_defaults_center_on_both_axes_and_explicit_alignment_survives() {
    let definition = parse(
        r#"<Window.Resources><DataTemplate x:Key="Face"><Icon Symbol="Check" Width="20" Height="20"/></DataTemplate></Window.Resources><ButtonEdit x:Name="Reference" Text="accepted" Height="48"><ButtonEdit.Buttons><EditorButton Content="Choose document" Command="ChooseReference" Placement="Leading" Width="60" ContentTemplate="{StaticResource Face}"/></ButtonEdit.Buttons></ButtonEdit>"#,
    );
    let mut form = Form::new(definition, false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let parts = form
        .ui
        .editor_control_parts(form.find_name("Reference").unwrap())
        .unwrap()
        .clone();
    let button = parts.buttons[0];
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(parts.editor)))
        .unwrap();
    form.ui.set_text(parts.editor, "draft").unwrap();
    let assert_center = |form: &Form| {
        let content = form
            .ui
            .runtime
            .bounds(form.ui.button_content(button).unwrap())
            .unwrap();
        let owner = form.ui.runtime.bounds(button).unwrap();
        assert!(
            (content.x + content.width * 0.5 - owner.x - owner.width * 0.5).abs() < 0.5,
            "horizontal {content:?} in {owner:?}"
        );
        assert!(
            (content.y + content.height * 0.5 - owner.y - owner.height * 0.5).abs() < 0.5,
            "vertical {content:?} in {owner:?}"
        );
        assert_eq!(form.ui.runtime.focused(), Some(parts.editor));
        assert_eq!(form.ui.document(parts.editor).unwrap().text(), "draft");
        assert_eq!(form.ui.accepted_value(parts.editor), Some("accepted"));
    };
    assert_center(&form);
    for xml in [
        r#"<Icon Symbol="Check" Width="20" Height="20"/>"#,
        r#"<TextBlock Text="ID"/>"#,
        r#"<Icon Symbol="Check" Width="20" Height="20"/>"#,
    ] {
        form.view
            .replace_editor_button_template(&mut form.ui, button, &parse(xml).content)
            .unwrap();
        form.update(SIZE, 1.0).unwrap();
        assert_center(&form);
    }
    let aligned = parse(r#"<Icon Symbol="Check" Width="20" Height="20" HorizontalAlignment="Right" VerticalAlignment="Bottom"/>"#).content;
    form.view
        .replace_editor_button_template(&mut form.ui, button, &aligned)
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    let content = form
        .ui
        .runtime
        .bounds(form.ui.button_content(button).unwrap())
        .unwrap();
    let owner = form.ui.runtime.bounds(button).unwrap();
    assert!((content.x + content.width - owner.x - owner.width).abs() < 0.5);
    assert!((content.y + content.height - owner.y - owner.height).abs() < 0.5);
}

#[test]
fn memo_xaml_multiline_binding_uses_the_retained_editor_and_validation_baseline() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Notes", "First\nSecond".into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(parse(r#"<MemoEdit x:Name="Notes" Text="{Binding Notes}" IsRequired="True" MaxLength="100" PopupWidth="440" PopupHeight="240"/>"#), false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Notes").unwrap();
    let memo = form.ui.memo_parts(owner).unwrap();
    let editor = memo.editor;
    assert_eq!(form.ui.composite_editor(owner), Some(editor));
    assert_eq!(form.ui.document(editor).unwrap().text(), "First\nSecond");
    form.ui.open_memo(memo).unwrap();
    form.ui.set_text(editor, "Updated\nMemo").unwrap();
    form.ui.commit_memo(memo).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Notes").unwrap(),
        &DataValue::from("Updated\nMemo")
    );
    form.ui.open_memo(memo).unwrap();
    form.ui.set_text(editor, "").unwrap();
    form.ui.commit_memo(memo).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert!(form.ui.memo_is_open(memo));
    assert_eq!(
        context.borrow().get("Notes").unwrap(),
        &DataValue::from("Updated\nMemo")
    );
    form.ui.cancel_memo(memo).unwrap();
    assert_eq!(form.ui.memo_editor(owner), Some(editor));
    assert_eq!(form.ui.document(editor).unwrap().text(), "Updated\nMemo");
    form.ui.set_composite_read_only(owner, true).unwrap();
    assert_eq!(form.ui.composite_read_only(owner), Some(true));
    form.ui.open_memo(memo).unwrap();
    assert!(form.ui.memo_is_open(memo));
}
fn parse(body: &str) -> rust_desktop_ui_xaml::WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Padding="0">{body}</Window>"#)).unwrap()
}

#[test]
fn civil_date_time_range_and_calendar_bindings_keep_typed_storage_and_seconds() {
    use rust_desktop_ui_controls::{CalendarSelection, DateRangeValue, DateValue};
    let initial_range =
        DataValue::object([("start", "2024-02-29".into()), ("end", "2024-03-02".into())]);
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Meeting", "2024-02-29T23:59:58".into()),
            ("Clock", "12:30:59".into()),
            ("Range", initial_range.clone()),
            ("CalendarRange", initial_range),
        ]))
        .unwrap(),
    ));
    let definition = parse(
        r#"<StackPanel><DateTimeEdit x:Name="Meeting" Value="{Binding Meeting}" ShowSeconds="True" TimeStep="1" Minimum="2024-01-01T00:00:00" Maximum="2024-12-31T23:59:59"/><TimeEdit x:Name="Clock" Value="{Binding Clock}" ShowSeconds="False" TimeStep="1"/><DateRangeEdit x:Name="Range" Value="{Binding Range}" Minimum="2024-01-01" Maximum="2024-12-31"/><Calendar x:Name="Calendar" Value="{Binding CalendarRange}" SelectionMode="Range" DisplayDate="2024-02-01" Minimum="2024-01-01" Maximum="2024-12-31"/></StackPanel>"#,
    );
    let mut form = Form::new_with_model(definition, false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let meeting = form.find_name("Meeting").unwrap();
    let editor = form.ui.composite_editor(meeting).unwrap();
    assert_eq!(
        form.ui.document(editor).unwrap().text(),
        "29.02.2024 23:59:58"
    );
    form.ui.set_text(editor, "01.03.2024 00:00:01").unwrap();
    form.ui.commit_edit(editor).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Meeting").unwrap(),
        &DataValue::from("2024-03-01T00:00:01")
    );
    let clock = form.find_name("Clock").unwrap();
    let clock_editor = form.ui.composite_editor(clock).unwrap();
    assert_eq!(form.ui.document(clock_editor).unwrap().text(), "12:30");
    form.ui.commit_edit(clock_editor).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Clock").unwrap(),
        &DataValue::from("12:30:59")
    );
    assert_eq!(form.ui.time_value(clock).unwrap().unwrap().second, 59);
    let range = form.find_name("Range").unwrap();
    let range_editor = form.ui.composite_editor(range).unwrap();
    form.ui
        .set_text(range_editor, "03.03.2024 – 01.03.2024")
        .unwrap();
    assert!(matches!(
        form.ui.commit_edit(range_editor).unwrap(),
        rust_desktop_ui_controls::CommitOutcome::Invalid(_)
    ));
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Range").unwrap(),
        &DataValue::object([("start", "2024-02-29".into()), ("end", "2024-03-02".into())])
    );
    form.ui
        .set_text(range_editor, "01.03.2024 – 03.03.2024")
        .unwrap();
    form.ui.commit_edit(range_editor).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("Range").unwrap(),
        &DataValue::object([("start", "2024-03-01".into()), ("end", "2024-03-03".into())])
    );
    let calendar = form.find_name("Calendar").unwrap();
    let value = DateRangeValue::new(
        DateValue::new(2024, 3, 5).unwrap(),
        DateValue::new(2024, 3, 8).unwrap(),
    )
    .unwrap();
    form.ui
        .set_calendar_selection(calendar, CalendarSelection::Range(Some(value)))
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        context.borrow().get("CalendarRange").unwrap(),
        &DataValue::object([("start", "2024-03-05".into()), ("end", "2024-03-08".into())])
    );
    context
        .borrow_mut()
        .set("CalendarRange", DataValue::Null)
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        form.ui.calendar_selection(calendar),
        Some(CalendarSelection::Range(None))
    );
}
fn template(text: &str) -> ControlTemplateDefinition {
    let window = parse(&format!(
        r#"<Window.Resources><ControlTemplate x:Key="Action" TargetType="Button"><Border Padding="2"><ContentPresenter x:Name="PART_Content"/></Border></ControlTemplate></Window.Resources><Button Content="{text}" Template="{{StaticResource Action}}"/>"#
    ));
    *window.content.common.control_template.unwrap()
}

#[test]
fn auto_height_button_template_is_centered_on_mount_and_repeated_replacement() {
    fn window(icon_height: u32) -> rust_desktop_ui_xaml::WindowDefinition {
        parse(&format!(
            r#"<Window.Resources><ControlTemplate x:Key="Action" TargetType="Button"><StackPanel Orientation="Horizontal" Spacing="8"><Icon Symbol="Check" Width="20" Height="{icon_height}"/><ContentPresenter x:Name="PART_Content"/></StackPanel></ControlTemplate></Window.Resources><Button x:Name="Action" Content="Template caption" Width="260" Height="60" Template="{{StaticResource Action}}"/>"#
        ))
    }
    fn assert_centered(form: &Form, owner: WidgetId) {
        let content = form.ui.button_content(owner).unwrap();
        let owner_bounds = form.ui.runtime.bounds(owner).unwrap();
        let bounds = form.ui.runtime.bounds(content).unwrap();
        assert!(bounds.height < owner_bounds.height - 8.0, "{bounds:?}");
        assert!(
            ((bounds.y + bounds.height * 0.5) - (owner_bounds.y + owner_bounds.height * 0.5)).abs()
                < 0.5,
            "owner={owner_bounds:?}, content={bounds:?}"
        );
        for child in form.ui.runtime.tree().children(content).unwrap() {
            let child = form.ui.runtime.bounds(*child).unwrap();
            assert!(child.y > owner_bounds.y + 4.0, "{child:?}");
            assert!(
                child.y + child.height < owner_bounds.y + owner_bounds.height - 4.0,
                "{child:?}"
            );
        }
    }
    for dark in [false, true] {
        let mut form = Form::new(window(20), dark).unwrap();
        form.update(SIZE, 1.0).unwrap();
        let owner = form.find_name("Action").unwrap();
        assert_centered(&form, owner);
        form.ui.runtime.apply(Mutation::Focus(Some(owner))).unwrap();
        for icon_height in [28, 16, 20] {
            let old = form.ui.button_content(owner).unwrap();
            let template = window(icon_height).content.common.control_template.unwrap();
            form.view
                .replace_control_template(&mut form.ui, owner, &template)
                .unwrap();
            form.update(SIZE, 1.0).unwrap();
            assert_centered(&form, owner);
            assert_eq!(form.ui.runtime.focused(), Some(owner));
            assert_eq!(form.find_name("Action"), Some(owner));
            assert!(!form.ui.runtime.tree().contains(old));
        }
    }
}
#[test]
fn control_template_swap_preserves_behavior_owner_focus_and_binding_engine() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Caption", "Bound caption".into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(parse(r#"<StackPanel><Button x:Name="Action" Content="{Binding Caption}" ToolTip="Original tip"/><TextBox x:Name="Message" Text="Original"/></StackPanel>"#), false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Action").unwrap();
    let editor = form.find_name("Message").unwrap();
    form.ui.set_text(editor, "Uncommitted draft").unwrap();
    form.ui.runtime.apply(Mutation::Focus(Some(owner))).unwrap();
    form.view
        .replace_control_template(&mut form.ui, owner, &template("First decoration"))
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    let first = form.ui.button_content(owner).unwrap();
    for caption in ["Second decoration", "Third decoration"] {
        form.view
            .replace_control_template(&mut form.ui, owner, &template(caption))
            .unwrap();
        form.update(SIZE, 1.0).unwrap();
        assert_eq!(form.find_name("Action"), Some(owner));
        assert_eq!(form.ui.runtime.focused(), Some(owner));
        assert_eq!(
            form.ui.document(editor).unwrap().text(),
            "Uncommitted draft"
        );
        assert_eq!(form.view.roles[&owner], Role::Button);
    }
    assert!(!form.ui.runtime.tree().contains(first));
    assert!(form.view.bindings.is_some());
    assert!(Rc::ptr_eq(
        &form.view.bindings.as_ref().unwrap().context(),
        &context
    ));
    context
        .borrow_mut()
        .set("Caption", "Updated owner caption".into())
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    let semantic = form
        .ui
        .semantics()
        .into_iter()
        .find(|node| node.id == owner)
        .unwrap();
    assert_eq!(semantic.label, "Updated owner caption");
    assert_eq!(semantic.tooltip.as_deref(), Some("Original tip"));
    assert_eq!(semantic.role, Role::Button);
    assert!(semantic.focused);
}
#[test]
fn group_header_swap_preserves_child_editor_draft_focus_and_stable_name() {
    let mut form = Form::new(parse(r#"<GroupBox x:Name="Group" Header="Original accessible name"><GroupBox.HeaderTemplate><TextBlock Text="Old presentation"/></GroupBox.HeaderTemplate><TextBox x:Name="Message" Text="Original"/></GroupBox>"#), false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Group").unwrap();
    let editor = form.find_name("Message").unwrap();
    let header = form.ui.group_header_id(owner).unwrap();
    let previous = form.ui.button_content(header).unwrap();
    let header_bounds = form.ui.runtime.bounds(header).unwrap();
    let content_bounds = form.ui.runtime.bounds(previous).unwrap();
    assert!(
        ((header_bounds.y + header_bounds.height * 0.5)
            - (content_bounds.y + content_bounds.height * 0.5))
            .abs()
            < 0.5
    );
    form.ui.set_text(editor, "Draft remains").unwrap();
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(editor)))
        .unwrap();
    let replacement = parse(r#"<StackPanel Orientation="Horizontal"><Icon Symbol="Check"/><TextBlock Text="New presentation"/></StackPanel>"#).content;
    form.view
        .replace_group_header_template(&mut form.ui, owner, &replacement)
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.find_name("Message"), Some(editor));
    assert_eq!(form.ui.runtime.focused(), Some(editor));
    assert_eq!(form.ui.group_header_id(owner), Some(header));
    assert_eq!(
        form.ui.group_header(owner),
        Some("Original accessible name")
    );
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft remains");
    assert!(!form.ui.runtime.tree().contains(previous));
}
#[test]
fn invalid_template_parts_and_binding_configuration_roll_back_without_leaks() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.update(SIZE).unwrap();
    let root = ui.root();
    let mut view = MountedView::mount(
        &mut ui,
        root,
        &parse(r#"<Button x:Name="Action" Content="Original"/>"#).content,
    )
    .unwrap();
    let owner = view.find_name("Action").unwrap();
    view.replace_control_template(&mut ui, owner, &template("Valid"))
        .unwrap();
    let content = ui.button_content(owner);
    let ids = ui.runtime.tree().live_ids();
    let mut invalid = template("Invalid");
    invalid.content_part = "MissingRequiredPart".into();
    assert!(
        view.replace_control_template(&mut ui, owner, &invalid)
            .is_err()
    );
    let mut invalid = template("Binding failure");
    fn inject(element: &mut ElementDefinition) {
        if matches!(element.kind, ElementKind::TextBlock(_)) {
            element.common.bindings.insert(
                "Text".into(),
                rust_desktop_ui_xaml::BindingDefinition {
                    path: "Text".into(),
                    element_name: Some("MissingOwner".into()),
                    ..Default::default()
                },
            );
        }
        for child in element.children_mut() {
            inject(child);
        }
    }
    inject(&mut invalid.visual);
    assert!(
        view.replace_control_template(&mut ui, owner, &invalid)
            .unwrap_err()
            .contains("Unknown ElementName")
    );
    assert_eq!(ui.runtime.tree().live_ids(), ids);
    assert_eq!(ui.button_content(owner), content);
    assert_eq!(view.find_name("Action"), Some(owner));
}

#[test]
fn replacement_private_names_do_not_capture_existing_editors_or_another_instance() {
    let mut form = Form::new(parse(r#"<StackPanel><GroupBox x:Name="Group" Header="Header"><TextBox x:Name="Message" Text="Draft"/></GroupBox><Button x:Name="One" Content="One"/><Button x:Name="Two" Content="Two"/></StackPanel>"#), false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let editor = form.find_name("Message").unwrap();
    let group = form.find_name("Group").unwrap();
    let header = parse(r#"<TextBlock x:Name="Message" Text="Decoration"/>"#).content;
    form.view
        .replace_group_header_template(&mut form.ui, group, &header)
        .unwrap();
    assert_eq!(form.find_name("Message"), Some(editor));
    let template = template("Reusable");
    for name in ["One", "Two", "One"] {
        let owner = form.find_name(name).unwrap();
        form.view
            .replace_control_template(&mut form.ui, owner, &template)
            .unwrap();
        assert_eq!(form.find_name("Message"), Some(editor));
    }
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft");
}

#[test]
fn decorative_writes_are_rejected_before_shared_model_or_owner_changes() {
    let mut form = Form::new(
        parse(r#"<Button x:Name="Action" Content="Original"/>"#),
        false,
    )
    .unwrap();
    let owner = form.find_name("Action").unwrap();
    let before = form.ui.runtime.tree().live_ids();
    let mut definition = template("Forbidden write");
    let part = definition.visual.children_mut().into_iter().next().unwrap();
    part.common.bindings.insert(
        "Text".into(),
        rust_desktop_ui_xaml::BindingDefinition {
            path: "Caption".into(),
            mode: rust_desktop_ui_xaml::BindingMode::OneWayToSource,
            ..Default::default()
        },
    );
    assert!(
        form.view
            .replace_control_template(&mut form.ui, owner, &definition)
            .unwrap_err()
            .contains("OneWay or OneTime")
    );
    assert_eq!(form.ui.runtime.tree().live_ids(), before);
}
