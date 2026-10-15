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
