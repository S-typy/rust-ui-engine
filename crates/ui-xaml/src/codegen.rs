use crate::{EditBoxDefinition, ElementDefinition, ElementKind, WindowDefinition};

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

fn element(element: &ElementDefinition) -> String {
    let common = &element.common;
    let kind = match &element.kind {
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
            "rust_desktop_ui_xaml::ElementKind::Button(rust_desktop_ui_xaml::ButtonDefinition {{ content: {:?}.into(), command: {} }})",
            button.content,
            optional_string(&button.command),
        ),
        ElementKind::CheckBox(check_box) => format!(
            "rust_desktop_ui_xaml::ElementKind::CheckBox(rust_desktop_ui_xaml::CheckBoxDefinition {{ content: {:?}.into(), is_checked: {} }})",
            check_box.content,
            check_box.is_checked,
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
            }},
            kind: {kind},
        }}",
        name = optional_string(&common.name),
        width = common.width,
        height = common.height,
        margin = common.margin,
        is_enabled = common.is_enabled,
        automation_name = optional_string(&common.automation_name),
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
