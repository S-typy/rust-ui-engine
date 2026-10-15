//! Property bindings between application data and retained controls.

use crate::{
    BindingContext, BindingConverterRegistry, BindingError, BindingPath, BindingPathSegment,
    CommitOutcome, Controls, DataValue, DateFormat, DateValue, EditorValidator, SelectionItem,
    SelectionValidator, TimeValue,
};
use rust_desktop_ui_core::{Length, Mutation, WidgetId};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap, HashSet},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingProperty {
    Text,
    Value,
    SelectedValue,
    SelectedValues,
    ItemsSource,
    IsChecked,
    IsEnabled,
    IsReadOnly,
    Visibility,
    Width,
    Height,
    PlaceholderText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingEndpoint {
    pub widget: WidgetId,
    pub property: BindingProperty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingSource {
    DataContext(BindingPath),
    Element(BindingEndpoint),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlBindingMode {
    #[default]
    OneWay,
    TwoWay,
    OneTime,
    OneWayToSource,
}

impl ControlBindingMode {
    fn reads_source(self) -> bool {
        self != Self::OneWayToSource
    }

    fn writes_source(self) -> bool {
        matches!(self, Self::TwoWay | Self::OneWayToSource)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BindingUpdateTrigger {
    #[default]
    PropertyChanged,
    /// Publish accepted editor values: Enter and a successful focus-loss
    /// commit both count. Non-editor properties publish immediately.
    LostFocus,
    Explicit,
}

/// Item objects require stable keys. By default key/id identifies an item and
/// label/text/name provides its caption. String items use themselves as both.
/// Member paths are relative to each item; columns follow the main caption.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemBindingMapping {
    pub display_member_path: Option<BindingPath>,
    pub value_member_path: Option<BindingPath>,
    pub columns: Vec<BindingPath>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlBinding {
    pub target: BindingEndpoint,
    pub source: BindingSource,
    pub mode: ControlBindingMode,
    pub trigger: BindingUpdateTrigger,
    pub converter: Option<String>,
    pub fallback: Option<DataValue>,
    pub items: ItemBindingMapping,
}

impl ControlBinding {
    pub fn new(
        widget: WidgetId,
        property: BindingProperty,
        path: &str,
    ) -> Result<Self, BindingError> {
        Ok(Self {
            target: BindingEndpoint { widget, property },
            source: BindingSource::DataContext(BindingPath::parse(path)?),
            mode: ControlBindingMode::OneWay,
            trigger: BindingUpdateTrigger::PropertyChanged,
            converter: None,
            fallback: None,
            items: ItemBindingMapping::default(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingDiagnostic {
    pub target: BindingEndpoint,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
struct ItemState {
    values: Vec<(String, DataValue)>,
}

type SharedItems = Rc<RefCell<HashMap<WidgetId, ItemState>>>;

#[derive(Debug)]
struct LinkState {
    binding: ControlBinding,
    source: Option<DataValue>,
    target: Option<DataValue>,
    initialized: bool,
    error: Option<String>,
}

/// UI-thread binding host. Call `initialize` after all widgets exist, then
/// `sync` after input/model updates and before rendering. The host compares
/// values without draining control events. Source changes win simultaneous
/// edits; changes to unrelated fields preserve drafts, undo and composition.
/// Acyclic Element bindings are evaluated in dependency order.
pub struct ControlBindings {
    context: Rc<RefCell<BindingContext>>,
    converters: Rc<RefCell<BindingConverterRegistry>>,
    links: Vec<LinkState>,
    items: SharedItems,
    applying: Rc<Cell<bool>>,
    diagnostics: Vec<BindingDiagnostic>,
    installed: bool,
    evaluation_order: Vec<usize>,
}

impl ControlBindings {
    pub fn new(context: Rc<RefCell<BindingContext>>) -> Self {
        Self {
            context,
            converters: Rc::new(RefCell::new(BindingConverterRegistry::default())),
            links: Vec::new(),
            items: Rc::new(RefCell::new(HashMap::new())),
            applying: Rc::new(Cell::new(false)),
            diagnostics: Vec::new(),
            installed: false,
            evaluation_order: Vec::new(),
        }
    }

    pub fn with_converters(mut self, converters: Rc<RefCell<BindingConverterRegistry>>) -> Self {
        self.converters = converters;
        self
    }

    pub fn context(&self) -> Rc<RefCell<BindingContext>> {
        Rc::clone(&self.context)
    }

    pub fn converters(&self) -> Rc<RefCell<BindingConverterRegistry>> {
        Rc::clone(&self.converters)
    }

    pub fn add(&mut self, binding: ControlBinding) -> Result<(), BindingError> {
        if self.installed {
            return Err(invalid("Add bindings before initialize"));
        }
        if self.links.len() >= 1024 {
            return Err(invalid("A binding host supports at most 1024 links"));
        }
        if self.links.iter().any(|link| {
            link.binding.target.widget == binding.target.widget
                && (link.binding.target.property == binding.target.property
                    || matches!(
                        (link.binding.target.property, binding.target.property),
                        (BindingProperty::Text, BindingProperty::Value)
                            | (BindingProperty::Value, BindingProperty::Text)
                            | (
                                BindingProperty::SelectedValue,
                                BindingProperty::SelectedValues
                            )
                            | (
                                BindingProperty::SelectedValues,
                                BindingProperty::SelectedValue
                            )
                    ))
        }) {
            return Err(invalid("A property can have only one binding"));
        }
        if binding.target.property == BindingProperty::ItemsSource && binding.mode.writes_source() {
            return Err(invalid("ItemsSource supports OneWay or OneTime"));
        }
        if let Some(value) = &binding.fallback {
            value.validate()?;
        }
        self.links.push(LinkState {
            binding,
            source: None,
            target: None,
            initialized: false,
            error: None,
        });
        if let Err(error) = self.order() {
            self.links.pop();
            return Err(error);
        }
        Ok(())
    }

    /// Install pre-commit source validation and transfer initial values.
    /// Ordinary conversion/model errors are retained as diagnostics and can
    /// recover when data changes. Invalid endpoints/converter registrations
    /// are configuration errors returned here.
    pub fn initialize(&mut self, ui: &mut Controls) -> Result<(), BindingError> {
        if self.installed {
            return Ok(());
        }
        self.evaluation_order = self.order()?;
        for link in &self.links {
            let binding = &link.binding;
            read_property(
                ui,
                binding.target,
                BindingUpdateTrigger::LostFocus,
                &self.items.borrow(),
            )?;
            if let BindingSource::Element(source) = binding.source {
                read_property(
                    ui,
                    source,
                    BindingUpdateTrigger::LostFocus,
                    &self.items.borrow(),
                )?;
            }
            if let Some(converter) = &binding.converter {
                if !self.converters.borrow().contains(converter) {
                    return Err(BindingError::UnknownConverter(converter.clone()));
                }
                if binding.mode.writes_source()
                    && !self.converters.borrow().supports_reverse(converter)
                {
                    return Err(BindingError::MissingReverseConverter(converter.clone()));
                }
            }
        }
        for &index in &self.evaluation_order {
            self.install_validator(ui, &self.links[index].binding)?;
        }
        self.installed = true;
        self.sync(ui);
        Ok(())
    }

    pub fn sync(&mut self, ui: &mut Controls) -> bool {
        self.synchronize(ui, false)
    }

    /// Publish explicit bindings as well as pending ordinary source updates.
    /// Editor drafts are validated before publication and accepted only after
    /// the model accepts their value. Failed values remain editable drafts.
    pub fn update_sources(&mut self, ui: &mut Controls) -> bool {
        self.synchronize(ui, true)
    }

    pub fn drain_diagnostics(&mut self) -> Vec<BindingDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    pub fn diagnostics(&self) -> &[BindingDiagnostic] {
        &self.diagnostics
    }

    fn order(&self) -> Result<Vec<usize>, BindingError> {
        let targets: HashMap<_, _> = self
            .links
            .iter()
            .enumerate()
            .map(|(index, link)| (dependency_endpoint(link.binding.target), index))
            .collect();
        let mut incoming = vec![0_usize; self.links.len()];
        let mut children = vec![Vec::new(); self.links.len()];
        for (index, link) in self.links.iter().enumerate() {
            if let BindingSource::Element(source) = link.binding.source
                && let Some(&parent) = targets.get(&dependency_endpoint(source))
            {
                incoming[index] += 1;
                children[parent].push(index);
            }
        }
        let priority = |index: usize| {
            (
                self.links[index].binding.target.property != BindingProperty::ItemsSource,
                index,
            )
        };
        let mut ready: BTreeSet<_> = incoming
            .iter()
            .enumerate()
            .filter(|(_, count)| **count == 0)
            .map(|(index, _)| priority(index))
            .collect();
        let mut ordered = Vec::with_capacity(self.links.len());
        while let Some((_, index)) = ready.pop_first() {
            ordered.push(index);
            for &child in &children[index] {
                incoming[child] -= 1;
                if incoming[child] == 0 {
                    ready.insert(priority(child));
                }
            }
        }
        if ordered.len() != self.links.len() {
            return Err(invalid(
                "Element bindings cannot contain a dependency cycle",
            ));
        }
        Ok(ordered)
    }

    fn synchronize(&mut self, ui: &mut Controls, explicit: bool) -> bool {
        if !self.installed {
            return false;
        }
        let order = self.evaluation_order.clone();
        let mut changed = false;
        for index in order {
            let binding = self.links[index].binding.clone();
            let result = self.sync_link(ui, index, &binding, explicit);
            match result {
                Ok(did_change) => {
                    changed |= did_change;
                    self.links[index].error = None;
                }
                Err(error) => {
                    let message = error.to_string();
                    if self.links[index].error.as_ref() != Some(&message) {
                        if self.diagnostics.len() == 1024 {
                            self.diagnostics.remove(0);
                        }
                        self.diagnostics.push(BindingDiagnostic {
                            target: binding.target,
                            message: message.clone(),
                        });
                        self.links[index].error = Some(message);
                    }
                }
            }
        }
        changed
    }

    fn sync_link(
        &mut self,
        ui: &mut Controls,
        index: usize,
        binding: &ControlBinding,
        explicit: bool,
    ) -> Result<bool, BindingError> {
        if self.links[index].initialized && binding.mode == ControlBindingMode::OneTime {
            return Ok(false);
        }
        let source = if binding.mode.reads_source() {
            self.read_source(ui, binding)
        } else {
            Ok(DataValue::Null)
        };
        let source = match source {
            Ok(value) => value,
            Err(error) => match &binding.fallback {
                Some(fallback) if binding.mode.reads_source() => {
                    if self.links[index].target.as_ref() != Some(fallback) {
                        self.apply_target(ui, binding, fallback)?;
                        self.links[index].target = Some(read_property(
                            ui,
                            binding.target,
                            binding.trigger,
                            &self.items.borrow(),
                        )?);
                    }
                    if binding.mode == ControlBindingMode::OneTime {
                        self.links[index].initialized = true;
                    }
                    return Err(error);
                }
                _ => return Err(error),
            },
        };
        if binding.mode.reads_source()
            && (!self.links[index].initialized
                || self.links[index].source.as_ref() != Some(&source))
        {
            self.apply_target(ui, binding, &source)?;
            self.links[index].source = Some(source);
            self.links[index].target = Some(read_property(
                ui,
                binding.target,
                binding.trigger,
                &self.items.borrow(),
            )?);
            self.links[index].initialized = true;
            return Ok(true);
        }
        if !binding.mode.writes_source()
            || (binding.trigger == BindingUpdateTrigger::Explicit && !explicit)
        {
            self.links[index].initialized = true;
            return Ok(false);
        }
        let read_trigger = if explicit {
            BindingUpdateTrigger::PropertyChanged
        } else {
            binding.trigger
        };
        let value = read_property(ui, binding.target, read_trigger, &self.items.borrow())?;
        if self.links[index].initialized && self.links[index].target.as_ref() == Some(&value) {
            return Ok(false);
        }
        if let Some(editor) = value_editor(ui, binding.target) {
            let document = ui
                .document(editor)
                .ok_or_else(|| invalid("Missing bound editor"))?;
            if document.composition().is_some() {
                return Ok(false);
            }
            if read_trigger == BindingUpdateTrigger::PropertyChanged {
                ui.validate_edit(editor, document.text())
                    .map_err(BindingError::Validation)?;
            }
        }
        let mut converted = self.convert_back(binding, &value)?;
        // Boolean visibility is a compatibility shorthand (false = Hidden).
        // Preserve an explicitly boolean source in a reverse transfer.
        if binding.target.property == BindingProperty::Visibility
            && binding.converter.is_none()
            && self
                .read_source(ui, binding)
                .ok()
                .is_some_and(|source| matches!(source, DataValue::Bool(_)))
        {
            converted = DataValue::Bool(value.as_str() == Some("Visible"));
        }
        let changed = match &binding.source {
            BindingSource::DataContext(path) => {
                self.context.borrow_mut().set_path(path, converted)?
            }
            BindingSource::Element(endpoint) => {
                let result = write_property(
                    ui,
                    *endpoint,
                    &converted,
                    &ItemBindingMapping::default(),
                    &self.items,
                );
                result?;
                true
            }
        };
        if explicit && let Some(editor) = value_editor(ui, binding.target) {
            self.applying.set(true);
            let outcome = ui.commit_edit(editor).map_err(control_error);
            self.applying.set(false);
            accepted(outcome?)?;
        }
        if binding.mode.reads_source() {
            self.links[index].source = Some(self.read_source(ui, binding)?);
        }
        self.links[index].target = Some(value);
        self.links[index].initialized = true;
        Ok(changed)
    }

    fn read_source(
        &self,
        ui: &Controls,
        binding: &ControlBinding,
    ) -> Result<DataValue, BindingError> {
        let value = match &binding.source {
            BindingSource::DataContext(path) => self.context.borrow().get_path(path)?.clone(),
            BindingSource::Element(endpoint) => read_property(
                ui,
                *endpoint,
                BindingUpdateTrigger::PropertyChanged,
                &self.items.borrow(),
            )?,
        };
        match &binding.converter {
            Some(name) => self.converters.borrow().convert(name, &value),
            None => Ok(value),
        }
    }

    fn convert_back(
        &self,
        binding: &ControlBinding,
        value: &DataValue,
    ) -> Result<DataValue, BindingError> {
        match &binding.converter {
            Some(name) => self.converters.borrow().convert_back(name, value),
            None => Ok(value.clone()),
        }
    }

    fn apply_target(
        &self,
        ui: &mut Controls,
        binding: &ControlBinding,
        value: &DataValue,
    ) -> Result<(), BindingError> {
        self.applying.set(true);
        let result = write_property(ui, binding.target, value, &binding.items, &self.items);
        self.applying.set(false);
        result
    }

    fn install_validator(
        &self,
        ui: &mut Controls,
        binding: &ControlBinding,
    ) -> Result<(), BindingError> {
        if !binding.mode.writes_source() {
            return Ok(());
        }
        let source_check: Box<dyn Fn(DataValue) -> Result<(), String>> = match &binding.source {
            BindingSource::DataContext(path) => {
                let path = path.clone();
                let context = Rc::clone(&self.context);
                Box::new(move |value| {
                    context
                        .borrow()
                        .validate_set(path.as_str(), &value)
                        .map_err(|error| error.to_string())
                })
            }
            BindingSource::Element(endpoint) => {
                let Some(editor) = value_editor(ui, *endpoint) else {
                    return Ok(());
                };
                let application = ui.editor_validator(editor).cloned();
                let validation = ui.editor_validation(editor).cloned().unwrap_or_default();
                let settings = ui.editor_settings(editor).cloned().unwrap_or_default();
                let parser = editor_parser(ui, *endpoint);
                let spin = ui.spin_settings(endpoint.widget).cloned();
                let date = ui.date_settings(endpoint.widget).cloned();
                let time = ui.time_settings(endpoint.widget).cloned();
                Box::new(move |value| {
                    let text = match parser {
                        EditorParser::Text => string(&value)
                            .map_err(|error| error.to_string())?
                            .to_owned(),
                        EditorParser::Number => {
                            let value = number(&value).map_err(|error| error.to_string())?;
                            if let Some(settings) = &spin
                                && !(settings.minimum..=settings.maximum).contains(&value)
                            {
                                return Err("Number outside source editor bounds".into());
                            }
                            value.to_string()
                        }
                        EditorParser::Date(format) => {
                            let value = match value {
                                DataValue::Null => None,
                                _ => Some(
                                    DateValue::parse_iso(
                                        string(&value).map_err(|error| error.to_string())?,
                                    )
                                    .map_err(|error| error.to_string())?,
                                ),
                            };
                            if let Some(settings) = &date {
                                if let Some(value) = value {
                                    if settings.minimum.is_some_and(|min| value < min)
                                        || settings.maximum.is_some_and(|max| value > max)
                                    {
                                        return Err("Date outside source editor bounds".into());
                                    }
                                } else if !settings.allow_empty {
                                    return Err("Source date cannot be empty".into());
                                }
                            }
                            value.map_or(String::new(), |value| value.to_display(format))
                        }
                        EditorParser::Time => {
                            let value = match value {
                                DataValue::Null => None,
                                _ => Some(
                                    TimeValue::parse(
                                        string(&value).map_err(|error| error.to_string())?,
                                    )
                                    .map_err(|error| error.to_string())?,
                                ),
                            };
                            if let Some(settings) = &time {
                                if let Some(value) = value {
                                    if settings.minimum.is_some_and(|min| value < min)
                                        || settings.maximum.is_some_and(|max| value > max)
                                        || (!settings.show_seconds && value.second != 0)
                                    {
                                        return Err(
                                            "Time outside source editor bounds or precision".into(),
                                        );
                                    }
                                } else if !settings.allow_empty {
                                    return Err("Source time cannot be empty".into());
                                }
                            }
                            value.map_or(String::new(), |value| {
                                value.to_display(
                                    time.as_ref().is_some_and(|settings| settings.show_seconds),
                                )
                            })
                        }
                    };
                    rust_desktop_ui_text::TextLimits::default()
                        .validate(&text)
                        .map_err(|error| error.to_string())?;
                    if let Some(maximum) = settings.max_length
                        && rust_desktop_ui_text::TextDocument::new(&text)
                            .grapheme_boundaries()
                            .len()
                            .saturating_sub(1)
                            > maximum
                    {
                        return Err("Text exceeds source editor MaxLength".into());
                    }
                    validation.check(&text)?;
                    if let Some(validator) = &application {
                        validator.check(&text)?;
                    }
                    Ok(())
                })
            }
        };
        let converters = Rc::clone(&self.converters);
        let converter = binding.converter.clone();
        let applying = Rc::clone(&self.applying);
        let validate = move |value: DataValue| -> Result<(), String> {
            if applying.get() {
                return Ok(());
            }
            let value = match &converter {
                Some(name) => converters
                    .borrow()
                    .convert_back(name, &value)
                    .map_err(|error| error.to_string())?,
                None => value,
            };
            source_check(value)
        };
        if matches!(
            binding.target.property,
            BindingProperty::SelectedValue | BindingProperty::SelectedValues
        ) {
            let endpoint = binding.target;
            let items = Rc::clone(&self.items);
            ui.chain_selection_validator(
                endpoint.widget,
                SelectionValidator::new(move |keys| {
                    let value = selected_data(
                        keys,
                        endpoint.property == BindingProperty::SelectedValues,
                        items.borrow().get(&endpoint.widget),
                    );
                    validate(value)
                }),
            )
            .map_err(control_error)?;
        } else if let Some(editor) = value_editor(ui, binding.target) {
            let parser = editor_parser(ui, binding.target);
            ui.chain_editor_validator(
                editor,
                EditorValidator::new(move |text| {
                    validate(parser.parse(text).map_err(|error| error.to_string())?)
                }),
            )
            .map_err(control_error)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum EditorParser {
    Text,
    Number,
    Date(DateFormat),
    Time,
}

impl EditorParser {
    fn parse(self, text: &str) -> Result<DataValue, BindingError> {
        match self {
            Self::Text => Ok(text.into()),
            Self::Number => {
                let value = text
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| invalid("Expected a number"))?;
                if !value.is_finite() {
                    return Err(invalid("Expected a finite number"));
                }
                Ok(value.into())
            }
            Self::Date(format) => {
                if text.trim().is_empty() {
                    Ok(DataValue::Null)
                } else {
                    DateValue::parse_display(text, format)
                        .map(|value| value.to_iso().into())
                        .map_err(control_error)
                }
            }
            Self::Time => {
                if text.trim().is_empty() {
                    Ok(DataValue::Null)
                } else {
                    TimeValue::parse(text)
                        .map(|value| value.to_iso().into())
                        .map_err(control_error)
                }
            }
        }
    }
}

fn editor_parser(ui: &Controls, endpoint: BindingEndpoint) -> EditorParser {
    if endpoint.property != BindingProperty::Value {
        return EditorParser::Text;
    }
    if ui.spin_settings(endpoint.widget).is_some() {
        EditorParser::Number
    } else if let Some(settings) = ui.date_settings(endpoint.widget) {
        EditorParser::Date(settings.format)
    } else if ui.time_settings(endpoint.widget).is_some() {
        EditorParser::Time
    } else {
        EditorParser::Text
    }
}

fn value_editor(ui: &Controls, endpoint: BindingEndpoint) -> Option<WidgetId> {
    if !matches!(
        endpoint.property,
        BindingProperty::Text | BindingProperty::Value
    ) {
        return None;
    }
    ui.composite_editor(endpoint.widget)
        .or_else(|| ui.document(endpoint.widget).map(|_| endpoint.widget))
}

fn read_property(
    ui: &Controls,
    endpoint: BindingEndpoint,
    trigger: BindingUpdateTrigger,
    items: &HashMap<WidgetId, ItemState>,
) -> Result<DataValue, BindingError> {
    let id = endpoint.widget;
    let props = &ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| invalid("A bound widget no longer exists"))?
        .props;
    if let Some(editor) = value_editor(ui, endpoint) {
        let text = if trigger == BindingUpdateTrigger::LostFocus {
            ui.accepted_value(editor)
        } else {
            ui.document(editor).map(|doc| doc.text())
        }
        .ok_or_else(|| invalid("Missing bound editor"))?;
        return editor_parser(ui, endpoint).parse(text);
    }
    match endpoint.property {
        BindingProperty::Text => ui
            .label_text(id)
            .map(DataValue::from)
            .ok_or_else(|| invalid("Text requires an editor or caption")),
        BindingProperty::Value => Err(invalid("Value requires an editor")),
        BindingProperty::SelectedValue | BindingProperty::SelectedValues => {
            let keys = ui
                .selected_keys(id)
                .ok_or_else(|| invalid("SelectedValue requires a selection control"))?;
            Ok(selected_data(
                keys,
                endpoint.property == BindingProperty::SelectedValues,
                items.get(&id),
            ))
        }
        BindingProperty::ItemsSource => ui
            .selection_items(id)
            .map(|values| {
                DataValue::List(
                    values
                        .iter()
                        .map(|item| {
                            DataValue::object([
                                ("key", item.key.clone().into()),
                                ("label", item.text.clone().into()),
                                ("enabled", item.enabled.into()),
                                (
                                    "columns",
                                    DataValue::List(
                                        item.columns
                                            .iter()
                                            .cloned()
                                            .map(DataValue::String)
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                )
            })
            .ok_or_else(|| invalid("ItemsSource requires a selection control")),
        BindingProperty::IsChecked => ui
            .checked(id)
            .map(DataValue::Bool)
            .ok_or_else(|| invalid("IsChecked requires a checkable control")),
        BindingProperty::IsEnabled => Ok(props.enabled.into()),
        BindingProperty::Visibility => Ok(if props.collapsed {
            "Collapsed"
        } else if props.visible {
            "Visible"
        } else {
            "Hidden"
        }
        .into()),
        BindingProperty::IsReadOnly => ui
            .composite_read_only(id)
            .or_else(|| ui.is_read_only(id))
            .or_else(|| ui.selection_settings(id).map(|s| s.read_only))
            .map(DataValue::Bool)
            .ok_or_else(|| invalid("IsReadOnly requires an editor or list")),
        BindingProperty::PlaceholderText => {
            let editor = ui.composite_editor(id).unwrap_or(id);
            ui.editor_settings(editor)
                .map(|s| s.placeholder.clone().into())
                .ok_or_else(|| invalid("PlaceholderText requires an editor"))
        }
        BindingProperty::Width | BindingProperty::Height => match if endpoint.property
            == BindingProperty::Width
        {
            props.style.width
        } else {
            props.style.height
        } {
            Length::Px(value) => Ok(DataValue::Number(value as f64)),
            Length::Auto => Ok(DataValue::Null),
            Length::Percent(_) => Err(invalid("Bound dimensions support pixels or null for Auto")),
        },
    }
}

fn write_property(
    ui: &mut Controls,
    endpoint: BindingEndpoint,
    value: &DataValue,
    mapping: &ItemBindingMapping,
    items: &SharedItems,
) -> Result<(), BindingError> {
    let id = endpoint.widget;
    value.validate()?;
    if let Some(editor) = value_editor(ui, endpoint) {
        let outcome = match editor_parser(ui, endpoint) {
            EditorParser::Text => ui.set_editor_value(editor, string(value)?),
            EditorParser::Number => ui.set_spin_value(id, number(value)?),
            EditorParser::Date(_) => ui.set_date_value(
                id,
                match value {
                    DataValue::Null => None,
                    _ => Some(DateValue::parse_iso(string(value)?).map_err(control_error)?),
                },
            ),
            EditorParser::Time => ui.set_time_value(
                id,
                match value {
                    DataValue::Null => None,
                    _ => Some(TimeValue::parse(string(value)?).map_err(control_error)?),
                },
            ),
        }
        .map_err(control_error)?;
        return accepted(outcome);
    }
    match endpoint.property {
        BindingProperty::Text => ui.set_label(id, string(value)?).map_err(control_error),
        BindingProperty::Value => Err(invalid("Value requires an editor")),
        BindingProperty::SelectedValue | BindingProperty::SelectedValues => {
            let selected = if endpoint.property == BindingProperty::SelectedValues {
                value
                    .as_list()
                    .ok_or_else(|| invalid("SelectedValues requires a list"))?
                    .to_vec()
            } else if *value == DataValue::Null {
                Vec::new()
            } else {
                vec![value.clone()]
            };
            let states = items.borrow();
            let keys: Result<Vec<_>, _> = selected
                .iter()
                .map(|value| {
                    if let Some(state) = states.get(&id) {
                        state
                            .values
                            .iter()
                            .find(|(_, candidate)| candidate == value)
                            .map(|(key, _)| key.clone())
                            .ok_or_else(|| invalid("Selected value is absent from ItemsSource"))
                    } else {
                        string(value).map(str::to_owned)
                    }
                })
                .collect();
            ui.set_selected_keys(id, &keys?)
                .map(|_| ())
                .map_err(control_error)
        }
        BindingProperty::ItemsSource => {
            let (rows, state) = map_items(value, mapping)?;
            // Install identity mapping before validators inspect a selection
            // retained across replacement. Restore it if the control rejects.
            let previous = items.borrow_mut().insert(id, state);
            if let Err(error) = ui.set_selection_items(id, rows) {
                if let Some(previous) = previous {
                    items.borrow_mut().insert(id, previous);
                } else {
                    items.borrow_mut().remove(&id);
                }
                return Err(control_error(error));
            }
            Ok(())
        }
        BindingProperty::IsChecked => ui.set_checked(id, boolean(value)?).map_err(control_error),
        BindingProperty::IsEnabled => ui.set_enabled(id, boolean(value)?).map_err(control_error),
        BindingProperty::IsReadOnly => {
            let value = boolean(value)?;
            if ui.composite_editor(id).is_some() {
                ui.set_composite_read_only(id, value).map_err(control_error)
            } else if let Some(settings) = ui.selection_settings(id) {
                let mut settings = settings.clone();
                settings.read_only = value;
                ui.set_selection_settings(id, settings)
                    .map_err(control_error)
            } else {
                ui.set_read_only(id, value).map_err(control_error)
            }
        }
        BindingProperty::PlaceholderText => ui
            .set_placeholder(ui.composite_editor(id).unwrap_or(id), string(value)?)
            .map_err(control_error),
        BindingProperty::Visibility | BindingProperty::Width | BindingProperty::Height => {
            let mut props = ui
                .runtime
                .tree()
                .node(id)
                .ok_or_else(|| invalid("Missing bound widget"))?
                .props
                .clone();
            if endpoint.property == BindingProperty::Visibility {
                let (visible, collapsed) = match value {
                    DataValue::Bool(value) => (*value, false),
                    DataValue::String(value) if value == "Visible" => (true, false),
                    DataValue::String(value) if value == "Hidden" => (false, false),
                    DataValue::String(value) if value == "Collapsed" => (false, true),
                    _ => {
                        return Err(invalid(
                            "Visibility requires Visible, Hidden, Collapsed, or a boolean",
                        ));
                    }
                };
                props.visible = visible;
                props.collapsed = collapsed;
            } else {
                let dimension = match value {
                    DataValue::Null => Length::Auto,
                    _ => {
                        let value = number(value)?;
                        if !(0.0..=16_777_216.0).contains(&value) {
                            return Err(invalid("Bound pixel dimensions must be in 0..=16777216"));
                        }
                        Length::Px(value as f32)
                    }
                };
                if endpoint.property == BindingProperty::Width {
                    props.style.width = dimension;
                } else {
                    props.style.height = dimension;
                }
            }
            ui.runtime
                .apply(Mutation::SetProps { node: id, props })
                .map(|_| ())
                .map_err(|error| invalid(&error.to_string()))
        }
    }
}

fn selected_data(keys: &[String], multiple: bool, state: Option<&ItemState>) -> DataValue {
    let values: Vec<_> = keys
        .iter()
        .map(|key| {
            state
                .and_then(|state| state.values.iter().find(|(candidate, _)| candidate == key))
                .map_or_else(|| key.clone().into(), |(_, value)| value.clone())
        })
        .collect();
    if multiple {
        DataValue::List(values)
    } else {
        values.into_iter().next().unwrap_or(DataValue::Null)
    }
}

fn map_items(
    value: &DataValue,
    mapping: &ItemBindingMapping,
) -> Result<(Vec<SelectionItem>, ItemState), BindingError> {
    let source = value
        .as_list()
        .ok_or_else(|| invalid("ItemsSource requires a list"))?;
    let mut rows = Vec::with_capacity(source.len());
    let mut values = Vec::with_capacity(source.len());
    let mut keys = HashSet::new();
    for item in source {
        let identity = if let Some(path) = &mapping.value_member_path {
            member(item, path)?
        } else if let DataValue::Object(fields) = item {
            fields
                .get("key")
                .or_else(|| fields.get("id"))
                .ok_or_else(|| invalid("Item objects require key/id or ValueMemberPath"))?
        } else {
            item
        };
        let key = match identity {
            DataValue::String(value) if !value.is_empty() => {
                if value.starts_with('\u{1f}') {
                    format!("\u{1f}string:{value}")
                } else {
                    value.clone()
                }
            }
            DataValue::Number(value) => {
                format!("\u{1f}number:{}", if *value == 0.0 { 0.0 } else { *value })
            }
            DataValue::Bool(value) => format!("\u{1f}bool:{value}"),
            _ => {
                return Err(invalid(
                    "Item identities must be nonempty strings, numbers or booleans",
                ));
            }
        };
        if !keys.insert(key.clone()) {
            return Err(invalid("ItemsSource identities must be unique"));
        }
        let label = if let Some(path) = &mapping.display_member_path {
            member(item, path)?
        } else if let DataValue::Object(fields) = item {
            fields
                .get("label")
                .or_else(|| fields.get("text"))
                .or_else(|| fields.get("name"))
                .unwrap_or(identity)
        } else {
            item
        };
        let columns = if mapping.columns.is_empty() {
            match item.as_object().and_then(|fields| fields.get("columns")) {
                Some(DataValue::List(values)) => {
                    values.iter().map(display_value).collect::<Result<_, _>>()?
                }
                Some(_) => return Err(invalid("Item columns must be a list")),
                None => Vec::new(),
            }
        } else {
            mapping
                .columns
                .iter()
                .map(|path| member(item, path).and_then(display_value))
                .collect::<Result<_, _>>()?
        };
        let enabled = item
            .as_object()
            .and_then(|fields| fields.get("enabled"))
            .map(boolean)
            .transpose()?
            .unwrap_or(true);
        rows.push(SelectionItem {
            key: key.clone(),
            text: display_value(label)?,
            columns,
            enabled,
        });
        values.push((key, identity.clone()));
    }
    Ok((rows, ItemState { values }))
}

fn member<'a>(value: &'a DataValue, path: &BindingPath) -> Result<&'a DataValue, BindingError> {
    path.segments().iter().try_fold(value, |value, segment| {
        match (value, segment) {
            (DataValue::Object(fields), BindingPathSegment::Field(name)) => fields.get(name),
            (DataValue::List(values), BindingPathSegment::Index(index)) => values.get(*index),
            _ => None,
        }
        .ok_or_else(|| BindingError::MissingPath(path.to_string()))
    })
}

fn string(value: &DataValue) -> Result<&str, BindingError> {
    value
        .as_str()
        .ok_or_else(|| invalid("Expected a string; register an explicit converter for other types"))
}
fn boolean(value: &DataValue) -> Result<bool, BindingError> {
    value.as_bool().ok_or_else(|| invalid("Expected a boolean"))
}
fn number(value: &DataValue) -> Result<f64, BindingError> {
    value
        .as_number()
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid("Expected a finite number"))
}
fn display_value(value: &DataValue) -> Result<String, BindingError> {
    match value {
        DataValue::Null => Ok(String::new()),
        DataValue::String(value) => Ok(value.clone()),
        DataValue::Bool(value) => Ok(value.to_string()),
        DataValue::Number(value) => Ok(value.to_string()),
        _ => Err(invalid("An item caption or cell requires a scalar value")),
    }
}
fn accepted(outcome: CommitOutcome) -> Result<(), BindingError> {
    match outcome {
        CommitOutcome::Committed | CommitOutcome::Unchanged => Ok(()),
        CommitOutcome::Invalid(message) => Err(BindingError::Validation(message)),
        CommitOutcome::CompositionActive => Err(invalid(
            "An active composition cannot be replaced by a binding",
        )),
    }
}
fn invalid(message: &str) -> BindingError {
    BindingError::InvalidBinding(message.into())
}

fn dependency_endpoint(mut endpoint: BindingEndpoint) -> BindingEndpoint {
    endpoint.property = match endpoint.property {
        BindingProperty::Value => BindingProperty::Text,
        BindingProperty::SelectedValues => BindingProperty::SelectedValue,
        property => property,
    };
    endpoint
}
fn control_error(error: crate::ControlError) -> BindingError {
    invalid(&error.to_string())
}

#[cfg(test)]
#[path = "control_binding_tests.rs"]
mod tests;
