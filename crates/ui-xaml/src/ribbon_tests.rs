use super::*;

#[test]
fn ribbon_title_limit_is_checked_before_native_window_creation() {
    let source = |title: &str| {
        format!(
            r#"<Window xmlns="urn:rust-ui-engine:ui" TitleBar="Ribbon" Padding="0" Title="{title}"><Ribbon><RibbonTab Id="Home" Header="Home" KeyTip="H"><RibbonGroup Id="G" Header="G"><Button Command="Save" KeyTip="S" /></RibbonGroup></RibbonTab></Ribbon></Window>"#
        )
    };
    assert!(parse(&source(&"я".repeat(2048))).is_ok());
    assert!(
        parse(&source(&"я".repeat(2049)))
            .unwrap_err()
            .to_string()
            .contains("4096")
    );
}

fn form(content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{content}</Window>")
}

fn ribbon(items: &str) -> String {
    format!(
        "<Ribbon><RibbonTab Id='Home' Header='Главная' KeyTip='h'><RibbonGroup Id='File' Header='Файл'>{items}</RibbonGroup></RibbonTab></Ribbon>"
    )
}

fn definition(content: &str) -> RibbonDefinition {
    let ElementKind::Ribbon(ribbon) = parse(&form(content)).unwrap().content.kind else {
        panic!("Ribbon");
    };
    ribbon
}

#[test]
fn retained_ribbon_content_backstage_and_qat_compile_with_shared_names_and_resources() {
    let source = form(
        r#"
      <StackPanel>
        <Ribbon CustomizeQuickAccess="True">
          <Ribbon.Resources>
            <Style x:Key="Entry" TargetType="TextBox"><Setter Property="Width" Value="160"/></Style>
          </Ribbon.Resources>
          <Ribbon.Backstage><Backstage Header="Файл" KeyTip="F">
            <BackstagePage Id="Info" Header="Сведения" KeyTip="I">
              <TextBox x:Name="DocumentTitle" Style="{StaticResource Entry}" Text="{Binding Title}"/>
            </BackstagePage>
            <BackstageCommand Command="Save" KeyTip="S"/>
          </Backstage></Ribbon.Backstage>
          <RibbonTab Id="Home" Header="Главная" KeyTip="H"><RibbonGroup Id="Font" Header="Шрифт">
            <Button Command="Save" KeyTip="S"/>
            <RibbonContent Id="font.entry" Header="Шрифт" KeyTip="FF" Width="180" MinWidth="120" Height="84">
              <StackPanel><TextBox x:Name="Font" Style="{StaticResource Entry}" Text="{Binding Font}"/>
              <TextBlock Text="{Binding Text, ElementName=DocumentTitle}"/></StackPanel>
            </RibbonContent>
          </RibbonGroup></RibbonTab>
        </Ribbon>
        <TextBlock Text="{Binding Text, ElementName=Font}"/>
      </StackPanel>"#,
    );
    let parsed = parse(&source).unwrap();
    let ribbon_element = parsed.content.children()[0];
    let ElementKind::Ribbon(definition) = &ribbon_element.kind else {
        panic!("Ribbon")
    };
    assert!(definition.customize_quick_access);
    assert_eq!(definition.content_elements().len(), 2);
    assert_eq!(definition.tabs[0].groups[0].contents[0].id, "font.entry");
    assert_eq!(
        definition.content_elements()[0].children()[0].common.width,
        Some(160.0)
    );
    let output = compile(&source).unwrap();
    for marker in [
        "customize_quick_access: true",
        "RibbonContentDefinition",
        "RibbonBackstageItemDefinition::Page",
        "RibbonBackstageItemDefinition::Command",
        "DocumentTitle",
        "BindingDefinition",
    ] {
        assert!(output.contains(marker), "{marker}");
    }
    let content_only =
        ribbon("<RibbonContent Id='only' Header='Only' KeyTip='E'><TextBox/></RibbonContent>");
    assert!(parse(&form(&content_only)).is_ok());
}

#[test]
fn retained_ribbon_profiles_reject_ambiguous_names_structure_and_geometry() {
    let slot = "<RibbonContent Id='slot' Header='Editor' KeyTip='E'><TextBox x:Name='Value'/></RibbonContent>";
    for content in [
        slot.replace("Id='slot'", "Id='bad id'"),
        slot.replace("Header='Editor'", "Header=''"),
        slot.replace("KeyTip='E'", "KeyTip=''"),
        slot.replace("KeyTip='E'", "KeyTip='é'"),
        slot.replace("<TextBox x:Name='Value'/>", ""),
        slot.replace("<TextBox x:Name='Value'/>", "<TextBox/><TextBox/>"),
        slot.replace("Header='Editor'", "Header='Editor' Width='23'"),
        slot.replace("Header='Editor'", "Header='Editor' Width='4097'"),
        slot.replace("Header='Editor'", "Header='Editor' MinWidth='181'"),
        slot.replace("Header='Editor'", "Header='Editor' Height='513'"),
        format!("{slot}<Button Command='Save' KeyTip='S'/>"),
        format!("<Button Command='Save' KeyTip='EF'/>{slot}"),
        format!("{slot}{slot}"),
        format!(
            "{slot}{}",
            slot.replace("Id='slot'", "Id='other'")
                .replace("KeyTip='E'", "KeyTip='O'")
        ),
        slot.replace(
            "<TextBox x:Name='Value'/>",
            &ribbon("<Button Command='Save' KeyTip='S'/>"),
        ),
    ] {
        assert!(parse(&form(&ribbon(&content))).is_err(), "{content}");
    }
    let backstage = "<Ribbon.Backstage><Backstage Header='File' KeyTip='F'><BackstagePage Id='Info' Header='Info' KeyTip='I'><TextBox/></BackstagePage><BackstageCommand Command='Save' KeyTip='S'/></Backstage></Ribbon.Backstage>";
    let base = ribbon("<Button Command='Save' KeyTip='S'/>");
    for invalid in [
        backstage.replace("KeyTip='F'", "KeyTip='H'"),
        backstage.replace("KeyTip='F'", "KeyTip='1'"),
        backstage.replace("KeyTip='S'", "KeyTip='IX'"),
        backstage.replace("Command='Save'", "Command='Save' Header='Override'"),
        backstage.replace("<TextBox/>", ""),
        backstage.replace("<TextBox/>", "<TextBox/><TextBox/>"),
        backstage.replace("</Backstage>", "<BackstagePage Id='Info' Header='Other' KeyTip='O'><TextBox/></BackstagePage></Backstage>"),
        format!("{backstage}{backstage}"),
    ] {
        assert!(parse(&form(&base.replacen("<Ribbon>", &format!("<Ribbon>{invalid}"), 1))).is_err(), "{invalid}");
    }
}

#[test]
fn retained_ribbon_limits_include_nested_control_depth_and_slot_count() {
    let slot = |i| {
        format!(
            "<RibbonContent Id='slot{i}' Header='Editor' KeyTip='E{i:02}'><TextBox/></RibbonContent>"
        )
    };
    let slots = (0..64).map(slot).collect::<String>();
    assert!(parse(&form(&ribbon(&slots))).is_ok());
    assert!(parse(&form(&ribbon(&(slots + &slot(64))))).is_err());
    let nested =
        |depth: usize| "<StackPanel>".repeat(depth) + "<TextBox/>" + &"</StackPanel>".repeat(depth);
    let content = |depth| {
        format!(
            "<RibbonContent Id='slot' Header='Editor' KeyTip='E'>{}</RibbonContent>",
            nested(depth)
        )
    };
    assert!(parse(&form(&ribbon(&content(MAX_ELEMENT_DEPTH - 5)))).is_ok());
    assert!(parse(&form(&ribbon(&content(MAX_ELEMENT_DEPTH - 4)))).is_err());
    let mut model = definition(&ribbon(&slot(0)));
    model.tabs[0].groups[0].contents[0].height = f32::NAN;
    assert!(validate_ribbon(&model).is_err());
}

fn chrome_form(window_attributes: &str, content: &str) -> String {
    format!("<Window xmlns='{UI_NAMESPACE}' {window_attributes}>{content}</Window>")
}

#[test]
fn native_title_bar_is_compatible_with_existing_content_placement() {
    for attributes in ["", "TitleBar='Native'"] {
        let source = chrome_form(
            attributes,
            "<StackPanel Orientation='Horizontal' Padding='12' Margin='6' Width='400' Height='300'><TextBox/></StackPanel>",
        );
        let window = parse(&source).unwrap();
        assert_eq!(window.title_bar, TitleBar::Native);
        assert_eq!(window.padding, [32.0; 4]);
        validate_window_chrome(&window).unwrap();
        assert!(
            compile(&source)
                .unwrap()
                .contains("title_bar: rust_desktop_ui_xaml::TitleBar::Native,")
        );
    }
}

#[test]
fn ribbon_title_bar_accepts_flush_direct_and_first_child_placement() {
    let ribbon = ribbon("<Button Command='Save' KeyTip='S'/>");
    for content in [
        ribbon.clone(),
        format!(
            "<StackPanel Spacing='12' Surface='Card'>{ribbon}<TextBox Margin='16' Width='320' Height='40'/></StackPanel>"
        ),
    ] {
        let source = chrome_form("TitleBar='Ribbon' Padding='0'", &content);
        let window = parse(&source).unwrap();
        assert_eq!(window.title_bar, TitleBar::Ribbon);
        validate_window_chrome(&window).unwrap();
        assert!(
            compile(&source)
                .unwrap()
                .contains("title_bar: rust_desktop_ui_xaml::TitleBar::Ribbon,")
        );
    }
}

#[test]
fn title_bar_values_are_explicit_and_diagnostic_points_to_the_attribute() {
    for value in ["", "native", "ribbon", "Custom", " Ribbon "] {
        let source = chrome_form(&format!("\n  TitleBar='{value}'"), "<TextBox/>");
        let error = parse(&source).unwrap_err();
        assert_eq!((error.line, error.column), (2, 3));
        assert_eq!(error.message, "TitleBar must be Native or Ribbon");
    }
}

#[test]
fn ribbon_title_bar_rejects_offsets_fixed_sizes_and_nonleading_ribbons() {
    let ribbon = ribbon("<Button Command='Save' KeyTip='S'/>");
    for (attributes, expected) in [
        ("", "Padding=0"),
        ("Padding='1'", "Padding=0"),
        ("Padding='0,0,0,1'", "Padding=0"),
    ] {
        let source = chrome_form(&format!("TitleBar='Ribbon' {attributes}"), &ribbon);
        assert!(parse(&source).unwrap_err().message.contains(expected));
    }
    let mut invalid = vec![
        ("<TextBox/>".to_owned(), "requires a Ribbon"),
        ("<StackPanel/>".to_owned(), "first StackPanel child"),
        (
            format!("<StackPanel><TextBlock/>{ribbon}</StackPanel>"),
            "first StackPanel child",
        ),
        (
            format!("<StackPanel><StackPanel>{ribbon}</StackPanel></StackPanel>"),
            "first StackPanel child",
        ),
        (
            format!("<StackPanel Orientation='Horizontal'>{ribbon}</StackPanel>"),
            "vertical root StackPanel",
        ),
        (
            format!("<StackPanel Padding='0,0,1,0'>{ribbon}</StackPanel>"),
            "root StackPanel Padding=0",
        ),
    ];
    for attribute in ["Margin='0,0,0,1'", "Width='640'", "Height='360'"] {
        invalid.push((
            format!("<StackPanel {attribute}>{ribbon}</StackPanel>"),
            "root StackPanel Margin=0 and automatic Width/Height",
        ));
        let placed = ribbon.replacen("<Ribbon>", &format!("<Ribbon {attribute}>"), 1);
        invalid.push((placed.clone(), "Ribbon Margin=0 and automatic Width/Height"));
        invalid.push((
            format!("<StackPanel>{placed}<TextBox/></StackPanel>"),
            "Ribbon Margin=0 and automatic Width/Height",
        ));
    }
    for (content, expected) in invalid {
        let source = chrome_form("TitleBar='Ribbon' Padding='0'", &content);
        let error = parse(&source).unwrap_err();
        assert!(error.message.contains(expected), "{source}: {error}");
        assert!(compile(&source).is_err());
    }
}

#[test]
fn manual_window_chrome_validation_rejects_invalid_geometry_and_duplicate_ribbons() {
    let source = chrome_form(
        "TitleBar='Ribbon' Padding='0'",
        &format!(
            "<StackPanel>{}<TextBox/></StackPanel>",
            ribbon("<Button Command='Save' KeyTip='S'/>")
        ),
    );
    let valid = parse(&source).unwrap();
    for padding in [[1.0; 4], [f32::NAN; 4], [f32::INFINITY; 4]] {
        let mut invalid = valid.clone();
        invalid.padding = padding;
        assert!(
            validate_window_chrome(&invalid)
                .unwrap_err()
                .contains("Padding=0")
        );
    }
    for (margin, width, height) in [
        ([1.0; 4], None, None),
        ([f32::NAN; 4], None, None),
        ([0.0; 4], Some(640.0), None),
        ([0.0; 4], None, Some(360.0)),
    ] {
        for root in [true, false] {
            let mut invalid = valid.clone();
            let common = if root {
                &mut invalid.content.common
            } else {
                let ElementKind::StackPanel(panel) = &mut invalid.content.kind else {
                    unreachable!();
                };
                &mut panel.children[0].common
            };
            common.margin = margin;
            common.width = width;
            common.height = height;
            assert!(
                validate_window_chrome(&invalid)
                    .unwrap_err()
                    .contains("Margin=0 and automatic Width/Height")
            );
        }
    }
    let mut invalid = valid;
    let ElementKind::StackPanel(panel) = &mut invalid.content.kind else {
        unreachable!();
    };
    panel.children.push(ElementDefinition {
        kind: ElementKind::StackPanel(StackPanelDefinition {
            children: vec![panel.children[0].clone()],
            ..Default::default()
        }),
        ..Default::default()
    });
    assert!(
        validate_window_chrome(&invalid)
            .unwrap_err()
            .contains("exactly one Ribbon")
    );
}

#[test]
fn ribbon_preserves_command_model_and_normalizes_key_tips() {
    let source = r#"<Ribbon x:Name="MainRibbon" SelectedTab="Home" IsEnabled="False" Margin="1,2">
        <Ribbon.QuickAccessToolbar><Button Command="Save"/><Button Command="Pin"/></Ribbon.QuickAccessToolbar>
        <RibbonTab Id="Home" Header="Главная" KeyTip="h">
            <RibbonGroup Id="File" Header="Файл" CollapsePriority="65535">
                <Button Command="Save" KeyTip="sv" Icon="Save" ToolTip="Сохранить"/>
                <ToggleButton Command="Pin" KeyTip="p" Size="Small" Icon="Pin"/>
                <SplitButton Command="Export" KeyTip="e" Size="Large"><MenuItem Command="Csv"/><MenuItem Command="Json"/></SplitButton>
            </RibbonGroup>
        </RibbonTab>
        <RibbonTab Id="Selection" Header="Выбор" KeyTip="s" Context="Text">
            <RibbonGroup Id="File" Header="Файл"><Button Command="Save" KeyTip="sv"/></RibbonGroup>
        </RibbonTab>
    </Ribbon>"#;
    let window = parse(&form(source)).unwrap();
    assert_eq!(window.content.common.name.as_deref(), Some("MainRibbon"));
    assert!(!window.content.common.is_enabled);
    assert_eq!(window.content.common.margin, [1.0, 2.0, 1.0, 2.0]);
    let ElementKind::Ribbon(ribbon) = window.content.kind else {
        panic!("Ribbon")
    };
    validate_ribbon(&ribbon).unwrap();
    assert_eq!(ribbon.selected_tab.as_deref(), Some("Home"));
    assert!(!ribbon.is_minimized);
    assert_eq!(ribbon.quick_access, ["Save", "Pin"]);
    assert_eq!(ribbon.tabs[0].key_tip, "H");
    assert_eq!(ribbon.tabs[1].context.as_deref(), Some("Text"));
    let group = &ribbon.tabs[0].groups[0];
    assert_eq!(group.collapse_priority, u16::MAX);
    assert_eq!(group.items[0].key_tip, "SV");
    assert_eq!(group.items[0].size, RibbonItemSize::Large);
    assert_eq!(group.items[0].min_size, RibbonItemSize::IconOnly);
    assert_eq!(group.items[0].icon, Some(IconSymbol::Save));
    assert_eq!(group.items[0].tool_tip.as_deref(), Some("Сохранить"));
    assert_eq!(group.items[1].kind, RibbonItemKindDefinition::ToggleButton);
    assert_eq!(group.items[1].size, RibbonItemSize::Small);
    assert_eq!(group.items[2].kind, RibbonItemKindDefinition::SplitButton);
    assert_eq!(
        group.items[2].menu,
        [
            RibbonMenuEntryDefinition::Command("Csv".into()),
            RibbonMenuEntryDefinition::Command("Json".into()),
        ]
    );
}

#[test]
fn ribbon_size_limits_preserve_preferred_size_and_validate_each_minimum() {
    for kind in ["Button", "ToggleButton", "SplitButton", "DropDownButton"] {
        let menu = if matches!(kind, "SplitButton" | "DropDownButton") {
            "<MenuItem Command='Leaf'/>"
        } else {
            ""
        };
        for (preferred, minimum, valid) in [
            ("Large", "Large", true),
            ("Large", "Small", true),
            ("Large", "IconOnly", true),
            ("Small", "Large", false),
            ("Small", "Small", true),
            ("Small", "IconOnly", true),
            ("IconOnly", "IconOnly", false),
            ("Large", "Medium", false),
        ] {
            let source = ribbon(&format!(
                "<{kind} Command='Owner' KeyTip='O' Size='{preferred}' MinSize='{minimum}'>{menu}</{kind}>"
            ));
            let parsed = parse(&form(&source));
            assert_eq!(parsed.is_ok(), valid, "{source}");
            if valid {
                let ElementKind::Ribbon(ribbon) = parsed.unwrap().content.kind else {
                    panic!("Ribbon")
                };
                validate_ribbon(&ribbon).unwrap();
                let item = &ribbon.tabs[0].groups[0].items[0];
                assert_eq!(format!("{:?}", item.size), preferred);
                assert_eq!(format!("{:?}", item.min_size), minimum);
                let generated = compile(&form(&source)).unwrap();
                assert!(generated.contains(&format!(
                    "min_size: rust_desktop_ui_xaml::RibbonItemSize::{minimum}"
                )));
            }
        }
    }
    let source = definition(&ribbon("<Button Command='Owner' KeyTip='O'/>"));
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].items[0].size = RibbonItemSize::IconOnly;
    assert!(validate_ribbon(&invalid).unwrap_err().contains("Size"));
    let mut invalid = source;
    invalid.tabs[0].groups[0].items[0].size = RibbonItemSize::Small;
    invalid.tabs[0].groups[0].items[0].min_size = RibbonItemSize::Large;
    assert!(validate_ribbon(&invalid).unwrap_err().contains("MinSize"));
}

#[test]
fn ribbon_minimized_state_and_nested_command_menus_compile_without_static_leaf_labels() {
    let source = ribbon(
        r#"<DropDownButton Command="Export" KeyTip="e" Size="Small" Icon="Save">
            <MenuItem Header="Formats &quot;\&#10;🌍">
                <MenuItem Command="Csv"/>
                <MenuSeparator/>
                <MenuItem Header="Advanced"><MenuItem Command="Json"/></MenuItem>
            </MenuItem>
            <Separator/>
            <MenuItem Command="Other"/>
        </DropDownButton>
        <SplitButton Command="Save" KeyTip="s" Size="Small"><MenuItem Command="Other"/></SplitButton>"#,
    ).replacen("<Ribbon>", "<Ribbon IsMinimized='True'>", 1);
    let ribbon = definition(&source);
    assert!(ribbon.is_minimized);
    let items = &ribbon.tabs[0].groups[0].items;
    assert_eq!(items[0].kind, RibbonItemKindDefinition::DropDownButton);
    assert_eq!(items[0].size, RibbonItemSize::Small);
    assert_eq!(items[1].kind, RibbonItemKindDefinition::SplitButton);
    assert_eq!(items[1].size, RibbonItemSize::Small);
    let RibbonMenuEntryDefinition::Submenu {
        header,
        items: submenu,
    } = &items[0].menu[0]
    else {
        panic!("expected a submenu");
    };
    assert_eq!(header, "Formats \"\\\n🌍");
    assert_eq!(submenu[0], RibbonMenuEntryDefinition::Command("Csv".into()));
    assert_eq!(submenu[1], RibbonMenuEntryDefinition::Separator);
    assert!(matches!(
        submenu[2],
        RibbonMenuEntryDefinition::Submenu { .. }
    ));
    assert_eq!(items[0].menu[1], RibbonMenuEntryDefinition::Separator);
    assert_eq!(
        items[0].menu[2],
        RibbonMenuEntryDefinition::Command("Other".into())
    );
    validate_ribbon(&ribbon).unwrap();
    let generated = compile(&form(&source)).unwrap();
    for expected in [
        "is_minimized: true",
        "RibbonItemKindDefinition::DropDownButton",
        "RibbonMenuEntryDefinition::Command(\"Csv\".into())",
        "RibbonMenuEntryDefinition::Submenu { header: \"Formats \\\"\\\\\\n🌍\".into()",
        "RibbonMenuEntryDefinition::Separator",
    ] {
        assert!(
            generated.contains(expected),
            "missing {expected}\n{generated}"
        );
    }
    for value in ["False", "false"] {
        assert!(
            !definition(&source.replace("IsMinimized='True'", &format!("IsMinimized='{value}'")))
                .is_minimized
        );
    }
    for value in ["1", "0", "TRUE", "yes", "{Binding Minimized}"] {
        let error = parse(&form(
            &source.replace("IsMinimized='True'", &format!("IsMinimized='{value}'")),
        ))
        .unwrap_err();
        assert!(error.line > 0 && error.column > 0);
    }
}

#[test]
fn ribbon_menu_parser_rejects_state_overrides_ambiguous_leaves_and_invalid_branches() {
    for menu in [
        "<MenuItem Header='Leaf' Command='Save'/>",
        "<MenuItem Command='Save' IsEnabled='False'/>",
        "<MenuItem Command='Save' IsChecked='True'/>",
        "<MenuItem Command='Save' x:Name='Leaf'/>",
        "<MenuItem Command='Save'><MenuSeparator/></MenuItem>",
        "<MenuItem Command='Save'>text</MenuItem>",
        "<MenuItem Command='Bad.Command'/>",
        "<MenuItem Header='Branch'/>",
        "<MenuItem Header=' '><MenuItem Command='Save'/></MenuItem>",
        "<MenuItem Header='{Binding Header}'><MenuItem Command='Save'/></MenuItem>",
        "<MenuItem><MenuItem Command='Save'/></MenuItem>",
        "<MenuItem Header='Branch' IsEnabled='False'><MenuItem Command='Save'/></MenuItem>",
        "<MenuSeparator Header='Rule'/>",
        "<MenuSeparator><MenuItem Command='Save'/></MenuSeparator>",
        "<Separator>text</Separator>",
        "<MenuItem xmlns='' Command='Save'/>",
        "<MenuSeparator xmlns='urn:other'/>",
        "<Button Command='Save'/>",
    ] {
        for kind in ["SplitButton", "DropDownButton"] {
            let source = ribbon(&format!(
                "<{kind} Command='Owner' KeyTip='O'>{menu}</{kind}>"
            ));
            let error = parse(&form(&source)).expect_err(menu);
            assert!(error.line > 0 && error.column > 0, "{menu}");
        }
    }
}

fn nested_menu(depth: usize) -> String {
    let mut menu = "<MenuItem Command='Leaf'/>".to_owned();
    for _ in 1..depth {
        menu = format!("<MenuItem Header='Branch'>{menu}</MenuItem>");
    }
    menu
}

#[test]
fn nested_ribbon_menu_limits_include_every_branch_separator_and_window_ancestor() {
    let item =
        |menu: &str| format!("<DropDownButton Command='Owner' KeyTip='O'>{menu}</DropDownButton>");
    let source = ribbon(&item(&nested_menu(MAX_MENU_DEPTH)));
    let parsed = definition(&source);
    validate_ribbon(&parsed).unwrap();
    assert!(
        parse(&form(&ribbon(&item(&nested_menu(MAX_MENU_DEPTH + 1)))))
            .unwrap_err()
            .message
            .contains("depth")
    );
    let mut nested = source;
    for _ in 0..MAX_ELEMENT_DEPTH - (4 + MAX_MENU_DEPTH) {
        nested = format!("<StackPanel>{nested}</StackPanel>");
    }
    assert!(parse(&form(&nested)).is_ok());
    assert!(
        parse(&form(&format!("<StackPanel>{nested}</StackPanel>")))
            .unwrap_err()
            .message
            .contains("depth")
    );

    // Ribbon, tab, group and button consume four nodes; menu separators count.
    let menu = format!(
        "{}<MenuItem Command='Leaf'/>",
        "<MenuSeparator/>".repeat(MAX_ELEMENTS - 5)
    );
    let source = ribbon(&item(&menu));
    validate_ribbon(&definition(&source)).unwrap();
    assert!(
        parse(&form(&ribbon(&item(&format!("{menu}<Separator/>")))))
            .unwrap_err()
            .message
            .contains("count")
    );
}

#[test]
fn manually_constructed_ribbon_menus_receive_the_same_shape_depth_count_and_text_limits() {
    let source = definition(&ribbon(
        "<DropDownButton Command='Owner' KeyTip='O'><MenuItem Command='Leaf'/></DropDownButton>",
    ));
    for invalid_menu in [
        Vec::new(),
        vec![RibbonMenuEntryDefinition::Command("bad.command".into())],
        vec![RibbonMenuEntryDefinition::Submenu {
            header: "Branch".into(),
            items: Vec::new(),
        }],
        vec![RibbonMenuEntryDefinition::Submenu {
            header: " ".into(),
            items: vec![RibbonMenuEntryDefinition::Command("Leaf".into())],
        }],
        vec![RibbonMenuEntryDefinition::Separator; MAX_ELEMENTS],
        vec![RibbonMenuEntryDefinition::Submenu {
            header: "x".repeat(MAX_SOURCE_BYTES),
            items: vec![RibbonMenuEntryDefinition::Command("Leaf".into())],
        }],
    ] {
        let mut invalid = source.clone();
        invalid.tabs[0].groups[0].items[0].menu = invalid_menu;
        assert!(validate_ribbon(&invalid).is_err());
    }
    let mut nested = RibbonMenuEntryDefinition::Command("Leaf".into());
    for _ in 1..MAX_MENU_DEPTH {
        nested = RibbonMenuEntryDefinition::Submenu {
            header: "Branch".into(),
            items: vec![nested],
        };
    }
    let mut boundary = source;
    boundary.tabs[0].groups[0].items[0].menu = vec![nested.clone()];
    validate_ribbon(&boundary).unwrap();
    boundary.tabs[0].groups[0].items[0].menu = vec![RibbonMenuEntryDefinition::Submenu {
        header: "One more level".into(),
        items: vec![nested],
    }];
    assert!(validate_ribbon(&boundary).unwrap_err().contains("depth"));
}

#[test]
fn ribbon_codegen_escapes_all_text_fields_and_emits_typed_model() {
    let source = form(
        r#"<Ribbon SelectedTab="Home"><Ribbon.QuickAccessToolbar><Button Command="Save"/></Ribbon.QuickAccessToolbar><RibbonTab Id="Home" Header="Главная &quot;\&#10;🌍" KeyTip="h"><RibbonGroup Id="File" Header="Файл &quot;\&#10;🌍" CollapsePriority="12"><ToggleButton Command="Pin" KeyTip="p" Size="Small" Icon="Pin" ToolTip="Закрепить &quot;\&#10;🌍"/><SplitButton Command="Export" KeyTip="e"><MenuItem Command="Csv"/></SplitButton></RibbonGroup></RibbonTab><RibbonTab Id="Contextual" Header="Выбор" KeyTip="x" Context="Selection"><RibbonGroup Id="File" Header="Файл"><Button Command="Save" KeyTip="s"/></RibbonGroup></RibbonTab></Ribbon>"#,
    );
    let generated = compile(&source).unwrap();
    for expected in [
        "ElementKind::Ribbon(",
        "selected_tab: Some(\"Home\".into())",
        "quick_access: vec![\"Save\".into()]",
        "header: \"Главная \\\"\\\\\\n🌍\".into()",
        "header: \"Файл \\\"\\\\\\n🌍\".into()",
        "tool_tip: Some(\"Закрепить \\\"\\\\\\n🌍\".into())",
        "RibbonItemKindDefinition::ToggleButton",
        "RibbonItemKindDefinition::SplitButton",
        "RibbonItemKindDefinition::Button",
        "RibbonItemSize::Small",
        "RibbonItemSize::Large",
        "icon: Some(rust_desktop_ui_xaml::IconSymbol::Pin)",
        "key_tip: \"P\".into()",
        "collapse_priority: 12",
        "menu: vec![rust_desktop_ui_xaml::RibbonMenuEntryDefinition::Command(\"Csv\".into())]",
        "context: Some(\"Selection\".into())",
    ] {
        assert!(
            generated.contains(expected),
            "missing {expected}\n{generated}"
        );
    }
}

#[test]
fn ribbon_rejects_internal_layout_state_unknown_controls_and_bindings() {
    for item in [
        "<Button KeyTip='S'/>",
        "<Button Command='Save'/>",
        "<Button Command='Save' KeyTip='S' Content='Save'/>",
        "<Button Command='Save' KeyTip='S' IsEnabled='False'/>",
        "<Button Command='Save' KeyTip='S' Width='100'/>",
        "<Button Command='Save' KeyTip='S' FontSize='20'/>",
        "<Button Command='Save' KeyTip='S' x:Name='Save'/>",
        "<Button Command='Save' KeyTip='S' Size='Medium'/>",
        "<Button Command='{Binding Save}' KeyTip='S'/>",
        "<Button Command='Save.Other' KeyTip='S'/>",
        "<Button Command='Save' KeyTip='S' ToolTip='{Binding Tip}'/>",
        "<Button Command='Save' KeyTip='S' Icon='Unknown'/>",
        "<ToggleButton Command='Pin' KeyTip='P' IsChecked='True'/>",
        "<DropDownButton Command='Save' KeyTip='S'/>",
        "<TextBox/>",
        "<Separator/>",
        "<Button Command='Save' KeyTip='S'><MenuItem Command='Other'/></Button>",
        "<Button Command='Save' KeyTip='S'>text</Button>",
    ] {
        assert!(parse(&form(&ribbon(item))).is_err(), "{item}");
    }
}

#[test]
fn ribbon_rejects_ambiguous_menus_and_preserves_command_only_qat_references() {
    for item in [
        "<SplitButton Command='Save' KeyTip='S'/>",
        "<SplitButton Command='Save' KeyTip='S'><MenuItem Header='Other' Command='Other'/></SplitButton>",
        "<SplitButton Command='Save' KeyTip='S'><MenuItem/></SplitButton>",
        "<SplitButton Command='Save' KeyTip='S'><MenuItem Command='Other'><MenuItem Command='Save'/></MenuItem></SplitButton>",
    ] {
        assert!(parse(&form(&ribbon(item))).is_err(), "{item}");
    }
    let tab = "<RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab>";
    for references in [
        "<Button/>",
        "<Button Command='Save' KeyTip='S'/>",
        "<Button Command='Save' Icon='Save'/>",
        "<ToggleButton Command='Save'/>",
        "<Button Command='Save'/><Button Command='Save'/>",
        "<Button Command='Save'>text</Button>",
    ] {
        assert!(parse(&form(&format!("<Ribbon><Ribbon.QuickAccessToolbar>{references}</Ribbon.QuickAccessToolbar>{tab}</Ribbon>"))).is_err(), "{references}");
    }
    assert!(
        parse(&form(&format!(
            "<Ribbon><Ribbon.QuickAccessToolbar/><Ribbon.QuickAccessToolbar/>{tab}</Ribbon>"
        )))
        .is_err()
    );
}

#[test]
fn ribbon_requires_complete_shape_and_ordinary_selected_tab() {
    for markup in [
        "<Ribbon/>",
        "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'/></Ribbon>",
        "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'><RibbonGroup Id='File' Header='File'/></RibbonTab></Ribbon>",
        "<Ribbon><RibbonTab Header='Home' KeyTip='H'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab></Ribbon>",
        "<Ribbon><RibbonTab Id='Home' Header=' ' KeyTip='H'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab></Ribbon>",
        "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H' Context='Selected'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab></Ribbon>",
    ] {
        assert!(parse(&form(markup)).is_err(), "{markup}");
    }
    let source = ribbon("<Button Command='Save' KeyTip='S'/>");
    assert!(
        parse(&form(&source.replacen(
            "<Ribbon>",
            "<Ribbon SelectedTab='Missing'>",
            1
        )))
        .is_err()
    );
    let contextual = "<RibbonTab Id='Selected' Header='Selected' KeyTip='X' Context='Selection'><RibbonGroup Id='File' Header='File'><Button Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab>";
    let source = source.replace("</Ribbon>", &format!("{contextual}</Ribbon>"));
    assert!(
        parse(&form(&source.replacen(
            "<Ribbon>",
            "<Ribbon SelectedTab='Selected'>",
            1
        )))
        .is_err()
    );
    for priority in ["-1", "65536", "1.5", "NaN", "+1", " 1"] {
        assert!(
            parse(&form(&source.replace(
                "Header='Файл'",
                &format!("Header='Файл' CollapsePriority='{priority}'")
            )))
            .is_err(),
            "{priority}"
        );
    }
}

#[test]
fn ribbon_keytips_and_ids_are_validated_in_their_scopes() {
    for second in ["S", "s", "SA", "", "Ё", "A B", "A-"] {
        let first = "<Button Command='Save' KeyTip='S'/>";
        let second = format!("<Button Command='Other' KeyTip='{second}'/>");
        assert!(
            parse(&form(&ribbon(&format!("{first}{second}")))).is_err(),
            "{second}"
        );
    }
    for tip in ["", "1", "9A", "Ё", "H-", "H H"] {
        let source = ribbon("<Button Command='Save' KeyTip='S'/>")
            .replace("KeyTip='h'", &format!("KeyTip='{tip}'"));
        assert!(parse(&form(&source)).is_err(), "{tip}");
    }
    let group = "<RibbonGroup Id='A' Header='A'><Button Command='Save' KeyTip='S'/></RibbonGroup>";
    let duplicates = format!(
        "<Ribbon><RibbonTab Id='Home' Header='Home' KeyTip='H'>{group}{group}</RibbonTab></Ribbon>"
    );
    assert!(parse(&form(&duplicates)).is_err());
    let prefix_across_groups = duplicates
        .replacen(
            "</RibbonGroup><RibbonGroup Id='A'",
            "</RibbonGroup><RibbonGroup Id='B'",
            1,
        )
        .replacen(
            "Command='Save' KeyTip='S'/></RibbonGroup></RibbonTab>",
            "Command='Other' KeyTip='SA'/></RibbonGroup></RibbonTab>",
            1,
        );
    assert!(
        parse(&form(&prefix_across_groups))
            .unwrap_err()
            .message
            .contains("KeyTips")
    );
    let tab = format!("<RibbonTab Id='Home' Header='Home' KeyTip='H'>{group}</RibbonTab>");
    assert!(parse(&form(&format!("<Ribbon>{tab}{tab}</Ribbon>"))).is_err());
    assert!(
        parse(&form(&format!(
            "<Ribbon>{tab}{}</Ribbon>",
            tab.replace("Id='Home'", "Id='Other'")
                .replace("KeyTip='H'", "KeyTip='HA'")
        )))
        .is_err()
    );
}

#[test]
fn ribbon_namespace_and_one_per_window_boundaries_are_enforced() {
    let source = ribbon("<Button Command='Save' KeyTip='S'/>");
    assert!(
        parse(&form(&format!(
            "<StackPanel>{source}<StackPanel>{source}</StackPanel></StackPanel>"
        )))
        .unwrap_err()
        .message
        .contains("at most one Ribbon")
    );
    for replacement in [
        "<RibbonTab xmlns='' Id='Home'",
        "<RibbonTab xmlns='urn:other' Id='Home'",
    ] {
        assert!(parse(&form(&source.replace("<RibbonTab Id='Home'", replacement))).is_err());
    }
    assert!(
        parse(&form(
            &source.replace("<Button Command", "<Button xmlns='' Command")
        ))
        .is_err()
    );
    assert!(
        parse(&form(&source.replace(
            "<Ribbon>",
            "<Ribbon xmlns:y='urn:other' y:Name='R'>"
        )))
        .is_err()
    );
    let aliases = source
        .replace("<Ribbon>", "<ui:Ribbon>")
        .replace("</Ribbon>", "</ui:Ribbon>")
        .replace("<RibbonTab", "<ui:RibbonTab")
        .replace("</RibbonTab>", "</ui:RibbonTab>")
        .replace("<RibbonGroup", "<ui:RibbonGroup")
        .replace("</RibbonGroup>", "</ui:RibbonGroup>")
        .replace("<Button", "<ui:Button");
    assert!(
        parse(&format!(
            "<Window xmlns='{UI_NAMESPACE}' xmlns:ui='{UI_NAMESPACE}'>{aliases}</Window>"
        ))
        .is_ok()
    );
}

#[test]
fn ribbon_count_depth_and_tooltip_limits_accept_exact_boundaries() {
    let items = (0..MAX_ELEMENTS - 3)
        .map(|index| format!("<Button Command='Save' KeyTip='{index:03}'/>"))
        .collect::<String>();
    assert!(parse(&form(&ribbon(&items))).is_ok());
    assert!(
        parse(&form(&ribbon(&format!(
            "{items}<Button Command='Other' KeyTip='XXX'/>"
        ))))
        .is_err()
    );
    let mut nested =
        ribbon("<SplitButton Command='Save' KeyTip='S'><MenuItem Command='Other'/></SplitButton>");
    for _ in 0..MAX_ELEMENT_DEPTH - 5 {
        nested = format!("<StackPanel>{nested}</StackPanel>");
    }
    assert!(parse(&form(&nested)).is_ok());
    assert!(
        parse(&form(&format!("<StackPanel>{nested}</StackPanel>")))
            .unwrap_err()
            .message
            .contains("depth")
    );
    let tip = "Я".repeat(MAX_TOOLTIP_BYTES / 2);
    assert!(
        parse(&form(&ribbon(&format!(
            "<Button Command='Save' KeyTip='S' ToolTip='{tip}'/>"
        ))))
        .is_ok()
    );
    assert!(
        parse(&form(&ribbon(&format!(
            "<Button Command='Save' KeyTip='S' ToolTip='{tip}X'/>"
        ))))
        .unwrap_err()
        .message
        .contains("ToolTip")
    );
}

#[test]
fn manual_ribbon_validation_rejects_structural_errors_before_mount() {
    let source = definition(&ribbon(
        "<Button Command='Save' KeyTip='S'/><SplitButton Command='Export' KeyTip='E'><MenuItem Command='Csv'/></SplitButton>",
    ));
    validate_ribbon(&source).unwrap();
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].items[1].menu.clear();
    assert!(
        validate_ribbon(&invalid)
            .unwrap_err()
            .contains("SplitButton")
    );
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].items[0]
        .menu
        .push(RibbonMenuEntryDefinition::Command("Other".into()));
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].items[1].key_tip = "s".into();
    assert!(validate_ribbon(&invalid).unwrap_err().contains("KeyTips"));
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].items[0].command = "Bad.Command".into();
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].items[0].tool_tip = Some("я".repeat(MAX_TOOLTIP_BYTES));
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source.clone();
    invalid.quick_access = vec!["Save".into(), "Save".into()];
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source.clone();
    invalid.selected_tab = Some("Missing".into());
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source.clone();
    invalid.tabs[0].context = Some("Selected".into());
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source.clone();
    invalid.tabs[0].groups[0].header = " ".into();
    assert!(validate_ribbon(&invalid).is_err());
    let mut invalid = source;
    invalid.tabs[0].groups[0].items = (0..MAX_ELEMENTS)
        .map(|index| RibbonItemDefinition {
            command: "Save".into(),
            key_tip: format!("{index:03}"),
            ..Default::default()
        })
        .collect();
    assert!(validate_ribbon(&invalid).unwrap_err().contains("count"));
}
