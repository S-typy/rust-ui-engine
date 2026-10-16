use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

mod asynchronous;
mod history;
use asynchronous::{AsyncHandler, AsyncHub, RunningTask};
pub use asynchronous::{
    CancellationToken, CommandAsyncEvent, CommandAsyncOutcome, CommandAsyncState, CommandTaskError,
};
pub use history::{CommandGroup, CommandHistory, CommandHistoryError};

/// An application-defined stable command name, independent of widget identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub String);

impl From<&str> for CommandId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}
impl From<String> for CommandId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Immutable, shared application parameter. The concrete Rust type is preserved.
#[derive(Clone)]
pub struct CommandParameter(Arc<dyn Any + Send + Sync>);
impl CommandParameter {
    pub fn new<T: Send + Sync + 'static>(value: T) -> Self {
        Self(Arc::new(value))
    }
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}
impl Default for CommandParameter {
    fn default() -> Self {
        Self::new(())
    }
}
impl std::fmt::Debug for CommandParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandParameter").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub id: CommandId,
    pub checked: Option<bool>,
}

type ParameterGate = Box<dyn Fn(&CommandParameter) -> bool>;
type ParameterAction = Box<dyn FnMut(&CommandParameter)>;
enum Handler {
    None,
    Sync {
        accepts: fn(&CommandParameter) -> bool,
        action: ParameterAction,
    },
    Async(AsyncHandler),
}
impl Handler {
    fn accepts(&self, parameter: &CommandParameter) -> bool {
        match self {
            Self::None => true,
            Self::Sync { accepts, .. } => accepts(parameter),
            Self::Async(handler) => (handler.accepts)(parameter),
        }
    }
}
struct Command {
    label: String,
    enabled: bool,
    checked: Option<bool>,
    can_execute: Box<dyn Fn() -> bool>,
    parameter_gate: Option<ParameterGate>,
    parameter: CommandParameter,
    handler: Handler,
    running: Option<RunningTask>,
    async_state: CommandAsyncState,
}
impl Command {
    fn available(&self, parameter: &CommandParameter) -> bool {
        self.enabled
            && self.running.is_none()
            && self.handler.accepts(parameter)
            && (self.can_execute)()
            && self
                .parameter_gate
                .as_ref()
                .is_none_or(|gate| gate(parameter))
    }
}
impl Drop for Command {
    fn drop(&mut self) {
        if let Some(task) = &self.running {
            task.cancel.cancel();
        }
    }
}

/// Registry shared by Ribbon items, menus, and ordinary controls.
///
/// Mutations and completion callbacks run on the owning UI thread. Background
/// workers receive only their typed parameter and cooperative cancellation token.
pub struct CommandRegistry {
    commands: HashMap<CommandId, Command>,
    invocations: VecDeque<CommandInvocation>,
    dropped_invocations: usize,
    async_events: VecDeque<CommandAsyncEvent>,
    dropped_async_events: usize,
    next_sequence: u64,
    hub: Arc<AsyncHub>,
}
impl Default for CommandRegistry {
    fn default() -> Self {
        Self {
            commands: HashMap::new(),
            invocations: VecDeque::new(),
            dropped_invocations: 0,
            async_events: VecDeque::new(),
            dropped_async_events: 0,
            next_sequence: 0,
            hub: Arc::new(AsyncHub::default()),
        }
    }
}
impl Drop for CommandRegistry {
    fn drop(&mut self) {
        self.hub.close();
    }
}
impl CommandRegistry {
    pub const MAX_PENDING_INVOCATIONS: usize = 4096;
    pub const MAX_PENDING_ASYNC_EVENTS: usize = 1024;
    pub const MAX_ASYNC_WORKERS: usize = 16;

    /// Replacing an ID cancels its previous task and discards its stale result.
    pub fn register(&mut self, id: impl Into<CommandId>, label: impl Into<String>) {
        self.commands.insert(
            id.into(),
            Command {
                label: label.into(),
                enabled: true,
                checked: None,
                can_execute: Box::new(|| true),
                parameter_gate: None,
                parameter: CommandParameter::default(),
                handler: Handler::None,
                running: None,
                async_state: CommandAsyncState::Idle,
            },
        );
    }
    /// Register a toggle command, resetting callbacks and policy for this ID.
    pub fn register_toggle(
        &mut self,
        id: impl Into<CommandId>,
        label: impl Into<String>,
        checked: bool,
    ) {
        let id = id.into();
        self.register(id.clone(), label);
        self.set_checked(&id, Some(checked));
    }
    pub fn unregister(&mut self, id: &CommandId) -> bool {
        self.commands.remove(id).is_some()
    }
    pub fn contains(&self, id: &CommandId) -> bool {
        self.commands.contains_key(id)
    }
    pub fn label(&self, id: &CommandId) -> Option<&str> {
        self.commands.get(id).map(|c| c.label.as_str())
    }
    pub fn enabled(&self, id: &CommandId) -> bool {
        self.commands
            .get(id)
            .is_some_and(|c| c.available(&c.parameter) && self.worker_available(c))
    }
    pub fn enabled_with(&self, id: &CommandId, parameter: &CommandParameter) -> bool {
        self.commands
            .get(id)
            .is_some_and(|c| c.available(parameter) && self.worker_available(c))
    }
    fn worker_available(&self, command: &Command) -> bool {
        !matches!(command.handler, Handler::Async(_)) || self.hub.worker_available()
    }
    pub fn checked(&self, id: &CommandId) -> Option<bool> {
        self.commands.get(id).and_then(|c| c.checked)
    }
    pub fn set_enabled(&mut self, id: &CommandId, enabled: bool) -> bool {
        self.commands.get_mut(id).is_some_and(|c| {
            c.enabled = enabled;
            true
        })
    }
    /// Set shared state without invoking. `Some` makes every placement a toggle.
    pub fn set_checked(&mut self, id: &CommandId, checked: Option<bool>) -> bool {
        self.commands.get_mut(id).is_some_and(|c| {
            c.checked = checked;
            true
        })
    }
    pub fn set_can_execute(&mut self, id: &CommandId, check: impl Fn() -> bool + 'static) -> bool {
        self.commands.get_mut(id).is_some_and(|c| {
            c.can_execute = Box::new(check);
            true
        })
    }
    /// Additional parameter-sensitive predicate. Wrong parameter types are rejected.
    pub fn set_can_execute_with<P: 'static>(
        &mut self,
        id: &CommandId,
        check: impl Fn(&P) -> bool + 'static,
    ) -> bool {
        self.commands.get_mut(id).is_some_and(|c| {
            c.parameter_gate = Some(Box::new(move |p| p.downcast_ref::<P>().is_some_and(&check)));
            true
        })
    }
    pub fn clear_parameter_gate(&mut self, id: &CommandId) -> bool {
        self.commands.get_mut(id).is_some_and(|c| {
            c.parameter_gate = None;
            true
        })
    }
    /// Default parameter shared by all placements; an in-flight task keeps its old snapshot.
    pub fn set_parameter(&mut self, id: &CommandId, parameter: CommandParameter) -> bool {
        self.commands.get_mut(id).is_some_and(|c| {
            c.parameter = parameter;
            true
        })
    }
    pub fn parameter(&self, id: &CommandId) -> Option<&CommandParameter> {
        self.commands.get(id).map(|c| &c.parameter)
    }
    pub fn on_execute(&mut self, id: &CommandId, mut action: impl FnMut() + 'static) -> bool {
        self.replace_handler(
            id,
            Handler::Sync {
                accepts: |_| true,
                action: Box::new(move |_| action()),
            },
        )
    }
    pub fn on_execute_with<P: 'static>(
        &mut self,
        id: &CommandId,
        mut action: impl FnMut(&P) + 'static,
    ) -> bool {
        self.replace_handler(
            id,
            Handler::Sync {
                accepts: |p| p.downcast_ref::<P>().is_some(),
                action: Box::new(move |p| {
                    action(p.downcast_ref::<P>().expect("validated parameter"))
                }),
            },
        )
    }
    fn replace_handler(&mut self, id: &CommandId, handler: Handler) -> bool {
        let Some(command) = self.commands.get_mut(id) else {
            return false;
        };
        if let Some(task) = command.running.take() {
            task.cancel.cancel();
        }
        command.handler = handler;
        command.async_state = CommandAsyncState::Idle;
        true
    }
    /// Check policy once, toggle once, run/start once, emit one invocation.
    pub fn activate(&mut self, id: &CommandId) -> bool {
        let Some(p) = self.parameter(id).cloned() else {
            return false;
        };
        self.invoke(id, p, true)
    }
    /// Low-level execution preserving the checked state; applies the same gate.
    pub fn execute(&mut self, id: &CommandId) -> bool {
        let Some(p) = self.parameter(id).cloned() else {
            return false;
        };
        self.invoke(id, p, false)
    }
    pub fn activate_with(&mut self, id: &CommandId, parameter: CommandParameter) -> bool {
        self.invoke(id, parameter, true)
    }
    pub fn execute_with(&mut self, id: &CommandId, parameter: CommandParameter) -> bool {
        self.invoke(id, parameter, false)
    }
    fn invoke(&mut self, id: &CommandId, parameter: CommandParameter, toggle: bool) -> bool {
        let Some(command) = self.commands.get_mut(id) else {
            return false;
        };
        if !command.available(&parameter) {
            return false;
        }
        if let Handler::Async(handler) = &command.handler {
            let Some(sequence) = self.next_sequence.checked_add(1) else {
                return false;
            };
            let Some(task) = RunningTask::start(handler, parameter.clone(), sequence, &self.hub)
            else {
                return false;
            };
            self.next_sequence = sequence;
            command.running = Some(task);
            command.async_state = CommandAsyncState::Running {
                sequence,
                cancellation_requested: false,
            };
        }
        if toggle && let Some(checked) = &mut command.checked {
            *checked = !*checked;
        }
        if let Handler::Sync { action, .. } = &mut command.handler {
            action(&parameter);
        }
        if self.invocations.len() == Self::MAX_PENDING_INVOCATIONS {
            self.invocations.pop_front();
            self.dropped_invocations = self.dropped_invocations.saturating_add(1);
        }
        self.invocations.push_back(CommandInvocation {
            id: id.clone(),
            checked: command.checked,
        });
        true
    }
    pub fn drain_invocations(&mut self) -> Vec<CommandInvocation> {
        self.invocations.drain(..).collect()
    }
    /// Number of old notifications discarded since the last call. Actions are never replayed.
    pub fn take_dropped_invocations(&mut self) -> usize {
        std::mem::take(&mut self.dropped_invocations)
    }
}

#[cfg(test)]
mod tests;
