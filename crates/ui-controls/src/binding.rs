//! Observable application data for retained form bindings.
//!
//! A context owns a bounded value tree. Updates are synchronous and atomic;
//! subscribers poll revisions on the UI thread instead of running a hidden
//! event loop. Widgets, update triggers, and subscription lifetimes belong to
//! the form adapter. Dates and times can be represented by ISO strings.

use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt,
    rc::Rc,
};

#[path = "binding_collection.rs"]
mod collection;
pub use collection::CollectionWakeSubscription;
use collection::RegisteredCollection;

pub const MAX_BINDING_DEPTH: usize = 64;
pub const MAX_BINDING_NODES: usize = 100_000;
pub const MAX_BINDING_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BINDING_PATH_BYTES: usize = 4096;
pub const MAX_BINDING_HISTORY: usize = 256;
pub const MAX_BINDING_UPDATES: usize = 1024;

/// A backend-independent value. Contexts and converters reject non-finite
/// numbers, oversized trees, and excessive nesting before publishing values.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum DataValue {
    #[default]
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<DataValue>),
    Object(BTreeMap<String, DataValue>),
}

impl DataValue {
    pub fn object<K: Into<String>>(fields: impl IntoIterator<Item = (K, Self)>) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::List(_) => "list",
            Self::Object(_) => "object",
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub const fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Self>> {
        match self {
            Self::Object(values) => Some(values),
            _ => None,
        }
    }

    /// Check representation limits independently of a context or schema.
    pub fn validate(&self) -> Result<(), BindingError> {
        validate_value(self)
    }
}

impl From<&str> for DataValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for DataValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<bool> for DataValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for DataValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

/// Dot-separated field names and zero-based list indices. Empty text or `.`
/// denotes the root. Field names begin with a letter or `_`, followed by
/// letters, digits or `_`. Examples: `Customer.Name`, `Rows[0].Amount`.
/// Whitespace, method calls, expressions and implicit field creation are not
/// supported. Parsing canonicalizes indices such as `[01]` to `[1]`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingPath {
    text: String,
    segments: Vec<BindingPathSegment>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingPathSegment {
    Field(String),
    Index(usize),
}

impl BindingPath {
    pub fn root() -> Self {
        Self {
            text: String::new(),
            segments: Vec::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self, BindingError> {
        if text.len() > MAX_BINDING_PATH_BYTES {
            return Err(BindingError::InvalidPath(
                "Path exceeds 4096 UTF-8 bytes".into(),
            ));
        }
        if text.is_empty() || text == "." {
            return Ok(Self::root());
        }
        let mut remaining = text;
        let mut segments = Vec::new();
        loop {
            let first = remaining.chars().next().ok_or_else(|| invalid_path(text))?;
            if first != '[' {
                if first != '_' && !first.is_alphabetic() {
                    return Err(invalid_path(text));
                }
                let end = remaining
                    .char_indices()
                    .find(|(_, ch)| *ch != '_' && !ch.is_alphanumeric())
                    .map_or(remaining.len(), |(index, _)| index);
                segments.push(BindingPathSegment::Field(remaining[..end].to_owned()));
                remaining = &remaining[end..];
            } else if !segments.is_empty() {
                return Err(invalid_path(text));
            }
            while remaining.starts_with('[') {
                let end = remaining.find(']').ok_or_else(|| invalid_path(text))?;
                let index = &remaining[1..end];
                if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(invalid_path(text));
                }
                segments.push(BindingPathSegment::Index(
                    index.parse().map_err(|_| invalid_path(text))?,
                ));
                remaining = &remaining[end + 1..];
                if segments.len() > MAX_BINDING_DEPTH {
                    return Err(invalid_path(text));
                }
            }
            if segments.len() > MAX_BINDING_DEPTH {
                return Err(invalid_path(text));
            }
            if remaining.is_empty() {
                break;
            }
            remaining = remaining
                .strip_prefix('.')
                .ok_or_else(|| invalid_path(text))?;
            if remaining.is_empty() || remaining.starts_with('[') {
                return Err(invalid_path(text));
            }
        }
        let mut canonical = String::new();
        for segment in &segments {
            match segment {
                BindingPathSegment::Field(field) => {
                    if !canonical.is_empty() {
                        canonical.push('.');
                    }
                    canonical.push_str(field);
                }
                BindingPathSegment::Index(index) => canonical.push_str(&format!("[{index}]")),
            }
        }
        Ok(Self {
            text: canonical,
            segments,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn segments(&self) -> &[BindingPathSegment] {
        &self.segments
    }

    /// True for this path or any descendant, with structural rather than text
    /// prefix matching (`A` does not contain `Amount`).
    pub fn contains(&self, other: &Self) -> bool {
        other.segments.starts_with(&self.segments)
    }

    /// Either a parent replacement or a child mutation invalidates a binding.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }
}

impl fmt::Display for BindingPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.text.is_empty() {
            "."
        } else {
            &self.text
        })
    }
}

impl AsRef<str> for BindingPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::str::FromStr for BindingPath {
    type Err = BindingError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Optional structural model contract. Object schemas require exactly their
/// declared fields. Nullable accepts either `Null` or its inner schema; Any
/// still enforces finite numbers and the context's representation limits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ValueSchema {
    #[default]
    Any,
    Null,
    Bool,
    Number,
    String,
    List(Box<Self>),
    Object(BTreeMap<String, Self>),
    Nullable(Box<Self>),
}

impl ValueSchema {
    pub fn object<K: Into<String>>(fields: impl IntoIterator<Item = (K, Self)>) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    pub fn validate(&self, value: &DataValue) -> Result<(), BindingError> {
        validate_value(value)?;
        validate_schema(self)?;
        check_schema(self, value, ".")
    }
}

/// One atomic publication. Values are read from the context, avoiding copies
/// of entire collections in the bounded change history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingChange {
    pub revision: u64,
    /// Minimal changed paths: a changed ancestor subsumes its descendants.
    pub paths: Vec<BindingPath>,
}

impl BindingChange {
    pub fn affects(&self, path: &BindingPath) -> bool {
        self.paths.iter().any(|changed| changed.overlaps(path))
    }
}

type ModelValidator = dyn Fn(&DataValue) -> Result<(), String>;

/// Synchronous whole-model validation, including cross-field constraints.
/// Callbacks should be fast and free of side effects. Clones share a callback;
/// neither thread dispatch nor `Send`/`Sync` is imposed on the application.
#[derive(Clone)]
pub struct BindingValidator(Rc<ModelValidator>);

impl BindingValidator {
    pub fn new(validate: impl Fn(&DataValue) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(validate))
    }
}

impl fmt::Debug for BindingValidator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BindingValidator(..)")
    }
}

/// Application-owned observable object tree. A form holds its own revision
/// cursor, so multiple forms can observe one context without consuming each
/// other's notifications. `set_many` validates the final model, allowing
/// coordinated changes to fields with cross-field constraints.
#[derive(Clone, Debug)]
pub struct BindingContext {
    root: DataValue,
    schema: ValueSchema,
    validator: Option<BindingValidator>,
    field_validators: BTreeMap<BindingPath, BindingValidator>,
    revision: u64,
    history_floor: u64,
    history: VecDeque<BindingChange>,
    collections: BTreeMap<BindingPath, RegisteredCollection>,
    collection_wakes: Rc<collection::CollectionWakeHub>,
}

impl Default for BindingContext {
    fn default() -> Self {
        Self {
            root: DataValue::Object(BTreeMap::new()),
            schema: ValueSchema::Any,
            validator: None,
            field_validators: BTreeMap::new(),
            revision: 0,
            history_floor: 0,
            history: VecDeque::new(),
            collections: BTreeMap::new(),
            collection_wakes: Rc::default(),
        }
    }
}

impl BindingContext {
    pub fn new(root: DataValue) -> Result<Self, BindingError> {
        validate_root(&root)?;
        Ok(Self {
            root,
            ..Self::default()
        })
    }

    pub fn root(&self) -> &DataValue {
        &self.root
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, path: &str) -> Result<&DataValue, BindingError> {
        self.get_path(&BindingPath::parse(path)?)
    }

    pub fn get_path(&self, path: &BindingPath) -> Result<&DataValue, BindingError> {
        lookup(&self.root, path).ok_or_else(|| BindingError::MissingPath(path.to_string()))
    }

    pub fn schema(&self) -> &ValueSchema {
        &self.schema
    }

    /// Replacing a contract validates current data first. Contracts do not
    /// mutate data or create a new data revision.
    pub fn set_schema(&mut self, schema: ValueSchema) -> Result<(), BindingError> {
        schema.validate(&self.root)?;
        self.schema = schema;
        Ok(())
    }

    /// Installing a new validator first checks the current model. Failed
    /// registration leaves both the previous policy and data unchanged.
    pub fn set_validator(
        &mut self,
        validator: Option<BindingValidator>,
    ) -> Result<(), BindingError> {
        if let Some(validator) = &validator {
            (validator.0)(&self.root)
                .map_err(|message| BindingError::Validation(diagnostic(message)))?;
        }
        self.validator = validator;
        Ok(())
    }

    /// Attach a validator to an existing field. Its callback receives that
    /// field's value. Removal of a required path is rejected while its
    /// validator is registered; detach the validator before replacing that
    /// portion of a dynamic schema.
    pub fn set_field_validator(
        &mut self,
        path: &str,
        validator: Option<BindingValidator>,
    ) -> Result<(), BindingError> {
        let path = BindingPath::parse(path)?;
        if let Some(validator) = validator {
            (validator.0)(self.get_path(&path)?)
                .map_err(|message| BindingError::Validation(diagnostic(message)))?;
            self.field_validators.insert(path, validator);
        } else {
            self.field_validators.remove(&path);
        }
        Ok(())
    }

    pub fn set(&mut self, path: &str, value: DataValue) -> Result<bool, BindingError> {
        self.set_many([(path, value)])
    }

    pub fn set_path(&mut self, path: &BindingPath, value: DataValue) -> Result<bool, BindingError> {
        self.set(path.as_str(), value)
    }

    /// Apply in iterator order and publish once. All paths must already exist
    /// when their update is applied. Replace a parent object or list to add or
    /// remove members. Duplicate and overlapping paths are supported; the
    /// final value wins. A rejected batch has no partial effect. A batch whose
    /// final value equals the original has no revision or notification.
    pub fn set_many<I, P>(&mut self, updates: I) -> Result<bool, BindingError>
    where
        I: IntoIterator<Item = (P, DataValue)>,
        P: AsRef<str>,
    {
        self.set_many_impl(updates, false)
    }

    fn set_many_impl<I, P>(
        &mut self,
        updates: I,
        collection_publication: bool,
    ) -> Result<bool, BindingError>
    where
        I: IntoIterator<Item = (P, DataValue)>,
        P: AsRef<str>,
    {
        let mut candidate = self.root.clone();
        let mut paths = Vec::new();
        for (path, value) in updates {
            if paths.len() == MAX_BINDING_UPDATES {
                return Err(BindingError::InvalidValue(
                    "Too many updates in one batch".into(),
                ));
            }
            let path = BindingPath::parse(path.as_ref())?;
            if !collection_publication {
                self.validate_collection_write(&path)?;
            }
            value.validate()?;
            let slot = lookup_mut(&mut candidate, &path)
                .ok_or_else(|| BindingError::MissingPath(path.to_string()))?;
            *slot = value;
            paths.push(path);
        }
        self.validate_candidate(&candidate)?;
        if candidate == self.root {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(BindingError::RevisionOverflow)?;
        paths.sort_by(|first, second| {
            first
                .segments
                .len()
                .cmp(&second.segments.len())
                .then(first.cmp(second))
        });
        paths.dedup();
        let mut minimal: Vec<BindingPath> = Vec::new();
        for path in paths {
            if !minimal.iter().any(|parent| parent.contains(&path))
                && lookup(&self.root, &path) != lookup(&candidate, &path)
            {
                minimal.push(path);
            }
        }
        self.root = candidate;
        self.revision = revision;
        self.history.push_back(BindingChange {
            revision,
            paths: minimal,
        });
        if self.history.len() > MAX_BINDING_HISTORY
            && let Some(change) = self.history.pop_front()
        {
            self.history_floor = change.revision;
        }
        Ok(true)
    }

    /// Validate a proposed publication without changing data or revisions.
    pub fn validate_candidate(&self, value: &DataValue) -> Result<(), BindingError> {
        validate_root(value)?;
        check_schema(&self.schema, value, ".")?;
        for (path, validator) in &self.field_validators {
            let field =
                lookup(value, path).ok_or_else(|| BindingError::MissingPath(path.to_string()))?;
            (validator.0)(field)
                .map_err(|message| BindingError::Validation(diagnostic(message)))?;
        }
        if let Some(validator) = &self.validator {
            (validator.0)(value)
                .map_err(|message| BindingError::Validation(diagnostic(message)))?;
        }
        Ok(())
    }

    /// Check a proposed field update, including the final model's schema and
    /// validator, without mutating data or emitting a notification.
    pub fn validate_set(&self, path: &str, value: &DataValue) -> Result<(), BindingError> {
        self.validate_many([(path, value)])
    }

    /// Preflight a batch with the same sequential path semantics as set_many.
    /// Callbacks observe only the completed candidate, never intermediate data.
    pub fn validate_many<'a, I, P>(&self, updates: I) -> Result<(), BindingError>
    where
        I: IntoIterator<Item = (P, &'a DataValue)>,
        P: AsRef<str>,
    {
        let mut candidate = self.root.clone();
        for (index, (path, value)) in updates.into_iter().enumerate() {
            if index == MAX_BINDING_UPDATES {
                return Err(BindingError::InvalidValue(
                    "Too many updates in one batch".into(),
                ));
            }
            let path = BindingPath::parse(path.as_ref())?;
            self.validate_collection_write(&path)?;
            value.validate()?;
            let slot = lookup_mut(&mut candidate, &path)
                .ok_or_else(|| BindingError::MissingPath(path.to_string()))?;
            *slot = value.clone();
        }
        self.validate_candidate(&candidate)
    }

    /// Changes strictly after a subscriber's revision. `None` means its
    /// cursor is too old (or ahead of this context): refresh all bindings.
    /// Collection changes invalidate their item bindings and the collection
    /// binding itself. At most 256 atomic publications are retained.
    pub fn changes_since(&self, revision: u64) -> Option<Vec<BindingChange>> {
        if revision < self.history_floor || revision > self.revision {
            return None;
        }
        Some(
            self.history
                .iter()
                .filter(|change| change.revision > revision)
                .cloned()
                .collect(),
        )
    }

    /// Consume history for a single-owner integration. Other subscribers with
    /// older cursors must subsequently refresh all bindings. Multi-form hosts
    /// should prefer `changes_since`.
    pub fn drain_changes(&mut self) -> Vec<BindingChange> {
        self.history_floor = self.revision;
        self.history.drain(..).collect()
    }
}

type ConvertValue = dyn Fn(&DataValue) -> Result<DataValue, String>;

/// Explicit forward conversion and optional reverse conversion. A missing
/// reverse conversion is an error for a source update, never an implicit
/// identity conversion. All converter results satisfy `DataValue` limits.
#[derive(Clone)]
pub struct BindingConverter {
    forward: Rc<ConvertValue>,
    reverse: Option<Rc<ConvertValue>>,
}

impl BindingConverter {
    pub fn new(convert: impl Fn(&DataValue) -> Result<DataValue, String> + 'static) -> Self {
        Self {
            forward: Rc::new(convert),
            reverse: None,
        }
    }

    pub fn with_reverse(
        mut self,
        convert: impl Fn(&DataValue) -> Result<DataValue, String> + 'static,
    ) -> Self {
        self.reverse = Some(Rc::new(convert));
        self
    }

    pub fn supports_reverse(&self) -> bool {
        self.reverse.is_some()
    }
}

impl fmt::Debug for BindingConverter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BindingConverter")
            .field("supports_reverse", &self.supports_reverse())
            .finish()
    }
}

/// Application-provided conversions are named independently of XAML and
/// widgets. Registration replaces an existing name; invalid names do not
/// change the registry.
#[derive(Clone, Debug, Default)]
pub struct BindingConverterRegistry {
    converters: BTreeMap<String, BindingConverter>,
    revision: u64,
}

impl BindingConverterRegistry {
    pub fn register(
        &mut self,
        name: impl Into<String>,
        converter: BindingConverter,
    ) -> Result<(), BindingError> {
        let name = name.into();
        if name.is_empty()
            || name.len() > 256
            || name.chars().any(char::is_whitespace)
            || name.chars().any(char::is_control)
        {
            return Err(BindingError::InvalidConverterName);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(BindingError::RevisionOverflow)?;
        self.converters.insert(name, converter);
        self.revision = revision;
        Ok(())
    }

    /// Changes when a converter is registered or replaced, invalidating hosts.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn contains(&self, name: &str) -> bool {
        self.converters.contains_key(name)
    }

    pub fn supports_reverse(&self, name: &str) -> bool {
        self.converters
            .get(name)
            .is_some_and(BindingConverter::supports_reverse)
    }

    pub fn convert(&self, name: &str, value: &DataValue) -> Result<DataValue, BindingError> {
        let converter = self
            .converters
            .get(name)
            .ok_or_else(|| BindingError::UnknownConverter(name.into()))?;
        self.apply(name, &converter.forward, value)
    }

    pub fn convert_back(&self, name: &str, value: &DataValue) -> Result<DataValue, BindingError> {
        let converter = self
            .converters
            .get(name)
            .ok_or_else(|| BindingError::UnknownConverter(name.into()))?;
        let reverse = converter
            .reverse
            .as_ref()
            .ok_or_else(|| BindingError::MissingReverseConverter(name.into()))?;
        self.apply(name, reverse, value)
    }

    fn apply(
        &self,
        name: &str,
        convert: &Rc<ConvertValue>,
        value: &DataValue,
    ) -> Result<DataValue, BindingError> {
        value.validate()?;
        let converted = convert(value).map_err(|message| BindingError::Conversion {
            name: name.to_owned(),
            message: diagnostic(message),
        })?;
        converted.validate()?;
        Ok(converted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingError {
    InvalidBinding(String),
    InvalidPath(String),
    MissingPath(String),
    InvalidValue(String),
    Schema { path: String, message: String },
    Validation(String),
    InvalidConverterName,
    UnknownConverter(String),
    MissingReverseConverter(String),
    Conversion { name: String, message: String },
    RevisionOverflow,
}

impl fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBinding(message) => {
                write!(formatter, "Invalid control binding: {message}")
            }
            Self::InvalidPath(path) => write!(formatter, "Invalid binding path: {path}"),
            Self::MissingPath(path) => write!(formatter, "Binding path does not exist: {path}"),
            Self::InvalidValue(message) => write!(formatter, "Invalid binding value: {message}"),
            Self::Schema { path, message } => {
                write!(formatter, "Binding schema at {path}: {message}")
            }
            Self::Validation(message) => write!(formatter, "Binding validation: {message}"),
            Self::InvalidConverterName => formatter.write_str(
                "Converter names require 1..=256 bytes without whitespace or control characters",
            ),
            Self::UnknownConverter(name) => write!(formatter, "Unknown binding converter: {name}"),
            Self::MissingReverseConverter(name) => write!(
                formatter,
                "Binding converter has no reverse conversion: {name}"
            ),
            Self::Conversion { name, message } => {
                write!(formatter, "Binding converter {name}: {message}")
            }
            Self::RevisionOverflow => formatter.write_str("Binding context revision exhausted"),
        }
    }
}

impl Error for BindingError {}

fn invalid_path(text: &str) -> BindingError {
    BindingError::InvalidPath(text.to_owned())
}

fn lookup<'a>(root: &'a DataValue, path: &BindingPath) -> Option<&'a DataValue> {
    path.segments
        .iter()
        .try_fold(root, |value, segment| match (value, segment) {
            (DataValue::Object(fields), BindingPathSegment::Field(field)) => fields.get(field),
            (DataValue::List(values), BindingPathSegment::Index(index)) => values.get(*index),
            _ => None,
        })
}

fn lookup_mut<'a>(root: &'a mut DataValue, path: &BindingPath) -> Option<&'a mut DataValue> {
    path.segments
        .iter()
        .try_fold(root, |value, segment| match (value, segment) {
            (DataValue::Object(fields), BindingPathSegment::Field(field)) => fields.get_mut(field),
            (DataValue::List(values), BindingPathSegment::Index(index)) => values.get_mut(*index),
            _ => None,
        })
}

fn validate_root(value: &DataValue) -> Result<(), BindingError> {
    if !matches!(value, DataValue::Object(_)) {
        return Err(BindingError::InvalidValue(
            "The context root must be an object".into(),
        ));
    }
    validate_value(value)
}

fn validate_value(value: &DataValue) -> Result<(), BindingError> {
    let mut pending = vec![(value, 0_usize)];
    let mut nodes = 0_usize;
    let mut bytes = 0_usize;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > MAX_BINDING_NODES || depth > MAX_BINDING_DEPTH {
            return Err(BindingError::InvalidValue(
                "Value tree exceeds its node or nesting limit".into(),
            ));
        }
        match value {
            DataValue::Number(value) if !value.is_finite() => {
                return Err(BindingError::InvalidValue("Numbers must be finite".into()));
            }
            DataValue::String(value) => bytes = bytes.saturating_add(value.len()),
            DataValue::List(values) => {
                if nodes + pending.len() + values.len() > MAX_BINDING_NODES {
                    return Err(BindingError::InvalidValue(
                        "Value tree exceeds its node limit".into(),
                    ));
                }
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            DataValue::Object(values) => {
                if nodes + pending.len() + values.len() > MAX_BINDING_NODES {
                    return Err(BindingError::InvalidValue(
                        "Value tree exceeds its node limit".into(),
                    ));
                }
                for (key, value) in values {
                    bytes = bytes.saturating_add(key.len());
                    pending.push((value, depth + 1));
                }
            }
            _ => {}
        }
        if bytes > MAX_BINDING_BYTES {
            return Err(BindingError::InvalidValue(
                "Value tree exceeds its UTF-8 byte limit".into(),
            ));
        }
    }
    Ok(())
}

fn validate_schema(schema: &ValueSchema) -> Result<(), BindingError> {
    let mut pending = vec![(schema, 0_usize)];
    let mut nodes = 0_usize;
    let mut bytes = 0_usize;
    while let Some((schema, depth)) = pending.pop() {
        nodes += 1;
        if nodes > MAX_BINDING_NODES || depth > MAX_BINDING_DEPTH {
            return Err(BindingError::InvalidValue(
                "Schema exceeds its node or nesting limit".into(),
            ));
        }
        match schema {
            ValueSchema::Object(fields) => {
                if nodes + pending.len() + fields.len() > MAX_BINDING_NODES {
                    return Err(BindingError::InvalidValue(
                        "Schema exceeds its node limit".into(),
                    ));
                }
                for (key, schema) in fields {
                    bytes = bytes.saturating_add(key.len());
                    pending.push((schema, depth + 1));
                }
            }
            ValueSchema::List(item) | ValueSchema::Nullable(item) => {
                pending.push((item, depth + 1))
            }
            _ => {}
        }
        if bytes > MAX_BINDING_BYTES {
            return Err(BindingError::InvalidValue(
                "Schema exceeds its UTF-8 byte limit".into(),
            ));
        }
    }
    Ok(())
}

fn check_schema(schema: &ValueSchema, value: &DataValue, path: &str) -> Result<(), BindingError> {
    match (schema, value) {
        (ValueSchema::Any, _)
        | (ValueSchema::Null, DataValue::Null)
        | (ValueSchema::Bool, DataValue::Bool(_))
        | (ValueSchema::Number, DataValue::Number(_))
        | (ValueSchema::String, DataValue::String(_))
        | (ValueSchema::Nullable(_), DataValue::Null) => Ok(()),
        (ValueSchema::Nullable(inner), _) => check_schema(inner, value, path),
        (ValueSchema::List(item), DataValue::List(values)) => {
            for (index, value) in values.iter().enumerate() {
                check_schema(item, value, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        (ValueSchema::Object(fields), DataValue::Object(values)) => {
            if fields.len() != values.len() || fields.keys().ne(values.keys()) {
                return Err(BindingError::Schema {
                    path: path.into(),
                    message: "Object fields differ from the declared schema".into(),
                });
            }
            for (key, schema) in fields {
                let child = if path == "." {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                };
                check_schema(schema, &values[key], &child)?;
            }
            Ok(())
        }
        _ => Err(BindingError::Schema {
            path: path.into(),
            message: format!("Unexpected {} value", value.type_name()),
        }),
    }
}

fn diagnostic(mut message: String) -> String {
    message.truncate(message.floor_char_boundary(4096));
    if message.trim().is_empty() {
        "The value is invalid.".into()
    } else {
        message
    }
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
