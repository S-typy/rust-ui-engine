//! Explicit, versioned workspace configuration with staged component codecs.
//!
//! The store owns configuration values. Codecs must be pure: they must not
//! mutate shared application state, perform I/O, or change live controls. A
//! prepared restore exclusively borrows the store; dropping it cancels it,
//! while committing it is infallible. Applications may validate and apply a
//! retained UI mutation batch before committing the prepared configuration.
//! Arbitrary live-control callbacks are not an atomic transaction.

use serde::{Deserialize, Serialize};
use std::{
    any::Any,
    collections::BTreeMap,
    fmt,
    io::{self, Write},
    rc::Rc,
};

const FORMAT: &str = "rust-ui-engine.workspace";
/// Current envelope version; individual components have independent versions.
pub const WORKSPACE_VERSION: u32 = 1;
pub const MAX_WORKSPACE_BYTES: usize = 1_048_576;
pub const MAX_WORKSPACE_COMPONENTS: usize = 64;

/// A bounded diagnostic. Component payloads are never included automatically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceError {
    pub component: Option<String>,
    pub message: String,
}
impl WorkspaceError {
    pub fn new(message: impl AsRef<str>) -> Self {
        Self {
            component: None,
            message: bounded(message.as_ref()),
        }
    }
    fn at(mut self, id: &str) -> Self {
        self.component = Some(bounded(id));
        self.message = bounded(&self.message);
        self
    }
}
impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(component) = &self.component {
            write!(f, "Workspace component {component}: {}", self.message)
        } else {
            write!(f, "Workspace: {}", self.message)
        }
    }
}
impl std::error::Error for WorkspaceError {}

/// A component owns its format, validation and migrations.
///
/// `decode` receives a version between one and `version()` inclusive. It must
/// explicitly reject unsupported older versions or migrate them to a complete
/// new value. `current` is read-only context, such as an existing pane catalog.
/// Values must own their configuration; an `Rc<RefCell<_>>` alias to mutable
/// external state defeats staging and is outside this contract.
pub trait WorkspaceCodec<T>: 'static {
    fn kind(&self) -> &str;
    fn version(&self) -> u32;
    fn validate(&self, value: &T) -> Result<(), WorkspaceError>;
    fn encode(&self, value: &T) -> Result<String, WorkspaceError>;
    fn decode(&self, version: u32, payload: &str, current: &T) -> Result<T, WorkspaceError>;
}

trait ErasedComponent {
    fn value(&self) -> &dyn Any;
    fn kind(&self) -> &str;
    fn version(&self) -> u32;
    fn payload(&self) -> &str;
    fn decode(
        &self,
        version: u32,
        payload: &str,
    ) -> Result<Box<dyn ErasedComponent>, WorkspaceError>;
    fn replace(&self, value: Box<dyn Any>) -> Result<Box<dyn ErasedComponent>, WorkspaceError>;
}
struct Component<T> {
    value: T,
    codec: Rc<dyn WorkspaceCodec<T>>,
    kind: String,
    version: u32,
    payload: String,
}
impl<T: 'static> Component<T> {
    fn make(value: T, codec: Rc<dyn WorkspaceCodec<T>>) -> Result<Self, WorkspaceError> {
        identifier(codec.kind())?;
        let kind = codec.kind().to_owned();
        let version = codec.version();
        if version == 0 {
            return Err(error("Component version must be positive"));
        }
        codec.validate(&value)?;
        let payload = codec.encode(&value)?;
        payload_limit(&payload)?;
        Ok(Self {
            value,
            codec,
            kind,
            version,
            payload,
        })
    }
}
impl<T: 'static> ErasedComponent for Component<T> {
    fn value(&self) -> &dyn Any {
        &self.value
    }
    fn kind(&self) -> &str {
        &self.kind
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn payload(&self) -> &str {
        &self.payload
    }
    fn decode(
        &self,
        version: u32,
        payload: &str,
    ) -> Result<Box<dyn ErasedComponent>, WorkspaceError> {
        if version == 0 || version > self.version {
            return Err(error("Unsupported component version"));
        }
        let value = self.codec.decode(version, payload, &self.value)?;
        let next = Self::make(value, Rc::clone(&self.codec))?;
        if next.kind != self.kind || next.version != self.version {
            return Err(error("Codec metadata changed after registration"));
        }
        Ok(Box::new(next))
    }
    fn replace(&self, value: Box<dyn Any>) -> Result<Box<dyn ErasedComponent>, WorkspaceError> {
        let value = *value
            .downcast::<T>()
            .map_err(|_| error("Component Rust type does not match registration"))?;
        let next = Self::make(value, Rc::clone(&self.codec))?;
        if next.kind != self.kind || next.version != self.version {
            return Err(error("Codec metadata changed after registration"));
        }
        Ok(Box::new(next))
    }
}

/// Typed, application-owned configuration registry. No automatic file I/O.
pub struct WorkspaceStore {
    id: String,
    generation: u64,
    components: BTreeMap<String, Box<dyn ErasedComponent>>,
}
impl WorkspaceStore {
    pub fn new(id: impl Into<String>) -> Result<Self, WorkspaceError> {
        let id = id.into();
        identifier(&id)?;
        Ok(Self {
            id,
            generation: 0,
            components: BTreeMap::new(),
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn component_ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }
    pub fn get<T: 'static>(&self, id: &str) -> Option<&T> {
        self.components.get(id)?.value().downcast_ref()
    }
    pub fn register<T: 'static>(
        &mut self,
        id: impl Into<String>,
        value: T,
        codec: impl WorkspaceCodec<T>,
    ) -> Result<(), WorkspaceError> {
        let id = id.into();
        identifier(&id)?;
        if self.components.contains_key(&id) {
            return Err(error("Duplicate component identifier").at(&id));
        }
        if self.components.len() >= MAX_WORKSPACE_COMPONENTS {
            return Err(error("Workspace supports at most 64 components"));
        }
        let generation = self.next_generation()?;
        let entry = Component::make(value, Rc::new(codec)).map_err(|e| e.at(&id))?;
        encode(
            &self.id,
            self.components
                .iter()
                .map(|(id, c)| (id.as_str(), c.as_ref()))
                .chain(std::iter::once((
                    id.as_str(),
                    &entry as &dyn ErasedComponent,
                ))),
        )?;
        self.components.insert(id, Box::new(entry));
        self.generation = generation;
        Ok(())
    }
    /// Replace one accepted configuration after codec and envelope validation.
    pub fn replace<T: 'static>(&mut self, id: &str, value: T) -> Result<(), WorkspaceError> {
        identifier(id)?;
        let generation = self.next_generation()?;
        let current = self
            .components
            .get(id)
            .ok_or_else(|| error("Unknown component").at(id))?;
        let candidate = current.replace(Box::new(value)).map_err(|e| e.at(id))?;
        encode(
            &self.id,
            self.components.iter().map(|(key, c)| {
                (
                    key.as_str(),
                    if key == id {
                        candidate.as_ref()
                    } else {
                        c.as_ref()
                    },
                )
            }),
        )?;
        self.components.insert(id.into(), candidate);
        self.generation = generation;
        Ok(())
    }
    /// Explicitly encode the accepted configuration. Does not invoke codecs or I/O.
    pub fn save_json(&self) -> Result<String, WorkspaceError> {
        encode(
            &self.id,
            self.components
                .iter()
                .map(|(id, c)| (id.as_str(), c.as_ref())),
        )
    }
    /// Stage a complete restore. Missing/unknown components, versions, codec
    /// errors and exhausted generation fail before any accepted state changes.
    pub fn prepare_restore(
        &mut self,
        source: &str,
    ) -> Result<PreparedWorkspace<'_>, WorkspaceError> {
        if source.len() > MAX_WORKSPACE_BYTES {
            return Err(error("Workspace JSON exceeds one MiB"));
        }
        let generation = self.next_generation()?;
        let envelope: Envelope = serde_json::from_str(source).map_err(|e| {
            error(format!(
                "Invalid envelope JSON at line {} column {}",
                e.line(),
                e.column()
            ))
        })?;
        if envelope.format != FORMAT || envelope.version != WORKSPACE_VERSION {
            return Err(error("Unsupported envelope format or version"));
        }
        if envelope.workspace != self.id {
            return Err(error("Workspace identifier does not match"));
        }
        if envelope.components.len() > MAX_WORKSPACE_COMPONENTS {
            return Err(error("Workspace supports at most 64 components"));
        }
        let mut records = BTreeMap::new();
        for record in envelope.components {
            identifier(&record.id)?;
            identifier(&record.kind)?;
            payload_limit(&record.payload)?;
            let id = record.id.clone();
            if records.insert(id.clone(), record).is_some() {
                return Err(error("Duplicate component identifier").at(&id));
            }
            if !self.components.contains_key(&id) {
                return Err(error("Unknown component").at(&id));
            }
        }
        for id in self.components.keys() {
            if !records.contains_key(id) {
                return Err(error("Missing component").at(id));
            }
        }
        let mut candidates = BTreeMap::new();
        let mut payload_bytes = 0usize;
        for (id, record) in records {
            let registered = &self.components[&id];
            if registered.kind() != record.kind {
                return Err(error("Component type identifier does not match").at(&id));
            }
            let candidate = registered
                .decode(record.version, &record.payload)
                .map_err(|e| e.at(&id))?;
            payload_bytes = payload_bytes
                .checked_add(candidate.payload().len())
                .ok_or_else(|| error("Workspace payload limit exceeded"))?;
            if payload_bytes > MAX_WORKSPACE_BYTES {
                return Err(error("Workspace payload limit exceeded"));
            }
            candidates.insert(id, candidate);
        }
        // Migrations may expand payloads; validate their final envelope too.
        encode(
            &self.id,
            candidates.iter().map(|(id, c)| (id.as_str(), c.as_ref())),
        )?;
        Ok(PreparedWorkspace {
            store: self,
            candidates,
            generation,
        })
    }
    pub fn restore_json(&mut self, source: &str) -> Result<u64, WorkspaceError> {
        Ok(self.prepare_restore(source)?.commit())
    }
    fn next_generation(&self) -> Result<u64, WorkspaceError> {
        self.generation
            .checked_add(1)
            .ok_or_else(|| error("Workspace generation exhausted"))
    }
}

/// An exclusive restore transaction. Drop to cancel; commit cannot fail.
///
/// Validate required live handles, construct all mutations and call a single
/// `Runtime::apply_batch` while this guard is held. Only on success call commit.
/// No fallible callback is executed by commit, and generation overflow has
/// already been checked before this guard can exist.
pub struct PreparedWorkspace<'a> {
    store: &'a mut WorkspaceStore,
    candidates: BTreeMap<String, Box<dyn ErasedComponent>>,
    generation: u64,
}
impl PreparedWorkspace<'_> {
    pub fn get<T: 'static>(&self, id: &str) -> Option<&T> {
        self.candidates.get(id)?.value().downcast_ref()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn commit(self) -> u64 {
        let old = std::mem::replace(&mut self.store.components, self.candidates);
        self.store.generation = self.generation;
        drop(old);
        self.generation
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    workspace: String,
    components: Vec<Record>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    kind: String,
    version: u32,
    payload: String,
}
#[derive(Serialize)]
struct Output<'a> {
    format: &'static str,
    version: u32,
    workspace: &'a str,
    components: Vec<OutputRecord<'a>>,
}
#[derive(Serialize)]
struct OutputRecord<'a> {
    id: &'a str,
    kind: &'a str,
    version: u32,
    payload: &'a str,
}
fn encode<'a>(
    id: &str,
    components: impl Iterator<Item = (&'a str, &'a dyn ErasedComponent)>,
) -> Result<String, WorkspaceError> {
    let mut records = Vec::new();
    let mut total = 0usize;
    for (id, c) in components {
        total = total
            .checked_add(c.payload().len())
            .ok_or_else(|| error("Workspace payload limit exceeded"))?;
        if total > MAX_WORKSPACE_BYTES {
            return Err(error("Workspace payload limit exceeded"));
        }
        records.push(OutputRecord {
            id,
            kind: c.kind(),
            version: c.version(),
            payload: c.payload(),
        });
    }
    records.sort_by_key(|r| r.id);
    let mut buffer = BoundedOutput(Vec::new());
    serde_json::to_writer_pretty(
        &mut buffer,
        &Output {
            format: FORMAT,
            version: WORKSPACE_VERSION,
            workspace: id,
            components: records,
        },
    )
    .map_err(|_| error("Workspace JSON exceeds one MiB"))?;
    buffer
        .write_all(b"\n")
        .map_err(|_| error("Workspace JSON exceeds one MiB"))?;
    String::from_utf8(buffer.0).map_err(|_| error("Invalid workspace encoding"))
}
struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_WORKSPACE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Workspace size limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn identifier(value: &str) -> Result<(), WorkspaceError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-/".contains(&c))
    {
        Err(error(
            "Identifiers require 1..128 ASCII letters, digits, _, ., -, /",
        ))
    } else {
        Ok(())
    }
}
fn payload_limit(value: &str) -> Result<(), WorkspaceError> {
    if value.len() > MAX_WORKSPACE_BYTES {
        Err(error("Component payload exceeds one MiB"))
    } else {
        Ok(())
    }
}
fn bounded(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .scan(0, |size, c| {
            *size += c.len_utf8();
            (*size <= 512).then_some(c)
        })
        .collect()
}
fn error(message: impl AsRef<str>) -> WorkspaceError {
    WorkspaceError::new(message)
}

/// Placement codec using the existing docking catalog as validation context.
/// Pane content, widget IDs and business data are absent from its payload.
pub struct DockWorkspaceCodec;
impl WorkspaceCodec<crate::DockLayout> for DockWorkspaceCodec {
    fn kind(&self) -> &str {
        "docking"
    }
    fn version(&self) -> u32 {
        1
    }
    fn validate(&self, value: &crate::DockLayout) -> Result<(), WorkspaceError> {
        value
            .snapshot_json()
            .map(|_| ())
            .map_err(|_| error("Invalid docking configuration"))
    }
    fn encode(&self, value: &crate::DockLayout) -> Result<String, WorkspaceError> {
        value
            .snapshot_json()
            .map_err(|_| error("Invalid docking configuration"))
    }
    fn decode(
        &self,
        version: u32,
        payload: &str,
        current: &crate::DockLayout,
    ) -> Result<crate::DockLayout, WorkspaceError> {
        if version != 1 {
            return Err(error("Unsupported docking codec version"));
        }
        let mut next = current.clone();
        next.restore_json(payload)
            .map_err(|e| error(e.to_string()))?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests;
