use super::*;

fn form(window_attributes: &str, text_box: &str) -> String {
    format!(
        "<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}' {window_attributes}>\n{text_box}\n</Window>"
    )
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
    assert_eq!(parsed.edit_box.name.as_deref(), Some("_title2"));
    assert_eq!(parsed.edit_box.text, "Привет 🌍 \"Rust\"");
    assert_eq!(parsed.edit_box.placeholder, "Имя");
    assert_eq!(parsed.edit_box.label, "Название");
    assert_eq!(parsed.edit_box.width, Some(320.0));
    assert_eq!(parsed.edit_box.height, 40.0);
    assert_eq!(parsed.edit_box.margin, [8.0, 12.0, 8.0, 12.0]);
    assert!(!parsed.edit_box.is_enabled);
}

#[test]
fn namespace_uris_determine_meaning_instead_of_prefix_spelling() {
    let source = format!(
        "<ui:Window xmlns:ui='{UI_NAMESPACE}' xmlns:lang='{XAML_NAMESPACE}'><ui:TextBox lang:Name='editor'/></ui:Window>"
    );
    assert_eq!(
        parse(&source).unwrap().edit_box.name.as_deref(),
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
fn requires_exactly_one_editor_and_no_nested_elements() {
    for child in [
        "",
        "<TextBox/><TextBox/>",
        "<Button/>",
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
        assert_eq!(parsed.edit_box.is_read_only, expected);
        assert_eq!(parsed.edit_box.max_length, Some(64));
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
        parse(&form("", "<TextBox MaxLength='1048576'/>"))
            .unwrap()
            .edit_box
            .max_length,
        Some(MAX_TEXT_LENGTH)
    );
    assert_eq!(
        parse(&form("", "<TextBox MaxLength='0'/>"))
            .unwrap()
            .edit_box
            .max_length,
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
    let style = parse(&source).unwrap().edit_box;
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
        parse(&form("", "<TextBox Padding='12,5'/>"))
            .unwrap()
            .edit_box
            .padding,
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
    let parsed = parse(&source).unwrap().edit_box;
    assert!(parsed.is_required);
    assert!(!parsed.commit_on_lost_focus);
    assert_eq!(parsed.validation_message, "Введите \"текст\" \n здесь");
    assert_eq!(parsed.validation_brush, Some([64, 32, 16, 128]));
    let generated = compile(&source).unwrap();
    assert!(generated.contains("is_required: true,"));
    assert!(generated.contains("commit_on_lost_focus: false,"));
    assert!(generated.contains("validation_message: \"Введите \\\"текст\\\" \\n здесь\".into(),"));
    assert!(generated.contains("validation_brush: Some([64, 32, 16, 128]),"));
    let defaults = parse(&form("", "<TextBox/>")).unwrap().edit_box;
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
        assert_eq!(parse(&source).unwrap().edit_box.validation_message, message);
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
    assert_eq!(parsed.edit_box.text, text);
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
    let nodes = format!("{}<TextBox/>", "<!-- comment -->".repeat(128));
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
