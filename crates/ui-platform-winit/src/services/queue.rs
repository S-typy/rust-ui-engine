use super::*;
use std::{
    collections::VecDeque,
    sync::{Mutex, Weak},
};
const MAX_REQUESTS: usize = 64;
const MAX_BYTES: usize = 24 * 1024 * 1024;
#[derive(Clone)]
pub struct ServiceRequestId {
    owner: Weak<()>,
    serial: u64,
}
impl fmt::Debug for ServiceRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ServiceRequestId")
            .field(&self.serial)
            .finish()
    }
}
impl PartialEq for ServiceRequestId {
    fn eq(&self, v: &Self) -> bool {
        self.serial == v.serial && self.owner.ptr_eq(&v.owner)
    }
}
impl Eq for ServiceRequestId {}
struct Pending {
    id: ServiceRequestId,
    request: PlatformServiceRequest,
    bytes: usize,
}
struct State {
    active: bool,
    next: u64,
    bytes: usize,
    pending: VecDeque<Pending>,
}
struct Shared {
    identity: Arc<()>,
    state: Mutex<State>,
    waker: AppWaker,
}
/// Cloneable worker handle. Posting only schedules a UI-thread completion;
/// no application callback executes under its lock or on a worker.
#[derive(Clone)]
pub struct PlatformServiceHandle {
    shared: Weak<Shared>,
}
impl fmt::Debug for PlatformServiceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlatformServiceHandle")
            .finish_non_exhaustive()
    }
}
impl PlatformServiceHandle {
    pub fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities::current()
    }
    pub fn submit(&self, request: PlatformServiceRequest) -> ServiceResult<ServiceRequestId> {
        let shared = self.shared.upgrade().ok_or_else(closed)?;
        let bytes = request.validate()?;
        let id = {
            let mut state = shared.state.lock().map_err(|_| closed())?;
            if !state.active {
                return Err(closed());
            }
            if state.pending.len() >= MAX_REQUESTS || bytes > MAX_BYTES.saturating_sub(state.bytes)
            {
                return Err(ServiceError::new(
                    ServiceErrorKind::Capacity,
                    "Service queue is full",
                ));
            }
            let next = state.next.checked_add(1).ok_or_else(|| {
                ServiceError::new(ServiceErrorKind::Capacity, "Service request IDs exhausted")
            })?;
            let id = ServiceRequestId {
                owner: Arc::downgrade(&shared.identity),
                serial: state.next,
            };
            state.next = next;
            state.bytes += bytes;
            state.pending.push_back(Pending {
                id: id.clone(),
                request,
                bytes,
            });
            id
        };
        if !shared.waker.wake() {
            if let Ok(mut state) = shared.state.lock() {
                state.active = false;
                state.pending.clear();
                state.bytes = 0;
            }
            return Err(closed());
        }
        Ok(id)
    }
    /// True only when a queued request was removed before execution. No
    /// completion is delivered for it. A running OS operation cannot be undone.
    pub fn cancel(&self, id: &ServiceRequestId) -> bool {
        let Some(shared) = self.shared.upgrade() else {
            return false;
        };
        if !id.owner.ptr_eq(&Arc::downgrade(&shared.identity)) {
            return false;
        }
        let Ok(mut state) = shared.state.lock() else {
            return false;
        };
        let Some(index) = state.pending.iter().position(|v| v.id == *id) else {
            return false;
        };
        if let Some(pending) = state.pending.remove(index) {
            state.bytes -= pending.bytes;
            true
        } else {
            false
        }
    }
}
fn closed() -> ServiceError {
    ServiceError::new(ServiceErrorKind::OwnerClosed, "Service owner is closed")
}
pub(super) struct ServiceQueue {
    shared: Arc<Shared>,
}
impl ServiceQueue {
    pub(super) fn new(waker: AppWaker) -> Self {
        Self {
            shared: Arc::new(Shared {
                identity: Arc::new(()),
                state: Mutex::new(State {
                    active: true,
                    next: 1,
                    bytes: 0,
                    pending: VecDeque::new(),
                }),
                waker,
            }),
        }
    }
    pub(super) fn handle(&self) -> PlatformServiceHandle {
        PlatformServiceHandle {
            shared: Arc::downgrade(&self.shared),
        }
    }
    pub(super) fn pop(&self) -> Option<(ServiceRequestId, PlatformServiceRequest)> {
        let mut state = self.shared.state.lock().ok()?;
        if !state.active {
            return None;
        }
        let item = state.pending.pop_front()?;
        state.bytes -= item.bytes;
        Some((item.id, item.request))
    }
    pub(super) fn close(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.active = false;
            state.pending.clear();
            state.bytes = 0;
        }
    }
}
impl Drop for ServiceQueue {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counter_overflow_and_closed_owner_preserve_the_pending_queue() {
        let queue = ServiceQueue::new(AppWaker::testing(|| true));
        let handle = queue.handle();
        let request = || PlatformServiceRequest::Dispatch {
            channel: 1,
            payload: vec![1],
        };
        let first = handle.submit(request()).unwrap();
        queue.shared.state.lock().unwrap().next = u64::MAX;
        assert_eq!(
            handle.submit(request()).unwrap_err().kind,
            ServiceErrorKind::Capacity
        );
        assert_eq!(queue.pop().unwrap().0, first);
        queue.close();
        assert_eq!(
            handle.submit(request()).unwrap_err().kind,
            ServiceErrorKind::OwnerClosed
        );
        assert!(queue.pop().is_none());
    }
    #[test]
    fn logical_byte_budget_is_released_on_cancel_and_completion() {
        let queue = ServiceQueue::new(AppWaker::testing(|| true));
        let handle = queue.handle();
        let request = || {
            PlatformServiceRequest::WriteClipboard(ClipboardContent {
                html: Some("x".repeat(4_194_304)),
                ..Default::default()
            })
        };
        let ids: Vec<_> = (0..6).map(|_| handle.submit(request()).unwrap()).collect();
        assert_eq!(
            handle.submit(request()).unwrap_err().kind,
            ServiceErrorKind::Capacity
        );
        assert!(handle.cancel(&ids[0]));
        assert!(handle.submit(request()).is_ok());
        queue.pop().unwrap();
        assert!(handle.submit(request()).is_ok());
    }
}
