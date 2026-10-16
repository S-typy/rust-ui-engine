use crate::*;

fn parse_content(content: &str) -> Result<WindowDefinition, Diagnostic> {
    parse(&format!(
        "<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>"
    ))
}

#[test]
fn binding_defaults_follow_editable_property_semantics() {
    for (content, property, mode, trigger) in [
        (
            "<TextBox Text='{Binding Customer.Name}'/>",
            "Text",
            BindingMode::TwoWay,
            UpdateSourceTrigger::LostFocus,
        ),
        (
            "<TextBlock Text='{Binding Customer.Name}'/>",
            "Text",
            BindingMode::OneWay,
            UpdateSourceTrigger::PropertyChanged,
        ),
        (
            "<CheckBox IsChecked='{Binding Enabled}'/>",
            "IsChecked",
            BindingMode::TwoWay,
            UpdateSourceTrigger::PropertyChanged,
        ),
        (
            "<ComboBox SelectedValue='{Binding Choice}'/>",
            "SelectedValue",
            BindingMode::TwoWay,
            UpdateSourceTrigger::PropertyChanged,
        ),
        (
            "<SpinBox Value='{Binding Amount}'/>",
            "Value",
            BindingMode::TwoWay,
            UpdateSourceTrigger::LostFocus,
        ),
    ] {
        let parsed = parse_content(content).unwrap();
        let binding = &parsed.content.common.bindings[property];
        assert_eq!(
            (binding.mode, binding.update_source_trigger),
            (mode, trigger),
            "{content}"
        );
    }
}

#[test]
fn binding_arguments_quoted_fallback_and_all_modes_round_trip_codegen() {
    for mode in ["OneWay", "TwoWay", "OneTime", "OneWayToSource"] {
        let source = format!(
            "<Window xmlns='{UI_NAMESPACE}'><TextBox Text=\"{{Binding Path=Value, ElementName=Source, Mode={mode}, Converter=ToText, FallbackValue='Unknown, name'}}\"/></Window>"
        );
        let parsed = parse(&source).unwrap();
        let binding = &parsed.content.common.bindings["Text"];
        assert_eq!(binding.element_name.as_deref(), Some("Source"));
        assert_eq!(binding.fallback_value.as_deref(), Some("Unknown, name"));
        let generated = compile(&source).unwrap();
        assert!(generated.contains(&format!("BindingMode::{mode}")));
        assert!(generated.contains("Unknown, name"));
    }
}

#[test]
fn malformed_bindings_are_never_silently_accepted() {
    for value in [
        "{Binding}",
        "{Binding }",
        "{BindingName}",
        "{Binding A..B}",
        "{Binding A, Path=B}",
        "{Binding A, Mode=TwoWay, Mode=OneWay}",
        "{Binding A, Unknown=True}",
        "{Binding Path=}",
        "{Binding A,}",
        "{Binding A, Mode=OneWay, UpdateSourceTrigger=Explicit}",
        "{Binding A, Converter=1Name}",
        "{Binding A, ElementName=A.B}",
        "{Binding A, FallbackValue={StaticResource X}}",
        "{Binding A, FallbackValue=&quot;unterminated}",
        "{Binding A, UpdateSourceTrigger=Immediate}",
        "{Binding A, Mode=Once}",
    ] {
        assert!(
            parse_content(&format!("<TextBox Text='{value}'/>")).is_err(),
            "{value}"
        );
    }
    assert!(parse_content("<Button Command='{Binding Save}'/>").is_err());
    assert!(parse_content("<TextBox FontSize='{Binding Font}'/>").is_err());
}

#[test]
fn data_context_and_binding_definitions_inherit_without_capturing_data() {
    let parsed = parse_content("<Grid DataContext='{Binding Customer}'><TextBox Text='{Binding Name, Mode=TwoWay, UpdateSourceTrigger=Explicit}'/></Grid>").unwrap();
    assert_eq!(
        parsed.content.common.data_context.as_deref(),
        Some("Customer")
    );
    let child = parsed.content.children()[0];
    assert!(child.common.data_context.is_none());
    assert_eq!(
        child.common.bindings["Text"].update_source_trigger,
        UpdateSourceTrigger::Explicit
    );
    for context in [
        "A..B",
        "{Binding A, Mode=TwoWay}",
        "{Binding A, ElementName=Other}",
    ] {
        assert!(parse_content(&format!("<Grid DataContext='{context}'/>")).is_err());
    }
}

#[test]
fn static_selection_preserves_values_labels_and_disabled_rows() {
    let parsed = parse_content("<ListBox SelectionMode='Multiple' SelectedValues='a,c'><ListBoxItem Value='a' Content='Alpha'/><ListBoxItem Value='b' Content='Beta' IsEnabled='False'/><ListBoxItem Value='c'/></ListBox>").unwrap();
    let ElementKind::ListBox(list) = parsed.content.kind else {
        panic!()
    };
    assert_eq!(list.selection_mode, SelectionMode::Multiple);
    assert_eq!(list.selected_values, ["a", "c"]);
    assert_eq!(list.items[2].display, "c");
    assert!(!list.items[1].is_enabled);
}

#[test]
fn selection_rejects_ambiguous_sources_duplicate_values_and_invalid_initial_selection() {
    for content in [
        "<ListBox ItemsSource='{Binding Items}'><ListBoxItem Value='a'/></ListBox>",
        "<ComboBox><ListBoxItem Value='a'/><ListBoxItem Value='a'/></ComboBox>",
        "<ListBox SelectedValue='a'><ListBoxItem Value='a' IsEnabled='False'/></ListBox>",
        "<ListBox SelectedValue='missing'><ListBoxItem Value='a'/></ListBox>",
        "<ComboBox SelectionMode='Multiple'/>",
        "<ListBox IsEditable='True'/>",
        "<ListBox SelectedValue='a' SelectedValues='b'/>",
        "<ListBox SelectedValues='a,b'/>",
        "<ListBox><ListBoxItem Value='a' Unknown='x'/></ListBox>",
        "<ListBox><Button/></ListBox>",
    ] {
        assert!(parse_content(content).is_err(), "{content}");
    }
}

#[test]
fn lookup_columns_and_dynamic_member_paths_compile() {
    let parsed = parse_content("<LookupEdit ItemsSource='{Binding Customers}' SelectedValue='{Binding CustomerId}' DisplayMemberPath='Name' ValueMemberPath='Id' IsEditable='True'><LookupEdit.Columns><LookupColumn Field='Name' Header='Customer' Width='200'/><LookupColumn Field='Address.City' Header='City'/></LookupEdit.Columns></LookupEdit>").unwrap();
    let ElementKind::LookupEdit(lookup) = parsed.content.kind else {
        panic!()
    };
    assert_eq!(lookup.columns[1].field, "Address.City");
    assert_eq!(lookup.selection.value_member_path.as_deref(), Some("Id"));
    for content in [
        "<LookupEdit><LookupEdit.Columns><LookupColumn/></LookupEdit.Columns></LookupEdit>",
        "<LookupEdit><LookupEdit.Columns/><LookupEdit.Columns/></LookupEdit>",
        "<ComboBox DisplayMemberPath='Name[-1]'/>",
        "<LookupEdit DisplayMemberPath='Name'><LookupEdit.Columns><LookupColumn Field='Id'/></LookupEdit.Columns></LookupEdit>",
    ] {
        assert!(parse_content(content).is_err(), "{content}");
    }
}

#[test]
fn embedded_buttons_require_registered_command_names_and_accessible_labels() {
    let parsed = parse_content("<ButtonEdit Text='{Binding Path}' IsReadOnly='True'><ButtonEdit.Buttons><EditorButton Command='Browse' Icon='Add' Content='Browse' ToolTip='Browse'/><EditorButton Command='Clear' Content='Clear'/></ButtonEdit.Buttons></ButtonEdit>").unwrap();
    let ElementKind::ButtonEdit(editor) = parsed.content.kind else {
        panic!()
    };
    assert_eq!(editor.buttons.len(), 2);
    assert!(editor.editor.is_read_only);
    for content in [
        "<ButtonEdit/>",
        "<ButtonEdit><ButtonEdit.Buttons><EditorButton Content='X'/></ButtonEdit.Buttons></ButtonEdit>",
        "<ButtonEdit><ButtonEdit.Buttons><EditorButton Command='Clear'/></ButtonEdit.Buttons></ButtonEdit>",
        "<ButtonEdit><ButtonEdit.Buttons><EditorButton Command='{Binding Clear}' Content='Clear'/></ButtonEdit.Buttons></ButtonEdit>",
    ] {
        assert!(parse_content(content).is_err(), "{content}");
    }
}

#[test]
fn spinbox_finite_ranges_and_positive_increment_are_checked_before_mount() {
    assert!(
        parse_content("<SpinBox Value='3.5' Minimum='-10' Maximum='20' Increment='0.25'/>").is_ok()
    );
    assert!(parse_content("<SpinBox Value='{Binding Amount}' Minimum='10' Maximum='20'/>").is_ok());
    for attributes in [
        "Value='NaN'",
        "Value='inf'",
        "Minimum='4' Maximum='2'",
        "Value='2' Maximum='1'",
        "Increment='0'",
        "Increment='-1'",
        "Text='1'",
    ] {
        assert!(
            parse_content(&format!("<SpinBox {attributes}/>")).is_err(),
            "{attributes}"
        );
    }
}

#[test]
fn temporal_editors_validate_calendar_dates_seconds_and_nullable_initial_values() {
    assert!(parse_content("<DateEdit/>").is_ok());
    assert!(parse_content("<TimeEdit Value='12:30:01' Minimum='12:00:01' TimeStep='1'/>").is_ok());
    assert!(parse_content("<DateEdit Value='2024-02-29' Minimum='1900-01-01' Maximum='2100-12-31' Format='yyyy-MM-dd'/>").is_ok());
    assert!(
        parse_content(
            "<TimeEdit Value='09:30:59' Minimum='09:30' Maximum='10:00' Format='HH:mm:ss'/>"
        )
        .is_ok()
    );
    for content in [
        "<DateEdit Value='1900-02-29'/>",
        "<DateEdit Value='2023-02-29'/>",
        "<DateEdit Value='0000-01-01'/>",
        "<DateEdit Value='2024-04-31'/>",
        "<DateEdit Value='31.12.2026'/>",
        "<DateEdit Value='ééééé'/>",
        "<DateEdit Minimum='2025-01-01' Maximum='2024-01-01'/>",
        "<TimeEdit Value='24:00'/>",
        "<TimeEdit Value='12:60'/>",
        "<TimeEdit Value='12:30:60'/>",
        "<TimeEdit TimeStep='0'/>",
        "<TimeEdit TimeStep='86401'/>",
        "<TimeEdit Value='09:30' Minimum='10:00'/>",
        "<TimeEdit Format='hh:mm tt'/>",
    ] {
        assert!(parse_content(content).is_err(), "{content}");
    }
}

#[test]
fn grid_tracks_placement_alignment_and_constraints_are_typed() {
    let parsed = parse_content("<Grid Padding='8' Spacing='4'><Grid.RowDefinitions><RowDefinition Height='Auto'/><RowDefinition Height='2*'/></Grid.RowDefinitions><Grid.ColumnDefinitions><ColumnDefinition Width='120'/><ColumnDefinition Width='*'/></Grid.ColumnDefinitions><TextBox Grid.Row='1' Grid.ColumnSpan='2' Width='Auto' MinWidth='40' MaxWidth='300' HorizontalAlignment='Center' VerticalAlignment='Bottom'/></Grid>").unwrap();
    let ElementKind::Grid(grid) = parsed.content.kind else {
        panic!()
    };
    assert_eq!(grid.rows, [GridLength::Auto, GridLength::Star(2.0)]);
    assert_eq!(
        grid.columns,
        [GridLength::Pixel(120.0), GridLength::Star(1.0)]
    );
    let common = &grid.children[0].common;
    assert_eq!((common.grid_row, common.grid_column_span), (1, 2));
    assert_eq!(common.horizontal_alignment, Alignment::Center);
    assert_eq!(common.vertical_alignment, Alignment::End);
    assert_eq!(common.width, None);
}

#[test]
fn layout_rejects_invalid_tracks_spans_constraints_and_property_ownership() {
    for content in [
        "<Grid><Grid.RowDefinitions><RowDefinition Height='0*'/></Grid.RowDefinitions></Grid>",
        "<Grid><Grid.RowDefinitions><RowDefinition Height='NaN*'/></Grid.RowDefinitions></Grid>",
        "<Grid><Grid.RowDefinitions/><Grid.RowDefinitions/></Grid>",
        "<Grid><Grid.RowDefinitions><ColumnDefinition/></Grid.RowDefinitions></Grid>",
        "<TextBox Grid.Row='-1'/>",
        "<TextBox Grid.Column='1024'/>",
        "<TextBox Grid.Column='1023' Grid.ColumnSpan='2'/>",
        "<TextBox Grid.RowSpan='0'/>",
        "<TextBox MinWidth='30' MaxWidth='20'/>",
        "<TextBox HorizontalAlignment='Top'/>",
        "<Grid><ButtonEdit.Buttons/></Grid>",
        "<Border/>",
        "<ScrollViewer><Button/><Button/></ScrollViewer>",
    ] {
        assert!(parse_content(content).is_err(), "{content}");
    }
}

#[test]
fn all_new_containers_preserve_traversal_and_old_ribbon_docking_limits() {
    let parsed = parse_content("<DockPanel LastChildFill='True'><Border DockPanel.Dock='Top' Padding='4' Background='#FF112233' CornerRadius='8'><WrapPanel><Button/><Button/></WrapPanel></Border><ScrollViewer HorizontalScrollBarVisibility='Disabled'><TextBox/></ScrollViewer></DockPanel>").unwrap();
    assert_eq!(parsed.content.children().len(), 2);
    assert_eq!(
        parsed.content.children()[0].children()[0].children().len(),
        2
    );
    let ribbon = "<Ribbon><RibbonTab Header='Home' Id='Home'><RibbonGroup Header='Group' Id='Group'><Button Command='Run'/></RibbonGroup></RibbonTab></Ribbon>";
    assert!(parse_content(&format!("<Grid>{ribbon}<Border>{ribbon}</Border></Grid>")).is_err());
    assert!(parse_content(&format!("<DockWorkspace><DockGroup><DockPane Id='Pane' Header='Pane'><Border>{ribbon}</Border></DockPane></DockGroup></DockWorkspace>")).is_err());
}

#[test]
fn ribbon_title_bar_does_not_accept_new_geometry_that_offsets_native_chrome() {
    for properties in [
        "HorizontalAlignment='Center'",
        "VerticalAlignment='Bottom'",
        "MinWidth='900'",
        "MaxHeight='40'",
        "Width='{Binding Width}'",
    ] {
        let source = format!(
            "<Window xmlns='{UI_NAMESPACE}' TitleBar='Ribbon' Padding='0'><StackPanel {properties}><Ribbon><RibbonTab Id='Home' Header='Home'><RibbonGroup Id='Group' Header='Group'><Button Command='Run'/></RibbonGroup></RibbonTab></Ribbon></StackPanel></Window>"
        );
        assert!(parse(&source).is_err(), "{properties}");
    }
}

#[test]
fn binding_paths_accept_bounded_array_index_syntax() {
    for path in ["Rows[0].Name", "Matrix[0][1].Value", "Form.Items[12]"] {
        let parsed = parse_content(&format!("<TextBox Text='{{Binding {path}}}'/>")).unwrap();
        assert_eq!(parsed.content.common.bindings["Text"].path, path);
    }
    for path in [
        "Rows[-1].Name",
        "Rows[].Name",
        "Rows[1]Name",
        "Rows[one]",
        "[0].Name",
    ] {
        assert!(
            parse_content(&format!("<TextBox Text='{{Binding {path}}}'/>")).is_err(),
            "{path}"
        );
    }
}

#[test]
fn generated_form_definitions_preserve_all_new_variants_and_strings() {
    let source = format!(
        "<Window xmlns='{UI_NAMESPACE}'><Grid DataContext='Form'><ListBox/><ComboBox/><LookupEdit/><ButtonEdit><ButtonEdit.Buttons><EditorButton Command='Clear' Content='Clear &quot;quotes&quot;'/></ButtonEdit.Buttons></ButtonEdit><SpinBox/><DateEdit/><TimeEdit/><Border><TextBox Text='{{Binding Name}}'/></Border><ScrollViewer><DockPanel><WrapPanel/></DockPanel></ScrollViewer></Grid></Window>"
    );
    let generated = compile(&source).unwrap();
    for variant in [
        "Grid",
        "ListBox",
        "ComboBox",
        "LookupEdit",
        "ButtonEdit",
        "SpinBox",
        "DateEdit",
        "TimeEdit",
        "Border",
        "ScrollViewer",
        "DockPanel",
        "WrapPanel",
    ] {
        assert!(
            generated.contains(&format!("ElementKind::{variant}(")),
            "{variant}"
        );
    }
    assert!(generated.contains("Clear \\\"quotes\\\""));
    assert!(generated.contains("BindingDefinition"));
}
