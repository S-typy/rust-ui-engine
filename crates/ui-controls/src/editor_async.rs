//! Cooperative asynchronous validation with immutable request snapshots.
use crate::{DataValue, UiCulture};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug)]
pub struct ValidationCancellation(pub(crate) Arc<AtomicBool>);
impl ValidationCancellation {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
/// A worker never receives controls or a mutable model. The host explicitly
/// supplies any cross-field context; its revision and culture invalidate requests.
#[derive(Clone, Debug)]
pub struct EditorValidationRequest {
    pub draft: String,
    pub context: DataValue,
    pub context_revision: u64,
    pub culture: UiCulture,
    pub cancellation: ValidationCancellation,
}
type AsyncValidator = dyn Fn(EditorValidationRequest) -> Result<(), String> + Send + Sync;
#[derive(Clone)]
pub struct AsyncEditorValidator(pub(crate) Arc<AsyncValidator>);
impl AsyncEditorValidator {
    pub fn new(
        validate: impl Fn(EditorValidationRequest) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(validate))
    }
}
impl fmt::Debug for AsyncEditorValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AsyncEditorValidator(..)")
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorValidationStatus {
    #[default]
    Idle,
    Pending,
}
