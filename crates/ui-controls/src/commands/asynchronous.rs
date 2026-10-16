use super::{CommandId, CommandParameter, CommandRegistry, Handler};
use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

/// A cooperative request. Cancellation never terminates a thread forcibly.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), CommandTaskError> {
        if self.is_cancelled() {
            Err(CommandTaskError::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandTaskError {
    Cancelled,
    Failed(String),
    Panicked,
}
impl std::fmt::Display for CommandTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "command cancelled"),
            Self::Failed(s) => f.write_str(s),
            Self::Panicked => write!(f, "command worker panicked"),
        }
    }
}
impl std::error::Error for CommandTaskError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAsyncOutcome {
    Succeeded,
    Cancelled,
    Failed(String),
    Panicked,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAsyncState {
    Idle,
    Running {
        sequence: u64,
        cancellation_requested: bool,
    },
    Finished {
        sequence: u64,
        outcome: CommandAsyncOutcome,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandAsyncEvent {
    pub id: CommandId,
    pub sequence: u64,
    pub outcome: CommandAsyncOutcome,
}

type TaskResult = Result<Box<dyn Any + Send>, CommandTaskError>;
type Worker = Arc<dyn Fn(CommandParameter, CancellationToken) -> TaskResult + Send + Sync>;
type Wake = Arc<dyn Fn() + Send + Sync>;
pub(super) struct AsyncHandler {
    pub accepts: fn(&CommandParameter) -> bool,
    worker: Worker,
    complete: Box<dyn FnMut(TaskResult)>,
}
pub(super) struct AsyncHub {
    alive: AtomicBool,
    workers: AtomicUsize,
    wake: Mutex<Option<Wake>>,
}
impl Default for AsyncHub {
    fn default() -> Self {
        Self {
            alive: AtomicBool::new(true),
            workers: AtomicUsize::new(0),
            wake: Mutex::new(None),
        }
    }
}
impl AsyncHub {
    pub fn close(&self) {
        self.alive.store(false, Ordering::Release);
        *self.wake.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub fn worker_available(&self) -> bool {
        self.workers.load(Ordering::Acquire) < CommandRegistry::MAX_ASYNC_WORKERS
    }
    fn reserve(&self) -> bool {
        self.workers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < CommandRegistry::MAX_ASYNC_WORKERS).then_some(n + 1)
            })
            .is_ok()
    }
    fn notify(&self) {
        let wake = self.wake.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if self.alive.load(Ordering::Acquire)
            && let Some(wake) = wake
        {
            // A host wake is only a hint. A faulty host callback cannot poison
            // completion delivery or leak a worker slot.
            let _ = catch_unwind(AssertUnwindSafe(|| wake()));
        }
    }
}
pub(super) struct RunningTask {
    sequence: u64,
    pub cancel: CancellationToken,
    result: Arc<Mutex<Option<TaskResult>>>,
}
fn bounded_error(error: CommandTaskError) -> CommandTaskError {
    match error {
        CommandTaskError::Failed(text) => {
            if text.capacity() > 4096 {
                let mut end = text.len().min(4096);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                // Do not retain a worker's oversized String allocation after
                // shortening its visible diagnostic.
                return CommandTaskError::Failed(text[..end].to_owned());
            }
            CommandTaskError::Failed(text)
        }
        other => other,
    }
}
impl RunningTask {
    pub fn start(
        handler: &AsyncHandler,
        parameter: CommandParameter,
        sequence: u64,
        hub: &Arc<AsyncHub>,
    ) -> Option<Self> {
        if !hub.reserve() {
            return None;
        }
        let result = Arc::new(Mutex::new(None));
        let cancel = CancellationToken::default();
        let task = Self {
            sequence,
            cancel: cancel.clone(),
            result: result.clone(),
        };
        let worker = handler.worker.clone();
        let worker_hub = hub.clone();
        let spawned = std::thread::Builder::new()
            .name("ui-command".into())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    cancel.check()?;
                    worker(parameter, cancel.clone()).map_err(bounded_error)
                }))
                .unwrap_or(Err(CommandTaskError::Panicked));
                *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
                worker_hub.workers.fetch_sub(1, Ordering::AcqRel);
                worker_hub.notify();
            });
        if spawned.is_err() {
            hub.workers.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(task)
    }
}
impl CommandRegistry {
    /// Install a worker and its UI-thread completion callback. A parameter of
    /// type `P` must be supplied before activation. Only one task per ID runs.
    /// A replaced handler cancels and forgets its old completion.
    pub fn on_async_execute<P: Send + Sync + 'static, R: Send + 'static>(
        &mut self,
        id: &CommandId,
        work: impl Fn(&P, CancellationToken) -> Result<R, CommandTaskError> + Send + Sync + 'static,
        mut complete: impl FnMut(Result<R, CommandTaskError>) + 'static,
    ) -> bool {
        self.replace_handler(
            id,
            Handler::Async(AsyncHandler {
                accepts: |parameter| parameter.downcast_ref::<P>().is_some(),
                worker: Arc::new(move |parameter, cancel| {
                    work(
                        parameter.downcast_ref::<P>().expect("validated parameter"),
                        cancel,
                    )
                    .map(|result| Box::new(result) as Box<dyn Any + Send>)
                }),
                complete: Box::new(move |result| {
                    complete(
                        result.map(|value| *value.downcast::<R>().expect("worker result type")),
                    )
                }),
            }),
        )
    }
    /// Wake callbacks may run on a worker thread and must only schedule UI work.
    /// Setting a callback sends a hint immediately, closing the pending-result race.
    pub fn set_waker(&mut self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        *self.hub.wake.lock().unwrap_or_else(|e| e.into_inner()) = wake;
        self.hub.notify();
    }
    pub fn async_state(&self, id: &CommandId) -> Option<&CommandAsyncState> {
        self.commands.get(id).map(|command| &command.async_state)
    }
    /// Request cancellation; remain busy until the worker finishes. A result
    /// arriving before this request but not yet polled is also discarded.
    pub fn cancel(&mut self, id: &CommandId) -> bool {
        let Some(command) = self.commands.get_mut(id) else {
            return false;
        };
        let Some(task) = &command.running else {
            return false;
        };
        task.cancel.cancel();
        command.async_state = CommandAsyncState::Running {
            sequence: task.sequence,
            cancellation_requested: true,
        };
        true
    }
    /// Deliver completed tasks once on the calling UI thread. Cancellation wins
    /// over an unpolled success. Toggle state records acceptance, not job success;
    /// completion never silently rolls it back.
    pub fn poll_async(&mut self) -> usize {
        let mut count = 0;
        for (id, command) in &mut self.commands {
            let Some(task) = &command.running else {
                continue;
            };
            let Some(mut result) = task.result.lock().unwrap_or_else(|e| e.into_inner()).take()
            else {
                continue;
            };
            let sequence = task.sequence;
            if task.cancel.is_cancelled() {
                result = Err(CommandTaskError::Cancelled);
            }
            let outcome = match &result {
                Ok(_) => CommandAsyncOutcome::Succeeded,
                Err(CommandTaskError::Cancelled) => CommandAsyncOutcome::Cancelled,
                Err(CommandTaskError::Failed(message)) => {
                    CommandAsyncOutcome::Failed(message.clone())
                }
                Err(CommandTaskError::Panicked) => CommandAsyncOutcome::Panicked,
            };
            command.running = None;
            command.async_state = CommandAsyncState::Finished {
                sequence,
                outcome: outcome.clone(),
            };
            if self.async_events.len() == Self::MAX_PENDING_ASYNC_EVENTS {
                self.async_events.pop_front();
                self.dropped_async_events = self.dropped_async_events.saturating_add(1);
            }
            self.async_events.push_back(CommandAsyncEvent {
                id: id.clone(),
                sequence,
                outcome,
            });
            count += 1;
            if let Handler::Async(handler) = &mut command.handler {
                (handler.complete)(result);
            }
        }
        count
    }
    pub fn drain_async_events(&mut self) -> Vec<CommandAsyncEvent> {
        self.async_events.drain(..).collect()
    }
    pub fn take_dropped_async_events(&mut self) -> usize {
        std::mem::take(&mut self.dropped_async_events)
    }
    /// Includes cancelled/replaced workers until they actually exit.
    pub fn active_workers(&self) -> usize {
        self.hub.workers.load(Ordering::Acquire)
    }
}
