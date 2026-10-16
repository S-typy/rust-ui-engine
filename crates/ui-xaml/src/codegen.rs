use crate::{
    ButtonStyleDefinition, EditBoxDefinition, ElementDefinition, ElementKind, IconSymbol,
    MenuEntryDefinition, WindowDefinition,
};

pub(super) fn window(window: &WindowDefinition) -> String {
    format!(
        "\
pub fn window_definition() -> rust_desktop_ui_xaml::WindowDefinition {{
    rust_desktop_ui_xaml::WindowDefinition {{
        title: {title:?}.into(),
        width: {width:?},
        height: {height:?},
        padding: {padding:?},
        theme: rust_desktop_ui_xaml::Theme::{theme:?},
        density: rust_desktop_ui_xaml::ThemeDensity::{density:?},
        accent_color: {accent_color:?},
        title_bar: rust_desktop_ui_xaml::TitleBar::{title_bar:?},
        content: {content},
    }}
}}
",
        title = window.title,
        width = window.width,
        height = window.height,
        padding = window.padding,
        theme = window.theme,
        density = window.density,
        accent_color = window.accent_color,
        title_bar = window.title_bar,
        content = element(&window.content),
    )
}

fn optional_string(value: &Option<String>) -> String {
    match value {
        Some(value) => format!("Some({value:?}.into())"),
        None => "None".into(),
    }
}

fn optional_icon(value: Option<IconSymbol>) -> String {
    match value {
        Some(symbol) => format!("Some(rust_desktop_ui_xaml::IconSymbol::{symbol:?})"),
        None => "None".into(),
    }
}

pub(crate) fn element(element: &ElementDefinition) -> String {
    let common = &element.common;
    let kind = match &element.kind {
        ElementKind::ToolBar(_) | ElementKind::MenuBar(_) | ElementKind::Popup(_) | ElementKind::SplitContainer(_) | ElementKind::Slider(_) | ElementKind::RangeSlider(_) => crate::extras::codegen(&element.kind).unwrap(),
        ElementKind::DockWorkspace(workspace) => format!(
            "rust_desktop_ui_xaml::ElementKind::DockWorkspace(rust_desktop_ui_xaml::DockWorkspaceDefinition {{ root: {} }})",
            dock_node(&workspace.root),
        ),
        ElementKind::Ribbon(ribbon) => format!(
            "rust_desktop_ui_xaml::ElementKind::Ribbon({})",
            ribbon_definition(ribbon),
        ),
        ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
            let kind = if matches!(element.kind, ElementKind::SplitButton(_)) { "SplitButton" } else { "DropDownButton" };
            let button = &menu.button;
            format!("rust_desktop_ui_xaml::ElementKind::{kind}(rust_desktop_ui_xaml::MenuButtonDefinition {{ button: rust_desktop_ui_xaml::ButtonDefinition {{ content: {:?}.into(), command: {}, style: {}, icon: {}, icon_size: {:?}, icon_spacing: {:?} }}, items: vec![{}] }})",
                button.content, optional_string(&button.command), button_style(&button.style), optional_icon(button.icon), button.icon_size, button.icon_spacing, menu.items.iter().map(menu_entry).collect::<Vec<_>>().join(","))
        }
        ElementKind::StackPanel(panel) | ElementKind::WrapPanel(panel) | ElementKind::Overlay(panel) => format!(
            "rust_desktop_ui_xaml::ElementKind::{kind}(rust_desktop_ui_xaml::StackPanelDefinition {{
                orientation: rust_desktop_ui_xaml::Orientation::{orientation:?},
                spacing: {spacing:?},
                padding: {padding:?},
                surface: rust_desktop_ui_xaml::SurfaceKindDefinition::{surface:?},
                children: vec![{children}],
            }})",
            orientation = panel.orientation,
            spacing = panel.spacing,
            padding = panel.padding,
            surface = panel.surface,
            kind = if matches!(element.kind, ElementKind::WrapPanel(_)) { "WrapPanel" } else if matches!(element.kind, ElementKind::Overlay(_)) { "Overlay" } else { "StackPanel" },
            children = panel.children.iter().map(self::element).collect::<Vec<_>>().join(",\n"),
        ),
        ElementKind::TextBox(editor) => format!(
            "rust_desktop_ui_xaml::ElementKind::TextBox({})",
            edit_box(editor),
        ),
        ElementKind::TextBlock(text) => format!(
            "rust_desktop_ui_xaml::ElementKind::TextBlock(rust_desktop_ui_xaml::TextBlockDefinition {{ text: {:?}.into(), font_size: {:?}, font_weight: {:?}, foreground: {:?}, tone: rust_desktop_ui_xaml::TextToneDefinition::{:?}, wrap: {}, trimming: rust_desktop_ui_xaml::TextTrimmingDefinition::{:?}, max_lines: {:?}, inlines: vec![{}] }})",
            text.text, text.font_size, text.font_weight, text.foreground, text.tone, text.wrap, text.trimming, text.max_lines, text.inlines.iter().map(|s| format!("rust_desktop_ui_xaml::InlineDefinition {{ text: {:?}.into(), font_size: {:?}, font_weight: {:?}, foreground: {:?} }}", s.text, s.font_size, s.font_weight, s.foreground)).collect::<Vec<_>>().join(","),
        ),
        ElementKind::Button(button) => format!(
            "rust_desktop_ui_xaml::ElementKind::Button(rust_desktop_ui_xaml::ButtonDefinition {{ content: {:?}.into(), command: {}, style: {}, icon: {}, icon_size: {:?}, icon_spacing: {:?} }})",
            button.content,
            optional_string(&button.command),
            button_style(&button.style),
            optional_icon(button.icon), button.icon_size, button.icon_spacing,
        ),
        ElementKind::ToggleButton(button) => format!(
            "rust_desktop_ui_xaml::ElementKind::ToggleButton(rust_desktop_ui_xaml::ToggleButtonDefinition {{ content: {:?}.into(), command: {}, is_checked: {}, is_three_state: {}, is_indeterminate: {}, style: {}, icon: {}, icon_size: {:?}, icon_spacing: {:?} }})",
            button.content,
            optional_string(&button.command),
            button.is_checked, button.is_three_state, button.is_indeterminate,
            button_style(&button.style),
            optional_icon(button.icon), button.icon_size, button.icon_spacing,
        ),
        ElementKind::CheckBox(check_box) | ElementKind::Switch(check_box) => format!(
            "rust_desktop_ui_xaml::ElementKind::{}(rust_desktop_ui_xaml::CheckBoxDefinition {{ content: {:?}.into(), is_checked: {}, is_three_state: {}, is_indeterminate: {}, style: {} }})",
            if matches!(element.kind, ElementKind::Switch(_)) { "Switch" } else { "CheckBox" }, check_box.content,
            check_box.is_checked, check_box.is_three_state, check_box.is_indeterminate,
            button_style(&check_box.style),
        ),
        ElementKind::RadioButton(radio) => format!(
            "rust_desktop_ui_xaml::ElementKind::RadioButton(rust_desktop_ui_xaml::RadioButtonDefinition {{ content: {:?}.into(), group_name: {}, is_checked: {}, style: {} }})",
            radio.content,
            optional_string(&radio.group_name),
            radio.is_checked,
            button_style(&radio.style),
        ),
        ElementKind::Icon(icon) => format!(
            "rust_desktop_ui_xaml::ElementKind::Icon(rust_desktop_ui_xaml::IconDefinition {{ symbol: rust_desktop_ui_xaml::IconSymbol::{:?}, foreground: {:?} }})",
            icon.symbol, icon.foreground,
        ),
        ElementKind::Image(image) => format!(
            "rust_desktop_ui_xaml::ElementKind::Image(rust_desktop_ui_xaml::ImageDefinition {{ source: {:?}.into(), stretch: rust_desktop_ui_xaml::ImageStretchDefinition::{:?}, sampling: rust_desktop_ui_xaml::ImageSamplingDefinition::{:?}, opacity: {:?}, foreground: {:?} }})",
            image.source, image.stretch, image.sampling, image.opacity, image.foreground,
        ),
        ElementKind::Separator(separator) => format!(
            "rust_desktop_ui_xaml::ElementKind::Separator(rust_desktop_ui_xaml::SeparatorDefinition {{ orientation: rust_desktop_ui_xaml::Orientation::{:?}, thickness: {:?}, foreground: {:?} }})",
            separator.orientation, separator.thickness, separator.foreground,
        ),
        other => form_kind(other),
    };
    format!(
        "rust_desktop_ui_xaml::ElementDefinition {{
            common: rust_desktop_ui_xaml::CommonProperties {{
                control_template: {control_template},
                name: {name},
                rich_tool_tip: {rich_tool_tip}, is_link: {is_link}, repeat_delay_ms: {repeat_delay_ms:?}, repeat_interval_ms: {repeat_interval_ms},
                width: {width:?},
                height: {height:?},
                margin: {margin:?},
                is_enabled: {is_enabled},
                visibility: rust_desktop_ui_xaml::Visibility::{visibility:?},
                automation_name: {automation_name},
                tool_tip: {tool_tip},
                min_width: {min_width:?}, min_height: {min_height:?},
                max_width: {max_width:?}, max_height: {max_height:?},
                horizontal_alignment: rust_desktop_ui_xaml::Alignment::{horizontal_alignment:?},
                vertical_alignment: rust_desktop_ui_xaml::Alignment::{vertical_alignment:?},
                grid_row: {grid_row}, grid_column: {grid_column},
                grid_row_span: {grid_row_span}, grid_column_span: {grid_column_span},
                dock: rust_desktop_ui_xaml::DockSide::{dock:?},
                data_context: {data_context}, bindings: {bindings},
            }},
            kind: {kind},
        }}",
        name = optional_string(&common.name),
        control_template = common.control_template.as_ref().map_or("None".into(), |template| format!("Some(Box::new(rust_desktop_ui_xaml::ControlTemplateDefinition {{ target: rust_desktop_ui_xaml::ControlTemplateTarget::{:?}, content_part: {:?}.into(), visual: Box::new({}) }}))", template.target, template.content_part, self::element(&template.visual))),
        rich_tool_tip = common.rich_tool_tip.as_ref().map_or("None".into(), |tip| format!("Some(rust_desktop_ui_xaml::RichToolTipDefinition {{ header: {:?}.into(), icon: {}, image: {}, image_description: {:?}.into() }})", tip.header, optional_icon(tip.icon), optional_string(&tip.image), tip.image_description)),
        is_link = common.is_link, repeat_delay_ms = common.repeat_delay_ms, repeat_interval_ms = common.repeat_interval_ms,
        width = common.width,
        height = common.height,
        margin = common.margin,
        is_enabled = common.is_enabled,
        visibility = common.visibility,
        automation_name = optional_string(&common.automation_name),
        tool_tip = optional_string(&common.tool_tip),
        min_width = common.min_width,
        min_height = common.min_height,
        max_width = common.max_width,
        max_height = common.max_height,
        horizontal_alignment = common.horizontal_alignment,
        vertical_alignment = common.vertical_alignment,
        grid_row = common.grid_row,
        grid_column = common.grid_column,
        grid_row_span = common.grid_row_span,
        grid_column_span = common.grid_column_span,
        dock = common.dock,
        data_context = optional_string(&common.data_context),
        bindings = bindings(&common.bindings),
    )
}

fn bindings(bindings: &std::collections::BTreeMap<String, crate::BindingDefinition>) -> String {
    format!("std::collections::BTreeMap::from([{}])", bindings.iter().map(|(property,b)| format!(
        "({property:?}.into(), rust_desktop_ui_xaml::BindingDefinition {{ source_line: {}, source_column: {}, path: {:?}.into(), element_name: {}, mode: rust_desktop_ui_xaml::BindingMode::{:?}, update_source_trigger: rust_desktop_ui_xaml::UpdateSourceTrigger::{:?}, converter: {}, fallback_value: {}, inputs: vec![{}], expression: {} }})",
        b.source_line, b.source_column, b.path, optional_string(&b.element_name), b.mode, b.update_source_trigger, optional_string(&b.converter), optional_string(&b.fallback_value), b.inputs.iter().map(|i| format!("rust_desktop_ui_xaml::BindingInputDefinition {{ path: {:?}.into(), element_name: {} }}", i.path, optional_string(&i.element_name))).collect::<Vec<_>>().join(","), optional_string(&b.expression)
    )).collect::<Vec<_>>().join(","))
}

fn selection(s: &crate::SelectionDefinition) -> String {
    let items = s.items.iter().map(|i| format!("rust_desktop_ui_xaml::SelectionItemDefinition {{ value: {:?}.into(), display: {:?}.into(), is_enabled: {} }}", i.value, i.display, i.is_enabled)).collect::<Vec<_>>().join(",");
    let selected = s
        .selected_values
        .iter()
        .map(|s| format!("{s:?}.into()"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "rust_desktop_ui_xaml::SelectionDefinition {{ items: vec![{items}], selected_value: {}, display_member_path: {}, value_member_path: {}, placeholder: {:?}.into(), is_read_only: {}, is_editable: {}, selection_mode: rust_desktop_ui_xaml::SelectionMode::{:?}, selected_values: vec![{selected}] }}",
        optional_string(&s.selected_value),
        optional_string(&s.display_member_path),
        optional_string(&s.value_member_path),
        s.placeholder,
        s.is_read_only,
        s.is_editable,
        s.selection_mode
    )
}

fn form_kind(kind: &ElementKind) -> String {
    let (name, definition) = match kind {
        ElementKind::GridControl(grid) => ("GridControl", format!("rust_desktop_ui_xaml::GridControlDefinition {{ data_source: {:?}.into(), columns: vec![{}], is_read_only: {}, filter_text: {:?}.into(), selected_value: {} }}", grid.data_source, grid.columns.iter().map(|column| format!("rust_desktop_ui_xaml::GridColumnDefinition {{ field: {:?}.into(), header: {:?}.into(), width: {:?}, min_width: {:?}, max_width: {:?}, is_pinned: {}, is_read_only: {} }}", column.field, column.header, column.width, column.min_width, column.max_width, column.is_pinned, column.is_read_only)).collect::<Vec<_>>().join(","), grid.is_read_only, grid.filter_text, optional_string(&grid.selected_value))),
        ElementKind::StatusBar(bar) | ElementKind::RibbonStatusBar(bar) => (if matches!(kind, ElementKind::StatusBar(_)) { "StatusBar" } else { "RibbonStatusBar" }, format!("rust_desktop_ui_xaml::StatusBarDefinition {{ is_customizable: {}, show_resize_grip: {}, items: vec![{}] }}", bar.is_customizable, bar.show_resize_grip, bar.items.iter().map(element).collect::<Vec<_>>().join(","))),
        ElementKind::StatusBarItem(item) => ("StatusBarItem", format!("rust_desktop_ui_xaml::StatusBarItemDefinition {{ key: {}, label: {}, can_hide: {}, placement: rust_desktop_ui_xaml::StatusBarPlacement::{:?}, child: {} }}", optional_string(&item.key), optional_string(&item.label), item.can_hide, item.placement, optional_child(item.child.as_deref()))),
        ElementKind::ProgressBar(progress) | ElementKind::BusyIndicator(progress) => (if matches!(kind, ElementKind::BusyIndicator(_)) { "BusyIndicator" } else { "ProgressBar" }, format!("rust_desktop_ui_xaml::ProgressBarDefinition {{ minimum: {:?}, maximum: {:?}, value: {:?}, is_indeterminate: {}, reduced_motion: {} }}", progress.minimum, progress.maximum, progress.value, progress.is_indeterminate, progress.reduced_motion)),
        ElementKind::ZoomControl(zoom) => ("ZoomControl", format!("rust_desktop_ui_xaml::ZoomControlDefinition {{ minimum: {:?}, maximum: {:?}, value: {:?}, step: {:?} }}", zoom.minimum, zoom.maximum, zoom.value, zoom.step)),
        ElementKind::GroupBox(g) => ("GroupBox", format!("rust_desktop_ui_xaml::GroupBoxDefinition {{ header: {:?}.into(), appearance: rust_desktop_ui_xaml::GroupBoxAppearanceDefinition::{:?}, header_style: rust_desktop_ui_xaml::GroupHeaderStyleDefinition::{:?}, header_icon: {}, show_header: {}, corner_radius: {:?}, header_foreground: {:?}, header_background: {:?}, padding: {:?}, is_collapsible: {}, is_expanded: {}, child: {}, header_content: {} }}", g.header, g.appearance, g.header_style, optional_icon(g.header_icon), g.show_header, g.corner_radius, g.header_foreground, g.header_background, g.padding, g.is_collapsible, g.is_expanded, optional_child(g.child.as_deref()), optional_child(g.header_content.as_deref()))),
        ElementKind::Expander(e) => ("Expander", format!("rust_desktop_ui_xaml::ExpanderDefinition {{ header: {:?}.into(), appearance: rust_desktop_ui_xaml::GroupBoxAppearanceDefinition::{:?}, header_style: rust_desktop_ui_xaml::GroupHeaderStyleDefinition::{:?}, header_icon: {}, show_header: {}, corner_radius: {:?}, header_foreground: {:?}, header_background: {:?}, padding: {:?}, is_expanded: {}, child: {}, header_content: {}, expand_direction: rust_desktop_ui_xaml::ExpandDirectionDefinition::{:?}, animation_duration_ms: {} }}", e.header, e.appearance, e.header_style, optional_icon(e.header_icon), e.show_header, e.corner_radius, e.header_foreground, e.header_background, e.padding, e.is_expanded, optional_child(e.child.as_deref()), optional_child(e.header_content.as_deref()), e.expand_direction, e.animation_duration_ms)),
        ElementKind::TabbedGroup(g) => ("TabbedGroup", format!("rust_desktop_ui_xaml::TabbedGroupDefinition {{ selected_index: {}, padding: {:?}, tabs: vec![{}] }}", g.selected_index, g.padding, g.tabs.iter().map(|tab| format!("rust_desktop_ui_xaml::GroupTabDefinition {{ header: {:?}.into(), header_icon: {}, is_enabled: {}, child: {}, header_content: {} }}", tab.header, optional_icon(tab.header_icon), tab.is_enabled, optional_child(tab.child.as_deref()), optional_child(tab.header_content.as_deref()))).collect::<Vec<_>>().join(","))),
        ElementKind::RadioGroup(s) => ("RadioGroup", selection(s)),
        ElementKind::ListBox(s) => ("ListBox", selection(s)),
        ElementKind::ComboBox(s) => ("ComboBox", selection(s)),
        ElementKind::LookupEdit(l) => ("LookupEdit", format!("rust_desktop_ui_xaml::LookupDefinition {{ selection: {}, columns: vec![{}] }}", selection(&l.selection), l.columns.iter().map(|c| format!("rust_desktop_ui_xaml::LookupColumnDefinition {{ field: {:?}.into(), header: {:?}.into(), width: {:?} }}", c.field, c.header, c.width)).collect::<Vec<_>>().join(","))),
        ElementKind::ButtonEdit(e) => ("ButtonEdit", format!("rust_desktop_ui_xaml::ButtonEditDefinition {{ editor: {}, buttons: vec![{}] }}", edit_box(&e.editor), e.buttons.iter().map(|b| format!("rust_desktop_ui_xaml::EditorButtonDefinition {{ command: {:?}.into(), icon: {}, content: {:?}.into(), tool_tip: {} }}", b.command, optional_icon(b.icon), b.content, optional_string(&b.tool_tip))).collect::<Vec<_>>().join(","))),
        ElementKind::SpinBox(e) => ("SpinBox", format!("rust_desktop_ui_xaml::SpinBoxDefinition {{ editor: {}, value: {:?}, minimum: {:?}, maximum: {:?}, increment: {:?} }}", edit_box(&e.editor), e.value, e.minimum, e.maximum, e.increment)),
        ElementKind::DateEdit(e) | ElementKind::TimeEdit(e) => (if matches!(kind, ElementKind::DateEdit(_)) { "DateEdit" } else { "TimeEdit" }, format!("rust_desktop_ui_xaml::TemporalEditDefinition {{ editor: {}, value: {}, minimum: {}, maximum: {}, format: {:?}.into() }}", edit_box(&e.editor), optional_string(&e.value), optional_string(&e.minimum), optional_string(&e.maximum), e.format)),
        ElementKind::Grid(g) => {
            let tracks = |tracks: &[crate::GridLength]| tracks.iter().map(|track| format!("rust_desktop_ui_xaml::GridLength::{track:?}")).collect::<Vec<_>>().join(",");
            ("Grid", format!("rust_desktop_ui_xaml::GridDefinition {{ rows: vec![{}], columns: vec![{}], spacing: {:?}, padding: {:?}, surface: rust_desktop_ui_xaml::SurfaceKindDefinition::{:?}, children: vec![{}] }}", tracks(&g.rows), tracks(&g.columns), g.spacing, g.padding, g.surface, g.children.iter().map(element).collect::<Vec<_>>().join(",")))
        }
        ElementKind::Border(b) => ("Border", format!("rust_desktop_ui_xaml::BorderDefinition {{ padding: {:?}, background: {:?}, border_brush: {:?}, border_thickness: {:?}, corner_radius: {:?}, child: Box::new({}) }}", b.padding, b.background, b.border_brush, b.border_thickness, b.corner_radius, element(&b.child))),
        ElementKind::ScrollViewer(s) => ("ScrollViewer", format!("rust_desktop_ui_xaml::ScrollViewerDefinition {{ horizontal_scroll_bar_visibility: rust_desktop_ui_xaml::ScrollBarVisibility::{:?}, vertical_scroll_bar_visibility: rust_desktop_ui_xaml::ScrollBarVisibility::{:?}, child: Box::new({}) }}", s.horizontal_scroll_bar_visibility, s.vertical_scroll_bar_visibility, element(&s.child))),
        ElementKind::DockPanel(p) => ("DockPanel", format!("rust_desktop_ui_xaml::DockPanelDefinition {{ last_child_fill: {}, padding: {:?}, children: vec![{}] }}", p.last_child_fill, p.padding, p.children.iter().map(element).collect::<Vec<_>>().join(","))),
        _ => unreachable!("ordinary content is generated by element"),
    };
    format!("rust_desktop_ui_xaml::ElementKind::{name}({definition})")
}

fn optional_child(child: Option<&ElementDefinition>) -> String {
    child.map_or_else(
        || "None".into(),
        |child| format!("Some(Box::new({}))", element(child)),
    )
}

fn dock_node(node: &crate::DockNodeDefinition) -> String {
    match node {
        crate::DockNodeDefinition::Split { orientation, ratio, first, second } => format!(
            "rust_desktop_ui_xaml::DockNodeDefinition::Split {{ orientation: rust_desktop_ui_xaml::Orientation::{orientation:?}, ratio: {ratio:?}, first: Box::new({}), second: Box::new({}) }}",
            dock_node(first), dock_node(second),
        ),
        crate::DockNodeDefinition::Group { selected_pane, panes } => format!(
            "rust_desktop_ui_xaml::DockNodeDefinition::Group {{ selected_pane: {}, panes: vec![{}] }}",
            optional_string(selected_pane), panes.iter().map(|pane| format!(
                "rust_desktop_ui_xaml::DockPaneDefinition {{ id: {:?}.into(), header: {:?}.into(), can_close: {}, min_width: {:?}, min_height: {:?}, content: Box::new({}) }}",
                pane.id, pane.header, pane.can_close, pane.min_width, pane.min_height,
                element(&pane.content),
            )).collect::<Vec<_>>().join(",\n"),
        ),
    }
}

fn ribbon_definition(ribbon: &crate::RibbonDefinition) -> String {
    let tabs = ribbon.tabs.iter().map(|tab| {
        let groups = tab.groups.iter().map(|group| {
            let items = group.items.iter().map(|item| format!(
                "rust_desktop_ui_xaml::RibbonItemDefinition {{ command: {:?}.into(), key_tip: {:?}.into(), size: rust_desktop_ui_xaml::RibbonItemSize::{:?}, min_size: rust_desktop_ui_xaml::RibbonItemSize::{:?}, kind: rust_desktop_ui_xaml::RibbonItemKindDefinition::{:?}, icon: {}, tool_tip: {}, menu: {} }}",
                item.command, item.key_tip, item.size, item.min_size, item.kind, optional_icon(item.icon), optional_string(&item.tool_tip), ribbon_menu_entries(&item.menu),
            )).collect::<Vec<_>>().join(",");
            format!(
                "rust_desktop_ui_xaml::RibbonGroupDefinition {{ id: {:?}.into(), header: {:?}.into(), collapse_priority: {}, items: vec![{}] }}",
                group.id, group.header, group.collapse_priority, items,
            )
        }).collect::<Vec<_>>().join(",");
        format!(
            "rust_desktop_ui_xaml::RibbonTabDefinition {{ id: {:?}.into(), header: {:?}.into(), key_tip: {:?}.into(), context: {}, groups: vec![{}] }}",
            tab.id, tab.header, tab.key_tip, optional_string(&tab.context), groups,
        )
    }).collect::<Vec<_>>().join(",");
    format!(
        "rust_desktop_ui_xaml::RibbonDefinition {{ selected_tab: {}, is_minimized: {}, quick_access: {}, tabs: vec![{}] }}",
        optional_string(&ribbon.selected_tab),
        ribbon.is_minimized,
        string_vec(&ribbon.quick_access),
        tabs,
    )
}

fn ribbon_menu_entries(entries: &[crate::RibbonMenuEntryDefinition]) -> String {
    let entries = entries.iter().map(|entry| match entry {
        crate::RibbonMenuEntryDefinition::Command(command) => format!(
            "rust_desktop_ui_xaml::RibbonMenuEntryDefinition::Command({command:?}.into())"
        ),
        crate::RibbonMenuEntryDefinition::Submenu { header, items } => format!(
            "rust_desktop_ui_xaml::RibbonMenuEntryDefinition::Submenu {{ header: {header:?}.into(), items: {} }}",
            ribbon_menu_entries(items),
        ),
        crate::RibbonMenuEntryDefinition::Separator =>
            "rust_desktop_ui_xaml::RibbonMenuEntryDefinition::Separator".into(),
    }).collect::<Vec<_>>().join(",");
    format!("vec![{entries}]")
}

fn string_vec(values: &[String]) -> String {
    format!(
        "vec![{}]",
        values
            .iter()
            .map(|value| format!("{value:?}.into()"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn menu_entry(entry: &MenuEntryDefinition) -> String {
    match entry {
        MenuEntryDefinition::Separator => {
            "rust_desktop_ui_xaml::MenuEntryDefinition::Separator".into()
        }
        MenuEntryDefinition::Item(item) => format!(
            "rust_desktop_ui_xaml::MenuEntryDefinition::Item(rust_desktop_ui_xaml::MenuItemDefinition {{ header: {:?}.into(), command: {}, is_enabled: {}, items: vec![{}] }})",
            item.header,
            optional_string(&item.command),
            item.is_enabled,
            item.items
                .iter()
                .map(menu_entry)
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn button_style(style: &ButtonStyleDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::ButtonStyleDefinition {{
            font_size: {font_size:?},
            padding: {padding:?},
            corner_radius: {corner_radius:?},
            border_thickness: {border_thickness:?},
            background: {background:?},
            foreground: {foreground:?},
            border_brush: {border_brush:?},
            focus_brush: {focus_brush:?},
        }}",
        font_size = style.font_size,
        padding = style.padding,
        corner_radius = style.corner_radius,
        border_thickness = style.border_thickness,
        background = style.background,
        foreground = style.foreground,
        border_brush = style.border_brush,
        focus_brush = style.focus_brush,
    )
}

fn edit_box(edit: &EditBoxDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::EditBoxDefinition {{
            text: {text:?}.into(),
            placeholder: {placeholder:?}.into(),
            is_read_only: {read_only},
            max_length: {max_length:?},
            is_required: {is_required},
            validation_message: {validation_message:?}.into(),
            commit_on_lost_focus: {commit_on_lost_focus},
            font_size: {font_size:?},
            padding: {padding:?},
            corner_radius: {corner_radius:?},
            border_thickness: {border_thickness:?},
            background: {background:?},
            foreground: {foreground:?},
            border_brush: {border_brush:?},
            focus_brush: {focus_brush:?},
            placeholder_foreground: {placeholder_foreground:?},
            selection_brush: {selection_brush:?},
            validation_brush: {validation_brush:?},
        }}",
        text = edit.text,
        placeholder = edit.placeholder,
        read_only = edit.is_read_only,
        max_length = edit.max_length,
        is_required = edit.is_required,
        validation_message = edit.validation_message,
        commit_on_lost_focus = edit.commit_on_lost_focus,
        font_size = edit.font_size,
        padding = edit.padding,
        corner_radius = edit.corner_radius,
        border_thickness = edit.border_thickness,
        background = edit.background,
        foreground = edit.foreground,
        border_brush = edit.border_brush,
        focus_brush = edit.focus_brush,
        placeholder_foreground = edit.placeholder_foreground,
        selection_brush = edit.selection_brush,
        validation_brush = edit.validation_brush,
    )
}
