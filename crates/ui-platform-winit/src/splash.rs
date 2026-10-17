//! Bounded worker reporting for a real, non-activating owned splash window.
//!
//! A session has one lifetime. Progress is coalesced and terminal completion is
//! accepted once. Dropping the UI session cancels outstanding work; it never
//! joins a worker or waits for application code. The application retains its
//! native view until `DesktopApplication::window_closed` as usual.
use crate::{AppWaker, RunOptions, WindowKey, WindowSpec};
use std::sync::{Arc, Mutex};

/// Latest worker result. `None` progress denotes an indeterminate operation.
#[derive(Clone, Debug, PartialEq)]
pub enum SplashState {
    Running {
        message: String,
        progress: Option<f64>,
    },
    Succeeded,
    Failed(String),
    Cancelled,
}
impl SplashState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

struct State {
    value: SplashState,
    waker: Option<AppWaker>,
}

/// UI-side owner of a single loading operation and its native splash lifetime.
pub struct SplashSession(Arc<Mutex<State>>);

/// Thread-safe reporting handle. It cannot reopen a completed/cancelled session.
#[derive(Clone)]
pub struct SplashWorker(Arc<Mutex<State>>);

impl SplashSession {
    pub fn new(message: impl Into<String>) -> Result<(Self, SplashWorker), String> {
        let message = message.into();
        validate_message(&message)?;
        let state = Arc::new(Mutex::new(State {
            value: SplashState::Running {
                message,
                progress: None,
            },
            waker: None,
        }));
        Ok((Self(state.clone()), SplashWorker(state)))
    }

    pub fn state(&self) -> SplashState {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .value
            .clone()
    }

    /// Connect to the owning UI window. Installing a handle also wakes it so
    /// progress published before native creation cannot be stranded.
    pub fn set_waker(&self, waker: Option<AppWaker>) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).waker = waker.clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Complete cancellation once. Worker results arriving later are ignored.
    pub fn cancel(&self) -> bool {
        publish(&self.0, SplashState::Cancelled)
    }

    /// A real owned, nonmodal, non-activating native window while work is live.
    /// The caller supplies a unique key and retains its view through destruction.
    pub fn window_spec(
        &self,
        key: WindowKey,
        owner: WindowKey,
        options: RunOptions,
    ) -> Result<Option<WindowSpec>, String> {
        if key == WindowKey::MAIN || key == owner {
            return Err("Splash requires a distinct non-main window key".into());
        }
        if !options.width.is_finite()
            || !options.height.is_finite()
            || options.width <= 0.0
            || options.height <= 0.0
            || options.min_size.is_some_and(|size| !size.is_valid())
        {
            return Err("Invalid splash window size".into());
        }
        if !self.state().is_running() {
            return Ok(None);
        }
        let mut spec = WindowSpec::new(key, options);
        spec.owner = Some(owner);
        spec.activate = false;
        spec.modal = false;
        Ok(Some(spec))
    }
}
impl Drop for SplashSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl SplashWorker {
    /// Latest state only is stored: reporting progress cannot grow a queue.
    pub fn report(
        &self,
        message: impl Into<String>,
        progress: Option<f64>,
    ) -> Result<bool, String> {
        if self.is_finished() {
            return Ok(false);
        }
        let message = message.into();
        validate_message(&message)?;
        if progress.is_some_and(|n| !n.is_finite() || !(0.0..=1.0).contains(&n)) {
            return Err("Splash progress must be finite and between zero and one".into());
        }
        Ok(publish(&self.0, SplashState::Running { message, progress }))
    }
    pub fn succeed(&self) -> bool {
        publish(&self.0, SplashState::Succeeded)
    }
    pub fn fail(&self, message: impl Into<String>) -> Result<bool, String> {
        if self.is_finished() {
            return Ok(false);
        }
        let message = message.into();
        validate_message(&message)?;
        Ok(publish(&self.0, SplashState::Failed(message)))
    }
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self.0.lock().unwrap_or_else(|p| p.into_inner()).value,
            SplashState::Cancelled
        )
    }
    pub fn is_finished(&self) -> bool {
        !self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .value
            .is_running()
    }
}

fn publish(shared: &Mutex<State>, next: SplashState) -> bool {
    let waker = {
        let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
        if !state.value.is_running() {
            return false;
        }
        state.value = next;
        state.waker.clone()
    };
    // Never invoke native event dispatch while holding the state lock.
    if let Some(waker) = waker {
        waker.wake();
    }
    true
}
fn validate_message(message: &str) -> Result<(), String> {
    if message.len() > 4096
        || message
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        Err("Splash message is limited to 4096 UTF-8 bytes without control characters".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_results_and_cancel_are_once_and_sessions_are_isolated() {
        let (first, old) = SplashSession::new("First").unwrap();
        assert!(old.report("Half", Some(0.5)).unwrap());
        assert!(first.cancel());
        assert!(old.is_cancelled());
        assert!(!old.succeed());
        assert!(!old.fail("late error").unwrap());
        assert!(!old.fail("x".repeat(5000)).unwrap());
        assert!(!old.report("bad\0", Some(f64::NAN)).unwrap());
        let (next, fresh) = SplashSession::new("Next").unwrap();
        assert!(!old.report("stale", None).unwrap());
        assert!(next.state().is_running());
        assert!(fresh.succeed());
        assert!(!next.cancel());
        assert_eq!(next.state(), SplashState::Succeeded);
    }
    #[test]
    fn owner_drop_cancels_without_joining_worker() {
        let (session, worker) = SplashSession::new("Work").unwrap();
        let retained_worker = worker.clone();
        drop(session);
        assert!(retained_worker.is_cancelled());
        let thread = std::thread::spawn(move || worker.fail("late").unwrap());
        assert!(!thread.join().unwrap());
    }
    #[test]
    fn malformed_progress_is_atomic_and_latest_only_is_coalesced() {
        let (session, worker) = SplashSession::new("Work").unwrap();
        for i in 0..10000 {
            assert!(worker.report("Work", Some(i as f64 / 10000.0)).unwrap());
        }
        let before = session.state();
        for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert!(worker.report("bad", Some(value)).is_err());
        }
        assert!(worker.fail("x".repeat(4097)).is_err());
        assert_eq!(session.state(), before);
        assert!(worker.fail("Failed safely").unwrap());
        assert_eq!(session.state(), SplashState::Failed("Failed safely".into()));
    }
    #[test]
    fn splash_spec_is_owned_nonmodal_nonactivating_and_disappears_at_completion() {
        let (session, worker) = SplashSession::new("Work").unwrap();
        let spec = session
            .window_spec(WindowKey(1), WindowKey::MAIN, RunOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(spec.owner, Some(WindowKey::MAIN));
        assert!(!spec.modal && !spec.activate);
        assert!(
            session
                .window_spec(WindowKey::MAIN, WindowKey::MAIN, RunOptions::default())
                .is_err()
        );
        assert!(worker.succeed());
        assert!(
            session
                .window_spec(WindowKey(1), WindowKey::MAIN, RunOptions::default())
                .unwrap()
                .is_none()
        );
    }
}
