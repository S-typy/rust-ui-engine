use crate::*;

pub(super) fn new_kind(name: &str) -> Option<ElementKind> {
    Some(match name {
        "Grid" => ElementKind::Grid(GridDefinition::default()),
        "GridControl" => ElementKind::GridControl(GridControlDefinition::default()),
        "TreeView" => ElementKind::TreeView(TreeDefinition::default()),
        "TreeGrid" => ElementKind::TreeGrid(TreeDefinition::default()),
        "Rating" => ElementKind::Rating(RatingDefinition::default()),
        "Breadcrumb" => ElementKind::Breadcrumb(BreadcrumbDefinition::default()),
        "ColorEdit" => ElementKind::ColorEdit(ColorEditorDefinition::default()),
        "BrushEdit" => ElementKind::BrushEdit(ColorEditorDefinition::default()),
        "NavigationPane" | "Accordion" | "NavigationFrame" => {
            ElementKind::NavigationPane(Box::new(NavigationDefinition {
                view: match name {
                    "Accordion" => NavigationViewDefinition::Accordion,
                    "NavigationFrame" => NavigationViewDefinition::Frame,
                    _ => NavigationViewDefinition::Side,
                },
                ..Default::default()
            }))
        }
        "FontPicker" => ElementKind::FontPicker(FontPickerDefinition::default()),
        "DataForm" => ElementKind::DataForm(DataFormDefinition::default()),
        "Border" => ElementKind::Border(BorderDefinition::default()),
        "GroupBox" => ElementKind::GroupBox(GroupBoxDefinition::default()),
        "Expander" => ElementKind::Expander(ExpanderDefinition::default()),
        "TabbedGroup" => ElementKind::TabbedGroup(TabbedGroupDefinition::default()),
        "ScrollViewer" => ElementKind::ScrollViewer(ScrollViewerDefinition::default()),
        "WrapPanel" => ElementKind::WrapPanel(StackPanelDefinition {
            orientation: Orientation::Horizontal,
            ..Default::default()
        }),
        "DockPanel" => ElementKind::DockPanel(DockPanelDefinition::default()),
        "StatusBar" => ElementKind::StatusBar(StatusBarDefinition::default()),
        "RibbonStatusBar" => ElementKind::RibbonStatusBar(StatusBarDefinition::default()),
        "StatusBarItem" => ElementKind::StatusBarItem(StatusBarItemDefinition::default()),
        "ProgressBar" => ElementKind::ProgressBar(ProgressBarDefinition::default()),
        "ZoomControl" => ElementKind::ZoomControl(ZoomControlDefinition::default()),
        "ListBox" => ElementKind::ListBox(SelectionDefinition::default()),
        "ComboBox" => ElementKind::ComboBox(SelectionDefinition::default()),
        "LookupEdit" => ElementKind::LookupEdit(LookupDefinition::default()),
        "ButtonEdit" => ElementKind::ButtonEdit(ButtonEditDefinition::default()),
        "MemoEdit" => ElementKind::MemoEdit(MemoEditDefinition::default()),
        "SearchBox" => ElementKind::SearchBox(SearchBoxDefinition::default()),
        "PasswordBox" => ElementKind::PasswordBox(PasswordBoxDefinition::default()),
        "NumericEdit" => ElementKind::NumericEdit(NumericEditDefinition::default()),
        "MaskEdit" => ElementKind::MaskEdit(MaskEditDefinition::default()),
        "DateTimeEdit" => ElementKind::DateTimeEdit(DateTimeEditDefinition::default()),
        "DateRangeEdit" => ElementKind::DateRangeEdit(DateRangeEditDefinition::default()),
        "Calendar" => ElementKind::Calendar(CalendarDefinition::default()),
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
    if badges::binding_property(name)
        || matches!(name, "IsEnabled" | "Visibility" | "Width" | "Height")
    {
        return true;
    }
    match kind {
        ElementKind::Slider(_) => name == "Value",
        ElementKind::Rating(_) => matches!(name, "Value" | "IsReadOnly"),
        ElementKind::NavigationPane(_) => name == "Route",
        ElementKind::Breadcrumb(_) => matches!(name, "Value" | "IsReadOnly"),
        ElementKind::ColorEdit(_) | ElementKind::BrushEdit(_) => {
            matches!(name, "Value" | "IsReadOnly")
        }
        ElementKind::FontPicker(_) => matches!(name, "Value" | "IsReadOnly"),
        ElementKind::TreeView(_) | ElementKind::TreeGrid(_) => {
            matches!(name, "SelectedKey" | "IsReadOnly")
        }
        ElementKind::RangeSlider(_) => matches!(name, "LowerValue" | "UpperValue"),
        ElementKind::GridControl(_) => {
            matches!(name, "FilterText" | "SelectedValue" | "IsReadOnly")
        }
        ElementKind::ProgressBar(_) | ElementKind::BusyIndicator(_) => {
            matches!(name, "Value" | "IsIndeterminate")
        }
        ElementKind::ZoomControl(_) => name == "Value",
        ElementKind::RadioGroup(_) => matches!(name, "ItemsSource" | "SelectedKey"),
        ElementKind::GroupBox(_) => matches!(name, "Header" | "IsExpanded"),
        ElementKind::Expander(_) => matches!(name, "Header" | "IsExpanded"),
        ElementKind::TabbedGroup(_) => name == "SelectedIndex",
        ElementKind::TextBox(_) | ElementKind::ButtonEdit(_) | ElementKind::MemoEdit(_) => {
            matches!(name, "Text" | "IsReadOnly" | "PlaceholderText")
        }
        ElementKind::SearchBox(_) => matches!(name, "Query" | "IsReadOnly" | "PlaceholderText"),
        ElementKind::SpinBox(_)
        | ElementKind::NumericEdit(_)
        | ElementKind::MaskEdit(_)
        | ElementKind::DateTimeEdit(_)
        | ElementKind::DateRangeEdit(_)
        | ElementKind::DateEdit(_)
        | ElementKind::TimeEdit(_) => {
            matches!(name, "Value" | "IsReadOnly" | "PlaceholderText")
        }
        ElementKind::TextBlock(_) => name == "Text",
        ElementKind::Calendar(_) => matches!(name, "Value" | "IsReadOnly"),
        ElementKind::Button(_) | ElementKind::DropDownButton(_) | ElementKind::SplitButton(_) => {
            name == "Content"
        }
        ElementKind::ToggleButton(_)
        | ElementKind::CheckBox(_)
        | ElementKind::Switch(_)
        | ElementKind::RadioButton(_) => {
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
            ) || name == "Text" && matches!(kind, ElementKind::ComboBox(_))
        }
        _ => false,
    }
}

pub(super) fn common_property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    common: &mut CommonProperties,
) -> Result<bool, Diagnostic> {
    if badges::property(document, attr, common)? {
        return Ok(true);
    }
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
            common.horizontal_alignment_explicit = true;
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
            common.vertical_alignment_explicit = true;
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
        "Text" => selection.text = attr.value().into(),
        "CommitOnLostFocus" => selection.commit_on_lost_focus = Some(boolean(document, attr)?),
        "ItemPresentation" => {
            selection.item_presentation = match attr.value() {
                "Plain" => ListItemPresentationDefinition::Plain,
                "CheckBox" => ListItemPresentationDefinition::CheckBox,
                "RadioButton" => ListItemPresentationDefinition::RadioButton,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "ItemPresentation must be Plain, CheckBox or RadioButton",
                    ));
                }
            }
        }
        "SelectionPresentation" => {
            selection.selection_presentation = match attr.value() {
                "Text" => ComboSelectionPresentationDefinition::Text,
                "Tokens" => ComboSelectionPresentationDefinition::Tokens,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "SelectionPresentation must be Text or Tokens",
                    ));
                }
            }
        }
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

fn finite_number(document: &Document<'_>, attr: Attribute<'_, '_>) -> Result<f64, Diagnostic> {
    attr.value()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| at_attribute(document, attr, "number must be finite"))
}

pub(super) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    kind: &mut ElementKind,
) -> Result<bool, Diagnostic> {
    if matches!(
        kind,
        ElementKind::DateTimeEdit(_) | ElementKind::DateRangeEdit(_) | ElementKind::Calendar(_)
    ) {
        return temporal::property(document, attr, kind);
    }
    let time_editor = matches!(kind, ElementKind::TimeEdit(_));
    if let ElementKind::Expander(expander) = kind {
        match attr.name() {
            "ExpandDirection" => {
                expander.expand_direction = match attr.value() {
                    "Down" => ExpandDirectionDefinition::Down,
                    "Up" => ExpandDirectionDefinition::Up,
                    "Left" => ExpandDirectionDefinition::Left,
                    "Right" => ExpandDirectionDefinition::Right,
                    _ => {
                        return Err(at_attribute(
                            document,
                            attr,
                            "ExpandDirection must be Down/Up/Left/Right",
                        ));
                    }
                };
                return Ok(true);
            }
            "AnimationDurationMs" => {
                expander.animation_duration_ms = attr
                    .value()
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n <= 1000)
                    .ok_or_else(|| {
                        at_attribute(document, attr, "AnimationDurationMs must be 0..1000")
                    })?;
                return Ok(true);
            }
            _ => {}
        }
    }
    let metric = || editor_metric(document, attr, MAX_DIMENSION, true);
    match kind {
        ElementKind::PasswordBox(password) => {
            match attr.name() {
                "MaxLength" => {
                    password.max_length = attr.value().parse().map_err(|_| {
                        at_attribute(
                            document,
                            attr,
                            "PasswordBox MaxLength must be an integer in [0, 4096]",
                        )
                    })?
                }
                "IsReadOnly" => password.is_read_only = boolean(document, attr)?,
                "AllowPaste" => password.allow_paste = boolean(document, attr)?,
                "AllowReveal" => password.allow_reveal = boolean(document, attr)?,
                "PlaceholderText" => password.placeholder = attr.value().into(),
                "RevealDurationMs" => {
                    password.reveal_duration_ms = attr.value().parse().map_err(|_| {
                        at_attribute(
                            document,
                            attr,
                            "RevealDurationMs must be an integer in [1, 10000]",
                        )
                    })?
                }
                "PlaceholderForeground" => {
                    password.placeholder_foreground = Some(color(document, attr)?)
                }
                "SelectionBrush" => password.selection_brush = Some(color(document, attr)?),
                _ => return button_style_property(document, attr, &mut password.style),
            }
            Ok(true)
        }
        ElementKind::RadioGroup(selection) => {
            if attr.name() == "SelectedKey" {
                selection.selected_value = Some(attr.value().into());
                Ok(true)
            } else {
                selection_property(document, attr, selection)
            }
        }
        ElementKind::Rating(rating) => rating::property(document, attr, rating),
        ElementKind::NavigationPane(value) => navigation::property(document, attr, value),
        ElementKind::Breadcrumb(value) => breadcrumb::property(document, attr, value),
        ElementKind::ColorEdit(value) | ElementKind::BrushEdit(value) => {
            color::property(document, attr, value)
        }
        ElementKind::FontPicker(font) => font_picker::property(document, attr, font),
        ElementKind::TreeView(tree) => tree::property(document, attr, tree, false),
        ElementKind::TreeGrid(tree) => tree::property(document, attr, tree, true),
        ElementKind::GridControl(grid) => {
            match attr.name() {
                "DataSource" => grid.data_source = attr.value().into(),
                "IsReadOnly" => grid.is_read_only = boolean(document, attr)?,
                "FilterText" => grid.filter_text = attr.value().into(),
                "SelectedValue" => grid.selected_value = Some(attr.value().into()),
                "View" => grid.view = grid_extensions::view(document, attr)?,
                "GroupFields" => grid.group_fields = grid_extensions::fields(document, attr, 8)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::DataForm(form) => {
            match attr.name() {
                "DataSource" => form.data_source = attr.value().into(),
                "RowKey" => {
                    form.row_key = attr.value().parse::<i64>().map_err(|_| {
                        at_attribute(document, attr, "RowKey must be a signed 64-bit integer")
                    })?
                }
                "IsReadOnly" => form.is_read_only = boolean(document, attr)?,
                "LabelWidth" => form.label_width = editor_metric(document, attr, 4096.0, true)?,
                "RowGap" => form.row_gap = editor_metric(document, attr, 256.0, true)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::StatusBar(bar) | ElementKind::RibbonStatusBar(bar) => {
            match attr.name() {
                "IsCustomizable" => bar.is_customizable = boolean(document, attr)?,
                "ShowResizeGrip" => bar.show_resize_grip = boolean(document, attr)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::StatusBarItem(item) => {
            match attr.name() {
                "Key" => item.key = Some(attr.value().into()),
                "Label" => item.label = Some(attr.value().into()),
                "CanHide" => item.can_hide = boolean(document, attr)?,
                "Placement" => {
                    item.placement = match attr.value() {
                        "Left" => StatusBarPlacement::Left,
                        "Right" => StatusBarPlacement::Right,
                        _ => {
                            return Err(at_attribute(
                                document,
                                attr,
                                "Placement must be Left or Right",
                            ));
                        }
                    }
                }
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::ProgressBar(progress) | ElementKind::BusyIndicator(progress) => {
            match attr.name() {
                "IsIndeterminate" => progress.is_indeterminate = boolean(document, attr)?,
                "ReducedMotion" => progress.reduced_motion = boolean(document, attr)?,
                "Minimum" => progress.minimum = finite_number(document, attr)?,
                "Maximum" => progress.maximum = finite_number(document, attr)?,
                "Value" => progress.value = finite_number(document, attr)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::ZoomControl(zoom) => {
            match attr.name() {
                "Minimum" => zoom.minimum = finite_number(document, attr)?,
                "Maximum" => zoom.maximum = finite_number(document, attr)?,
                "Value" => zoom.value = finite_number(document, attr)?,
                "Step" => zoom.step = finite_number(document, attr)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::GroupBox(group) => {
            match attr.name() {
                "IsCollapsible" => group.is_collapsible = boolean(document, attr)?,
                "IsExpanded" => group.is_expanded = boolean(document, attr)?,
                _ => {
                    return group_property(
                        document,
                        attr,
                        GroupPropertiesMut {
                            header: &mut group.header,
                            appearance: &mut group.appearance,
                            padding: &mut group.padding,
                            header_style: &mut group.header_style,
                            header_icon: &mut group.header_icon,
                            show_header: &mut group.show_header,
                            corner_radius: &mut group.corner_radius,
                            header_foreground: &mut group.header_foreground,
                            header_background: &mut group.header_background,
                        },
                    );
                }
            }
            Ok(true)
        }
        ElementKind::Expander(expander) => {
            if attr.name() == "IsExpanded" {
                expander.is_expanded = boolean(document, attr)?;
                Ok(true)
            } else {
                group_property(
                    document,
                    attr,
                    GroupPropertiesMut {
                        header: &mut expander.header,
                        appearance: &mut expander.appearance,
                        padding: &mut expander.padding,
                        header_style: &mut expander.header_style,
                        header_icon: &mut expander.header_icon,
                        show_header: &mut expander.show_header,
                        corner_radius: &mut expander.corner_radius,
                        header_foreground: &mut expander.header_foreground,
                        header_background: &mut expander.header_background,
                    },
                )
            }
        }
        ElementKind::TabbedGroup(group) => {
            match attr.name() {
                "SelectedIndex" => {
                    let value = attr.value();
                    group.selected_index = value
                        .parse::<usize>()
                        .ok()
                        .filter(|_| !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()))
                        .ok_or_else(|| {
                            at_attribute(
                                document,
                                attr,
                                "SelectedIndex must be an unsigned integer",
                            )
                        })?;
                }
                "Padding" => group.padding = thickness(document, attr)?,
                _ => return Ok(false),
            }
            Ok(true)
        }
        ElementKind::ListBox(selection) | ElementKind::ComboBox(selection) => {
            selection_property(document, attr, selection)
        }
        ElementKind::LookupEdit(lookup) => {
            match attr.name() {
                "SearchColumns" => {
                    lookup.search_columns = if attr.value().trim().is_empty() {
                        Vec::new()
                    } else {
                        attr.value()
                            .split(',')
                            .map(|value| {
                                value.trim().parse::<usize>().map_err(|_| {
                                    at_attribute(
                                        document,
                                        attr,
                                        "SearchColumns requires comma-separated indices 0..31",
                                    )
                                })
                            })
                            .collect::<Result<_, _>>()?
                    };
                }
                "SearchMode" => {
                    lookup.search_mode = match attr.value() {
                        "Contains" => LookupSearchModeDefinition::Contains,
                        "StartsWith" => LookupSearchModeDefinition::StartsWith,
                        _ => {
                            return Err(at_attribute(
                                document,
                                attr,
                                "SearchMode must be Contains or StartsWith",
                            ));
                        }
                    }
                }
                "SearchComparison" => {
                    lookup.search_comparison = match attr.value() {
                        "Ordinal" => LookupSearchComparisonDefinition::Ordinal,
                        "UnicodeLowercase" => LookupSearchComparisonDefinition::UnicodeLowercase,
                        _ => {
                            return Err(at_attribute(
                                document,
                                attr,
                                "SearchComparison must be Ordinal or UnicodeLowercase",
                            ));
                        }
                    }
                }
                _ => return selection_property(document, attr, &mut lookup.selection),
            }
            Ok(true)
        }
        ElementKind::ButtonEdit(editor) => editor_property(document, attr, &mut editor.editor),
        ElementKind::SearchBox(search) => {
            match attr.name() {
                "Query" => search.editor.text = attr.value().into(),
                "MinQueryLength" | "MaxSuggestions" | "HistoryLimit" => {
                    let value = attr.value().parse::<usize>().map_err(|_| {
                        at_attribute(document, attr, "Expected a nonnegative integer")
                    })?;
                    match attr.name() {
                        "MinQueryLength" => search.min_query_length = value,
                        "MaxSuggestions" => search.max_suggestions = value,
                        _ => search.history_limit = value,
                    }
                }
                "PopupWidth" => search.popup_width = editor_metric(document, attr, 4096.0, false)?,
                "PopupHeight" => {
                    search.popup_height = editor_metric(document, attr, 4096.0, false)?
                }
                "ChoicePolicy" => {
                    search.choice_policy = match attr.value() {
                        "FillQuery" => SearchChoicePolicyDefinition::FillQuery,
                        "FillAndSearch" => SearchChoicePolicyDefinition::FillAndSearch,
                        _ => {
                            return Err(at_attribute(
                                document,
                                attr,
                                "ChoicePolicy must be FillQuery or FillAndSearch",
                            ));
                        }
                    }
                }
                "Text" | "Value" | "CommitOnLostFocus" | "IsRequired" | "ValidationMessage" => {
                    return Ok(false);
                }
                _ => return editor_property(document, attr, &mut search.editor),
            }
            Ok(true)
        }
        ElementKind::MemoEdit(memo) => {
            match attr.name() {
                "PopupWidth" => {
                    memo.popup_width = editor_metric(document, attr, MAX_DIMENSION, false)?
                }
                "PopupHeight" => {
                    memo.popup_height = editor_metric(document, attr, MAX_DIMENSION, false)?
                }
                _ => return editor_property(document, attr, &mut memo.editor),
            }
            Ok(true)
        }
        ElementKind::MaskEdit(editor) => {
            match attr.name() {
                "Mask" => editor.mask = attr.value().into(),
                "Value" => editor.value = attr.value().into(),
                "AllowEmpty" => editor.allow_empty = boolean(document, attr)?,
                "PromptChar" => {
                    let mut chars = attr.value().chars();
                    editor.prompt_char = chars
                        .next()
                        .filter(|_| chars.next().is_none())
                        .ok_or_else(|| {
                            at_attribute(document, attr, "PromptChar must be one character")
                        })?;
                }
                "Culture" => {
                    editor.culture = match attr.value() {
                        "ru" | "ru-RU" => NumericCultureDefinition::Russian,
                        "en" | "en-US" => NumericCultureDefinition::English,
                        "ar" | "ar-SA" => NumericCultureDefinition::Arabic,
                        _ => {
                            return Err(at_attribute(
                                document,
                                attr,
                                "Culture must be ru-RU, en-US or ar-SA",
                            ));
                        }
                    }
                }
                "Text" => return Ok(false),
                _ => return editor_property(document, attr, &mut editor.editor),
            }
            Ok(true)
        }
        ElementKind::NumericEdit(editor) => {
            match attr.name() {
                "Value" => editor.value = Some(attr.value().into()),
                "Minimum" => editor.minimum = attr.value().into(),
                "Maximum" => editor.maximum = attr.value().into(),
                "Increment" => editor.increment = attr.value().into(),
                "Precision" => {
                    editor.precision = attr.value().parse().map_err(|_| {
                        at_attribute(document, attr, "Precision must be an integer in [0, 28]")
                    })?
                }
                "AllowEmpty" => editor.allow_empty = boolean(document, attr)?,
                "UseGrouping" => editor.use_grouping = boolean(document, attr)?,
                "ShowCalculator" => editor.show_calculator = boolean(document, attr)?,
                "Culture" => {
                    editor.culture = match attr.value() {
                        "ru" | "ru-RU" => NumericCultureDefinition::Russian,
                        "en" | "en-US" => NumericCultureDefinition::English,
                        "ar" | "ar-SA" => NumericCultureDefinition::Arabic,
                        _ => {
                            return Err(at_attribute(
                                document,
                                attr,
                                "Culture must be ru-RU, en-US or ar-SA",
                            ));
                        }
                    }
                }
                "Text" => return Ok(false),
                _ => return editor_property(document, attr, &mut editor.editor),
            }
            Ok(true)
        }
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
                "AllowEmpty" => editor.allow_empty = boolean(document, attr)?,
                "TimeStep" if time_editor => {
                    editor.time_step = attr.value().parse().map_err(|_| {
                        at_attribute(document, attr, "TimeStep must be 1..86400 seconds")
                    })?
                }
                "ShowSeconds" if time_editor => {
                    editor.format = if boolean(document, attr)? {
                        "HH:mm:ss"
                    } else {
                        "HH:mm"
                    }
                    .into()
                }
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

struct GroupPropertiesMut<'a> {
    header: &'a mut String,
    appearance: &'a mut GroupBoxAppearanceDefinition,
    padding: &'a mut [f32; 4],
    header_style: &'a mut GroupHeaderStyleDefinition,
    header_icon: &'a mut Option<IconSymbol>,
    show_header: &'a mut bool,
    corner_radius: &'a mut Option<f32>,
    header_foreground: &'a mut Option<[u8; 4]>,
    header_background: &'a mut Option<[u8; 4]>,
}

fn group_property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    group: GroupPropertiesMut<'_>,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Header" => *group.header = attr.value().into(),
        "Padding" => *group.padding = thickness(document, attr)?,
        "HeaderIcon" => *group.header_icon = Some(icon_symbol(document, attr)?),
        "ShowHeader" => *group.show_header = boolean(document, attr)?,
        "CornerRadius" => {
            *group.corner_radius = Some(editor_metric(document, attr, MAX_DIMENSION, true)?)
        }
        "HeaderForeground" => *group.header_foreground = Some(color(document, attr)?),
        "HeaderBackground" => *group.header_background = Some(color(document, attr)?),
        "HeaderStyle" => {
            *group.header_style = match attr.value() {
                "Normal" => GroupHeaderStyleDefinition::Normal,
                "Light" => GroupHeaderStyleDefinition::Light,
                "Accent" => GroupHeaderStyleDefinition::Accent,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "HeaderStyle must be Normal, Light or Accent",
                    ));
                }
            };
        }
        "Appearance" => {
            *group.appearance = match attr.value() {
                "Card" => GroupBoxAppearanceDefinition::Card,
                "Outline" => GroupBoxAppearanceDefinition::Outline,
                "Plain" => GroupBoxAppearanceDefinition::Plain,
                "Standard" => GroupBoxAppearanceDefinition::Standard,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "Appearance must be Card, Outline, Plain or Standard",
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

fn editor_button(
    node: Node<'_, '_>,
    depth: usize,
    parent: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    groups: &mut HashSet<RadioGroupKey>,
) -> Result<EditorButtonDefinition, Diagnostic> {
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
            "Placement" => {
                result.placement = match attr.value() {
                    "Leading" => EditorButtonPlacement::Leading,
                    "Trailing" => EditorButtonPlacement::Trailing,
                    _ => {
                        return Err(at_attribute(
                            node.document(),
                            attr,
                            "EditorButton Placement must be Leading or Trailing",
                        ));
                    }
                }
            }
            "Width" => result.width = editor_metric(node.document(), attr, 1024.0, false)?,
            "IsVisible" => result.is_visible = boolean(node.document(), attr)?,
            "IsEnabled" => result.is_enabled = boolean(node.document(), attr)?,
            "IsEnabledWhenReadOnly" => {
                result.is_enabled_when_read_only = boolean(node.document(), attr)?
            }
            "IsTabStop" => result.is_tab_stop = boolean(node.document(), attr)?,
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
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        require_element(child, "EditorButton.ContentTemplate")?;
        if result.content_template.is_some() || child.attributes().len() != 0 {
            return Err(at_node(
                child,
                "Invalid or duplicate EditorButton.ContentTemplate",
            ));
        }
        for visual in child.children() {
            if !visual.is_element() {
                require_empty_content(visual)?;
                continue;
            }
            if result.content_template.is_some() {
                return Err(at_node(
                    visual,
                    "EditorButton.ContentTemplate requires one visual root",
                ));
            }
            let mut definition = parse_element(visual, depth + 2, parent, count, names, groups)?;
            if visual.attribute("VerticalAlignment").is_none() {
                definition.common.vertical_alignment = Alignment::Center;
            }
            result.content_template = Some(Box::new(definition));
        }
        if result.content_template.is_none() {
            return Err(at_node(
                child,
                "EditorButton.ContentTemplate requires one visual root",
            ));
        }
    }
    validate_editor_button(&result).map_err(|message| at_node(node, message))?;
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
    let is_brush = matches!(kind, ElementKind::BrushEdit(_));
    if let ElementKind::ColorEdit(value) | ElementKind::BrushEdit(value) = kind {
        return color::children(node, value, is_brush, depth, count);
    }
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
        if matches!(
            name,
            "ListBox.ItemTemplate" | "ComboBox.ItemTemplate" | "LookupEdit.ItemTemplate"
        ) {
            let selection = match (&mut *kind, name) {
                (ElementKind::ListBox(s), "ListBox.ItemTemplate")
                | (ElementKind::ComboBox(s), "ComboBox.ItemTemplate") => s,
                (ElementKind::LookupEdit(s), "LookupEdit.ItemTemplate") => &mut s.selection,
                _ => return Err(at_node(child, "ItemTemplate does not belong to this owner")),
            };
            if selection.item_template.is_some() || child.attributes().len() != 0 {
                return Err(at_node(child, "Invalid or duplicate ItemTemplate"));
            }
            for visual in child.children() {
                if !visual.is_element() {
                    require_empty_content(visual)?;
                    continue;
                }
                if selection.item_template.is_some() {
                    return Err(at_node(child, "ItemTemplate requires one visual root"));
                }
                let definition = parse_element(
                    visual,
                    depth + 2,
                    parent,
                    count,
                    &mut HashSet::new(),
                    &mut HashSet::new(),
                )?;
                crate::validate_item_template(&definition)
                    .map_err(|message| at_node(visual, message))?;
                selection.item_template = Some(Box::new(definition));
            }
            if selection.item_template.is_none() {
                return Err(at_node(child, "ItemTemplate requires one visual root"));
            }
            continue;
        }
        if matches!(name, "GroupBox.HeaderTemplate" | "Expander.HeaderTemplate") {
            let belongs = matches!(
                (&*kind, name),
                (ElementKind::GroupBox(_), "GroupBox.HeaderTemplate")
                    | (ElementKind::Expander(_), "Expander.HeaderTemplate")
            );
            if !belongs || !properties.insert(name) || child.attributes().len() != 0 {
                return Err(at_node(child, "Invalid or duplicate header template"));
            }
            let mut header = None;
            for entry in child.children() {
                if entry.is_element() {
                    if header.is_some() {
                        return Err(at_node(entry, "HeaderTemplate requires one child"));
                    }
                    *count += 1;
                    let mut content =
                        parse_element(entry, depth + 2, parent, count, names, groups)?;
                    if entry.attribute("VerticalAlignment").is_none() {
                        content.common.vertical_alignment = Alignment::Center;
                    }
                    header = Some(Box::new(content));
                } else {
                    require_empty_content(entry)?;
                }
            }
            if header.is_none() {
                return Err(at_node(child, "HeaderTemplate requires one child"));
            }
            match kind {
                ElementKind::GroupBox(g) => g.header_content = header,
                ElementKind::Expander(g) => g.header_content = header,
                _ => unreachable!(),
            }
            continue;
        }
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
                    (ElementKind::ButtonEdit(editor), "ButtonEdit.Buttons") => editor.buttons.push(
                        editor_button(entry, depth + 1, parent, count, names, groups)?,
                    ),
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
                ElementKind::StatusBar(bar) | ElementKind::RibbonStatusBar(bar) => {
                    require_element(child, "StatusBarItem")?;
                    bar.items.push(parse_element(
                        child,
                        depth + 1,
                        parent,
                        count,
                        names,
                        groups,
                    )?);
                }
                ElementKind::TabbedGroup(group) => {
                    require_element(child, "GroupTab")?;
                    *count += 1;
                    if group.tabs.len() >= 64
                        || *count > MAX_ELEMENTS
                        || depth + 1 > MAX_ELEMENT_DEPTH
                    {
                        return Err(at_node(
                            child,
                            "TabbedGroup exceeds page count or content depth limit",
                        ));
                    }
                    let tab_parent = *count;
                    let mut tab = GroupTabDefinition::default();
                    for attr in child.attributes() {
                        if attr.namespace().is_some() {
                            return Err(at_attribute(
                                child.document(),
                                attr,
                                "GroupTab does not support directives",
                            ));
                        }
                        reject_extension(child.document(), attr)?;
                        match attr.name() {
                            "Header" => tab.header = attr.value().into(),
                            "HeaderIcon" => {
                                tab.header_icon = Some(icon_symbol(child.document(), attr)?)
                            }
                            "IsEnabled" => tab.is_enabled = boolean(child.document(), attr)?,
                            _ => {
                                return Err(at_attribute(
                                    child.document(),
                                    attr,
                                    "unsupported GroupTab property",
                                ));
                            }
                        }
                    }
                    for content in child.children() {
                        if !content.is_element() {
                            require_empty_content(content)?;
                            continue;
                        }
                        if content.tag_name().namespace() == Some(UI_NAMESPACE)
                            && content.tag_name().name() == "GroupTab.HeaderTemplate"
                        {
                            if tab.header_content.is_some() || content.attributes().len() != 0 {
                                return Err(at_node(
                                    content,
                                    "Invalid or duplicate GroupTab.HeaderTemplate",
                                ));
                            }
                            for visual in content.children() {
                                if visual.is_element() {
                                    if tab.header_content.is_some() {
                                        return Err(at_node(
                                            visual,
                                            "GroupTab.HeaderTemplate requires exactly one child",
                                        ));
                                    }
                                    let mut header = parse_element(
                                        visual,
                                        depth + 2,
                                        tab_parent,
                                        count,
                                        names,
                                        groups,
                                    )?;
                                    if visual.attribute("VerticalAlignment").is_none() {
                                        header.common.vertical_alignment = Alignment::Center;
                                    }
                                    tab.header_content = Some(Box::new(header));
                                } else {
                                    require_empty_content(visual)?;
                                }
                            }
                            if tab.header_content.is_none() {
                                return Err(at_node(
                                    content,
                                    "GroupTab.HeaderTemplate requires exactly one child",
                                ));
                            }
                            continue;
                        }
                        if tab.child.is_some() {
                            return Err(at_node(content, "GroupTab accepts at most one child"));
                        }
                        tab.child = Some(Box::new(parse_element(
                            content,
                            depth + 2,
                            tab_parent,
                            count,
                            names,
                            groups,
                        )?));
                    }
                    group.tabs.push(tab);
                }
                ElementKind::ListBox(selection)
                | ElementKind::ComboBox(selection)
                | ElementKind::RadioGroup(selection) => {
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
                | ElementKind::StatusBarItem(_)
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
        ElementKind::StatusBarItem(item) => item.child = single.map(Box::new),
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
        crate::validate_selection_presentation(kind, common)
            .map_err(|message| at_node(node, message))?;
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
        ElementKind::Rating(rating) => {
            validate_rating(rating, common).map_err(|message| at_node(node, message))?
        }
        ElementKind::NavigationPane(value) => {
            validate_navigation(value, common).map_err(|message| at_node(node, message))?
        }
        ElementKind::Breadcrumb(value) => {
            validate_breadcrumb(value, common).map_err(|message| at_node(node, message))?
        }
        ElementKind::ColorEdit(value) | ElementKind::BrushEdit(value) => {
            validate_color_editor(value, matches!(kind, ElementKind::BrushEdit(_)), common)
                .map_err(|message| at_node(node, message))?;
        }
        ElementKind::FontPicker(font) => {
            validate_font_picker(font, common).map_err(|message| at_node(node, message))?
        }
        ElementKind::DataForm(form) => {
            validate_data_form(form).map_err(|message| at_node(node, message))?;
        }
        ElementKind::TreeView(tree) | ElementKind::TreeGrid(tree) => {
            validate_tree(tree, matches!(kind, ElementKind::TreeGrid(_)), common)
                .map_err(|message| at_node(node, message))?;
        }
        ElementKind::GridControl(grid) => {
            validate_grid_control(grid).map_err(|message| at_node(node, message))?;
            validate_grid_control_bindings(common).map_err(|message| at_node(node, message))?;
        }
        ElementKind::ProgressBar(progress) | ElementKind::BusyIndicator(progress) => {
            validate_progress_bar(progress).map_err(|message| at_node(node, message))?;
            if common.bindings.get("Value").is_some_and(|binding| {
                !matches!(binding.mode, BindingMode::OneWay | BindingMode::OneTime)
            }) {
                return Err(at_node(
                    node,
                    "ProgressBar Value supports only OneWay or OneTime binding",
                ));
            }
        }
        ElementKind::ZoomControl(zoom) => {
            validate_zoom_control(zoom).map_err(|message| at_node(node, message))?
        }
        ElementKind::GroupBox(group) => {
            validate_group_box(group).map_err(|message| at_node(node, message))?;
            if !group.is_collapsible && common.bindings.contains_key("IsExpanded") {
                return Err(at_node(
                    node,
                    "GroupBox IsExpanded binding requires IsCollapsible=True",
                ));
            }
        }
        ElementKind::Expander(expander) => {
            validate_expander(expander).map_err(|message| at_node(node, message))?
        }
        ElementKind::TabbedGroup(group) => {
            validate_tabbed_group(group).map_err(|message| at_node(node, message))?
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
        ElementKind::MemoEdit(memo) => {
            validate_memo_edit(memo).map_err(|message| at_node(node, message))?
        }
        ElementKind::SearchBox(search) => {
            validate_search_box(search, common).map_err(|message| at_node(node, message))?
        }
        ElementKind::PasswordBox(password) => {
            validate_password_box(password).map_err(|message| at_node(node, message))?;
            validate_password_bindings(common).map_err(|message| at_node(node, message))?;
        }
        ElementKind::NumericEdit(numeric) => {
            validate_numeric_edit(numeric, common.bindings.contains_key("Value"))
                .map_err(|message| at_node(node, message))?;
        }
        ElementKind::MaskEdit(mask) => {
            validate_mask_edit(mask, common.bindings.contains_key("Value"))
                .map_err(|message| at_node(node, message))?;
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
            if !editor.allow_empty
                && editor.value.is_none()
                && !common.bindings.contains_key("Value")
            {
                return Err(at_node(
                    node,
                    "A temporal editor requires Value when AllowEmpty=False",
                ));
            }
            if !date && !(1..=86400).contains(&editor.time_step) {
                return Err(at_node(node, "TimeStep must be 1..86400 seconds"));
            }
            if !date
                && let (Some(format), Some(show)) =
                    (node.attribute("Format"), node.attribute_node("ShowSeconds"))
                && (format == "HH:mm:ss") != boolean(node.document(), show)?
            {
                return Err(at_node(node, "TimeEdit Format conflicts with ShowSeconds"));
            }
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
        ElementKind::DateTimeEdit(definition) => {
            validate_date_time_edit(definition, common.bindings.contains_key("Value"))
                .map_err(|message| at_node(node, message))?
        }
        ElementKind::DateRangeEdit(definition) => {
            validate_date_range_edit(definition, common.bindings.contains_key("Value"))
                .map_err(|message| at_node(node, message))?
        }
        ElementKind::Calendar(definition) => {
            validate_calendar(definition).map_err(|message| at_node(node, message))?
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
