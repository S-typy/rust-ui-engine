use crate::*;

fn window(content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>")
}

fn grid(content: &str) -> GridControlDefinition {
    let parsed = parse(&window(content)).unwrap();
    let ElementKind::GridControl(grid) = parsed.content.kind else {
        panic!("expected GridControl");
    };
    grid
}

#[test]
fn grid_control_named_source_and_auto_columns_have_explicit_defaults() {
    let parsed = parse(&window(
        "<GridControl x:Name='OrdersGrid' DataSource='Orders'/>",
    ))
    .unwrap();
    assert_eq!(parsed.content.common.name.as_deref(), Some("OrdersGrid"));
    assert!(parsed.content.children().is_empty());
    assert_eq!(
        parsed.content.kind,
        ElementKind::GridControl(GridControlDefinition {
            data_source: "Orders".into(),
            ..Default::default()
        })
    );
    assert!(
        grid("<GridControl DataSource='Orders'><GridControl.Columns/></GridControl>")
            .columns
            .is_empty()
    );
}

#[test]
fn grid_column_direct_and_property_syntax_preserve_presentation_and_codegen() {
    let columns = "<GridColumn FieldName='Name' Header='Название' Width='240' MinWidth='60' MaxWidth='900' IsPinned='True' IsReadOnly='True'/><GridColumn FieldName='Amount'/>";
    let direct = format!(
        "<GridControl DataSource='Orders' IsReadOnly='True' FilterText='Активные' SelectedValue='9223372036854775807'>{columns}</GridControl>"
    );
    let wrapped = direct.replace(
        columns,
        &format!("<GridControl.Columns>{columns}</GridControl.Columns>"),
    );
    assert_eq!(grid(&direct), grid(&wrapped));
    let definition = grid(&wrapped);
    assert!(definition.is_read_only);
    assert_eq!(definition.filter_text, "Активные");
    assert_eq!(
        definition.selected_value.as_deref(),
        Some("9223372036854775807")
    );
    assert_eq!(
        definition.columns[0],
        GridColumnDefinition {
            field: "Name".into(),
            header: "Название".into(),
            width: 240.0,
            min_width: 60.0,
            max_width: 900.0,
            is_pinned: true,
            is_read_only: true,
        }
    );
    assert_eq!(
        definition.columns[1],
        GridColumnDefinition {
            field: "Amount".into(),
            header: "Amount".into(),
            ..Default::default()
        }
    );
    let generated = compile(&window(&wrapped)).unwrap();
    for fragment in [
        "ElementKind::GridControl(",
        "GridControlDefinition { data_source: \"Orders\".into()",
        "GridColumnDefinition { field: \"Name\".into(), header: \"Название\".into()",
        "width: 240.0, min_width: 60.0, max_width: 900.0, is_pinned: true, is_read_only: true",
        "selected_value: Some(\"9223372036854775807\".into())",
        "filter_text: \"Активные\".into()",
    ] {
        assert!(generated.contains(fragment), "{fragment}");
    }
}

#[test]
fn grid_filter_and_selection_bindings_are_immediate_and_keep_string_keys() {
    let source = window(
        "<GridControl DataSource='Orders' FilterText='{Binding Query}' SelectedValue='{Binding SelectedKey}' IsReadOnly='{Binding Locked}'/>",
    );
    let parsed = parse(&source).unwrap();
    for property in ["FilterText", "SelectedValue"] {
        let binding = &parsed.content.common.bindings[property];
        assert_eq!(binding.mode, BindingMode::TwoWay);
        assert_eq!(
            binding.update_source_trigger,
            UpdateSourceTrigger::PropertyChanged
        );
    }
    assert_eq!(
        parsed.content.common.bindings["IsReadOnly"].mode,
        BindingMode::OneWay
    );
    assert!(
        compile(&source)
            .unwrap()
            .contains("(\"FilterText\".into(), rust_desktop_ui_xaml::BindingDefinition")
    );
    let ElementKind::GridControl(grid) = parsed.content.kind else {
        panic!()
    };
    assert_eq!(grid.selected_value, None);
    assert!(grid.filter_text.is_empty());
}

#[test]
fn grid_data_source_requires_bounded_registration_identifier_and_no_query_syntax() {
    for attributes in [
        "",
        "DataSource=''",
        "DataSource='orders.db'",
        "DataSource='C:/data/orders.db'",
        "DataSource='SELECT * FROM Orders'",
        "DataSource='Orders;DROP'",
        "DataSource='orders.Items'",
        "DataSource='{Binding Orders}'",
        "DataSource='Orders' Query='SELECT 1'",
        "DataSource='Orders' ConnectionString='file.db'",
        "DataSource='Orders' ItemsSource='Orders'",
        "DataSource='Orders' AutoGenerateColumns='True'",
    ] {
        assert!(
            parse(&window(&format!("<GridControl {attributes}/>"))).is_err(),
            "{attributes}"
        );
    }
    assert!(
        parse(&window(&format!(
            "<GridControl DataSource='{}'/>",
            "a".repeat(129)
        )))
        .is_err()
    );
    assert!(
        parse(&window(&format!(
            "<GridControl DataSource='{}'/>",
            "a".repeat(128)
        )))
        .is_ok()
    );
}

#[test]
fn grid_columns_reject_ambiguous_invalid_or_visual_children() {
    for body in [
        "<GridColumn/>",
        "<GridColumn FieldName='A'/><GridColumn FieldName='A'/>",
        "<GridColumn FieldName='Name' x:Name='Column'/>",
        "<GridColumn FieldName='{Binding Field}'/>",
        "<GridColumn FieldName='Name' Header='{Binding Caption}'/>",
        "<GridColumn FieldName='Name' IsEnabled='False'/>",
        "<GridColumn FieldName='Name'><TextBox/></GridColumn>",
        "<GridColumn FieldName='Name'>Caption</GridColumn>",
        "<TextBlock/>",
        "<GridControl.Columns/><GridColumn FieldName='A'/>",
        "<GridColumn FieldName='A'/><GridControl.Columns/>",
        "<GridControl.Columns/><GridControl.Columns/>",
        "<GridControl.Columns x:Name='Columns'/>",
        "<GridControl.Columns><GridColumn xmlns='wrong' FieldName='A'/></GridControl.Columns>",
        "<GridControl.Columns><GridControl.Columns/></GridControl.Columns>",
        "<GridControl.Columns>Caption</GridControl.Columns>",
    ] {
        assert!(
            parse(&window(&format!(
                "<GridControl DataSource='Orders'>{body}</GridControl>"
            )))
            .is_err(),
            "{body}"
        );
    }
    assert!(parse(&window("<GridColumn FieldName='Name'/>")).is_err());
    assert!(parse(&window("<StackPanel><GridControl.Columns/></StackPanel>")).is_err());
}

#[test]
fn grid_column_dimensions_reject_nonfinite_out_of_range_and_inconsistent_limits() {
    for attributes in [
        "Width='NaN'",
        "Width='inf'",
        "Width='0'",
        "Width='31'",
        "Width='4097'",
        "Width='Auto'",
        "MinWidth='-1'",
        "MinWidth='31'",
        "MinWidth='150'",
        "MaxWidth='100'",
        "MaxWidth='5000'",
        "MinWidth='200' MaxWidth='100'",
        "MaxWidth='NaN'",
    ] {
        assert!(parse(&window(&format!("<GridControl DataSource='Orders'><GridColumn FieldName='Name' {attributes}/></GridControl>"))).is_err(), "{attributes}");
    }
    for width in [32, 4096] {
        assert!(parse(&window(&format!("<GridControl DataSource='Orders'><GridColumn FieldName='Name' Width='{width}'/></GridControl>"))).is_ok());
    }
}

#[test]
fn grid_selection_is_exact_signed_i64_and_never_float() {
    for key in [
        "-9223372036854775808",
        "9223372036854775807",
        "9007199254740993",
        "-1",
        "0",
    ] {
        let parsed = grid(&format!(
            "<GridControl DataSource='Orders' SelectedValue='{key}'/>"
        ));
        assert_eq!(parsed.selected_value.as_deref(), Some(key));
    }
    for key in [
        "",
        " ",
        "1.0",
        "1e3",
        "NaN",
        "null",
        "+1",
        "--1",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        assert!(
            parse(&window(&format!(
                "<GridControl DataSource='Orders' SelectedValue='{key}'/>"
            )))
            .is_err(),
            "{key}"
        );
    }
}

#[test]
fn grid_text_limits_are_utf8_bytes_for_parser_and_manual_definitions() {
    for property in ["FilterText", "Header"] {
        for (value, accepted) in [("Я".repeat(2048), true), ("Я".repeat(2049), false)] {
            let content = if property == "FilterText" {
                format!("<GridControl DataSource='Orders' FilterText='{value}'/>")
            } else {
                format!(
                    "<GridControl DataSource='Orders'><GridColumn FieldName='Name' Header='{value}'/></GridControl>"
                )
            };
            assert_eq!(parse(&window(&content)).is_ok(), accepted, "{property}");
        }
    }
    let mut definition =
        grid("<GridControl DataSource='Orders'><GridColumn FieldName='Name'/></GridControl>");
    definition.filter_text = "a\0b".into();
    assert!(validate_grid_control(&definition).is_err());
    definition.filter_text.clear();
    definition.columns[0].header = "a\0b".into();
    assert!(validate_grid_control(&definition).is_err());
}

#[test]
fn grid_field_names_are_bounded_exact_schema_names_without_path_interpretation() {
    for field in ["Order total", "Наименование", "A.B", "A[0]"] {
        let definition = grid(&format!(
            "<GridControl DataSource='Orders'><GridColumn FieldName='{field}'/></GridControl>"
        ));
        assert_eq!(definition.columns[0].field, field);
        assert_eq!(definition.columns[0].header, field);
    }
    for (field, accepted) in [("Я".repeat(128), true), ("Я".repeat(129), false)] {
        assert_eq!(
            parse(&window(&format!(
                "<GridControl DataSource='Orders'><GridColumn FieldName='{field}'/></GridControl>"
            )))
            .is_ok(),
            accepted
        );
    }
    let mut definition =
        grid("<GridControl DataSource='Orders'><GridColumn FieldName='Name'/></GridControl>");
    definition.columns[0].field = "Name\0Other".into();
    assert!(validate_grid_control(&definition).is_err());
}

#[test]
fn manually_constructed_grids_validate_geometry_fields_and_binding_targets() {
    let base = parse(&window(
        "<GridControl DataSource='Orders'><GridColumn FieldName='Name'/></GridControl>",
    ))
    .unwrap();
    for mutate in [
        |grid: &mut GridControlDefinition| grid.data_source.clear(),
        |grid: &mut GridControlDefinition| grid.columns[0].field.clear(),
        |grid: &mut GridControlDefinition| grid.columns[0].width = f32::NAN,
        |grid: &mut GridControlDefinition| grid.columns[0].max_width = f32::INFINITY,
        |grid: &mut GridControlDefinition| grid.columns[0].min_width = 200.0,
        |grid: &mut GridControlDefinition| grid.columns.push(grid.columns[0].clone()),
        |grid: &mut GridControlDefinition| grid.selected_value = Some("1.5".into()),
    ] {
        let mut definition = base.clone();
        let ElementKind::GridControl(grid) = &mut definition.content.kind else {
            panic!()
        };
        mutate(grid);
        assert!(validate_grid_control(grid).is_err());
        assert!(validate_window_docking(&definition).is_err());
    }
    let mut definition = base;
    definition.content.common.bindings.insert(
        "DataSource".into(),
        BindingDefinition {
            path: "Orders".into(),
            ..Default::default()
        },
    );
    assert!(validate_window_docking(&definition).is_err());
}

fn columns(count: usize) -> String {
    (0..count)
        .map(|index| format!("<GridColumn FieldName='Field{index}'/>"))
        .collect()
}

#[test]
fn grid_columns_have_per_grid_and_whole_window_element_budgets() {
    for count in [0, 1, 64, 65] {
        let body = format!(
            "<GridControl DataSource='Orders'>{}</GridControl>",
            columns(count)
        );
        assert_eq!(parse(&window(&body)).is_ok(), count <= 64);
    }
    let body = format!(
        "<GridControl DataSource='Orders'>{}</GridControl>",
        columns(64)
    );
    let mut definition = parse(&window(&format!(
        "<StackPanel>{}</StackPanel>",
        body.repeat(3)
    )))
    .unwrap();
    let ElementKind::StackPanel(panel) = &mut definition.content.kind else {
        panic!()
    };
    panel.children.push(panel.children[0].clone());
    assert!(validate_window_docking(&definition).is_err());
    assert!(
        parse(&window(&format!(
            "<StackPanel>{}</StackPanel>",
            body.repeat(4)
        )))
        .is_err()
    );
    let mut definition = grid(&body);
    definition.columns.push(GridColumnDefinition {
        field: "Extra".into(),
        ..Default::default()
    });
    assert!(validate_grid_control(&definition).is_err());
}

#[test]
fn grid_metadata_obeys_depth_limits_in_xml_and_manual_trees() {
    let direct = "<GridControl DataSource='Orders'><GridColumn FieldName='Name'/></GridControl>";
    let wrapped = "<GridControl DataSource='Orders'><GridControl.Columns><GridColumn FieldName='Name'/></GridControl.Columns></GridControl>";
    for (body, count) in [(direct, 30), (wrapped, 29)] {
        let content = format!(
            "{}{}{}",
            "<StackPanel>".repeat(count),
            body,
            "</StackPanel>".repeat(count)
        );
        let mut definition = parse(&window(&content)).unwrap();
        let child = definition.content;
        definition.content = ElementDefinition {
            kind: ElementKind::StackPanel(StackPanelDefinition {
                children: vec![child],
                ..Default::default()
            }),
            ..Default::default()
        };
        if body == direct {
            assert!(validate_window_docking(&definition).is_err());
        }
        assert!(parse(&window(&format!("<StackPanel>{content}</StackPanel>"))).is_err());
    }
}

#[test]
fn grid_metadata_is_not_named_content_and_validation_reaches_inactive_pages_and_panes() {
    let source = window(
        "<TabbedGroup><GroupTab Header='First'><TextBlock x:Name='Text'/></GroupTab><GroupTab Header='Grid'><GridControl x:Name='OrdersGrid' DataSource='Orders'><GridColumn FieldName='Name'/></GridControl></GroupTab></TabbedGroup>",
    );
    let mut definition = parse(&source).unwrap();
    assert_eq!(definition.content.children()[1].children().len(), 0);
    let ElementKind::GridControl(grid) = &mut definition.content.children_mut()[1].kind else {
        panic!()
    };
    grid.columns[0].width = 1.0;
    assert!(validate_window_docking(&definition).is_err());
    let source = window(
        "<DockWorkspace><DockGroup><DockPane Id='Orders' Header='Orders'><GridControl x:Name='OrdersGrid' DataSource='Orders'/></DockPane></DockGroup></DockWorkspace>",
    );
    let mut definition = parse(&source).unwrap();
    let ElementKind::GridControl(grid) = &mut definition.content.children_mut()[0].kind else {
        panic!()
    };
    grid.data_source = "invalid.path".into();
    assert!(validate_window_docking(&definition).is_err());
    assert!(parse(&window("<StackPanel><GridControl DataSource='Orders' x:Name='Same'/><TextBlock x:Name='Same'/></StackPanel>")).is_err());
}
