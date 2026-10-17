//! Bounded routes and history shared by navigation presentations.
use super::*;

pub const MAX_NAVIGATION_PAGES: usize = 64;
pub const MAX_NAVIGATION_SECTIONS: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationView {
    #[default]
    Side,
    Accordion,
    Hamburger,
    Office,
    Frame,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationMode {
    Push,
    /// Replace the current history entry. Used by external route bindings.
    #[default]
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavigationSettings {
    pub view: NavigationView,
    pub pane_width: f32,
    pub history_limit: usize,
    pub show_history: bool,
}
impl Default for NavigationSettings {
    fn default() -> Self {
        Self {
            view: NavigationView::Side,
            pane_width: 220.0,
            history_limit: 64,
            show_history: true,
        }
    }
}
impl NavigationSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if !self.pane_width.is_finite()
            || !(120.0..=640.0).contains(&self.pane_width)
            || !(1..=128).contains(&self.history_limit)
        {
            return Err(invalid(
                "Navigation requires pane width 120..640 and history limit 1..128",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationSection {
    pub id: String,
    pub label: String,
    pub icon: Option<IconSymbol>,
}
impl NavigationSection {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationPage {
    pub route: String,
    pub section: String,
    pub label: String,
    pub icon: Option<IconSymbol>,
    pub enabled: bool,
}
impl NavigationPage {
    pub fn new(
        route: impl Into<String>,
        section: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            route: route.into(),
            section: section.into(),
            label: label.into(),
            icon: None,
            enabled: true,
        }
    }
}

/// Opaque route IDs are never interpreted as URLs or filesystem paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationModel {
    sections: Vec<NavigationSection>,
    pages: Vec<NavigationPage>,
    history: Vec<String>,
    position: usize,
    limit: usize,
}
impl NavigationModel {
    pub fn new(
        sections: Vec<NavigationSection>,
        pages: Vec<NavigationPage>,
        route: &str,
        history_limit: usize,
    ) -> Result<Self, ControlError> {
        if !(1..=MAX_NAVIGATION_SECTIONS).contains(&sections.len())
            || !(1..=MAX_NAVIGATION_PAGES).contains(&pages.len())
            || !(1..=128).contains(&history_limit)
        {
            return Err(invalid(
                "Navigation requires 1..16 sections, 1..64 pages and history 1..128",
            ));
        }
        let mut ids = HashSet::new();
        for section in &sections {
            validate_id(&section.id)?;
            validate_label(&section.label)?;
            if !ids.insert(&section.id) {
                return Err(invalid("Duplicate navigation section"));
            }
        }
        let mut routes = HashSet::new();
        let mut bytes: usize = sections.iter().map(|s| s.id.len() + s.label.len()).sum();
        for page in &pages {
            validate_id(&page.route)?;
            validate_label(&page.label)?;
            if !routes.insert(&page.route) || !ids.contains(&page.section) {
                return Err(invalid(
                    "Navigation routes must be unique and reference a declared section",
                ));
            }
            bytes += page.route.len() + page.section.len() + page.label.len();
        }
        if bytes > 32768
            || sections
                .iter()
                .any(|s| !pages.iter().any(|p| p.section == s.id))
        {
            return Err(invalid(
                "Navigation sections require pages and metadata must fit 32 KiB",
            ));
        }
        let result = Self {
            sections,
            pages,
            history: vec![route.into()],
            position: 0,
            limit: history_limit,
        };
        result.check_route(route)?;
        Ok(result)
    }
    pub fn route(&self) -> &str {
        &self.history[self.position]
    }
    pub fn current_page(&self) -> &NavigationPage {
        self.pages.iter().find(|p| p.route == self.route()).unwrap()
    }
    pub fn pages(&self) -> &[NavigationPage] {
        &self.pages
    }
    pub fn sections(&self) -> &[NavigationSection] {
        &self.sections
    }
    pub fn history(&self) -> &[String] {
        &self.history
    }
    pub fn history_position(&self) -> usize {
        self.position
    }
    pub fn back_route(&self) -> Option<&str> {
        self.back_position().map(|i| self.history[i].as_str())
    }
    pub fn forward_route(&self) -> Option<&str> {
        self.forward_position().map(|i| self.history[i].as_str())
    }
    pub fn check_route(&self, route: &str) -> Result<(), ControlError> {
        if self.pages.iter().any(|p| p.route == route && p.enabled) {
            Ok(())
        } else {
            Err(invalid("Navigation route is unknown or disabled"))
        }
    }
    pub fn navigate(&mut self, route: &str, mode: NavigationMode) -> Result<bool, ControlError> {
        self.check_route(route)?;
        if self.route() == route {
            return Ok(false);
        }
        match mode {
            NavigationMode::Replace => self.history[self.position] = route.into(),
            NavigationMode::Push => {
                self.history.truncate(self.position + 1);
                self.history.push(route.into());
                if self.history.len() > self.limit {
                    self.history.remove(0);
                }
                self.position = self.history.len() - 1;
            }
        }
        Ok(true)
    }
    pub fn back(&mut self) -> bool {
        if let Some(i) = self.back_position() {
            self.position = i;
            true
        } else {
            false
        }
    }
    pub fn forward(&mut self) -> bool {
        if let Some(i) = self.forward_position() {
            self.position = i;
            true
        } else {
            false
        }
    }
    pub fn set_page_enabled(&mut self, route: &str, enabled: bool) -> Result<(), ControlError> {
        let page = self
            .pages
            .iter_mut()
            .find(|p| p.route == route)
            .ok_or_else(|| invalid("Unknown navigation route"))?;
        page.enabled = enabled;
        Ok(())
    }
    pub(super) fn set_history_limit(&mut self, limit: usize) {
        if self.history.len() > limit {
            let start = (self.position + 1).saturating_sub(limit);
            let end = (start + limit).min(self.history.len());
            self.history = self.history[start..end].to_vec();
            self.position -= start;
        }
        self.limit = limit;
    }
    fn back_position(&self) -> Option<usize> {
        (0..self.position)
            .rev()
            .find(|&i| self.check_route(&self.history[i]).is_ok())
    }
    fn forward_position(&self) -> Option<usize> {
        (self.position + 1..self.history.len())
            .find(|&i| self.check_route(&self.history[i]).is_ok())
    }
}
pub(super) fn validate_id(id: &str) -> Result<(), ControlError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b))
    {
        return Err(invalid(
            "Navigation IDs require 1..128 ASCII letters, digits, '/', '_', '.', '-'",
        ));
    }
    Ok(())
}
fn validate_label(text: &str) -> Result<(), ControlError> {
    if text.trim().is_empty()
        || text.chars().any(char::is_control)
        || text.contains(['\u{2028}', '\u{2029}'])
    {
        return Err(invalid(
            "Navigation captions require nonempty single-line text",
        ));
    }
    TextLimits {
        max_graphemes: Some(128),
        max_bytes: 512,
    }
    .validate(text)
    .map_err(edit_error)
}
pub(super) fn invalid(text: &str) -> ControlError {
    ControlError::Invalid(text.into())
}
