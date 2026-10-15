use super::*;

fn window(content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>")
}

fn pane(id: &str, content: &str) -> String {
    format!("<DockPane Id='{id}'>{content}</DockPane>")
}

fn group(id: &str) -> String {
    format!("<DockGroup>{}</DockGroup>", pane(id, "<TextBox/>"))
}

fn workspace(content: &str) -> String {
    format!("<DockWorkspace>{content}</DockWorkspace>")
}

fn parsed_workspace(content: &str) -> DockWorkspaceDefinition {
    let ElementKind::DockWorkspace(workspace) = parse(&window(content)).unwrap().content.kind
    else {
        panic!("expected workspace");
    };
    workspace
}

#[test]
fn docking_parses_ordered_binary_splits_tabs_and_persistent_content() {
    let xml = workspace(&format!(
        "<DockSplit Orientation='Vertical' Ratio='0.25'>
          <DockGroup SelectedPane='Second'>
            <DockPane Id='First'><TextBlock Text='One'/></DockPane>
            <DockPane Id='Second' Header='Второй' CanClose='False' MinWidth='240' MinHeight='160'>
              <StackPanel><TextBox x:Name='Message' Text='Draft' CommitOnLostFocus='True'/></StackPanel>
            </DockPane>
          </DockGroup>{}</DockSplit>", group("Third")
    ));
    let model = parsed_workspace(&xml);
    validate_docking(&model).unwrap();
    let DockNodeDefinition::Split {
        orientation,
        ratio,
        first,
        ..
    } = &model.root
    else {
        panic!("expected split");
    };
    assert_eq!(*orientation, Orientation::Vertical);
    assert_eq!(*ratio, 0.25);
    let DockNodeDefinition::Group { selected_pane, .. } = first.as_ref() else {
        panic!("expected group");
    };
    assert_eq!(selected_pane.as_deref(), Some("Second"));
    let panes = model.panes();
    assert_eq!(
        panes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["First", "Second", "Third"]
    );
    assert_eq!(panes[0].header, "First");
    assert!(panes[0].can_close);
    assert_eq!((panes[0].min_width, panes[0].min_height), (120.0, 80.0));
    assert_eq!(panes[1].header, "Второй");
    assert!(!panes[1].can_close);
    assert_eq!((panes[1].min_width, panes[1].min_height), (240.0, 160.0));
    let ElementKind::StackPanel(content) = &panes[1].content.kind else {
        panic!()
    };
    assert_eq!(content.children[0].common.name.as_deref(), Some("Message"));
    let generated = compile(&window(&xml)).unwrap();
    for expected in [
        "ElementKind::DockWorkspace(",
        "DockWorkspaceDefinition { root:",
        "DockNodeDefinition::Split {",
        "Orientation::Vertical",
        "ratio: 0.25",
        "first: Box::new(",
        "second: Box::new(",
        "DockNodeDefinition::Group {",
        "selected_pane: Some(\"Second\".into())",
        "DockPaneDefinition {",
        "header: \"Второй\".into()",
        "can_close: false",
        "min_width: 240.0",
        "content: Box::new(",
        "name: Some(\"Message\".into())",
    ] {
        assert!(generated.contains(expected), "{expected}\n{generated}");
    }
}

#[test]
fn docking_rejects_unsupported_structure_and_internal_attributes() {
    let group_a = group("A");
    let cases = [
        String::new(),
        format!("{group_a}{group_a}"),
        "<TextBox/>".into(),
        "<DockGroup/>".into(),
        "<DockGroup><TextBox/></DockGroup>".into(),
        "<DockGroup Id='G'><DockPane Id='A'><TextBox/></DockPane></DockGroup>".into(),
        "<DockGroup x:Name='G'><DockPane Id='A'><TextBox/></DockPane></DockGroup>".into(),
        "<DockGroup SelectedPane='Missing'><DockPane Id='A'><TextBox/></DockPane></DockGroup>"
            .into(),
        "<DockGroup><DockPane><TextBox/></DockPane></DockGroup>".into(),
        "<DockGroup><DockPane Id='A'/></DockGroup>".into(),
        "<DockGroup><DockPane Id='A'><TextBox/><Button/></DockPane></DockGroup>".into(),
        "<DockGroup><DockPane Id='A' IsFloating='True'><TextBox/></DockPane></DockGroup>".into(),
        "<DockGroup><DockPane Id='A' x:Name='P'><TextBox/></DockPane></DockGroup>".into(),
        "<DockGroup><DockPane Id='A' CanClose='yes'><TextBox/></DockPane></DockGroup>".into(),
        format!("<DockSplit>{group_a}</DockSplit>"),
        format!("<DockSplit>{group_a}{group_a}{group_a}</DockSplit>"),
        format!(
            "<DockSplit Orientation='horizontal'>{group_a}{}</DockSplit>",
            group("B")
        ),
        format!("<DockSplit Width='200'>{group_a}{}</DockSplit>", group("B")),
        format!("<DockFloating>{group_a}</DockFloating>"),
        format!("text{group_a}"),
        "<DockGroup xmlns='wrong'><DockPane Id='A'><TextBox/></DockPane></DockGroup>".into(),
    ];
    for content in cases {
        assert!(parse(&window(&workspace(&content))).is_err(), "{content}");
    }
    for id in ["", " ", "1Pane", "has-dash", "Русский", &"a".repeat(129)] {
        assert!(parse(&window(&workspace(&group(id)))).is_err(), "{id}");
    }
    assert!(parse(&window(&workspace(&group(&"a".repeat(128))))).is_ok());
}

#[test]
fn docking_validates_ratios_minimum_dimensions_and_headers() {
    for ratio in [
        "0",
        "0.09",
        "0.91",
        "1",
        "NaN",
        "inf",
        "-inf",
        "{Binding Ratio}",
    ] {
        let split = format!(
            "<DockSplit Ratio='{ratio}'>{}{}</DockSplit>",
            group("A"),
            group("B")
        );
        assert!(parse(&window(&workspace(&split))).is_err(), "{ratio}");
    }
    for ratio in ["0.1", "0.5", "0.9"] {
        let split = format!(
            "<DockSplit Ratio='{ratio}'>{}{}</DockSplit>",
            group("A"),
            group("B")
        );
        assert!(parse(&window(&workspace(&split))).is_ok(), "{ratio}");
    }
    for property in ["MinWidth", "MinHeight"] {
        for value in ["0", "-1", "NaN", "inf", "16385"] {
            let content = format!(
                "<DockGroup><DockPane Id='A' {property}='{value}'><TextBox/></DockPane></DockGroup>"
            );
            assert!(
                parse(&window(&workspace(&content))).is_err(),
                "{property}={value}"
            );
        }
    }
    for header in [String::new(), "  ".into(), "ж".repeat(2049)] {
        let content = format!(
            "<DockGroup><DockPane Id='A' Header='{header}'><TextBox/></DockPane></DockGroup>"
        );
        assert!(parse(&window(&workspace(&content))).is_err());
    }
    let header = "ж".repeat(2048);
    let content =
        format!("<DockGroup><DockPane Id='A' Header='{header}'><TextBox/></DockPane></DockGroup>");
    assert!(parse(&window(&workspace(&content))).is_ok());
}

#[test]
fn docking_rejects_duplicate_ids_names_nested_workspaces_and_ribbons() {
    let duplicate_ids = format!("<DockSplit>{}{}</DockSplit>", group("A"), group("A"));
    assert!(
        parse(&window(&workspace(&duplicate_ids)))
            .unwrap_err()
            .message
            .contains("Duplicate DockPane")
    );
    let duplicate_names = format!(
        "<DockGroup>{}{}</DockGroup>",
        pane("A", "<TextBox x:Name='Same'/>"),
        pane("B", "<TextBox x:Name='Same'/>")
    );
    assert!(
        parse(&window(&workspace(&duplicate_names)))
            .unwrap_err()
            .message
            .contains("duplicate x:Name")
    );
    let simple = workspace(&group("A"));
    let nested = workspace(&format!(
        "<DockGroup>{}</DockGroup>",
        pane("Outer", &format!("<StackPanel>{simple}</StackPanel>"))
    ));
    assert!(
        parse(&window(&nested))
            .unwrap_err()
            .message
            .contains("cannot contain a DockWorkspace")
    );
    let ribbon = "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='G' Header='Commands'><Button Command='Go' KeyTip='G'/></RibbonGroup></RibbonTab></Ribbon>";
    let nested = workspace(&format!(
        "<DockGroup>{}</DockGroup>",
        pane("Outer", &format!("<StackPanel>{ribbon}</StackPanel>"))
    ));
    assert!(
        parse(&window(&nested))
            .unwrap_err()
            .message
            .contains("cannot contain a Ribbon")
    );
    let duplicate = format!("<StackPanel>{simple}{simple}</StackPanel>");
    assert!(
        parse(&window(&duplicate))
            .unwrap_err()
            .message
            .contains("at most one DockWorkspace")
    );
    let across = format!(
        "<StackPanel><TextBox x:Name='Same'/>{}</StackPanel>",
        workspace(&format!(
            "<DockGroup>{}</DockGroup>",
            pane("A", "<TextBox x:Name='Same'/>")
        ))
    );
    assert!(parse(&window(&across)).is_err());
}

#[test]
fn docking_counts_internal_nodes_and_content_in_global_budgets() {
    for (panes, valid) in [(64, true), (65, false)] {
        let children = (0..panes)
            .map(|n| pane(&format!("P{n}"), "<TextBox/>"))
            .collect::<String>();
        assert_eq!(
            parse(&window(&workspace(&format!(
                "<DockGroup>{children}</DockGroup>"
            ))))
            .is_ok(),
            valid
        );
    }
    for (children, valid) in [(252, true), (253, false)] {
        let content = format!("<StackPanel>{}</StackPanel>", "<Button/>".repeat(children));
        let model = workspace(&format!("<DockGroup>{}</DockGroup>", pane("A", &content)));
        assert_eq!(parse(&window(&model)).is_ok(), valid, "{children}");
    }
    for (panels, valid) in [(28, true), (29, false)] {
        let content = format!(
            "{}<TextBox/>{}",
            "<StackPanel>".repeat(panels),
            "</StackPanel>".repeat(panels)
        );
        let model = workspace(&format!("<DockGroup>{}</DockGroup>", pane("A", &content)));
        assert_eq!(parse(&window(&model)).is_ok(), valid, "{panels}");
    }
    let content = "<StackPanel><DropDownButton><MenuItem Header='One' Command='Go'/></DropDownButton></StackPanel>";
    let model = workspace(&format!("<DockGroup>{}</DockGroup>", pane("A", content)));
    assert!(parse(&window(&model)).is_ok());
}

#[test]
fn manual_docking_validation_covers_mutated_snapshots_before_mount() {
    let base = parsed_workspace(&workspace(&group("A")));
    for change in 0..7 {
        let mut model = base.clone();
        let DockNodeDefinition::Group {
            panes,
            selected_pane,
        } = &mut model.root
        else {
            panic!()
        };
        match change {
            0 => *selected_pane = Some("Unknown".into()),
            1 => panes[0].id.clear(),
            2 => panes[0].min_width = f32::NAN,
            3 => panes[0].header.clear(),
            4 => panes[0].content.kind = ElementKind::DockWorkspace(base.clone()),
            5 => panes.push(panes[0].clone()),
            6 => panes[0].content.kind = ElementKind::Ribbon(RibbonDefinition::default()),
            _ => unreachable!(),
        }
        assert!(validate_docking(&model).is_err(), "change {change}");
    }
    let model = DockWorkspaceDefinition {
        root: DockNodeDefinition::Split {
            orientation: Orientation::Horizontal,
            ratio: f32::INFINITY,
            first: Box::new(base.root.clone()),
            second: Box::new(base.root.clone()),
        },
    };
    assert!(validate_docking(&model).unwrap_err().contains("Ratio"));
    let mut model = base;
    for _ in 0..MAX_ELEMENT_DEPTH {
        model.root = DockNodeDefinition::Split {
            orientation: Orientation::Horizontal,
            ratio: 0.5,
            first: Box::new(model.root),
            second: Box::new(DockNodeDefinition::default()),
        };
    }
    assert!(validate_docking(&model).is_err());
}

#[test]
fn named_radio_groups_cannot_span_independent_pane_arenas() {
    let same_group = "<RadioButton GroupName='Alignment'/>";
    let source = workspace(&format!(
        "<DockGroup>{}{}</DockGroup>",
        pane("A", same_group),
        pane("B", same_group)
    ));
    assert!(
        parse(&window(&source))
            .unwrap_err()
            .message
            .contains("cannot span multiple DockPane")
    );
    let local_group = "<StackPanel><RadioButton GroupName='Alignment' IsChecked='True'/><RadioButton GroupName='Alignment'/></StackPanel>";
    let source = workspace(&format!(
        "<DockGroup>{}{}</DockGroup>",
        pane("A", local_group),
        pane("B", "<RadioButton GroupName='Other'/>")
    ));
    let mut model = parsed_workspace(&source);
    let DockNodeDefinition::Group { panes, .. } = &mut model.root else {
        panic!()
    };
    let ElementKind::RadioButton(radio) = &mut panes[1].content.kind else {
        panic!()
    };
    radio.group_name = Some("Alignment".into());
    assert!(
        validate_docking(&model)
            .unwrap_err()
            .contains("cannot span multiple DockPane")
    );
    let source = workspace(&format!(
        "<DockGroup>{}{}</DockGroup>",
        pane("A", "<RadioButton IsChecked='True'/>"),
        pane("B", "<RadioButton IsChecked='True'/>")
    ));
    assert!(parse(&window(&source)).is_ok());
}

#[test]
fn integrated_ribbon_header_accepts_docking_as_remaining_shell_child() {
    let content = format!(
        "<StackPanel><Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='G' Header='Commands'><Button Command='Go' KeyTip='G'/></RibbonGroup></RibbonTab></Ribbon>{}</StackPanel>",
        workspace(&group("A"))
    );
    let source =
        format!("<Window xmlns='{UI_NAMESPACE}' TitleBar='Ribbon' Padding='0'>{content}</Window>");
    let definition = parse(&source).unwrap();
    validate_window_chrome(&definition).unwrap();
    validate_window_docking(&definition).unwrap();
    let mut invalid = definition.clone();
    let ElementKind::StackPanel(shell) = &mut invalid.content.kind else {
        panic!()
    };
    shell.children.push(shell.children[1].clone());
    assert!(
        validate_window_docking(&invalid)
            .unwrap_err()
            .contains("at most one")
    );
}
