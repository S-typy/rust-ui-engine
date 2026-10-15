use crate::*;

fn window(content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>")
}

#[test]
fn status_bars_preserve_section_order_placement_and_content() {
    for tag in ["StatusBar", "RibbonStatusBar"] {
        let source = window(&format!(
            "<{tag} x:Name='Bar'><StatusBarItem x:Name='Message' Width='240'><TextBlock Text='Готово'/></StatusBarItem><StatusBarItem Placement='Right'><ZoomControl/></StatusBarItem><StatusBarItem Placement='Left'/></{tag}>"
        ));
        let mut parsed = parse(&source).unwrap();
        let items = parsed.content.children();
        assert_eq!(items.len(), 3);
        let ElementKind::StatusBarItem(first) = &items[0].kind else {
            panic!()
        };
        assert_eq!(first.placement, StatusBarPlacement::Left);
        assert_eq!(items[0].common.width, Some(240.0));
        assert!(matches!(
            items[0].children()[0].kind,
            ElementKind::TextBlock(_)
        ));
        let ElementKind::StatusBarItem(second) = &items[1].kind else {
            panic!()
        };
        assert_eq!(second.placement, StatusBarPlacement::Right);
        assert!(items[2].children().is_empty());
        parsed.content.children_mut()[0].children_mut()[0]
            .common
            .is_enabled = false;
        assert!(!parsed.content.children()[0].children()[0].common.is_enabled);
        let generated = compile(&source).unwrap();
        assert!(generated.contains(&format!("ElementKind::{tag}(")));
        assert!(generated.contains("StatusBarPlacement::Right"));
        assert!(generated.contains("child: None"));
        assert!(generated.contains("Готово"));
        assert!(generated.contains(
            "ZoomControlDefinition { minimum: 25.0, maximum: 400.0, value: 100.0, step: 10.0 }"
        ));
    }
}

#[test]
fn empty_status_bars_and_standalone_numeric_controls_are_supported() {
    for tag in ["StatusBar", "RibbonStatusBar", "ProgressBar", "ZoomControl"] {
        let parsed = parse(&window(&format!("<{tag}/>"))).unwrap();
        assert!(parsed.content.children().is_empty());
        validate_status_bars(&parsed.content).unwrap();
    }
    let parsed = parse(&window("<ProgressBar/>")).unwrap();
    assert_eq!(
        parsed.content.kind,
        ElementKind::ProgressBar(ProgressBarDefinition::default())
    );
}

#[test]
fn status_bar_structure_rejects_invalid_placement_nesting_and_children() {
    for content in [
        "<StatusBarItem/>",
        "<StackPanel><StatusBarItem/></StackPanel>",
        "<StatusBar><TextBlock/></StatusBar>",
        "<RibbonStatusBar><ProgressBar/></RibbonStatusBar>",
        "<StatusBar><StatusBarItem Placement='Center'/></StatusBar>",
        "<StatusBar><StatusBarItem><TextBlock/><TextBlock/></StatusBarItem></StatusBar>",
        "<StatusBar><StatusBarItem><StatusBar/></StatusBarItem></StatusBar>",
        "<RibbonStatusBar><StatusBarItem><GroupBox><StatusBar/></GroupBox></StatusBarItem></RibbonStatusBar>",
        "<StatusBar><StatusBarItem><StatusBarItem/></StatusBarItem></StatusBar>",
        "<ProgressBar><TextBlock/></ProgressBar>",
        "<ZoomControl><TextBlock/></ZoomControl>",
        "<StatusBar Padding='5'/>",
        "<StatusBar><StatusBarItem Placement='{Binding Side}'/></StatusBar>",
    ] {
        assert!(parse(&window(content)).is_err(), "{content}");
    }
}

#[test]
fn progress_and_zoom_validate_finite_ranges_values_and_steps() {
    for tag in ["ProgressBar", "ZoomControl"] {
        for properties in [
            "Minimum='100' Maximum='100'",
            "Minimum='200' Maximum='100'",
            "Value='-1'",
            "Value='500'",
            "Value='NaN'",
            "Minimum='-inf'",
            "Maximum='inf'",
            "Minimum='-1e308' Maximum='1e308' Value='0'",
        ] {
            let content = format!("<{tag} {properties}/>");
            assert!(parse(&window(&content)).is_err(), "{content}");
        }
    }
    for step in ["0", "-1", "NaN", "inf"] {
        assert!(parse(&window(&format!("<ZoomControl Step='{step}'/>"))).is_err());
    }
    let source = window(
        "<StackPanel><ProgressBar Minimum='-5' Maximum='5' Value='-5'/><ZoomControl Minimum='10' Maximum='500' Value='500' Step='12.5'/></StackPanel>",
    );
    let generated = compile(&source).unwrap();
    assert!(
        generated.contains("ProgressBarDefinition { minimum: -5.0, maximum: 5.0, value: -5.0 }")
    );
    assert!(generated.contains(
        "ZoomControlDefinition { minimum: 10.0, maximum: 500.0, value: 500.0, step: 12.5 }"
    ));
}

#[test]
fn indicator_binding_modes_defaults_and_bounded_initial_placeholders() {
    let parsed = parse(&window("<StackPanel><ProgressBar Minimum='50' Maximum='90' Value='{Binding Progress}'/><ZoomControl Minimum='200' Maximum='300' Value='{Binding Zoom}'/></StackPanel>")).unwrap();
    let children = parsed.content.children();
    let progress_binding = &children[0].common.bindings["Value"];
    assert_eq!(progress_binding.mode, BindingMode::OneWay);
    assert_eq!(
        progress_binding.update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    let zoom_binding = &children[1].common.bindings["Value"];
    assert_eq!(zoom_binding.mode, BindingMode::TwoWay);
    assert_eq!(
        zoom_binding.update_source_trigger,
        UpdateSourceTrigger::PropertyChanged
    );
    let ElementKind::ProgressBar(progress) = children[0].kind else {
        panic!()
    };
    assert_eq!(progress.value, 50.0);
    let ElementKind::ZoomControl(zoom) = children[1].kind else {
        panic!()
    };
    assert_eq!(zoom.value, 200.0);
    for mode in ["TwoWay", "OneWayToSource"] {
        assert!(
            parse(&window(&format!(
                "<ProgressBar Value='{{Binding Progress, Mode={mode}}}'/>"
            )))
            .is_err()
        );
    }
    assert!(
        parse(&window(
            "<ProgressBar Value='{Binding Progress, Mode=OneTime}'/>"
        ))
        .is_ok()
    );
    assert!(
        parse(&window(
            "<ZoomControl Value='{Binding Zoom, Mode=TwoWay, UpdateSourceTrigger=Explicit}'/>"
        ))
        .is_ok()
    );
    assert!(parse(&window("<ZoomControl Minimum='{Binding Minimum}'/>")).is_err());
}

#[test]
fn status_children_participate_in_shared_names_and_radio_scope() {
    assert!(parse(&window("<StatusBar><StatusBarItem x:Name='Repeated'><TextBlock x:Name='Repeated'/></StatusBarItem></StatusBar>")).is_err());
    // Radio choices within two different items have different implicit owners.
    let independent = "<StatusBar><StatusBarItem><RadioButton IsChecked='True'/></StatusBarItem><StatusBarItem><RadioButton IsChecked='True'/></StatusBarItem></StatusBar>";
    assert!(parse(&window(independent)).is_ok());
    let shared = independent.replace("IsChecked='True'", "IsChecked='True' GroupName='Shared'");
    assert!(parse(&window(&shared)).is_err());
}

#[test]
fn status_item_wrappers_count_toward_element_and_depth_limits() {
    let maximum = format!(
        "<StatusBar>{}</StatusBar>",
        "<StatusBarItem><TextBlock/></StatusBarItem>".repeat((MAX_ELEMENTS - 1) / 2)
    );
    assert!(parse(&window(&maximum)).is_ok());
    let excessive = maximum.replace(
        "</StatusBar>",
        "<StatusBarItem><TextBlock/></StatusBarItem></StatusBar>",
    );
    assert!(parse(&window(&excessive)).is_err());
    let at_limit = format!(
        "<StatusBar><StatusBarItem>{}<TextBlock/>{}</StatusBarItem></StatusBar>",
        "<GroupBox>".repeat(MAX_ELEMENT_DEPTH - 3),
        "</GroupBox>".repeat(MAX_ELEMENT_DEPTH - 3)
    );
    assert!(parse(&window(&at_limit)).is_ok());
    let excessive = at_limit.replace("<TextBlock/>", "<GroupBox><TextBlock/></GroupBox>");
    assert!(parse(&window(&excessive)).is_err());
}

#[test]
fn manual_status_ast_validation_checks_all_pages_and_docking_panes() {
    let mut parsed = parse(&window("<TabbedGroup><GroupTab Header='One'><StatusBar><StatusBarItem><ProgressBar/></StatusBarItem></StatusBar></GroupTab><GroupTab Header='Two'><ZoomControl/></GroupTab></TabbedGroup>")).unwrap();
    let ElementKind::ZoomControl(zoom) = &mut parsed.content.children_mut()[1].kind else {
        panic!()
    };
    zoom.step = f64::NAN;
    assert!(validate_status_bars(&parsed.content).is_err());
    assert!(validate_window_docking(&parsed).is_err());
    let mut parsed = parse(&window("<DockWorkspace><DockGroup><DockPane Id='One'><StatusBar><StatusBarItem><ProgressBar/></StatusBarItem></StatusBar></DockPane></DockGroup></DockWorkspace>")).unwrap();
    {
        let mut panes = parsed.content.children_mut();
        let mut items = panes[0].children_mut();
        let mut contents = items[0].children_mut();
        let ElementKind::ProgressBar(progress) = &mut contents[0].kind else {
            panic!()
        };
        progress.value = f64::INFINITY;
    }
    assert!(validate_status_bars(&parsed.content).is_err());
    assert!(validate_window_docking(&parsed).is_err());
    let ElementKind::DockWorkspace(workspace) = &parsed.content.kind else {
        panic!()
    };
    assert!(validate_docking(workspace).is_err());
}

#[test]
fn manual_status_ast_rejects_items_outside_bar_and_writing_progress_bindings() {
    let item = ElementDefinition {
        kind: ElementKind::StatusBarItem(StatusBarItemDefinition::default()),
        ..Default::default()
    };
    assert!(validate_status_bars(&item).is_err());
    let mut parsed = parse(&window("<ProgressBar Value='{Binding Progress}'/>")).unwrap();
    parsed
        .content
        .common
        .bindings
        .get_mut("Value")
        .unwrap()
        .mode = BindingMode::TwoWay;
    assert!(validate_status_bars(&parsed.content).is_err());
    let mut parsed = parse(&window("<StatusBar><StatusBarItem/></StatusBar>")).unwrap();
    parsed.content.children_mut()[0].kind = ElementKind::TextBlock(TextBlockDefinition::default());
    assert!(validate_status_bars(&parsed.content).is_err());
}

fn chrome_grid(
    grid_properties: &str,
    rows: &str,
    columns: &str,
    ribbon_properties: &str,
    later: &str,
) -> String {
    format!(
        "<Window xmlns='{UI_NAMESPACE}' TitleBar='Ribbon' Padding='0'><Grid {grid_properties}><Grid.RowDefinitions>{rows}</Grid.RowDefinitions>{columns}<Ribbon {ribbon_properties}><RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab></Ribbon>{later}</Grid></Window>"
    )
}

#[test]
fn ribbon_chrome_grid_reserves_auto_first_row_and_allows_bottom_status_bar() {
    let source = chrome_grid(
        "",
        "<RowDefinition Height='Auto'/><RowDefinition Height='*'/><RowDefinition Height='Auto'/>",
        "",
        "",
        "<TextBox Grid.Row='1'/><RibbonStatusBar Grid.Row='2'><StatusBarItem><TextBlock Text='Ready'/></StatusBarItem></RibbonStatusBar>",
    );
    let parsed = parse(&source).unwrap();
    validate_window_chrome(&parsed).unwrap();
    assert!(
        compile(&source)
            .unwrap()
            .contains("ElementKind::RibbonStatusBar(")
    );
    let explicit_column = source.replace(
        "<Grid>",
        "<Grid><Grid.ColumnDefinitions><ColumnDefinition Width='2*'/></Grid.ColumnDefinitions>",
    );
    assert!(parse(&explicit_column).is_ok());
}

#[test]
fn ribbon_chrome_grid_rejects_caption_row_overlap_and_nonflush_layout() {
    let rows = "<RowDefinition Height='Auto'/><RowDefinition Height='*'/>";
    let later = "<TextBox Grid.Row='1'/>";
    for props in [
        "Padding='1'",
        "Spacing='1'",
        "Margin='1'",
        "Width='600'",
        "HorizontalAlignment='Left'",
        "Visibility='Hidden'",
    ] {
        assert!(
            parse(&chrome_grid(props, rows, "", "", later)).is_err(),
            "{props}"
        );
    }
    for ribbon_props in [
        "Grid.Row='1'",
        "Grid.Column='1'",
        "Grid.RowSpan='2'",
        "Grid.ColumnSpan='2'",
        "Margin='1'",
    ] {
        assert!(
            parse(&chrome_grid("", rows, "", ribbon_props, later)).is_err(),
            "{ribbon_props}"
        );
    }
    assert!(parse(&chrome_grid("", rows, "", "", "<TextBox/>")).is_err());
    for props in ["Grid.Column='1'", "Grid.ColumnSpan='2'"] {
        assert!(
            parse(&chrome_grid(
                "",
                rows,
                "",
                "",
                &format!("<TextBox Grid.Row='1' {props}/>")
            ))
            .is_err(),
            "{props}"
        );
    }
    assert!(parse(&chrome_grid("", "<RowDefinition Height='*'/>", "", "", "")).is_err());
    for column in [
        "<ColumnDefinition Width='Auto'/>",
        "<ColumnDefinition Width='*'/><ColumnDefinition Width='*'/>",
    ] {
        assert!(
            parse(&chrome_grid(
                "",
                rows,
                &format!("<Grid.ColumnDefinitions>{column}</Grid.ColumnDefinitions>"),
                "",
                later
            ))
            .is_err()
        );
    }
}

#[test]
fn manual_chrome_grid_rejects_implicit_columns_created_below_caption() {
    let source = chrome_grid(
        "",
        "<RowDefinition Height='Auto'/><RowDefinition Height='*'/>",
        "",
        "",
        "<TextBox Grid.Row='1'/>",
    );
    let initial = parse(&source).unwrap();
    for column_span in [false, true] {
        let mut changed = initial.clone();
        let ElementKind::Grid(grid) = &mut changed.content.kind else {
            panic!()
        };
        if column_span {
            grid.children[1].common.grid_column_span = 2;
        } else {
            grid.children[1].common.grid_column = 1;
        }
        assert!(validate_window_chrome(&changed).is_err());
    }
}
