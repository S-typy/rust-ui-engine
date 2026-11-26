//! Retained property inspection over one transactional source and explicit keys.
use super::data_form::{checked_format, create_editor, parse_value};
use super::property_grid_model::{aggregate, invalid};
use super::*;
use rust_desktop_ui_core::{Align, GridTrack, Modifiers};
use rust_desktop_ui_data::{CellValue, DataWrite, FieldSchema, TableSchema};

#[path = "property_grid/values.rs"]
mod values;
use values::*;
#[path = "property_grid/presentation.rs"]
mod presentation;
pub(super) use presentation::InspectorPart;
use presentation::{ValueMark, action_icon, show, vertical_stack};

#[path = "property_grid/inline_editors.rs"]
mod inline_editors;
#[path = "property_grid/popup_editors.rs"]
mod popup_editors;
#[path = "property_grid/registry.rs"]
mod registry;
#[path = "property_grid/structured_editors.rs"]
mod structured_editors;
#[path = "property_grid/text_editors.rs"]
mod text_editors;
pub use registry::{PropertyCellEditorFactory, PropertyCellEditors};

/// Each editor prepares a typed draft. Only PropertyGrid owns source writes.
pub trait PropertyCellEditor {
    /// Root of the mounted editor's widgets within the property value host.
    fn owner(&self) -> WidgetId;
    /// Widget that receives keyboard focus when editing begins.
    fn focus(&self) -> WidgetId;
    /// Restore a typed value on initialization, refresh, programmatic draft or cancel.
    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError>;
    /// Read an editable draft without writing to the inspected data source.
    fn read(&self, ui: &Controls) -> Result<CellValue, String>;
    /// Process this editor's interactions. Leave other controls' events intact.
    fn update(&mut self, _ui: &mut Controls) -> Result<(), ControlError> {
        Ok(())
    }
    /// Keep the real editor visible in a uniform row before entering edit mode.
    fn inline(&self) -> bool {
        false
    }
    /// Update edit-only child visibility and close owned popups at session end.
    fn set_active(&self, _ui: &mut Controls, _active: bool) -> Result<(), ControlError> {
        Ok(())
    }
    /// Optional caption for an accepted value in the inactive row.
    fn rest_label(&self, _value: &CellValue) -> Option<String> {
        None
    }
    /// Retrieve a navigation request for the application to handle.
    fn take_navigation(&mut self) -> Option<String> {
        None
    }
}
use PropertyCellEditor as InspectorEditor;

enum EditorFamily {
    Inline,
    Popup,
    Text,
    Structured,
    Custom(std::rc::Rc<dyn PropertyCellEditorFactory>),
}
impl EditorFamily {
    fn resolve(name: &str, registry: &PropertyCellEditors) -> Option<Self> {
        if inline_editors::supports(name) {
            Some(Self::Inline)
        } else if popup_editors::supports(name) {
            Some(Self::Popup)
        } else if text_editors::supports(name) {
            Some(Self::Text)
        } else if structured_editors::supports(name) {
            Some(Self::Structured)
        } else {
            registry.get(name).map(Self::Custom)
        }
    }
    fn validate(
        &self,
        d: &PropertyDescriptor,
        f: &FieldSchema,
        v: &CellValue,
    ) -> Result<(), ControlError> {
        match self {
            Self::Inline => inline_editors::validate(d, f, v),
            Self::Popup => popup_editors::validate(d, f, v),
            Self::Text => text_editors::validate(d, f, v),
            Self::Structured => structured_editors::validate(d, f, v),
            Self::Custom(factory) => factory.validate(d, f, v),
        }
    }
    fn mount(
        &self,
        ui: &mut Controls,
        host: WidgetId,
        d: &PropertyDescriptor,
        f: &FieldSchema,
        v: &CellValue,
    ) -> Result<Box<dyn InspectorEditor>, ControlError> {
        match self {
            Self::Inline => inline_editors::mount(ui, host, d, f, v),
            Self::Popup => popup_editors::mount(ui, host, d, f, v),
            Self::Text => text_editors::mount(ui, host, d, f, v),
            Self::Structured => structured_editors::mount(ui, host, d, f, v),
            Self::Custom(factory) => {
                let editor = factory.mount(ui, host, d, f, v)?;
                if editor.owner() == host
                    || !ui.flyout_descendant(editor.owner(), host)
                    || !ui.flyout_descendant(editor.focus(), editor.owner())
                {
                    return Err(invalid(
                        "A custom property editor must own a child of the value host and focus a widget in its own subtree",
                    ));
                }
                Ok(editor)
            }
        }
    }
}

struct Property {
    descriptor: PropertyDescriptor,
    schema: FieldSchema,
    recipe: DataFormEditorFactory,
    specialized: Option<Box<dyn InspectorEditor>>,
    family: Option<EditorFamily>,
    row: WidgetId,
    host: WidgetId,
    name_label: WidgetId,
    owner: WidgetId,
    editor: WidgetId,
    active: bool,
    value_label: Option<WidgetId>,
    measure_value_width: bool,
    edit: WidgetId,
    currency: Option<WidgetId>,
    apply: WidgetId,
    reset: WidgetId,
    null: Option<WidgetId>,
    baseline: Vec<CellValue>,
    seed: CellValue,
    editing: bool,
}

/// Owns inspector state beside Controls. Call update after input and before
/// draining generic activations. It consumes only its own action buttons.
/// Accepted values are source/business values; editor preparation never changes
/// them. A failed source setter preserves accepted values and the editable draft.
pub struct PropertyGrid {
    pub container: WidgetId,
    pub summary: WidgetId,
    source: DataSourceHandle,
    schema: TableSchema,
    objects: Vec<i64>,
    properties: Vec<Property>,
    groups: Vec<(Vec<String>, Expander)>,
    parents: Vec<(WidgetId, WidgetId)>,
    search: WidgetId,
    categorized: WidgetId,
    sorted: WidgetId,
    flat_host: WidgetId,
    viewport: Option<ScrollViewer>,
    empty: WidgetId,
    filter: String,
    flat: bool,
    saved_expansion: Vec<(WidgetId, bool)>,
    grouped_order: Vec<(WidgetId, WidgetId)>,
    details: Option<(WidgetId, WidgetId)>,
    selection: Option<usize>,
    last_focus: Option<WidgetId>,
    error: Option<String>,
    changes: Vec<PropertyChanged>,
}
impl PropertyGrid {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        source: DataSourceHandle,
        objects: &[i64],
        settings: PropertyGridSettings,
        factories: &DataFormFactories,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        Self::mount_with_editors(
            ui,
            parent,
            source,
            objects,
            settings,
            factories,
            &PropertyCellEditors::default(),
            style,
        )
    }

    /// Mount an inspector with application-defined property cell editors.
    /// Registered factories validate each inspected value and default before
    /// the inspector creates widgets. Each row owns its mounted editor.
    #[allow(clippy::too_many_arguments)]
    pub fn mount_with_editors(
        ui: &mut Controls,
        parent: WidgetId,
        source: DataSourceHandle,
        objects: &[i64],
        settings: PropertyGridSettings,
        factories: &DataFormFactories,
        cell_editors: &PropertyCellEditors,
        mut style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        settings.validate()?;
        if objects.is_empty()
            || objects.len() > 64
            || objects.iter().collect::<HashSet<_>>().len() != objects.len()
        {
            return Err(invalid("PropertyGrid requires 1..64 unique object keys"));
        }
        let data = source
            .try_borrow()
            .map_err(|_| invalid("Property source is borrowed"))?;
        let schema = data.table().schema().clone();
        let metadata = if settings.properties.is_empty() {
            schema
                .fields()
                .iter()
                .map(|f| PropertyDescriptor::new(&f.name))
                .collect()
        } else {
            settings.properties.clone()
        };
        if metadata.is_empty() || metadata.len() > 128 {
            return Err(invalid("PropertyGrid requires 1..128 properties"));
        }
        let mut definitions = Vec::new();
        let mut metadata_bytes = 0usize;
        for mut descriptor in metadata {
            let field = schema
                .field(&descriptor.metadata.field)
                .ok_or_else(|| invalid("Unknown property field"))?
                .clone();
            use rust_desktop_ui_data::FieldType;
            let compatible = match descriptor.preview {
                PropertyPreview::Auto => true,
                PropertyPreview::Switch => field.kind == FieldType::Bool,
                PropertyPreview::Color => field.kind == FieldType::Text,
                PropertyPreview::Progress => field.kind == FieldType::Integer,
            };
            if !compatible
                || descriptor.preview != PropertyPreview::Auto && !descriptor.choices.is_empty()
            {
                return Err(invalid(format!(
                    "Incompatible preview for property '{}'",
                    field.name
                )));
            }
            let name = descriptor.metadata.editor.as_str();
            let family = EditorFamily::resolve(name, cell_editors);
            let custom = matches!(family, Some(EditorFamily::Custom(_)));
            if family.is_some() && descriptor.preview != PropertyPreview::Auto
                || !custom
                    && !descriptor.choice_keys.is_empty()
                    && !matches!(
                        name,
                        "AutoComplete"
                            | "Lookup"
                            | "SearchLookup"
                            | "ListBox"
                            | "Flags"
                            | "TokenList"
                    )
                || !custom
                    && descriptor.mask.is_some()
                    && !matches!(name, "MaskedText" | "NumericText")
                || !custom
                    && (descriptor.minimum.is_some()
                        || descriptor.maximum.is_some()
                        || descriptor.step.is_some())
                    && !matches!(
                        name,
                        "Slider" | "ZoomSlider" | "RangeSlider" | "ProgressBar" | "Rating" | "Spin"
                    )
                || !custom
                    && !descriptor.choices.is_empty()
                    && family.is_some()
                    && !matches!(
                        name,
                        "AutoComplete"
                            | "Lookup"
                            | "SearchLookup"
                            | "ListBox"
                            | "Image"
                            | "Flags"
                            | "AutoSuggest"
                            | "TokenList"
                    )
            {
                return Err(invalid(format!(
                    "Incompatible property editor settings for '{}'",
                    field.name
                )));
            }
            descriptor.metadata.read_only |=
                settings.read_only || field.read_only || name == "ProgressBar";
            let base_name = match (&family, name) {
                (Some(_), _) => "Auto",
                (_, "Spin") => "Numeric",
                _ => name,
            };
            let mut recipe = factories.resolve(base_name, &field)?;
            if name == "Spin"
                && let DataFormEditorFactory::Numeric(s) = &mut recipe
            {
                if let Some(v) = descriptor.minimum {
                    s.minimum = DecimalValue::new(i128::from(v), 0).unwrap();
                }
                if let Some(v) = descriptor.maximum {
                    s.maximum = DecimalValue::new(i128::from(v), 0).unwrap();
                }
                if let Some(v) = descriptor.step {
                    s.step = DecimalValue::new(i128::from(v), 0).unwrap();
                }
                s.trim_trailing_zeros = true;
            }
            if let DataFormEditorFactory::Text(s) = &recipe {
                descriptor.metadata.read_only |= s.is_read_only;
            }
            let baseline = objects
                .iter()
                .map(|key| {
                    data.value(*key, &field.name)
                        .cloned()
                        .ok_or_else(|| invalid("Unknown inspected object"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for value in baseline.iter().chain(descriptor.default.as_ref()) {
                field.validate(value).map_err(|e| invalid(e.to_string()))?;
                if let Some(family) = &family {
                    family.validate(&descriptor, &field, value)?;
                } else {
                    checked_format(&recipe, &field, value)?;
                }
            }
            let seed = if name == "Password"
                && descriptor.default.is_none()
                && matches!(aggregate(&baseline), PropertyValue::Mixed)
            {
                CellValue::Text(String::new())
            } else if family.is_some() {
                descriptor
                    .default
                    .as_ref()
                    .or_else(|| baseline.iter().find(|v| **v != CellValue::Null))
                    .unwrap_or(&CellValue::Null)
                    .clone()
            } else if descriptor.choices.is_empty() {
                neutral_value(&recipe, &field, descriptor.default.as_ref())?
            } else {
                if field.kind != rust_desktop_ui_data::FieldType::Text
                    || !matches!(recipe, DataFormEditorFactory::Text(_))
                    || !matches!(descriptor.metadata.editor.as_str(), "Auto" | "Text")
                {
                    return Err(invalid(
                        "Choices requires an Auto or Text editor on a text field",
                    ));
                }
                for value in baseline.iter().chain(descriptor.default.as_ref()) {
                    validate_choice(&descriptor.choices, value)?;
                }
                descriptor.default.clone().unwrap_or_else(|| {
                    if field.nullable {
                        CellValue::Null
                    } else {
                        CellValue::Text(descriptor.choices[0].clone())
                    }
                })
            };
            metadata_bytes += descriptor.category.len()
                + descriptor.groups.iter().map(String::len).sum::<usize>()
                + field.name.len()
                + descriptor
                    .metadata
                    .label
                    .as_deref()
                    .unwrap_or(&field.title)
                    .len()
                + descriptor.metadata.editor.len();
            if metadata_bytes > 32768 {
                return Err(invalid("Property metadata exceeds 32 KiB"));
            }
            for label in
                std::iter::once(descriptor.metadata.label.as_deref().unwrap_or(&field.title))
                    .chain(std::iter::once(descriptor.category.as_str()))
                    .chain(descriptor.groups.iter().map(String::as_str))
            {
                ui.measure_text(label, 4096.0, false)?;
            }
            definitions.push((descriptor, field, recipe, family, baseline, seed));
        }
        drop(data);
        let descriptions = settings
            .properties
            .iter()
            .any(|p| !p.description.is_empty());
        if settings.scrollable {
            style.kind = LayoutKind::Grid;
            style.grid_columns = vec![GridTrack::Fr(1.)];
            style.grid_rows = vec![GridTrack::Auto, GridTrack::Fr(1.), GridTrack::Auto];
            if descriptions {
                style.grid_rows.push(GridTrack::Auto);
            }
        } else {
            style.kind = LayoutKind::Stack(Axis::Vertical);
        }
        style.gap = 0.0;
        let container = ui.transparent_panel(parent, style)?;
        ui.set_role(container, Role::Group);
        ui.set_automation_name(container, "Свойства объектов")?;
        let result = (|| {
            let (search, categorized, sorted) = presentation::toolbar(ui, container)?;
            let viewport = if settings.scrollable {
                let scroll = ui.scroll_view(container, LayoutStyle::default(), 0.)?;
                ui.set_surface(scroll.viewport, SurfaceKind::Transparent)?;
                ui.controls
                    .get_mut(&scroll.viewport)
                    .unwrap()
                    .inspector_part = Some(InspectorPart::Viewport);
                ui.set_surface(scroll.content, SurfaceKind::Transparent)?;
                ui.edit_props(scroll.content, |n| {
                    n.clip = false;
                    n.style = vertical_stack();
                    n.style.padding = Edges {
                        left: 1.,
                        top: 1.,
                        right: 10.,
                        bottom: 1.,
                    };
                })?;
                Some(scroll)
            } else {
                None
            };
            let body = viewport.as_ref().map_or(container, |scroll| scroll.content);
            let flat_host = ui.transparent_panel(body, vertical_stack())?;
            show(ui, flat_host, false)?;
            let mut groups: Vec<(Vec<String>, Expander)> = Vec::new();
            let mut properties = Vec::new();
            for (descriptor, schema, recipe, family, baseline, seed) in definitions {
                let path: Vec<_> = std::iter::once(descriptor.category.clone())
                    .chain(descriptor.groups.clone())
                    .collect();
                let mut host = body;
                for depth in 1..=path.len() {
                    let prefix = path[..depth].to_vec();
                    if let Some((_, group)) = groups.iter().find(|(key, _)| *key == prefix) {
                        host = group.content;
                    } else {
                        if groups.len() >= 256 {
                            return Err(invalid(
                                "PropertyGrid supports at most 256 expandable groups",
                            ));
                        }
                        let group = ui.expander(
                            host,
                            GroupSettings {
                                header: path[depth - 1].clone(),
                                appearance: GroupAppearance::Plain,
                                header_style: GroupHeaderStyle::Light,
                                padding: Edges::default(),
                                ..Default::default()
                            },
                            true,
                            LayoutStyle {
                                width: Length::Percent(1.0),
                                ..Default::default()
                            },
                        )?;
                        ui.controls.get_mut(&group.header).unwrap().inspector_part =
                            Some(InspectorPart::Category {
                                owner: group.container,
                                depth: depth - 1,
                            });
                        ui.edit_props(group.header, |p| p.style.height = Length::Px(28.0))?;
                        ui.edit_props(group.content, |p| {
                            p.style.kind = LayoutKind::Stack(Axis::Vertical);
                            p.style.gap = 0.0;
                        })?;
                        host = group.content;
                        groups.push((prefix, group));
                    }
                }
                let column = if descriptor.full_width {
                    GridTrack::Px(0.)
                } else {
                    settings
                        .label_fraction
                        .map_or(GridTrack::Px(settings.label_width), GridTrack::Percent)
                };
                let row = ui.transparent_panel(
                    host,
                    LayoutStyle {
                        kind: LayoutKind::Grid,
                        grid_columns: vec![
                            column,
                            GridTrack::Fr(1.0),
                            GridTrack::Auto,
                            GridTrack::Auto,
                        ],
                        min_size: Size::new(0., 26.),
                        padding: Edges {
                            bottom: 1.,
                            ..Default::default()
                        },
                        width: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?;
                let label = descriptor
                    .metadata
                    .label
                    .as_deref()
                    .unwrap_or(&schema.title);
                let name_label = ui.label(
                    row,
                    label,
                    LayoutStyle {
                        height: Length::Px(20.),
                        margin: Edges {
                            left: 16.,
                            ..Default::default()
                        },
                        vertical_alignment: Some(Align::Center),
                        ..Default::default()
                    },
                )?;
                ui.controls.get_mut(&name_label).unwrap().inspector_part =
                    Some(InspectorPart::Name);
                ui.set_text_auto_height(name_label, true)?;
                if descriptor.full_width {
                    show(ui, name_label, false)?;
                }
                let host = ui.transparent_panel(
                    row,
                    LayoutStyle {
                        // Stack deliberately ignores child flex_grow. The
                        // editor and mixed-value caption share this remaining
                        // Grid column, while the NULL toggle keeps its width.
                        grid_column: rust_desktop_ui_core::GridPlacement {
                            start: Some(2),
                            span: 1,
                        },
                        kind: LayoutKind::Flex(Axis::Horizontal),
                        margin: Edges {
                            left: 1.,
                            ..Default::default()
                        },
                        gap: 2.0,
                        ..Default::default()
                    },
                )?;
                ui.controls.get_mut(&row).unwrap().inspector_part = Some(InspectorPart::Row {
                    value_host: (!descriptor.full_width).then_some(host),
                    alternate: properties.len() % 2 != 0,
                });
                let state = aggregate(&baseline);
                let value = match &state {
                    PropertyValue::Uniform(v) => v,
                    PropertyValue::Mixed => &seed,
                };
                let specialized = family
                    .as_ref()
                    .map(|family| family.mount(ui, host, &descriptor, &schema, value))
                    .transpose()?;
                let (owner, editor) = if let Some(control) = &specialized {
                    (control.owner(), control.focus())
                } else if descriptor.choices.is_empty() {
                    create_editor(ui, host, label, &schema, &recipe, value)?
                } else {
                    let choices = descriptor
                        .choices
                        .iter()
                        .map(|v| SelectionItem::new(v, v))
                        .collect();
                    let parts = ui.combo_box(
                        host,
                        label,
                        choices,
                        ComboBoxSettings {
                            editable: false,
                            read_only: descriptor.metadata.read_only,
                        },
                        LayoutStyle {
                            height: Length::Px(32.),
                            ..Default::default()
                        },
                    )?;
                    if let CellValue::Text(v) = value {
                        ui.set_selected_keys(parts.container, std::slice::from_ref(v))?;
                    }
                    (parts.container, parts.editor)
                };
                if descriptor.preview == PropertyPreview::Switch {
                    ui.set_check_presentation(owner, CheckPresentation::Switch)?;
                }
                ui.edit_props(owner, |p| {
                    p.style.flex_grow = 1.0;
                    if owner == editor && specialized.is_none() {
                        p.style.height = Length::Px(28.);
                        p.style.vertical_alignment = Some(Align::Center);
                    }
                })?;
                if specialized.is_none()
                    || ui
                        .controls
                        .get(&editor)
                        .is_some_and(|part| part.automation_name.is_none())
                {
                    ui.set_automation_name(editor, label)?;
                }
                let null = if schema.nullable && !matches!(recipe, DataFormEditorFactory::CheckBox)
                {
                    // Keep enough room for the Fluent checkbox indicator,
                    // chrome insets and the full caption at the largest theme font.
                    let id = ui.checkbox(
                        host,
                        "NULL",
                        *value == CellValue::Null,
                        LayoutStyle {
                            min_size: Size::new(84.0, 0.0),
                            flex_shrink: 0.0,
                            height: Length::Px(28.),
                            ..button_style(84.0)
                        },
                    )?;
                    ui.set_automation_name(id, format!("{label}: значение отсутствует"))?;
                    Some(id)
                } else {
                    None
                };
                let currency = if let Some(symbol) = &descriptor.currency_symbol {
                    let width = ui.measure_text(symbol, 128., false)?.width.ceil() + 8.;
                    Some(ui.label(
                        host,
                        symbol,
                        LayoutStyle {
                            width: Length::Px(width),
                            flex_shrink: 0.,
                            vertical_alignment: Some(Align::Center),
                            ..Default::default()
                        },
                    )?)
                } else {
                    None
                };
                let edit = ui.button(
                    host,
                    "",
                    None,
                    LayoutStyle {
                        height: Length::Px(28.),
                        vertical_alignment: Some(Align::Center),
                        flex_grow: 1.,
                        ..Default::default()
                    },
                )?;
                ui.set_button_chrome(edit, ButtonChrome::Flat)?;
                let apply = action_icon(
                    ui,
                    row,
                    IconSymbol::Check,
                    &format!("{label}: применить ко всем"),
                )?;
                let reset = action_icon(
                    ui,
                    row,
                    IconSymbol::Eraser,
                    &format!("{label}: значение по умолчанию"),
                )?;
                if specialized.is_some() {
                    if descriptor.metadata.read_only
                        && !matches!(
                            descriptor.metadata.editor.as_str(),
                            "ProgressBar" | "Hyperlink"
                        )
                    {
                        ui.set_enabled(owner, false)?;
                    }
                } else if matches!(recipe, DataFormEditorFactory::CheckBox) {
                    ui.set_enabled(owner, !descriptor.metadata.read_only)?;
                } else if owner != editor {
                    ui.set_composite_read_only(owner, descriptor.metadata.read_only)?;
                } else {
                    ui.set_read_only(editor, descriptor.metadata.read_only)?;
                }
                ui.edit_props(row, |p| {
                    p.visible = descriptor.metadata.visible;
                    p.collapsed = !descriptor.metadata.visible;
                })?;
                let mut property = Property {
                    descriptor,
                    schema,
                    recipe,
                    specialized,
                    family,
                    row,
                    host,
                    name_label,
                    owner,
                    editor,
                    active: false,
                    value_label: None,
                    measure_value_width: false,
                    edit,
                    currency,
                    apply,
                    reset,
                    null,
                    baseline,
                    seed,
                    editing: matches!(state, PropertyValue::Uniform(_)),
                };
                present(ui, &mut property)?;
                properties.push(property);
            }
            let empty = ui.label(body, "Свойства не найдены", LayoutStyle::default())?;
            show(
                ui,
                empty,
                !properties.iter().any(|p| p.descriptor.metadata.visible),
            )?;
            let grouped_order = groups
                .iter()
                .flat_map(|(_, group)| {
                    ui.runtime
                        .tree()
                        .children(group.content)
                        .unwrap_or(&[])
                        .iter()
                        .map(move |&id| (id, group.content))
                })
                .collect();
            let summary = ui.label(container, "", LayoutStyle::default())?;
            ui.set_automation_name(summary, "Результат изменения свойств")?;
            ui.set_text_auto_height(summary, true)?;
            show(ui, summary, false)?;
            let details = if descriptions {
                let panel = ui.transparent_panel(
                    container,
                    LayoutStyle {
                        padding: Edges::all(6.),
                        min_size: Size::new(0., 52.),
                        ..vertical_stack()
                    },
                )?;
                let title = ui.label(
                    panel,
                    "",
                    LayoutStyle {
                        height: Length::Px(20.),
                        ..Default::default()
                    },
                )?;
                ui.set_text_style(
                    title,
                    TextStyle {
                        font_weight: Some(600),
                        wrap: false,
                        ..Default::default()
                    },
                )?;
                let hint = ui.label(panel, "", LayoutStyle::default())?;
                ui.set_text_style(
                    hint,
                    TextStyle {
                        tone: TextTone::Secondary,
                        ..Default::default()
                    },
                )?;
                ui.set_text_auto_height(hint, true)?;
                Some((title, hint))
            } else {
                None
            };
            let mut parents = Vec::new();
            let mut stack = ui
                .runtime
                .tree()
                .children(container)
                .unwrap_or(&[])
                .to_vec();
            while let Some(id) = stack.pop() {
                parents.push((id, ui.runtime.tree().parent(id).unwrap()));
                // Popups, lists and image editors may replace their internal children.
                if properties
                    .iter()
                    .any(|p| p.specialized.is_some() && p.owner == id)
                {
                    continue;
                }
                stack.extend_from_slice(ui.runtime.tree().children(id).unwrap_or(&[]));
            }
            let mut inspector = Self {
                container,
                summary,
                search,
                categorized,
                sorted,
                flat_host,
                viewport,
                empty,
                grouped_order,
                flat: false,
                filter: String::new(),
                saved_expansion: Vec::new(),
                source,
                schema,
                objects: objects.to_vec(),
                properties,
                groups,
                parents,
                details,
                selection: None,
                last_focus: None,
                error: None,
                changes: Vec::new(),
            };
            inspector.restripe(ui);
            inspector.sync_selection(ui)?;
            Ok(inspector)
        })();
        if result.is_err() {
            let _ = ui.remove(container);
        }
        result
    }
    /// Size the value button to its installed display template. An optional text
    /// slot receives the accepted value. Auto-sized slots in horizontal stacks
    /// are remeasured when that value changes.
    pub fn configure_display_template(
        &mut self,
        ui: &mut Controls,
        field: &str,
        slot: Option<(WidgetId, bool)>,
    ) -> Result<(), ControlError> {
        let i = self.index(field)?;
        let p = &mut self.properties[i];
        if p.descriptor.preview != PropertyPreview::Auto || p.specialized.is_some() {
            return Err(invalid(
                "A display template requires the Auto property preview",
            ));
        }
        let root = ui
            .button_content(p.edit)
            .ok_or_else(|| invalid("Property has no display template"))?;
        if let Some((label, auto_width)) = slot {
            if ui.text_style(label).is_none() || !ui.flyout_descendant(label, root) {
                return Err(invalid(
                    "Value label must belong to this property's display template",
                ));
            }
            p.measure_value_width = auto_width
                && ui
                    .runtime
                    .tree()
                    .parent(label)
                    .and_then(|id| ui.runtime.tree().node(id))
                    .is_some_and(|parent| {
                        parent.props.style.kind == LayoutKind::Stack(Axis::Horizontal)
                    });
            if p.descriptor.metadata.read_only {
                ui.controls.get_mut(&label).unwrap().inspector_part =
                    Some(InspectorPart::ReadOnlyText { host: p.host });
            }
        }
        p.value_label = slot.map(|(id, _)| id);
        ui.edit_props(p.edit, |n| {
            n.style.kind = LayoutKind::Grid;
            n.style.grid_columns = vec![GridTrack::Fr(1.)];
            n.style.grid_rows = vec![GridTrack::Fr(1.)];
            n.style.height = Length::Auto;
            n.style.min_size.height = 28.;
        })?;
        present(ui, p)
    }
    /// Search box and the categorized/alphabetical view selectors in tab order.
    pub fn toolbar_parts(&self) -> (WidgetId, WidgetId, WidgetId) {
        (self.search, self.categorized, self.sorted)
    }
    pub fn object_keys(&self) -> &[i64] {
        &self.objects
    }
    pub fn property(&self, name: &str) -> Option<WidgetId> {
        Some(
            self.properties
                .iter()
                .find(|p| p.schema.name == name)?
                .owner,
        )
    }
    pub fn property_editor(&self, name: &str) -> Option<WidgetId> {
        Some(
            self.properties
                .iter()
                .find(|p| p.schema.name == name)?
                .editor,
        )
    }
    pub fn property_row(&self, name: &str) -> Option<WidgetId> {
        Some(self.properties.iter().find(|p| p.schema.name == name)?.row)
    }
    pub fn property_actions(&self, name: &str) -> Option<(WidgetId, WidgetId, WidgetId)> {
        let p = self.properties.iter().find(|p| p.schema.name == name)?;
        Some((p.edit, p.apply, p.reset))
    }
    pub fn property_null_toggle(&self, name: &str) -> Option<WidgetId> {
        self.properties.iter().find(|p| p.schema.name == name)?.null
    }
    pub fn group(&self, path: &[&str]) -> Option<Expander> {
        self.groups
            .iter()
            .find(|(p, _)| p.iter().map(String::as_str).eq(path.iter().copied()))
            .map(|(_, g)| *g)
    }
    pub fn accepted(&self, name: &str) -> Option<PropertyValue> {
        Some(aggregate(
            &self
                .properties
                .iter()
                .find(|p| p.schema.name == name)?
                .baseline,
        ))
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn take_changes(&mut self) -> Vec<PropertyChanged> {
        std::mem::take(&mut self.changes)
    }
    /// Requests produced by explicit hyperlink activations, paired with their field names.
    pub fn take_navigation_requests(&mut self) -> Vec<(String, String)> {
        self.properties
            .iter_mut()
            .filter_map(|p| {
                Some((
                    p.schema.name.clone(),
                    p.specialized.as_mut()?.take_navigation()?,
                ))
            })
            .collect()
    }
    pub fn draft(&self, ui: &Controls, name: &str) -> Result<Option<CellValue>, ControlError> {
        self.check(ui)?;
        let p = &self.properties[self.index(name)?];
        if !p.editing {
            return Ok(None);
        }
        read(ui, p).map(Some).map_err(invalid)
    }
    fn index(&self, name: &str) -> Result<usize, ControlError> {
        self.properties
            .iter()
            .position(|p| p.schema.name == name)
            .ok_or_else(|| invalid("Unknown property"))
    }
    fn check(&self, ui: &Controls) -> Result<(), ControlError> {
        if !ui.runtime.tree().contains(self.container)
            || self
                .parents
                .iter()
                .any(|(id, parent)| ui.runtime.tree().parent(*id) != Some(*parent))
        {
            return Err(invalid(
                "PropertyGrid required parts were removed or reparented",
            ));
        }
        for p in &self.properties {
            check_recipe(ui, p)?;
        }
        let source = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Property source is borrowed"))?;
        if source.table().schema() != &self.schema
            || self.objects.iter().any(|key| source.row(*key).is_none())
        {
            return Err(invalid(
                "Inspected schema or object identities changed; reselect explicitly",
            ));
        }
        Ok(())
    }
    pub fn begin_edit(&mut self, ui: &mut Controls, name: &str) -> Result<(), ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &mut self.properties[i];
        if p.descriptor.metadata.read_only {
            return Err(invalid("Property is read-only"));
        }
        if !p.editing {
            let seed = p.seed.clone();
            load(ui, p, &seed)?;
            p.editing = true;
        }
        self.activate(ui, i)?;
        let p = &self.properties[i];
        if ui.window_focused
            && ui.effective_enabled(p.editor)
            && ui
                .runtime
                .tree()
                .node(p.editor)
                .is_some_and(|node| node.props.focusable)
        {
            match ui.runtime.apply(Mutation::Focus(Some(p.editor))) {
                Ok(()) | Err(RuntimeError::IneligibleFocus(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    /// Prepare an explicit typed draft. This does not write the source.
    pub fn set_draft(
        &mut self,
        ui: &mut Controls,
        name: &str,
        value: CellValue,
    ) -> Result<(), ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &mut self.properties[i];
        if p.descriptor.metadata.read_only {
            return Err(invalid("Property is read-only"));
        }
        load(ui, p, &value)?;
        p.editing = true;
        self.activate(ui, i)
    }
    pub fn cancel(&mut self, ui: &mut Controls, name: &str) -> Result<(), ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &mut self.properties[i];
        let value = match aggregate(&p.baseline) {
            PropertyValue::Uniform(v) => v,
            PropertyValue::Mixed => p.seed.clone(),
        };
        if ui
            .document(p.editor)
            .is_some_and(|d| d.composition().is_some())
            || ui.password_composition_active(p.editor) == Some(true)
        {
            ui.cancel_preedit();
            return Ok(());
        }
        load(ui, p, &value)?;
        p.editing = matches!(aggregate(&p.baseline), PropertyValue::Uniform(_));
        p.active = false;
        present(ui, p)
    }
    pub fn reset(&mut self, ui: &mut Controls, name: &str) -> Result<PropertyCommit, ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let value = self.properties[i]
            .descriptor
            .default
            .clone()
            .ok_or_else(|| invalid("Property has no default"))?;
        self.set_draft(ui, name, value)?;
        self.commit(ui, name)
    }
    pub fn commit(
        &mut self,
        ui: &mut Controls,
        name: &str,
    ) -> Result<PropertyCommit, ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &self.properties[i];
        if p.descriptor.metadata.read_only {
            return self.reject(ui, "Property is read-only");
        }
        if !p.editing {
            return Ok(PropertyCommit::Unchanged);
        }
        if self.changes.len() >= 256 {
            return self.reject(
                ui,
                "Consume property change events before applying more changes",
            );
        }
        if ui
            .document(p.editor)
            .is_some_and(|d| d.composition().is_some())
            || ui.password_composition_active(p.editor) == Some(true)
        {
            return Ok(PropertyCommit::Pending);
        }
        let value = match read(ui, p) {
            Ok(v) => v,
            Err(e) => return self.reject(ui, &e),
        };
        if p.specialized.is_none()
            && p.descriptor.choices.is_empty()
            && ui.document(p.editor).is_some()
            && p.null.is_none_or(|id| ui.checked(id) != Some(true))
        {
            match ui.commit_edit(p.editor)? {
                CommitOutcome::Invalid(e) => return self.reject(ui, &e),
                CommitOutcome::Pending | CommitOutcome::CompositionActive => {
                    return Ok(PropertyCommit::Pending);
                }
                _ => {}
            }
        }
        // Read accepted preparation again: a typed editor may normalize its display.
        let p = &self.properties[i];
        let accepted = read(ui, p).map_err(invalid)?;
        if accepted != value {
            return self.reject(ui, "Editor normalization changed the prepared typed value");
        }
        let changed = p.baseline.iter().any(|old| *old != value);
        if let Some(family) = &p.family
            && let Err(error) = family.validate(&p.descriptor, &p.schema, &value)
        {
            return self.reject(ui, &error.to_string());
        }
        // Include unchanged targets too: their optimistic expectations still
        // belong to the single multi-object transaction.
        let operations: Vec<_> = self
            .objects
            .iter()
            .zip(&p.baseline)
            .map(|(key, old)| DataWrite::Update {
                key: *key,
                field: p.schema.name.clone(),
                expected: old.clone(),
                value: value.clone(),
            })
            .collect();
        // All UI validation happens before storage. Success publishes only owned data.
        ui.set_label(self.summary, "")?;
        show(ui, self.summary, false)?;
        self.error = None;
        let result = self
            .source
            .try_borrow_mut()
            .map_err(|_| invalid("Property source is borrowed"))?
            .write_batch(&operations);
        if let Err(e) = result {
            if self.properties[i].descriptor.metadata.editor == "Password" {
                return self.reject(ui, "Источник отклонил изменение пароля");
            }
            return self.reject(ui, &e.to_string());
        }
        let p = &mut self.properties[i];
        p.active = false;
        if changed {
            p.baseline = vec![value.clone(); self.objects.len()];
            self.changes.push(PropertyChanged {
                field: name.into(),
                value: value.clone(),
                objects: self.objects.clone(),
            });
        }
        if let Some(editor) = &p.specialized {
            editor.load(ui, &value)?;
        }
        present(ui, p)?;
        Ok(if changed {
            PropertyCommit::Saved
        } else {
            PropertyCommit::Unchanged
        })
    }
    fn reject(&mut self, ui: &mut Controls, message: &str) -> Result<PropertyCommit, ControlError> {
        let mut message = bounded_error(message);
        if ui.measure_text(&message, 4096.0, false).is_err() {
            message = "Property change was rejected".into();
        }
        ui.set_label(self.summary, &message)?;
        show(ui, self.summary, true)?;
        self.error = Some(message.clone());
        Ok(PropertyCommit::Rejected(message))
    }
    /// Refresh from the current source snapshot only; no adapter I/O. Dirty
    /// properties keep their baseline and draft and report conflicts. Refresh
    /// applies per property, not as an all-property UI transaction: if a later
    /// editor rejects loading, earlier successful clean refreshes remain valid.
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check(ui)?;
        let source = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Property source is borrowed"))?;
        let values: Vec<Vec<CellValue>> = self
            .properties
            .iter()
            .map(|p| {
                self.objects
                    .iter()
                    .map(|k| source.value(*k, &p.schema.name).unwrap().clone())
                    .collect()
            })
            .collect();
        drop(source);
        // Validate all changed values before replacing any editor baseline,
        // including values hidden behind a mixed multi-object presentation.
        for (p, new) in self.properties.iter().zip(&values) {
            if p.baseline != *new
                && let Some(family) = &p.family
            {
                for value in new {
                    family.validate(&p.descriptor, &p.schema, value)?;
                }
            }
        }
        let mut conflict = false;
        for (p, new) in self.properties.iter_mut().zip(values) {
            if new == p.baseline {
                continue;
            }
            if dirty(ui, p) {
                conflict = true;
                continue;
            }
            let (value, editing) = match aggregate(&new) {
                PropertyValue::Uniform(v) => (v, true),
                PropertyValue::Mixed => (p.seed.clone(), false),
            };
            load(ui, p, &value)?;
            p.baseline = new;
            p.editing = editing;
            p.active &= editing;
            present(ui, p)?;
        }
        if conflict {
            let _ = self.reject(
                ui,
                "Source changed while a property has a prepared draft; cancel or retry explicitly",
            )?;
        }
        Ok(())
    }
    /// Consumes only this inspector's action buttons; other activations remain.
    pub fn update(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        self.check(ui)?;
        for p in &mut self.properties {
            if let Some(editor) = &mut p.specialized {
                editor.update(ui)?;
            }
        }
        if let Some(focus) = ui.runtime.focused()
            && Some(focus) != self.last_focus
            && let Some(i) = self.properties.iter().position(|p| {
                !p.active
                    && !p.descriptor.metadata.read_only
                    && p.editing
                    && p.specialized.as_ref().is_some_and(|e| e.inline())
                    && ui.flyout_descendant(focus, p.owner)
            })
        {
            self.activate(ui, i)?;
        }
        let mut view_mode = self.flat;
        let mut actions = Vec::new();
        ui.activated.retain(|id| {
            if *id == self.categorized || *id == self.sorted {
                view_mode = *id == self.sorted;
                return false;
            }
            if let Some((i, action)) = self.properties.iter().enumerate().find_map(|(i, p)| {
                [(p.edit, 0), (p.apply, 1), (p.reset, 2)]
                    .iter()
                    .find(|(button, _)| button == id)
                    .map(|(_, a)| (i, *a))
            }) {
                actions.push((i, action));
                false
            } else {
                true
            }
        });
        let query = ui
            .document(self.search)
            .unwrap()
            .text()
            .trim()
            .to_lowercase();
        let changed = !actions.is_empty() || view_mode != self.flat || query != self.filter;
        self.arrange(ui, view_mode, query)?;
        if ui.checked(self.categorized) != Some(!view_mode) {
            ui.set_checked(self.categorized, !view_mode)?;
        }
        if ui.checked(self.sorted) != Some(view_mode) {
            ui.set_checked(self.sorted, view_mode)?;
        }
        for (i, action) in actions {
            let name = self.properties[i].schema.name.clone();
            let result = match action {
                0 if self.properties[i].descriptor.metadata.read_only => {
                    self.select(ui, Some(i)).map(|_| ())
                }
                0 => self.begin_edit(ui, &name).map(|_| ()),
                1 => self.commit(ui, &name).map(|_| ()),
                _ => self.reset(ui, &name).map(|_| ()),
            };
            if let Err(e) = result {
                let _ = self.reject(ui, &e.to_string())?;
            }
        }
        for p in self
            .properties
            .iter_mut()
            .filter(|p| p.active && p.null.is_some())
        {
            present(ui, p)?;
        }
        let selection_changed = self.sync_selection(ui)?;
        self.last_focus = ui.runtime.focused();
        self.update_scroll_extent(ui)?;
        Ok(changed || selection_changed)
    }
    /// Optional pre-dispatch Escape route: composition first, then this draft.
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        if let InputEvent::PointerDown {
            position,
            button: rust_desktop_ui_core::PointerButton::Primary,
            ..
        } = event
            && !ui.popup_open()
            && let Some(target) = ui.runtime.hit_test(*position)
            && let Some(index) = self
                .properties
                .iter()
                .position(|p| ui.flyout_descendant(target, p.row))
        {
            let p = &self.properties[index];
            if !p.active
                && p.editing
                && !p.descriptor.metadata.read_only
                && p.specialized.as_ref().is_some_and(|e| e.inline())
                && ui.flyout_descendant(target, p.owner)
            {
                self.activate(ui, index)?;
            } else {
                self.select(ui, Some(index))?;
            }
        }
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        } = event
        else {
            return Ok(false);
        };
        if !matches!(key, Key::Escape | Key::Enter)
            || *modifiers != Modifiers::default()
            || ui.popup_open()
        {
            return Ok(false);
        }
        let Some(id) = ui.runtime.focused() else {
            return Ok(false);
        };
        let Some(p) = self
            .properties
            .iter()
            .find(|p| p.active && ui.flyout_descendant(id, p.host))
        else {
            return Ok(false);
        };
        if *key == Key::Enter
            && (p.descriptor.metadata.editor == "Memo"
                || id != p.editor
                || (ui.document(id).is_none() && ui.password_composition_active(id).is_none())
                || ui.password_composition_active(id) == Some(true)
                || ui.document(id).is_some_and(|d| d.composition().is_some()))
        {
            return Ok(false);
        }
        let name = p.schema.name.clone();
        let button = if p.specialized.as_ref().is_some_and(|e| e.inline()) {
            p.editor
        } else {
            p.edit
        };
        if *key == Key::Escape {
            self.cancel(ui, &name)?;
        } else {
            self.commit(ui, &name)?;
        }
        if !self.properties[self.index(&name)?].active && ui.window_focused {
            match ui.runtime.apply(Mutation::Focus(Some(button))) {
                Ok(()) | Err(RuntimeError::IneligibleFocus(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(true)
    }
}
fn button_style(width: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(32.0),
        vertical_alignment: Some(Align::Center),
        ..Default::default()
    }
}
fn bounded_error(message: &str) -> String {
    // Bound each grapheme before shaping, even for hostile adapter errors.
    let mut result = String::new();
    for ch in message.chars().filter(|c| !c.is_control()).take(256) {
        if ch.is_ascii()
            || ch.is_alphanumeric()
            || ch.is_whitespace()
            || "—.,:;!?()[]+-/".contains(ch)
        {
            result.push(ch);
        }
    }
    if result.is_empty() {
        "Property change was rejected".into()
    } else {
        result
    }
}
