//! Bounded workflow definitions and application-approved transitions.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_WIZARD_STEPS: usize = 64;
const MAX_EVENTS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WizardStep {
    pub id: String,
    pub title: String,
    pub next: Vec<String>,
    pub is_final: bool,
}
impl WizardStep {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            next: Vec::new(),
            is_final: false,
        }
    }
}

/// A bounded directed acyclic workflow. Back follows accepted history rather
/// than graph edges. Every declared step is reachable from `start`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WizardModel {
    steps: Vec<WizardStep>,
    start: String,
}
impl WizardModel {
    pub fn new(steps: Vec<WizardStep>, start: impl Into<String>) -> Result<Self, ControlError> {
        let start = start.into();
        if !(1..=MAX_WIZARD_STEPS).contains(&steps.len()) {
            return Err(invalid("Wizard requires 1..64 steps"));
        }
        let mut ids = HashMap::new();
        let mut bytes = 0usize;
        for (index, step) in steps.iter().enumerate() {
            identifier(&step.id)?;
            label(&step.title)?;
            if ids.insert(step.id.as_str(), index).is_some() {
                return Err(invalid("Wizard step IDs must be unique"));
            }
            if step.next.len() > MAX_WIZARD_STEPS || step.is_final != step.next.is_empty() {
                return Err(invalid(
                    "Final steps have no outgoing targets; other steps require at least one",
                ));
            }
            bytes +=
                step.id.len() + step.title.len() + step.next.iter().map(String::len).sum::<usize>();
        }
        if bytes > 32_768 {
            return Err(invalid("Wizard metadata exceeds 32 KiB"));
        }
        let Some(&start_index) = ids.get(start.as_str()) else {
            return Err(invalid("Unknown initial wizard step"));
        };
        let mut edges = vec![Vec::new(); steps.len()];
        let mut indegrees = vec![0usize; steps.len()];
        for (index, step) in steps.iter().enumerate() {
            let mut unique = HashSet::new();
            for target in &step.next {
                let Some(&target_index) = ids.get(target.as_str()) else {
                    return Err(invalid("Unknown wizard branch target"));
                };
                if !unique.insert(target) || index == target_index {
                    return Err(invalid("Duplicate or self-referencing wizard target"));
                }
                edges[index].push(target_index);
                indegrees[target_index] += 1;
            }
        }
        let mut ready: Vec<_> = indegrees
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (*n == 0).then_some(i))
            .collect();
        let mut visited = 0;
        while let Some(index) = ready.pop() {
            visited += 1;
            for &target in &edges[index] {
                indegrees[target] -= 1;
                if indegrees[target] == 0 {
                    ready.push(target);
                }
            }
        }
        if visited != steps.len() {
            return Err(invalid("Wizard next-target graph must be acyclic"));
        }
        let mut reachable = HashSet::new();
        let mut pending = vec![start_index];
        while let Some(index) = pending.pop() {
            if reachable.insert(index) {
                pending.extend(&edges[index]);
            }
        }
        if reachable.len() != steps.len() {
            return Err(invalid(
                "Every wizard step must be reachable from the initial step",
            ));
        }
        Ok(Self { steps, start })
    }
    pub fn steps(&self) -> &[WizardStep] {
        &self.steps
    }
    pub fn start(&self) -> &str {
        &self.start
    }
    pub fn step(&self, id: &str) -> Option<&WizardStep> {
        self.steps.iter().find(|step| step.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardIntent {
    Next,
    Back,
    Finish,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardResult {
    Finished,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WizardStatus {
    Ready,
    Pending(WizardIntent),
    Error(String),
    Finished,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WizardDecision {
    Step(String),
    Finish,
    Cancel,
    Reject(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardCompletion {
    Applied,
    Rejected,
    Stale,
}

#[derive(Debug)]
struct RequestIdentity {
    owner: WidgetId,
    generation: u64,
    context_version: u64,
    step: String,
    intent: WizardIntent,
    targets: Vec<String>,
    cancelled: AtomicBool,
}
#[derive(Clone, Debug)]
pub struct WizardRequest(Arc<RequestIdentity>);
impl WizardRequest {
    pub fn owner(&self) -> WidgetId {
        self.0.owner
    }
    pub fn generation(&self) -> u64 {
        self.0.generation
    }
    pub fn context_version(&self) -> u64 {
        self.0.context_version
    }
    pub fn step(&self) -> &str {
        &self.0.step
    }
    pub fn intent(&self) -> WizardIntent {
        self.0.intent
    }
    pub fn targets(&self) -> &[String] {
        &self.0.targets
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WizardEventKind {
    StepChanged {
        from: String,
        to: String,
        intent: WizardIntent,
    },
    Rejected {
        step: String,
        message: String,
    },
    Finished,
    Cancelled,
    Reset,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WizardEvent {
    pub owner: WidgetId,
    pub kind: WizardEventKind,
}

pub(super) struct WizardSession {
    pub model: WizardModel,
    pub history: Vec<String>,
    pub status: WizardStatus,
    pub result: Option<WizardResult>,
    pub events: Vec<WizardEventKind>,
    generation: u64,
    pending: Option<WizardRequest>,
    outgoing: bool,
}
impl WizardSession {
    pub fn new(model: WizardModel) -> Self {
        Self {
            history: vec![model.start.clone()],
            model,
            status: WizardStatus::Ready,
            result: None,
            events: Vec::new(),
            generation: 0,
            pending: None,
            outgoing: false,
        }
    }
    pub fn current(&self) -> &str {
        self.history.last().unwrap()
    }
    pub fn can_request(&self, intent: WizardIntent) -> bool {
        self.result.is_none()
            && (self.pending.is_none() || intent == WizardIntent::Cancel)
            && match intent {
                WizardIntent::Next => !self.model.step(self.current()).unwrap().is_final,
                WizardIntent::Back => self.history.len() > 1,
                WizardIntent::Finish => self.model.step(self.current()).unwrap().is_final,
                WizardIntent::Cancel => true,
            }
    }
    pub fn request(
        &mut self,
        owner: WidgetId,
        intent: WizardIntent,
        context_version: u64,
    ) -> Result<(), ControlError> {
        if !self.can_request(intent) {
            return Err(invalid(
                "Wizard action is unavailable for its current step or pending state",
            ));
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("Wizard request generation exhausted"))?;
        let targets = match intent {
            WizardIntent::Next => self.model.step(self.current()).unwrap().next.clone(),
            WizardIntent::Back => vec![self.history[self.history.len() - 2].clone()],
            _ => Vec::new(),
        };
        let request = WizardRequest(Arc::new(RequestIdentity {
            owner,
            generation,
            context_version,
            step: self.current().into(),
            intent,
            targets,
            cancelled: AtomicBool::new(false),
        }));
        self.cancel_pending();
        self.generation = generation;
        self.pending = Some(request);
        self.outgoing = true;
        self.status = WizardStatus::Pending(intent);
        Ok(())
    }
    pub fn take_request(&mut self) -> Option<WizardRequest> {
        if self.outgoing {
            self.outgoing = false;
            self.pending.clone()
        } else {
            None
        }
    }
    pub fn matches(&self, request: &WizardRequest) -> bool {
        !request.is_cancelled()
            && self
                .pending
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(&p.0, &request.0))
            && request.generation() == self.generation
            && request.step() == self.current()
            && self.result.is_none()
    }
    pub fn preflight_complete(
        &self,
        request: &WizardRequest,
        decision: &WizardDecision,
    ) -> Result<(), ControlError> {
        if !self.matches(request) {
            return Err(invalid("Stale wizard request"));
        }
        self.preflight_event()?;
        let valid = match decision {
            WizardDecision::Reject(message) => {
                error_text(message)?;
                true
            }
            WizardDecision::Step(target) => {
                matches!(request.intent(), WizardIntent::Next | WizardIntent::Back)
                    && request.targets().contains(target)
            }
            WizardDecision::Finish => request.intent() == WizardIntent::Finish,
            WizardDecision::Cancel => request.intent() == WizardIntent::Cancel,
        };
        if !valid {
            return Err(invalid(
                "Wizard completion does not match its request intent and declared target",
            ));
        }
        Ok(())
    }
    pub fn publish(
        &mut self,
        request: &WizardRequest,
        decision: WizardDecision,
    ) -> WizardCompletion {
        let from = self.current().to_owned();
        let intent = request.intent();
        self.cancel_pending();
        match decision {
            WizardDecision::Step(to) => {
                if intent == WizardIntent::Back {
                    self.history.pop();
                } else {
                    self.history.push(to.clone());
                }
                self.status = WizardStatus::Ready;
                self.events
                    .push(WizardEventKind::StepChanged { from, to, intent });
            }
            WizardDecision::Finish => {
                self.result = Some(WizardResult::Finished);
                self.status = WizardStatus::Finished;
                self.events.push(WizardEventKind::Finished);
            }
            WizardDecision::Cancel => {
                self.result = Some(WizardResult::Cancelled);
                self.status = WizardStatus::Cancelled;
                self.events.push(WizardEventKind::Cancelled);
            }
            WizardDecision::Reject(message) => {
                self.status = WizardStatus::Error(message.clone());
                self.events.push(WizardEventKind::Rejected {
                    step: from,
                    message,
                });
                return WizardCompletion::Rejected;
            }
        }
        WizardCompletion::Applied
    }
    pub fn preflight_event(&self) -> Result<(), ControlError> {
        if self.events.len() >= MAX_EVENTS {
            Err(invalid("Drain wizard events before another transition"))
        } else {
            Ok(())
        }
    }
    pub fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.0.cancelled.store(true, Ordering::Release);
        }
        self.outgoing = false;
        if matches!(self.status, WizardStatus::Pending(_)) {
            self.status = WizardStatus::Ready;
        }
    }
    pub fn reset(&mut self) {
        self.cancel_pending();
        self.history = vec![self.model.start.clone()];
        self.result = None;
        self.status = WizardStatus::Ready;
        self.events.push(WizardEventKind::Reset);
    }
}
impl Drop for WizardSession {
    fn drop(&mut self) {
        self.cancel_pending();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Upcoming,
    Current,
    Completed,
    Error,
    Skipped,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepProgressItem {
    pub id: String,
    pub title: String,
    pub state: StepState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepProgressValue {
    pub steps: Vec<StepProgressItem>,
    pub current: String,
}
impl StepProgressValue {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !(1..=MAX_WIZARD_STEPS).contains(&self.steps.len()) {
            return Err(invalid("StepProgress requires 1..64 steps"));
        }
        let mut ids = HashSet::new();
        let mut bytes = 0;
        for step in &self.steps {
            identifier(&step.id)?;
            label(&step.title)?;
            if !ids.insert(&step.id) {
                return Err(invalid("StepProgress IDs must be unique"));
            }
            if matches!(step.state, StepState::Current | StepState::Error)
                && step.id != self.current
            {
                return Err(invalid(
                    "Only the current step may have Current or Error state",
                ));
            }
            bytes += step.id.len() + step.title.len();
        }
        if bytes > 32_768
            || !self.steps.iter().any(|s| {
                s.id == self.current && !matches!(s.state, StepState::Upcoming | StepState::Skipped)
            })
        {
            return Err(invalid(
                "StepProgress requires an existing current/completed/error step and bounded metadata",
            ));
        }
        Ok(())
    }
}
pub(super) fn identifier(text: &str) -> Result<(), ControlError> {
    if text.is_empty() || text.len() > 128 || text.chars().any(char::is_control) {
        Err(invalid(
            "Step ID requires 1..128 UTF-8 bytes without controls",
        ))
    } else {
        Ok(())
    }
}
pub(super) fn label(text: &str) -> Result<(), ControlError> {
    if text.trim().is_empty()
        || text.len() > 512
        || (TextLimits {
            max_graphemes: Some(128),
            max_bytes: 512,
        })
        .validate(text)
        .is_err()
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        Err(invalid(
            "Step title requires 1..128 graphemes / 512 UTF-8 bytes without line breaks",
        ))
    } else {
        Ok(())
    }
}
pub(super) fn error_text(text: &str) -> Result<(), ControlError> {
    if text.trim().is_empty()
        || text.len() > 2048
        || (TextLimits {
            max_graphemes: Some(512),
            max_bytes: 2048,
        })
        .validate(text)
        .is_err()
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        Err(invalid(
            "Wizard error requires 1..512 graphemes / 2048 bytes without line breaks",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn invalid(text: impl Into<String>) -> ControlError {
    ControlError::Invalid(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> WizardSession {
        WizardSession::new(
            WizardModel::new(
                vec![WizardStep {
                    id: "done".into(),
                    title: "Done".into(),
                    next: vec![],
                    is_final: true,
                }],
                "done",
            )
            .unwrap(),
        )
    }

    #[test]
    fn wizard_request_generation_overflow_preserves_existing_cancelable_request() {
        let owner = rust_desktop_ui_core::UiTree::new().root();
        let mut state = session();
        state.generation = u64::MAX - 1;
        state.request(owner, WizardIntent::Finish, 8).unwrap();
        let request = state.take_request().unwrap();
        assert_eq!(request.generation(), u64::MAX);
        assert!(state.request(owner, WizardIntent::Cancel, 9).is_err());
        assert!(state.matches(&request));
        assert!(!request.is_cancelled());
        assert_eq!(state.status, WizardStatus::Pending(WizardIntent::Finish));
        state
            .preflight_complete(&request, &WizardDecision::Finish)
            .unwrap();
        state.publish(&request, WizardDecision::Finish);
        assert_eq!(state.result, Some(WizardResult::Finished));
    }

    #[test]
    fn wizard_event_backpressure_and_foreign_identity_are_atomic() {
        let owner = rust_desktop_ui_core::UiTree::new().root();
        let mut state = session();
        let mut foreign = session();
        state.request(owner, WizardIntent::Finish, 1).unwrap();
        foreign.request(owner, WizardIntent::Finish, 1).unwrap();
        let request = state.take_request().unwrap();
        let other = foreign.take_request().unwrap();
        assert!(!state.matches(&other));
        state.events = vec![WizardEventKind::Reset; MAX_EVENTS];
        assert!(
            state
                .preflight_complete(&request, &WizardDecision::Finish)
                .is_err()
        );
        assert!(state.matches(&request));
        assert_eq!(state.current(), "done");
        assert_eq!(state.result, None);
        assert_eq!(state.events.len(), MAX_EVENTS);
        state.events.clear();
        state
            .preflight_complete(&request, &WizardDecision::Finish)
            .unwrap();
        state.publish(&request, WizardDecision::Finish);
        assert_eq!(state.events, vec![WizardEventKind::Finished]);
        assert!(!state.matches(&request));
    }
}
