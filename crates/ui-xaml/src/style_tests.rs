use crate::*;

fn form(body: &str) -> String {
    format!(
        r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">{body}</Window>"#
    )
}

#[test]
fn lexical_resources_selectors_inheritance_and_local_precedence() {
    let window = parse(&form(r##"
        <Window.Resources>
            <Color x:Key="Ink" Value="#112233"/>
            <Double x:Key="Size" Value="15"/>
            <Style x:Key="Base" TargetType="Button"><Setter Property="FontSize" Value="{StaticResource Size}"/><Setter Property="Foreground" Value="{StaticResource Ink}"/></Style>
            <Style x:Key="Explicit" TargetType="Button" BasedOn="{StaticResource Base}"><Setter Property="FontSize" Value="19"/></Style>
            <Style TargetType="Button"><Setter Property="FontSize" Value="12"/></Style>
            <Style TargetType="Button" Selector="Button.primary"><Setter Property="FontSize" Value="14"/></Style>
            <Style TargetType="Button" Selector="Button#Named"><Setter Property="FontSize" Value="16"/></Style>
        </Window.Resources>
        <StackPanel>
            <Button Content="implicit"/>
            <Button Content="class" Classes="primary extra"/>
            <Button x:Name="Named" Content="name" Classes="primary"/>
            <Button Content="explicit" Classes="primary" Style="{StaticResource Explicit}"/>
            <Button Content="local" Style="{StaticResource Explicit}" FontSize="23"/>
            <StackPanel>
                <StackPanel.Resources><Double x:Key="Size" Value="27"/><Style TargetType="Button"><Setter Property="FontSize" Value="{StaticResource Size}"/></Style></StackPanel.Resources>
                <Button Content="inner"/><Button Content="lexical base" Style="{StaticResource Base}"/>
            </StackPanel>
        </StackPanel>"##)).unwrap();
    let ElementKind::StackPanel(panel) = &window.content.kind else {
        panic!()
    };
    let size = |element: &ElementDefinition| {
        let ElementKind::Button(button) = &element.kind else {
            panic!()
        };
        button.style.font_size.unwrap()
    };
    assert_eq!(
        panel.children[..5].iter().map(size).collect::<Vec<_>>(),
        [12., 14., 16., 19., 23.]
    );
    let ElementKind::StackPanel(inner) = &panel.children[5].kind else {
        panic!()
    };
    assert_eq!(
        inner.children.iter().map(size).collect::<Vec<_>>(),
        [27., 15.]
    );
}

#[test]
fn compiled_templates_have_isolated_required_parts_and_live_bindings() {
    let source = form(
        r#"
        <Window.Resources>
            <ControlTemplate x:Key="Action" TargetType="Button">
                <StackPanel Orientation="Horizontal"><Icon Symbol="Check"/><ContentPresenter x:Name="PART_Content" Foreground="{TemplateBinding Foreground}"/></StackPanel>
            </ControlTemplate>
            <HeaderTemplate x:Key="Header"><TextBlock Text="{TemplateBinding Header}" FontWeight="600"/></HeaderTemplate>
            <DataTemplate x:Key="Record"><Border Padding="8"><TextBox x:Name="Editor" Text="{Binding Name}"/></Border></DataTemplate>
        </Window.Resources>
        <StackPanel>
            <Button x:Name="One" Content="First" Template="{StaticResource Action}"/>
            <Button x:Name="Two" Content="Second" Template="{StaticResource Action}"/>
            <GroupBox Header="Record" HeaderTemplate="{StaticResource Header}"><TextBox Text="draft"/></GroupBox>
            <ContentPresenter ContentTemplate="{StaticResource Record}" DataContext="First"/>
            <ContentPresenter ContentTemplate="{StaticResource Record}" DataContext="Second"/>
        </StackPanel>"#,
    );
    let window = parse(&source).unwrap();
    let ElementKind::StackPanel(panel) = &window.content.kind else {
        panic!()
    };
    let first = panel.children[0].common.control_template.as_ref().unwrap();
    let second = panel.children[1].common.control_template.as_ref().unwrap();
    assert_ne!(first.content_part, second.content_part);
    validate_control_template(first).unwrap();
    let ElementKind::StackPanel(visual) = &first.visual.kind else {
        panic!()
    };
    assert!(
        matches!(&visual.children[1].kind, ElementKind::TextBlock(text) if text.text == "First")
    );
    let ElementKind::GroupBox(group) = &panel.children[2].kind else {
        panic!()
    };
    assert!(
        matches!(&group.header_content.as_ref().unwrap().kind, ElementKind::TextBlock(text) if text.text == "Record")
    );
    assert_eq!(
        panel.children[3].common.data_context.as_deref(),
        Some("First")
    );
    let ElementKind::Border(first) = &panel.children[3].kind else {
        panic!()
    };
    let ElementKind::Border(second) = &panel.children[4].kind else {
        panic!()
    };
    assert_ne!(
        first.child.children()[0].common.name,
        second.child.children()[0].common.name
    );
    assert_eq!(
        first.child.children()[0].common.bindings["Text"].path,
        "Name"
    );
    assert!(
        compile(&source)
            .unwrap()
            .contains("ControlTemplateDefinition")
    );
}

#[test]
fn resource_and_template_errors_are_atomic_and_located() {
    for (body, expected) in [
        (
            r#"<Window.Resources><String x:Key="A" Value="{StaticResource B}"/><String x:Key="B" Value="{StaticResource A}"/></Window.Resources><Button/>"#,
            "cycle",
        ),
        (
            r#"<Window.Resources><Style x:Key="A" TargetType="Button" BasedOn="{StaticResource B}"/><Style x:Key="B" TargetType="Button" BasedOn="{StaticResource A}"/></Window.Resources><Button/>"#,
            "cycle",
        ),
        (
            r#"<Window.Resources><DataTemplate x:Key="A"><ContentPresenter ContentTemplate="{StaticResource A}"/></DataTemplate></Window.Resources><ContentPresenter ContentTemplate="{StaticResource A}"/>"#,
            "cycle",
        ),
        (
            r#"<Window.Resources><ControlTemplate x:Key="A" TargetType="TextBox"><TextBlock/></ControlTemplate></Window.Resources><Button/>"#,
            "Button or ToggleButton",
        ),
        (
            r#"<Window.Resources><ControlTemplate x:Key="A" TargetType="Button"><TextBlock/></ControlTemplate></Window.Resources><Button/>"#,
            "PART_Content",
        ),
        (
            r#"<Window.Resources><ControlTemplate x:Key="A" TargetType="Button"><StackPanel><ContentPresenter x:Name="PART_Content"/><Button/></StackPanel></ControlTemplate></Window.Resources><Button Template="{StaticResource A}"/>"#,
            "noninteractive",
        ),
        (
            r#"<Window.Resources><DataTemplate x:Key="A"><StackPanel><TextBox x:Name="Same"/><TextBox x:Name="Same"/></StackPanel></DataTemplate></Window.Resources><Button/>"#,
            "duplicate name",
        ),
        (
            r#"<Button Width="{StaticResource Missing}"/>"#,
            "Unknown resource",
        ),
        (
            r#"<Window.Resources><Style TargetType="Button" Selector="Button:hover"/></Window.Resources><Button/>"#,
            "Selector",
        ),
    ] {
        let error = parse(&form(body)).unwrap_err();
        assert!(error.message.contains(expected), "{expected}: {error}");
        assert!(error.line > 0 && error.column > 0);
    }
    let error = parse(&form("\n<Button Width=\"{StaticResource Missing}\"/>\n")).unwrap_err();
    assert_eq!(error.line, 2);
}

#[test]
fn template_setters_keep_declaration_scope_and_owner_binding_names() {
    let window = parse(&form(r#"
        <Window.Resources>
            <ControlTemplate x:Key="Action" TargetType="Button"><ContentPresenter x:Name="PART_Content"/></ControlTemplate>
            <Style x:Key="ActionStyle" TargetType="Button"><Setter Property="Template" Value="{StaticResource Action}"/></Style>
        </Window.Resources>
        <StackPanel>
            <StackPanel.Resources><String x:Key="Action" Value="shadowed"/></StackPanel.Resources>
            <TextBox x:Name="Source" Text="Original"/>
            <Button Style="{StaticResource ActionStyle}" Content="{Binding Path=Text, ElementName=Source}"/>
        </StackPanel>"#)).unwrap();
    let ElementKind::StackPanel(panel) = window.content.kind else {
        panic!()
    };
    let visual = &panel.children[1]
        .common
        .control_template
        .as_ref()
        .unwrap()
        .visual;
    assert_eq!(
        visual.common.bindings["Text"].element_name.as_deref(),
        Some("Source")
    );
}

#[test]
fn resource_limits_unused_errors_and_expansion_budgets_are_checked() {
    for (body, expected) in [
        (
            r#"<Window.Resources><Color x:Key="Bad" Value="red"/></Window.Resources><Button/>"#,
            "Invalid Color",
        ),
        (
            r#"<Window.Resources><Double x:Key="Bad" Value="NaN"/></Window.Resources><Button/>"#,
            "Invalid Double",
        ),
        (
            r#"<Window.Resources><Style x:Key="Bad" TargetType="Unknown"><Setter Property="Width" Value="5"/></Style></Window.Resources><Button/>"#,
            "unsupported content",
        ),
        (
            r#"<Window.Resources><Style x:Key="Bad" TargetType="Button"><Setter Property="Click" Value="Handler"/></Style></Window.Resources><Button/>"#,
            "unsupported Button",
        ),
        (
            r#"<Window.Resources><DataTemplate x:Key="Loop"><ContentPresenter ContentTemplate="{StaticResource Loop}"/></DataTemplate></Window.Resources><Button/>"#,
            "cycle",
        ),
    ] {
        assert!(
            parse(&form(body)).unwrap_err().message.contains(expected),
            "{body}"
        );
    }
    let declarations = (0..257)
        .map(|index| format!(r#"<String x:Key="Key{index}" Value="x"/>"#))
        .collect::<String>();
    assert!(
        parse(&form(&format!(
            "<Window.Resources>{declarations}</Window.Resources><Button/>"
        )))
        .unwrap_err()
        .message
        .contains("256-resource")
    );
    let instances = r#"<ContentPresenter ContentTemplate="{StaticResource Item}"/>"#.repeat(129);
    let source = form(&format!(
        r#"<Window.Resources><DataTemplate x:Key="Item"><Border><TextBlock Text="x"/></Border></DataTemplate></Window.Resources><StackPanel>{instances}</StackPanel>"#
    ));
    assert!(parse(&source).unwrap_err().message.contains("256-element"));
}

#[test]
fn tab_headers_and_status_customization_survive_typed_codegen() {
    let source = form(
        r#"<Window.Resources><HeaderTemplate x:Key="Tab" TargetType="GroupTab"><TextBlock Text="{TemplateBinding Header}"/></HeaderTemplate></Window.Resources><StackPanel><TabbedGroup><GroupTab Header="First" HeaderTemplate="{StaticResource Tab}"><TextBox/></GroupTab></TabbedGroup><StatusBar IsCustomizable="True" ShowResizeGrip="True"><StatusBarItem Key="State" Label="Document status" CanHide="False"><TextBlock Text="Ready"/></StatusBarItem></StatusBar></StackPanel>"#,
    );
    let window = parse(&source).unwrap();
    validate_status_bars(&window.content).unwrap();
    let generated = compile(&source).unwrap();
    assert!(generated.contains("is_customizable: true"));
    assert!(generated.contains("show_resize_grip: true"));
    assert!(generated.contains("can_hide: false"));
    let ElementKind::StackPanel(panel) = window.content.kind else {
        panic!()
    };
    let ElementKind::TabbedGroup(tabs) = &panel.children[0].kind else {
        panic!()
    };
    assert!(tabs.tabs[0].header_content.is_some());
    assert_eq!(panel.children[0].children().len(), 2);
}

#[test]
fn presenter_public_name_does_not_replace_template_root_identity() {
    let window = parse(&form(r#"<Window.Resources><DataTemplate x:Key="T"><StackPanel x:Name="Root"><TextBlock Text="{Binding Path=Width, ElementName=Root}"/></StackPanel></DataTemplate></Window.Resources><ContentPresenter x:Name="Presenter" ContentTemplate="{StaticResource T}"/>"#)).unwrap();
    assert_eq!(window.content.common.name.as_deref(), Some("Presenter"));
    let private = window.content.children()[0];
    assert_eq!(
        private.children()[0].common.bindings["Text"].element_name,
        private.common.name
    );
}

#[test]
fn repeated_large_resources_fail_before_unbounded_expansion() {
    let text = "x".repeat(400_000);
    let source = form(&format!(
        r#"<Window.Resources><String x:Key="Large" Value="{text}"/><Style TargetType="TextBlock"><Setter Property="Text" Value="{{StaticResource Large}}"/></Style></Window.Resources><StackPanel><TextBlock/><TextBlock/><TextBlock/></StackPanel>"#
    ));
    assert!(parse(&source).unwrap_err().message.contains("size limit"));
}

#[test]
fn template_name_rewriting_uses_binding_grammar_and_read_only_projections() {
    let source = form(
        r#"<Window.Resources>
        <DataTemplate x:Key="T"><StackPanel x:Name="Root" Width="100"><TextBlock Text="{Binding Path=Width, ElementName = 'Root'}"/><TextBlock Text="{MultiBinding Paths='#Root.Width;Amount', Expression='$0 + $1'}"/><TextBox Text="{Binding Name}"/></StackPanel></DataTemplate>
        <ControlTemplate x:Key="C" TargetType="Button"><ContentPresenter x:Name="PART_Content"/></ControlTemplate>
        </Window.Resources><StackPanel><ContentPresenter ContentTemplate="{StaticResource T}"/><Button Content="{Binding Caption, Mode=TwoWay, UpdateSourceTrigger=Explicit}" Template="{StaticResource C}"/></StackPanel>"#,
    );
    let definition = parse(&source).unwrap();
    let root = definition.content.children()[0].children()[0];
    assert_eq!(
        root.children()[0].common.bindings["Text"].element_name,
        root.common.name
    );
    assert_eq!(
        root.children()[1].common.bindings["Text"].inputs[0].element_name,
        root.common.name
    );
    assert_eq!(
        root.children()[2].common.bindings["Text"].mode,
        BindingMode::TwoWay
    );
    let owner = definition.content.children()[1];
    assert_eq!(owner.common.bindings["Content"].mode, BindingMode::TwoWay);
    assert_eq!(
        owner
            .common
            .control_template
            .as_ref()
            .unwrap()
            .visual
            .common
            .bindings["Text"]
            .mode,
        BindingMode::OneWay
    );
}

#[test]
fn decorative_template_default_centers_without_overriding_explicit_alignment() {
    for (attribute, expected) in [
        ("", Alignment::Center),
        ("VerticalAlignment='Top'", Alignment::Start),
        ("VerticalAlignment='Stretch'", Alignment::Stretch),
    ] {
        let definition = parse(&form(&format!("<GroupBox><GroupBox.HeaderTemplate><TextBlock Text='Header' {attribute}/></GroupBox.HeaderTemplate></GroupBox>"))).unwrap();
        let ElementKind::GroupBox(group) = definition.content.kind else {
            panic!()
        };
        assert_eq!(
            group.header_content.unwrap().common.vertical_alignment,
            expected
        );
        let definition = parse(&form(&format!("<Window.Resources><HeaderTemplate x:Key='H'><TextBlock Text='Header' {attribute}/></HeaderTemplate></Window.Resources><GroupBox HeaderTemplate='{{StaticResource H}}'/>"))).unwrap();
        let ElementKind::GroupBox(group) = definition.content.kind else {
            panic!()
        };
        assert_eq!(
            group.header_content.unwrap().common.vertical_alignment,
            expected
        );
    }
}
