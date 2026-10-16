use super::*;
use crate::{
    AsyncEditorValidator, DataValue, EditorValidationRequest, EditorValidationStatus, UiCulture,
    ValidationCancellation,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

struct Entry {
    validator: AsyncEditorValidator,
    context: DataValue,
    context_revision: u64,
    culture: UiCulture,
    generation: u64,
    pending: Option<Arc<AtomicBool>>,
    approved: bool,
}
struct Completion {
    id: WidgetId,
    generation: u64,
    draft: String,
    result: Result<(), String>,
}
pub(super) struct AsyncEditorStore {
    entries: HashMap<WidgetId, Entry>,
    sender: mpsc::SyncSender<Completion>,
    receiver: mpsc::Receiver<Completion>,
    workers: Arc<AtomicUsize>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl Default for AsyncEditorStore {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel(32);
        Self {
            entries: HashMap::new(),
            sender,
            receiver,
            workers: Arc::new(AtomicUsize::new(0)),
            wake: None,
        }
    }
}
impl Drop for AsyncEditorStore {
    fn drop(&mut self) {
        for entry in self.entries.values() {
            if let Some(cancel) = &entry.pending {
                cancel.store(true, Ordering::Release);
            }
        }
    }
}
impl Controls {
    pub fn set_async_editor_validator(
        &mut self,
        id: WidgetId,
        validator: Option<AsyncEditorValidator>,
    ) -> Result<(), ControlError> {
        self.editor_control(id)?;
        self.invalidate_async_editor(id);
        let previous = self.async_editors.entries.remove(&id);
        if let Some(validator) = validator {
            if self.async_editors.entries.len() >= 1024 {
                return Err(ControlError::Invalid(
                    "At most 1024 async editor validators may be registered".into(),
                ));
            }
            self.async_editors.entries.insert(
                id,
                Entry {
                    validator,
                    context: previous
                        .as_ref()
                        .map_or(DataValue::Null, |e| e.context.clone()),
                    context_revision: previous.as_ref().map_or(0, |e| e.context_revision),
                    culture: previous.as_ref().map_or(UiCulture::Russian, |e| e.culture),
                    generation: NEXT_REQUEST.fetch_add(1, Ordering::Relaxed),
                    pending: None,
                    approved: false,
                },
            );
        }
        Ok(())
    }
    /// Set an immutable cross-field snapshot. Changing revision, value or culture
    /// cancels pending validation without touching draft, accepted value or focus.
    pub fn set_editor_validation_context(
        &mut self,
        id: WidgetId,
        context: DataValue,
        revision: u64,
        culture: UiCulture,
    ) -> Result<(), ControlError> {
        context
            .validate()
            .map_err(|e| ControlError::Invalid(e.to_string()))?;
        let bytes = context_cost(&context)
            + self
                .async_editors
                .entries
                .iter()
                .filter(|(key, _)| **key != id)
                .map(|(_, entry)| context_cost(&entry.context))
                .sum::<usize>();
        if bytes > 64 * 1024 * 1024 {
            return Err(ControlError::Invalid(
                "Async validation contexts exceed the 64 MiB retained budget".into(),
            ));
        }
        let entry =
            self.async_editors.entries.get(&id).ok_or_else(|| {
                ControlError::Invalid("Editor has no asynchronous validator".into())
            })?;
        if entry.context != context
            || entry.context_revision != revision
            || entry.culture != culture
        {
            self.invalidate_async_editor(id);
            if let Some(control) = self.controls.get_mut(&id) {
                control.validation_error = None;
                self.scene_dirty = true;
            }
            let entry = self.async_editors.entries.get_mut(&id).unwrap();
            entry.context = context.clone();
            entry.context_revision = revision;
            entry.culture = culture;
        }
        Ok(())
    }
    /// The callback only schedules a UI-thread update. Polling and acceptance
    /// happen in `Controls::update` or an explicit `poll_editor_validation` call.
    pub fn set_editor_validation_waker(&mut self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        self.async_editors.wake = wake;
    }
    pub fn has_async_editor_validator(&self, id: WidgetId) -> bool {
        self.async_editors.entries.contains_key(&id)
    }
    pub fn invalidate_all_async_editor_validations(&mut self) {
        let ids: Vec<_> = self.async_editors.entries.keys().copied().collect();
        for id in ids {
            self.invalidate_async_editor(id);
        }
    }
    pub fn editor_validation_status(&self, id: WidgetId) -> EditorValidationStatus {
        if self
            .async_editors
            .entries
            .get(&id)
            .is_some_and(|e| e.pending.is_some())
        {
            EditorValidationStatus::Pending
        } else {
            EditorValidationStatus::Idle
        }
    }
    pub fn invalidate_async_editor(&mut self, id: WidgetId) {
        if let Some(entry) = self.async_editors.entries.get_mut(&id) {
            if let Some(cancel) = entry.pending.take() {
                cancel.store(true, Ordering::Release);
            }
            entry.generation = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
            entry.approved = false;
        }
    }
    pub(super) fn begin_async_editor_commit(
        &mut self,
        id: WidgetId,
        draft: &str,
    ) -> Result<Option<CommitOutcome>, ControlError> {
        let Some(entry) = self.async_editors.entries.get_mut(&id) else {
            return Ok(None);
        };
        if entry.approved {
            entry.approved = false;
            return Ok(None);
        }
        if entry.pending.is_some() {
            return Ok(Some(CommitOutcome::Pending));
        }
        if self.async_editors.workers.load(Ordering::Acquire) >= 16 {
            return Err(ControlError::Invalid(
                "Asynchronous editor validation worker limit (16) reached".into(),
            ));
        }
        entry.generation = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        let generation = entry.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        let request = EditorValidationRequest {
            draft: draft.into(),
            context: entry.context.clone(),
            context_revision: entry.context_revision,
            culture: entry.culture,
            cancellation: ValidationCancellation(Arc::clone(&cancel)),
        };
        let validator = entry.validator.clone();
        let sender = self.async_editors.sender.clone();
        let wake = self.async_editors.wake.clone();
        let workers = Arc::clone(&self.async_editors.workers);
        workers.fetch_add(1, Ordering::AcqRel);
        let started = std::thread::Builder::new()
            .name("ui-editor-validation".into())
            .spawn(move || {
                struct WorkerGuard(Arc<AtomicUsize>);
                impl Drop for WorkerGuard {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::AcqRel);
                    }
                }
                let _guard = WorkerGuard(workers);
                let draft = request.draft.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (validator.0)(request)
                }))
                .unwrap_or_else(|_| Err("Asynchronous editor validator panicked".into()))
                .map_err(|message| {
                    let text = message
                        [..message.floor_char_boundary(crate::MAX_VALIDATION_MESSAGE_BYTES)]
                        .trim();
                    if text.is_empty() {
                        "The value is invalid.".into()
                    } else {
                        text.to_owned()
                    }
                });
                if sender
                    .send(Completion {
                        id,
                        generation,
                        draft,
                        result,
                    })
                    .is_ok()
                    && let Some(wake) = wake
                {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wake()));
                }
            });
        if let Err(error) = started {
            self.async_editors.workers.fetch_sub(1, Ordering::AcqRel);
            return Err(ControlError::Invalid(format!(
                "Cannot start editor validator: {error}"
            )));
        }
        entry.pending = Some(cancel);
        Ok(Some(CommitOutcome::Pending))
    }
    pub fn poll_editor_validation(&mut self) -> Result<usize, ControlError> {
        let stale: Vec<_> = self
            .async_editors
            .entries
            .keys()
            .copied()
            .filter(|id| self.runtime.tree().node(*id).is_none() || self.document(*id).is_none())
            .collect();
        for id in stale {
            self.invalidate_async_editor(id);
            self.async_editors.entries.remove(&id);
        }
        let mut applied = 0;
        // At most 16 in-flight workers can complete; bound stale completion work too.
        for _ in 0..1024 {
            let Ok(completion) = self.async_editors.receiver.try_recv() else {
                break;
            };
            let current = self
                .async_editors
                .entries
                .get(&completion.id)
                .is_some_and(|e| {
                    e.generation == completion.generation
                        && e.pending
                            .as_ref()
                            .is_some_and(|c| !c.load(Ordering::Acquire))
                })
                && self
                    .document(completion.id)
                    .is_some_and(|d| d.text() == completion.draft && d.composition().is_none());
            if !current {
                if self
                    .async_editors
                    .entries
                    .get(&completion.id)
                    .is_some_and(|entry| entry.generation == completion.generation)
                {
                    self.invalidate_async_editor(completion.id);
                }
                continue;
            }
            let entry = self.async_editors.entries.get_mut(&completion.id).unwrap();
            entry.pending = None;
            match completion.result {
                Ok(()) => {
                    entry.approved = true;
                    let outcome = self.commit_edit(completion.id);
                    if let Some(entry) = self.async_editors.entries.get_mut(&completion.id) {
                        entry.approved = false;
                    }
                    let _ = outcome?;
                }
                Err(message) => {
                    self.controls
                        .get_mut(&completion.id)
                        .unwrap()
                        .validation_error = Some(message.clone());
                    self.edit_events.push(EditEvent::ValidationFailed {
                        id: completion.id,
                        message,
                    });
                    self.scene_dirty = true;
                }
            }
            applied += 1;
        }
        Ok(applied)
    }
}

fn context_cost(value: &DataValue) -> usize {
    let mut pending = vec![value];
    let mut bytes = 0usize;
    while let Some(value) = pending.pop() {
        bytes = bytes.saturating_add(64);
        match value {
            DataValue::String(text) => bytes = bytes.saturating_add(text.len()),
            DataValue::List(values) => pending.extend(values),
            DataValue::Object(values) => {
                bytes = bytes.saturating_add(values.keys().map(String::len).sum::<usize>());
                pending.extend(values.values());
            }
            _ => {}
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Mutex, mpsc},
        time::{Duration, Instant},
    };
    fn editor() -> (Controls, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .edit_box(ui.root(), "Name", "Accepted", LayoutStyle::default())
            .unwrap();
        (ui, id)
    }
    fn gate(ui: &mut Controls, id: WidgetId, outcome: Result<(), String>) -> mpsc::Sender<()> {
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        ui.set_async_editor_validator(
            id,
            Some(AsyncEditorValidator::new(move |_| {
                receiver.lock().unwrap().recv().unwrap();
                outcome.clone()
            })),
        )
        .unwrap();
        sender
    }
    fn drain(ui: &mut Controls) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while ui.async_editors.workers.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
            ui.poll_editor_validation().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        ui.poll_editor_validation().unwrap();
        assert_eq!(ui.async_editors.workers.load(Ordering::Acquire), 0);
    }
    #[test]
    fn pending_validation_accepts_once_on_ui_thread_and_wakes() {
        let (mut ui, id) = editor();
        let release = gate(&mut ui, id, Ok(()));
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&wakes);
        ui.set_editor_validation_waker(Some(Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })));
        ui.set_text(id, "Draft").unwrap();
        assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Pending);
        assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Pending);
        assert_eq!(ui.accepted_value(id), Some("Accepted"));
        release.send(()).unwrap();
        drain(&mut ui);
        assert_eq!(ui.accepted_value(id), Some("Draft"));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(
            ui.drain_edit_events()
                .iter()
                .filter(|e| matches!(e, EditEvent::Committed { .. }))
                .count(),
            1
        );
    }
    #[test]
    fn changed_then_restored_draft_rejects_old_completion() {
        let (mut ui, id) = editor();
        let release = gate(&mut ui, id, Ok(()));
        ui.set_text(id, "Draft").unwrap();
        ui.commit_edit(id).unwrap();
        ui.set_text(id, "Other").unwrap();
        ui.set_text(id, "Draft").unwrap();
        release.send(()).unwrap();
        drain(&mut ui);
        assert_eq!(ui.accepted_value(id), Some("Accepted"));
        assert_eq!(
            ui.editor_validation_status(id),
            EditorValidationStatus::Idle
        );
    }
    #[test]
    fn culture_context_cancel_and_replacement_invalidate_requests() {
        for operation in 0..4 {
            let (mut ui, id) = editor();
            let release = gate(&mut ui, id, Ok(()));
            ui.set_text(id, "Draft").unwrap();
            ui.commit_edit(id).unwrap();
            match operation {
                0 => ui
                    .set_editor_validation_context(id, DataValue::Null, 0, UiCulture::English)
                    .unwrap(),
                1 => ui
                    .set_editor_validation_context(
                        id,
                        DataValue::from("changed"),
                        1,
                        UiCulture::Russian,
                    )
                    .unwrap(),
                2 => {
                    ui.cancel_edit(id).unwrap();
                }
                _ => {
                    ui.set_async_editor_validator(id, None).unwrap();
                    ui.set_async_editor_validator(id, Some(AsyncEditorValidator::new(|_| Ok(()))))
                        .unwrap();
                }
            }
            release.send(()).unwrap();
            drain(&mut ui);
            assert_eq!(ui.accepted_value(id), Some("Accepted"));
        }
    }
    #[test]
    fn failure_is_bounded_preserves_draft_and_revalidates_sync_policy() {
        let (mut ui, id) = editor();
        let release = gate(&mut ui, id, Err("x".repeat(8192)));
        ui.set_text(id, "Draft").unwrap();
        ui.commit_edit(id).unwrap();
        release.send(()).unwrap();
        drain(&mut ui);
        assert_eq!(ui.accepted_value(id), Some("Accepted"));
        assert_eq!(ui.document(id).unwrap().text(), "Draft");
        assert_eq!(ui.validation_error(id).unwrap().len(), 4096);
        let release = gate(&mut ui, id, Ok(()));
        ui.commit_edit(id).unwrap();
        ui.set_editor_validator(
            id,
            Some(EditorValidator::new(|_| Err("Changed rule".into()))),
        )
        .unwrap();
        release.send(()).unwrap();
        drain(&mut ui);
        assert_eq!(ui.accepted_value(id), Some("Accepted"));
    }
    #[test]
    fn disposed_control_cannot_receive_completion() {
        let (mut ui, id) = editor();
        let release = gate(&mut ui, id, Ok(()));
        ui.set_text(id, "Draft").unwrap();
        ui.commit_edit(id).unwrap();
        ui.remove(id).unwrap();
        release.send(()).unwrap();
        drain(&mut ui);
        assert!(!ui.has_async_editor_validator(id));
    }
}
