//! Property bindings between application data and retained controls.

use crate::controls::TemporalValueParser;
use crate::{
    BindingContext, BindingConverterRegistry, BindingError, BindingExpression, BindingPath,
    BindingPathSegment, CalendarSelection, CalendarValidator, CommitOutcome, Controls, DataValue,
    DateFormat, DateRangeValue, DateValue, EditorValidator, EditorValue, NumericEditSettings,
    SelectionItem, SelectionValidator, TimeValue,
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
    Query,
    FilterText,
    Header,
    IsExpanded,
    SelectedIndex,
    Value,
    LowerValue,
    UpperValue,
    IsIndeterminate,
    SelectedKey,
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
    BadgeCount,
    BadgeText,
    BadgeVisibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingEndpoint {
    pub widget: WidgetId,
    pub property: BindingProperty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingInput {
    DataContext(BindingPath),
    Element(BindingEndpoint),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BindingSource {
    DataContext(BindingPath),
    Element(BindingEndpoint),
    /// OneWay/OneTime source. Its converter receives a DataValue::List in input order.
    Multi(Vec<BindingInput>),
    /// OneWay/OneTime source using a bounded expression and explicit dependencies.
    Expression {
        inputs: Vec<BindingInput>,
        expression: BindingExpression,
    },
}
impl BindingSource {
    fn elements(&self) -> Vec<BindingEndpoint> {
        match self {
            Self::Element(endpoint) => vec![*endpoint],
            Self::DataContext(_) => vec![],
            Self::Multi(inputs) | Self::Expression { inputs, .. } => inputs
                .iter()
                .filter_map(|input| {
                    if let BindingInput::Element(e) = input {
                        Some(*e)
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
    fn paths(&self) -> Vec<&BindingPath> {
        match self {
            Self::DataContext(path) => vec![path],
            Self::Element(_) => vec![],
            Self::Multi(inputs) | Self::Expression { inputs, .. } => inputs
                .iter()
                .filter_map(|input| {
                    if let BindingInput::DataContext(p) = input {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
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
    source_revision: Option<u64>,
    converter_revision: u64,
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
    validation_revision: u64,
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
            validation_revision: 0,
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
                        (BindingProperty::BadgeCount, BindingProperty::BadgeText)
                            | (BindingProperty::BadgeText, BindingProperty::BadgeCount)
                            | (BindingProperty::Text, BindingProperty::Value)
                            | (BindingProperty::Value, BindingProperty::Text)
                            | (BindingProperty::Text, BindingProperty::Header)
                            | (BindingProperty::Header, BindingProperty::Text)
                            | (BindingProperty::Value, BindingProperty::Header)
                            | (BindingProperty::Header, BindingProperty::Value)
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
        if matches!(
            binding.target.property,
            BindingProperty::BadgeCount
                | BindingProperty::BadgeText
                | BindingProperty::BadgeVisibility
        ) && binding.mode.writes_source()
        {
            return Err(invalid("Badge bindings support OneWay or OneTime"));
        }
        if binding.target.property == BindingProperty::ItemsSource && binding.mode.writes_source() {
            return Err(invalid("ItemsSource supports OneWay or OneTime"));
        }
        if let BindingSource::Multi(inputs) | BindingSource::Expression { inputs, .. } =
            &binding.source
        {
            if inputs.is_empty() || inputs.len() > 32 || binding.mode.writes_source() {
                return Err(invalid(
                    "Multi-source bindings require 1..32 inputs and OneWay/OneTime mode",
                ));
            }
            if let BindingSource::Expression { expression, .. } = &binding.source
                && expression.input_count() > inputs.len()
            {
                return Err(invalid("Expression references an undeclared input"));
            }
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
            source_revision: None,
            converter_revision: 0,
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
        self.normalize_group_endpoints(ui)?;
        self.evaluation_order = self.order()?;
        for link in &self.links {
            let binding = &link.binding;
            if binding.target.property == BindingProperty::Value
                && ui.progress_settings(binding.target.widget).is_some()
                && binding.mode.writes_source()
            {
                return Err(invalid("ProgressBar Value supports OneWay or OneTime"));
            }
            read_property(
                ui,
                binding.target,
                BindingUpdateTrigger::LostFocus,
                &self.items.borrow(),
            )?;
            for source in binding.source.elements() {
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

    fn normalize_group_endpoints(&mut self, ui: &Controls) -> Result<(), BindingError> {
        let canonical = |mut endpoint: BindingEndpoint| {
            if matches!(
                endpoint.property,
                BindingProperty::Text | BindingProperty::Header | BindingProperty::IsExpanded
            ) && let Some(owner) = ui.group_owner(endpoint.widget)
            {
                endpoint.widget = owner;
            }
            if endpoint.property == BindingProperty::SelectedIndex
                && let Some(owner) = ui.tab_owner(endpoint.widget)
            {
                endpoint.widget = owner;
            }
            endpoint
        };
        let mut targets = HashSet::new();
        let mut normalized = Vec::with_capacity(self.links.len());
        for link in &self.links {
            let mut binding = link.binding.clone();
            binding.target = canonical(binding.target);
            if !targets.insert(dependency_endpoint(binding.target)) {
                return Err(invalid(
                    "A property can have only one binding, including group header aliases",
                ));
            }
            match &mut binding.source {
                BindingSource::Element(source) => *source = canonical(*source),
                BindingSource::Multi(inputs) | BindingSource::Expression { inputs, .. } => {
                    for input in inputs {
                        if let BindingInput::Element(source) = input {
                            *source = canonical(*source);
                        }
                    }
                }
                BindingSource::DataContext(_) => {}
            }
            normalized.push(binding);
        }
        let previous: Vec<_> = self
            .links
            .iter_mut()
            .zip(normalized)
            .map(|(link, normalized)| std::mem::replace(&mut link.binding, normalized))
            .collect();
        if let Err(error) = self.order() {
            for (link, previous) in self.links.iter_mut().zip(previous) {
                link.binding = previous;
            }
            return Err(error);
        }
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
        let mut parents: Vec<HashSet<usize>> = self
            .links
            .iter()
            .map(|link| {
                link.binding
                    .source
                    .elements()
                    .into_iter()
                    .filter_map(|source| targets.get(&dependency_endpoint(source)).copied())
                    .collect()
            })
            .collect();
        // Both range endpoints publish together. Their dependency sets are
        // therefore one transaction: all inputs must be evaluated before either
        // endpoint, and a dependency through the other thumb is still a cycle.
        for (index, link) in self.links.iter().enumerate() {
            if !link.binding.mode.reads_source()
                || !matches!(
                    link.binding.target.property,
                    BindingProperty::LowerValue | BindingProperty::UpperValue
                )
            {
                continue;
            }
            if let Some(partner) = self.links.iter().position(|other| {
                other.binding.mode.reads_source()
                    && other.binding.target.widget == link.binding.target.widget
                    && matches!(
                        other.binding.target.property,
                        BindingProperty::LowerValue | BindingProperty::UpperValue
                    )
                    && other.binding.target.property != link.binding.target.property
            }) {
                let union: HashSet<_> = parents[index].union(&parents[partner]).copied().collect();
                parents[index] = union.clone();
                parents[partner] = union;
            }
        }
        let mut incoming = vec![0_usize; self.links.len()];
        let mut children = vec![Vec::new(); self.links.len()];
        for (index, parents) in parents.iter().enumerate() {
            incoming[index] = parents.len();
            for &parent in parents {
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
        let collection_result = self.context.borrow_mut().synchronize_collections();
        let revision = self.context.borrow().revision();
        if revision != self.validation_revision {
            for link in &self.links {
                if let Some(editor) = value_editor(ui, link.binding.target) {
                    ui.invalidate_async_editor(editor);
                }
            }
            self.validation_revision = revision;
        }
        let order = self.evaluation_order.clone();
        let mut changed = false;
        let mut range_skips = HashSet::new();
        let mut ranges = HashSet::new();
        for index in order {
            let target = self.links[index].binding.target;
            if matches!(
                target.property,
                BindingProperty::LowerValue | BindingProperty::UpperValue
            ) && ranges.insert(target.widget)
            {
                let (range_changed, skips) = self.synchronize_range_sources(ui, target.widget);
                changed |= range_changed;
                range_skips.extend(skips);
                if !range_skips.contains(&index) {
                    let (reverse_changed, skips) =
                        self.synchronize_range_reverse(ui, target.widget, explicit);
                    changed |= reverse_changed;
                    range_skips.extend(skips);
                }
            }
            if range_skips.contains(&index) {
                continue;
            }
            let binding = self.links[index].binding.clone();
            let blocked_collection = binding.mode.reads_source()
                && !(binding.mode == ControlBindingMode::OneTime && self.links[index].initialized)
                && matches!(&binding.source, BindingSource::DataContext(path)
                    if self.context.borrow().has_collection_path(path));
            let result = match &collection_result {
                Err(error) if blocked_collection => Err(error.clone()),
                _ => self.sync_link(ui, index, &binding, explicit),
            };
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

    // Both thumbs are one value transaction. A model set_many must not fail on
    // an intermediate (new lower, old upper) pair or expose half an invalid pair.
    fn synchronize_range_sources(
        &mut self,
        ui: &mut Controls,
        widget: WidgetId,
    ) -> (bool, HashSet<usize>) {
        let mut groups: HashMap<WidgetId, Vec<usize>> = HashMap::new();
        for (index, link) in self.links.iter().enumerate() {
            if link.binding.target.widget == widget
                && matches!(
                    link.binding.target.property,
                    BindingProperty::LowerValue | BindingProperty::UpperValue
                )
                && link.binding.mode.reads_source()
                && !(link.initialized && link.binding.mode == ControlBindingMode::OneTime)
            {
                groups
                    .entry(link.binding.target.widget)
                    .or_default()
                    .push(index);
            }
        }
        let mut changed = false;
        let mut skip = HashSet::new();
        for (id, indexes) in groups {
            if indexes.len() != 2 {
                continue;
            }
            let bindings: Vec<_> = indexes
                .iter()
                .map(|index| self.links[*index].binding.clone())
                .collect();
            let invalidated: Vec<_> = indexes
                .iter()
                .zip(&bindings)
                .map(|(index, binding)| self.source_invalidated(*index, binding))
                .collect();
            if !invalidated.iter().any(|value| *value) {
                continue;
            }
            let revisions = self.source_revisions();
            let mut fallback_errors = HashMap::new();
            let result: Result<bool, BindingError> = (|| {
                let values = indexes
                    .iter()
                    .zip(&bindings)
                    .map(|(index, binding)| {
                        self.read_source(ui, binding).or_else(|error| {
                            let Some(fallback) = &binding.fallback else {
                                return Err(error);
                            };
                            fallback_errors.insert(*index, error.to_string());
                            Ok(fallback.clone())
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if indexes.iter().zip(&values).all(|(index, value)| {
                    self.links[*index].initialized
                        && self.links[*index].source.as_ref() == Some(value)
                }) {
                    for index in &indexes {
                        self.accept_source_revision(*index, revisions);
                    }
                    return Ok(false);
                }
                let first = number(&values[0])?;
                let second = number(&values[1])?;
                let (lower, upper) = if bindings[0].target.property == BindingProperty::LowerValue {
                    (first, second)
                } else {
                    (second, first)
                };
                ui.set_range_slider_values(id, lower, upper)
                    .map_err(control_error)?;
                for (index, value) in indexes.iter().zip(values) {
                    self.links[*index].source = Some(value.clone());
                    self.links[*index].target = Some(value);
                    self.links[*index].initialized = true;
                    self.accept_source_revision(*index, revisions);
                }
                Ok(true)
            })();
            match result {
                Ok(did_change) => {
                    changed |= did_change;
                    for index in &indexes {
                        if let Some(message) = fallback_errors.get(index) {
                            if self.links[*index].error.as_ref() != Some(message) {
                                if self.diagnostics.len() == 1024 {
                                    self.diagnostics.remove(0);
                                }
                                self.diagnostics.push(BindingDiagnostic {
                                    target: self.links[*index].binding.target,
                                    message: message.clone(),
                                });
                            }
                            self.links[*index].error = Some(message.clone());
                        } else {
                            self.links[*index].error = None;
                        }
                    }
                    // A fallback is presentation only. Do not publish the pair
                    // back into a missing or invalid model path on this pass.
                    if !fallback_errors.is_empty() {
                        skip.extend(indexes);
                    }
                }
                Err(error) => {
                    for index in indexes {
                        let message = error.to_string();
                        if self.links[index].error.as_ref() != Some(&message) {
                            if self.diagnostics.len() == 1024 {
                                self.diagnostics.remove(0);
                            }
                            self.diagnostics.push(BindingDiagnostic {
                                target: self.links[index].binding.target,
                                message: message.clone(),
                            });
                            self.links[index].error = Some(message);
                        }
                        skip.insert(index);
                    }
                }
            }
        }
        (changed, skip)
    }

    // A pair of writable model paths is published in one set_many, so a
    // cross-field validator observes the final pair in either direction.
    fn synchronize_range_reverse(
        &mut self,
        ui: &Controls,
        widget: WidgetId,
        explicit: bool,
    ) -> (bool, HashSet<usize>) {
        let indexes: Vec<_> = self
            .links
            .iter()
            .enumerate()
            .filter(|(_, link)| {
                link.binding.target.widget == widget
                    && matches!(
                        link.binding.target.property,
                        BindingProperty::LowerValue | BindingProperty::UpperValue
                    )
                    && link.binding.mode.writes_source()
                    && matches!(link.binding.source, BindingSource::DataContext(_))
            })
            .map(|(index, _)| index)
            .collect();
        if indexes.len() != 2 {
            return (false, HashSet::new());
        }
        let skip = indexes.iter().copied().collect();
        let bindings: Vec<_> = indexes
            .iter()
            .map(|index| self.links[*index].binding.clone())
            .collect();
        if !explicit
            && bindings
                .iter()
                .any(|binding| binding.trigger == BindingUpdateTrigger::Explicit)
        {
            return (false, skip);
        }
        let result: Result<bool, BindingError> = (|| {
            let values = bindings
                .iter()
                .map(|binding| {
                    read_property(ui, binding.target, binding.trigger, &self.items.borrow())
                })
                .collect::<Result<Vec<_>, _>>()?;
            if indexes.iter().zip(&values).all(|(index, value)| {
                self.links[*index].initialized && self.links[*index].target.as_ref() == Some(value)
            }) {
                return Ok(false);
            }
            let updates = bindings
                .iter()
                .zip(&values)
                .map(|(binding, value)| {
                    let BindingSource::DataContext(path) = &binding.source else {
                        unreachable!()
                    };
                    Ok((path.as_str().to_owned(), self.convert_back(binding, value)?))
                })
                .collect::<Result<Vec<_>, BindingError>>()?;
            let changed = self.context.borrow_mut().set_many(updates)?;
            let revisions = self.source_revisions();
            for ((index, binding), value) in indexes.iter().zip(&bindings).zip(values) {
                if binding.mode.reads_source() {
                    self.links[*index].source = Some(self.read_source(ui, binding)?);
                    self.accept_source_revision(*index, revisions);
                }
                self.links[*index].target = Some(value);
                self.links[*index].initialized = true;
                self.links[*index].error = None;
            }
            Ok(changed)
        })();
        match result {
            Ok(changed) => (changed, skip),
            Err(error) => {
                for index in indexes {
                    let message = error.to_string();
                    if self.links[index].error.as_ref() != Some(&message) {
                        if self.diagnostics.len() == 1024 {
                            self.diagnostics.remove(0);
                        }
                        self.diagnostics.push(BindingDiagnostic {
                            target: self.links[index].binding.target,
                            message: message.clone(),
                        });
                        self.links[index].error = Some(message);
                    }
                }
                (false, skip)
            }
        }
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
        if binding.mode.reads_source() && self.source_invalidated(index, binding) {
            // Converters are application callbacks and may update another
            // model value. Never claim to have read a revision emitted later
            // in the callback or during target application.
            let revisions = self.source_revisions();
            let source = match self.read_source(ui, binding) {
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
            if !self.links[index].initialized || self.links[index].source.as_ref() != Some(&source)
            {
                self.apply_target(ui, binding, &source)?;
                self.links[index].source = Some(source);
                self.links[index].target =
                    if binding.target.property == BindingProperty::ItemsSource {
                        // ItemsSource is never reverse-bound. Avoid constructing a
                        // second full object list just to cache an unread target.
                        None
                    } else {
                        Some(read_property(
                            ui,
                            binding.target,
                            binding.trigger,
                            &self.items.borrow(),
                        )?)
                    };
                self.links[index].initialized = true;
                self.accept_source_revision(index, revisions);
                return Ok(true);
            }
            self.accept_source_revision(index, revisions);
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
            if ui.has_async_editor_validator(editor)
                && (explicit || read_trigger == BindingUpdateTrigger::PropertyChanged)
                && document.text() != ui.accepted_value(editor).unwrap_or("")
            {
                let outcome = ui.commit_edit(editor).map_err(control_error)?;
                match outcome {
                    CommitOutcome::Pending => return Ok(false),
                    other => accepted(other)?,
                }
            }
            if read_trigger == BindingUpdateTrigger::PropertyChanged {
                ui.validate_edit(editor, ui.document(editor).unwrap().text())
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
            BindingSource::Multi(_) | BindingSource::Expression { .. } => {
                return Err(invalid(
                    "Multi-source bindings cannot write to their inputs",
                ));
            }
        };
        if explicit && let Some(editor) = value_editor(ui, binding.target) {
            self.applying.set(true);
            let outcome = ui.commit_edit(editor).map_err(control_error);
            self.applying.set(false);
            accepted(outcome?)?;
        }
        if binding.mode.reads_source() {
            let revisions = self.source_revisions();
            self.links[index].source = Some(self.read_source(ui, binding)?);
            self.accept_source_revision(index, revisions);
        }
        self.links[index].target = Some(value);
        self.links[index].initialized = true;
        Ok(changed)
    }

    fn source_invalidated(&mut self, index: usize, binding: &ControlBinding) -> bool {
        let link = &mut self.links[index];
        if !link.initialized
            || link.error.is_some()
            || (binding.converter.is_some()
                && link.converter_revision != self.converters.borrow().revision())
        {
            return true;
        }
        if !binding.source.elements().is_empty() {
            return true;
        }
        let paths = binding.source.paths();
        let context = self.context.borrow();
        let Some(cursor) = link.source_revision else {
            return true;
        };
        if cursor == context.revision() {
            return false;
        }
        let affected = context.changes_since(cursor).is_none_or(|changes| {
            changes
                .iter()
                .any(|change| paths.iter().any(|path| change.affects(path)))
        });
        if !affected {
            // Advance through unrelated changes without cloning source data.
            link.source_revision = Some(context.revision());
        }
        affected
    }

    fn source_revisions(&self) -> (u64, u64) {
        (
            self.context.borrow().revision(),
            self.converters.borrow().revision(),
        )
    }

    fn accept_source_revision(&mut self, index: usize, revisions: (u64, u64)) {
        self.links[index].source_revision = Some(revisions.0);
        self.links[index].converter_revision = revisions.1;
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
            BindingSource::Multi(inputs) => DataValue::List(self.read_inputs(ui, inputs)?),
            BindingSource::Expression { inputs, expression } => {
                expression.evaluate(&self.read_inputs(ui, inputs)?)?
            }
        };
        match &binding.converter {
            Some(name) => self.converters.borrow().convert(name, &value),
            None => Ok(value),
        }
    }

    fn read_inputs(
        &self,
        ui: &Controls,
        inputs: &[BindingInput],
    ) -> Result<Vec<DataValue>, BindingError> {
        inputs
            .iter()
            .map(|input| match input {
                BindingInput::DataContext(path) => {
                    Ok(self.context.borrow().get_path(path)?.clone())
                }
                BindingInput::Element(endpoint) => read_property(
                    ui,
                    *endpoint,
                    BindingUpdateTrigger::PropertyChanged,
                    &self.items.borrow(),
                ),
            })
            .collect()
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
                if endpoint.property == BindingProperty::Value
                    && let Some(validator) = ui.rating_source_validator(endpoint.widget)
                {
                    Box::new(move |value| {
                        validator.check(match value {
                            DataValue::Null => None,
                            DataValue::Number(value) => Some(value),
                            _ => return Err("Rating requires number or null".into()),
                        })
                    })
                } else if endpoint.property == BindingProperty::Value
                    && let Some(mode) = ui.calendar_selection(endpoint.widget)
                {
                    let validator = ui.calendar_source_validator(endpoint.widget).unwrap();
                    Box::new(move |value| {
                        validator
                            .check(parse_calendar_data(mode, &value).map_err(|e| e.to_string())?)
                    })
                } else {
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
                        let _assignment = match &parser {
                            EditorParser::Temporal(parser) => Some(parser.assign_storage(&value)?),
                            _ => None,
                        };
                        let text = match &parser {
                            EditorParser::Mask(settings) => {
                                let raw = string(&value).map_err(|e| e.to_string())?;
                                match &*settings.borrow() {
                                    Some(settings) => {
                                        settings.format(raw).map_err(|e| e.to_string())?
                                    }
                                    None => raw.to_owned(),
                                }
                            }
                            EditorParser::Temporal(parser) => {
                                parser.format_value(&parser.parse_storage(&value)?)?
                            }
                            EditorParser::Decimal(settings) => {
                                let settings = settings.borrow();
                                let decimal = settings.parse_storage(match &value {
                                    DataValue::Null => None,
                                    _ => Some(string(&value).map_err(|e| e.to_string())?),
                                })?;
                                settings.format(decimal).map_err(|e| e.to_string())?
                            }
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
                                value.map_or(String::new(), |value| value.to_display(*format))
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
                                                "Time outside source editor bounds or precision"
                                                    .into(),
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
            }
            BindingSource::Multi(_) | BindingSource::Expression { .. } => return Ok(()),
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
        if binding.target.property == BindingProperty::SelectedKey
            && ui.tree_settings(binding.target.widget).is_some()
        {
            ui.chain_tree_selection_validator(
                binding.target.widget,
                SelectionValidator::new(move |keys| {
                    validate(
                        keys.first()
                            .map(|key| DataValue::String(key.clone()))
                            .unwrap_or(DataValue::Null),
                    )
                }),
            )
            .map_err(control_error)?;
        } else if binding.target.property == BindingProperty::SelectedValue
            && ui.grid_settings(binding.target.widget).is_some()
        {
            ui.chain_grid_selection_validator(
                binding.target.widget,
                SelectionValidator::new(move |keys| {
                    validate(
                        keys.first()
                            .map(|key| DataValue::String(key.clone()))
                            .unwrap_or(DataValue::Null),
                    )
                }),
            )
            .map_err(control_error)?;
        } else if binding.target.property == BindingProperty::FilterText {
            ui.chain_grid_filter_validator(
                binding.target.widget,
                EditorValidator::new(move |text| validate(DataValue::String(text.to_owned()))),
            )
            .map_err(control_error)?;
        } else if matches!(
            binding.target.property,
            BindingProperty::SelectedValue | BindingProperty::SelectedValues
        ) && ui.grid_settings(binding.target.widget).is_none()
        {
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
        } else if binding.target.property == BindingProperty::Value
            && ui.rating_settings(binding.target.widget).is_some()
        {
            ui.chain_rating_validator(
                binding.target.widget,
                crate::RatingValidator::new(move |value| {
                    validate(value.map_or(DataValue::Null, DataValue::Number))
                }),
            )
            .map_err(control_error)?;
        } else if binding.target.property == BindingProperty::Value
            && ui.font_value(binding.target.widget).is_some()
        {
            ui.chain_font_validator(
                binding.target.widget,
                crate::FontValidator::new(move |value| {
                    validate(crate::font_descriptor_to_value(value))
                }),
            )
            .map_err(control_error)?;
        } else if binding.target.property == BindingProperty::Value
            && ui.calendar_selection(binding.target.widget).is_some()
        {
            ui.chain_calendar_validator(
                binding.target.widget,
                CalendarValidator::new(move |value| validate(calendar_data(value))),
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

#[derive(Clone)]
enum EditorParser {
    Mask(Rc<RefCell<Option<crate::MaskSettings>>>),
    Temporal(TemporalValueParser),
    Decimal(Rc<RefCell<NumericEditSettings>>),
    Text,
    Number,
    Date(DateFormat),
    Time,
}

impl EditorParser {
    fn parse(&self, text: &str) -> Result<DataValue, BindingError> {
        match self {
            Self::Mask(settings) => match &*settings.borrow() {
                Some(settings) => {
                    let value = settings.parse_display(text).map_err(|e| invalid(&e))?;
                    if !value.complete {
                        return Err(invalid("Complete all required mask slots"));
                    }
                    Ok(DataValue::String(value.storage))
                }
                None => Ok(text.into()),
            },
            Self::Temporal(parser) => parser
                .parse(text)
                .map(temporal_data)
                .map_err(|e| invalid(&e)),
            Self::Decimal(settings) => settings
                .borrow()
                .parse(text)
                .map(|v| v.map_or(DataValue::Null, |v| DataValue::String(v.to_string())))
                .map_err(|e| invalid(&e)),
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
                    DateValue::parse_display(text, *format)
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

fn calendar_data(selection: CalendarSelection) -> DataValue {
    temporal_data(match selection {
        CalendarSelection::Single(value) => EditorValue::Date(value),
        CalendarSelection::Range(value) => EditorValue::DateRange(value),
    })
}
fn parse_calendar_data(
    mode: CalendarSelection,
    value: &DataValue,
) -> Result<CalendarSelection, BindingError> {
    Ok(match mode {
        CalendarSelection::Single(_) => CalendarSelection::Single(if *value == DataValue::Null {
            None
        } else {
            Some(DateValue::parse_iso(string(value)?).map_err(control_error)?)
        }),
        CalendarSelection::Range(_) => CalendarSelection::Range(if *value == DataValue::Null {
            None
        } else {
            let DataValue::Object(fields) = value else {
                return Err(invalid(
                    "Calendar range requires object {start,end} or null",
                ));
            };
            if fields.len() != 2 {
                return Err(invalid("Calendar range requires exactly start and end"));
            }
            let field = |name| {
                fields
                    .get(name)
                    .ok_or_else(|| invalid("Range endpoint missing"))
                    .and_then(string)
                    .and_then(|s| DateValue::parse_iso(s).map_err(control_error))
            };
            Some(DateRangeValue::new(field("start")?, field("end")?).map_err(control_error)?)
        }),
    })
}
fn temporal_data(value: EditorValue) -> DataValue {
    match value {
        EditorValue::Date(value) => value.map_or(DataValue::Null, |v| v.to_iso().into()),
        EditorValue::Time(value) => value.map_or(DataValue::Null, |v| v.to_iso().into()),
        EditorValue::DateTime(value) => value.map_or(DataValue::Null, |v| v.to_iso().into()),
        EditorValue::DateRange(value) => value.map_or(DataValue::Null, |v| {
            DataValue::Object(
                [
                    ("start".into(), DataValue::String(v.start.to_iso())),
                    ("end".into(), DataValue::String(v.end.to_iso())),
                ]
                .into_iter()
                .collect(),
            )
        }),
        EditorValue::Number(value) => DataValue::Number(value),
        EditorValue::Decimal(value) => value.map_or(DataValue::Null, |v| v.to_string().into()),
    }
}

fn editor_parser(ui: &Controls, endpoint: BindingEndpoint) -> EditorParser {
    if endpoint.property != BindingProperty::Value {
        return EditorParser::Text;
    }
    if let Some(settings) = ui.mask_binding_settings(endpoint.widget) {
        EditorParser::Mask(settings)
    } else if let Some(settings) = ui.numeric_binding_settings(endpoint.widget) {
        EditorParser::Decimal(settings)
    } else if let Some(parser) = ui.temporal_value_parser(endpoint.widget) {
        EditorParser::Temporal(parser)
    } else if ui.spin_settings(endpoint.widget).is_some() {
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
    if ui
        .combo_selection_settings(id)
        .is_some_and(|settings| settings.mode == crate::SelectionMode::Multiple)
        && matches!(
            endpoint.property,
            BindingProperty::Text | BindingProperty::Value | BindingProperty::SelectedValue
        )
    {
        return Err(invalid(
            "Multiple combo selection binds SelectedValues; its text is a search draft",
        ));
    }
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
        BindingProperty::BadgeCount => match &ui
            .badge(id)
            .ok_or_else(|| invalid("Badge.Count requires an attachment"))?
            .content
        {
            crate::BadgeContent::Count(value) => Ok(DataValue::String(value.to_string())),
            _ => Err(invalid("Badge.Count requires count content")),
        },
        BindingProperty::BadgeText => match &ui
            .badge(id)
            .ok_or_else(|| invalid("Badge.Text requires an attachment"))?
            .content
        {
            crate::BadgeContent::Text(value) => Ok(DataValue::String(value.clone())),
            _ => Err(invalid("Badge.Text requires text content")),
        },
        BindingProperty::BadgeVisibility => ui
            .badge(id)
            .map(|settings| DataValue::Bool(settings.visible))
            .ok_or_else(|| invalid("Badge.Visibility requires an attachment")),

        BindingProperty::Query => ui
            .search_query(id)
            .map(|query| DataValue::String(query.into()))
            .ok_or_else(|| invalid("Query requires SearchBox")),
        BindingProperty::FilterText => ui
            .grid_filter(id)
            .map(DataValue::from)
            .ok_or_else(|| invalid("FilterText requires GridControl")),
        BindingProperty::SelectedIndex => ui
            .selected_tab(id)
            .map(|index| DataValue::Number(index as f64))
            .ok_or_else(|| invalid("SelectedIndex requires a tabbed group")),
        BindingProperty::Header => ui
            .group_header(id)
            .map(DataValue::from)
            .ok_or_else(|| invalid("Header requires a group or expander")),
        BindingProperty::IsExpanded => ui
            .expander_expanded(id)
            .map(DataValue::Bool)
            .ok_or_else(|| invalid("IsExpanded requires an expander")),
        BindingProperty::Text => ui
            .label_text(id)
            .map(DataValue::from)
            .ok_or_else(|| invalid("Text requires an editor or caption")),
        BindingProperty::LowerValue | BindingProperty::UpperValue => ui
            .range_slider_values(id)
            .map(|(lower, upper)| {
                DataValue::Number(if endpoint.property == BindingProperty::LowerValue {
                    lower
                } else {
                    upper
                })
            })
            .ok_or_else(|| invalid("Range value requires RangeSlider")),
        BindingProperty::IsIndeterminate => ui
            .progress_indeterminate(id)
            .map(DataValue::Bool)
            .ok_or_else(|| invalid("IsIndeterminate requires ProgressBar")),
        BindingProperty::SelectedKey => {
            if let Some(settings) = ui.tree_settings(id) {
                if settings.selection_mode != crate::SelectionMode::Single {
                    return Err(invalid("Tree SelectedKey requires Single selection"));
                }
                Ok(ui
                    .tree_selected_keys(id)
                    .unwrap()
                    .first()
                    .map(|key| DataValue::String(key.0.to_string()))
                    .unwrap_or(DataValue::Null))
            } else if ui.radio_items(id).is_some() {
                Ok(ui
                    .radio_selected_key(id)
                    .map(DataValue::from)
                    .unwrap_or(DataValue::Null))
            } else {
                Err(invalid("SelectedKey requires RadioGroup or tree"))
            }
        }
        BindingProperty::Value if ui.rating_settings(id).is_some() => Ok(ui
            .rating_value(id)
            .unwrap()
            .map_or(DataValue::Null, DataValue::Number)),
        BindingProperty::Value if ui.font_value(id).is_some() => {
            Ok(crate::font_descriptor_to_value(ui.font_value(id).unwrap()))
        }
        BindingProperty::Value if ui.calendar_selection(id).is_some() => {
            Ok(calendar_data(ui.calendar_selection(id).unwrap()))
        }
        BindingProperty::Value => ui
            .slider_value(id)
            .or_else(|| ui.progress_value(id))
            .or_else(|| ui.progress_settings(id).map(|s| s.value))
            .or_else(|| ui.zoom_value(id))
            .map(DataValue::Number)
            .ok_or_else(|| invalid("Value requires an editor, ProgressBar or ZoomControl")),
        BindingProperty::SelectedValue | BindingProperty::SelectedValues => {
            if endpoint.property == BindingProperty::SelectedValue && ui.grid_settings(id).is_some()
            {
                return Ok(ui
                    .grid_selected_key(id)
                    .map(|key| DataValue::String(key.to_string()))
                    .unwrap_or(DataValue::Null));
            }
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
            .map(|v| v.to_vec())
            .or_else(|| ui.radio_items(id))
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
            .check_state(id)
            .map(|state| match state {
                crate::CheckState::Indeterminate => DataValue::Null,
                crate::CheckState::Checked => DataValue::Bool(true),
                crate::CheckState::Unchecked => DataValue::Bool(false),
            })
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
            .or_else(|| {
                ui.search_parts(id)
                    .and_then(|parts| ui.is_read_only(parts.editor))
            })
            .or_else(|| ui.is_read_only(id))
            .or_else(|| ui.selection_settings(id).map(|s| s.read_only))
            .or_else(|| ui.grid_settings(id).map(|s| s.is_read_only))
            .or_else(|| ui.tree_settings(id).map(|s| s.read_only))
            .or_else(|| ui.rating_settings(id).map(|s| s.read_only))
            .or_else(|| ui.font_picker_settings(id).map(|s| s.read_only))
            .or_else(|| ui.calendar_settings(id).map(|s| s.is_read_only))
            .map(DataValue::Bool)
            .ok_or_else(|| invalid("IsReadOnly requires an editor or list")),
        BindingProperty::PlaceholderText => {
            let editor = ui
                .composite_editor(id)
                .or_else(|| ui.search_parts(id).map(|parts| parts.editor))
                .unwrap_or(id);
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
    if ui
        .combo_selection_settings(id)
        .is_some_and(|settings| settings.mode == crate::SelectionMode::Multiple)
        && matches!(
            endpoint.property,
            BindingProperty::Text | BindingProperty::Value | BindingProperty::SelectedValue
        )
    {
        return Err(invalid(
            "Multiple combo selection binds SelectedValues; its text is a search draft",
        ));
    }
    value.validate()?;
    if let Some(editor) = value_editor(ui, endpoint) {
        let outcome = match editor_parser(ui, endpoint) {
            EditorParser::Mask(settings) => {
                let display = match &*settings.borrow() {
                    Some(settings) => settings.format(string(value)?).map_err(control_error)?,
                    None => string(value)?.to_owned(),
                };
                ui.set_editor_value(editor, &display)
            }
            EditorParser::Temporal(parser) => {
                match parser.parse_storage(value).map_err(|e| invalid(&e))? {
                    EditorValue::Date(value) => ui.set_date_value(id, value),
                    EditorValue::Time(value) => ui.set_time_value(id, value),
                    EditorValue::DateTime(value) => ui.set_date_time_value(id, value),
                    EditorValue::DateRange(value) => ui.set_date_range_value(id, value),
                    _ => return Err(invalid("Expected a temporal value")),
                }
            }
            EditorParser::Decimal(settings) => ui.set_numeric_value(
                id,
                settings
                    .borrow()
                    .parse_storage(match value {
                        DataValue::Null => None,
                        _ => Some(string(value)?),
                    })
                    .map_err(|e| invalid(&e))?,
            ),
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
        BindingProperty::BadgeCount
        | BindingProperty::BadgeText
        | BindingProperty::BadgeVisibility => {
            let mut settings = ui
                .badge(id)
                .cloned()
                .ok_or_else(|| invalid("Badge bindings require an attachment"))?;
            match endpoint.property {
                BindingProperty::BadgeCount => {
                    let DataValue::String(text) = value else {
                        return Err(invalid("Badge.Count requires a canonical u64 string"));
                    };
                    let count = text
                        .parse::<u64>()
                        .map_err(|_| invalid("Badge.Count requires a canonical u64 string"))?;
                    if count.to_string() != *text {
                        return Err(invalid("Badge.Count requires a canonical u64 string"));
                    }
                    if !matches!(settings.content, crate::BadgeContent::Count(_)) {
                        return Err(invalid("Badge.Count requires count content"));
                    }
                    settings.content = crate::BadgeContent::Count(count);
                }
                BindingProperty::BadgeText => {
                    let DataValue::String(text) = value else {
                        return Err(invalid("Badge.Text requires a string"));
                    };
                    if !matches!(settings.content, crate::BadgeContent::Text(_)) {
                        return Err(invalid("Badge.Text requires text content"));
                    }
                    settings.content = crate::BadgeContent::Text(text.clone());
                }
                _ => {
                    settings.visible = match value {
                        DataValue::Bool(value) => *value,
                        DataValue::String(value) if value == "Visible" => true,
                        DataValue::String(value) if value == "Hidden" || value == "Collapsed" => {
                            false
                        }
                        _ => {
                            return Err(invalid(
                                "Badge.Visibility requires bool or Visible/Hidden/Collapsed",
                            ));
                        }
                    };
                }
            }
            ui.set_badge(id, Some(settings)).map_err(control_error)
        }

        BindingProperty::Query => ui
            .set_search_query(id, string(value)?)
            .map_err(control_error),
        BindingProperty::FilterText => ui
            .set_grid_filter(id, string(value)?)
            .map_err(control_error),
        BindingProperty::SelectedIndex => {
            let index = number(value)?;
            if !(0.0..=63.0).contains(&index) || index.fract() != 0.0 {
                return Err(invalid("SelectedIndex must be an integer in 0..=63"));
            }
            ui.set_selected_tab(id, index as usize)
                .map_err(control_error)
        }
        BindingProperty::Header => ui
            .set_group_header(id, string(value)?)
            .map_err(control_error),
        BindingProperty::IsExpanded => ui
            .set_expander_expanded(id, boolean(value)?)
            .map_err(control_error),
        BindingProperty::Text => ui.set_label(id, string(value)?).map_err(control_error),
        BindingProperty::LowerValue | BindingProperty::UpperValue => {
            let (lower, upper) = ui
                .range_slider_values(id)
                .ok_or_else(|| invalid("Range value requires RangeSlider"))?;
            ui.set_range_slider_values(
                id,
                if endpoint.property == BindingProperty::LowerValue {
                    number(value)?
                } else {
                    lower
                },
                if endpoint.property == BindingProperty::UpperValue {
                    number(value)?
                } else {
                    upper
                },
            )
            .map_err(control_error)
        }
        BindingProperty::IsIndeterminate => ui
            .set_progress_indeterminate(id, boolean(value)?)
            .map_err(control_error),
        BindingProperty::SelectedKey if ui.tree_settings(id).is_some() => {
            if ui.tree_settings(id).unwrap().selection_mode != crate::SelectionMode::Single {
                return Err(invalid("Tree SelectedKey requires Single selection"));
            }
            let key = match value {
                DataValue::Null => None,
                DataValue::String(text) => {
                    let key = text
                        .parse::<u64>()
                        .map_err(|_| invalid("Tree key requires canonical u64 text"))?;
                    if key.to_string() != *text {
                        return Err(invalid("Tree key requires canonical u64 text"));
                    }
                    Some(rust_desktop_ui_treegrid::RowKey(key))
                }
                _ => {
                    return Err(invalid(
                        "Tree SelectedKey requires canonical u64 text or null",
                    ));
                }
            };
            ui.set_tree_selected_key(id, key).map_err(control_error)
        }
        BindingProperty::SelectedKey => ui
            .set_radio_selected_key(
                id,
                if *value == DataValue::Null {
                    None
                } else {
                    Some(string(value)?)
                },
            )
            .map_err(control_error),
        BindingProperty::Value => {
            if ui.rating_settings(id).is_some() {
                ui.set_rating_value(
                    id,
                    if *value == DataValue::Null {
                        None
                    } else {
                        Some(number(value)?)
                    },
                )
                .map_err(control_error)
            } else if ui.font_value(id).is_some() {
                ui.set_font_value(
                    id,
                    crate::font_descriptor_from_value(value).map_err(|e| invalid(&e))?,
                )
                .map(|_| ())
                .map_err(control_error)
            } else if let Some(selection) = ui.calendar_selection(id) {
                ui.set_calendar_selection(id, parse_calendar_data(selection, value)?)
                    .map_err(control_error)
            } else if ui.slider_value(id).is_some() {
                ui.set_slider_value(id, number(value)?)
                    .map_err(control_error)
            } else if ui.progress_settings(id).is_some() {
                ui.set_progress_value(id, number(value)?)
                    .map_err(control_error)
            } else if ui.zoom_settings(id).is_some() {
                ui.set_zoom_value(id, number(value)?).map_err(control_error)
            } else {
                Err(invalid(
                    "Value requires an editor, ProgressBar or ZoomControl",
                ))
            }
        }
        BindingProperty::SelectedValue | BindingProperty::SelectedValues => {
            if endpoint.property == BindingProperty::SelectedValue && ui.grid_settings(id).is_some()
            {
                let key = match value {
                    DataValue::Null => None,
                    DataValue::String(text) => Some(text.parse::<i64>().map_err(|_| {
                        invalid("Grid SelectedValue requires a signed integer string")
                    })?),
                    _ => {
                        return Err(invalid(
                            "Grid SelectedValue requires a signed integer string or null",
                        ));
                    }
                };
                return ui.set_grid_selected_key(id, key).map_err(control_error);
            }
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
            let result = if ui.radio_items(id).is_some() {
                ui.set_radio_items(id, rows)
            } else {
                ui.set_selection_items(id, rows)
            };
            if let Err(error) = result {
                if let Some(previous) = previous {
                    items.borrow_mut().insert(id, previous);
                } else {
                    items.borrow_mut().remove(&id);
                }
                return Err(control_error(error));
            }
            Ok(())
        }
        BindingProperty::IsChecked => {
            if *value == DataValue::Null {
                if !ui.is_three_state(id) {
                    return Err(invalid("Null IsChecked requires a three-state control"));
                }
                ui.set_check_state(id, crate::CheckState::Indeterminate)
                    .map_err(control_error)
            } else {
                ui.set_check_state(
                    id,
                    if boolean(value)? {
                        crate::CheckState::Checked
                    } else {
                        crate::CheckState::Unchecked
                    },
                )
                .map_err(control_error)
            }
        }
        BindingProperty::IsEnabled => ui.set_enabled(id, boolean(value)?).map_err(control_error),
        BindingProperty::IsReadOnly => {
            let value = boolean(value)?;
            if ui.rating_settings(id).is_some() {
                ui.set_rating_read_only(id, value).map_err(control_error)
            } else if ui.font_value(id).is_some() {
                ui.set_font_read_only(id, value).map_err(control_error)
            } else if let Some(settings) = ui.tree_settings(id) {
                ui.set_tree_interaction(id, settings.allow_node_drag, value)
                    .map_err(control_error)
            } else if let Some(parts) = ui.search_parts(id) {
                ui.set_read_only(parts.editor, value).map_err(control_error)
            } else if let Some(settings) = ui.calendar_settings(id) {
                let mut settings = settings.clone();
                settings.is_read_only = value;
                ui.set_calendar_settings(id, settings)
                    .map_err(control_error)
            } else if ui.grid_settings(id).is_some() {
                ui.set_grid_read_only(id, value).map_err(control_error)
            } else if ui.composite_editor(id).is_some() {
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
            .set_placeholder(
                ui.composite_editor(id)
                    .or_else(|| ui.search_parts(id).map(|parts| parts.editor))
                    .unwrap_or(id),
                string(value)?,
            )
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
                    DataValue::Null
                        if endpoint.property == BindingProperty::Height
                            && ui.grid_settings(id).is_some() =>
                    {
                        Length::Px(320.0)
                    }
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
        CommitOutcome::Pending => Err(invalid("Editor validation is still pending")),
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
        BindingProperty::Value | BindingProperty::Header => BindingProperty::Text,
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

#[cfg(test)]
#[path = "observable_binding_tests.rs"]
mod observable_tests;

#[cfg(test)]
#[path = "multi_binding_tests.rs"]
mod multi_tests;
