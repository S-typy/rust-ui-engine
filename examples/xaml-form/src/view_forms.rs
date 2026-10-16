//! Layout and editor adapters for the compiled form profile.
use super::*;
use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::{Align, GridPlacement, GridTrack, Mutation, Scene, Size};
use rust_desktop_ui_xaml as x;

pub(super) struct BorderSurface {
    id: WidgetId,
    background: Option<Color>,
    border: Option<Color>,
    thickness: f32,
    radius: f32,
}
pub(super) struct ScrollLayout {
    viewport: WidgetId,
    child: WidgetId,
    horizontal: x::ScrollBarVisibility,
    vertical: x::ScrollBarVisibility,
}

pub(super) struct AutomaticHeight {
    id: WidgetId,
    offset: f32,
    applied: f32,
}

pub(super) fn alignment(value: x::Alignment) -> Align {
    match value {
        x::Alignment::Start => Align::Start,
        x::Alignment::Center => Align::Center,
        x::Alignment::End => Align::End,
        x::Alignment::Stretch => Align::Stretch,
    }
}
fn scroll_visibility(value: x::ScrollBarVisibility) -> ScrollBarVisibility {
    match value {
        x::ScrollBarVisibility::Auto => ScrollBarVisibility::Auto,
        x::ScrollBarVisibility::Hidden => ScrollBarVisibility::Hidden,
        x::ScrollBarVisibility::Disabled => ScrollBarVisibility::Disabled,
        x::ScrollBarVisibility::Visible => ScrollBarVisibility::Visible,
    }
}
fn track(value: &x::GridLength) -> GridTrack {
    match value {
        x::GridLength::Auto => GridTrack::Auto,
        x::GridLength::Pixel(v) => GridTrack::Px(*v),
        x::GridLength::Star(v) => GridTrack::Fr(*v),
    }
}
fn group_appearance(appearance: x::GroupBoxAppearanceDefinition) -> GroupAppearance {
    match appearance {
        x::GroupBoxAppearanceDefinition::Card => GroupAppearance::Card,
        x::GroupBoxAppearanceDefinition::Outline => GroupAppearance::Outline,
        x::GroupBoxAppearanceDefinition::Plain => GroupAppearance::Plain,
        x::GroupBoxAppearanceDefinition::Standard => GroupAppearance::Standard,
    }
}
fn group_header_style(style: x::GroupHeaderStyleDefinition) -> GroupHeaderStyle {
    match style {
        x::GroupHeaderStyleDefinition::Normal => GroupHeaderStyle::Normal,
        x::GroupHeaderStyleDefinition::Light => GroupHeaderStyle::Light,
        x::GroupHeaderStyleDefinition::Accent => GroupHeaderStyle::Accent,
    }
}
pub(super) fn group_settings(group: &x::GroupBoxDefinition) -> GroupSettings {
    GroupSettings {
        header: group.header.clone(),
        appearance: group_appearance(group.appearance),
        padding: edges(group.padding),
        header_style: group_header_style(group.header_style),
        header_icon: group.header_icon.map(icon_symbol),
        show_header: group.show_header,
        corner_radius: group.corner_radius,
        header_foreground: group.header_foreground.map(color),
        header_background: group.header_background.map(color),
    }
}
pub(super) fn expander_settings(group: &x::ExpanderDefinition) -> GroupSettings {
    GroupSettings {
        header: group.header.clone(),
        appearance: group_appearance(group.appearance),
        padding: edges(group.padding),
        header_style: group_header_style(group.header_style),
        header_icon: group.header_icon.map(icon_symbol),
        show_header: group.show_header,
        corner_radius: group.corner_radius,
        header_foreground: group.header_foreground.map(color),
        header_background: group.header_background.map(color),
    }
}
fn group_content_layout(ui: &mut Controls, content: WidgetId) -> Result<(), String> {
    // XAML has one content element. A single grid cell stretches that child
    // inside an explicitly sized group while retaining its intrinsic Auto size.
    let mut style = ui.runtime.tree().node(content).unwrap().props.style.clone();
    style.kind = LayoutKind::Grid;
    style.grid_columns = vec![GridTrack::Fr(1.0)];
    style.grid_rows = vec![GridTrack::Fr(1.0)];
    ui.runtime
        .apply(Mutation::SetStyle {
            node: content,
            style,
        })
        .map_err(err)
}
fn grid_tracks(values: &[x::GridLength]) -> Vec<GridTrack> {
    // XAML Grid has one fill track on an omitted axis. The backend's
    // unconstrained auto-placement mode has different intrinsic sizing.
    if values.is_empty() {
        vec![GridTrack::Fr(1.0)]
    } else {
        values.iter().map(track).collect()
    }
}
fn items(selection: &x::SelectionDefinition) -> Vec<SelectionItem> {
    selection
        .items
        .iter()
        .map(|item| SelectionItem {
            key: item.value.clone(),
            text: item.display.clone(),
            columns: Vec::new(),
            enabled: item.is_enabled,
        })
        .collect()
}
fn configure_editor(
    ui: &mut Controls,
    parts: &EditorControl,
    field: &x::EditBoxDefinition,
) -> Result<(), String> {
    ui.set_editor_settings(
        parts.editor,
        EditorSettings {
            is_read_only: field.is_read_only,
            max_length: field.max_length,
            placeholder: field.placeholder.clone(),
            commit_on_focus_lost: field.commit_on_lost_focus,
        },
    )
    .map_err(err)?;
    ui.set_editor_validation(
        parts.editor,
        EditorValidation {
            is_required: field.is_required,
            message: field.validation_message.clone(),
        },
    )
    .map_err(err)?;
    ui.set_composite_read_only(parts.container, field.is_read_only)
        .map_err(err)?;
    ui.set_edit_box_style(
        parts.editor,
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
    .map_err(err)
}

impl MountedView {
    pub(super) fn record_automatic_height(
        &mut self,
        ui: &Controls,
        id: WidgetId,
        kind: &ElementKind,
    ) {
        let offset = match kind {
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
            | ElementKind::SplitButton(_)
            | ElementKind::RibbonStatusBar(_) => 0.0,
            ElementKind::CheckBox(_) | ElementKind::RadioButton(_) | ElementKind::StatusBar(_) => {
                -4.0
            }
            ElementKind::ZoomControl(_) => -8.0,
            _ => return,
        };
        self.automatic_heights.push(AutomaticHeight {
            id,
            offset,
            applied: ui.theme().tokens().control_height + offset,
        });
    }

    fn sync_automatic_heights(&mut self, ui: &mut Controls) -> Result<bool, String> {
        let mut mutations = Vec::new();
        let height = ui.theme().tokens().control_height;
        self.automatic_heights.retain_mut(|entry| {
            let Some(node) = ui.runtime.tree().node(entry.id) else {
                return false;
            };
            // Explicit runtime overrides acquire the axis. Equal values have
            // no observable provenance, matching the Controls builder contract.
            if node.props.style.height != Length::Px(entry.applied) {
                return false;
            }
            let desired = height + entry.offset;
            if desired != entry.applied {
                let mut style = node.props.style.clone();
                style.height = Length::Px(desired);
                mutations.push(Mutation::SetStyle {
                    node: entry.id,
                    style,
                });
            }
            true
        });
        if mutations.is_empty() {
            return Ok(false);
        }
        ui.runtime.apply_batch(&mutations).map_err(err)?;
        for entry in &mut self.automatic_heights {
            entry.applied = height + entry.offset;
        }
        Ok(true)
    }
    pub(super) fn mount_form_element(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        style: LayoutStyle,
        width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        let label = element
            .common
            .automation_name
            .as_deref()
            .or(element.common.name.as_deref())
            .unwrap_or("Editor");
        let parts = match &element.kind {
            ElementKind::GroupBox(group) => {
                let width = match style.width {
                    Length::Px(value) => value,
                    _ => (width - element.common.margin[0] - element.common.margin[2]).max(0.0),
                };
                let settings = group_settings(group);
                let parts = if group.is_collapsible {
                    ui.collapsible_group_box(parent, settings, group.is_expanded, style)
                } else {
                    ui.group_box(parent, settings, style)
                }
                .map_err(err)?;
                group_content_layout(ui, parts.content)?;
                if let Some(child) = &group.child {
                    self.mount_element(
                        ui,
                        parts.content,
                        child,
                        Axis::Vertical,
                        (width - group.padding[0] - group.padding[2]).max(0.0),
                        context,
                    )?;
                }
                return Ok(parts.container);
            }
            ElementKind::Expander(group) => {
                let width = match style.width {
                    Length::Px(value) => value,
                    _ => (width - element.common.margin[0] - element.common.margin[2]).max(0.0),
                };
                let parts = ui
                    .expander(parent, expander_settings(group), group.is_expanded, style)
                    .map_err(err)?;
                group_content_layout(ui, parts.content)?;
                if let Some(child) = &group.child {
                    self.mount_element(
                        ui,
                        parts.content,
                        child,
                        Axis::Vertical,
                        (width - group.padding[0] - group.padding[2]).max(0.0),
                        context,
                    )?;
                }
                return Ok(parts.container);
            }
            ElementKind::TabbedGroup(group) => {
                let width = match style.width {
                    Length::Px(value) => value,
                    _ => (width - element.common.margin[0] - element.common.margin[2]).max(0.0),
                };
                let parts = ui
                    .tabbed_group(
                        parent,
                        group
                            .tabs
                            .iter()
                            .map(|tab| GroupTab {
                                header: tab.header.clone(),
                                icon: tab.header_icon.map(icon_symbol),
                                enabled: tab.is_enabled,
                            })
                            .collect(),
                        group.selected_index,
                        style,
                    )
                    .map_err(err)?;
                for (tab, content) in group.tabs.iter().zip(parts.contents) {
                    group_content_layout(ui, content)?;
                    let mut content_style =
                        ui.runtime.tree().node(content).unwrap().props.style.clone();
                    content_style.padding = edges(group.padding);
                    ui.runtime
                        .apply(Mutation::SetStyle {
                            node: content,
                            style: content_style,
                        })
                        .map_err(err)?;
                    if let Some(child) = &tab.child {
                        self.mount_element(
                            ui,
                            content,
                            child,
                            Axis::Vertical,
                            (width - group.padding[0] - group.padding[2]).max(0.0),
                            context,
                        )?;
                    }
                }
                return Ok(parts.container);
            }
            ElementKind::ListBox(selection) => {
                let list = ui
                    .list_box(
                        parent,
                        label,
                        items(selection),
                        SelectionSettings {
                            mode: match selection.selection_mode {
                                x::SelectionMode::Single => SelectionMode::Single,
                                x::SelectionMode::Multiple => SelectionMode::Multiple,
                            },
                            read_only: selection.is_read_only,
                        },
                        style,
                    )
                    .map_err(err)?;
                let keys = selection
                    .selected_value
                    .clone()
                    .map(|v| vec![v])
                    .unwrap_or_else(|| selection.selected_values.clone());
                ui.set_selected_keys(list.container, &keys).map_err(err)?;
                return Ok(list.container);
            }
            ElementKind::ComboBox(selection)
            | ElementKind::LookupEdit(x::LookupDefinition { selection, .. }) => {
                let parts = if let ElementKind::LookupEdit(lookup) = &element.kind {
                    ui.lookup_box(
                        parent,
                        label,
                        items(selection),
                        lookup.columns.iter().map(|c| c.header.clone()).collect(),
                        style,
                    )
                } else {
                    ui.combo_box(
                        parent,
                        label,
                        items(selection),
                        ComboBoxSettings {
                            editable: selection.is_editable,
                            read_only: selection.is_read_only,
                        },
                        style,
                    )
                }
                .map_err(err)?;
                if let ElementKind::LookupEdit(lookup) = &element.kind {
                    ui.set_lookup_columns(
                        parts.container,
                        lookup
                            .columns
                            .iter()
                            .map(|column| LookupColumn {
                                header: column.header.clone(),
                                width: Some(column.width),
                            })
                            .collect(),
                    )
                    .map_err(err)?;
                    ui.set_combo_settings(
                        parts.container,
                        ComboBoxSettings {
                            editable: selection.is_editable,
                            read_only: selection.is_read_only,
                        },
                    )
                    .map_err(err)?;
                }
                let mut settings = ui.editor_settings(parts.editor).unwrap().clone();
                settings.placeholder.clone_from(&selection.placeholder);
                ui.set_editor_settings(parts.editor, settings)
                    .map_err(err)?;
                ui.set_composite_read_only(parts.container, selection.is_read_only)
                    .map_err(err)?;
                if let Some(key) = &selection.selected_value {
                    ui.set_selected_keys(parts.container, std::slice::from_ref(key))
                        .map_err(err)?;
                }
                parts
            }
            ElementKind::ButtonEdit(definition) => {
                let buttons = definition
                    .buttons
                    .iter()
                    .map(|b| EditorButton {
                        label: b.content.clone(),
                        icon: b.icon.map(icon_symbol),
                        action: EditorButtonAction::Command(b.command.as_str().into()),
                    })
                    .collect();
                let parts = ui
                    .button_edit(parent, label, &definition.editor.text, buttons, style)
                    .map_err(err)?;
                for (button, definition) in parts.buttons.iter().zip(&definition.buttons) {
                    self.commands.push(definition.command.clone());
                    if let Some(tip) = &definition.tool_tip {
                        ui.set_tooltip(*button, tip).map_err(err)?;
                    }
                }
                configure_editor(ui, &parts, &definition.editor)?;
                parts
            }
            ElementKind::SpinBox(definition) => {
                let parts = ui
                    .spin_box(
                        parent,
                        label,
                        if element.common.bindings.contains_key("Value") {
                            definition
                                .value
                                .clamp(definition.minimum, definition.maximum)
                        } else {
                            definition.value
                        },
                        SpinBoxSettings {
                            minimum: definition.minimum,
                            maximum: definition.maximum,
                            step: definition.increment,
                        },
                        style,
                    )
                    .map_err(err)?;
                configure_editor(ui, &parts, &definition.editor)?;
                parts
            }
            ElementKind::DateEdit(definition) => {
                let parse = |v: &Option<String>| {
                    v.as_deref()
                        .map(DateValue::parse_iso)
                        .transpose()
                        .map_err(err)
                };
                let parts = ui
                    .date_edit(
                        parent,
                        label,
                        parse(&definition.value)?,
                        DateEditSettings {
                            minimum: parse(&definition.minimum)?,
                            maximum: parse(&definition.maximum)?,
                            allow_empty: true,
                            format: if definition.format == "yyyy-MM-dd" {
                                DateFormat::Iso
                            } else {
                                DateFormat::Russian
                            },
                        },
                        style,
                    )
                    .map_err(err)?;
                configure_editor(ui, &parts, &definition.editor)?;
                parts
            }
            ElementKind::TimeEdit(definition) => {
                let parse = |v: &Option<String>| {
                    v.as_deref().map(TimeValue::parse).transpose().map_err(err)
                };
                let parts = ui
                    .time_edit(
                        parent,
                        label,
                        parse(&definition.value)?,
                        TimeEditSettings {
                            minimum: parse(&definition.minimum)?,
                            maximum: parse(&definition.maximum)?,
                            allow_empty: true,
                            show_seconds: definition.format == "HH:mm:ss",
                            ..Default::default()
                        },
                        style,
                    )
                    .map_err(err)?;
                configure_editor(ui, &parts, &definition.editor)?;
                parts
            }
            ElementKind::Grid(grid) => {
                let id = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            kind: LayoutKind::Grid,
                            grid_columns: grid_tracks(&grid.columns),
                            grid_rows: grid_tracks(&grid.rows),
                            gap: grid.spacing,
                            padding: edges(grid.padding),
                            ..style
                        },
                    )
                    .map_err(err)?;
                ui.set_surface(
                    id,
                    if grid.surface == x::SurfaceKindDefinition::Card {
                        SurfaceKind::Card
                    } else {
                        SurfaceKind::Transparent
                    },
                )
                .map_err(err)?;
                for child in &grid.children {
                    self.mount_element(ui, id, child, Axis::Vertical, width, context)?;
                }
                return Ok(id);
            }
            ElementKind::WrapPanel(panel) => {
                let orientation = axis(panel.orientation);
                let id = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            kind: LayoutKind::Wrap(orientation),
                            gap: panel.spacing,
                            padding: edges(panel.padding),
                            ..style
                        },
                    )
                    .map_err(err)?;
                ui.set_surface(
                    id,
                    if panel.surface == x::SurfaceKindDefinition::Card {
                        SurfaceKind::Card
                    } else {
                        SurfaceKind::Transparent
                    },
                )
                .map_err(err)?;
                for child in &panel.children {
                    self.mount_element(ui, id, child, orientation, width, context)?;
                }
                return Ok(id);
            }
            ElementKind::Border(border) => {
                let mut padding = edges(border.padding);
                padding.left += border.border_thickness;
                padding.right += border.border_thickness;
                padding.top += border.border_thickness;
                padding.bottom += border.border_thickness;
                let id = ui
                    .transparent_panel(
                        parent,
                        LayoutStyle {
                            kind: LayoutKind::Grid,
                            grid_columns: vec![GridTrack::Fr(1.0)],
                            grid_rows: vec![GridTrack::Fr(1.0)],
                            padding,
                            ..style
                        },
                    )
                    .map_err(err)?;
                self.decorations.push(BorderSurface {
                    id,
                    background: border.background.map(color),
                    border: border.border_brush.map(color),
                    thickness: border.border_thickness,
                    radius: border.corner_radius,
                });
                self.mount_element(
                    ui,
                    id,
                    &border.child,
                    Axis::Vertical,
                    (width - padding.left - padding.right).max(0.0),
                    context,
                )?;
                return Ok(id);
            }
            ElementKind::ScrollViewer(scroll) => {
                let parts = ui.scroll_view(parent, style, 0.0).map_err(err)?;
                ui.set_surface(parts.viewport, SurfaceKind::Transparent)
                    .map_err(err)?;
                ui.configure_scroll_view(
                    parts.viewport,
                    ScrollSettings {
                        horizontal: scroll_visibility(scroll.horizontal_scroll_bar_visibility),
                        vertical: scroll_visibility(scroll.vertical_scroll_bar_visibility),
                    },
                )
                .map_err(err)?;
                let mut content_style = ui
                    .runtime
                    .tree()
                    .node(parts.content)
                    .unwrap()
                    .props
                    .style
                    .clone();
                content_style.padding = Edges::all(0.0);
                content_style.gap = 0.0;
                content_style.width = if scroll.horizontal_scroll_bar_visibility
                    == x::ScrollBarVisibility::Disabled
                {
                    Length::Percent(1.0)
                } else {
                    Length::Auto
                };
                content_style.height =
                    if scroll.vertical_scroll_bar_visibility == x::ScrollBarVisibility::Disabled {
                        Length::Percent(1.0)
                    } else {
                        Length::Auto
                    };
                ui.runtime
                    .apply(Mutation::SetStyle {
                        node: parts.content,
                        style: content_style,
                    })
                    .map_err(err)?;
                // Only the viewport clips. Its intrinsically measured content
                // host can remain smaller than overflowing descendants.
                let mut content_props =
                    ui.runtime.tree().node(parts.content).unwrap().props.clone();
                content_props.clip = false;
                ui.runtime
                    .apply(Mutation::SetProps {
                        node: parts.content,
                        props: content_props,
                    })
                    .map_err(err)?;
                ui.set_surface(parts.content, SurfaceKind::Transparent)
                    .map_err(err)?;
                let child = self.mount_element(
                    ui,
                    parts.content,
                    &scroll.child,
                    Axis::Vertical,
                    width,
                    context,
                )?;
                self.scrolls.push(ScrollLayout {
                    viewport: parts.viewport,
                    child,
                    horizontal: scroll.horizontal_scroll_bar_visibility,
                    vertical: scroll.vertical_scroll_bar_visibility,
                });
                return Ok(parts.viewport);
            }
            ElementKind::DockPanel(panel) => {
                return self.mount_dock_panel(ui, parent, panel, style, width, context);
            }
            _ => return Err("Unsupported form element".into()),
        };
        self.editors.push(parts.editor);
        if let Some(name) = &element.common.automation_name {
            self.automation_names.insert(parts.editor, name.clone());
        }
        Ok(parts.container)
    }

    fn mount_dock_panel(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        panel: &x::DockPanelDefinition,
        style: LayoutStyle,
        width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        // Each level reserves one edge and leaves the remaining rectangle for subsequent children.
        if panel.children.len() > 48 {
            return Err("DockPanel supports at most 48 edge reservations".into());
        }
        let root = ui
            .transparent_panel(
                parent,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    padding: edges(panel.padding),
                    ..style
                },
            )
            .map_err(err)?;
        let mut host = root;
        for (index, child) in panel.children.iter().enumerate() {
            let fill = panel.last_child_fill && index + 1 == panel.children.len();
            let horizontal = matches!(child.common.dock, x::DockSide::Left | x::DockSide::Right);
            let reverse = matches!(child.common.dock, x::DockSide::Right | x::DockSide::Bottom);
            let mut host_style = ui.runtime.tree().node(host).unwrap().props.style.clone();
            let tracks = if fill {
                vec![GridTrack::Fr(1.0)]
            } else if reverse {
                vec![GridTrack::Fr(1.0), GridTrack::Auto]
            } else {
                vec![GridTrack::Auto, GridTrack::Fr(1.0)]
            };
            if horizontal {
                host_style.grid_columns = tracks;
            } else {
                host_style.grid_rows = tracks;
            }
            ui.runtime
                .apply(Mutation::SetStyle {
                    node: host,
                    style: host_style,
                })
                .map_err(err)?;
            let child_id = self.mount_element(
                ui,
                host,
                child,
                if horizontal && !fill {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                },
                width,
                context,
            )?;
            let mut child_style = ui
                .runtime
                .tree()
                .node(child_id)
                .unwrap()
                .props
                .style
                .clone();
            child_style.grid_column = GridPlacement {
                start: Some(1),
                span: 1,
            };
            child_style.grid_row = GridPlacement {
                start: Some(1),
                span: 1,
            };
            let placement = GridPlacement {
                start: Some(if reverse && !fill { 2 } else { 1 }),
                span: 1,
            };
            if horizontal {
                child_style.grid_column = placement;
            } else {
                child_style.grid_row = placement;
            }
            ui.runtime
                .apply(Mutation::SetStyle {
                    node: child_id,
                    style: child_style,
                })
                .map_err(err)?;
            if !fill {
                let placement = GridPlacement {
                    start: Some(if reverse { 1 } else { 2 }),
                    span: 1,
                };
                host = ui
                    .transparent_panel(
                        host,
                        LayoutStyle {
                            kind: LayoutKind::Grid,
                            grid_columns: vec![GridTrack::Fr(1.0)],
                            grid_rows: vec![GridTrack::Fr(1.0)],
                            grid_column: if horizontal {
                                placement
                            } else {
                                GridPlacement {
                                    start: Some(1),
                                    span: 1,
                                }
                            },
                            grid_row: if horizontal {
                                GridPlacement {
                                    start: Some(1),
                                    span: 1,
                                }
                            } else {
                                placement
                            },
                            ..Default::default()
                        },
                    )
                    .map_err(err)?;
            }
        }
        Ok(root)
    }

    pub(crate) fn update_form_layout(
        &mut self,
        ui: &mut Controls,
        viewport: Size,
    ) -> Result<(), String> {
        if self.sync_automatic_heights(ui)? {
            ui.update(viewport).map_err(err)?;
        }
        for scroll in &self.scrolls {
            let Some(bounds) = ui.runtime.bounds(scroll.viewport) else {
                continue;
            };
            let mut extent = Size::ZERO;
            let mut pending = vec![(scroll.child, rust_desktop_ui_core::Point::ZERO)];
            while let Some((id, origin)) = pending.pop() {
                let Some(node) = ui.runtime.tree().node(id) else {
                    continue;
                };
                if node.props.collapsed {
                    continue;
                }
                let Some(local) = ui.runtime.layout_bounds(id) else {
                    continue;
                };
                let origin =
                    rust_desktop_ui_core::Point::new(origin.x + local.x, origin.y + local.y);
                extent.width = extent
                    .width
                    .max(origin.x + local.width + node.props.style.margin.right);
                extent.height = extent
                    .height
                    .max(origin.y + local.height + node.props.style.margin.bottom);
                // A nested viewport clips its own overflowing content; it must
                // not enlarge this outer viewer. Hidden content still measures.
                if !node.props.clip {
                    pending.extend(
                        ui.runtime
                            .tree()
                            .children(id)
                            .unwrap_or_default()
                            .iter()
                            .map(|child| (*child, origin)),
                    );
                }
            }
            let height = if scroll.vertical == x::ScrollBarVisibility::Disabled {
                bounds.height
            } else {
                extent.height.max(bounds.height)
            };
            let width = if scroll.horizontal == x::ScrollBarVisibility::Disabled {
                bounds.width
            } else {
                extent.width.max(bounds.width)
            };
            ui.set_scroll_measured_extent(scroll.viewport, Size::new(width, height))
                .map_err(err)?;
        }
        if !self.scrolls.is_empty() {
            ui.update(viewport).map_err(err)?;
        }
        for border in &self.decorations {
            let Some(bounds) = ui.runtime.bounds(border.id) else {
                continue;
            };
            let mut scene = Scene::default();
            if let Some(background) = border.background {
                scene.rounded_fill(bounds, bounds, border.radius, background);
            }
            if let Some(color) = border.border {
                scene.rounded_stroke(bounds, bounds, border.radius, border.thickness, color);
            }
            ui.set_custom_scene(border.id, scene).map_err(err)?;
        }
        if !self.decorations.is_empty() {
            ui.update(viewport).map_err(err)?;
        }
        Ok(())
    }
}
