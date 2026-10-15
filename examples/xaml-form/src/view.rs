//! Mount compiled elements without attaching application-specific behavior.
use rust_desktop_ui_controls::{
    ButtonAppearance, ButtonIcon, ButtonStyle, Controls, EditBoxAppearance, EditBoxStyle,
    EditorSettings, EditorValidation, IconStyle, IconSymbol, MenuEntry, MenuItem, RadioGroupId,
    Role, SeparatorSettings, SplitButton, SurfaceKind, TextStyle, TextTone,
};
use rust_desktop_ui_core::{Axis, Color, Edges, LayoutKind, LayoutStyle, Length, WidgetId};
use rust_desktop_ui_xaml::{ButtonStyleDefinition, ElementDefinition, ElementKind, Orientation};
use std::collections::{HashMap, HashSet};
#[path = "view_bindings.rs"]
mod bindings;
#[path = "view_forms.rs"]
mod forms;

#[derive(Default)]
pub(crate) struct MountedView {
    pub(crate) ribbon: Option<crate::view_ribbon::MountedRibbon>,
    pub(crate) names: HashMap<String, WidgetId>,
    pub(crate) automation_names: HashMap<WidgetId, String>,
    pub(crate) editors: Vec<WidgetId>,
    pub(crate) checks: Vec<WidgetId>,
    pub(crate) toggles: Vec<WidgetId>,
    pub(crate) radios: Vec<WidgetId>,
    pub(crate) commands: Vec<String>,
    pub(crate) roles: HashMap<WidgetId, Role>,
    pub(crate) splits: HashMap<WidgetId, SplitButton>,
    pub(crate) bindings: Option<rust_desktop_ui_controls::ControlBindings>,
    pending_bindings: Vec<bindings::PendingBindings>,
    decorations: Vec<forms::BorderSurface>,
    scrolls: Vec<forms::ScrollLayout>,
    radio_groups: HashMap<RadioGroupKey<WidgetId>, RadioGroupId>,
}

#[derive(Eq, Hash, PartialEq)]
enum RadioGroupKey<Parent> {
    Named(String),
    Parent(Parent),
}

impl MountedView {
    pub(crate) fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
    ) -> Result<Self, String> {
        Self::mount_with_context(
            ui,
            parent,
            element,
            std::rc::Rc::new(std::cell::RefCell::new(
                rust_desktop_ui_controls::BindingContext::new(
                    rust_desktop_ui_controls::DataValue::Object(Default::default()),
                )
                .map_err(err)?,
            )),
        )
    }

    pub(crate) fn mount_with_context(
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        context: std::rc::Rc<std::cell::RefCell<rust_desktop_ui_controls::BindingContext>>,
    ) -> Result<Self, String> {
        Self::mount_with_converters(
            ui,
            parent,
            element,
            context,
            std::rc::Rc::new(std::cell::RefCell::new(
                rust_desktop_ui_controls::BindingConverterRegistry::default(),
            )),
        )
    }

    pub(crate) fn mount_with_converters(
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        context: std::rc::Rc<std::cell::RefCell<rust_desktop_ui_controls::BindingContext>>,
        converters: std::rc::Rc<
            std::cell::RefCell<rust_desktop_ui_controls::BindingConverterRegistry>,
        >,
    ) -> Result<Self, String> {
        let available_width = ui.runtime.bounds(parent).map_or(0.0, |bounds| {
            let padding = ui.runtime.tree().node(parent).unwrap().props.style.padding;
            (bounds.width - padding.left - padding.right).max(0.0)
        });
        validate_tree(ui, element)?;
        validate_ribbon_width(ui, element, available_width)?;
        let mut view = Self::default();
        let content =
            view.mount_element(ui, parent, element, Axis::Vertical, available_width, "")?;
        if element.common.height.is_none()
            && !element.common.bindings.contains_key("Height")
            && element.common.vertical_alignment == rust_desktop_ui_xaml::Alignment::Stretch
            && matches!(
                element.kind,
                ElementKind::Grid(_)
                    | ElementKind::Border(_)
                    | ElementKind::ScrollViewer(_)
                    | ElementKind::DockPanel(_)
            )
        {
            let mut style = ui.runtime.tree().node(content).unwrap().props.style.clone();
            style.height = Length::Percent(1.0);
            ui.runtime
                .apply(rust_desktop_ui_core::Mutation::SetStyle {
                    node: content,
                    style,
                })
                .map_err(err)?;
        }
        view.initialize_bindings(ui, context, converters)?;
        Ok(view)
    }

    pub(crate) fn find_name(&self, name: &str) -> Option<WidgetId> {
        self.names.get(name).copied()
    }

    fn mount_element(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        parent_axis: Axis,
        available_width: f32,
        inherited_context: &str,
    ) -> Result<WidgetId, String> {
        let common = &element.common;
        let context = bindings::context_path(inherited_context, common.data_context.as_deref());
        let width = match (common.width, &element.kind) {
            (Some(width), _) => Length::Px(width),
            (None, ElementKind::Icon(_)) => Length::Px(16.0),
            (None, ElementKind::Separator(separator))
                if separator.orientation == Orientation::Vertical =>
            {
                Length::Auto
            }
            (None, _) if parent_axis == Axis::Horizontal => Length::Px(desired_width(ui, element)?),
            (None, _) => Length::Auto,
        };
        let height = common.height.map_or_else(
            || match &element.kind {
                ElementKind::StackPanel(_)
                | ElementKind::Grid(_)
                | ElementKind::Border(_)
                | ElementKind::ScrollViewer(_)
                | ElementKind::WrapPanel(_)
                | ElementKind::DockPanel(_)
                | ElementKind::DockWorkspace(_)
                | ElementKind::Ribbon(_) => Length::Auto,
                ElementKind::ListBox(_) => Length::Px(160.0),
                ElementKind::TextBox(_)
                | ElementKind::ComboBox(_)
                | ElementKind::LookupEdit(_)
                | ElementKind::ButtonEdit(_)
                | ElementKind::SpinBox(_)
                | ElementKind::DateEdit(_)
                | ElementKind::TimeEdit(_)
                | ElementKind::Button(_)
                | ElementKind::ToggleButton(_)
                | ElementKind::DropDownButton(_)
                | ElementKind::SplitButton(_) => Length::Px(36.0),
                ElementKind::CheckBox(_) | ElementKind::RadioButton(_) => Length::Px(32.0),
                ElementKind::TextBlock(_) => Length::Px(24.0),
                ElementKind::Icon(_) => Length::Px(16.0),
                ElementKind::Separator(separator) => match separator.orientation {
                    Orientation::Horizontal => Length::Auto,
                    Orientation::Vertical if parent_axis == Axis::Horizontal => Length::Auto,
                    Orientation::Vertical => Length::Px(16.0),
                },
            },
            Length::Px,
        );
        let style = LayoutStyle {
            width,
            height,
            margin: edges(common.margin),
            flex_shrink: 0.0,
            min_size: rust_desktop_ui_core::Size::new(common.min_width, common.min_height),
            max_size: rust_desktop_ui_core::Size::new(
                common.max_width.unwrap_or(f32::INFINITY),
                common.max_height.unwrap_or(f32::INFINITY),
            ),
            horizontal_alignment: Some(forms::alignment(common.horizontal_alignment)),
            vertical_alignment: Some(forms::alignment(common.vertical_alignment)),
            grid_row: rust_desktop_ui_core::GridPlacement {
                start: Some(common.grid_row + 1),
                span: common.grid_row_span,
            },
            grid_column: rust_desktop_ui_core::GridPlacement {
                start: Some(common.grid_column + 1),
                span: common.grid_column_span,
            },
            ..Default::default()
        };
        let id = match &element.kind {
            ElementKind::ListBox(_)
            | ElementKind::ComboBox(_)
            | ElementKind::LookupEdit(_)
            | ElementKind::ButtonEdit(_)
            | ElementKind::SpinBox(_)
            | ElementKind::DateEdit(_)
            | ElementKind::TimeEdit(_)
            | ElementKind::Grid(_)
            | ElementKind::Border(_)
            | ElementKind::ScrollViewer(_)
            | ElementKind::WrapPanel(_)
            | ElementKind::DockPanel(_) => {
                self.mount_form_element(ui, parent, element, style, available_width, &context)?
            }
            ElementKind::DockWorkspace(_) => {
                return Err("DockWorkspace requires the docking window coordinator".into());
            }
            ElementKind::Ribbon(definition) => {
                let mut ribbon = crate::view_ribbon::build_ribbon(ui, definition)?;
                let initial_width = resolved_width(ui, element, parent_axis, available_width)?;
                ribbon.preflight(ui, initial_width).map_err(err)?;
                let wrapper = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            kind: LayoutKind::Stack(Axis::Vertical),
                            ..style
                        },
                    )
                    .map_err(err)?;
                ribbon
                    .mount_with_width(ui, wrapper, initial_width)
                    .map_err(err)?;
                self.commands
                    .extend(crate::view_ribbon::commands(definition));
                self.ribbon = Some(crate::view_ribbon::MountedRibbon { wrapper, ribbon });
                wrapper
            }
            ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
                let entries = menu_entries(&menu.items, &mut self.commands);
                let button = &menu.button;
                if let Some(command) = &button.command {
                    self.commands.push(command.clone());
                }
                if matches!(element.kind, ElementKind::SplitButton(_)) {
                    let parts = ui
                        .split_button(
                            parent,
                            &button.content,
                            button.command.as_deref().map(Into::into),
                            &entries,
                            style,
                        )
                        .map_err(err)?;
                    for part in [parts.primary, parts.dropdown] {
                        ui.set_button_style(part, button_style(&button.style))
                            .map_err(err)?;
                        if let Some(tip) = &common.tool_tip {
                            ui.set_tooltip(part, tip).map_err(err)?;
                        }
                    }
                    ui.set_button_icon(
                        parts.primary,
                        button_icon(button.icon, button.icon_size, button.icon_spacing),
                    )
                    .map_err(err)?;
                    if let Some(name) = &common.automation_name {
                        self.automation_names.insert(parts.primary, name.clone());
                    }
                    let id = parts.container;
                    self.splits.insert(id, parts);
                    id
                } else {
                    let id = ui
                        .dropdown_button(parent, &button.content, &entries, style)
                        .map_err(err)?;
                    ui.set_button_style(id, button_style(&button.style))
                        .map_err(err)?;
                    ui.set_button_icon(
                        id,
                        button_icon(button.icon, button.icon_size, button.icon_spacing),
                    )
                    .map_err(err)?;
                    id
                }
            }
            ElementKind::StackPanel(panel) => {
                let width = resolved_width(ui, element, parent_axis, available_width)?;
                let child_width = (width - panel.padding[0] - panel.padding[2]).max(0.0);
                let axis = axis(panel.orientation);
                let id = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            kind: LayoutKind::Stack(axis),
                            gap: panel.spacing,
                            padding: edges(panel.padding),
                            ..style
                        },
                    )
                    .map_err(err)?;
                ui.set_surface(
                    id,
                    match panel.surface {
                        rust_desktop_ui_xaml::SurfaceKindDefinition::Transparent => {
                            SurfaceKind::Transparent
                        }
                        rust_desktop_ui_xaml::SurfaceKindDefinition::Card => SurfaceKind::Card,
                    },
                )
                .map_err(err)?;
                for child in &panel.children {
                    self.mount_element(ui, id, child, axis, child_width, &context)?;
                }
                id
            }
            ElementKind::TextBlock(text) => {
                let id = ui.label(parent, &text.text, style).map_err(err)?;
                ui.set_text_style(id, text_style(text)).map_err(err)?;
                ui.set_text_auto_height(id, common.height.is_none())
                    .map_err(err)?;
                id
            }
            ElementKind::Icon(icon) => {
                let id = ui
                    .icon(parent, icon_symbol(icon.symbol), style)
                    .map_err(err)?;
                ui.set_icon_style(
                    id,
                    IconStyle {
                        foreground: icon.foreground.map(color),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
                if let Some(name) = &common.automation_name {
                    ui.set_label(id, name).map_err(err)?;
                }
                id
            }
            ElementKind::Separator(separator) => ui
                .separator(
                    parent,
                    SeparatorSettings {
                        orientation: axis(separator.orientation),
                        thickness: separator.thickness,
                        foreground: separator.foreground.map(color),
                    },
                    style,
                )
                .map_err(err)?,
            ElementKind::Button(button) => {
                let command = button.command.as_ref().map(|name| name.as_str().into());
                if let Some(command) = &command
                    && !ui.commands().contains(command)
                {
                    return Err(format!("Unregistered XAML command '{}'", command.0));
                }
                if let Some(command) = &button.command {
                    self.commands.push(command.clone());
                }
                let id = ui
                    .button(parent, &button.content, command, style)
                    .map_err(err)?;
                ui.set_button_style(id, button_style(&button.style))
                    .map_err(err)?;
                ui.set_button_icon(
                    id,
                    button_icon(button.icon, button.icon_size, button.icon_spacing),
                )
                .map_err(err)?;
                id
            }
            ElementKind::ToggleButton(button) => {
                let command = button.command.as_ref().map(|name| name.as_str().into());
                // Registration owns shared state; markup only initializes a standalone toggle.
                let checked = if let Some(command) = &command {
                    ui.commands().checked(command).ok_or_else(|| {
                        format!(
                            "ToggleButton command '{}' requires a registered checked state",
                            command.0
                        )
                    })?
                } else {
                    button.is_checked
                };
                if let Some(command) = &button.command {
                    self.commands.push(command.clone());
                }
                let id = ui
                    .toggle(parent, &button.content, command, checked, style)
                    .map_err(err)?;
                ui.set_button_style(id, button_style(&button.style))
                    .map_err(err)?;
                ui.set_button_icon(
                    id,
                    button_icon(button.icon, button.icon_size, button.icon_spacing),
                )
                .map_err(err)?;
                self.toggles.push(id);
                id
            }
            ElementKind::CheckBox(check) => {
                let id = ui
                    .checkbox(parent, &check.content, check.is_checked, style)
                    .map_err(err)?;
                ui.set_button_style(id, button_style(&check.style))
                    .map_err(err)?;
                self.checks.push(id);
                id
            }
            ElementKind::RadioButton(radio) => {
                let key = radio
                    .group_name
                    .as_ref()
                    .map_or(RadioGroupKey::Parent(parent), |name| {
                        RadioGroupKey::Named(name.clone())
                    });
                let group = *self
                    .radio_groups
                    .entry(key)
                    .or_insert_with(|| ui.radio_group());
                let id = ui
                    .radio_button(parent, &radio.content, group, radio.is_checked, style)
                    .map_err(err)?;
                ui.set_button_style(id, button_style(&radio.style))
                    .map_err(err)?;
                self.radios.push(id);
                id
            }
            ElementKind::TextBox(field) => {
                let id = ui
                    .edit_box(
                        parent,
                        common.automation_name.as_deref().unwrap_or("Text"),
                        &field.text,
                        style,
                    )
                    .map_err(err)?;
                ui.set_editor_settings(
                    id,
                    EditorSettings {
                        is_read_only: field.is_read_only,
                        max_length: field.max_length,
                        placeholder: field.placeholder.clone(),
                        commit_on_focus_lost: field.commit_on_lost_focus,
                    },
                )
                .map_err(err)?;
                ui.set_editor_validation(
                    id,
                    EditorValidation {
                        is_required: field.is_required,
                        message: field.validation_message.clone(),
                    },
                )
                .map_err(err)?;
                ui.set_edit_box_style(
                    id,
                    EditBoxStyle {
                        font_size: field.font_size,
                        padding: field.padding.map(edges),
                        corner_radius: field.corner_radius,
                        border_width: field.border_thickness,
                        normal: EditBoxAppearance {
                            background: field.background.map(color),
                            foreground: field.foreground.map(color),
                            placeholder: field.placeholder_foreground.map(color),
                            border: field.border_brush.map(color),
                            accent: field.focus_brush.map(color),
                            selection: field.selection_brush.map(color),
                        },
                        invalid: EditBoxAppearance {
                            border: field.validation_brush.map(color),
                            accent: field.validation_brush.map(color),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .map_err(err)?;
                self.editors.push(id);
                id
            }
        };
        ui.set_enabled(id, common.is_enabled).map_err(err)?;
        if common.visibility != rust_desktop_ui_xaml::Visibility::Visible {
            let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
            props.visible = false;
            props.collapsed = common.visibility == rust_desktop_ui_xaml::Visibility::Collapsed;
            ui.runtime
                .apply(rust_desktop_ui_core::Mutation::SetProps { node: id, props })
                .map_err(err)?;
        }
        if let Some(tip) = &common.tool_tip
            && !matches!(element.kind, ElementKind::SplitButton(_))
        {
            ui.set_tooltip(id, tip).map_err(err)?;
        }
        self.roles.insert(
            id,
            match &element.kind {
                ElementKind::StackPanel(_)
                | ElementKind::Grid(_)
                | ElementKind::Border(_)
                | ElementKind::WrapPanel(_)
                | ElementKind::DockPanel(_) => Role::Panel,
                ElementKind::ScrollViewer(_) => Role::ScrollView,
                ElementKind::ListBox(_) => Role::ListBox,
                ElementKind::ComboBox(_) | ElementKind::LookupEdit(_) => Role::ComboBox,
                ElementKind::ButtonEdit(_)
                | ElementKind::SpinBox(_)
                | ElementKind::DateEdit(_)
                | ElementKind::TimeEdit(_) => Role::Group,
                ElementKind::DockWorkspace(_) => Role::Panel,
                ElementKind::Ribbon(_) => Role::Group,
                ElementKind::TextBox(_) => Role::TextBox,
                ElementKind::TextBlock(_) => Role::Label,
                ElementKind::Button(_) => Role::Button,
                ElementKind::ToggleButton(_) => Role::ToggleButton,
                ElementKind::DropDownButton(_) => Role::Button,
                ElementKind::SplitButton(_) => Role::Group,
                ElementKind::CheckBox(_) => Role::CheckBox,
                ElementKind::RadioButton(_) => Role::RadioButton,
                ElementKind::Icon(_) => Role::Image,
                ElementKind::Separator(_) => Role::Separator,
            },
        );
        if let Some(name) = &common.name
            && self.names.insert(name.clone(), id).is_some()
        {
            return Err(format!("Duplicate XAML name '{name}'"));
        }
        if let Some(name) = &common.automation_name {
            self.automation_names.insert(id, name.clone());
        }
        self.record_bindings(id, element, context);
        Ok(id)
    }
}

// The layout engine does not measure leaf text. Horizontal Auto uses a desired
// width measured at mount; vertical Auto continues to stretch with the parent.
fn desired_width(ui: &mut Controls, element: &ElementDefinition) -> Result<f32, String> {
    if let Some(width) = element.common.width {
        return Ok(width);
    }
    match &element.kind {
        ElementKind::DockWorkspace(_) => {
            Err("DockWorkspace requires the docking window coordinator".into())
        }
        ElementKind::Ribbon(_) => Ok(640.0),
        ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
            let button = &menu.button;
            let arrow = if matches!(element.kind, ElementKind::SplitButton(_)) {
                32.0
            } else {
                20.0
            };
            Ok(button_content_width(
                ui,
                &button.content,
                &button.style,
                button.icon,
                button.icon_size,
                button.icon_spacing,
            )? + arrow)
        }
        ElementKind::TextBox(_)
        | ElementKind::ListBox(_)
        | ElementKind::ComboBox(_)
        | ElementKind::LookupEdit(_)
        | ElementKind::ButtonEdit(_)
        | ElementKind::SpinBox(_)
        | ElementKind::DateEdit(_)
        | ElementKind::TimeEdit(_) => Ok(240.0),
        ElementKind::Grid(_)
        | ElementKind::Border(_)
        | ElementKind::ScrollViewer(_)
        | ElementKind::DockPanel(_) => {
            let mut width = 0.0_f32;
            for child in element.children() {
                width = width.max(desired_width(ui, child)?);
            }
            Ok(width)
        }
        ElementKind::TextBlock(text) => Ok(ui
            .measure_styled_text(
                &text.text,
                16384.0,
                TextStyle {
                    wrap: false,
                    ..text_style(text)
                },
            )
            .map_err(err)?
            .width
            + 16.0),
        ElementKind::Button(button) => button_content_width(
            ui,
            &button.content,
            &button.style,
            button.icon,
            button.icon_size,
            button.icon_spacing,
        ),
        ElementKind::ToggleButton(button) => button_content_width(
            ui,
            &button.content,
            &button.style,
            button.icon,
            button.icon_size,
            button.icon_spacing,
        ),
        ElementKind::CheckBox(check) => button_width(ui, &check.content, &check.style, true),
        ElementKind::RadioButton(radio) => button_width(ui, &radio.content, &radio.style, true),
        ElementKind::Icon(_) => Ok(16.0),
        ElementKind::Separator(separator) => {
            Ok(if separator.orientation == Orientation::Vertical {
                separator.thickness
            } else {
                16.0
            })
        }
        ElementKind::StackPanel(panel) | ElementKind::WrapPanel(panel) => {
            let mut widths = Vec::with_capacity(panel.children.len());
            for child in &panel.children {
                widths.push(
                    desired_width(ui, child)? + child.common.margin[0] + child.common.margin[2],
                );
            }
            let children = match panel.orientation {
                Orientation::Horizontal => {
                    widths.iter().sum::<f32>()
                        + panel.spacing * widths.len().saturating_sub(1) as f32
                }
                Orientation::Vertical => widths.into_iter().fold(0.0_f32, f32::max),
            };
            Ok((children + panel.padding[0] + panel.padding[2]).max(0.0))
        }
    }
}

// Match the width assigned by mount_element. Auto stretches vertically, while
// horizontal children retain their measured desired width (flex_shrink = 0).
fn resolved_width(
    ui: &mut Controls,
    element: &ElementDefinition,
    parent_axis: Axis,
    available_width: f32,
) -> Result<f32, String> {
    if let Some(width) = element.common.width {
        Ok(width)
    } else if parent_axis == Axis::Horizontal {
        desired_width(ui, element)
    } else {
        Ok((available_width - element.common.margin[0] - element.common.margin[2]).max(0.0))
    }
}

fn validate_ribbon_width(
    ui: &mut Controls,
    root: &ElementDefinition,
    available_width: f32,
) -> Result<(), String> {
    // Structural validation runs first, before recursive desired-width
    // measurement can inspect any manually constructed descendant tree.
    let mut pending = vec![(root, Axis::Vertical, available_width)];
    while let Some((element, parent_axis, available_width)) = pending.pop() {
        match &element.kind {
            ElementKind::Ribbon(definition) => {
                let width = resolved_width(ui, element, parent_axis, available_width)?;
                crate::view_ribbon::build_ribbon(ui, definition)?
                    .preflight(ui, width)
                    .map_err(err)?;
            }
            ElementKind::StackPanel(panel) => {
                let width = resolved_width(ui, element, parent_axis, available_width)?;
                let child_width = (width - panel.padding[0] - panel.padding[2]).max(0.0);
                let axis = axis(panel.orientation);
                pending.extend(
                    panel
                        .children
                        .iter()
                        .map(|child| (child, axis, child_width)),
                );
            }
            _ => pending.extend(
                element
                    .children()
                    .into_iter()
                    .map(|child| (child, Axis::Vertical, available_width)),
            ),
        }
    }
    Ok(())
}

fn validate_tree(ui: &mut Controls, root: &ElementDefinition) -> Result<(), String> {
    let mut pending = vec![(root, 1, 0)];
    let mut names = HashSet::new();
    let mut checked_radio_groups = HashSet::new();
    let mut count = 0;
    let mut has_ribbon = false;
    while let Some((element, depth, parent)) = pending.pop() {
        count += 1;
        if count > rust_desktop_ui_xaml::MAX_ELEMENTS
            || depth > rust_desktop_ui_xaml::MAX_ELEMENT_DEPTH
        {
            return Err("XAML content exceeds the element count or depth limit".into());
        }
        if let Some(name) = &element.common.name
            && !names.insert(name)
        {
            return Err(format!("Duplicate XAML name '{name}'"));
        }
        if element
            .common
            .tool_tip
            .as_ref()
            .is_some_and(|tip| tip.len() > rust_desktop_ui_xaml::MAX_TOOLTIP_BYTES)
        {
            return Err("ToolTip exceeds the 4096-byte UTF-8 limit".into());
        }
        match &element.kind {
            ElementKind::DockWorkspace(_) => {
                return Err("DockWorkspace requires the docking window coordinator".into());
            }
            ElementKind::TextBlock(text) => {
                rust_desktop_ui_xaml::validate_text_block(text)?;
                text_style(text).validate().map_err(err)?;
                if text.text.len() > 1024 * 1024 {
                    return Err("TextBlock text exceeds the one-MiB limit".into());
                }
            }
            ElementKind::Ribbon(definition) => {
                if has_ribbon {
                    return Err("Only one Ribbon is supported per Window".into());
                }
                has_ribbon = true;
                crate::view_ribbon::build_ribbon(ui, definition)?;
                let (nested, ribbon_depth) = crate::view_ribbon::nested_size(definition);
                count += nested;
                if count > rust_desktop_ui_xaml::MAX_ELEMENTS
                    || depth + ribbon_depth - 1 > rust_desktop_ui_xaml::MAX_ELEMENT_DEPTH
                {
                    return Err("XAML Ribbon exceeds the element count or depth limit".into());
                }
            }
            ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
                let button = &menu.button;
                validate_menu_button_metrics(&element.common, &button.style)?;
                validate_button_icon(
                    &button.content,
                    button.icon,
                    button.icon_size,
                    button.icon_spacing,
                    element.common.automation_name.as_deref(),
                )?;
                if matches!(element.kind, ElementKind::SplitButton(_)) != button.command.is_some() {
                    return Err(
                        "SplitButton requires Command; DropDownButton forbids Command".into(),
                    );
                }
                if let Some(command) = &button.command {
                    validate_command(ui, command)?;
                }
                validate_menu(ui, &menu.items, depth, &mut count)?;
            }
            ElementKind::StackPanel(panel) => {
                if panel.children.len() > rust_desktop_ui_xaml::MAX_ELEMENTS {
                    return Err("XAML content exceeds the element count limit".into());
                }
            }
            ElementKind::Button(button) => {
                validate_button_icon(
                    &button.content,
                    button.icon,
                    button.icon_size,
                    button.icon_spacing,
                    element.common.automation_name.as_deref(),
                )?;
                if let Some(command) = &button.command
                    && !ui.commands().contains(&command.as_str().into())
                {
                    return Err(format!("Unregistered XAML command '{command}'"));
                }
            }
            ElementKind::ToggleButton(button) => {
                validate_button_icon(
                    &button.content,
                    button.icon,
                    button.icon_size,
                    button.icon_spacing,
                    element.common.automation_name.as_deref(),
                )?;
                if let Some(command) = &button.command {
                    let id = command.as_str().into();
                    if !ui.commands().contains(&id) {
                        return Err(format!("Unregistered XAML command '{command}'"));
                    }
                    if ui.commands().checked(&id).is_none() {
                        return Err(format!(
                            "ToggleButton command '{command}' requires a registered checked state"
                        ));
                    }
                }
            }
            ElementKind::RadioButton(radio) => {
                if let Some(name) = &radio.group_name {
                    let mut chars = name.chars();
                    if !chars
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return Err("GroupName must match [A-Za-z_][A-Za-z0-9_]*".into());
                    }
                }
                if radio.is_checked {
                    let group = radio
                        .group_name
                        .as_ref()
                        .map_or(RadioGroupKey::Parent(parent), |name| {
                            RadioGroupKey::Named(name.clone())
                        });
                    if !checked_radio_groups.insert(group) {
                        return Err(
                            "A RadioButton group may have at most one initial IsChecked=True"
                                .into(),
                        );
                    }
                }
            }
            ElementKind::Separator(separator)
                if !separator.thickness.is_finite()
                    || separator.thickness <= 0.0
                    || separator.thickness > 16384.0 =>
            {
                return Err("Separator Thickness must be finite and in (0, 16384]".into());
            }
            _ => {}
        }
        pending.extend(
            element
                .children()
                .into_iter()
                .map(|child| (child, depth + 1, count)),
        );
    }
    Ok(())
}

fn button_width(
    ui: &mut Controls,
    content: &str,
    style: &ButtonStyleDefinition,
    checkbox: bool,
) -> Result<f32, String> {
    let font_size = style.font_size.unwrap_or(ui.theme().tokens().font_size);
    let padding = style.padding.unwrap_or([8.0, 5.0, 8.0, 5.0]);
    let text = ui
        .measure_text_with_font_size(content, 16384.0, false, font_size)
        .map_err(err)?;
    let extra = if checkbox { 26.0 } else { 0.0 };
    Ok((text.width + padding[0] + padding[2] + extra).max(if checkbox { extra } else { 80.0 }))
}

fn validate_command(ui: &Controls, command: &str) -> Result<(), String> {
    let mut chars = command.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("Command must match [A-Za-z_][A-Za-z0-9_]*".into());
    }
    if !ui.commands().contains(&command.into()) {
        return Err(format!("Unregistered XAML command '{command}'"));
    }
    Ok(())
}

fn validate_menu_button_metrics(
    common: &rust_desktop_ui_xaml::CommonProperties,
    style: &ButtonStyleDefinition,
) -> Result<(), String> {
    let metric = |value: f32, positive: bool, maximum: f32| {
        value.is_finite() && value <= maximum && if positive { value > 0.0 } else { value >= 0.0 }
    };
    if common
        .width
        .into_iter()
        .chain(common.height)
        .any(|value| !metric(value, true, 16384.0))
        || common
            .margin
            .iter()
            .any(|value| !metric(*value, false, 16384.0))
        || style
            .font_size
            .is_some_and(|value| !metric(value, true, 512.0))
        || style
            .corner_radius
            .into_iter()
            .chain(style.border_thickness)
            .any(|value| !metric(value, false, 16384.0))
        || style.padding.is_some_and(|values| {
            values
                .into_iter()
                .any(|value| !metric(value, false, 16384.0))
        })
    {
        return Err("Menu button metrics must be finite and within the XAML ranges".into());
    }
    Ok(())
}

fn validate_menu(
    ui: &Controls,
    entries: &[rust_desktop_ui_xaml::MenuEntryDefinition],
    owner_depth: usize,
    count: &mut usize,
) -> Result<(), String> {
    use rust_desktop_ui_xaml::{
        MAX_ELEMENT_DEPTH, MAX_ELEMENTS, MAX_MENU_DEPTH, MAX_MENU_TEXT_BYTES, MenuEntryDefinition,
    };
    if entries.is_empty() || entries.len() > MAX_ELEMENTS {
        return Err("Menu requires nonempty bounded items".into());
    }
    let mut pending: Vec<_> = entries.iter().map(|entry| (entry, 1)).collect();
    let mut text_bytes = 0;
    while let Some((entry, depth)) = pending.pop() {
        *count += 1;
        if *count > MAX_ELEMENTS
            || depth > MAX_MENU_DEPTH
            || owner_depth + depth > MAX_ELEMENT_DEPTH
        {
            return Err("Menu exceeds the element count or depth limit".into());
        }
        if let MenuEntryDefinition::Item(item) = entry {
            if item.header.trim().is_empty() || item.command.is_some() != item.items.is_empty() {
                return Err(
                    "MenuItem requires Header and either Command or nonempty submenu, exclusively"
                        .into(),
                );
            }
            text_bytes += item.header.len() + item.command.as_ref().map_or(0, String::len);
            if text_bytes > MAX_MENU_TEXT_BYTES || item.items.len() > MAX_ELEMENTS {
                return Err("Menu exceeds the text or element count limit".into());
            }
            if let Some(command) = &item.command {
                validate_command(ui, command)?;
            }
            pending.extend(item.items.iter().map(|child| (child, depth + 1)));
        }
    }
    Ok(())
}

fn menu_entries(
    entries: &[rust_desktop_ui_xaml::MenuEntryDefinition],
    commands: &mut Vec<String>,
) -> Vec<MenuEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            rust_desktop_ui_xaml::MenuEntryDefinition::Separator => MenuEntry::Separator,
            rust_desktop_ui_xaml::MenuEntryDefinition::Item(item) => {
                if let Some(command) = &item.command {
                    commands.push(command.clone());
                    MenuEntry::Item(
                        MenuItem::new(&item.header, command.as_str()).with_enabled(item.is_enabled),
                    )
                } else {
                    MenuEntry::Submenu {
                        label: item.header.clone(),
                        items: menu_entries(&item.items, commands),
                        enabled: item.is_enabled,
                    }
                }
            }
        })
        .collect()
}

fn button_content_width(
    ui: &mut Controls,
    content: &str,
    style: &ButtonStyleDefinition,
    icon: Option<rust_desktop_ui_xaml::IconSymbol>,
    size: f32,
    spacing: f32,
) -> Result<f32, String> {
    if icon.is_none() {
        return button_width(ui, content, style, false);
    }
    let font_size = style.font_size.unwrap_or(ui.theme().tokens().font_size);
    let padding = style.padding.unwrap_or([8.0, 5.0, 8.0, 5.0]);
    let text = ui
        .measure_text_with_font_size(content, 16384.0, false, font_size)
        .map_err(err)?;
    let gap = if content.is_empty() { 0.0 } else { spacing };
    let minimum = if content.is_empty() { 36.0 } else { 80.0 };
    Ok((text.width + padding[0] + padding[2] + size + gap).max(minimum))
}

fn validate_button_icon(
    content: &str,
    icon: Option<rust_desktop_ui_xaml::IconSymbol>,
    size: f32,
    spacing: f32,
    automation_name: Option<&str>,
) -> Result<(), String> {
    if !size.is_finite() || size <= 0.0 || size > 512.0 {
        return Err("IconSize must be finite and in (0, 512]".into());
    }
    if !spacing.is_finite() || !(0.0..=16384.0).contains(&spacing) {
        return Err("IconSpacing must be finite and in [0, 16384]".into());
    }
    if icon.is_none() && (size != 16.0 || spacing != 8.0) {
        return Err("IconSize and IconSpacing require Icon".into());
    }
    if icon.is_some()
        && content.trim().is_empty()
        && automation_name.is_none_or(|name| name.trim().is_empty())
    {
        return Err("An icon-only button requires a nonempty AutomationName".into());
    }
    Ok(())
}

fn button_icon(
    symbol: Option<rust_desktop_ui_xaml::IconSymbol>,
    size: f32,
    spacing: f32,
) -> Option<ButtonIcon> {
    symbol.map(|symbol| ButtonIcon {
        symbol: icon_symbol(symbol),
        size,
        spacing,
    })
}

pub(crate) fn icon_symbol(symbol: rust_desktop_ui_xaml::IconSymbol) -> IconSymbol {
    match symbol {
        rust_desktop_ui_xaml::IconSymbol::Check => IconSymbol::Check,
        rust_desktop_ui_xaml::IconSymbol::Close => IconSymbol::Close,
        rust_desktop_ui_xaml::IconSymbol::Add => IconSymbol::Add,
        rust_desktop_ui_xaml::IconSymbol::Remove => IconSymbol::Remove,
        rust_desktop_ui_xaml::IconSymbol::ChevronDown => IconSymbol::ChevronDown,
        rust_desktop_ui_xaml::IconSymbol::ChevronRight => IconSymbol::ChevronRight,
        rust_desktop_ui_xaml::IconSymbol::Save => IconSymbol::Save,
        rust_desktop_ui_xaml::IconSymbol::Pin => IconSymbol::Pin,
        rust_desktop_ui_xaml::IconSymbol::FolderOpen => IconSymbol::FolderOpen,
        rust_desktop_ui_xaml::IconSymbol::Copy => IconSymbol::Copy,
        rust_desktop_ui_xaml::IconSymbol::Edit => IconSymbol::Edit,
        rust_desktop_ui_xaml::IconSymbol::Eraser => IconSymbol::Eraser,
        rust_desktop_ui_xaml::IconSymbol::Sun => IconSymbol::Sun,
        rust_desktop_ui_xaml::IconSymbol::Moon => IconSymbol::Moon,
        rust_desktop_ui_xaml::IconSymbol::Lock => IconSymbol::Lock,
    }
}

fn text_style(text: &rust_desktop_ui_xaml::TextBlockDefinition) -> TextStyle {
    TextStyle {
        font_size: text.font_size,
        font_weight: text.font_weight,
        foreground: text.foreground.map(color),
        tone: match text.tone {
            rust_desktop_ui_xaml::TextToneDefinition::Primary => TextTone::Primary,
            rust_desktop_ui_xaml::TextToneDefinition::Secondary => TextTone::Secondary,
        },
        wrap: text.wrap,
    }
}

fn button_style(style: &ButtonStyleDefinition) -> ButtonStyle {
    ButtonStyle {
        font_size: style.font_size,
        padding: style.padding.map(edges),
        corner_radius: style.corner_radius,
        border_width: style.border_thickness,
        normal: ButtonAppearance {
            background: style.background.map(color),
            foreground: style.foreground.map(color),
            border: style.border_brush.map(color),
            accent: style.focus_brush.map(color),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn axis(orientation: Orientation) -> Axis {
    match orientation {
        Orientation::Vertical => Axis::Vertical,
        Orientation::Horizontal => Axis::Horizontal,
    }
}

pub(crate) fn edges([left, top, right, bottom]: [f32; 4]) -> Edges {
    Edges {
        left,
        top,
        right,
        bottom,
    }
}

fn color([red, green, blue, alpha]: [u8; 4]) -> Color {
    Color {
        r: f32::from(red) / 255.0,
        g: f32::from(green) / 255.0,
        b: f32::from(blue) / 255.0,
        a: f32::from(alpha) / 255.0,
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod docking_rejection_tests {
    use super::*;

    #[test]
    fn ordinary_form_rejects_docking_before_creating_any_nodes() {
        let definition = rust_desktop_ui_xaml::parse(
            r#"<Window xmlns="urn:rust-ui-engine:ui">
                <StackPanel><TextBox/><DockWorkspace><DockGroup><DockPane Id="P">
                    <TextBox/>
                </DockPane></DockGroup></DockWorkspace></StackPanel>
            </Window>"#,
        )
        .unwrap();
        let mut ui = Controls::new(rust_desktop_ui_controls::Theme::FluentLight).unwrap();
        let root = ui.root();
        let original = ui.runtime.tree().preorder();
        let result = MountedView::mount(&mut ui, root, &definition.content);
        assert!(result.err().unwrap().contains("docking window coordinator"));
        assert_eq!(ui.runtime.tree().preorder(), original);
    }
}
