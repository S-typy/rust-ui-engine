//! Single-pointer drag negotiation independent of native window APIs.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{Point, WidgetId};

static NEXT_DRAG: AtomicU64 = AtomicU64::new(1);

/// Maximum owned binary payload for one drag gesture.
pub const MAX_DRAG_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Maximum UTF-8 bytes in an application-defined payload format name.
pub const MAX_DRAG_FORMAT_BYTES: usize = 256;
/// Bounded buffered metadata. Consumers drain it using `take_events`.
pub const MAX_DRAG_EVENTS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DragId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropEffect {
    Copy,
    Move,
    Link,
}

/// The source's supported operations. A target cannot negotiate another effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropEffects {
    pub copy: bool,
    pub move_item: bool,
    pub link: bool,
}

impl DropEffects {
    pub const COPY: Self = Self {
        copy: true,
        move_item: false,
        link: false,
    };
    pub const MOVE: Self = Self {
        copy: false,
        move_item: true,
        link: false,
    };
    pub const ALL: Self = Self {
        copy: true,
        move_item: true,
        link: true,
    };
    pub fn contains(self, effect: DropEffect) -> bool {
        match effect {
            DropEffect::Copy => self.copy,
            DropEffect::Move => self.move_item,
            DropEffect::Link => self.link,
        }
    }
    pub fn is_empty(self) -> bool {
        !self.copy && !self.move_item && !self.link
    }
}

/// An owned, format-tagged value. No cross-process or OS clipboard transport is
/// implied. Applications define the payload format and decode it at the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragPayload {
    pub format: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragCancelReason {
    Explicit,
    Escape,
    WindowBlur,
    CaptureChanged,
    SourceUnavailable,
    TargetUnavailable,
    Rejected,
    /// A deferred mutation invalidated geometry that cannot be resolved during
    /// input dispatch (for example, a layout pass is required before drop).
    GeometryUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPhase {
    Started,
    Enter,
    Over,
    Leave,
    /// Buffered hover history was compacted. Apply this event's current
    /// target/effect/position instead of replaying intermediate transitions.
    /// Direct runtime target callbacks never receive this journal-only phase.
    Resynchronized,
    Dropped,
    Cancelled(DragCancelReason),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DragEvent {
    pub id: DragId,
    pub source: WidgetId,
    pub target: Option<WidgetId>,
    pub position: Point,
    pub phase: DragPhase,
    pub effect: Option<DropEffect>,
}

#[derive(Debug, Clone)]
pub struct DragSession {
    pub id: DragId,
    pub source: WidgetId,
    pub target: Option<WidgetId>,
    pub position: Point,
    /// Immutable payload shared by session snapshots without copying bytes.
    pub payload: Arc<DragPayload>,
    pub allowed: DropEffects,
    pub effect: Option<DropEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragError {
    AlreadyDragging,
    InvalidPayload,
    InvalidPosition,
    StaleSession,
    InvalidEffect,
    /// Drain buffered events before beginning another gesture. No session was
    /// accepted, and already queued lifecycle outcomes remain intact.
    EventCapacity,
}

impl std::fmt::Display for DragError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "drag operation rejected: {self:?}")
    }
}
impl std::error::Error for DragError {}

/// Reusable negotiation state. `UiRuntime` additionally validates target/source
/// eligibility, routes target callbacks, owns capture and handles blur/removal.
/// Buffered events retain every Started and terminal result until drained.
/// Consecutive Over updates coalesce; if capacity is reached, current-gesture
/// hover history becomes an explicit Resynchronized state. A terminal slot is
/// always reserved, and a full lifecycle queue rejects the next begin.
#[derive(Debug, Default)]
pub struct DragDropService {
    active: Option<DragSession>,
    events: Vec<DragEvent>,
}

impl DragDropService {
    pub fn active(&self) -> Option<&DragSession> {
        self.active.as_ref()
    }
    pub fn take_events(&mut self) -> Vec<DragEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn begin(
        &mut self,
        source: WidgetId,
        payload: DragPayload,
        allowed: DropEffects,
        position: Point,
    ) -> Result<DragId, DragError> {
        if self.active.is_some() {
            return Err(DragError::AlreadyDragging);
        }
        if payload.format.trim().is_empty()
            || payload.format.len() > MAX_DRAG_FORMAT_BYTES
            || payload.format.contains('\0')
            || payload.data.len() > MAX_DRAG_PAYLOAD_BYTES
            || allowed.is_empty()
        {
            return Err(DragError::InvalidPayload);
        }
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(DragError::InvalidPosition);
        }
        // Started, latest hover snapshot and terminal outcome must all fit.
        if self.events.len() > MAX_DRAG_EVENTS - 3 {
            return Err(DragError::EventCapacity);
        }
        let id = DragId(
            NEXT_DRAG
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("drag identities exhausted"),
        );
        let session = DragSession {
            id,
            source,
            target: None,
            position,
            payload: Arc::new(payload),
            allowed,
            effect: None,
        };
        self.events.push(session.event(DragPhase::Started));
        self.active = Some(session);
        Ok(id)
    }

    /// Resolve one hit-test update. Rejected targets can remain the hovered
    /// target with `effect=None`; this produces a rejecting cursor, not a drop.
    pub fn hover(
        &mut self,
        id: DragId,
        target: Option<WidgetId>,
        position: Point,
        effect: Option<DropEffect>,
    ) -> Result<(), DragError> {
        let session = self
            .active
            .as_mut()
            .filter(|session| session.id == id)
            .ok_or(DragError::StaleSession)?;
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(DragError::InvalidPosition);
        }
        if effect.is_some_and(|effect| !session.allowed.contains(effect))
            || (target.is_none() && effect.is_some())
        {
            return Err(DragError::InvalidEffect);
        }
        let mut events = [None, None, None];
        let mut next_event = 0;
        session.position = position;
        if session.target != target {
            if session.target.is_some() {
                events[next_event] = Some(session.event(DragPhase::Leave));
                next_event += 1;
            }
            session.target = target;
            session.effect = effect;
            if target.is_some() {
                events[next_event] = Some(session.event(DragPhase::Enter));
                next_event += 1;
            }
        } else {
            session.effect = effect;
        }
        if target.is_some() {
            events[next_event] = Some(session.event(DragPhase::Over));
        }
        let latest = session.event(DragPhase::Resynchronized);
        self.queue_hover(events, latest);
        Ok(())
    }

    fn queue_hover(&mut self, events: [Option<DragEvent>; 3], latest: DragEvent) {
        for event in events.into_iter().flatten() {
            if event.phase == DragPhase::Over
                && let Some(previous) = self.events.last_mut()
                && previous.phase == DragPhase::Over
                && previous.id == event.id
                && previous.target == event.target
            {
                *previous = event;
            } else if self.events.len() < MAX_DRAG_EVENTS - 1 {
                // The remaining slot belongs to finish/cancel, even if the
                // application never drains pointer-motion metadata.
                self.events.push(event);
            } else {
                self.events.retain(|queued| {
                    queued.id != latest.id
                        || matches!(
                            queued.phase,
                            DragPhase::Started | DragPhase::Dropped | DragPhase::Cancelled(_)
                        )
                });
                self.events.push(latest);
                debug_assert!(self.events.len() < MAX_DRAG_EVENTS);
                return;
            }
        }
    }

    /// Consume a session exactly once. Failed negotiation is terminal and emits
    /// `Cancelled(Rejected)`; a stale/replayed ID does not modify another drag.
    pub fn finish(&mut self, id: DragId) -> Result<DragEvent, DragError> {
        let session = self
            .active
            .as_ref()
            .filter(|session| session.id == id)
            .ok_or(DragError::StaleSession)?;
        let phase = if session.target.is_some() && session.effect.is_some() {
            DragPhase::Dropped
        } else {
            DragPhase::Cancelled(DragCancelReason::Rejected)
        };
        let event = session.event(phase);
        self.active = None;
        debug_assert!(self.events.len() < MAX_DRAG_EVENTS);
        self.events.push(event.clone());
        Ok(event)
    }

    pub fn cancel(&mut self, id: DragId, reason: DragCancelReason) -> Result<DragEvent, DragError> {
        let session = self
            .active
            .as_ref()
            .filter(|session| session.id == id)
            .ok_or(DragError::StaleSession)?;
        let event = session.event(DragPhase::Cancelled(reason));
        self.active = None;
        debug_assert!(self.events.len() < MAX_DRAG_EVENTS);
        self.events.push(event.clone());
        Ok(event)
    }
}

impl DragSession {
    pub(crate) fn event(&self, phase: DragPhase) -> DragEvent {
        DragEvent {
            id: self.id,
            source: self.source,
            target: self.target,
            position: self.position,
            phase,
            effect: self.effect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiTree;

    #[test]
    fn negotiation_is_atomic_and_terminal_ids_cannot_be_replayed() {
        let tree = UiTree::new();
        let mut service = DragDropService::default();
        let id = service
            .begin(
                tree.root(),
                DragPayload {
                    format: "test".into(),
                    data: vec![],
                },
                DropEffects::COPY,
                Point::ZERO,
            )
            .unwrap();
        service.take_events();
        assert_eq!(
            service.hover(
                id,
                Some(tree.root()),
                Point::new(5.0, 5.0),
                Some(DropEffect::Move)
            ),
            Err(DragError::InvalidEffect)
        );
        assert_eq!(service.active().unwrap().position, Point::ZERO);
        assert_eq!(service.active().unwrap().target, None);
        assert!(service.take_events().is_empty());
        assert_eq!(
            service.hover(id, Some(tree.root()), Point::new(f32::NAN, 5.0), None),
            Err(DragError::InvalidPosition)
        );
        service
            .hover(
                id,
                Some(tree.root()),
                Point::new(5.0, 5.0),
                Some(DropEffect::Copy),
            )
            .unwrap();
        assert_eq!(service.finish(id).unwrap().phase, DragPhase::Dropped);
        assert_eq!(service.finish(id), Err(DragError::StaleSession));
        assert_eq!(
            service
                .take_events()
                .iter()
                .filter(|event| event.phase == DragPhase::Dropped)
                .count(),
            1
        );
    }

    #[test]
    fn sessions_are_unique_across_services_and_conflicting_begin_keeps_active_data() {
        let tree = UiTree::new();
        let mut a = DragDropService::default();
        let mut b = DragDropService::default();
        let payload = DragPayload {
            format: "test".into(),
            data: vec![7],
        };
        let first = a
            .begin(tree.root(), payload.clone(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        let second = b
            .begin(tree.root(), payload.clone(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            a.cancel(second, DragCancelReason::Explicit),
            Err(DragError::StaleSession)
        );
        assert_eq!(
            a.begin(tree.root(), payload.clone(), DropEffects::ALL, Point::ZERO),
            Err(DragError::AlreadyDragging)
        );
        assert_eq!(a.active().unwrap().id, first);
        assert_eq!(*a.active().unwrap().payload, payload);
    }

    fn payload() -> DragPayload {
        DragPayload {
            format: "application/test".into(),
            data: vec![1, 2, 3],
        }
    }

    #[test]
    fn payload_limits_are_checked_without_accepting_a_session_or_event() {
        let tree = UiTree::new();
        for payload in [
            DragPayload {
                format: "x".repeat(MAX_DRAG_FORMAT_BYTES + 1),
                data: vec![],
            },
            DragPayload {
                format: "test\0format".into(),
                data: vec![],
            },
            DragPayload {
                format: "test".into(),
                data: vec![0; MAX_DRAG_PAYLOAD_BYTES + 1],
            },
        ] {
            let mut service = DragDropService::default();
            assert_eq!(
                service.begin(tree.root(), payload, DropEffects::COPY, Point::ZERO),
                Err(DragError::InvalidPayload)
            );
            assert!(service.active().is_none());
            assert!(service.take_events().is_empty());
        }
        let mut service = DragDropService::default();
        let id = service
            .begin(
                tree.root(),
                DragPayload {
                    format: "я".repeat(MAX_DRAG_FORMAT_BYTES / 2),
                    data: vec![0; MAX_DRAG_PAYLOAD_BYTES],
                },
                DropEffects::COPY,
                Point::ZERO,
            )
            .unwrap();
        service.cancel(id, DragCancelReason::Explicit).unwrap();
    }

    #[test]
    fn session_snapshots_share_payload_without_copying_binary_data() {
        let tree = UiTree::new();
        let mut service = DragDropService::default();
        let id = service
            .begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        let first = service.active().unwrap().clone();
        let second = first.clone();
        assert!(Arc::ptr_eq(&first.payload, &second.payload));
        assert_eq!(first.payload.data.as_ptr(), second.payload.data.as_ptr());
        service.cancel(id, DragCancelReason::Explicit).unwrap();
        assert_eq!(first.payload.data, [1, 2, 3]);
    }

    #[test]
    fn repeated_pointer_updates_coalesce_to_latest_position_and_negotiation() {
        let tree = UiTree::new();
        let mut service = DragDropService::default();
        let id = service
            .begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        for index in 0..100_000 {
            service
                .hover(
                    id,
                    Some(tree.root()),
                    Point::new(index as f32, 0.0),
                    if index % 2 == 0 {
                        Some(DropEffect::Copy)
                    } else {
                        None
                    },
                )
                .unwrap();
        }
        assert_eq!(service.events.len(), 3);
        let latest = service.events.last().unwrap();
        assert_eq!(latest.phase, DragPhase::Over);
        assert_eq!(latest.position, Point::new(99_999.0, 0.0));
        assert_eq!(latest.effect, None);
        service.finish(id).unwrap();
        let events = service.take_events();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].phase, DragPhase::Started);
        assert_eq!(
            events[3].phase,
            DragPhase::Cancelled(DragCancelReason::Rejected)
        );
    }

    #[test]
    fn target_transition_pressure_resynchronizes_but_preserves_terminal() {
        let mut tree = UiTree::new();
        let other = tree
            .insert(tree.root(), crate::NodeProps::default())
            .unwrap();
        let mut service = DragDropService::default();
        let id = service
            .begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        for index in 0..MAX_DRAG_EVENTS * 4 {
            let target = if index % 2 == 0 { tree.root() } else { other };
            service
                .hover(
                    id,
                    Some(target),
                    Point::new(index as f32, 0.0),
                    Some(DropEffect::Copy),
                )
                .unwrap();
            assert!(service.events.len() < MAX_DRAG_EVENTS);
        }
        let snapshot = service.active().unwrap().clone();
        service.finish(id).unwrap();
        let events = service.take_events();
        assert!(events.len() <= MAX_DRAG_EVENTS);
        assert!(
            events
                .iter()
                .any(|event| event.phase == DragPhase::Resynchronized)
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.phase == DragPhase::Started)
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.phase == DragPhase::Dropped)
                .count(),
            1
        );
        let terminal = events.last().unwrap();
        assert_eq!(terminal.target, snapshot.target);
        assert_eq!(terminal.position, snapshot.position);
        assert_eq!(terminal.effect, snapshot.effect);
    }

    #[test]
    fn full_lifecycle_queue_rejects_new_begin_and_keeps_every_accepted_outcome() {
        let tree = UiTree::new();
        let mut service = DragDropService::default();
        let mut accepted = Vec::new();
        while let Ok(id) = service.begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO) {
            accepted.push(id);
            service.cancel(id, DragCancelReason::Explicit).unwrap();
        }
        let before = service.events.clone();
        assert_eq!(
            service.begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO),
            Err(DragError::EventCapacity)
        );
        assert!(service.active().is_none());
        assert_eq!(service.events, before);
        let events = service.take_events();
        assert_eq!(events.len(), accepted.len() * 2);
        assert!(events.len() <= MAX_DRAG_EVENTS);
        for (id, pair) in accepted.iter().zip(events.chunks_exact(2)) {
            assert_eq!(pair[0].id, *id);
            assert_eq!(pair[1].id, *id);
            assert_eq!(pair[0].phase, DragPhase::Started);
            assert_eq!(
                pair[1].phase,
                DragPhase::Cancelled(DragCancelReason::Explicit)
            );
        }
        let id = service
            .begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        service.cancel(id, DragCancelReason::Explicit).unwrap();
    }

    #[test]
    fn almost_full_queue_reserves_terminal_during_hover_and_after_mid_drag_drain() {
        let tree = UiTree::new();
        let mut service = DragDropService::default();
        for _ in 0..(MAX_DRAG_EVENTS - 3) / 2 {
            let id = service
                .begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO)
                .unwrap();
            service.cancel(id, DragCancelReason::Explicit).unwrap();
        }
        let id = service
            .begin(tree.root(), payload(), DropEffects::COPY, Point::ZERO)
            .unwrap();
        for index in 0..20 {
            service
                .hover(
                    id,
                    Some(tree.root()),
                    Point::new(index as f32, 0.0),
                    Some(DropEffect::Copy),
                )
                .unwrap();
            service
                .hover(id, None, Point::new(index as f32, 0.0), None)
                .unwrap();
        }
        assert!(service.events.len() < MAX_DRAG_EVENTS);
        let previous = service.take_events();
        assert_eq!(
            previous
                .iter()
                .filter(|event| event.id == id && event.phase == DragPhase::Started)
                .count(),
            1
        );
        service
            .hover(id, Some(tree.root()), Point::ZERO, Some(DropEffect::Copy))
            .unwrap();
        service.finish(id).unwrap();
        let current = service.take_events();
        assert_eq!(
            current
                .iter()
                .filter(|event| event.phase == DragPhase::Dropped)
                .count(),
            1
        );
        assert!(current.iter().all(|event| event.id == id));
    }
}
