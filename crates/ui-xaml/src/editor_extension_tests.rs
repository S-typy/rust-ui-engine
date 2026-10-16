use crate::*;

#[test]
fn mask_edit_compiles_raw_values_culture_and_bounded_pattern_syntax() {
    let xml = source(
        "<MaskEdit Mask='LL-0009' Value='{Binding Reference}' Culture='en-US' PromptChar='?' AllowEmpty='False' MaxLength='20'/>",
    );
    let window = parse(&xml).unwrap();
    let ElementKind::MaskEdit(mask) = &window.content.kind else {
        panic!()
    };
    assert_eq!(mask.mask, "LL-0009");
    assert_eq!(mask.culture, NumericCultureDefinition::English);
    assert_eq!(mask.prompt_char, '?');
    assert_eq!(
        window.content.common.bindings["Value"].mode,
        BindingMode::TwoWay
    );
    assert_eq!(
        window.content.common.bindings["Value"].update_source_trigger,
        UpdateSourceTrigger::LostFocus
    );
    let generated = compile(&xml).unwrap();
    assert!(generated.contains("MaskEditDefinition"));
    assert!(generated.contains("prompt_char: '?'"));
    for body in [
        "<MaskEdit/>",
        "<MaskEdit Mask='---'/>",
        "<MaskEdit Mask='00\\'/>",
        "<MaskEdit Mask='00' Text='12'/>",
        "<MaskEdit Mask='00' PromptChar=''/>",
        "<MaskEdit Mask='00' PromptChar='ab'/>",
        "<MaskEdit Mask='00' PromptChar='&#x301;'/>",
        "<MaskEdit Mask='00' PromptChar='&#x1F1E6;'/>",
        "<MaskEdit Mask='00&#x2028;'/>",
        "<MaskEdit Mask='00' Value='123'/>",
        "<MaskEdit Mask='00' AllowEmpty='False'/>",
        "<MaskEdit Mask='{Binding Mask}'/>",
        "<MaskEdit Mask='00'><TextBlock/></MaskEdit>",
    ] {
        assert!(parse(&source(body)).is_err(), "{body}");
    }
    assert!(parse(&source(&format!("<MaskEdit Mask='{}'/>", "0".repeat(257)))).is_err());
    assert!(parse(&source("<MaskEdit Mask='00\\L' Value='12'/>")).is_ok());
}

#[test]
fn compiled_alignment_preserves_explicit_stretch_separately_from_presentation_defaults() {
    let parsed = parse(&source("<StackPanel><Icon Symbol='Check'/><Icon Symbol='Check' HorizontalAlignment='Stretch' VerticalAlignment='Stretch'/></StackPanel>")).unwrap();
    let children = parsed.content.children();
    assert!(!children[0].common.horizontal_alignment_explicit);
    assert!(!children[0].common.vertical_alignment_explicit);
    assert!(children[1].common.horizontal_alignment_explicit);
    assert!(children[1].common.vertical_alignment_explicit);
    assert_eq!(children[1].common.horizontal_alignment, Alignment::Stretch);
    assert_eq!(children[1].common.vertical_alignment, Alignment::Stretch);
    let generated = compile(&source(
        "<Icon Symbol='Check' HorizontalAlignment='Stretch' VerticalAlignment='Stretch'/>",
    ))
    .unwrap();
    assert!(generated.contains("horizontal_alignment_explicit: true"));
    assert!(generated.contains("vertical_alignment_explicit: true"));
}

#[test]
fn temporal_compositions_compile_civil_storage_and_boundaries_without_losing_seconds() {
    let source_text = source(
        "<StackPanel><DateTimeEdit Value='2024-02-29T23:59:58' Minimum='2024-02-29T00:00:01' Maximum='2024-03-01T00:00:00' TimeStep='1'/><DateRangeEdit Value='2024-02-29/2024-03-02' Minimum='2024-02-01' Maximum='2024-03-31'/><Calendar Value='2024-02-29/2024-03-02' SelectionMode='Range' View='Months' DisplayDate='2024-02-01'/><TimeEdit Value='12:30:59' ShowSeconds='False' TimeStep='1' AllowEmpty='False'/></StackPanel>",
    );
    let definition = parse(&source_text).unwrap();
    let children = definition.content.children();
    let ElementKind::DateTimeEdit(value) = &children[0].kind else {
        panic!()
    };
    assert!(!value.show_seconds);
    assert_eq!(value.time_step, 1);
    let generated = compile(&source_text).unwrap();
    for name in [
        "DateTimeEditDefinition",
        "DateRangeEditDefinition",
        "CalendarDefinition",
        "time_step: 1",
        "23:59:58",
    ] {
        assert!(generated.contains(name), "{name}");
    }
    let bound = parse(&source(
        "<Calendar Value='{Binding Selection}' SelectionMode='Range'/>",
    ))
    .unwrap();
    assert_eq!(
        bound.content.common.bindings["Value"].mode,
        BindingMode::TwoWay
    );
    assert_eq!(
        bound.content.common.bindings["Value"].update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    for content in [
        "<DateTimeEdit Value='2023-02-29T12:00:00'/>",
        "<DateTimeEdit Value='2024-02-29T12:00'/>",
        "<DateTimeEdit Value='2024-02-29T12:00:00Z'/>",
        "<DateTimeEdit Value='2024-02-29T12:00:00.1'/>",
        "<DateTimeEdit Value='2024-02-29T24:00:00'/>",
        "<DateTimeEdit AllowEmpty='False'/>",
        "<DateTimeEdit TimeStep='0'/>",
        "<DateTimeEdit TimeStep='86401'/>",
        "<DateTimeEdit Value='2024-02-29T23:00:00' Maximum='2024-02-29T22:59:59'/>",
        "<DateRangeEdit Value='2024-03-01/2024-02-29'/>",
        "<DateRangeEdit Value='2024-03-01/2024-03-02' Maximum='2024-03-01'/>",
        "<DateRangeEdit Value='2024-03-01'/>",
        "<DateRangeEdit AllowEmpty='False'/>",
        "<Calendar SelectionMode='Multiple'/>",
        "<Calendar View='Hours'/>",
        "<Calendar DisplayDate='2023-02-29'/>",
        "<Calendar SelectionMode='Range' Value='2024-03-01'/>",
        "<Calendar Value='2024-03-01/2024-03-02'/>",
        "<TimeEdit Format='HH:mm:ss' ShowSeconds='False'/>",
        "<TimeEdit ShowSeconds='True' Format='HH:mm'/>",
        "<DateEdit TimeStep='2'/>",
        "<DateEdit ShowSeconds='True'/>",
        "<DateEdit AllowEmpty='False'/>",
    ] {
        assert!(parse(&source(content)).is_err(), "{content}");
    }
    assert!(
        parse(&source(
            "<DateTimeEdit Value='{Binding Timestamp}' AllowEmpty='False'/>"
        ))
        .is_ok()
    );
}

#[test]
fn numeric_edit_preserves_exact_storage_and_rejects_lossy_literals() {
    let definition = parse(&source("<NumericEdit Value='9007199254740993.25' Minimum='0' Maximum='99999999999999999' Increment='0.01' Precision='2' Culture='en-US' UseGrouping='True' ShowCalculator='False' AllowEmpty='False'/>" )).unwrap();
    let ElementKind::NumericEdit(numeric) = &definition.content.kind else {
        panic!()
    };
    assert_eq!(numeric.value.as_deref(), Some("9007199254740993.25"));
    assert_eq!(numeric.culture, NumericCultureDefinition::English);
    assert!(numeric.use_grouping && !numeric.show_calculator && !numeric.allow_empty);
    let generated = compile(&source("<NumericEdit Value='0.1'/>")).unwrap();
    assert!(generated.contains("NumericEditDefinition"));
    assert!(generated.contains("value: Some(\"0.1\".into())"));
    let bound = parse(&source(
        "<NumericEdit Value='{Binding Amount}' AllowEmpty='False'/>",
    ))
    .unwrap();
    assert_eq!(
        bound.content.common.bindings["Value"].mode,
        BindingMode::TwoWay
    );
    assert_eq!(
        bound.content.common.bindings["Value"].update_source_trigger,
        UpdateSourceTrigger::LostFocus
    );
    for attrs in [
        "Value='NaN'",
        "Value='1e2'",
        "Value='1,2'",
        "Value='01'",
        "Value='+1'",
        "Value='-0'",
        "Value='1.00'",
        "Value='0.001'",
        "Minimum='2' Maximum='1'",
        "Increment='0'",
        "Increment='-1'",
        "Increment='0.001'",
        "Precision='29'",
        "AllowEmpty='False'",
        "Culture='fr-FR'",
        "Value='170141183460469231731687303715884105728'",
        "Text='1'",
    ] {
        assert!(
            parse(&source(&format!("<NumericEdit {attrs}/>"))).is_err(),
            "{attrs}"
        );
    }
    assert!(parse(&source("<NumericEdit><TextBox/></NumericEdit>")).is_err());
}

#[test]
fn protected_input_compiles_only_settings_and_never_a_value() {
    let window = parse(&source("<PasswordBox MaxLength='128' IsReadOnly='True' AllowPaste='False' AllowReveal='False' RevealDurationMs='1000' PlaceholderText='Password' FontSize='16' Padding='8' PlaceholderForeground='#808080' Width='{Binding PasswordWidth}'/>" )).unwrap();
    let ElementKind::PasswordBox(password) = &window.content.kind else {
        panic!()
    };
    assert_eq!(password.max_length, 128);
    assert!(password.is_read_only && !password.allow_paste && !password.allow_reveal);
    assert_eq!(password.reveal_duration_ms, 1000);
    assert_eq!(password.style.font_size, Some(16.0));
    let compiled = compile(&source("<PasswordBox/>")).unwrap();
    assert!(compiled.contains("PasswordBoxDefinition"));
    assert!(!compiled.contains("EditBoxDefinition"));
    for attr in [
        "Text='PRIVATE_VALUE'",
        "Password='PRIVATE_VALUE'",
        "Value='PRIVATE_VALUE'",
        "Text='{Binding Secret}'",
        "Password='{Binding Secret}'",
        "Value='{Binding Secret}'",
        "IsReadOnly='{Binding ReadOnly}'",
        "MaxLength='4097'",
        "MaxLength='-1'",
        "RevealDurationMs='0'",
        "RevealDurationMs='10001'",
        "AllowPaste='Sometimes'",
        "IsRequired='True'",
        "CommitOnLostFocus='True'",
    ] {
        let error = parse(&source(&format!("<PasswordBox {attr}/>"))).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_VALUE"));
    }
    for children in ["<TextBox/>", "raw private text"] {
        assert!(parse(&source(&format!("<PasswordBox>{children}</PasswordBox>"))).is_err());
    }
    let mut common = CommonProperties::default();
    common.bindings.insert(
        "Text".into(),
        window.content.common.bindings["Width"].clone(),
    );
    assert!(validate_password_bindings(&common).is_err());
    assert!(parse(&source("<PasswordBox MaxLength='0'/>")).is_ok());
}

fn source(body: &str) -> String {
    format!(r#"<Window xmlns="{UI_NAMESPACE}" xmlns:x="{XAML_NAMESPACE}">{body}</Window>"#)
}
fn buttons(body: &str) -> String {
    source(&format!(
        "<ButtonEdit><ButtonEdit.Buttons>{body}</ButtonEdit.Buttons></ButtonEdit>"
    ))
}

#[test]
fn memo_definitions_preserve_multiline_settings_and_committed_binding_default() {
    let definition = parse(&source("<MemoEdit Text='First&#10;Second' MaxLength='12' PopupWidth='640' PopupHeight='400' IsReadOnly='True' IsRequired='True' CommitOnLostFocus='False'/>" )).unwrap();
    let ElementKind::MemoEdit(memo) = &definition.content.kind else {
        panic!()
    };
    assert_eq!(memo.editor.text, "First\nSecond");
    assert_eq!((memo.popup_width, memo.popup_height), (640.0, 400.0));
    assert!(memo.editor.is_read_only && memo.editor.is_required);
    assert!(!memo.editor.commit_on_lost_focus);
    let definition = parse(&source("<MemoEdit Text='{Binding Notes}'/>")).unwrap();
    let binding = &definition.content.common.bindings["Text"];
    assert_eq!(binding.mode, BindingMode::TwoWay);
    assert_eq!(
        binding.update_source_trigger,
        UpdateSourceTrigger::LostFocus
    );
    let generated = compile(&source("<MemoEdit Text='First&#10;Second'/>")).unwrap();
    assert!(generated.contains("ElementKind::MemoEdit("));
    assert!(generated.contains("popup_width: 480.0"));
    assert!(generated.contains("First\\nSecond"));
    for bad in [
        "<MemoEdit PopupWidth='159'/>",
        "<MemoEdit PopupHeight='NaN'/>",
        "<MemoEdit PopupWidth='16385'/>",
        "<MemoEdit Text='A&#10;B' MaxLength='2'/>",
        "<MemoEdit><TextBox/></MemoEdit>",
    ] {
        assert!(parse(&source(bad)).is_err(), "{bad}");
    }
}

#[test]
fn embedded_button_settings_have_bounded_values_and_explicit_accessible_content() {
    let definition = parse(&buttons("<EditorButton Command='Browse' Content='Browse' Placement='Leading' Width='48' IsVisible='False' IsEnabled='False' IsEnabledWhenReadOnly='True' IsTabStop='False'/><EditorButton Command='Clear' Content='Clear'/>" )).unwrap();
    let ElementKind::ButtonEdit(editor) = &definition.content.kind else {
        panic!()
    };
    let first = &editor.buttons[0];
    assert_eq!(first.placement, EditorButtonPlacement::Leading);
    assert_eq!(first.width, 48.0);
    assert!(!first.is_visible && !first.is_enabled && !first.is_tab_stop);
    assert!(first.is_enabled_when_read_only);
    let second = &editor.buttons[1];
    assert_eq!(second.placement, EditorButtonPlacement::Trailing);
    assert_eq!(second.width, 30.0);
    assert!(second.is_visible && second.is_enabled && second.is_tab_stop);
    assert!(!second.is_enabled_when_read_only);
    for attr in [
        "Width='0'",
        "Width='1025'",
        "Width='NaN'",
        "Placement='Left'",
        "IsTabStop='Maybe'",
        "IsVisible='{Binding Visible}'",
    ] {
        assert!(
            parse(&buttons(&format!(
                "<EditorButton Command='Browse' Content='Browse' {attr}/>"
            )))
            .is_err(),
            "{attr}"
        );
    }
    assert!(
        parse(&buttons(
            "<EditorButton Command='Browse' ToolTip='Browse'/>"
        ))
        .is_err()
    );
}

#[test]
fn embedded_button_resource_templates_keep_private_names_and_declaration_scope() {
    let definition = parse(&source(r#"<Window.Resources><String x:Key="Caption" Value="Declaration"/><DataTemplate x:Key="Visual"><StackPanel x:Name="Local" Orientation="Horizontal"><TextBlock Text="{StaticResource Caption}"/><TextBlock Text="{Binding Width, ElementName=Local}"/></StackPanel></DataTemplate></Window.Resources><ButtonEdit><ButtonEdit.Resources><String x:Key="Caption" Value="Consumer"/></ButtonEdit.Resources><ButtonEdit.Buttons><EditorButton Command="One" Content="One" ContentTemplate="{StaticResource Visual}"/><EditorButton Command="Two" Content="Two" ContentTemplate="{StaticResource Visual}"/></ButtonEdit.Buttons></ButtonEdit>"#)).unwrap();
    let ElementKind::ButtonEdit(editor) = &definition.content.kind else {
        panic!()
    };
    let first = editor.buttons[0].content_template.as_ref().unwrap();
    let second = editor.buttons[1].content_template.as_ref().unwrap();
    assert_ne!(first.common.name, second.common.name);
    assert_eq!(first.common.vertical_alignment, Alignment::Center);
    let ElementKind::TextBlock(caption) = &first.children()[0].kind else {
        panic!()
    };
    assert_eq!(caption.text, "Declaration");
    assert_eq!(
        first.children()[1].common.bindings["Text"].element_name,
        first.common.name
    );
    assert_eq!(definition.content.children().len(), 2);
    let generated = crate::codegen::window(&definition);
    assert!(generated.contains("content_template: Some(Box::new("));
    assert!(generated.contains("EditorButtonPlacement::Trailing"));
}

#[test]
fn inline_button_visuals_are_isolated_and_interactive_or_writing_templates_rejected() {
    let definition = parse(&buttons("<EditorButton Command='One' Content='One'><EditorButton.ContentTemplate><TextBlock x:Name='Caption' Text='{Binding Label}'/></EditorButton.ContentTemplate></EditorButton><EditorButton Command='Two' Content='Two'><EditorButton.ContentTemplate><TextBlock x:Name='Caption' Text='Two'/></EditorButton.ContentTemplate></EditorButton>" )).unwrap();
    let children = definition.content.children();
    assert_ne!(children[0].common.name, children[1].common.name);
    for visual in [
        "<TextBox Text='not decorative'/>",
        "<TextBlock Text='{Binding Label, Mode=OneWayToSource}'/>",
        "<TextBlock/><TextBlock/>",
        "",
    ] {
        assert!(parse(&buttons(&format!("<EditorButton Command='One' Content='One'><EditorButton.ContentTemplate>{visual}</EditorButton.ContentTemplate></EditorButton>"))).is_err(), "{visual}");
    }
    for extra in [
        "<EditorButton.ContentTemplate><TextBlock/></EditorButton.ContentTemplate>",
        "",
    ] {
        let body = format!(
            "<Window.Resources><DataTemplate x:Key='Bad'><TextBox/></DataTemplate></Window.Resources><ButtonEdit><ButtonEdit.Buttons><EditorButton Command='One' Content='One' ContentTemplate='{{StaticResource Bad}}'>{extra}</EditorButton></ButtonEdit.Buttons></ButtonEdit>"
        );
        assert!(parse(&source(&body)).is_err());
    }
}
