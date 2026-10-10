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
