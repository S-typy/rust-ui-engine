//! Optional bounded worker-to-owner delivery. No worker is created by the cache.
use super::*;
use std::{
    collections::VecDeque,
    fmt,
    sync::{Mutex, TryLockError},
};
type Waker = Arc<dyn Fn() + Send + Sync>;
struct Submission {
    token: PageToken,
    result: Result<PageResponse, String>,
    cancel: PageCancellation,
    bytes: usize,
}
struct Queue {
    items: VecDeque<Submission>,
    bytes: usize,
    waker: Option<Waker>,
}
pub(super) struct Shared {
    source: u64,
    settings: PageCacheSettings,
    alive: AtomicBool,
    queue: Mutex<Queue>,
}
impl Shared {
    pub(super) fn new(source: u64, settings: PageCacheSettings) -> Self {
        Self {
            source,
            settings,
            alive: AtomicBool::new(true),
            queue: Mutex::new(Queue {
                items: VecDeque::new(),
                bytes: 0,
                waker: None,
            }),
        }
    }
    pub(super) fn clear_cancelled(&self) {
        if let Ok(mut q) = self.queue.try_lock() {
            q.items.retain(|i| !i.cancel.is_cancelled());
            q.bytes = q.items.iter().map(|i| i.bytes).sum();
        }
    }
    pub(super) fn close(&self) {
        self.alive.store(false, Ordering::Release);
        if let Ok(mut q) = self.queue.try_lock() {
            q.items.clear();
            q.bytes = 0;
            q.waker = None;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSendError {
    Closed,
    Cancelled,
    ForeignRequest,
    Full,
    Busy,
    PayloadLimit,
}
/// An unsuccessful send returns its owned payload; no page is silently dropped.
pub struct PageSendFailure {
    pub error: PageSendError,
    pub result: Result<PageResponse, String>,
}
impl fmt::Debug for PageSendFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageSendFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
/// Cloneable Send+Sync mailbox endpoint. try_complete never waits for the owner.
#[derive(Clone)]
pub struct PageCompletionSender {
    shared: Arc<Shared>,
}
impl PageCompletionSender {
    pub fn try_complete(
        &self,
        request: &PageRequest,
        mut result: Result<PageResponse, String>,
    ) -> Result<(), Box<PageSendFailure>> {
        let failure = |error, result| Box::new(PageSendFailure { error, result });
        if request.token.source != self.shared.source {
            return Err(failure(PageSendError::ForeignRequest, result));
        }
        if !self.shared.alive.load(Ordering::Acquire) {
            return Err(failure(PageSendError::Closed, result));
        }
        if request.cancellation.is_cancelled() {
            return Err(failure(PageSendError::Cancelled, result));
        }
        if let Err(message) = &mut result {
            *message = bounded_error(message);
        }
        let bytes = match &result {
            Ok(page) => {
                if page.table.rows().len() > request.limit {
                    return Err(failure(PageSendError::PayloadLimit, result));
                }
                match payload_bytes(&page.table) {
                    Ok(bytes) => bytes,
                    Err(_) => return Err(failure(PageSendError::PayloadLimit, result)),
                }
            }
            Err(message) => message.len(),
        };
        if bytes > self.shared.settings.max_bytes {
            return Err(failure(PageSendError::PayloadLimit, result));
        }
        let mut queue = match self.shared.queue.try_lock() {
            Ok(q) => q,
            Err(TryLockError::WouldBlock) => return Err(failure(PageSendError::Busy, result)),
            Err(TryLockError::Poisoned(_)) => return Err(failure(PageSendError::Closed, result)),
        };
        if !self.shared.alive.load(Ordering::Acquire) {
            return Err(failure(PageSendError::Closed, result));
        }
        if request.cancellation.is_cancelled() {
            return Err(failure(PageSendError::Cancelled, result));
        }
        // Retiring old responses here prevents a cancelled query filling a queue
        // until the next owner drain. The lock contains no user callback.
        queue.items.retain(|i| !i.cancel.is_cancelled());
        queue.bytes = queue.items.iter().map(|i| i.bytes).sum();
        if queue.items.len() >= self.shared.settings.mailbox_capacity
            || queue.bytes + bytes > self.shared.settings.max_bytes
        {
            return Err(failure(PageSendError::Full, result));
        }
        queue.items.push_back(Submission {
            token: request.token,
            result,
            cancel: request.cancellation.clone(),
            bytes,
        });
        queue.bytes += bytes;
        let waker = queue.waker.clone();
        drop(queue);
        if !self.shared.alive.load(Ordering::Acquire) {
            self.shared.close();
            return Ok(());
        }
        if let Some(waker) = waker {
            waker();
        }
        Ok(())
    }
    pub fn is_closed(&self) -> bool {
        !self.shared.alive.load(Ordering::Acquire)
    }
}
#[derive(Debug)]
pub struct PageDrain {
    pub results: Vec<(PageToken, Result<PageCompletion, DataError>)>,
    pub remaining: usize,
}
impl PagedDataSource {
    pub fn completion_sender(&self) -> PageCompletionSender {
        PageCompletionSender {
            shared: self.mailbox.clone(),
        }
    }
    /// Wakes the owner event loop after a successful enqueue. Called without the
    /// mailbox lock; the callback must be quick and must not panic. A full/busy/
    /// closed send never invokes it. No callback runs from ordinary row getters.
    pub fn set_completion_waker(
        &mut self,
        waker: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<(), PageSendError> {
        let mut q = self.mailbox.queue.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => PageSendError::Busy,
            TryLockError::Poisoned(_) => PageSendError::Closed,
        })?;
        q.waker = waker;
        Ok(())
    }
    /// Poll up to 64 queued results without waiting for a producer. Publication
    /// validates each response independently; a bad page cannot roll back other
    /// successful responses. Direct complete remains available without a mailbox.
    pub fn drain_completions(&mut self, limit: usize) -> Result<PageDrain, PageSendError> {
        if !(1..=64).contains(&limit) {
            return Err(PageSendError::PayloadLimit);
        }
        let (batch, remaining) = {
            let mut q = self.mailbox.queue.try_lock().map_err(|e| match e {
                TryLockError::WouldBlock => PageSendError::Busy,
                TryLockError::Poisoned(_) => PageSendError::Closed,
            })?;
            let mut batch = Vec::with_capacity(limit.min(q.items.len()));
            for _ in 0..limit {
                let Some(item) = q.items.pop_front() else {
                    break;
                };
                q.bytes -= item.bytes;
                batch.push(item);
            }
            (batch, q.items.len())
        };
        let results = batch
            .into_iter()
            .map(|item| (item.token, self.complete(item.token, item.result)))
            .collect();
        Ok(PageDrain { results, remaining })
    }
}
