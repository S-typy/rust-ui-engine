use crate::*;

pub(super) fn new_kind(name: &str) -> Option<ElementKind> {
    Some(match name {
        "Grid" => ElementKind::Grid(GridDefinition::default()),
        "Border" => ElementKind::Border(BorderDefinition::default()),
        "GroupBox" => ElementKind::GroupBox(GroupBoxDefinition::default()),
        "Expander" => ElementKind::Expander(ExpanderDefinition::default()),
        "ScrollViewer" => ElementKind::ScrollViewer(ScrollViewerDefinition::default()),
        "WrapPanel" => ElementKind::WrapPanel(StackPanelDefinition {
            orientation: Orientation::Horizontal,
            ..Default::default()
        }),
        "DockPanel" => ElementKind::DockPanel(DockPanelDefinition::default()),
        "ListBox" => ElementKind::ListBox(SelectionDefinition::default()),
        "ComboBox" => ElementKind::ComboBox(SelectionDefinition::default()),
        "LookupEdit" => ElementKind::LookupEdit(LookupDefinition::default()),
        "ButtonEdit" => ElementKind::ButtonEdit(ButtonEditDefinition::default()),
        "SpinBox" => ElementKind::SpinBox(SpinBoxDefinition::default()),
        "DateEdit" => ElementKind::DateEdit(TemporalEditDefinition {
            format: "dd.MM.yyyy".into(),
            ..Default::default()
        }),
        "TimeEdit" => ElementKind::TimeEdit(TemporalEditDefinition {
            format: "HH:mm".into(),
            ..Default::default()
        }),
        _ => return None,
    })
}

pub(super) fn is_binding_property(kind: &ElementKind, name: &str) -> bool {
    if matches!(name, "IsEnabled" | "Visibility" | "Width" | "Height") {
        return true;
    }
    match kind {
        ElementKind::GroupBox(_) => name == "Header",
        ElementKind::Expander(_) => matches!(name, "Header" | "IsExpanded"),
        ElementKind::TextBox(_) | ElementKind::ButtonEdit(_) => {
            matches!(name, "Text" | "IsReadOnly" | "PlaceholderText")
        }
        ElementKind::SpinBox(_) | ElementKind::DateEdit(_) | ElementKind::TimeEdit(_) => {
            matches!(name, "Value" | "IsReadOnly" | "PlaceholderText")
        }
        ElementKind::TextBlock(_) => name == "Text",
        ElementKind::Button(_) | ElementKind::DropDownButton(_) | ElementKind::SplitButton(_) => {
            name == "Content"
        }
        ElementKind::ToggleButton(_) | ElementKind::CheckBox(_) | ElementKind::RadioButton(_) => {
            matches!(name, "Content" | "IsChecked")
        }
        ElementKind::ListBox(_) | ElementKind::ComboBox(_) | ElementKind::LookupEdit(_) => {
            matches!(
                name,
                "ItemsSource"
                    | "SelectedValue"
                    | "SelectedValues"
                    | "IsReadOnly"
                    | "PlaceholderText"
            )
        }
        _ => false,
    }
}

pub(super) fn common_property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    common: &mut CommonProperties,
) -> Result<bool, Diagnostic> {
    let value = attr.value();
    let metric = || editor_metric(document, attr, MAX_DIMENSION, true);
    match attr.name() {
        "Visibility" => {
            common.visibility = match value {
                "Visible" => Visibility::Visible,
                "Hidden" => Visibility::Hidden,
                "Collapsed" => Visibility::Collapsed,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "Visibility must be Visible, Hidden or Collapsed",
                    ));
                }
            }
        }
        "MinWidth" => common.min_width = metric()?,
        "MinHeight" => common.min_height = metric()?,
        "MaxWidth" => common.max_width = Some(metric()?),
        "MaxHeight" => common.max_height = Some(metric()?),
        "HorizontalAlignment" => {
            common.horizontal_alignment = match value {
                "Left" => Alignment::Start,
                "Center" => Alignment::Center,
                "Right" => Alignment::End,
                "Stretch" => Alignment::Stretch,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "HorizontalAlignment must be Left, Center, Right or Stretch",
                    ));
                }
            }
        }
        "VerticalAlignment" => {
            common.vertical_alignment = match value {
                "Top" => Alignment::Start,
                "Center" => Alignment::Center,
                "Bottom" => Alignment::End,
                "Stretch" => Alignment::Stretch,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "VerticalAlignment must be Top, Center, Bottom or Stretch",
                    ));
                }
            }
        }
        "Grid.Row" => common.grid_row = grid_index(document, attr, false)?,
        "Grid.Column" => common.grid_column = grid_index(document, attr, false)?,
        "Grid.RowSpan" => common.grid_row_span = grid_index(document, attr, true)?,
        "Grid.ColumnSpan" => common.grid_column_span = grid_index(document, attr, true)?,
        "DockPanel.Dock" => {
            common.dock = match value {
                "Left" => DockSide::Left,
                "Top" => DockSide::Top,
                "Right" => DockSide::Right,
                "Bottom" => DockSide::Bottom,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "DockPanel.Dock must be Left, Top, Right or Bottom",
                    ));
                }
            }
        }
        "DataContext" => {
            let path = if value.trim_start().starts_with('{') {
                let parsed = binding::parse_binding(
                    document,
                    attr,
                    false,
                    UpdateSourceTrigger::PropertyChanged,
                )?;
                if parsed.element_name.is_some()
                    || parsed.converter.is_some()
                    || parsed.fallback_value.is_some()
                    || parsed.mode != BindingMode::OneWay
                {
                    return Err(at_attribute(
                        document,
                        attr,
                        "DataContext supports a relative OneWay property path",
                    ));
                }
                parsed.path
            } else {
                value.into()
            };
            if !binding::valid_path(&path) {
                return Err(at_attribute(
                    document,
                    attr,
                    "DataContext must be a property path",
                ));
            }
            common.data_context = Some(path);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn grid_index(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    span: bool,
) -> Result<u16, Diagnostic> {
    attr.value()
        .parse::<u16>()
        .ok()
        .filter(|v| {
            if span {
                (1..=1024).contains(v)
            } else {
                *v < 1024
            }
        })
        .ok_or_else(|| {
            at_attribute(
                document,
                attr,
                "Grid index must be 0..1023; span must be 1..1024",
            )
        })
}

fn selection_property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    selection: &mut SelectionDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "SelectedValue" => selection.selected_value = Some(attr.value().into()),
        "SelectedValues" => {
            selection.selected_values = attr
                .value()
                .split(',')
                .map(|v| v.trim().to_owned())
                .collect()
        }
        "DisplayMemberPath" | "ValueMemberPath" => {
            if !binding::valid_path(attr.value()) {
                return Err(at_attribute(
                    document,
                    attr,
                    "member path must contain property identifiers",
                ));
            }
            if attr.name() == "DisplayMemberPath" {
                selection.display_member_path = Some(attr.value().into());
            } else {
                selection.value_member_path = Some(attr.value().into());
            }
        }
        "PlaceholderText" => selection.placeholder = attr.value().into(),
        "IsReadOnly" => selection.is_read_only = boolean(document, attr)?,
        "IsEditable" => selection.is_editable = boolean(document, attr)?,
        "SelectionMode" => {
            selection.selection_mode = match attr.value() {
                "Single" => SelectionMode::Single,
                "Multiple" => SelectionMode::Multiple,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "SelectionMode must be Single or Multiple",
                    ));
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn number(document: &Document<'_>, attr: Attribute<'_, '_>) -> Result<f64, Diagnostic> {
    attr.value()
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite() && x.abs() <= 1e15)
        .ok_or_else(|| {
            at_attribute(
                document,
                attr,
                "number must be finite, with absolute value at most 1e15",
            )
        })
}

pub(super) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    kind: &mut ElementKind,
) -> Result<bool, Diagnostic> {
    let metric = || editor_metric(document, attr, MAX_DIMENSION, true);
    match kind {
        ElementKind::GroupBox(group) => group_property(
            document,
            attr,
            &mut group.header,
            &mut group.appearance,
            &mut group.padding,
        ),
        ElementKind::Expander(expander) => {
            if attr.name() == "IsExpanded" {
                expander.is_expanded = boolean(document, attr)?;
                Ok(true)
            } else {
                group_property(
                    document,
                    attr,
                    &mut expander.header,
                    &mut expander.appearance,
                    &mut expander.padding,
                )
            }
        }
        ElementKind::ListBox(selection) | ElementKind::ComboBox(selection) => {
            selection_property(document, attr, selection)
        }
        ElementKind::LookupEdit(lookup) => {
            selection_property(document, attr, &mut lookup.selection)
        }
        ElementKind::ButtonEdit(editor) => editor_property(document, attr, &mut editor.editor),
        ElementKind::SpinBox(editor) => {
            match attr.name() {
                "Value" => editor.value = number(document, attr)?,
                "Minimum" => editor.minimum = number(document, attr)?,
                "Maximum" => editor.maximum = number(document, attr)?,
                "Increment" => editor.increment = number(document, attr)?,
                "Text" => return Ok(false),
                _ => return editor_property(document, attr, &mut editor.editor),
            }
            Ok(true)
        }
        ElementKind::DateEdit(editor) | ElementKind::TimeEdit(editor) => {
            match attr.name() {
                "Value" => editor.value = Some(attr.value().into()),
                "Minimum" => editor.minimum = Some(attr.value().into()),
                "Maximum" => editor.maximum = Some(attr.value().into()),
                "Format" => editor.format = attr.value().into(),
                "Text" => return Ok(false),
                _ => return editor_property(document, attr, &mut editor.editor),
            }
            Ok(true)
        }
        ElementKind::Grid(grid) => {
            match attr.name() {
                "Spacing" => grid.spacing = metric()?,
                "Padding" => grid.padding = thickness(document, attr)?,
                "Surface" => grid.surface = surface(document, attr)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::Border(border) => {
            match attr.name() {
                "Padding" => border.padding = thickness(document, attr)?,
                "Background" => border.background = Some(color(document, attr)?),
                "BorderBrush" => border.border_brush = Some(color(document, attr)?),
                "BorderThickness" => border.border_thickness = metric()?,
                "CornerRadius" => border.corner_radius = metric()?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::ScrollViewer(scroll) => {
            if !matches!(
                attr.name(),
                "HorizontalScrollBarVisibility" | "VerticalScrollBarVisibility"
            ) {
                return Ok(false);
            }
            let value = match attr.value() {
                "Auto" => ScrollBarVisibility::Auto,
                "Hidden" => ScrollBarVisibility::Hidden,
                "Disabled" => ScrollBarVisibility::Disabled,
                "Visible" => ScrollBarVisibility::Visible,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "scroll bar visibility must be Auto, Hidden, Disabled or Visible",
                    ));
                }
            };
            if attr.name().starts_with("Horizontal") {
                scroll.horizontal_scroll_bar_visibility = value;
            } else {
                scroll.vertical_scroll_bar_visibility = value;
            }
            Ok(true)
        }
        ElementKind::DockPanel(panel) => {
            match attr.name() {
                "Padding" => panel.padding = thickness(document, attr)?,
                "LastChildFill" => panel.last_child_fill = boolean(document, attr)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn group_property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    header: &mut String,
    appearance: &mut GroupBoxAppearanceDefinition,
    padding: &mut [f32; 4],
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Header" => *header = attr.value().into(),
        "Padding" => *padding = thickness(document, attr)?,
        "Appearance" => {
            *appearance = match attr.value() {
                "Card" => GroupBoxAppearanceDefinition::Card,
                "Outline" => GroupBoxAppearanceDefinition::Outline,
                "Plain" => GroupBoxAppearanceDefinition::Plain,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "Appearance must be Card, Outline or Plain",
                    ));
                }
            };
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn surface(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
) -> Result<SurfaceKindDefinition, Diagnostic> {
    match attr.value() {
        "Transparent" => Ok(SurfaceKindDefinition::Transparent),
        "Card" => Ok(SurfaceKindDefinition::Card),
        _ => Err(at_attribute(
            document,
            attr,
            "Surface must be Transparent or Card",
        )),
    }
}

fn track(node: Node<'_, '_>, row: bool) -> Result<GridLength, Diagnostic> {
    require_element(
        node,
        if row {
            "RowDefinition"
        } else {
            "ColumnDefinition"
        },
    )?;
    let mut result = GridLength::Star(1.0);
    for attr in node.attributes() {
        reject_extension(node.document(), attr)?;
        if attr.namespace().is_some() || attr.name() != if row { "Height" } else { "Width" } {
            return Err(at_attribute(
                node.document(),
                attr,
                "unsupported track property",
            ));
        }
        result = if attr.value() == "Auto" {
            GridLength::Auto
        } else if let Some(value) = attr.value().strip_suffix('*') {
            let value = if value.is_empty() {
                Some(1.0)
            } else {
                value.parse::<f32>().ok()
            };
            GridLength::Star(
                value
                    .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_DIMENSION as f32)
                    .ok_or_else(|| {
                        at_attribute(
                            node.document(),
                            attr,
                            "star weight must be positive and finite",
                        )
                    })?,
            )
        } else {
            GridLength::Pixel(editor_metric(node.document(), attr, MAX_DIMENSION, true)?)
        };
    }
    for child in node.children() {
        require_empty_content(child)?;
    }
    Ok(result)
}

fn item(node: Node<'_, '_>) -> Result<SelectionItemDefinition, Diagnostic> {
    require_element(node, "ListBoxItem")?;
    let mut result = SelectionItemDefinition::default();
    for attr in node.attributes() {
        reject_extension(node.document(), attr)?;
        if attr.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attr,
                "ListBoxItem does not support directives",
            ));
        }
        match attr.name() {
            "Value" => result.value = attr.value().into(),
            "Content" => result.display = attr.value().into(),
            "IsEnabled" => result.is_enabled = boolean(node.document(), attr)?,
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "unsupported ListBoxItem property",
                ));
            }
        }
    }
    if node.attribute("Value").is_none() {
        result.value = result.display.clone();
    }
    if node.attribute("Content").is_none() {
        result.display = result.value.clone();
    }
    for child in node.children() {
        require_empty_content(child)?;
    }
    Ok(result)
}

fn column(node: Node<'_, '_>) -> Result<LookupColumnDefinition, Diagnostic> {
    require_element(node, "LookupColumn")?;
    let mut result = LookupColumnDefinition {
        field: String::new(),
        header: String::new(),
        width: 120.0,
    };
    for attr in node.attributes() {
        reject_extension(node.document(), attr)?;
        if attr.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attr,
                "LookupColumn does not support directives",
            ));
        }
        match attr.name() {
            "Field" => result.field = attr.value().into(),
            "Header" => result.header = attr.value().into(),
            "Width" => result.width = editor_dimension(node.document(), attr)?,
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "unsupported LookupColumn property",
                ));
            }
        }
    }
    if !binding::valid_path(&result.field) {
        return Err(at_node(node, "LookupColumn requires a valid Field path"));
    }
    for child in node.children() {
        require_empty_content(child)?;
    }
    Ok(result)
}

fn editor_button(node: Node<'_, '_>) -> Result<EditorButtonDefinition, Diagnostic> {
    require_element(node, "EditorButton")?;
    let mut result = EditorButtonDefinition::default();
    for attr in node.attributes() {
        reject_extension(node.document(), attr)?;
        if attr.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attr,
                "EditorButton does not support directives",
            ));
        }
        match attr.name() {
            "Command" => {
                identifier(node.document(), attr, "Command")?;
                result.command = attr.value().into();
            }
            "Icon" => result.icon = Some(icon_symbol(node.document(), attr)?),
            "Content" => result.content = attr.value().into(),
            "ToolTip" if attr.value().len() <= MAX_TOOLTIP_BYTES => {
                result.tool_tip = Some(attr.value().into())
            }
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "unsupported EditorButton property",
                ));
            }
        }
    }
    if result.command.is_empty() {
        return Err(at_node(node, "EditorButton requires Command"));
    }
    if result.content.is_empty() && result.tool_tip.as_deref().is_none_or(str::is_empty) {
        return Err(at_node(
            node,
            "EditorButton requires Content or ToolTip for its accessible name",
        ));
    }
    for child in node.children() {
        require_empty_content(child)?;
    }
    Ok(result)
}

pub(super) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    parent: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    groups: &mut HashSet<RadioGroupKey>,
    kind: &mut ElementKind,
) -> Result<(), Diagnostic> {
    let mut single = None;
    let mut properties = HashSet::new();
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE) {
            return Err(at_node(child, "unsupported child namespace"));
        }
        let name = child.tag_name().name();
        let property = matches!(
            name,
            "Grid.RowDefinitions"
                | "Grid.ColumnDefinitions"
                | "ButtonEdit.Buttons"
                | "LookupEdit.Columns"
        );
        if property {
            if !matches!(
                (&*kind, name),
                (
                    ElementKind::Grid(_),
                    "Grid.RowDefinitions" | "Grid.ColumnDefinitions"
                ) | (ElementKind::ButtonEdit(_), "ButtonEdit.Buttons")
                    | (ElementKind::LookupEdit(_), "LookupEdit.Columns")
            ) {
                return Err(at_node(
                    child,
                    "property element does not belong to this control",
                ));
            }
            if !properties.insert(name) {
                return Err(at_node(child, "duplicate property element"));
            }
            if child.attributes().len() != 0 {
                return Err(at_node(child, "property elements cannot have attributes"));
            }
            *count += 1;
            for entry in child.children() {
                if !entry.is_element() {
                    require_empty_content(entry)?;
                    continue;
                }
                *count += 1;
                match (&mut *kind, name) {
                    (ElementKind::Grid(grid), "Grid.RowDefinitions") => {
                        grid.rows.push(track(entry, true)?)
                    }
                    (ElementKind::Grid(grid), "Grid.ColumnDefinitions") => {
                        grid.columns.push(track(entry, false)?)
                    }
                    (ElementKind::ButtonEdit(editor), "ButtonEdit.Buttons") => {
                        editor.buttons.push(editor_button(entry)?)
                    }
                    (ElementKind::LookupEdit(lookup), "LookupEdit.Columns") => {
                        lookup.columns.push(column(entry)?)
                    }
                    _ => {
                        return Err(at_node(
                            child,
                            "property element does not belong to this control",
                        ));
                    }
                }
            }
        } else {
            match kind {
                ElementKind::ListBox(selection) | ElementKind::ComboBox(selection) => {
                    *count += 1;
                    selection.items.push(item(child)?);
                }
                ElementKind::LookupEdit(lookup) => {
                    *count += 1;
                    lookup.selection.items.push(item(child)?);
                }
                ElementKind::Grid(grid) => grid.children.push(parse_element(
                    child,
                    depth + 1,
                    parent,
                    count,
                    names,
                    groups,
                )?),
                ElementKind::DockPanel(panel) => panel.children.push(parse_element(
                    child,
                    depth + 1,
                    parent,
                    count,
                    names,
                    groups,
                )?),
                ElementKind::Border(_)
                | ElementKind::ScrollViewer(_)
                | ElementKind::GroupBox(_)
                | ElementKind::Expander(_) => {
                    if single.is_some() {
                        return Err(at_node(child, "container accepts at most one child"));
                    }
                    single = Some(parse_element(
                        child,
                        depth + 1,
                        parent,
                        count,
                        names,
                        groups,
                    )?);
                }
                _ => return Err(at_node(child, "unsupported child element")),
            }
        }
        if *count > MAX_ELEMENTS || depth + if property { 2 } else { 1 } > MAX_ELEMENT_DEPTH {
            return Err(at_node(
                child,
                "content exceeds element count or depth limit",
            ));
        }
    }
    match kind {
        ElementKind::GroupBox(group) => group.child = single.map(Box::new),
        ElementKind::Expander(expander) => expander.child = single.map(Box::new),
        ElementKind::Border(border) => {
            *border.child = single.ok_or_else(|| at_node(node, "Border requires one child"))?
        }
        ElementKind::ScrollViewer(scroll) => {
            *scroll.child =
                single.ok_or_else(|| at_node(node, "ScrollViewer requires one child"))?
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate(
    node: Node<'_, '_>,
    kind: &ElementKind,
    common: &CommonProperties,
) -> Result<(), Diagnostic> {
    if common.max_width.is_some_and(|max| max < common.min_width)
        || common.max_height.is_some_and(|max| max < common.min_height)
    {
        return Err(at_node(node, "maximum size must be at least the minimum"));
    }
    if usize::from(common.grid_row) + usize::from(common.grid_row_span) > 1024
        || usize::from(common.grid_column) + usize::from(common.grid_column_span) > 1024
    {
        return Err(at_node(node, "Grid placement exceeds 1024 tracks"));
    }
    let selection = match kind {
        ElementKind::ListBox(s) | ElementKind::ComboBox(s) => Some(s),
        ElementKind::LookupEdit(l) => Some(&l.selection),
        _ => None,
    };
    if let Some(selection) = selection {
        if !selection.items.is_empty() && common.bindings.contains_key("ItemsSource") {
            return Err(at_node(
                node,
                "static items cannot be combined with ItemsSource",
            ));
        }
        let mut values = HashSet::new();
        if selection
            .items
            .iter()
            .any(|item| !values.insert(&item.value))
        {
            return Err(at_node(node, "item values must be unique"));
        }
        if selection.selected_value.is_some() && !selection.selected_values.is_empty() {
            return Err(at_node(
                node,
                "SelectedValue and SelectedValues are mutually exclusive",
            ));
        }
        if !matches!(kind, ElementKind::ListBox(_))
            && selection.selection_mode == SelectionMode::Multiple
        {
            return Err(at_node(node, "only ListBox supports Multiple selection"));
        }
        if matches!(kind, ElementKind::ListBox(_)) && selection.is_editable {
            return Err(at_node(node, "ListBox does not support IsEditable=True"));
        }
        if selection.selection_mode == SelectionMode::Single && selection.selected_values.len() > 1
        {
            return Err(at_node(
                node,
                "Single selection accepts at most one selected value",
            ));
        }
        if !selection.items.is_empty() {
            for value in selection
                .selected_values
                .iter()
                .chain(selection.selected_value.iter())
            {
                if !selection
                    .items
                    .iter()
                    .any(|item| &item.value == value && item.is_enabled)
                {
                    return Err(at_node(
                        node,
                        "initial selection must reference an enabled item",
                    ));
                }
            }
        }
    }
    match kind {
        ElementKind::GroupBox(group) => {
            validate_group_box(group).map_err(|message| at_node(node, message))?
        }
        ElementKind::Expander(expander) => {
            validate_expander(expander).map_err(|message| at_node(node, message))?
        }
        ElementKind::LookupEdit(lookup) => {
            if let (Some(display), Some(first)) = (
                &lookup.selection.display_member_path,
                lookup.columns.first(),
            ) && display != &first.field
            {
                return Err(at_node(
                    node,
                    "the first LookupColumn Field must match DisplayMemberPath",
                ));
            }
        }
        ElementKind::ButtonEdit(editor) => {
            if editor.buttons.is_empty() || editor.buttons.len() > 8 {
                return Err(at_node(node, "ButtonEdit requires 1..8 EditorButtons"));
            }
            validate_initial_text(node, &editor.editor)?;
        }
        ElementKind::SpinBox(editor) => {
            if editor.minimum > editor.maximum
                || editor.increment <= 0.0
                || (!common.bindings.contains_key("Value")
                    && !(editor.minimum..=editor.maximum).contains(&editor.value))
            {
                return Err(at_node(
                    node,
                    "SpinBox requires Minimum <= Value <= Maximum and positive Increment",
                ));
            }
        }
        ElementKind::DateEdit(editor) | ElementKind::TimeEdit(editor) => {
            let date = matches!(kind, ElementKind::DateEdit(_));
            let valid_formats: &[&str] = if date {
                &["yyyy-MM-dd", "dd.MM.yyyy"]
            } else {
                &["HH:mm", "HH:mm:ss"]
            };
            if !valid_formats.contains(&editor.format.as_str()) {
                return Err(at_node(node, "unsupported date/time Format"));
            }
            let values = [&editor.minimum, &editor.value, &editor.maximum]
                .map(|v| v.as_deref().map(|s| temporal_value(s, date)).transpose());
            let [minimum, value, maximum] = values;
            let minimum = minimum.map_err(|m| at_node(node, m))?;
            let value = value.map_err(|m| at_node(node, m))?;
            let maximum = maximum.map_err(|m| at_node(node, m))?;
            if !date
                && editor.format == "HH:mm"
                && [minimum, value, maximum]
                    .into_iter()
                    .flatten()
                    .any(|seconds| !seconds.is_multiple_of(60))
            {
                return Err(at_node(node, "nonzero seconds require Format=HH:mm:ss"));
            }
            if minimum.zip(maximum).is_some_and(|(a, b)| a > b)
                || value.zip(minimum).is_some_and(|(v, m)| v < m)
                || value.zip(maximum).is_some_and(|(v, m)| v > m)
            {
                return Err(at_node(
                    node,
                    "date/time value lies outside Minimum/Maximum",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn temporal_value(value: &str, date: bool) -> Result<u32, &'static str> {
    let invalid = "expected a valid ISO date YYYY-MM-DD or time HH:mm[:ss]";
    if date {
        if value.len() != 10
            || !value.bytes().enumerate().all(|(i, b)| {
                if matches!(i, 4 | 7) {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
        {
            return Err(invalid);
        }
        let year = value[..4].parse::<u32>().map_err(|_| invalid)?;
        let month = value[5..7].parse::<u32>().map_err(|_| invalid)?;
        let day = value[8..].parse::<u32>().map_err(|_| invalid)?;
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        };
        if year == 0 || day == 0 || day > days {
            return Err(invalid);
        }
        Ok(year * 10000 + month * 100 + day)
    } else {
        if !matches!(value.len(), 5 | 8)
            || !value.bytes().enumerate().all(|(i, b)| {
                if matches!(i, 2 | 5) {
                    b == b':'
                } else {
                    b.is_ascii_digit()
                }
            })
        {
            return Err(invalid);
        }
        let parts = value
            .split(':')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid)?;
        if parts[0] > 23 || parts[1] > 59 || parts.get(2).is_some_and(|s| *s > 59) {
            return Err(invalid);
        }
        Ok(parts[0] * 3600 + parts[1] * 60 + parts.get(2).copied().unwrap_or(0))
    }
}
