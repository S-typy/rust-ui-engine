//! Retained page hosts and shared route/history transitions.
use super::navigation_model::invalid;
use super::*;
use std::{collections::VecDeque, rc::Rc};
mod input;
mod layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationPane {
    pub container: WidgetId,
    pub frame: WidgetId,
    pub back_button: WidgetId,
    pub forward_button: WidgetId,
    pub menu_button: WidgetId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationCause {
    Push,
    Replace,
    Back,
    Forward,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationChanged {
    pub owner: WidgetId,
    pub route: String,
    pub cause: NavigationCause,
}
type RouteValidation = dyn Fn(&str) -> Result<(), String>;
#[derive(Clone)]
pub struct NavigationValidator(Rc<RouteValidation>);
impl NavigationValidator {
    pub fn new(validate: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(validate))
    }
    fn check(&self, route: &str) -> Result<(), ControlError> {
        (self.0)(route).map_err(ControlError::Invalid)
    }
}
#[derive(Clone)]
struct PageParts {
    route: String,
    host: WidgetId,
    selector: WidgetId,
    focus: Option<WidgetId>,
}
#[derive(Clone)]
struct SectionParts {
    id: String,
    host: WidgetId,
    header: WidgetId,
    office: WidgetId,
}
struct Entry {
    parts: NavigationPane,
    model: NavigationModel,
    settings: NavigationSettings,
    toolbar: WidgetId,
    caption: WidgetId,
    sidebar: WidgetId,
    viewer: ScrollViewer,
    office_viewer: ScrollViewer,
    pages: Vec<PageParts>,
    sections: Vec<SectionParts>,
    expanded_section: String,
    hamburger_open: bool,
    validator: Option<NavigationValidator>,
    source_validators: Vec<NavigationValidator>,
    error: Option<String>,
    parents: Vec<(WidgetId, WidgetId)>,
}
#[derive(Default)]
pub(super) struct Navigations {
    entries: HashMap<WidgetId, Entry>,
    events: VecDeque<NavigationChanged>,
    pointer: Option<WidgetId>,
}

impl Controls {
    pub fn navigation_pane(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        model: NavigationModel,
        settings: NavigationSettings,
        mut style: LayoutStyle,
    ) -> Result<NavigationPane, ControlError> {
        settings.validate()?;
        if self.navigations.entries.len() >= 32 {
            return Err(invalid("At most 32 navigation panes are supported"));
        }
        let label = label.into();
        self.measure_text(&label, 4096.0, false)?;
        for text in model
            .sections()
            .iter()
            .map(|s| &s.label)
            .chain(model.pages().iter().map(|p| &p.label))
        {
            self.measure_text(text, 4096.0, false)?;
        }
        style.kind = LayoutKind::Grid;
        style.padding = Edges::default();
        style.gap = 0.0;
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, label)?;
            let toolbar = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    gap: 4.0,
                    ..Default::default()
                },
            )?;
            let menu_button =
                self.navigation_button(toolbar, "Разделы", Some(IconSymbol::ChevronRight))?;
            let back_button = self.navigation_button(toolbar, "Назад", None)?;
            let forward_button = self.navigation_button(toolbar, "Вперёд", None)?;
            let caption = self.label(
                toolbar,
                &model.current_page().label,
                LayoutStyle {
                    flex_grow: 1.0,
                    ..Default::default()
                },
            )?;
            let sidebar = self.panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    ..Default::default()
                },
            )?;
            let viewer = self.scroll_view(sidebar, fill(), 400.0)?;
            let office_viewer = self.scroll_view(sidebar, fill(), 160.0)?;
            self.edit_props(viewer.content, |p| {
                p.style.kind = LayoutKind::Stack(Axis::Vertical);
                p.style.gap = 4.0;
            })?;
            self.edit_props(office_viewer.content, |p| {
                p.style.kind = LayoutKind::Stack(Axis::Vertical);
                p.style.gap = 2.0;
            })?;
            let frame = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    ..fill()
                },
            )?;
            self.set_role(frame, Role::Group);
            self.edit_props(frame, |p| {
                p.focusable = true;
                p.style.grid_rows = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
                p.style.grid_columns = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
            })?;
            let mut sections = Vec::new();
            let mut pages = Vec::new();
            for section in model.sections() {
                let host = self.transparent_panel(
                    viewer.content,
                    LayoutStyle {
                        kind: LayoutKind::Stack(Axis::Vertical),
                        gap: 2.0,
                        ..Default::default()
                    },
                )?;
                let header = self.navigation_button(host, &section.label, section.icon)?;
                let office =
                    self.navigation_button(office_viewer.content, &section.label, section.icon)?;
                for page in model.pages().iter().filter(|p| p.section == section.id) {
                    let selector = self.navigation_button(host, &page.label, page.icon)?;
                    self.set_role(selector, Role::Tab);
                    let page_host = self.transparent_panel(
                        frame,
                        LayoutStyle {
                            kind: LayoutKind::Grid,
                            ..fill()
                        },
                    )?;
                    self.set_role(page_host, Role::Group);
                    self.set_automation_name(page_host, &page.label)?;
                    self.edit_props(page_host, |p| {
                        p.style.grid_row.start = Some(1);
                        p.style.grid_column.start = Some(1);
                        p.style.grid_rows = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
                        p.style.grid_columns = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
                        p.collapsed = page.route != model.route();
                    })?;
                    pages.push(PageParts {
                        route: page.route.clone(),
                        host: page_host,
                        selector,
                        focus: None,
                    });
                }
                sections.push(SectionParts {
                    id: section.id.clone(),
                    host,
                    header,
                    office,
                });
            }
            let parts = NavigationPane {
                container,
                frame,
                back_button,
                forward_button,
                menu_button,
            };
            let expanded_section = model.current_page().section.clone();
            let mut model = model;
            model.set_history_limit(settings.history_limit);
            let mut entry = Entry {
                parts,
                model,
                settings,
                toolbar,
                caption,
                sidebar,
                viewer,
                office_viewer,
                pages,
                sections,
                expanded_section,
                hamburger_open: false,
                validator: None,
                source_validators: Vec::new(),
                error: None,
                parents: Vec::new(),
            };
            entry.parents = required(&entry)
                .into_iter()
                .map(|id| {
                    (
                        id,
                        self.runtime
                            .tree()
                            .parent(id)
                            .expect("new navigation part is attached"),
                    )
                })
                .collect();
            self.navigations.entries.insert(container, entry);
            self.present_navigation(container)?;
            Ok(parts)
        })();
        if result.is_err() {
            self.navigations.entries.remove(&container);
            self.remove(container)?;
        }
        result
    }
    fn navigation_button(
        &mut self,
        parent: WidgetId,
        label: &str,
        icon: Option<IconSymbol>,
    ) -> Result<WidgetId, ControlError> {
        let id = self.button(
            parent,
            label,
            None,
            LayoutStyle {
                height: Length::Px(36.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        self.set_button_chrome(id, ButtonChrome::Flat)?;
        self.set_automation_name(id, label)?;
        if let Some(icon) = icon {
            self.set_button_icon(id, Some(ButtonIcon::new(icon)))?;
        }
        Ok(id)
    }
    pub fn navigation_parts(&self, owner: WidgetId) -> Option<NavigationPane> {
        self.navigations.entries.get(&owner).map(|e| e.parts)
    }
    pub fn navigation_model(&self, owner: WidgetId) -> Option<&NavigationModel> {
        self.navigations.entries.get(&owner).map(|e| &e.model)
    }
    pub fn navigation_route(&self, owner: WidgetId) -> Option<&str> {
        self.navigation_model(owner).map(NavigationModel::route)
    }
    /// Last rejected user transition. Programmatic transitions return their error.
    pub fn navigation_error(&self, owner: WidgetId) -> Option<&str> {
        self.navigations.entries.get(&owner)?.error.as_deref()
    }
    pub fn clear_navigation_error(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.check_navigation(owner)?;
        if self
            .navigations
            .entries
            .get_mut(&owner)
            .unwrap()
            .error
            .take()
            .is_some()
        {
            self.present_navigation(owner)?;
        }
        Ok(())
    }
    fn navigation_user_result(
        &mut self,
        owner: WidgetId,
        result: Result<bool, ControlError>,
    ) -> Result<(), ControlError> {
        match result {
            Ok(_) => Ok(()),
            Err(ControlError::Invalid(reason)) => {
                let reason = if reason.is_empty()
                    || reason.len() > 512
                    || reason.chars().any(char::is_control)
                    || self
                        .measure_text(
                            &format!(
                                "{} — {reason}",
                                self.navigations.entries[&owner].model.current_page().label
                            ),
                            4096.0,
                            false,
                        )
                        .is_err()
                {
                    "Переход отклонён".to_owned()
                } else {
                    reason
                };
                self.navigations.entries.get_mut(&owner).unwrap().error = Some(reason);
                self.present_navigation(owner)
            }
            Err(error) => Err(error),
        }
    }
    pub fn navigation_settings(&self, owner: WidgetId) -> Option<NavigationSettings> {
        self.navigations.entries.get(&owner).map(|e| e.settings)
    }
    pub fn navigation_page_host(&self, owner: WidgetId, route: &str) -> Option<WidgetId> {
        self.navigations
            .entries
            .get(&owner)?
            .pages
            .iter()
            .find(|p| p.route == route)
            .map(|p| p.host)
    }
    pub fn navigation_selector(&self, owner: WidgetId, route: &str) -> Option<WidgetId> {
        self.navigations
            .entries
            .get(&owner)?
            .pages
            .iter()
            .find(|p| p.route == route)
            .map(|p| p.selector)
    }
    pub fn navigation_section_button(&self, owner: WidgetId, section: &str) -> Option<WidgetId> {
        let e = self.navigations.entries.get(&owner)?;
        let s = e.sections.iter().find(|s| s.id == section)?;
        Some(if e.settings.view == NavigationView::Office {
            s.office
        } else {
            s.header
        })
    }
    pub fn set_navigation_validator(
        &mut self,
        owner: WidgetId,
        validator: Option<NavigationValidator>,
    ) -> Result<(), ControlError> {
        self.check_navigation(owner)?;
        if let Some(validator) = &validator {
            validator.check(self.navigation_route(owner).unwrap())?;
        }
        self.navigations.entries.get_mut(&owner).unwrap().validator = validator;
        Ok(())
    }
    pub(crate) fn chain_navigation_validator(
        &mut self,
        owner: WidgetId,
        validator: NavigationValidator,
    ) -> Result<(), ControlError> {
        self.check_navigation(owner)?;
        let entry = self.navigations.entries.get_mut(&owner).unwrap();
        if entry.source_validators.len() >= 64 {
            return Err(invalid(
                "At most 64 source validation guards per navigation pane",
            ));
        }
        entry.source_validators.push(validator);
        Ok(())
    }
    pub fn navigate(
        &mut self,
        owner: WidgetId,
        route: &str,
        mode: NavigationMode,
    ) -> Result<bool, ControlError> {
        self.check_navigation(owner)?;
        let mut model = self.navigations.entries[&owner].model.clone();
        if !model.navigate(route, mode)? {
            self.clear_navigation_error(owner)?;
            return Ok(false);
        }
        self.install_navigation(
            owner,
            model,
            if mode == NavigationMode::Push {
                NavigationCause::Push
            } else {
                NavigationCause::Replace
            },
        )?;
        Ok(true)
    }
    pub fn navigation_back(&mut self, owner: WidgetId) -> Result<bool, ControlError> {
        self.navigation_history_step(owner, false)
    }
    pub fn navigation_forward(&mut self, owner: WidgetId) -> Result<bool, ControlError> {
        self.navigation_history_step(owner, true)
    }
    fn navigation_history_step(
        &mut self,
        owner: WidgetId,
        forward: bool,
    ) -> Result<bool, ControlError> {
        self.check_navigation(owner)?;
        let mut model = self.navigations.entries[&owner].model.clone();
        if !(if forward {
            model.forward()
        } else {
            model.back()
        }) {
            return Ok(false);
        }
        self.install_navigation(
            owner,
            model,
            if forward {
                NavigationCause::Forward
            } else {
                NavigationCause::Back
            },
        )?;
        Ok(true)
    }
    fn install_navigation(
        &mut self,
        owner: WidgetId,
        model: NavigationModel,
        cause: NavigationCause,
    ) -> Result<(), ControlError> {
        let entry = &self.navigations.entries[&owner];
        if let Some(validator) = &entry.validator {
            validator.check(model.route())?;
        }
        for validator in &entry.source_validators {
            validator.check(model.route())?;
        }
        let changed = entry.model.route() != model.route();
        if changed && self.navigations.events.len() >= 256 {
            return Err(invalid(
                "Drain navigation events before further transitions",
            ));
        }
        let old = entry
            .pages
            .iter()
            .find(|p| p.route == entry.model.route())
            .unwrap()
            .clone();
        let next = entry
            .pages
            .iter()
            .find(|p| p.route == model.route())
            .unwrap()
            .clone();
        let frame = entry.parts.frame;
        let focused = self.runtime.focused();
        let editor_focus = self
            .observed_editor_focus
            .filter(|id| self.flyout_descendant(*id, old.host));
        let overlay_focus = self.flyout_focus_anchor_in(old.host).or_else(|| {
            self.rich
                .popup
                .as_ref()
                .filter(|p| {
                    self.flyout_descendant(p.owner, old.host)
                        && focused.is_some_and(|id| self.flyout_descendant(id, p.surface))
                })
                .map(|p| p.owner)
        });
        let stored_focus = focused
            .filter(|id| self.flyout_descendant(*id, old.host))
            .or(editor_focus)
            .or(overlay_focus);
        let owned_focus = focused.is_some_and(|id| self.flyout_descendant(id, owner))
            || editor_focus.is_some()
            || overlay_focus.is_some();
        let mut mutations = Vec::new();
        for page in &entry.pages {
            let mut props = self.runtime.tree().node(page.host).unwrap().props.clone();
            props.collapsed = page.route != model.route();
            mutations.push(Mutation::SetProps {
                node: page.host,
                props,
            });
        }
        let restored = next
            .focus
            .filter(|id| self.navigation_focus_eligible(*id, next.host));
        if owned_focus && self.window_focused {
            mutations.push(Mutation::Focus(Some(restored.unwrap_or(frame))));
        }
        self.runtime.apply_batch(&mutations)?;
        // Hidden editors never implicitly commit on this focus transition.
        if self
            .rich
            .popup
            .as_ref()
            .is_some_and(|p| self.flyout_descendant(p.owner, old.host))
        {
            self.close_rich_popup(false)?;
        }
        self.sync_popup_availability()?;
        self.sync_flyouts()?;
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        let entry = self.navigations.entries.get_mut(&owner).unwrap();
        if let Some(focus) = stored_focus {
            entry
                .pages
                .iter_mut()
                .find(|p| p.route == old.route)
                .unwrap()
                .focus = Some(focus);
        }
        entry.expanded_section = model.current_page().section.clone();
        entry.model = model;
        entry.error = None;
        entry.hamburger_open = false;
        if changed {
            self.navigations.events.push_back(NavigationChanged {
                owner,
                route: self.navigations.entries[&owner].model.route().into(),
                cause,
            });
        }
        self.present_navigation(owner)?;
        self.scene_dirty = true;
        Ok(())
    }
    fn navigation_focus_eligible(&self, id: WidgetId, page: WidgetId) -> bool {
        if !self.flyout_descendant(id, page)
            || !self
                .runtime
                .tree()
                .node(id)
                .is_some_and(|n| n.props.focusable)
        {
            return false;
        }
        let mut current = id;
        while current != page {
            let Some(node) = self.runtime.tree().node(current) else {
                return false;
            };
            if !node.props.enabled
                || !node.props.visible
                || node.props.collapsed
                || self.controls.get(&current).is_some_and(|c| !c.enabled)
            {
                return false;
            }
            let Some(parent) = self.runtime.tree().parent(current) else {
                return false;
            };
            current = parent;
        }
        true
    }
    pub fn set_navigation_settings(
        &mut self,
        owner: WidgetId,
        settings: NavigationSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        self.check_navigation(owner)?;
        let e = self.navigations.entries.get_mut(&owner).unwrap();
        e.settings = settings;
        e.model.set_history_limit(settings.history_limit);
        e.hamburger_open = false;
        self.present_navigation(owner)
    }
    pub fn set_navigation_view(
        &mut self,
        owner: WidgetId,
        view: NavigationView,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .navigation_settings(owner)
            .ok_or_else(|| invalid("Not a navigation pane"))?;
        settings.view = view;
        self.set_navigation_settings(owner, settings)
    }
    pub fn set_navigation_page_enabled(
        &mut self,
        owner: WidgetId,
        route: &str,
        enabled: bool,
    ) -> Result<(), ControlError> {
        self.check_navigation(owner)?;
        self.navigations
            .entries
            .get_mut(&owner)
            .unwrap()
            .model
            .set_page_enabled(route, enabled)?;
        self.present_navigation(owner)
    }
    pub fn drain_navigation_events(&mut self) -> Vec<NavigationChanged> {
        self.navigations.events.drain(..).collect()
    }
    fn check_navigation(&self, owner: WidgetId) -> Result<(), ControlError> {
        let e = self
            .navigations
            .entries
            .get(&owner)
            .ok_or_else(|| invalid("Not a navigation pane"))?;
        if !self.runtime.tree().contains(owner)
            || e.parents
                .iter()
                .any(|(id, parent)| self.runtime.tree().parent(*id) != Some(*parent))
        {
            return Err(invalid(
                "Navigation required parts were removed or reparented",
            ));
        }
        Ok(())
    }
    pub(super) fn preflight_navigation_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self
            .navigations
            .entries
            .values()
            .any(|e| required(e).contains(&id))
        {
            Err(invalid(
                "Remove the navigation owner, not a required host or selector",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn prune_navigations(&mut self) {
        self.navigations
            .entries
            .retain(|id, _| self.runtime.tree().contains(*id));
        self.navigations
            .events
            .retain(|e| self.navigations.entries.contains_key(&e.owner));
        if self
            .navigations
            .pointer
            .is_some_and(|id| !self.runtime.tree().contains(id))
        {
            self.navigations.pointer = None;
        }
    }
}
fn required(e: &Entry) -> Vec<WidgetId> {
    let p = e.parts;
    let mut ids = vec![
        p.frame,
        p.back_button,
        p.forward_button,
        p.menu_button,
        e.toolbar,
        e.caption,
        e.sidebar,
        e.viewer.viewport,
        e.viewer.content,
        e.office_viewer.viewport,
        e.office_viewer.content,
    ];
    ids.extend(e.pages.iter().flat_map(|p| [p.host, p.selector]));
    ids.extend(e.sections.iter().flat_map(|s| [s.host, s.header, s.office]));
    ids
}
fn fill() -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Percent(1.0),
        ..Default::default()
    }
}
