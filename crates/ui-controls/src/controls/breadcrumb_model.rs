//! Accepted hierarchical locations and bounded application-owned navigation.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_BREADCRUMB_SEGMENTS: usize = 64;
pub const MAX_BREADCRUMB_CHILDREN: usize = 256;
const MAX_EVENTS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbSegment {
    pub key: String,
    pub label: String,
    pub has_children: bool,
}
impl BreadcrumbSegment {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            has_children: false,
        }
    }
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.key.is_empty() || self.key.len() > 128 || self.key.chars().any(char::is_control) {
            return Err(invalid(
                "Breadcrumb keys must contain 1..128 UTF-8 bytes without controls",
            ));
        }
        validate_text(&self.label, 128, 512, false)
    }
}

/// An application-resolved location. `text` is an opaque editable representation,
/// not a filesystem path or an instruction evaluated by the control.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BreadcrumbPath {
    pub text: String,
    pub segments: Vec<BreadcrumbSegment>,
}
impl BreadcrumbPath {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_text(&self.text, 1024, 4096, true)?;
        if self.segments.len() > MAX_BREADCRUMB_SEGMENTS {
            return Err(invalid("At most 64 breadcrumb segments are supported"));
        }
        let mut keys = std::collections::HashSet::new();
        let mut bytes = 0usize;
        for segment in &self.segments {
            segment.validate()?;
            if !keys.insert(&segment.key) {
                return Err(invalid(
                    "Breadcrumb segment keys must be unique within a path",
                ));
            }
            bytes += segment.label.len() + segment.key.len();
        }
        if bytes > 16_384 {
            return Err(invalid("Breadcrumb path metadata exceeds 16 KiB"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbChild {
    pub segment: BreadcrumbSegment,
    pub enabled: bool,
}
impl BreadcrumbChild {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            segment: BreadcrumbSegment::new(key, label),
            enabled: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BreadcrumbSourceVersion {
    pub identity: u64,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BreadcrumbSettings {
    pub editable: bool,
    pub read_only: bool,
    pub history_limit: usize,
    pub popup_size: Size,
    pub source: BreadcrumbSourceVersion,
}
impl Default for BreadcrumbSettings {
    fn default() -> Self {
        Self {
            editable: true,
            read_only: false,
            history_limit: 20,
            popup_size: Size::new(360.0, 280.0),
            source: BreadcrumbSourceVersion::default(),
        }
    }
}
impl BreadcrumbSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if self.history_limit > 100
            || !self.popup_size.is_valid()
            || !(160.0..=4096.0).contains(&self.popup_size.width)
            || !(80.0..=4096.0).contains(&self.popup_size.height)
        {
            return Err(invalid("Invalid breadcrumb history or popup limits"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreadcrumbNavigationTarget {
    Text(String),
    /// Stable key prefix of the currently accepted path.
    Segment(Vec<String>),
    Child {
        parent: Vec<String>,
        key: String,
    },
    History(BreadcrumbPath),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RequestKind {
    Navigation(BreadcrumbNavigationTarget),
    Children { parent: Vec<String> },
}
#[derive(Debug)]
struct RequestIdentity {
    owner: WidgetId,
    generation: u64,
    source: BreadcrumbSourceVersion,
    path_epoch: u64,
    kind: RequestKind,
    cancelled: AtomicBool,
}
#[derive(Clone, Debug)]
pub struct BreadcrumbNavigationRequest(Arc<RequestIdentity>);
#[derive(Clone, Debug)]
pub struct BreadcrumbChildrenRequest(Arc<RequestIdentity>);
macro_rules! request_accessors {
    ($name:ident) => {
        impl $name {
            pub fn owner(&self) -> WidgetId {
                self.0.owner
            }
            pub fn generation(&self) -> u64 {
                self.0.generation
            }
            pub fn source(&self) -> BreadcrumbSourceVersion {
                self.0.source
            }
            pub fn is_cancelled(&self) -> bool {
                self.0.cancelled.load(Ordering::Acquire)
            }
            fn cancel(&self) {
                self.0.cancelled.store(true, Ordering::Release);
            }
        }
    };
}
request_accessors!(BreadcrumbNavigationRequest);
request_accessors!(BreadcrumbChildrenRequest);
impl BreadcrumbNavigationRequest {
    pub fn target(&self) -> &BreadcrumbNavigationTarget {
        let RequestKind::Navigation(target) = &self.0.kind else {
            unreachable!()
        };
        target
    }
}
impl BreadcrumbChildrenRequest {
    pub fn parent_keys(&self) -> &[String] {
        let RequestKind::Children { parent } = &self.0.kind else {
            unreachable!()
        };
        parent
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreadcrumbCompletion {
    Applied,
    Rejected,
    Stale,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BreadcrumbStatus {
    #[default]
    Idle,
    Pending,
    Ready,
    Failed(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreadcrumbEventKind {
    NavigationAccepted(BreadcrumbPath),
    NavigationRejected(String),
    ChildrenRejected(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbEvent {
    pub owner: WidgetId,
    pub kind: BreadcrumbEventKind,
}

pub(super) struct BreadcrumbSession {
    pub settings: BreadcrumbSettings,
    pub path: BreadcrumbPath,
    pub history: Vec<BreadcrumbPath>,
    pub children: Vec<BreadcrumbChild>,
    pub children_parent: Vec<String>,
    pub navigation_status: BreadcrumbStatus,
    pub children_status: BreadcrumbStatus,
    pub events: Vec<BreadcrumbEventKind>,
    path_epoch: u64,
    generation: u64,
    navigation: Option<BreadcrumbNavigationRequest>,
    children_request: Option<BreadcrumbChildrenRequest>,
    navigation_unread: bool,
    children_unread: bool,
}
impl Drop for BreadcrumbSession {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl BreadcrumbSession {
    pub fn new(settings: BreadcrumbSettings, path: BreadcrumbPath) -> Result<Self, ControlError> {
        settings.validate()?;
        path.validate()?;
        Ok(Self {
            settings,
            path,
            history: Vec::new(),
            children: Vec::new(),
            children_parent: Vec::new(),
            navigation_status: BreadcrumbStatus::Idle,
            children_status: BreadcrumbStatus::Idle,
            events: Vec::new(),
            path_epoch: 0,
            generation: 0,
            navigation: None,
            children_request: None,
            navigation_unread: false,
            children_unread: false,
        })
    }
    pub fn cancel(&mut self) {
        if let Some(request) = self.navigation.take() {
            request.cancel();
        }
        if let Some(request) = self.children_request.take() {
            request.cancel();
        }
        self.navigation_unread = false;
        self.children_unread = false;
        self.navigation_status = BreadcrumbStatus::Idle;
        self.children_status = BreadcrumbStatus::Idle;
    }
    pub fn preflight_event(&self) -> Result<(), ControlError> {
        if self.events.len() >= MAX_EVENTS {
            return Err(invalid(
                "Drain breadcrumb events before completing more actions",
            ));
        }
        Ok(())
    }
    pub fn preflight_path(
        &self,
        path: &BreadcrumbPath,
        navigation: bool,
    ) -> Result<(), ControlError> {
        path.validate()?;
        self.path_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("Breadcrumb path revision exhausted"))?;
        if navigation {
            self.preflight_event()?;
        }
        Ok(())
    }
    pub fn publish_path(&mut self, path: BreadcrumbPath, navigation: bool) {
        self.cancel();
        self.path_epoch += 1; // preflight_path checked before any presentation mutation.
        self.path = path.clone();
        self.children.clear();
        self.children_parent.clear();
        if navigation {
            if self.settings.history_limit > 0 {
                self.history.retain(|old| !same_keys(old, &path));
                self.history.insert(0, path.clone());
                self.history.truncate(self.settings.history_limit);
            }
            self.events
                .push(BreadcrumbEventKind::NavigationAccepted(path));
        }
    }
    fn next(&self) -> Result<u64, ControlError> {
        self.generation
            .checked_add(1)
            .ok_or_else(|| invalid("Breadcrumb request generation exhausted"))
    }
    pub fn preflight_request(&self) -> Result<(), ControlError> {
        self.next().map(|_| ())
    }
    pub fn navigate(
        &mut self,
        owner: WidgetId,
        target: BreadcrumbNavigationTarget,
    ) -> Result<(), ControlError> {
        match &target {
            BreadcrumbNavigationTarget::Text(text) => validate_text(text, 1024, 4096, false)?,
            BreadcrumbNavigationTarget::Segment(keys) => {
                self.prefix(keys)?;
            }
            BreadcrumbNavigationTarget::Child { parent, key } => {
                self.prefix(parent)?;
                if self.children_status != BreadcrumbStatus::Ready
                    || *parent != self.children_parent
                    || !self
                        .children
                        .iter()
                        .any(|child| child.segment.key == *key && child.enabled)
                {
                    return Err(invalid("Breadcrumb child is unknown, disabled or stale"));
                }
            }
            BreadcrumbNavigationTarget::History(path) => {
                if !self.history.contains(path) {
                    return Err(invalid("Breadcrumb history entry is stale"));
                }
            }
        }
        let generation = self.next()?;
        self.cancel();
        self.generation = generation;
        self.navigation = Some(BreadcrumbNavigationRequest(Arc::new(RequestIdentity {
            owner,
            generation,
            source: self.settings.source,
            path_epoch: self.path_epoch,
            kind: RequestKind::Navigation(target),
            cancelled: AtomicBool::new(false),
        })));
        self.navigation_unread = true;
        self.navigation_status = BreadcrumbStatus::Pending;
        Ok(())
    }
    fn prefix(&self, keys: &[String]) -> Result<(), ControlError> {
        if keys.is_empty()
            || keys.len() > self.path.segments.len()
            || keys
                .iter()
                .zip(&self.path.segments)
                .any(|(key, segment)| *key != segment.key)
        {
            return Err(invalid("Breadcrumb segment prefix is not current"));
        }
        Ok(())
    }
    pub fn request_children(&mut self, owner: WidgetId, key: &str) -> Result<(), ControlError> {
        let index = self
            .path
            .segments
            .iter()
            .position(|segment| segment.key == key && segment.has_children)
            .ok_or_else(|| invalid("Breadcrumb segment has no declared children"))?;
        let parent = self.path.segments[..=index]
            .iter()
            .map(|segment| segment.key.clone())
            .collect();
        let generation = self.next()?;
        self.cancel();
        self.generation = generation;
        self.children.clear();
        self.children_parent = parent;
        self.children_request = Some(BreadcrumbChildrenRequest(Arc::new(RequestIdentity {
            owner,
            generation,
            source: self.settings.source,
            path_epoch: self.path_epoch,
            kind: RequestKind::Children {
                parent: self.children_parent.clone(),
            },
            cancelled: AtomicBool::new(false),
        })));
        self.children_unread = true;
        self.children_status = BreadcrumbStatus::Pending;
        Ok(())
    }
    pub fn take_navigation(&mut self) -> Option<BreadcrumbNavigationRequest> {
        std::mem::take(&mut self.navigation_unread)
            .then(|| self.navigation.clone())
            .flatten()
    }
    pub fn take_children(&mut self) -> Option<BreadcrumbChildrenRequest> {
        std::mem::take(&mut self.children_unread)
            .then(|| self.children_request.clone())
            .flatten()
    }
    pub fn matches_navigation(&self, request: &BreadcrumbNavigationRequest) -> bool {
        !request.is_cancelled()
            && request.0.source == self.settings.source
            && request.0.path_epoch == self.path_epoch
            && self
                .navigation
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&current.0, &request.0))
    }
    pub fn matches_children(&self, request: &BreadcrumbChildrenRequest) -> bool {
        !request.is_cancelled()
            && request.0.source == self.settings.source
            && request.0.path_epoch == self.path_epoch
            && self
                .children_request
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&current.0, &request.0))
    }
    pub fn reject_navigation(&mut self, message: String) -> Result<(), ControlError> {
        validate_error(&message)?;
        self.preflight_event()?;
        self.cancel();
        self.navigation_status = BreadcrumbStatus::Failed(message.clone());
        self.events
            .push(BreadcrumbEventKind::NavigationRejected(message));
        Ok(())
    }
    pub fn validate_children(&self, children: &[BreadcrumbChild]) -> Result<(), ControlError> {
        if children.len() > MAX_BREADCRUMB_CHILDREN {
            return Err(invalid("At most 256 breadcrumb children are supported"));
        }
        let mut keys = std::collections::HashSet::new();
        let mut bytes = 0usize;
        for child in children {
            child.segment.validate()?;
            if !keys.insert(&child.segment.key) || self.children_parent.contains(&child.segment.key)
            {
                return Err(invalid(
                    "Breadcrumb child keys must be unique and absent from their ancestor path",
                ));
            }
            bytes += child.segment.key.len() + child.segment.label.len();
        }
        if bytes > 65_536 {
            return Err(invalid("Breadcrumb child metadata exceeds 64 KiB"));
        }
        Ok(())
    }
    pub fn publish_children(&mut self, children: Vec<BreadcrumbChild>) {
        if let Some(request) = self.children_request.take() {
            request.cancel();
        }
        self.children_unread = false;
        self.children = children;
        self.children_status = BreadcrumbStatus::Ready;
    }
    pub fn reject_children(&mut self, message: String) -> Result<(), ControlError> {
        validate_error(&message)?;
        self.preflight_event()?;
        if let Some(request) = self.children_request.take() {
            request.cancel();
        }
        self.children_unread = false;
        self.children.clear();
        self.children_status = BreadcrumbStatus::Failed(message.clone());
        self.events
            .push(BreadcrumbEventKind::ChildrenRejected(message));
        Ok(())
    }
}
fn same_keys(left: &BreadcrumbPath, right: &BreadcrumbPath) -> bool {
    left.segments.len() == right.segments.len()
        && left
            .segments
            .iter()
            .zip(&right.segments)
            .all(|(left, right)| left.key == right.key)
}
pub(super) fn validate_text(
    text: &str,
    graphemes: usize,
    bytes: usize,
    empty: bool,
) -> Result<(), ControlError> {
    if (!empty && text.trim().is_empty())
        || text.chars().any(char::is_control)
        || text.contains(['\u{2028}', '\u{2029}'])
    {
        return Err(invalid(
            "Breadcrumb text must be a nonempty single line without controls",
        ));
    }
    TextLimits {
        max_graphemes: Some(graphemes),
        max_bytes: bytes,
    }
    .validate(text)
    .map_err(edit_error)
}
pub(super) fn validate_error(message: &str) -> Result<(), ControlError> {
    validate_text(message, 512, 2048, false)
}
fn invalid(message: &str) -> ControlError {
    ControlError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(key: &str) -> BreadcrumbPath {
        BreadcrumbPath {
            text: format!("route:{key}"),
            segments: vec![BreadcrumbSegment {
                key: key.into(),
                label: format!("Location {key}"),
                has_children: true,
            }],
        }
    }
    #[test]
    fn requests_are_owner_scoped_unforgeable_and_cancelled_on_new_intent() {
        let owner = UiTree::new().root();
        let mut session = BreadcrumbSession::new(Default::default(), path("root")).unwrap();
        session.request_children(owner, "root").unwrap();
        let children = session.take_children().unwrap();
        assert_eq!(children.parent_keys(), ["root"]);
        assert!(session.take_children().is_none());
        session
            .navigate(owner, BreadcrumbNavigationTarget::Text("route:next".into()))
            .unwrap();
        let navigation = session.take_navigation().unwrap();
        assert!(children.is_cancelled());
        assert!(!session.matches_children(&children));
        assert!(session.matches_navigation(&navigation));
        let mut other = BreadcrumbSession::new(Default::default(), path("root")).unwrap();
        other
            .navigate(owner, BreadcrumbNavigationTarget::Text("route:next".into()))
            .unwrap();
        assert!(!other.matches_navigation(&navigation));
        session.settings.source.revision = 1;
        assert!(!session.matches_navigation(&navigation));
        session.cancel();
        assert!(navigation.is_cancelled());
        assert_eq!(session.path, path("root"));
    }
    #[test]
    fn only_successful_navigation_populates_history_and_renamed_identity_deduplicates() {
        let owner = UiTree::new().root();
        let mut session = BreadcrumbSession::new(
            BreadcrumbSettings {
                history_limit: 2,
                ..Default::default()
            },
            path("root"),
        )
        .unwrap();
        session
            .navigate(owner, BreadcrumbNavigationTarget::Text("bad".into()))
            .unwrap();
        session
            .reject_navigation("Unknown location".into())
            .unwrap();
        assert!(session.history.is_empty());
        assert_eq!(session.path, path("root"));
        for key in ["one", "two", "three"] {
            let value = path(key);
            session.preflight_path(&value, true).unwrap();
            session.publish_path(value, true);
        }
        assert_eq!(session.history, [path("three"), path("two")]);
        let mut renamed = path("two");
        renamed.text = "renamed-route".into();
        renamed.segments[0].label = "Renamed label".into();
        session.preflight_path(&renamed, true).unwrap();
        session.publish_path(renamed.clone(), true);
        assert_eq!(session.history, [renamed, path("three")]);
        let app_path = path("application");
        session.preflight_path(&app_path, false).unwrap();
        session.publish_path(app_path, false);
        assert_eq!(session.history.len(), 2);
    }
    #[test]
    fn bounds_duplicate_children_disabled_choice_and_counter_exhaustion_are_atomic() {
        let owner = UiTree::new().root();
        let mut session = BreadcrumbSession::new(Default::default(), path("root")).unwrap();
        session.request_children(owner, "root").unwrap();
        let request = session.take_children().unwrap();
        assert!(
            session
                .validate_children(&[BreadcrumbChild::new("root", "Cycle")])
                .is_err()
        );
        assert!(
            session
                .validate_children(&[
                    BreadcrumbChild::new("a", "A"),
                    BreadcrumbChild::new("a", "Other")
                ])
                .is_err()
        );
        assert!(session.matches_children(&request));
        session.publish_children(vec![BreadcrumbChild {
            segment: BreadcrumbSegment::new("a", "A"),
            enabled: false,
        }]);
        assert!(
            session
                .navigate(
                    owner,
                    BreadcrumbNavigationTarget::Child {
                        parent: vec!["root".into()],
                        key: "a".into()
                    }
                )
                .is_err()
        );
        assert_eq!(session.path, path("root"));
        session.generation = u64::MAX;
        assert!(
            session
                .navigate(owner, BreadcrumbNavigationTarget::Text("next".into()))
                .is_err()
        );
        assert_eq!(session.generation, u64::MAX);
        session.path_epoch = u64::MAX;
        assert!(session.preflight_path(&path("new"), false).is_err());
        assert_eq!(session.path, path("root"));
    }
}
