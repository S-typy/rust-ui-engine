use super::*;
fn window(content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>")
}
#[test]
fn split_and_slider_fields_compile_and_invalid_geometry_is_rejected() {
    let source = window(
        "<SplitContainer Ratio='0.4' FirstMinSize='40' SecondMaxSize='500' CollapsedPane='Second'><Slider Minimum='10' Maximum='20' Value='12' Step='0.5' TickFrequency='2' SnapToTicks='True'/><RangeSlider LowerValue='20' UpperValue='70' CrossingPolicy='Swap'/></SplitContainer>",
    );
    let definition = parse(&source).unwrap();
    assert_eq!(definition.content.children().len(), 2);
    let generated = compile(&source).unwrap();
    assert!(generated.contains("SplitPaneDefinition::Second"));
    assert!(generated.contains("RangeCrossingPolicyDefinition::Swap"));
    for source in [
        "<SplitContainer><TextBlock/></SplitContainer>",
        "<SplitContainer Ratio='1.1'><TextBlock/><TextBlock/></SplitContainer>",
        "<SplitContainer FirstMinSize='100' FirstMaxSize='10'><TextBlock/><TextBlock/></SplitContainer>",
        "<Slider TickFrequency='0'/>",
        "<Slider Minimum='-1e308' Maximum='1e308'/>",
        "<RangeSlider LowerValue='80' UpperValue='20'/>",
    ] {
        assert!(parse(&window(source)).is_err(), "{source}");
    }
}
#[test]
fn nullable_checks_switch_and_radio_collection_use_explicit_contracts() {
    let source = window(
        "<StackPanel><CheckBox IsThreeState='True' IsChecked='Indeterminate'/><ToggleButton IsThreeState='True' IsChecked='Null'/><Switch IsChecked='True'/><RadioGroup SelectedKey='{Binding Choice}' ItemsSource='{Binding Choices}'/></StackPanel>",
    );
    let definition = parse(&source).unwrap();
    let children = definition.content.children();
    let ElementKind::CheckBox(check) = &children[0].kind else {
        panic!()
    };
    assert!(check.is_three_state && check.is_indeterminate);
    assert_eq!(
        children[3].common.bindings["SelectedKey"].mode,
        BindingMode::TwoWay
    );
    assert!(compile(&source).unwrap().contains("is_indeterminate: true"));
    for body in [
        "<CheckBox IsChecked='Null'/>",
        "<Switch IsThreeState='True'/>",
        "<ToggleButton IsThreeState='True' Command='Toggle'/>",
    ] {
        assert!(parse(&window(body)).is_err());
    }
}
#[test]
fn inline_text_and_trimming_are_typed_and_bounded() {
    let source = window(
        "<TextBlock TextTrimming='WordEllipsis' MaxLines='2'><Run Text='Title' FontWeight='700'/><Inline Text=' detail' FontSize='12' Foreground='#112233'/></TextBlock>",
    );
    let parsed = parse(&source).unwrap();
    let ElementKind::TextBlock(text) = parsed.content.kind else {
        panic!()
    };
    assert_eq!(text.inlines.len(), 2);
    assert_eq!(text.trimming, TextTrimmingDefinition::WordEllipsis);
    assert!(compile(&source).unwrap().contains("InlineDefinition"));
    for source in [
        "<TextBlock Text='A'><Run Text='B'/></TextBlock>",
        "<TextBlock MaxLines='257'/>",
        "<TextBlock><Run FontWeight='0'/></TextBlock>",
    ] {
        assert!(parse(&window(source)).is_err());
    }
}
#[test]
fn retained_headers_are_decorative_and_traversed_before_content() {
    let source = window(
        "<Expander Header='Details' ExpandDirection='Left' AnimationDurationMs='180'><Expander.HeaderTemplate><StackPanel><Icon Symbol='FolderOpen'/><TextBlock Text='Details'/></StackPanel></Expander.HeaderTemplate><TextBox x:Name='Value'/></Expander>",
    );
    let parsed = parse(&source).unwrap();
    assert_eq!(parsed.content.children().len(), 2);
    assert!(
        compile(&source)
            .unwrap()
            .contains("ExpandDirectionDefinition::Left")
    );
    for source in [
        "<Expander><Expander.HeaderTemplate><Button/></Expander.HeaderTemplate></Expander>",
        "<Expander AnimationDurationMs='1001'/>",
        "<GroupBox><Expander.HeaderTemplate><TextBlock/></Expander.HeaderTemplate></GroupBox>",
        "<GroupBox><GroupBox.HeaderTemplate/></GroupBox>",
    ] {
        assert!(parse(&window(source)).is_err(), "{source}");
    }
}
#[test]
fn bars_popup_tooltip_and_repeat_link_button_codegen_are_complete() {
    let source = window(
        "<StackPanel><Button x:Name='Anchor' Content='Open' ToolTip='Help' ToolTipHeader='Details' ToolTipIcon='Add' ToolTipImage='HelpImage' ToolTipImageDescription='Help illustration'/><Popup PlacementTarget='Anchor' PopupWidth='300' Placement='Beside'><TextBox/></Popup><ToolBar Key='Tools'><LinkButton Content='Help'/><RepeatButton Content='More' RepeatDelayMs='300' RepeatIntervalMs='50'/></ToolBar><MenuBar><DropDownButton Content='File'><MenuItem Header='Open' Command='Open'/></DropDownButton></MenuBar></StackPanel>",
    );
    let parsed = parse(&source).unwrap();
    let children = parsed.content.children();
    assert_eq!(children.len(), 4);
    assert_eq!(
        children[0].common.rich_tool_tip.as_ref().unwrap().header,
        "Details"
    );
    let generated = compile(&source).unwrap();
    for text in [
        "ToolBarDefinition",
        "MenuBarDefinition",
        "PopupDefinition",
        "repeat_delay_ms: Some(300)",
        "is_link: true",
    ] {
        assert!(generated.contains(text), "{text}");
    }
    for source in [
        "<ToolBar Key='Tools'/>",
        "<MenuBar/>",
        "<MenuBar><Button/></MenuBar>",
        "<Popup><TextBlock/></Popup>",
        "<Button ToolTipImage='http://bad' ToolTipImageDescription='x'/>",
        "<RepeatButton RepeatIntervalMs='1'/>",
    ] {
        assert!(parse(&window(source)).is_err(), "{source}");
    }
}
#[test]
fn multi_bindings_have_explicit_inputs_one_way_modes_and_round_trip() {
    let source = window(
        "<TextBlock Text=\"{MultiBinding Paths='Amount;Tax;#Other.Text', Expression='concat($0 + $1, $2)', FallbackValue='Unavailable'}\"/>",
    );
    let parsed = parse(&source).unwrap();
    let binding = &parsed.content.common.bindings["Text"];
    assert_eq!(binding.inputs.len(), 3);
    assert_eq!(binding.inputs[2].element_name.as_deref(), Some("Other"));
    assert!(compile(&source).unwrap().contains("BindingInputDefinition"));
    for binding in [
        "{MultiBinding Paths='A;B'}",
        "{MultiBinding Paths='A;B', Expression='$0', Mode=TwoWay}",
        "{MultiBinding Paths='A;;B', Converter=Join}",
        "{MultiBinding Paths='#Missing', Expression='$0'}",
    ] {
        assert!(parse(&window(&format!("<TextBlock Text=\"{binding}\"/>"))).is_err());
    }
}
#[test]
fn busy_and_range_bindings_use_correct_modes_and_triggers() {
    let source = window(
        "<StackPanel><BusyIndicator IsIndeterminate='{Binding Busy}' Value='{Binding Value}' ReducedMotion='True'/><RangeSlider LowerValue='{Binding Lower}' UpperValue='{Binding Upper}'/><Slider Value='{Binding Value}'/></StackPanel>",
    );
    let parsed = parse(&source).unwrap();
    let children = parsed.content.children();
    assert_eq!(
        children[0].common.bindings["Value"].mode,
        BindingMode::OneWay
    );
    assert_eq!(
        children[1].common.bindings["LowerValue"].mode,
        BindingMode::TwoWay
    );
    assert_eq!(
        children[2].common.bindings["Value"].update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    assert!(compile(&source).unwrap().contains("reduced_motion: true"));
}

#[test]
fn repeat_policy_matches_runtime_limits_and_defaults() {
    let form = |content: &str| parse(&window(content));
    let window = form("<RepeatButton Content='Repeat'/>").unwrap();
    assert_eq!(window.content.common.repeat_delay_ms, Some(400));
    assert_eq!(window.content.common.repeat_interval_ms, 80);
    assert!(
        form("<RepeatButton Content='Repeat' RepeatDelayMs='1' RepeatIntervalMs='60000'/>").is_ok()
    );
    assert!(form("<RepeatButton Content='Repeat' RepeatDelayMs='0'/>").is_err());
    assert!(form("<RepeatButton Content='Repeat' RepeatDelayMs='60001'/>").is_err());
    assert!(form("<RepeatButton Content='Repeat' RepeatIntervalMs='15'/>").is_err());
}

#[test]
fn retained_disclosures_resolve_names_and_reject_conflicting_or_cyclic_owners() {
    let markup = "<StackPanel><SplitButton x:Name='Apply' Content='Apply' Command='Apply'/><Popup PlacementTarget='Apply'><TextBox/></Popup><DropDownButton x:Name='Options' Content='Options'/><Popup PlacementTarget='Options'><TextBlock Text='Details'/></Popup></StackPanel>";
    assert!(compile(&window(markup)).is_ok());
    for markup in [
        "<SplitButton x:Name='Apply' Content='Apply' Command='Apply'/>",
        "<StackPanel><Button x:Name='Anchor'/><Popup PlacementTarget='Missing'><TextBox/></Popup></StackPanel>",
        "<StackPanel><Button x:Name='Anchor'/><Popup PlacementTarget='Anchor'><TextBox/></Popup><Popup PlacementTarget='Anchor'><TextBox/></Popup></StackPanel>",
        "<StackPanel><SplitButton x:Name='Apply' Command='Apply'><MenuItem Header='Run' Command='Run'/></SplitButton><Popup PlacementTarget='Apply'><TextBox/></Popup></StackPanel>",
        "<StackPanel><Popup PlacementTarget='Second'><Button x:Name='First'/></Popup><Popup PlacementTarget='First'><Button x:Name='Second'/></Popup></StackPanel>",
        "<StackPanel><MenuBar><DropDownButton x:Name='File' Content='File'/></MenuBar><Popup PlacementTarget='File'><TextBox/></Popup></StackPanel>",
    ] {
        assert!(parse(&window(markup)).is_err(), "{markup}");
    }
}
