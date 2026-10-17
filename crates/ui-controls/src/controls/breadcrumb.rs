//! Retained hierarchical location bar. Providers execute outside this control.
use super::breadcrumb_model::BreadcrumbSession;
use super::*;
use std::rc::Rc;

mod input;
mod layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Breadcrumb {
    pub container: WidgetId,
    pub editor: WidgetId,
    pub overflow_button: WidgetId,
    pub history_button: WidgetId,
    pub edit_button: WidgetId,
    pub popup: Flyout,
    pub list: ListBox,
}

type BreadcrumbValidationCallback = dyn Fn(&BreadcrumbPath) -> Result<(), String>;
#[derive(Clone)]
pub struct BreadcrumbValidator(Rc<BreadcrumbValidationCallback>);
impl BreadcrumbValidator {
    pub fn new(validate: impl Fn(&BreadcrumbPath) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(validate))
    }
    fn check(&self, path: &BreadcrumbPath) -> Result<(), ControlError> {
        (self.0)(path).map_err(ControlError::Invalid)
    }
}
#[derive(Clone)]
struct SegmentParts {
    key: String,
    button: WidgetId,
    children: WidgetId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum PopupMode {
    Overflow,
    History,
    Children,
}
struct Entry {
    parts: Breadcrumb,
    bar: WidgetId,
    status: WidgetId,
    segments: Vec<SegmentParts>,
    session: BreadcrumbSession,
    editing: bool,
    draft: String,
    hidden: Vec<String>,
    hidden_focus: Option<String>,
    popup_mode: Option<PopupMode>,
    was_open: bool,
    validator: Option<BreadcrumbValidator>,
}
#[derive(Default)]
pub(super) struct BreadcrumbState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    pub fn breadcrumb(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        path: BreadcrumbPath,
        settings: BreadcrumbSettings,
        mut style: LayoutStyle,
    ) -> Result<Breadcrumb, ControlError> {
        if self.breadcrumbs.entries.len() >= 64 {
            return Err(invalid("At most 64 Breadcrumb controls are supported"));
        }
        let session = BreadcrumbSession::new(settings, path.clone())?;
        self.prepare_breadcrumb_path(&path)?;
        let label = label.into();
        self.measure_text(&label, 4096.0, false)?;
        style.kind = LayoutKind::Overlay;
        if style.height == Length::Auto {
            style.height = Length::Px(self.theme.tokens().control_height);
        }
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, &label)?;
            let bar = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            let editor = self.edit_box(container, &label, &path.text, fill())?;
            self.set_editor_settings(
                editor,
                EditorSettings {
                    max_length: Some(1024),
                    commit_on_focus_lost: false,
                    is_read_only: settings.read_only,
                    ..Default::default()
                },
            )?;
            self.edit_props(editor, |p| {
                p.visible = false;
                p.collapsed = true;
            })?;
            let overflow_button =
                self.breadcrumb_button(bar, "Все уровни пути", Some(IconSymbol::ChevronRight))?;
            let history_button =
                self.breadcrumb_button(bar, "История переходов", Some(IconSymbol::ChevronDown))?;
            let edit_button =
                self.breadcrumb_button(bar, "Изменить путь", Some(IconSymbol::Edit))?;
            let mut segments = Vec::new();
            for segment in &path.segments {
                segments.push(self.make_breadcrumb_segment(bar, segment)?);
            }
            let popup = self.flyout(container, settings.popup_size)?;
            self.edit_props(popup.content, |p| {
                p.style.kind = LayoutKind::Grid;
                p.style.grid_columns = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
                p.style.grid_rows = vec![
                    rust_desktop_ui_core::GridTrack::Auto,
                    rust_desktop_ui_core::GridTrack::Fr(1.0),
                ];
            })?;
            let status = self.label(
                popup.content,
                "",
                LayoutStyle {
                    grid_row: rust_desktop_ui_core::GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let list = self.list_box(
                popup.content,
                "Элементы пути",
                vec![],
                Default::default(),
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_row: rust_desktop_ui_core::GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let parts = Breadcrumb {
                container,
                editor,
                overflow_button,
                history_button,
                edit_button,
                popup,
                list,
            };
            self.breadcrumbs.entries.insert(
                container,
                Entry {
                    parts,
                    bar,
                    status,
                    segments,
                    session,
                    editing: false,
                    draft: path.text,
                    hidden: vec![],
                    hidden_focus: None,
                    popup_mode: None,
                    was_open: false,
                    validator: None,
                },
            );
            self.apply_breadcrumb_availability(container)?;
            Ok(parts)
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        result
    }
    fn breadcrumb_button(
        &mut self,
        parent: WidgetId,
        label: &str,
        icon: Option<IconSymbol>,
    ) -> Result<WidgetId, ControlError> {
        let button = self.button(parent, label, None, fill())?;
        self.set_button_chrome(button, ButtonChrome::Flat)?;
        if let Some(icon) = icon {
            self.set_button_icon(button, Some(ButtonIcon::new(icon)))?;
            self.set_button_content_layout(button, ButtonContentLayout::IconOnly)?;
        }
        Ok(button)
    }
    fn make_breadcrumb_segment(
        &mut self,
        bar: WidgetId,
        segment: &BreadcrumbSegment,
    ) -> Result<SegmentParts, ControlError> {
        let button = self.breadcrumb_button(bar, &segment.label, None)?;
        self.set_automation_name(button, &segment.label)?;
        let children = match self.breadcrumb_button(
            bar,
            &format!("Дочерние элементы: {}", segment.label),
            Some(IconSymbol::ChevronDown),
        ) {
            Ok(children) => children,
            Err(error) => {
                self.remove(button)?;
                return Err(error);
            }
        };
        Ok(SegmentParts {
            key: segment.key.clone(),
            button,
            children,
        })
    }
    fn prepare_breadcrumb_path(&mut self, path: &BreadcrumbPath) -> Result<(), ControlError> {
        path.validate()?;
        self.measure_text(&path.text, 4096.0, false)?;
        // Joining labels can join graphemes across boundaries; validate the
        // final accessible location and each generated child caption too.
        self.measure_text(
            &path
                .segments
                .iter()
                .map(|s| s.label.as_str())
                .collect::<Vec<_>>()
                .join(" / "),
            4096.0,
            false,
        )?;
        for segment in &path.segments {
            self.measure_text(&segment.label, 4096.0, false)?;
            self.measure_text(
                &format!("Дочерние элементы: {}", segment.label),
                4096.0,
                false,
            )?;
        }
        Ok(())
    }
    pub fn breadcrumb_parts(&self, owner: WidgetId) -> Option<Breadcrumb> {
        self.breadcrumbs.entries.get(&owner).map(|e| e.parts)
    }
    pub fn breadcrumb_path(&self, owner: WidgetId) -> Option<&BreadcrumbPath> {
        self.breadcrumbs
            .entries
            .get(&owner)
            .map(|e| &e.session.path)
    }
    pub fn breadcrumb_draft(&self, owner: WidgetId) -> Option<&str> {
        self.document(self.breadcrumb_parts(owner)?.editor)
            .map(TextDocument::text)
    }
    pub fn breadcrumb_is_editing(&self, owner: WidgetId) -> bool {
        self.breadcrumbs
            .entries
            .get(&owner)
            .is_some_and(|e| e.editing)
    }
    pub fn breadcrumb_settings(&self, owner: WidgetId) -> Option<BreadcrumbSettings> {
        self.breadcrumbs
            .entries
            .get(&owner)
            .map(|e| e.session.settings)
    }
    pub fn breadcrumb_status(&self, owner: WidgetId) -> Option<&BreadcrumbStatus> {
        self.breadcrumbs
            .entries
            .get(&owner)
            .map(|e| &e.session.navigation_status)
    }
    pub fn breadcrumb_children_status(&self, owner: WidgetId) -> Option<&BreadcrumbStatus> {
        self.breadcrumbs
            .entries
            .get(&owner)
            .map(|e| &e.session.children_status)
    }
    pub fn breadcrumb_history(&self, owner: WidgetId) -> Option<&[BreadcrumbPath]> {
        self.breadcrumbs
            .entries
            .get(&owner)
            .map(|e| e.session.history.as_slice())
    }
    pub fn breadcrumb_children(&self, owner: WidgetId) -> Option<&[BreadcrumbChild]> {
        self.breadcrumbs
            .entries
            .get(&owner)
            .map(|e| e.session.children.as_slice())
    }
    pub fn breadcrumb_segment_button(&self, owner: WidgetId, key: &str) -> Option<WidgetId> {
        self.breadcrumbs
            .entries
            .get(&owner)?
            .segments
            .iter()
            .find(|s| s.key == key)
            .map(|s| s.button)
    }
    pub fn breadcrumb_children_button(&self, owner: WidgetId, key: &str) -> Option<WidgetId> {
        self.breadcrumbs
            .entries
            .get(&owner)?
            .segments
            .iter()
            .find(|s| s.key == key)
            .map(|s| s.children)
    }
    pub fn set_breadcrumb_validator(
        &mut self,
        owner: WidgetId,
        validator: Option<BreadcrumbValidator>,
    ) -> Result<(), ControlError> {
        let entry = self
            .breadcrumbs
            .entries
            .get(&owner)
            .ok_or_else(|| invalid("Not a Breadcrumb"))?;
        if let Some(validator) = &validator {
            validator.check(&entry.session.path)?;
        }
        self.breadcrumbs.entries.get_mut(&owner).unwrap().validator = validator;
        Ok(())
    }
    pub(crate) fn chain_breadcrumb_validator(
        &mut self,
        owner: WidgetId,
        validator: BreadcrumbValidator,
    ) -> Result<(), ControlError> {
        let entry = self
            .breadcrumbs
            .entries
            .get_mut(&owner)
            .ok_or_else(|| invalid("Not a Breadcrumb"))?;
        let old = entry.validator.take();
        entry.validator = Some(BreadcrumbValidator::new(move |path| {
            if let Some(old) = &old {
                old.check(path).map_err(|e| e.to_string())?;
            }
            validator.check(path).map_err(|e| e.to_string())
        }));
        Ok(())
    }
    /// Load an accepted application location without adding navigation history.
    /// A dirty editor rejects the replacement; cancel explicitly before retrying.
    pub fn set_breadcrumb_path(
        &mut self,
        owner: WidgetId,
        path: BreadcrumbPath,
    ) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        let entry = &self.breadcrumbs.entries[&owner];
        if self
            .breadcrumb_draft(owner)
            .is_some_and(|draft| draft != entry.session.path.text)
        {
            return Err(invalid(
                "Cancel the breadcrumb draft before replacing the accepted path",
            ));
        }
        self.install_breadcrumb_path(owner, path, false)
    }
    fn install_breadcrumb_path(
        &mut self,
        owner: WidgetId,
        path: BreadcrumbPath,
        navigation: bool,
    ) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        let entry = &self.breadcrumbs.entries[&owner];
        entry.session.preflight_path(&path, navigation)?;
        if let Some(validator) = &entry.validator {
            validator.check(&path)?;
        }
        if self
            .document(entry.parts.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            return Err(invalid("Finish breadcrumb composition before navigation"));
        }
        let parts = entry.parts;
        let bar = entry.bar;
        let previous = entry.segments.clone();
        self.prepare_breadcrumb_path(&path)?;
        self.preflight_editor_text(parts.editor, &path.text)?;
        let mut created: Vec<SegmentParts> = Vec::new();
        let mut next = Vec::new();
        for segment in &path.segments {
            if let Some(old) = previous.iter().find(|s| s.key == segment.key) {
                next.push(old.clone());
            } else {
                match self.make_breadcrumb_segment(bar, segment) {
                    Ok(new) => {
                        created.push(new.clone());
                        next.push(new);
                    }
                    Err(error) => {
                        for new in created {
                            self.remove(new.button)?;
                            self.remove(new.children)?;
                        }
                        return Err(error);
                    }
                }
            }
        }
        let owned_focus = self.runtime.focused().is_some_and(|focused| {
            self.flyout_descendant(focused, owner)
                || self.flyout_descendant(focused, parts.popup.container)
        });
        self.close_breadcrumb_popup(owner, false)?;
        for (segment, controls) in path.segments.iter().zip(&next) {
            self.set_label(controls.button, &segment.label)?;
            self.set_automation_name(controls.button, &segment.label)?;
            self.set_label(
                controls.children,
                format!("Дочерние элементы: {}", segment.label),
            )?;
        }
        let outcome = self.set_editor_value(parts.editor, &path.text)?;
        if !matches!(outcome, CommitOutcome::Committed | CommitOutcome::Unchanged) {
            return Err(invalid(
                "The owned breadcrumb editor rejected its accepted location",
            ));
        }
        let entry = self.breadcrumbs.entries.get_mut(&owner).unwrap();
        entry.segments = next;
        entry.draft = path.text.clone();
        entry.editing = false;
        entry.session.publish_path(path, navigation);
        for old in previous {
            if !self.breadcrumbs.entries[&owner]
                .segments
                .iter()
                .any(|s| s.key == old.key)
            {
                self.remove(old.button)?;
                self.remove(old.children)?;
            }
        }
        self.show_breadcrumb_editor(owner, false)?;
        self.apply_breadcrumb_availability(owner)?;
        if owned_focus {
            self.focus_breadcrumb_end(owner)?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn drain_breadcrumb_navigation_requests(&mut self) -> Vec<BreadcrumbNavigationRequest> {
        self.breadcrumbs
            .entries
            .values_mut()
            .filter_map(|e| e.session.take_navigation())
            .collect()
    }
    pub fn drain_breadcrumb_children_requests(&mut self) -> Vec<BreadcrumbChildrenRequest> {
        self.breadcrumbs
            .entries
            .values_mut()
            .filter_map(|e| e.session.take_children())
            .collect()
    }
    pub fn drain_breadcrumb_events(&mut self) -> Vec<BreadcrumbEvent> {
        self.breadcrumbs
            .entries
            .iter_mut()
            .flat_map(|(&owner, entry)| {
                std::mem::take(&mut entry.session.events)
                    .into_iter()
                    .map(move |kind| BreadcrumbEvent { owner, kind })
            })
            .collect()
    }
    pub fn complete_breadcrumb_navigation(
        &mut self,
        request: &BreadcrumbNavigationRequest,
        result: Result<BreadcrumbPath, String>,
    ) -> Result<BreadcrumbCompletion, ControlError> {
        self.sync_breadcrumbs()?;
        let owner = request.owner();
        if !self.breadcrumb_active(owner)
            || !self
                .breadcrumbs
                .entries
                .get(&owner)
                .is_some_and(|e| e.session.matches_navigation(request))
        {
            return Ok(BreadcrumbCompletion::Stale);
        }
        match result {
            Ok(path) => {
                self.install_breadcrumb_path(owner, path, true)?;
                Ok(BreadcrumbCompletion::Applied)
            }
            Err(message) => {
                super::breadcrumb_model::validate_error(&message)?;
                self.measure_text(&message, 4096.0, false)?;
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .session
                    .reject_navigation(message.clone())?;
                let editor = self.breadcrumbs.entries[&owner].parts.editor;
                self.controls.get_mut(&editor).unwrap().validation_error = Some(message);
                self.scene_dirty = true;
                Ok(BreadcrumbCompletion::Rejected)
            }
        }
    }
    pub fn complete_breadcrumb_children(
        &mut self,
        request: &BreadcrumbChildrenRequest,
        result: Result<Vec<BreadcrumbChild>, String>,
    ) -> Result<BreadcrumbCompletion, ControlError> {
        self.sync_breadcrumbs()?;
        let owner = request.owner();
        if !self.breadcrumb_active(owner)
            || !self
                .breadcrumbs
                .entries
                .get(&owner)
                .is_some_and(|e| e.session.matches_children(request))
        {
            return Ok(BreadcrumbCompletion::Stale);
        }
        self.check_breadcrumb_parts(owner)?;
        let parts = self.breadcrumbs.entries[&owner].parts;
        match result {
            Ok(children) => {
                self.breadcrumbs.entries[&owner]
                    .session
                    .validate_children(&children)?;
                for child in &children {
                    self.measure_text(&child.segment.label, 4096.0, false)?;
                }
                let items = child_items(&children);
                self.preflight_virtual_list_items(parts.list.container, &items)?;
                self.set_selection_items(parts.list.container, items)?;
                self.set_selected_keys(parts.list.container, &[])?;
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .session
                    .publish_children(children);
                self.set_label(self.breadcrumbs.entries[&owner].status, "")?;
                Ok(BreadcrumbCompletion::Applied)
            }
            Err(message) => {
                super::breadcrumb_model::validate_error(&message)?;
                self.measure_text(&message, 4096.0, false)?;
                self.breadcrumbs.entries[&owner].session.preflight_event()?;
                self.set_selection_items(parts.list.container, vec![])?;
                self.set_label(self.breadcrumbs.entries[&owner].status, &message)?;
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .session
                    .reject_children(message)?;
                Ok(BreadcrumbCompletion::Rejected)
            }
        }
    }
}
fn child_items(children: &[BreadcrumbChild]) -> Vec<SelectionItem> {
    children
        .iter()
        .map(|child| SelectionItem {
            key: child.segment.key.clone(),
            text: child.segment.label.clone(),
            columns: vec![],
            enabled: child.enabled,
        })
        .collect()
}
fn fill() -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Percent(1.0),
        ..Default::default()
    }
}
fn invalid(message: &str) -> ControlError {
    ControlError::Invalid(message.into())
}
