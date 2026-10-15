use crate::*;

fn window(content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>")
}

fn parsed(content: &str) -> WindowDefinition {
    parse(&window(content)).unwrap()
}

fn ribbon() -> &'static str {
    "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab></Ribbon>"
}

fn docking(content: &str) -> String {
    format!(
        "<DockWorkspace><DockGroup><DockPane Id='First'>{content}</DockPane></DockGroup></DockWorkspace>"
    )
}

#[test]
fn empty_groups_have_no_implicit_editor_and_expanders_start_expanded() {
    for tag in ["GroupBox", "Expander"] {
        let definition = parsed(&format!("<{tag}/>"));
        assert!(definition.content.children().is_empty());
        match &definition.content.kind {
            ElementKind::GroupBox(group) => assert_eq!(group, &GroupBoxDefinition::default()),
            ElementKind::Expander(expander) => {
                assert_eq!(expander, &ExpanderDefinition::default());
                assert!(expander.is_expanded);
            }
            _ => panic!("expected group"),
        }
        let generated = compile(&window(&format!("<{tag}/>"))).unwrap();
        assert!(generated.contains("child: None"));
        assert!(!generated.contains("EditBoxDefinition"));
    }
}

#[test]
fn groups_preserve_appearance_padding_and_source_ordered_content() {
    for (appearance, expected) in [
        ("Card", GroupBoxAppearanceDefinition::Card),
        ("Outline", GroupBoxAppearanceDefinition::Outline),
        ("Plain", GroupBoxAppearanceDefinition::Plain),
        ("Standard", GroupBoxAppearanceDefinition::Standard),
    ] {
        let mut definition = parsed(&format!(
            "<GroupBox Header='Account' Appearance='{appearance}' Padding='1,2,3,4'><Expander Header='Details' Appearance='{appearance}' IsExpanded='False' Padding='5'><StackPanel><TextBox x:Name='Name'/><CheckBox x:Name='Active'/></StackPanel></Expander></GroupBox>"
        ));
        let ElementKind::GroupBox(group) = &definition.content.kind else {
            panic!()
        };
        assert_eq!(group.header, "Account");
        assert_eq!(group.appearance, expected);
        assert_eq!(group.padding, [1.0, 2.0, 3.0, 4.0]);
        let children = definition.content.children();
        assert_eq!(children.len(), 1);
        let ElementKind::Expander(expander) = &children[0].kind else {
            panic!()
        };
        assert!(!expander.is_expanded);
        assert_eq!(expander.appearance, expected);
        assert_eq!(expander.padding, [5.0; 4]);
        assert_eq!(
            children[0].children()[0].children()[1]
                .common
                .name
                .as_deref(),
            Some("Active")
        );
        definition.content.children_mut()[0].children_mut()[0].children_mut()[1]
            .common
            .is_enabled = false;
        assert!(
            !definition.content.children()[0].children()[0].children()[1]
                .common
                .is_enabled
        );
    }
}

#[test]
fn group_headers_preserve_independent_style_icon_colors_and_radius() {
    for (style, expected) in [
        ("Normal", GroupHeaderStyleDefinition::Normal),
        ("Light", GroupHeaderStyleDefinition::Light),
        ("Accent", GroupHeaderStyleDefinition::Accent),
    ] {
        for tag in ["GroupBox", "Expander"] {
            let source = window(&format!(
                "<{tag} Header='Параметры' HeaderStyle='{style}' HeaderIcon='Edit' HeaderForeground='#123456' HeaderBackground='#80445566' CornerRadius='7' Appearance='Standard'/>"
            ));
            let definition = parse(&source).unwrap();
            let (header_style, icon, foreground, background, radius) = match definition.content.kind
            {
                ElementKind::GroupBox(g) => (
                    g.header_style,
                    g.header_icon,
                    g.header_foreground,
                    g.header_background,
                    g.corner_radius,
                ),
                ElementKind::Expander(g) => (
                    g.header_style,
                    g.header_icon,
                    g.header_foreground,
                    g.header_background,
                    g.corner_radius,
                ),
                _ => panic!(),
            };
            assert_eq!(header_style, expected);
            assert_eq!(icon, Some(IconSymbol::Edit));
            assert_eq!(foreground, Some([0x12, 0x34, 0x56, 0xff]));
            assert_eq!(background, Some([0x44, 0x55, 0x66, 0x80]));
            assert_eq!(radius, Some(7.0));
            let generated = compile(&source).unwrap();
            assert!(generated.contains(&format!("GroupHeaderStyleDefinition::{style}")));
            assert!(generated.contains("Some(rust_desktop_ui_xaml::IconSymbol::Edit)"));
            assert!(generated.contains("corner_radius: Some(7.0)"));
            assert!(generated.contains("header_background: Some([68, 85, 102, 128])"));
        }
    }
    assert!(parse(&window("<GroupBox ShowHeader='False'/>")).is_ok());
}

#[test]
fn group_box_expansion_requires_opt_in_independent_of_attribute_order() {
    for attributes in [
        "IsCollapsible='True' IsExpanded='False'",
        "IsExpanded='False' IsCollapsible='True'",
        "IsExpanded='{Binding Expanded}' IsCollapsible='True'",
    ] {
        let definition = parsed(&format!("<GroupBox {attributes}/>"));
        let ElementKind::GroupBox(group) = &definition.content.kind else {
            panic!()
        };
        assert!(group.is_collapsible);
        if let Some(binding) = definition.content.common.bindings.get("IsExpanded") {
            assert_eq!(binding.mode, BindingMode::TwoWay);
            assert_eq!(
                binding.update_source_trigger,
                UpdateSourceTrigger::PropertyChanged
            );
        } else {
            assert!(!group.is_expanded);
        }
        validate_window_docking(&definition).unwrap();
    }
    for source in [
        "<GroupBox IsExpanded='False'/>",
        "<GroupBox IsExpanded='{Binding Expanded}'/>",
        "<GroupBox ShowHeader='False' IsCollapsible='True'/>",
        "<Expander ShowHeader='False'/>",
        "<Expander IsCollapsible='False'/>",
    ] {
        assert!(parse(&window(source)).is_err(), "{source}");
    }
    let mut definition = parsed("<GroupBox IsCollapsible='True' IsExpanded='{Binding Open}'/>");
    let ElementKind::GroupBox(group) = &mut definition.content.kind else {
        panic!()
    };
    group.is_collapsible = false;
    assert!(validate_window_docking(&definition).is_err());
}

#[test]
fn rich_group_header_properties_reject_invalid_values_before_mount() {
    for tag in ["GroupBox", "Expander"] {
        for attribute in [
            "HeaderStyle='normal'",
            "HeaderStyle='{Binding Style}'",
            "HeaderIcon='Unknown'",
            "HeaderIcon='{Binding Icon}'",
            "ShowHeader='Maybe'",
            "CornerRadius='NaN'",
            "CornerRadius='inf'",
            "CornerRadius='-1'",
            "CornerRadius='16385'",
            "HeaderForeground='red'",
            "HeaderBackground='#12'",
        ] {
            assert!(
                parse(&window(&format!("<{tag} {attribute}/>"))).is_err(),
                "{tag} {attribute}"
            );
        }
    }
    for radius in [f32::NAN, f32::INFINITY, -1.0, 16385.0] {
        assert!(
            validate_group_box(&GroupBoxDefinition {
                corner_radius: Some(radius),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_expander(&ExpanderDefinition {
                corner_radius: Some(radius),
                ..Default::default()
            })
            .is_err()
        );
    }
    assert!(
        validate_expander(&ExpanderDefinition {
            show_header: false,
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn tabbed_groups_compile_all_retained_pages_and_immediate_selection_bindings() {
    let source = window(
        "<TabbedGroup x:Name='Pages' DataContext='{Binding Form}' SelectedIndex='{Binding CurrentPage}' Padding='1,2,3,4'><GroupTab Header='Один &quot;\\' HeaderIcon='Edit'><TextBox x:Name='First' Text='{Binding Name}'/></GroupTab><GroupTab Header='Two' IsEnabled='False'><Expander IsExpanded='False'><TextBox x:Name='Second'/></Expander></GroupTab><GroupTab Header='Empty'/></TabbedGroup>",
    );
    let mut definition = parse(&source).unwrap();
    let ElementKind::TabbedGroup(group) = &definition.content.kind else {
        panic!()
    };
    assert_eq!(group.padding, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(group.selected_index, 0);
    assert_eq!(group.tabs.len(), 3);
    assert_eq!(group.tabs[0].header_icon, Some(IconSymbol::Edit));
    assert!(!group.tabs[1].is_enabled);
    assert!(group.tabs[2].child.is_none());
    assert_eq!(definition.content.children().len(), 2);
    let binding = &definition.content.common.bindings["SelectedIndex"];
    assert_eq!(binding.mode, BindingMode::TwoWay);
    assert_eq!(
        binding.update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    definition.content.children_mut()[1].children_mut()[0]
        .common
        .is_enabled = false;
    assert!(
        !definition.content.children()[1].children()[0]
            .common
            .is_enabled
    );
    let generated = compile(&source).unwrap();
    assert!(generated.contains("TabbedGroupDefinition { selected_index: 0"));
    assert!(generated.contains("GroupTabDefinition { header: \"Один \\\"\\\\\".into()"));
    assert!(generated.contains("Some(\"Second\".into())"));
    assert!(generated.contains("is_enabled: false, child: Some"));
    validate_window_docking(&definition).unwrap();
}

#[test]
fn tabbed_groups_reject_invalid_selection_and_metadata_syntax() {
    for index in ["-1", "+0", "0.5", "NaN", "inf", "1", "18446744073709551616"] {
        assert!(
            parse(&window(&format!(
                "<TabbedGroup SelectedIndex='{index}'><GroupTab Header='One'/></TabbedGroup>"
            )))
            .is_err(),
            "{index}"
        );
    }
    for content in [
        "<TabbedGroup/>",
        "<TabbedGroup><TextBox/></TabbedGroup>",
        "<TabbedGroup><GroupTab/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header=' '/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='One' IsEnabled='False'/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='One' x:Name='Page'/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='One' DataContext='{Binding Model}'/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='{Binding Caption}'/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='One' IsEnabled='{Binding Enabled}'/></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='One'><TextBox/><TextBox/></GroupTab></TabbedGroup>",
        "<TabbedGroup><GroupTab Header='One'>text</GroupTab></TabbedGroup>",
        "<GroupTab Header='One'/>",
    ] {
        assert!(parse(&window(content)).is_err(), "{content}");
    }
    let definition = parsed(
        "<TabbedGroup SelectedIndex='1'><GroupTab Header='Disabled' IsEnabled='False'/><GroupTab Header='Enabled'/></TabbedGroup>",
    );
    validate_window_docking(&definition).unwrap();
    let oversized = format!(
        "<TabbedGroup>{}</TabbedGroup>",
        "<GroupTab Header='Page'/>".repeat(65)
    );
    assert!(parse(&window(&oversized)).is_err());
    let long = format!(
        "<TabbedGroup><GroupTab Header='{}'/></TabbedGroup>",
        "я".repeat(2049)
    );
    assert!(parse(&window(&long)).is_err());
}

#[test]
fn tab_metadata_counts_toward_xml_and_manual_tree_budgets() {
    let tabs = format!(
        "<TabbedGroup>{}</TabbedGroup>",
        "<GroupTab Header='Page'/>".repeat(64)
    );
    let source = |extra| {
        window(&format!(
            "<StackPanel>{tabs}{}</StackPanel>",
            "<TextBlock/>".repeat(extra)
        ))
    };
    let valid = parse(&source(190)).unwrap();
    validate_window_docking(&valid).unwrap();
    assert!(parse(&source(191)).is_err());
    let mut invalid = valid;
    let ElementKind::StackPanel(panel) = &mut invalid.content.kind else {
        panic!()
    };
    panel.children.push(ElementDefinition::default());
    assert!(validate_window_docking(&invalid).is_err());
    let nested = |levels| {
        format!(
            "{}<TextBox/>{}",
            "<TabbedGroup><GroupTab Header='Page'>".repeat(levels),
            "</GroupTab></TabbedGroup>".repeat(levels)
        )
    };
    let valid = parsed(&format!("<GroupBox>{}</GroupBox>", nested(15)));
    validate_window_docking(&valid).unwrap();
    assert!(parse(&window(&nested(16))).is_err());
    let invalid = WindowDefinition {
        content: ElementDefinition {
            kind: ElementKind::GroupBox(GroupBoxDefinition {
                child: Some(Box::new(valid.content)),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(validate_window_docking(&invalid).is_err());
}

#[test]
fn unselected_pages_keep_names_radio_and_ribbon_docking_validation() {
    let tabs = |first: &str, second: &str| {
        format!(
            "<TabbedGroup><GroupTab Header='One'>{first}</GroupTab><GroupTab Header='Two'>{second}</GroupTab></TabbedGroup>"
        )
    };
    assert!(
        parse(&window(&tabs(
            "<TextBox x:Name='Same'/>",
            "<TextBox x:Name='Same'/>"
        )))
        .is_err()
    );
    assert!(
        parse(&window(&tabs(
            "<RadioButton IsChecked='True'/>",
            "<RadioButton IsChecked='True'/>"
        )))
        .is_ok()
    );
    assert!(
        parse(&window(&tabs(
            "<RadioButton GroupName='Shared' IsChecked='True'/>",
            "<RadioButton GroupName='Shared' IsChecked='True'/>"
        )))
        .is_err()
    );
    assert!(
        parse(&window(&tabs(ribbon(), ribbon())))
            .unwrap_err()
            .to_string()
            .contains("at most one Ribbon")
    );
    for forbidden in [ribbon().to_owned(), docking("<TextBox/>")] {
        assert!(parse(&window(&docking(&tabs("<TextBox/>", &forbidden)))).is_err());
    }
    let valid = parsed(&tabs("<TextBox/>", &docking("<TextBox/>")));
    validate_window_docking(&valid).unwrap();
}

#[test]
fn manual_tabbed_group_preflight_validates_all_page_metadata() {
    let mut group = TabbedGroupDefinition {
        tabs: vec![GroupTabDefinition {
            header: "Page".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    validate_tabbed_group(&group).unwrap();
    group.selected_index = 1;
    assert!(validate_tabbed_group(&group).is_err());
    group.selected_index = 0;
    group.tabs[0].is_enabled = false;
    assert!(validate_tabbed_group(&group).is_err());
    group.tabs[0].is_enabled = true;
    group.padding[0] = f32::NAN;
    assert!(validate_tabbed_group(&group).is_err());
    group.padding[0] = 16.0;
    group.tabs.push(GroupTabDefinition::default());
    assert!(validate_tabbed_group(&group).is_err());
}

#[test]
fn group_bindings_default_to_one_way_header_and_immediate_two_way_expansion() {
    let definition = parsed(
        "<GroupBox Header='{Binding Title}' DataContext='{Binding Customer}'><Expander Header='{Binding Path=Header, ElementName=Source}' IsExpanded='{Binding DetailsOpen}'><TextBox Text='{Binding Name}'/></Expander></GroupBox>",
    );
    let header = &definition.content.common.bindings["Header"];
    assert_eq!(header.mode, BindingMode::OneWay);
    assert_eq!(
        header.update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    assert_eq!(
        definition.content.common.data_context.as_deref(),
        Some("Customer")
    );
    let expander = definition.content.children()[0];
    assert_eq!(
        expander.common.bindings["Header"].element_name.as_deref(),
        Some("Source")
    );
    let expanded = &expander.common.bindings["IsExpanded"];
    assert_eq!(expanded.mode, BindingMode::TwoWay);
    assert_eq!(
        expanded.update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    assert!(expander.common.data_context.is_none());
    assert_eq!(expander.children()[0].common.bindings["Text"].path, "Name");
    let explicit = parsed(
        "<Expander IsExpanded='{Binding DetailsOpen, Mode=TwoWay, UpdateSourceTrigger=Explicit}'/>",
    );
    assert_eq!(
        explicit.content.common.bindings["IsExpanded"].update_source_trigger,
        UpdateSourceTrigger::Explicit
    );
    let one_way = parsed("<Expander IsExpanded='{Binding DetailsOpen, Mode=OneWay}'/>");
    assert_eq!(
        one_way.content.common.bindings["IsExpanded"].mode,
        BindingMode::OneWay
    );
}

#[test]
fn immediate_binding_defaults_do_not_match_argument_words_in_paths_or_literals() {
    for (tag, property) in [
        ("Expander", "IsExpanded"),
        ("CheckBox", "IsChecked"),
        ("ComboBox", "SelectedValue"),
    ] {
        let definition = parsed(&format!(
            "<{tag} {property}=\"{{Binding UpdateSourceTriggerEnabled, FallbackValue='UpdateSourceTrigger'}}\"/>"
        ));
        assert_eq!(
            definition.content.common.bindings[property].update_source_trigger,
            UpdateSourceTrigger::PropertyChanged
        );
        let definition = parsed(&format!(
            "<{tag} {property}=\"{{Binding Open, UpdateSourceTrigger=LostFocus}}\"/>"
        ));
        assert_eq!(
            definition.content.common.bindings[property].update_source_trigger,
            UpdateSourceTrigger::LostFocus
        );
    }
}

#[test]
fn grouping_supports_common_layout_and_state_properties() {
    for tag in ["GroupBox", "Expander"] {
        let definition = parsed(&format!(
            "<{tag} x:Name='Settings' Width='320' Height='240' MinWidth='100' MaxWidth='400' Margin='1,2,3,4' HorizontalAlignment='Right' VerticalAlignment='Top' Grid.Row='2' Grid.Column='1' Grid.RowSpan='3' DockPanel.Dock='Bottom' IsEnabled='False' Visibility='Hidden' AutomationName='Settings group' ToolTip='Details'/>"
        ));
        let common = definition.content.common;
        assert_eq!(common.name.as_deref(), Some("Settings"));
        assert_eq!((common.width, common.height), (Some(320.0), Some(240.0)));
        assert_eq!((common.min_width, common.max_width), (100.0, Some(400.0)));
        assert_eq!(common.margin, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(common.horizontal_alignment, Alignment::End);
        assert_eq!(common.vertical_alignment, Alignment::Start);
        assert_eq!(
            (common.grid_row, common.grid_column, common.grid_row_span),
            (2, 1, 3)
        );
        assert_eq!(common.dock, DockSide::Bottom);
        assert!(!common.is_enabled);
        assert_eq!(common.visibility, Visibility::Hidden);
        assert_eq!(common.automation_name.as_deref(), Some("Settings group"));
        assert_eq!(common.tool_tip.as_deref(), Some("Details"));
    }
}

#[test]
fn group_headers_enforce_utf8_byte_limit_and_codegen_escapes_content() {
    for tag in ["GroupBox", "Expander"] {
        assert!(parse(&window(&format!("<{tag} Header='{}'/>", "я".repeat(2048)))).is_ok());
        assert!(
            parse(&window(&format!("<{tag} Header='{}'/>", "я".repeat(2049))))
                .unwrap_err()
                .to_string()
                .contains("4096")
        );
        let source = window(&format!(
            "<{tag} Header='Настройки &quot;\\&#10;🌍' Appearance='Outline'><TextBlock Text='Details'/></{tag}>"
        ));
        let generated = compile(&source).unwrap();
        assert!(generated.contains("header: \"Настройки \\\"\\\\\\n🌍\".into()"));
        assert!(generated.contains("GroupBoxAppearanceDefinition::Outline"));
        assert!(generated.contains("child: Some(Box::new("));
        assert!(generated.contains("ElementKind::TextBlock"));
    }
    let generated = compile(&window(
        "<Expander IsExpanded='False' Header='{Binding Title}'/>",
    ))
    .unwrap();
    assert!(generated.contains("is_expanded: false"));
    assert!(generated.contains("(\"Header\".into(), rust_desktop_ui_xaml::BindingDefinition"));
}

#[test]
fn grouping_rejects_unknown_markup_and_multiple_content_children() {
    for tag in ["GroupBox", "Expander"] {
        for attributes in [
            "Appearance='Rounded'",
            "Appearance='card'",
            "Padding='-1'",
            "Padding='NaN'",
            "Padding='16385'",
            "Padding='1,2,3'",
            "HeaderTemplate='Custom'",
            "Content='Text'",
            "Header='{StaticResource Title}'",
            "Appearance='{Binding Appearance}'",
            "Padding='{Binding Padding}'",
            "IsEnabled='Maybe'",
            "MinWidth='100' MaxWidth='50'",
            "Grid.Row='1023' Grid.RowSpan='2'",
        ] {
            assert!(
                parse(&window(&format!("<{tag} {attributes}/>"))).is_err(),
                "{tag} {attributes}"
            );
        }
        for child in [
            "text",
            "<TextBox/><TextBox/>",
            "<GroupBox.Header><TextBlock/></GroupBox.Header>",
            "<Expander.Content><TextBox/></Expander.Content>",
            "<Button xmlns='urn:foreign'/>",
        ] {
            assert!(
                parse(&window(&format!("<{tag}>{child}</{tag}>"))).is_err(),
                "{tag} {child}"
            );
        }
    }
    assert!(parse(&window("<GroupBox IsExpanded='False'/>")).is_err());
    assert!(parse(&window("<GroupBox IsExpanded='{Binding Open}'/>")).is_err());
    assert!(parse(&window("<Expander IsExpanded='yes'/>")).is_err());
}

#[test]
fn collapsed_groups_participate_in_depth_count_names_and_radio_validation() {
    let nested = |levels: usize| {
        format!(
            "{}<TextBox/>{}",
            "<Expander IsExpanded='False'>".repeat(levels),
            "</Expander>".repeat(levels)
        )
    };
    assert!(parse(&window(&nested(MAX_ELEMENT_DEPTH - 1))).is_ok());
    assert!(parse(&window(&nested(MAX_ELEMENT_DEPTH))).is_err());
    let counted = |children: usize| {
        format!(
            "<GroupBox><StackPanel>{}</StackPanel></GroupBox>",
            "<Expander/>".repeat(children)
        )
    };
    assert!(parse(&window(&counted(MAX_ELEMENTS - 2))).is_ok());
    assert!(parse(&window(&counted(MAX_ELEMENTS - 1))).is_err());
    for content in [
        "<GroupBox x:Name='Same'><Expander x:Name='Same'/></GroupBox>",
        "<Expander IsExpanded='False'><StackPanel><RadioButton IsChecked='True'/><RadioButton IsChecked='True'/></StackPanel></Expander>",
    ] {
        assert!(parse(&window(content)).is_err(), "{content}");
    }
    assert!(parse(&window("<StackPanel><GroupBox><RadioButton IsChecked='True'/></GroupBox><Expander><RadioButton IsChecked='True'/></Expander></StackPanel>")).is_ok());
}

#[test]
fn grouping_does_not_hide_ribbon_or_docking_from_tree_validation() {
    let source = format!(
        "<StackPanel><GroupBox>{}</GroupBox><Expander IsExpanded='False'>{}</Expander></StackPanel>",
        ribbon(),
        ribbon()
    );
    assert!(
        parse(&window(&source))
            .unwrap_err()
            .to_string()
            .contains("at most one Ribbon")
    );
    for tag in ["GroupBox", "Expander"] {
        let hidden_ribbon = docking(&format!("<{tag}>{}</{tag}>", ribbon()));
        assert!(
            parse(&window(&hidden_ribbon))
                .unwrap_err()
                .to_string()
                .contains("cannot contain a Ribbon")
        );
        let hidden_workspace = docking(&format!("<{tag}>{}</{tag}>", docking("<TextBox/>")));
        assert!(
            parse(&window(&hidden_workspace))
                .unwrap_err()
                .to_string()
                .contains("cannot contain a DockWorkspace")
        );
    }
    let valid = parsed(&format!(
        "<GroupBox>{}</GroupBox>",
        docking("<Expander><TextBox/></Expander>")
    ));
    validate_window_docking(&valid).unwrap();
}

#[test]
fn manual_group_definitions_validate_scalars_and_hidden_child_tree() {
    for padding in [[f32::NAN; 4], [f32::INFINITY; 4], [-1.0; 4], [16385.0; 4]] {
        assert!(
            validate_group_box(&GroupBoxDefinition {
                padding,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_expander(&ExpanderDefinition {
                padding,
                ..Default::default()
            })
            .is_err()
        );
    }
    validate_group_box(&GroupBoxDefinition {
        padding: [0.0, 1.0, 16384.0, 16.0],
        ..Default::default()
    })
    .unwrap();
    let mut definition =
        parsed("<Expander IsExpanded='False'><GroupBox Header='Valid'/></Expander>");
    let ElementKind::GroupBox(group) = &mut definition.content.children_mut()[0].kind else {
        panic!()
    };
    group.header = "я".repeat(2049);
    assert!(
        validate_window_docking(&definition)
            .unwrap_err()
            .contains("4096")
    );
    let mut definition = parsed("<GroupBox><Expander><TextBox/></Expander></GroupBox>");
    definition.content.common.name = Some("Same".into());
    definition.content.children_mut()[0].common.name = Some("Same".into());
    assert!(
        validate_window_docking(&definition)
            .unwrap_err()
            .contains("Duplicate")
    );
    let mut definition = parsed(&docking("<Expander/>"));
    let ElementKind::Expander(expander) = &mut definition.content.children_mut()[0].kind else {
        panic!()
    };
    expander.child = Some(Box::new(parsed(ribbon()).content));
    assert!(
        validate_window_docking(&definition)
            .unwrap_err()
            .contains("cannot contain a Ribbon")
    );
}
