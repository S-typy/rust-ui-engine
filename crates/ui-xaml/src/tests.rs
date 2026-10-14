use super::*;

#[test]
fn text_block_typography_defaults_and_supported_weights_are_explicit() {
    let plain = parse(&form("", "<TextBlock Text='Title'/>")).unwrap();
    assert_eq!(
        plain.content.kind,
        ElementKind::TextBlock(TextBlockDefinition {
            text: "Title".into(),
            ..Default::default()
        })
    );
    for (source, weight) in [
        ("Normal", 400),
        ("SemiBold", 600),
        ("Bold", 700),
        ("1", 1),
        ("525", 525),
        ("1000", 1000),
    ] {
        let window = parse(&form("", &format!("<TextBlock Text='Title &amp; detail' FontSize='18.5' FontWeight='{source}' Foreground='#80445566' TextTone='Secondary' TextWrapping='NoWrap'/>"))).unwrap();
        let ElementKind::TextBlock(text) = window.content.kind else {
            panic!("text block")
        };
        assert_eq!(
            text,
            TextBlockDefinition {
                text: "Title & detail".into(),
                font_size: Some(18.5),
                font_weight: Some(weight),
                foreground: Some([68, 85, 102, 128]),
                tone: TextToneDefinition::Secondary,
                wrap: false,
            }
        );
        validate_text_block(&text).unwrap();
    }
    let boundary = parse(&form(
        "",
        "<TextBlock FontSize='512' Foreground='#123456' TextTone='Primary' TextWrapping='Wrap'/>",
    ))
    .unwrap();
    let ElementKind::TextBlock(text) = boundary.content.kind else {
        panic!("text block")
    };
    assert_eq!(text.foreground, Some([18, 52, 86, 255]));
    assert_eq!(text.font_size, Some(512.0));
    assert!(text.wrap);
    assert_eq!(text.tone, TextToneDefinition::Primary);
}

#[test]
fn text_block_typography_rejects_invalid_scalars_and_preserves_text_content_rules() {
    for attributes in [
        "FontSize='0'",
        "FontSize='-1'",
        "FontSize='512.1'",
        "FontSize='NaN'",
        "FontSize='inf'",
        "FontSize='1e-300'",
        "FontWeight='0'",
        "FontWeight='1001'",
        "FontWeight='65536'",
        "FontWeight='-1'",
        "FontWeight='400.0'",
        "FontWeight='+400'",
        "FontWeight='normal'",
        "FontWeight='Semibold'",
        "FontWeight=' 600 '",
        "FontWeight='６００'",
        "FontWeight=''",
        "Foreground='Red'",
        "Foreground='#123'",
        "Foreground='#GG112233'",
        "TextTone='Muted'",
        "TextTone='secondary'",
        "TextWrapping='true'",
        "TextWrapping='WrapWithOverflow'",
    ] {
        let error = parse(&form("", &format!("<TextBlock\n {attributes}/>"))).unwrap_err();
        assert!(error.line > 1 && error.column > 0, "{attributes}: {error}");
        assert!(
            error
                .message
                .contains(attributes.split('=').next().unwrap()),
            "{attributes}: {error}"
        );
    }
    for content in [
        "<TextBlock FontSize='18'>inline text</TextBlock>",
        "<TextBlock FontWeight='Bold'><Button/></TextBlock>",
        "<TextBlock Text='{Binding Title}' FontSize='18'/>",
        "<TextBlock Foreground='{StaticResource Text}'/>",
        "<TextBlock xmlns:p='urn:other' p:TextTone='Secondary'/>",
        "<Button TextTone='Secondary'/>",
        "<Button TextWrapping='NoWrap'/>",
    ] {
        assert!(parse(&form("", content)).is_err(), "{content}");
    }
}

#[test]
fn stack_panel_surface_is_typed_and_local_to_panels() {
    for (attribute, expected) in [
        ("", SurfaceKindDefinition::Transparent),
        ("Surface='Transparent'", SurfaceKindDefinition::Transparent),
        ("Surface='Card'", SurfaceKindDefinition::Card),
    ] {
        let window = parse(&form(
            "",
            &format!(
                "<StackPanel {attribute} Padding='16'><TextBlock Text='Content'/></StackPanel>"
            ),
        ))
        .unwrap();
        let ElementKind::StackPanel(panel) = window.content.kind else {
            panic!("panel")
        };
        assert_eq!(panel.surface, expected);
        assert_eq!(panel.padding, [16.0; 4]);
        assert_eq!(panel.children.len(), 1);
    }
    for content in [
        "<StackPanel Surface='card'/>",
        "<StackPanel Surface='Solid'/>",
        "<StackPanel Surface=' Card '/>",
        "<StackPanel Surface='{Binding Surface}'/>",
        "<StackPanel Background='#FFFFFF'/>",
        "<TextBlock Surface='Card'/>",
        "<Button Surface='Card'/>",
    ] {
        assert!(parse(&form("", content)).is_err(), "{content}");
    }
}

#[test]
fn text_block_manual_preflight_rejects_invalid_font_metrics() {
    for font_size in [
        0.0,
        -1.0,
        512.01,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let text = TextBlockDefinition {
            font_size: Some(font_size),
            ..Default::default()
        };
        assert!(validate_text_block(&text).unwrap_err().contains("FontSize"));
    }
    for font_weight in [0, 1001, u16::MAX] {
        let text = TextBlockDefinition {
            font_weight: Some(font_weight),
            ..Default::default()
        };
        assert!(
            validate_text_block(&text)
                .unwrap_err()
                .contains("FontWeight")
        );
    }
    for (font_size, font_weight) in [(f32::MIN_POSITIVE, 1), (512.0, 1000)] {
        validate_text_block(&TextBlockDefinition {
            font_size: Some(font_size),
            font_weight: Some(font_weight),
            ..Default::default()
        })
        .unwrap();
    }
    validate_text_block(&TextBlockDefinition::default()).unwrap();
}

#[test]
fn typography_and_card_codegen_preserves_every_field_and_escaped_text() {
    let source = form(
        "",
        r##"<StackPanel Surface="Card" Padding="16">
        <TextBlock Text="Title &quot;\&#10;🌍" FontSize="24.5" FontWeight="SemiBold" Foreground="#80445566" TextTone="Secondary" TextWrapping="NoWrap"/>
        <StackPanel><TextBlock Text="Default"/></StackPanel>
    </StackPanel>"##,
    );
    let generated = compile(&source).unwrap();
    for expected in [
        "surface: rust_desktop_ui_xaml::SurfaceKindDefinition::Card",
        "surface: rust_desktop_ui_xaml::SurfaceKindDefinition::Transparent",
        "text: \"Title \\\"\\\\\\n🌍\".into()",
        "font_size: Some(24.5)",
        "font_weight: Some(600)",
        "foreground: Some([68, 85, 102, 128])",
        "tone: rust_desktop_ui_xaml::TextToneDefinition::Secondary, wrap: false",
        "font_size: None, font_weight: None, foreground: None, tone: rust_desktop_ui_xaml::TextToneDefinition::Primary, wrap: true",
    ] {
        assert!(
            generated.contains(expected),
            "missing {expected}\n{generated}"
        );
    }
    let equivalent = source.replace("FontWeight=\"SemiBold\"", "FontWeight=\"600\"");
    assert_eq!(parse(&source).unwrap(), parse(&equivalent).unwrap());
    assert_eq!(compile(&source).unwrap(), compile(&equivalent).unwrap());
}

#[test]
fn popup_buttons_compile_nested_items_separators_and_plain_tooltips() {
    let source = form(
        "",
        r#"<SplitButton Content="Accept" Command="Confirm" Icon="Check" ToolTip="Подсказка &quot;\&#10;🌍" FontSize="18"><MenuItem Header="Режим"><MenuItem Header="Pin" Command="Pin"/></MenuItem><Separator/><MenuItem Header="Disabled" Command="Confirm" IsEnabled="False"/></SplitButton>"#,
    );
    let parsed = parse(&source).unwrap();
    assert_eq!(
        parsed.content.common.tool_tip.as_deref(),
        Some("Подсказка \"\\\n🌍")
    );
    let ElementKind::SplitButton(menu) = parsed.content.kind else {
        panic!("split");
    };
    assert_eq!(menu.button.command.as_deref(), Some("Confirm"));
    assert_eq!(menu.button.style.font_size, Some(18.0));
    assert_eq!(menu.items.len(), 3);
    assert_eq!(menu.items[1], MenuEntryDefinition::Separator);
    let MenuEntryDefinition::Item(submenu) = &menu.items[0] else {
        panic!("submenu");
    };
    assert!(submenu.command.is_none());
    assert_eq!(submenu.items.len(), 1);
    let generated = compile(&source).unwrap();
    for part in [
        "ElementKind::SplitButton",
        "MenuEntryDefinition::Separator",
        "MenuItemDefinition",
        "command: Some(\"Pin\".into())",
        "is_enabled: false",
        "tool_tip: Some(\"Подсказка \\\"\\\\\\n🌍\".into())",
    ] {
        assert!(generated.contains(part), "{part}");
    }
    assert!(
        parse(&form(
            "",
            "<DropDownButton Content='Menu'><MenuItem Header='Do' Command='Do'/></DropDownButton>"
        ))
        .is_ok()
    );
}

#[test]
fn popup_markup_rejects_ambiguous_actions_unknown_contexts_and_menu_geometry() {
    for markup in [
        "<DropDownButton/>",
        "<SplitButton><MenuItem Header='Do' Command='Do'/></SplitButton>",
        "<DropDownButton Command='Do'><MenuItem Header='Do' Command='Do'/></DropDownButton>",
        "<MenuItem Header='Do' Command='Do'/>",
        "<DropDownButton><MenuItem Command='Do'/></DropDownButton>",
        "<DropDownButton><MenuItem Header=' '/></DropDownButton>",
        "<DropDownButton><MenuItem Header='Do'/></DropDownButton>",
        "<DropDownButton><MenuItem Header='Do' Command='Do'><MenuItem Header='Nested' Command='Other'/></MenuItem></DropDownButton>",
        "<DropDownButton><MenuItem Header='Do' Command='A.B'/></DropDownButton>",
        "<DropDownButton><MenuItem Header='Do' Command='{Binding Do}'/></DropDownButton>",
        "<DropDownButton><MenuItem Header='Do' Command='Do' x:Name='Transient'/></DropDownButton>",
        "<DropDownButton><Separator Thickness='2'/></DropDownButton>",
        "<DropDownButton><Separator><MenuItem Header='Do' Command='Do'/></Separator></DropDownButton>",
        "<DropDownButton><Button Content='Do'/></DropDownButton>",
        "<DropDownButton><MenuItem Header='Do' Command='Do'>text</MenuItem></DropDownButton>",
        "<DropDownButton ToolTip='{Binding Tip}'><Separator/></DropDownButton>",
    ] {
        assert!(parse(&form("", markup)).is_err(), "{markup}");
    }
}

#[test]
fn popup_depth_and_count_include_entries_in_the_bounded_content_tree() {
    let wrap = |entries: &str| form("", &format!("<DropDownButton>{entries}</DropDownButton>"));
    let mut nested = "<MenuItem Header='Leaf' Command='Do'/>".to_string();
    for _ in 1..MAX_MENU_DEPTH {
        nested = format!("<MenuItem Header='Sub'>{nested}</MenuItem>");
    }
    assert!(parse(&wrap(&nested)).is_ok());
    assert!(
        parse(&wrap(&format!(
            "<MenuItem Header='Extra'>{nested}</MenuItem>"
        )))
        .unwrap_err()
        .message
        .contains("depth")
    );
    let item = "<MenuItem Header='Do' Command='Do'/>";
    assert!(parse(&wrap(&item.repeat(MAX_ELEMENTS - 1))).is_ok());
    assert!(parse(&wrap(&item.repeat(MAX_ELEMENTS))).is_err());
}

#[test]
fn tooltip_limit_counts_utf8_bytes_and_is_common_to_layout_elements() {
    let boundary = "Я".repeat(MAX_TOOLTIP_BYTES / 2);
    assert!(parse(&form("", &format!("<StackPanel ToolTip='{boundary}'/>"))).is_ok());
    let error = parse(&form("", &format!("<Button ToolTip='{boundary}a'/>"))).unwrap_err();
    assert!(error.message.contains("ToolTip"));
    assert_eq!(
        parse(&form("", "<TextBox ToolTip=''/>"))
            .unwrap()
            .content
            .common
            .tool_tip
            .as_deref(),
        Some("")
    );
}

fn button_style(window: &WindowDefinition) -> &ButtonStyleDefinition {
    match &window.content.kind {
        ElementKind::Button(button) => &button.style,
        ElementKind::ToggleButton(button) => &button.style,
        ElementKind::CheckBox(button) => &button.style,
        ElementKind::RadioButton(button) => &button.style,
        _ => panic!("expected a button-like element"),
    }
}

fn form(window_attributes: &str, text_box: &str) -> String {
    format!(
        "<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}' {window_attributes}>\n{text_box}\n</Window>"
    )
}

fn editor(window: &WindowDefinition) -> &EditBoxDefinition {
    let ElementKind::TextBox(editor) = &window.content.kind else {
        panic!("expected a TextBox root");
    };
    editor
}

#[test]
fn minimal_form_has_documented_defaults() {
    assert_eq!(
        parse(&form("", "<TextBox/>")),
        Ok(WindowDefinition::default())
    );
}

#[test]
fn parses_properties_unicode_entities_and_thickness_order() {
    let source = form(
        "Title='Книга &amp; Author' Width='800.5' Height='480' Padding='1,2,3,4' Theme='Dark'",
        "<TextBox x:Name='_title2' Text='Привет 🌍 &quot;Rust&quot;' PlaceholderText='Имя' AutomationName='Название' Width='320' Height='40' Margin='8,12' IsEnabled='False'/>",
    );
    let parsed = parse(&source).unwrap();
    assert_eq!(parsed.title, "Книга & Author");
    assert_eq!((parsed.width, parsed.height), (800.5, 480.0));
    assert_eq!(parsed.padding, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(parsed.theme, Theme::Dark);
    assert_eq!(parsed.content.common.name.as_deref(), Some("_title2"));
    assert_eq!(editor(&parsed).text, "Привет 🌍 \"Rust\"");
    assert_eq!(editor(&parsed).placeholder, "Имя");
    assert_eq!(
        parsed.content.common.automation_name.as_deref(),
        Some("Название")
    );
    assert_eq!(parsed.content.common.width, Some(320.0));
    assert_eq!(parsed.content.common.height, Some(40.0));
    assert_eq!(parsed.content.common.margin, [8.0, 12.0, 8.0, 12.0]);
    assert!(!parsed.content.common.is_enabled);
}

#[test]
fn namespace_uris_determine_meaning_instead_of_prefix_spelling() {
    let source = format!(
        "<ui:Window xmlns:ui='{UI_NAMESPACE}' xmlns:lang='{XAML_NAMESPACE}'><ui:TextBox lang:Name='editor'/></ui:Window>"
    );
    assert_eq!(
        parse(&source).unwrap().content.common.name.as_deref(),
        Some("editor")
    );
    let impostor = form("xmlns:fake='urn:wrong'", "<TextBox fake:Name='editor'/>");
    assert!(parse(&impostor).unwrap_err().message.contains("namespace"));
}

#[test]
fn rejects_wpf_missing_and_overridden_namespaces() {
    for source in [
        "<Window><TextBox/></Window>".to_owned(),
        form("", "<TextBox xmlns='urn:other'/>"),
        "<Window xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><TextBox/></Window>".to_owned(),
    ] {
        assert!(parse(&source).is_err(), "{source}");
    }
}

#[test]
fn rejects_unknown_properties_and_directives() {
    for source in [
        form("Ttile='Oops'", "<TextBox/>"),
        form("x:Name='window'", "<TextBox/>"),
        form("", "<TextBox Placeholder='Oops'/>"),
        form("", "<TextBox Name='editor'/>"),
        form("", "<TextBox x:Class='Danger'/>"),
        form("", "<TextBox xml:space='preserve'/>"),
    ] {
        assert!(parse(&source).is_err(), "{source}");
    }
}

#[test]
fn rejects_bindings_resources_and_other_extensions() {
    for property in [
        "Text='{Binding Name}'",
        "Text=' &#123;Binding Name}'",
        "PlaceholderText='{StaticResource Prompt}'",
        "Text='{}{literal}'",
    ] {
        let error = parse(&form("", &format!("<TextBox {property}/>"))).unwrap_err();
        assert!(error.message.contains("markup extensions"), "{error}");
    }
}

#[test]
fn rejects_invalid_or_duplicate_names() {
    for name in ["", "2name", "first-name", "two names", "текст", "a.b"] {
        let error = parse(&form("", &format!("<TextBox x:Name='{name}'/>"))).unwrap_err();
        assert!(error.message.contains("x:Name"), "{error}");
    }
    assert!(parse(&form("", "<TextBox x:Name='one' x:Name='two'/>")).is_err());
    assert!(
        parse(&form(
            &format!("xmlns:other='{XAML_NAMESPACE}'"),
            "<TextBox x:Name='one' other:Name='two'/>"
        ))
        .is_err()
    );
}

#[test]
fn requires_exactly_one_root_and_no_children_inside_leaves() {
    for child in [
        "",
        "<TextBox/><TextBox/>",
        "<Unknown/>",
        "<TextBox><TextBox/></TextBox>",
        "<Window.Content><TextBox/></Window.Content>",
    ] {
        assert!(parse(&form("", child)).is_err(), "{child}");
    }
}

#[test]
fn whitespace_and_comments_are_allowed_but_text_and_instructions_are_not() {
    assert!(parse(&form("", "<!-- note --><TextBox> \n </TextBox>")).is_ok());
    for child in [
        "hello<TextBox/>",
        "<TextBox>hello</TextBox>",
        "<TextBox><![CDATA[hello]]></TextBox>",
        "<?load resource?><TextBox/>",
    ] {
        assert!(parse(&form("", child)).is_err(), "{child}");
    }
}

#[test]
fn rejects_zero_nonfinite_and_out_of_range_dimensions() {
    for value in [
        "0", "-1", "NaN", "inf", "-inf", "1e1000", "16385", "Auto", "",
    ] {
        for source in [
            form(&format!("Width='{value}'"), "<TextBox/>"),
            form("", &format!("<TextBox Height='{value}'/>")),
        ] {
            assert!(parse(&source).is_err(), "{source}");
        }
    }
    assert!(parse(&form("Width='16384'", "<TextBox Height='0.5'/>")).is_ok());
    assert!(parse(&form("", "<TextBox Width='1e-300'/>")).is_err());
}

#[test]
fn rejects_invalid_spacing_bools_and_theme() {
    for spacing in [
        "",
        "1,2,3",
        "1,2,3,4,5",
        "1,",
        "NaN",
        "-1",
        "0,inf",
        "16385",
    ] {
        assert!(parse(&form(&format!("Padding='{spacing}'"), "<TextBox/>")).is_err());
    }
    assert!(parse(&form("Theme='Compact'", "<TextBox/>")).is_err());
    assert!(parse(&form("", "<TextBox IsEnabled='yes'/>")).is_err());
}

#[test]
fn readonly_and_length_properties_have_strict_values_and_codegen() {
    for (value, expected) in [
        ("True", true),
        ("true", true),
        ("False", false),
        ("false", false),
    ] {
        let parsed = parse(&form(
            "",
            &format!("<TextBox IsReadOnly='{value}' MaxLength='64'/>"),
        ))
        .unwrap();
        assert_eq!(editor(&parsed).is_read_only, expected);
        assert_eq!(editor(&parsed).max_length, Some(64));
    }
    for value in ["", "yes", "1", "TRUE", " true "] {
        let error = parse(&form("", &format!("<TextBox IsReadOnly='{value}'/>"))).unwrap_err();
        assert!(error.message.contains("IsReadOnly"));
    }
    for value in [
        "",
        "-1",
        "+1",
        "1.0",
        "1e2",
        " 1",
        "1 ",
        "١",
        "1048577",
        "18446744073709551616",
    ] {
        let error = parse(&form("", &format!("<TextBox MaxLength='{value}'/>"))).unwrap_err();
        assert!(error.message.contains("MaxLength"), "{value}: {error}");
    }
    assert_eq!(
        editor(&parse(&form("", "<TextBox MaxLength='1048576'/>")).unwrap()).max_length,
        Some(MAX_TEXT_LENGTH)
    );
    assert_eq!(
        editor(&parse(&form("", "<TextBox MaxLength='0'/>")).unwrap()).max_length,
        None
    );
    let generated = compile(&form("", "<TextBox IsReadOnly='True' MaxLength='64'/>")).unwrap();
    assert!(generated.contains("is_read_only: true,"));
    assert!(generated.contains("max_length: Some(64),"));
    assert!(
        compile(&form("", "<TextBox MaxLength='0'/>"))
            .unwrap()
            .contains("max_length: None,")
    );
}

#[test]
fn parses_style_metrics_and_argb_colors_without_theme_defaults() {
    let source = form(
        "",
        "<TextBox FontSize='18.5' Padding='1,2,3,4' CornerRadius='8' BorderThickness='2' Background='#102030' Foreground='#80102030' BorderBrush='#00AAbBcC' FocusBrush='#abcdef' PlaceholderForeground='#FF334455' SelectionBrush='#40667788'/>",
    );
    let parsed = parse(&source).unwrap();
    let style = editor(&parsed);
    assert_eq!(style.font_size, Some(18.5));
    assert_eq!(style.padding, Some([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(style.corner_radius, Some(8.0));
    assert_eq!(style.border_thickness, Some(2.0));
    assert_eq!(style.background, Some([16, 32, 48, 255]));
    assert_eq!(style.foreground, Some([16, 32, 48, 128]));
    assert_eq!(style.border_brush, Some([170, 187, 204, 0]));
    assert_eq!(style.focus_brush, Some([171, 205, 239, 255]));
    assert_eq!(style.placeholder_foreground, Some([51, 68, 85, 255]));
    assert_eq!(style.selection_brush, Some([102, 119, 136, 64]));
    let generated = compile(&source).unwrap();
    for field in [
        "font_size: Some(18.5),",
        "padding: Some([1.0, 2.0, 3.0, 4.0]),",
        "corner_radius: Some(8.0),",
        "border_thickness: Some(2.0),",
        "background: Some([16, 32, 48, 255]),",
        "foreground: Some([16, 32, 48, 128]),",
        "border_brush: Some([170, 187, 204, 0]),",
        "focus_brush: Some([171, 205, 239, 255]),",
        "placeholder_foreground: Some([51, 68, 85, 255]),",
        "selection_brush: Some([102, 119, 136, 64]),",
    ] {
        assert!(generated.contains(field), "{field}");
    }
    let defaults = compile(&form("", "<TextBox/>")).unwrap();
    for name in [
        "font_size",
        "padding",
        "corner_radius",
        "border_thickness",
        "background",
        "foreground",
        "border_brush",
        "focus_brush",
        "placeholder_foreground",
        "selection_brush",
    ] {
        assert!(defaults.contains(&format!("{name}: None,")), "{name}");
    }
}

#[test]
fn style_metrics_enforce_ranges_and_padding_arity() {
    for (property, values) in [
        (
            "FontSize",
            vec!["0", "-1", "513", "1e-300", "NaN", "inf", "", "12,14"],
        ),
        ("CornerRadius", vec!["-1", "16385", "NaN", "inf", "", "1,2"]),
        (
            "BorderThickness",
            vec!["-1", "16385", "NaN", "inf", "", "1,2"],
        ),
        (
            "Padding",
            vec!["-1", "16385", "NaN", "inf", "", "1,2,3", "1,2,3,4,5"],
        ),
    ] {
        for value in values {
            let source = form("", &format!("<TextBox\n    {property}='{value}'/>"));
            let error = parse(&source).unwrap_err();
            assert!(error.message.contains(property), "{property}: {error}");
            assert_eq!((error.line, error.column), (3, 5));
        }
    }
    for source in [
        "<TextBox FontSize='512' Padding='0' CornerRadius='16384' BorderThickness='16384'/>",
        "<TextBox FontSize='0.5' Padding='16384' CornerRadius='0' BorderThickness='0'/>",
    ] {
        assert!(parse(&form("", source)).is_ok());
    }
    assert_eq!(
        editor(&parse(&form("", "<TextBox Padding='12,5'/>")).unwrap()).padding,
        Some([12.0, 5.0, 12.0, 5.0])
    );
}

#[test]
fn brush_colors_require_strict_hex_and_report_the_attribute() {
    for property in [
        "Background",
        "Foreground",
        "BorderBrush",
        "FocusBrush",
        "PlaceholderForeground",
        "SelectionBrush",
        "ValidationBrush",
    ] {
        for value in [
            "Red",
            "Transparent",
            "#fff",
            "#12345",
            "#1234567",
            "#123456789",
            "#GG1122",
            "112233",
            " #112233",
            "#112233 ",
            "#１２３４５６",
            "",
        ] {
            let source = form("", &format!("<TextBox\n    {property}='{value}'/>"));
            let error = parse(&source).unwrap_err();
            assert!(error.message.contains(property), "{property}: {error}");
            assert!(error.message.contains("#RRGGBB or #AARRGGBB"), "{error}");
            assert_eq!((error.line, error.column), (3, 5));
        }
    }
    assert!(
        parse(&form("", "<TextBox Background='{StaticResource Brush}'/>"))
            .unwrap_err()
            .message
            .contains("markup extensions")
    );
}

#[test]
fn validation_properties_parse_and_compile_without_rejecting_initial_empty_text() {
    let source = form(
        "",
        "<TextBox IsRequired='True' CommitOnLostFocus='False' ValidationMessage='Введите &quot;текст&quot; &#10; здесь' ValidationBrush='#80402010'/>",
    );
    let window = parse(&source).unwrap();
    let parsed = editor(&window);
    assert!(parsed.is_required);
    assert!(!parsed.commit_on_lost_focus);
    assert_eq!(parsed.validation_message, "Введите \"текст\" \n здесь");
    assert_eq!(parsed.validation_brush, Some([64, 32, 16, 128]));
    let generated = compile(&source).unwrap();
    assert!(generated.contains("is_required: true,"));
    assert!(generated.contains("commit_on_lost_focus: false,"));
    assert!(generated.contains("validation_message: \"Введите \\\"текст\\\" \\n здесь\".into(),"));
    assert!(generated.contains("validation_brush: Some([64, 32, 16, 128]),"));
    let window = parse(&form("", "<TextBox/>")).unwrap();
    let defaults = editor(&window);
    assert!(!defaults.is_required);
    assert!(defaults.commit_on_lost_focus);
    assert_eq!(defaults.validation_message, "A value is required.");
    assert_eq!(defaults.validation_brush, None);
    for property in ["IsRequired", "CommitOnLostFocus"] {
        for value in ["", "yes", "1", "TRUE", " true "] {
            let error =
                parse(&form("", &format!("<TextBox\n    {property}='{value}'/>"))).unwrap_err();
            assert_eq!((error.line, error.column), (3, 5));
            assert!(error.message.contains(property));
        }
        for value in ["true", "false", "True", "False"] {
            assert!(parse(&form("", &format!("<TextBox {property}='{value}'/>"))).is_ok());
        }
    }
}

#[test]
fn validation_message_limit_counts_decoded_utf8_bytes_and_allows_runtime_fallback() {
    for message in [String::new(), "   ".to_owned(), "Я".repeat(2048)] {
        let source = form("", &format!("<TextBox ValidationMessage='{message}'/>"));
        assert_eq!(editor(&parse(&source).unwrap()).validation_message, message);
    }
    for message in [
        "Я".repeat(2049),
        "x".repeat(MAX_VALIDATION_MESSAGE_BYTES + 1),
        "&#x1F44B;".repeat(1025),
    ] {
        let source = form(
            "",
            &format!("<TextBox\n    ValidationMessage='{message}'/>"),
        );
        let error = parse(&source).unwrap_err();
        assert_eq!((error.line, error.column), (3, 5));
        assert!(error.message.contains("4096-byte UTF-8 limit"));
    }
}

#[test]
fn initial_length_counts_normalized_extended_graphemes() {
    // Combining accents and ZWJ emoji each occupy one editing position.
    let text = "e\u{301}👩‍💻";
    let parsed = parse(&form(
        "",
        &format!("<TextBox Text='{text}' MaxLength='2'/>"),
    ))
    .unwrap();
    assert_eq!(editor(&parsed).text, text);
    let error = parse(&form(
        "",
        &format!("<TextBox MaxLength='1' Text='{text}'/>"),
    ))
    .unwrap_err();
    assert!(error.message.contains("Text exceeds MaxLength"));
    assert_eq!((error.line, error.column), (2, 24));
    let normalized = "<TextBox MaxLength='1' Text='a&#13;&#10;&#9;&#x85;&#x2028;&#x2029;&#x301;'/>";
    assert!(parse(&form("", normalized)).is_ok());
    assert!(
        parse(&form(
            "",
            &format!("<TextBox Text='{text}' MaxLength='0'/>")
        ))
        .is_ok()
    );
}

#[test]
fn malformed_xml_and_both_empty_and_entity_dtds_are_rejected() {
    for source in [
        form("", "<TextBox>"),
        format!("<!DOCTYPE Window>{}", form("", "<TextBox/>")),
        format!(
            "<!DOCTYPE Window [<!ENTITY ext SYSTEM 'file:///secret'>]>{}",
            form("", "<TextBox Text='&ext;'/>")
        ),
        format!(
            "<!DOCTYPE Window [<!ENTITY text 'Hello'>]>{}",
            form("", "<TextBox Text='&text;'/>")
        ),
        form("", "<TextBox Text='&unknown;'/>"),
    ] {
        assert!(parse(&source).is_err(), "{source}");
    }
}

#[test]
fn diagnostic_points_to_invalid_attribute_with_one_based_location() {
    let source = format!("<Window xmlns='{UI_NAMESPACE}'>\n  <TextBox\n    Wdth='30'/>\n</Window>");
    let error = parse(&source).unwrap_err();
    assert_eq!((error.line, error.column), (3, 5));
    assert_eq!(
        error.to_string(),
        "3:5: unsupported TextBox property 'Wdth'"
    );
}

#[test]
fn limits_source_and_xml_node_count() {
    let source = " ".repeat(MAX_SOURCE_BYTES + 1);
    assert!(parse(&source).unwrap_err().message.contains("byte limit"));
    let nodes = format!(
        "{}<TextBox/>",
        "<!-- comment -->".repeat(MAX_XML_NODES as usize)
    );
    assert!(parse(&form("", &nodes)).is_err());
}

#[test]
fn generated_rust_escapes_strings_and_keeps_numbers_typed() {
    let source = form(
        "Title='&quot;); panic!(&quot;injection&quot;); //' Width='800' Theme='Dark'",
        r#"<TextBox x:Name='editor' Text='Привет &quot;\ &#10; Rust 🌍' Width='300' Margin='1,2,3,4'/>"#,
    );
    let generated = compile(&source).unwrap();
    assert!(generated.contains("title: \"\\\"); panic!(\\\"injection\\\"); //\".into(),"));
    assert!(generated.contains("text: \"Привет \\\"\\\\ \\n Rust 🌍\".into(),"));
    assert!(generated.contains("width: 800.0,"));
    assert!(generated.contains("width: Some(300.0),"));
    assert!(generated.contains("theme: rust_desktop_ui_xaml::Theme::Dark,"));
    assert!(generated.contains("margin: [1.0, 2.0, 3.0, 4.0],"));
    assert!(generated.contains("name: Some(\"editor\".into()),"));
}

#[test]
fn file_compilation_reports_input_path_and_preserves_output_on_validation_failure() {
    let directory = std::env::temp_dir().join(format!("rust-ui-xaml-test-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("Form.xaml");
    let output = directory.join("form.rs");
    fs::write(&input, form("", "<TextBox/>")).unwrap();
    compile_file(&input, &output).unwrap();
    let generated = fs::read_to_string(&output).unwrap();
    assert!(generated.starts_with("pub fn window_definition()"));
    fs::write(&input, form("", "<TextBox Nope='1'/>")).unwrap();
    let error = compile_file(&input, &output).unwrap_err();
    assert_eq!(error.path, input);
    assert!(error.diagnostic.is_some());
    assert!(error.to_string().contains("Form.xaml:2:10:"));
    assert_eq!(fs::read_to_string(&output).unwrap(), generated);
    fs::remove_file(input).unwrap();
    fs::remove_file(output).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn nested_tree_preserves_common_properties_content_and_child_order() {
    let source = form(
        "",
        "<StackPanel x:Name='layout' Spacing='12' Padding='1,2,3,4' Margin='5' IsEnabled='False' AutomationName='Form'>
            <TextBlock x:Name='heading' Text='Name &amp; title' Height='28'/>
            <TextBox x:Name='name' Text='Ada' Width='320' MaxLength='12' IsRequired='True'/>
            <StackPanel Orientation='Horizontal' Spacing='8' Width='400' Height='48'>
                <CheckBox x:Name='remember' Content='Remember me' IsChecked='True' Margin='4,6'/>
                <Button x:Name='save' Content='Save' Command='SaveForm' AutomationName='Save form'/>
            </StackPanel>
        </StackPanel>",
    );
    let parsed = parse(&source).unwrap();
    assert_eq!(
        parsed.content.common,
        CommonProperties {
            name: Some("layout".into()),
            margin: [5.0; 4],
            is_enabled: false,
            automation_name: Some("Form".into()),
            ..CommonProperties::default()
        }
    );
    let ElementKind::StackPanel(root) = parsed.content.kind else {
        panic!("expected root panel")
    };
    assert_eq!(root.orientation, Orientation::Vertical);
    assert_eq!(root.spacing, 12.0);
    assert_eq!(root.padding, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(root.children.len(), 3);
    assert_eq!(root.children[0].common.name.as_deref(), Some("heading"));
    assert_eq!(root.children[0].common.height, Some(28.0));
    assert_eq!(
        root.children[0].kind,
        ElementKind::TextBlock(TextBlockDefinition {
            text: "Name & title".into(),
            ..Default::default()
        })
    );
    assert_eq!(root.children[1].common.width, Some(320.0));
    assert_eq!(
        root.children[1].kind,
        ElementKind::TextBox(EditBoxDefinition {
            text: "Ada".into(),
            max_length: Some(12),
            is_required: true,
            ..EditBoxDefinition::default()
        })
    );
    // Enabled state is local in the AST; the runtime applies ancestor state.
    assert!(root.children[1].common.is_enabled);
    let ElementKind::StackPanel(row) = &root.children[2].kind else {
        panic!("expected row panel")
    };
    assert_eq!(row.orientation, Orientation::Horizontal);
    assert_eq!(row.spacing, 8.0);
    assert_eq!(root.children[2].common.width, Some(400.0));
    assert_eq!(root.children[2].common.height, Some(48.0));
    assert_eq!(row.children.len(), 2);
    assert_eq!(row.children[0].common.margin, [4.0, 6.0, 4.0, 6.0]);
    assert_eq!(
        row.children[0].kind,
        ElementKind::CheckBox(CheckBoxDefinition {
            content: "Remember me".into(),
            is_checked: true,
            ..Default::default()
        })
    );
    assert_eq!(
        row.children[1].kind,
        ElementKind::Button(ButtonDefinition {
            content: "Save".into(),
            command: Some("SaveForm".into()),
            ..Default::default()
        })
    );
    assert_eq!(
        row.children[1].common.automation_name.as_deref(),
        Some("Save form")
    );
}

#[test]
fn every_supported_element_can_be_the_single_root_with_default_common_properties() {
    for (tag, kind) in [
        (
            "StackPanel",
            ElementKind::StackPanel(StackPanelDefinition::default()),
        ),
        (
            "TextBox",
            ElementKind::TextBox(EditBoxDefinition::default()),
        ),
        (
            "TextBlock",
            ElementKind::TextBlock(TextBlockDefinition::default()),
        ),
        ("Button", ElementKind::Button(ButtonDefinition::default())),
        (
            "ToggleButton",
            ElementKind::ToggleButton(ToggleButtonDefinition::default()),
        ),
        (
            "CheckBox",
            ElementKind::CheckBox(CheckBoxDefinition::default()),
        ),
        (
            "RadioButton",
            ElementKind::RadioButton(RadioButtonDefinition::default()),
        ),
        (
            "Icon Symbol='Check'",
            ElementKind::Icon(IconDefinition::default()),
        ),
        (
            "Separator",
            ElementKind::Separator(SeparatorDefinition::default()),
        ),
    ] {
        let parsed = parse(&form("", &format!("<{tag}/>"))).unwrap();
        assert_eq!(
            parsed.content,
            ElementDefinition {
                common: CommonProperties::default(),
                kind
            }
        );
    }
}

#[test]
fn names_are_unique_across_the_entire_window_including_parent_and_namespace_aliases() {
    for tree in [
        "<StackPanel x:Name='same'><Button x:Name='same'/></StackPanel>",
        "<StackPanel><StackPanel><TextBox x:Name='same'/></StackPanel><CheckBox x:Name='same'/></StackPanel>",
        "<StackPanel><Button x:Name='same'/><TextBlock other:Name='s&#97;me'/></StackPanel>",
    ] {
        let error = parse(&form(&format!("xmlns:other='{XAML_NAMESPACE}'"), tree)).unwrap_err();
        assert!(error.message.contains("duplicate x:Name 'same'"), "{error}");
    }
    let error = parse(&form(
        "",
        "<StackPanel>\n  <Button x:Name='repeat'/>\n  <TextBox x:Name='repeat'/>\n</StackPanel>",
    ))
    .unwrap_err();
    assert_eq!((error.line, error.column), (4, 12));
    let source = form(
        "",
        "<StackPanel><Button x:Name='name'/><Button x:Name='Name'/><Button/><Button/></StackPanel>",
    );
    assert!(parse(&source).is_ok());
    assert!(parse(&source).is_ok()); // Namescope belongs to each parse, not the process.
}

#[test]
fn namespaced_tree_uses_namespace_uris_at_every_depth() {
    let source = format!(
        "<u:Window xmlns:u='{UI_NAMESPACE}' xmlns:n='{XAML_NAMESPACE}'>
            <u:StackPanel n:Name='root'><u:Button n:Name='action' Content='Go'/></u:StackPanel>
        </u:Window>"
    );
    assert!(parse(&source).is_ok());
    for child in [
        "<Button xmlns='urn:other'/>",
        "<Button xmlns=''/>",
        "<x:Button/>",
    ] {
        let error = parse(&form("", &format!("<StackPanel>{child}</StackPanel>"))).unwrap_err();
        assert!(error.message.contains("namespace"), "{error}");
    }
}

#[test]
fn tree_rejects_events_cross_control_properties_and_nonliteral_commands() {
    for content in [
        "<StackPanel Text='oops'/>",
        "<TextBlock Content='oops'/>",
        "<TextBlock Text='{Binding Name}'/>",
        "<Button Text='oops'/>",
        "<Button Click='on_click'/>",
        "<Button IsChecked='True'/>",
        "<CheckBox Command='Save'/>",
        "<CheckBox IsChecked='null'/>",
        "<StackPanel Orientation='horizontal'/>",
        "<StackPanel Orientation=' Vertical '/>",
        "<StackPanel><Window/></StackPanel>",
        "<StackPanel><StackPanel.Children/></StackPanel>",
    ] {
        assert!(parse(&form("", content)).is_err(), "{content}");
    }
    for command in [
        "",
        " Save",
        "Save ",
        "Save.Form",
        "2Save",
        "save-now",
        "Сохранить",
        "{Binding Save}",
        "Save()",
        "Save;quit",
    ] {
        assert!(
            parse(&form("", &format!("<Button Command='{command}'/>"))).is_err(),
            "{command}"
        );
    }
    for command in ["Save", "_save2", "a", "A_B_9"] {
        let parsed = parse(&form("", &format!("<Button Command='{command}'/>"))).unwrap();
        assert_eq!(
            parsed.content.kind,
            ElementKind::Button(ButtonDefinition {
                content: String::new(),
                command: Some(command.into()),
                ..Default::default()
            })
        );
    }
}

#[test]
fn all_leaf_kinds_reject_children_and_text_while_panels_reject_mixed_text() {
    for tag in [
        "TextBox",
        "TextBlock",
        "Button",
        "ToggleButton",
        "CheckBox",
        "RadioButton",
        "Separator",
    ] {
        for content in ["<TextBox/>", "text", "<![CDATA[text]]>"] {
            assert!(
                parse(&form("", &format!("<{tag}>{content}</{tag}>"))).is_err(),
                "{tag}: {content}"
            );
        }
        assert!(parse(&form("", &format!("<{tag}> \n<!-- allowed --></{tag}>"))).is_ok());
    }
    assert!(parse(&form("", "<StackPanel>text<Button/></StackPanel>")).is_err());
    assert!(
        parse(&form(
            "",
            "<StackPanel><StackPanel>text</StackPanel></StackPanel>"
        ))
        .is_err()
    );
}

#[test]
fn common_geometry_and_state_are_validated_for_every_element_kind() {
    for tag in [
        "StackPanel",
        "TextBox",
        "TextBlock",
        "Button",
        "ToggleButton",
        "CheckBox",
        "RadioButton",
        "Separator",
    ] {
        for attributes in [
            "Width='0'",
            "Height='NaN'",
            "Height='1e-300'",
            "Margin='1,2,3'",
            "IsEnabled='yes'",
            "x:Name='2bad'",
            "AutomationName='{Binding Label}'",
        ] {
            assert!(
                parse(&form("", &format!("<{tag} {attributes}/>"))).is_err(),
                "{tag}: {attributes}"
            );
        }
        let parsed = parse(&form("", &format!("<{tag} Width='10' Height='20' Margin='1,2' IsEnabled='false' AutomationName='Accessible'/>"))).unwrap();
        assert_eq!(
            parsed.content.common,
            CommonProperties {
                width: Some(10.0),
                height: Some(20.0),
                margin: [1.0, 2.0, 1.0, 2.0],
                is_enabled: false,
                automation_name: Some("Accessible".into()),
                ..CommonProperties::default()
            }
        );
    }
    for value in ["-1", "16385", "NaN", "inf", "", "1,2"] {
        assert!(
            parse(&form("", &format!("<StackPanel Spacing='{value}'/>"))).is_err(),
            "{value}"
        );
    }
    assert!(parse(&form("", "<StackPanel Spacing='0' Padding='16384'/>")).is_ok());
    assert!(parse(&form("", "<StackPanel Spacing='16384'/>")).is_ok());
}

#[test]
fn content_depth_and_count_limits_accept_boundaries_and_reject_one_more() {
    let nested = |panels: usize| {
        format!(
            "{}<TextBox/>{}",
            "<StackPanel>".repeat(panels),
            "</StackPanel>".repeat(panels)
        )
    };
    assert!(parse(&form("", &nested(MAX_ELEMENT_DEPTH - 1))).is_ok());
    let error = parse(&form("", &nested(MAX_ELEMENT_DEPTH))).unwrap_err();
    assert!(
        error.message.contains("maximum element depth of 32"),
        "{error}"
    );
    let wide =
        |children: usize| format!("<StackPanel>{}</StackPanel>", "<Button/>".repeat(children));
    assert!(parse(&form("", &wide(MAX_ELEMENTS - 1))).is_ok());
    let error = parse(&form("", &wide(MAX_ELEMENTS))).unwrap_err();
    assert!(error.message.contains("256-element limit"), "{error}");
    // Comments consume XML nodes, but not the content element count.
    assert!(
        parse(&form(
            "",
            &format!(
                "<StackPanel>{}<Button/></StackPanel>",
                "<!-- note -->".repeat(300)
            )
        ))
        .is_ok()
    );
}

#[test]
fn nested_editors_keep_normalization_length_and_validation_diagnostics() {
    let error = parse(&form(
        "",
        "<StackPanel>\n<TextBox MaxLength='1' Text='ab'/>\n</StackPanel>",
    ))
    .unwrap_err();
    assert_eq!((error.line, error.column), (3, 24));
    assert!(error.message.contains("Text exceeds MaxLength"));
    assert!(parse(&form("", "<StackPanel><TextBox Text='a&#13;&#10;&#x301;' MaxLength='1' IsRequired='True' ValidationMessage='Required' CommitOnLostFocus='False'/></StackPanel>")).is_ok());
}

#[test]
fn recursive_codegen_emits_all_variants_and_escapes_every_new_string_field() {
    let source = form(
        "",
        "<StackPanel x:Name='root' Orientation='Horizontal' Spacing='3.5' Padding='2,4'>
        <TextBlock Text='Title &quot;\\&#10;🌍' AutomationName='Label &quot;\\&#10;'/>
        <Button Content='Save &quot;); panic!(&quot;bad&quot;); //' Command='_Save2'/>
        <CheckBox Content='Agree &quot;\\&#10;' IsChecked='True' Height='32'/>
        <StackPanel><TextBox Text='Nested' IsReadOnly='True'/></StackPanel>
    </StackPanel>",
    );
    let generated = compile(&source).unwrap();
    assert_eq!(
        generated
            .matches("rust_desktop_ui_xaml::ElementDefinition {")
            .count(),
        6
    );
    for variant in ["StackPanel", "TextBox", "TextBlock", "Button", "CheckBox"] {
        assert!(
            generated.contains(&format!("rust_desktop_ui_xaml::ElementKind::{variant}(")),
            "{variant}"
        );
    }
    for expected in [
        "orientation: rust_desktop_ui_xaml::Orientation::Horizontal,",
        "spacing: 3.5,",
        "padding: [2.0, 4.0, 2.0, 4.0],",
        "text: \"Title \\\"\\\\\\n🌍\".into()",
        "automation_name: Some(\"Label \\\"\\\\\\n\".into()),",
        "content: \"Save \\\"); panic!(\\\"bad\\\"); //\".into()",
        "command: Some(\"_Save2\".into())",
        "content: \"Agree \\\"\\\\\\n\".into()",
        "is_checked: true",
        "height: Some(32.0),",
        "text: \"Nested\".into(),",
        "is_read_only: true,",
    ] {
        assert!(
            generated.contains(expected),
            "missing {expected}\n{generated}"
        );
    }
    let empty = compile(&form("", "<StackPanel/>")).unwrap();
    assert!(empty.contains("children: vec![],"));
}

#[test]
fn button_like_controls_share_optional_metrics_and_argb_brushes() {
    for tag in ["Button", "ToggleButton", "CheckBox", "RadioButton"] {
        let definition = parse(&form("", &format!("<{tag} Content='Action' FontSize='18.5' Padding='1,2,3,4' CornerRadius='6' BorderThickness='2' Background='#112233' Foreground='#80445566' BorderBrush='#00778899' FocusBrush='#ABCDEF'/>"))).unwrap();
        assert_eq!(
            button_style(&definition),
            &ButtonStyleDefinition {
                font_size: Some(18.5),
                padding: Some([1.0, 2.0, 3.0, 4.0]),
                corner_radius: Some(6.0),
                border_thickness: Some(2.0),
                background: Some([17, 34, 51, 255]),
                foreground: Some([68, 85, 102, 128]),
                border_brush: Some([119, 136, 153, 0]),
                focus_brush: Some([171, 205, 239, 255]),
            }
        );
        assert_eq!(
            button_style(&parse(&form("", &format!("<{tag}/>"))).unwrap()),
            &ButtonStyleDefinition::default()
        );
        assert_eq!(
            button_style(&parse(&form("", &format!("<{tag} Padding='12,5'/>"))).unwrap()).padding,
            Some([12.0, 5.0, 12.0, 5.0])
        );
    }
}

#[test]
fn button_style_reports_invalid_properties_without_accepting_state_markup() {
    for tag in ["Button", "ToggleButton", "CheckBox", "RadioButton"] {
        for attributes in [
            "FontSize='0'",
            "FontSize='513'",
            "FontSize='NaN'",
            "FontSize='1e-300'",
            "Padding='-1'",
            "Padding='1,2,3'",
            "Padding='16385'",
            "Padding='inf'",
            "CornerRadius='-1'",
            "CornerRadius='NaN'",
            "BorderThickness='1,2'",
            "BorderThickness='16385'",
            "Background='Red'",
            "Foreground='#123'",
            "BorderBrush='#GG112233'",
            "FocusBrush='#123456789'",
            "HoverBackground='#FFFFFF'",
            "CheckedBackground='#FFFFFF'",
            "CheckmarkBrush='#000000'",
            "SelectionBrush='#112233'",
            "IsThreeState='True'",
        ] {
            let error = parse(&form("", &format!("<{tag}\n {attributes}/>"))).unwrap_err();
            assert!(error.line > 0 && error.column > 0, "{tag} {attributes}");
            let property = attributes.split('=').next().unwrap();
            assert!(
                error.message.contains(property),
                "{tag} {attributes}: {error}"
            );
        }
        assert!(
            parse(&form(
                "",
                &format!(
                    "<{tag} FontSize='512' Padding='0' CornerRadius='16384' BorderThickness='0'/>"
                )
            ))
            .is_ok()
        );
    }
    assert!(parse(&form("", "<TextBlock CornerRadius='6'/>")).is_err());
    assert!(parse(&form("", "<StackPanel Background='#FFFFFF'/>")).is_err());
}

#[test]
fn toggle_command_and_checked_are_literals_and_codegen_preserves_all_style_fields() {
    let source = form(
        "",
        "<ToggleButton x:Name='Pin' Content='Pin &quot;\\&#10;🌍' Command='_Pin2' IsChecked='True' FontSize='19' Padding='7,3' CornerRadius='8' BorderThickness='2' Background='#101112' Foreground='#80131415' BorderBrush='#161718' FocusBrush='#191A1B'/>",
    );
    let definition = parse(&source).unwrap();
    let ElementKind::ToggleButton(toggle) = definition.content.kind else {
        panic!("expected ToggleButton");
    };
    assert!(toggle.is_checked);
    assert_eq!(toggle.command.as_deref(), Some("_Pin2"));
    assert_eq!(toggle.content, "Pin \"\\\n🌍");
    let generated = compile(&source).unwrap();
    for literal in [
        "ElementKind::ToggleButton",
        "ToggleButtonDefinition",
        "command: Some(\"_Pin2\".into())",
        "is_checked: true",
        "font_size: Some(19.0)",
        "padding: Some([7.0, 3.0, 7.0, 3.0])",
        "corner_radius: Some(8.0)",
        "border_thickness: Some(2.0)",
        "background: Some([16, 17, 18, 255])",
        "foreground: Some([19, 20, 21, 128])",
        "border_brush: Some([22, 23, 24, 255])",
        "focus_brush: Some([25, 26, 27, 255])",
    ] {
        assert!(generated.contains(literal), "missing {literal}");
    }
    assert!(generated.contains(&format!("content: {:?}.into()", toggle.content)));
    for attributes in [
        "Command='{Binding Pin}'",
        "Command='Pin.Now'",
        "IsChecked='null'",
        "IsChecked='{Binding State}'",
        "Click='Handler'",
        "Text='wrong'",
    ] {
        assert!(
            parse(&form("", &format!("<ToggleButton {attributes}/>"))).is_err(),
            "{attributes}"
        );
    }
}

#[test]
fn radio_group_names_and_selection_are_literals_with_typed_codegen() {
    let source = form(
        "",
        "<RadioButton x:Name='Choice' Content='Choose &quot;\\&#10;🌍' GroupName='_Alignment2' IsChecked='True' FontSize='19' Padding='7,3' Background='#80131415'/>",
    );
    let definition = parse(&source).unwrap();
    let ElementKind::RadioButton(radio) = definition.content.kind else {
        panic!("expected RadioButton");
    };
    assert_eq!(radio.group_name.as_deref(), Some("_Alignment2"));
    assert!(radio.is_checked);
    assert_eq!(radio.content, "Choose \"\\\n🌍");
    let generated = compile(&source).unwrap();
    for literal in [
        "ElementKind::RadioButton",
        "RadioButtonDefinition",
        "group_name: Some(\"_Alignment2\".into())",
        "is_checked: true",
        "font_size: Some(19.0)",
        "padding: Some([7.0, 3.0, 7.0, 3.0])",
        "background: Some([19, 20, 21, 128])",
    ] {
        assert!(generated.contains(literal), "missing {literal}");
    }
    assert!(generated.contains(&format!("content: {:?}.into()", radio.content)));
    assert!(
        compile(&form("", "<RadioButton/>"))
            .unwrap()
            .contains("group_name: None")
    );
    for attributes in [
        "GroupName=''",
        "GroupName='two words'",
        "GroupName='2invalid'",
        "GroupName='Русский'",
        "GroupName='{Binding Group}'",
        "GroupName='a.b'",
        "IsChecked='null'",
        "Command='Select'",
        "IsThreeState='True'",
    ] {
        let error = parse(&form("", &format!("<RadioButton\n {attributes}/>"))).unwrap_err();
        assert_eq!(error.line, 3);
        let expected = if attributes.contains("{Binding") {
            "markup extensions"
        } else {
            attributes.split('=').next().unwrap()
        };
        assert!(error.message.contains(expected), "{error}");
    }
    for tag in ["Button", "ToggleButton", "CheckBox", "StackPanel"] {
        assert!(parse(&form("", &format!("<{tag} GroupName='Group'/>"))).is_err());
    }
}

#[test]
fn radio_initial_selection_rejects_conflicts_per_named_or_immediate_parent_group() {
    for tree in [
        "<StackPanel><RadioButton IsChecked='True'/><RadioButton IsChecked='True'/></StackPanel>",
        "<StackPanel><RadioButton GroupName='G' IsChecked='True'/><StackPanel><RadioButton GroupName='G' IsChecked='True'/></StackPanel></StackPanel>",
        "<StackPanel><RadioButton GroupName='G' IsChecked='True'/><RadioButton GroupName='G' IsChecked='True' IsEnabled='False'/></StackPanel>",
    ] {
        let error = parse(&form("", tree)).unwrap_err();
        assert!(
            error.message.contains("at most one initial IsChecked=True"),
            "{error}"
        );
        assert!(error.line > 0 && error.column > 0);
        assert!(compile(&form("", tree)).is_err());
    }
    for tree in [
        "<StackPanel><RadioButton/><RadioButton/></StackPanel>",
        "<StackPanel><RadioButton IsChecked='True'/><StackPanel><RadioButton IsChecked='True'/></StackPanel></StackPanel>",
        "<StackPanel><RadioButton IsChecked='True'/><RadioButton GroupName='G' IsChecked='True'/><RadioButton GroupName='g' IsChecked='True'/></StackPanel>",
        "<StackPanel><RadioButton GroupName='G' IsChecked='True'/><StackPanel><RadioButton GroupName='G'/></StackPanel></StackPanel>",
    ] {
        assert!(parse(&form("", tree)).is_ok(), "{tree}");
        assert!(parse(&form("", tree)).is_ok()); // State belongs to this parse only.
    }
    let error = parse(&form("", "<StackPanel>\n<RadioButton IsChecked='True'/>\n<RadioButton IsChecked='True'/>\n</StackPanel>")).unwrap_err();
    assert_eq!((error.line, error.column), (4, 1));
}

#[test]
fn icon_symbols_and_separator_settings_parse_and_generate_typed_values() {
    for (name, symbol) in [
        ("Check", IconSymbol::Check),
        ("Close", IconSymbol::Close),
        ("Add", IconSymbol::Add),
        ("Remove", IconSymbol::Remove),
        ("ChevronDown", IconSymbol::ChevronDown),
        ("ChevronRight", IconSymbol::ChevronRight),
        ("Save", IconSymbol::Save),
        ("Pin", IconSymbol::Pin),
        ("FolderOpen", IconSymbol::FolderOpen),
        ("Copy", IconSymbol::Copy),
        ("Edit", IconSymbol::Edit),
        ("Eraser", IconSymbol::Eraser),
        ("Sun", IconSymbol::Sun),
        ("Moon", IconSymbol::Moon),
        ("Lock", IconSymbol::Lock),
    ] {
        let source = form(
            "",
            &format!(
                "<Icon Symbol='{name}' Foreground='#80112233' Width='48' Height='16' AutomationName='Icon &quot;\\&#10;🌍'/>"
            ),
        );
        let definition = parse(&source).unwrap();
        assert_eq!(
            definition.content.kind,
            ElementKind::Icon(IconDefinition {
                symbol,
                foreground: Some([17, 34, 51, 128])
            })
        );
        assert_eq!(definition.content.common.width, Some(48.0));
        assert_eq!(definition.content.common.height, Some(16.0));
        let generated = compile(&source).unwrap();
        assert!(generated.contains(&format!("symbol: rust_desktop_ui_xaml::IconSymbol::{name}")));
        assert!(generated.contains("foreground: Some([17, 34, 51, 128])"));
        assert!(generated.contains("automation_name: Some(\"Icon \\\"\\\\\\n🌍\".into())"));
    }
    let source = form(
        "",
        "<Separator Orientation='Vertical' Thickness='2.5' Foreground='#405060'/>",
    );
    assert_eq!(
        parse(&source).unwrap().content.kind,
        ElementKind::Separator(SeparatorDefinition {
            orientation: Orientation::Vertical,
            thickness: 2.5,
            foreground: Some([64, 80, 96, 255]),
        })
    );
    let generated = compile(&source).unwrap();
    assert!(generated.contains("orientation: rust_desktop_ui_xaml::Orientation::Vertical"));
    assert!(generated.contains("thickness: 2.5"));
    assert!(
        compile(&form("", "<Separator/>"))
            .unwrap()
            .contains("orientation: rust_desktop_ui_xaml::Orientation::Horizontal")
    );
}

#[test]
fn icon_button_metrics_require_an_icon_and_icon_only_actions_require_a_name() {
    for tag in ["Button", "ToggleButton"] {
        let source = form(
            "",
            &format!(
                "<{tag} Icon='Save' IconSize='24' IconSpacing='5' AutomationName='Save document'/>"
            ),
        );
        let parsed = parse(&source).unwrap();
        let (icon, size, spacing) = match parsed.content.kind {
            ElementKind::Button(button) => (button.icon, button.icon_size, button.icon_spacing),
            ElementKind::ToggleButton(button) => {
                (button.icon, button.icon_size, button.icon_spacing)
            }
            _ => unreachable!(),
        };
        assert_eq!((icon, size, spacing), (Some(IconSymbol::Save), 24.0, 5.0));
        let generated = compile(&source).unwrap();
        for literal in [
            "icon: Some(rust_desktop_ui_xaml::IconSymbol::Save)",
            "icon_size: 24.0",
            "icon_spacing: 5.0",
        ] {
            assert!(generated.contains(literal), "{literal}");
        }
        for attrs in [
            "IconSize='16'",
            "IconSpacing='8'",
            "Content='Caption' IconSize='20'",
        ] {
            assert!(
                parse(&form("", &format!("<{tag} {attrs}/>")))
                    .unwrap_err()
                    .message
                    .contains("require Icon")
            );
        }
        for attrs in [
            "Icon='Check'",
            "Icon='Check' AutomationName=''",
            "Icon='Check' AutomationName='   '",
            "Icon='Check' Content='  '",
        ] {
            assert!(
                parse(&form("", &format!("<{tag} {attrs}/>")))
                    .unwrap_err()
                    .message
                    .contains("AutomationName")
            );
        }
        assert!(
            parse(&form(
                "",
                &format!("<{tag} Icon='Check' Content='Accept'/>")
            ))
            .is_ok()
        );
        assert!(parse(&form("", &format!("<{tag} Icon='Check' IconSize='512' IconSpacing='16384' AutomationName='Action'/>"))).is_ok());
    }
}

#[test]
fn icon_separator_and_embedded_metrics_reject_unknown_or_invalid_markup() {
    for element in [
        "<Icon/>",
        "<Icon Symbol='Unknown'/>",
        "<Icon Symbol='check'/>",
        "<Icon Symbol='{Binding Icon}'/>",
        "<Icon Symbol='Check' Content='caption'/>",
        "<Icon Symbol='Check' Source='icon.svg'/>",
        "<Icon Symbol='Check' Foreground='Red'/>",
        "<Icon Symbol='Check'><Button/></Icon>",
        "<Icon Symbol='Check'>caption</Icon>",
        "<Icon Symbol='Check' Width='0'/>",
        "<Separator Orientation='Diagonal'/>",
        "<Separator Symbol='Check'/>",
        "<Separator Foreground='#00000'/>",
        "<CheckBox Icon='Check'/>",
        "<RadioButton Icon='Pin'/>",
        "<TextBlock IconSize='16'/>",
    ] {
        assert!(parse(&form("", element)).is_err(), "{element}");
    }
    for value in ["0", "-1", "NaN", "inf", "1e-300", "16385"] {
        assert!(
            parse(&form("", &format!("<Separator Thickness='{value}'/>"))).is_err(),
            "{value}"
        );
    }
    for tag in ["Button", "ToggleButton"] {
        for (attribute, values) in [
            ("IconSize", &["0", "-1", "NaN", "inf", "1e-300", "513"][..]),
            ("IconSpacing", &["-1", "NaN", "inf", "16385"][..]),
        ] {
            for value in values {
                let error = parse(&form(
                    "",
                    &format!("<{tag} Icon='Check' Content='Action'\n {attribute}='{value}'/>"),
                ))
                .unwrap_err();
                assert_eq!(error.line, 3);
                assert!(error.message.contains(attribute), "{error}");
            }
        }
    }
    assert!(parse(&form("", "<Separator Thickness='16384'/>")).is_ok());
}
