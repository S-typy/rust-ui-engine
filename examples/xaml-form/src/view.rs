//! Mount compiled elements without attaching application-specific behavior.
use rust_desktop_ui_controls::{
    Controls, EditBoxAppearance, EditBoxStyle, EditorSettings, EditorValidation, Role,
};
use rust_desktop_ui_core::{Axis, Color, Edges, LayoutKind, LayoutStyle, Length, WidgetId};
use rust_desktop_ui_xaml::{ElementDefinition, ElementKind, Orientation};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct MountedView {
    pub(crate) names: HashMap<String, WidgetId>,
    pub(crate) automation_names: HashMap<WidgetId, String>,
    pub(crate) editors: Vec<WidgetId>,
    pub(crate) checks: Vec<WidgetId>,
    pub(crate) commands: Vec<String>,
    pub(crate) roles: HashMap<WidgetId, Role>,
}

impl MountedView {
    pub(crate) fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
    ) -> Result<Self, String> {
        validate_tree(ui, element)?;
        let mut view = Self::default();
        view.mount_element(ui, parent, element, Axis::Vertical)?;
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
    ) -> Result<WidgetId, String> {
        let common = &element.common;
        let width = match common.width {
            Some(width) => Length::Px(width),
            None if parent_axis == Axis::Horizontal => Length::Px(desired_width(ui, element)?),
            None => Length::Auto,
        };
        let height = common.height.map_or_else(
            || match &element.kind {
                ElementKind::StackPanel(_) => Length::Auto,
                ElementKind::TextBox(_) | ElementKind::Button(_) => Length::Px(36.0),
                ElementKind::CheckBox(_) => Length::Px(32.0),
                ElementKind::TextBlock(_) => Length::Px(24.0),
            },
            Length::Px,
        );
        let style = LayoutStyle {
            width,
            height,
            margin: edges(common.margin),
            flex_shrink: 0.0,
            ..Default::default()
        };
        let id = match &element.kind {
            ElementKind::StackPanel(panel) => {
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
                for child in &panel.children {
                    self.mount_element(ui, id, child, axis)?;
                }
                id
            }
            ElementKind::TextBlock(text) => ui.label(parent, &text.text, style).map_err(err)?,
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
                ui.button(parent, &button.content, command, style)
                    .map_err(err)?
            }
            ElementKind::CheckBox(check) => {
                let id = ui
                    .checkbox(parent, &check.content, check.is_checked, style)
                    .map_err(err)?;
                self.checks.push(id);
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
        self.roles.insert(
            id,
            match &element.kind {
                ElementKind::StackPanel(_) => Role::Panel,
                ElementKind::TextBox(_) => Role::TextBox,
                ElementKind::TextBlock(_) => Role::Label,
                ElementKind::Button(_) => Role::Button,
                ElementKind::CheckBox(_) => Role::CheckBox,
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
        ElementKind::TextBox(_) => Ok(240.0),
        ElementKind::TextBlock(text) => Ok(ui
            .measure_text(&text.text, 16384.0, false)
            .map_err(err)?
            .width
            + 16.0),
        ElementKind::Button(button) => Ok((ui
            .measure_text(&button.content, 16384.0, false)
            .map_err(err)?
            .width
            + 24.0)
            .max(80.0)),
        ElementKind::CheckBox(check) => Ok((ui
            .measure_text(&check.content, 16384.0, false)
            .map_err(err)?
            .width
            + 40.0)
            .max(40.0)),
        ElementKind::StackPanel(panel) => {
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

fn validate_tree(ui: &Controls, root: &ElementDefinition) -> Result<(), String> {
    let mut pending = vec![(root, 1)];
    let mut names = HashSet::new();
    let mut count = 0;
    while let Some((element, depth)) = pending.pop() {
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
        match &element.kind {
            ElementKind::StackPanel(panel) => {
                if panel.children.len() > rust_desktop_ui_xaml::MAX_ELEMENTS {
                    return Err("XAML content exceeds the element count limit".into());
                }
                pending.extend(panel.children.iter().map(|child| (child, depth + 1)));
            }
            ElementKind::Button(button) => {
                if let Some(command) = &button.command
                    && !ui.commands().contains(&command.as_str().into())
                {
                    return Err(format!("Unregistered XAML command '{command}'"));
                }
            }
            _ => {}
        }
    }
    Ok(())
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
