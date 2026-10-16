//! A bounded item-data projection, separate from the Window binding namescope.
use crate::*;

/// Validates readonly ListBox and combo item presentations. Text bindings refer
/// only to `Text`, `Key`, or `Column0` through `Column31` on the current item.
pub fn validate_item_template(root: &ElementDefinition) -> Result<(), String> {
    let mut pending = vec![(root, 1)];
    let mut count = 0;
    while let Some((node, depth)) = pending.pop() {
        count += 1;
        if count > 32 || depth > 8 {
            return Err("ItemTemplate supports at most 32 nodes and 8 levels".into());
        }
        let c = &node.common;
        if c.name.is_some()
            || c.data_context.is_some()
            || c.control_template.is_some()
            || c.rich_tool_tip.is_some()
            || c.tool_tip.is_some()
            || c.automation_name.is_some()
            || c.repeat_delay_ms.is_some()
            || c.is_link
            || !c.is_enabled
            || c.visibility != Visibility::Visible
            || c.grid_row != 0
            || c.grid_column != 0
            || c.grid_row_span != 1
            || c.grid_column_span != 1
        {
            return Err("ItemTemplate content is readonly and has no names, events, metadata or external context".into());
        }
        if c.width
            .into_iter()
            .chain(c.height)
            .chain(c.max_width)
            .chain(c.max_height)
            .chain([c.min_width, c.min_height])
            .chain(c.margin)
            .any(|value| !value.is_finite() || !(0.0..=4096.0).contains(&value))
        {
            return Err("ItemTemplate dimensions must be finite and in [0,4096]".into());
        }
        if c.max_width.is_some_and(|max| max < c.min_width)
            || c.max_height.is_some_and(|max| max < c.min_height)
        {
            return Err("ItemTemplate minimum exceeds maximum".into());
        }
        for (property, binding) in &c.bindings {
            let source = matches!(binding.path.as_str(), "Text" | "Key")
                || binding
                    .path
                    .strip_prefix("Column")
                    .and_then(|v| v.parse::<usize>().ok())
                    .is_some_and(|i| i < 32);
            if property != "Text"
                || !matches!(node.kind, ElementKind::TextBlock(_))
                || !source
                || !matches!(binding.mode, BindingMode::OneWay | BindingMode::OneTime)
                || binding.element_name.is_some()
                || binding.converter.is_some()
                || binding.fallback_value.is_some()
                || !binding.inputs.is_empty()
                || binding.expression.is_some()
            {
                return Err(
                    "ItemTemplate Text accepts readonly item fields Text, Key or Column0..Column31"
                        .into(),
                );
            }
        }
        match &node.kind {
            ElementKind::StackPanel(panel) => {
                if panel.surface != SurfaceKindDefinition::Transparent
                    || !panel.spacing.is_finite()
                    || !(0.0..=4096.0).contains(&panel.spacing)
                    || panel
                        .padding
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=4096.0).contains(v))
                {
                    return Err("Invalid ItemTemplate panel geometry".into());
                }
                pending.extend(panel.children.iter().map(|child| (child, depth + 1)));
            }
            ElementKind::TextBlock(text) => {
                if text.text.len() > 4096
                    || !text.inlines.is_empty()
                    || text.trimming != TextTrimmingDefinition::None
                    || text.max_lines.is_some()
                {
                    return Err(
                        "ItemTemplate text supports a bounded plain string and TextStyle".into(),
                    );
                }
            }
            ElementKind::Icon(_) | ElementKind::Separator(_) => {}
            _ => {
                return Err(
                    "ItemTemplate supports StackPanel, TextBlock, Icon and Separator only".into(),
                );
            }
        }
    }
    Ok(())
}

pub fn validate_selection_presentation(
    kind: &ElementKind,
    common: &CommonProperties,
) -> Result<(), String> {
    if let ElementKind::LookupEdit(lookup) = kind {
        let mut seen = std::collections::HashSet::new();
        if lookup.search_columns.len() > 32
            || lookup.search_columns.iter().any(|index| {
                *index >= 32
                    || !seen.insert(*index)
                    || !lookup.columns.is_empty() && *index >= lookup.columns.len()
            })
        {
            return Err("Lookup SearchColumns must be unique valid column indices 0..31".into());
        }
    }
    let (selection, combo, lookup) = match kind {
        ElementKind::ListBox(s) => (s, false, false),
        ElementKind::ComboBox(s) => (s, true, false),
        ElementKind::LookupEdit(s) => (&s.selection, true, true),
        _ => return Ok(()),
    };
    if let Some(template) = &selection.item_template {
        validate_item_template(template)?;
    }
    if selection.item_presentation == ListItemPresentationDefinition::CheckBox
        && selection.selection_mode != SelectionMode::Multiple
        || selection.item_presentation == ListItemPresentationDefinition::RadioButton
            && selection.selection_mode != SelectionMode::Single
    {
        return Err("CheckBox items require Multiple; RadioButton items require Single".into());
    }
    if combo && selection.item_presentation != ListItemPresentationDefinition::Plain {
        return Err("Combo popup presentation follows its selection mode".into());
    }
    if selection.selection_presentation == ComboSelectionPresentationDefinition::Tokens
        && (!combo || selection.selection_mode != SelectionMode::Multiple)
    {
        return Err("Tokens require a Multiple ComboBox".into());
    }
    if !combo && selection.commit_on_lost_focus.is_some() {
        return Err("CommitOnLostFocus selection policy requires a ComboBox".into());
    }
    let has_text = !selection.text.is_empty() || common.bindings.contains_key("Text");
    let has_single =
        selection.selected_value.is_some() || common.bindings.contains_key("SelectedValue");
    let has_multiple =
        !selection.selected_values.is_empty() || common.bindings.contains_key("SelectedValues");
    if has_single && has_multiple {
        return Err("SelectedValue and SelectedValues are mutually exclusive".into());
    }
    if has_text
        && (!combo
            || !selection.is_editable
            || selection.selection_mode != SelectionMode::Single
            || lookup)
    {
        return Err("Free Text requires an editable Single ComboBox".into());
    }
    if has_text && has_single {
        return Err(
            "Bind either Text or SelectedValue as the authoritative Single ComboBox value".into(),
        );
    }
    if combo && selection.selection_mode == SelectionMode::Multiple && has_single {
        return Err("Multiple ComboBox binds SelectedValues, not SelectedValue".into());
    }
    if combo && selection.selected_values.len() > 256 {
        return Err("ComboBox supports at most 256 selected values".into());
    }
    Ok(())
}
