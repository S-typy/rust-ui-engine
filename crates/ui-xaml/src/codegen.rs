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
        content: {content},
    }}
}}
",
        title = window.title,
        width = window.width,
        height = window.height,
        padding = window.padding,
        theme = window.theme,
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

fn element(element: &ElementDefinition) -> String {
    let common = &element.common;
    let kind = match &element.kind {
        ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
            let kind = if matches!(element.kind, ElementKind::SplitButton(_)) { "SplitButton" } else { "DropDownButton" };
            let button = &menu.button;
            format!("rust_desktop_ui_xaml::ElementKind::{kind}(rust_desktop_ui_xaml::MenuButtonDefinition {{ button: rust_desktop_ui_xaml::ButtonDefinition {{ content: {:?}.into(), command: {}, style: {}, icon: {}, icon_size: {:?}, icon_spacing: {:?} }}, items: vec![{}] }})",
                button.content, optional_string(&button.command), button_style(&button.style), optional_icon(button.icon), button.icon_size, button.icon_spacing, menu.items.iter().map(menu_entry).collect::<Vec<_>>().join(","))
        }
        ElementKind::StackPanel(panel) => format!(
            "rust_desktop_ui_xaml::ElementKind::StackPanel(rust_desktop_ui_xaml::StackPanelDefinition {{
                orientation: rust_desktop_ui_xaml::Orientation::{orientation:?},
                spacing: {spacing:?},
                padding: {padding:?},
                children: vec![{children}],
            }})",
            orientation = panel.orientation,
            spacing = panel.spacing,
            padding = panel.padding,
            children = panel.children.iter().map(self::element).collect::<Vec<_>>().join(",\n"),
        ),
        ElementKind::TextBox(editor) => format!(
            "rust_desktop_ui_xaml::ElementKind::TextBox({})",
            edit_box(editor),
        ),
        ElementKind::TextBlock(text) => format!(
            "rust_desktop_ui_xaml::ElementKind::TextBlock(rust_desktop_ui_xaml::TextBlockDefinition {{ text: {:?}.into() }})",
            text.text,
        ),
        ElementKind::Button(button) => format!(
            "rust_desktop_ui_xaml::ElementKind::Button(rust_desktop_ui_xaml::ButtonDefinition {{ content: {:?}.into(), command: {}, style: {}, icon: {}, icon_size: {:?}, icon_spacing: {:?} }})",
            button.content,
            optional_string(&button.command),
            button_style(&button.style),
            optional_icon(button.icon), button.icon_size, button.icon_spacing,
        ),
        ElementKind::ToggleButton(button) => format!(
            "rust_desktop_ui_xaml::ElementKind::ToggleButton(rust_desktop_ui_xaml::ToggleButtonDefinition {{ content: {:?}.into(), command: {}, is_checked: {}, style: {}, icon: {}, icon_size: {:?}, icon_spacing: {:?} }})",
            button.content,
            optional_string(&button.command),
            button.is_checked,
            button_style(&button.style),
            optional_icon(button.icon), button.icon_size, button.icon_spacing,
        ),
        ElementKind::CheckBox(check_box) => format!(
            "rust_desktop_ui_xaml::ElementKind::CheckBox(rust_desktop_ui_xaml::CheckBoxDefinition {{ content: {:?}.into(), is_checked: {}, style: {} }})",
            check_box.content,
            check_box.is_checked,
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
        ElementKind::Separator(separator) => format!(
            "rust_desktop_ui_xaml::ElementKind::Separator(rust_desktop_ui_xaml::SeparatorDefinition {{ orientation: rust_desktop_ui_xaml::Orientation::{:?}, thickness: {:?}, foreground: {:?} }})",
            separator.orientation, separator.thickness, separator.foreground,
        ),
    };
    format!(
        "rust_desktop_ui_xaml::ElementDefinition {{
            common: rust_desktop_ui_xaml::CommonProperties {{
                name: {name},
                width: {width:?},
                height: {height:?},
                margin: {margin:?},
                is_enabled: {is_enabled},
                automation_name: {automation_name},
                tool_tip: {tool_tip},
            }},
            kind: {kind},
        }}",
        name = optional_string(&common.name),
        width = common.width,
        height = common.height,
        margin = common.margin,
        is_enabled = common.is_enabled,
        automation_name = optional_string(&common.automation_name),
        tool_tip = optional_string(&common.tool_tip),
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
