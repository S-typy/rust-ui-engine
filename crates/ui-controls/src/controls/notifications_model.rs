//! Bounded notification requests and explicit dismissal history.
use super::*;
use std::time::Duration;

pub const MAX_PENDING_NOTIFICATIONS: usize = 32;
pub const MAX_NOTIFICATION_HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationSeverity {
    #[default]
    Information,
    Success,
    Warning,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationAction {
    pub label: String,
    pub command: CommandId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSpec {
    pub title: String,
    pub message: String,
    pub severity: NotificationSeverity,
    /// Expiration begins when the card first becomes visible, not while queued.
    pub timeout: Option<Duration>,
    pub actions: Vec<NotificationAction>,
}
impl NotificationSpec {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            severity: NotificationSeverity::Information,
            timeout: Some(Duration::from_secs(5)),
            actions: Vec::new(),
        }
    }
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        if self.title.len() > 128
            || self.message.len() > 1024
            || self.actions.len() > 3
            || self
                .actions
                .iter()
                .any(|a| a.label.is_empty() || a.label.len() > 64)
            || self
                .timeout
                .is_some_and(|t| t < Duration::from_millis(100) || t > Duration::from_secs(86400))
        {
            return Err(invalid(
                "Notification exceeds title/message/action/timeout limits",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotificationCenterSettings {
    pub width: f32,
    pub max_visible: usize,
}
impl Default for NotificationCenterSettings {
    fn default() -> Self {
        Self {
            width: 360.0,
            max_visible: 3,
        }
    }
}
impl NotificationCenterSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if !self.width.is_finite()
            || !(240.0..=640.0).contains(&self.width)
            || !(1..=3).contains(&self.max_visible)
        {
            Err(invalid(
                "Notification width must be 240..640 and visible count 1..3",
            ))
        } else {
            Ok(())
        }
    }
}
/// Belongs to one retained center; IDs are never reused by that center.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NotificationId {
    pub(super) owner: WidgetId,
    pub(super) serial: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationCloseReason {
    Dismissed,
    Expired,
    Action(CommandId),
    OwnerUnavailable,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationRecord {
    pub id: NotificationId,
    pub notification: NotificationSpec,
    pub reason: NotificationCloseReason,
}
pub(super) fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
