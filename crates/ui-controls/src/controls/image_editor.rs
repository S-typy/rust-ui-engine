//! Retained image editing with explicit apply/cancel and application-owned I/O.
use super::*;
use rust_desktop_ui_core::{Bitmap, GridPlacement, GridTrack, ImagePrimitive, ImageSampling};
use rust_desktop_ui_images::{
    ImageEditLimits, ImageEditSession, ImageError, PixelCrop, QuarterTurn, encode_bitmap_png,
};
use std::{
    collections::VecDeque,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
mod input;
mod paint;
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageEditorSettings {
    pub read_only: bool,
    pub limits: ImageEditLimits,
}
#[derive(Clone, Debug)]
pub struct ImageEditorParts {
    pub container: WidgetId,
    pub viewport: WidgetId,
    pub status: WidgetId,
    pub load: WidgetId,
    pub save: WidgetId,
    pub apply: WidgetId,
    pub cancel: WidgetId,
}
#[derive(Clone, Debug)]
pub struct ImageLoadToken {
    owner: WidgetId,
    identity: Arc<AtomicBool>,
}
impl ImageLoadToken {
    pub fn is_cancelled(&self) -> bool {
        self.identity.load(Ordering::Relaxed)
    }
}
#[derive(Clone, Debug)]
pub enum ImageEditorEvent {
    Applied {
        owner: WidgetId,
        value: Option<Bitmap>,
    },
    LoadRequested {
        owner: WidgetId,
        token: ImageLoadToken,
    },
    SaveRequested {
        owner: WidgetId,
        value: Bitmap,
    },
}
#[derive(Clone)]
pub struct ImageEditorValidator(Rc<ImageValidation>);
type ImageValidation = dyn Fn(Option<&Bitmap>) -> Result<(), String>;
impl ImageEditorValidator {
    pub fn new(check: impl Fn(Option<&Bitmap>) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(check))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageViewport {
    pub zoom: f32,
    pub pan: Point,
    pub fit: bool,
}
impl Default for ImageViewport {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Point::default(),
            fit: true,
        }
    }
}
#[derive(Clone, Copy)]
enum Action {
    Load,
    Save,
    Fit,
    Actual,
    ZoomOut,
    ZoomIn,
    Rotate,
    Crop,
    Undo,
    Redo,
    Apply,
    Cancel,
}
struct Entry {
    parts: ImageEditorParts,
    parents: Vec<(WidgetId, WidgetId)>,
    buttons: Vec<(WidgetId, Action)>,
    session: ImageEditSession,
    read_only: bool,
    viewer: bool,
    view: ImageViewport,
    crop: Option<PixelCrop>,
    crop_mode: bool,
    validator: Option<ImageEditorValidator>,
    load: Option<ImageLoadToken>,
    error: Option<String>,
}
#[derive(Clone, Copy)]
struct Drag {
    owner: WidgetId,
    start: Point,
    pan: Point,
    original_view: ImageViewport,
    prior_crop: Option<PixelCrop>,
    crop: bool,
    current: Point,
}
#[derive(Default)]
pub(super) struct ImageEditors {
    entries: HashMap<WidgetId, Entry>,
    events: VecDeque<ImageEditorEvent>,
    drag: Option<Drag>,
}
fn invalid(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}
fn cell(row: u16) -> LayoutStyle {
    LayoutStyle {
        grid_row: GridPlacement {
            start: Some(row + 1),
            span: 1,
        },
        ..Default::default()
    }
}
impl Controls {
    pub fn image_editor(
        &mut self,
        parent: WidgetId,
        value: Option<Bitmap>,
        settings: ImageEditorSettings,
        mut style: LayoutStyle,
    ) -> Result<ImageEditorParts, ControlError> {
        if self.image_editors.entries.len() >= 16 {
            return Err(invalid("At most 16 image editors are supported"));
        }
        let session = ImageEditSession::new(value, settings.limits).map_err(invalid)?;
        style.kind = LayoutKind::Grid;
        style.grid_rows = vec![GridTrack::Auto, GridTrack::Fr(1.0), GridTrack::Px(28.0)];
        style.gap = 4.0;
        let owner = self.transparent_panel(parent, style)?;
        let result = (|| {
            let toolbar = self.transparent_panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Wrap(Axis::Horizontal),
                    gap: 4.0,
                    ..cell(0)
                },
            )?;
            let mut buttons = Vec::new();
            for (label, action) in [
                ("Открыть", Action::Load),
                ("PNG", Action::Save),
                ("Вписать", Action::Fit),
                ("1:1", Action::Actual),
                ("−", Action::ZoomOut),
                ("+", Action::ZoomIn),
                ("Поворот", Action::Rotate),
                ("Обрезать", Action::Crop),
                ("Назад", Action::Undo),
                ("Вперёд", Action::Redo),
                ("Принять", Action::Apply),
                ("Отмена", Action::Cancel),
            ] {
                let button_width = (self.measure_text(label, 1024.0, false)?.width + 32.0)
                    .ceil()
                    .max(82.0);
                let b = self.button(
                    toolbar,
                    label,
                    None,
                    LayoutStyle {
                        width: Length::Px(button_width),
                        height: Length::Px(34.0),
                        ..Default::default()
                    },
                )?;
                buttons.push((b, action));
            }
            let viewport = self.transparent_panel(owner, cell(1))?;
            self.set_role(viewport, Role::Image);
            self.set_automation_name(viewport, "Изображение")?;
            self.edit_props(viewport, |p| {
                p.focusable = true;
                p.clip = true;
            })?;
            let status = self.label(
                owner,
                "Нет изображения",
                LayoutStyle {
                    height: Length::Px(28.0),
                    ..cell(2)
                },
            )?;
            let parts = ImageEditorParts {
                container: owner,
                viewport,
                status,
                load: buttons[0].0,
                save: buttons[1].0,
                apply: buttons[10].0,
                cancel: buttons[11].0,
            };
            let mut parents = vec![(toolbar, owner), (viewport, owner), (status, owner)];
            parents.extend(buttons.iter().map(|(id, _)| (*id, toolbar)));
            Ok::<_, ControlError>((parts, parents, buttons))
        })();
        let (parts, parents, buttons) = match result {
            Ok(v) => v,
            Err(e) => {
                self.remove(owner)?;
                return Err(e);
            }
        };
        self.image_editors.entries.insert(
            owner,
            Entry {
                parts: parts.clone(),
                parents,
                buttons,
                session,
                read_only: settings.read_only,
                viewer: false,
                view: ImageViewport::default(),
                crop: None,
                crop_mode: false,
                validator: None,
                load: None,
                error: None,
            },
        );
        self.refresh_image_editor(owner)?;
        Ok(parts)
    }
    pub fn image_viewer(
        &mut self,
        parent: WidgetId,
        value: Option<Bitmap>,
        style: LayoutStyle,
    ) -> Result<ImageEditorParts, ControlError> {
        let parts = self.image_editor(
            parent,
            value,
            ImageEditorSettings {
                read_only: true,
                ..Default::default()
            },
            style,
        )?;
        self.image_editors
            .entries
            .get_mut(&parts.container)
            .unwrap()
            .viewer = true;
        self.refresh_image_editor(parts.container)?;
        Ok(parts)
    }
    fn check_image_editor(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .image_editors
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown image editor"))?;
        if !self.runtime.tree().contains(id)
            || e.parents
                .iter()
                .any(|(id, parent)| self.runtime.tree().parent(*id) != Some(*parent))
        {
            return Err(invalid(
                "Image editor required parts were removed or reparented",
            ));
        }
        Ok(())
    }
    fn image_editor_editable(&self, id: WidgetId) -> bool {
        self.image_editors.entries.get(&id).is_some_and(|e| {
            !e.read_only && self.effective_enabled(id) && self.effective_enabled(e.parts.viewport)
        })
    }
    pub fn image_editor_parts(&self, id: WidgetId) -> Option<&ImageEditorParts> {
        self.image_editors.entries.get(&id).map(|e| &e.parts)
    }
    pub fn image_editor_value(&self, id: WidgetId) -> Option<Option<&Bitmap>> {
        self.image_editors
            .entries
            .get(&id)
            .map(|e| e.session.accepted())
    }
    pub fn image_editor_draft(&self, id: WidgetId) -> Option<Option<&Bitmap>> {
        self.image_editors
            .entries
            .get(&id)
            .map(|e| e.session.draft())
    }
    pub fn image_editor_dirty(&self, id: WidgetId) -> Option<bool> {
        self.image_editors
            .entries
            .get(&id)
            .map(|e| e.session.is_dirty())
    }
    pub fn image_editor_error(&self, id: WidgetId) -> Option<&str> {
        self.image_editors
            .entries
            .get(&id)
            .and_then(|e| e.error.as_deref())
    }
    pub fn image_editor_read_only(&self, id: WidgetId) -> Option<bool> {
        self.image_editors.entries.get(&id).map(|e| e.read_only)
    }
    pub fn set_image_editor_read_only(
        &mut self,
        id: WidgetId,
        value: bool,
    ) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        if self.image_editors.entries[&id].viewer && !value {
            return Err(invalid("ImageViewer is always read-only"));
        }
        self.image_editors.entries.get_mut(&id).unwrap().read_only = value;
        self.sync_image_editors()
    }
    pub fn set_image_editor_validator(
        &mut self,
        id: WidgetId,
        value: Option<ImageEditorValidator>,
    ) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        self.image_editors.entries.get_mut(&id).unwrap().validator = value;
        Ok(())
    }
    pub fn image_editor_viewport(&self, id: WidgetId) -> Option<ImageViewport> {
        self.image_editors.entries.get(&id).map(|e| e.view)
    }
    pub fn set_image_editor_viewport(
        &mut self,
        id: WidgetId,
        view: ImageViewport,
    ) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        if !view.zoom.is_finite()
            || !(0.05..=32.0).contains(&view.zoom)
            || !view.pan.is_finite()
            || view.pan.x.abs() > 262144.0
            || view.pan.y.abs() > 262144.0
        {
            return Err(invalid(
                "Image viewport requires zoom 0.05..32 and finite bounded pan",
            ));
        }
        self.scene_dirty |= self.image_editors.entries[&id].view != view;
        self.image_editors.entries.get_mut(&id).unwrap().view = view;
        self.refresh_image_editor(id)
    }
    pub fn set_image_editor_value(
        &mut self,
        id: WidgetId,
        value: Option<Bitmap>,
        discard_draft: bool,
    ) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        let e = &self.image_editors.entries[&id];
        if let Some(v) = &e.validator {
            (v.0)(value.as_ref()).map_err(invalid)?;
        }
        let mut candidate = e.session.clone();
        candidate.replace(value, discard_draft).map_err(invalid)?;
        self.cancel_image_load(id);
        self.cancel_image_owner_gesture(id)?;
        let e = self.image_editors.entries.get_mut(&id).unwrap();
        e.session = candidate;
        self.scene_dirty = true;
        e.crop = None;
        e.error = None;
        self.refresh_image_editor(id)
    }
    fn change_image_draft(
        &mut self,
        id: WidgetId,
        change: impl FnOnce(&mut ImageEditSession) -> Result<(), ImageError>,
    ) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        if !self.image_editor_editable(id) {
            return Err(invalid("Image editor is read-only or unavailable"));
        }
        let mut candidate = self.image_editors.entries[&id].session.clone();
        change(&mut candidate).map_err(invalid)?;
        self.cancel_image_load(id);
        self.cancel_image_owner_gesture(id)?;
        let e = self.image_editors.entries.get_mut(&id).unwrap();
        e.session = candidate;
        self.scene_dirty = true;
        e.crop = None;
        e.error = None;
        self.refresh_image_editor(id)
    }
    pub fn set_image_editor_draft(
        &mut self,
        id: WidgetId,
        value: Option<Bitmap>,
    ) -> Result<(), ControlError> {
        self.change_image_draft(id, |s| s.set_draft(value))
    }
    pub fn crop_image_editor(&mut self, id: WidgetId, crop: PixelCrop) -> Result<(), ControlError> {
        self.change_image_draft(id, |s| s.crop(crop))
    }
    pub fn rotate_image_editor(
        &mut self,
        id: WidgetId,
        turn: QuarterTurn,
    ) -> Result<(), ControlError> {
        self.change_image_draft(id, |s| s.rotate(turn))
    }
    pub fn undo_image_editor(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.change_image_draft(id, |s| {
            s.undo();
            Ok(())
        })
    }
    pub fn redo_image_editor(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.change_image_draft(id, |s| {
            s.redo();
            Ok(())
        })
    }
    fn check_image_event_budget(&self, value: Option<&Bitmap>) -> Result<(), ControlError> {
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0usize;
        for bitmap in self
            .image_editors
            .events
            .iter()
            .filter_map(|e| match e {
                ImageEditorEvent::Applied { value, .. } => value.as_ref(),
                ImageEditorEvent::SaveRequested { value, .. } => Some(value),
                _ => None,
            })
            .chain(value)
        {
            if seen.insert(bitmap.id()) {
                bytes += bitmap.pixels().len();
            }
        }
        if self.image_editors.events.len() >= 256 || bytes > 128 * 1024 * 1024 {
            return Err(invalid(
                "Image events exceed 256 entries or 128 MiB pixel references",
            ));
        }
        Ok(())
    }
    pub fn apply_image_editor(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        self.check_image_editor(id)?;
        if !self.image_editor_editable(id) {
            return Err(invalid("Image editor is read-only or unavailable"));
        }
        let e = &self.image_editors.entries[&id];
        if let Some(v) = &e.validator {
            (v.0)(e.session.draft()).map_err(invalid)?;
        }
        if e.session.is_dirty() {
            self.check_image_event_budget(e.session.draft())?;
        }
        self.cancel_image_load(id);
        self.cancel_image_owner_gesture(id)?;
        let e = self.image_editors.entries.get_mut(&id).unwrap();
        e.error = None;
        let changed = e.session.apply();
        if changed {
            self.image_editors
                .events
                .push_back(ImageEditorEvent::Applied {
                    owner: id,
                    value: e.session.accepted().cloned(),
                });
        }
        self.refresh_image_editor(id)?;
        Ok(changed)
    }
    pub fn cancel_image_editor(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        self.cancel_image_load(id);
        self.cancel_image_owner_gesture(id)?;
        let e = self.image_editors.entries.get_mut(&id).unwrap();
        e.session.cancel();
        self.scene_dirty = true;
        e.crop = None;
        e.crop_mode = false;
        e.error = None;
        self.refresh_image_editor(id)
    }
    pub fn export_image_editor_png(&self, id: WidgetId) -> Result<Vec<u8>, ControlError> {
        self.check_image_editor(id)?;
        encode_bitmap_png(
            self.image_editors.entries[&id]
                .session
                .accepted()
                .ok_or_else(|| invalid("No accepted image to export"))?,
        )
        .map_err(invalid)
    }
    pub fn take_image_editor_events(&mut self) -> Vec<ImageEditorEvent> {
        self.image_editors.events.drain(..).collect()
    }
    pub fn begin_image_editor_load(
        &mut self,
        id: WidgetId,
    ) -> Result<ImageLoadToken, ControlError> {
        self.check_image_editor(id)?;
        if !self.image_editor_editable(id) {
            return Err(invalid("Image editor is read-only or unavailable"));
        }
        self.cancel_image_load(id);
        let token = ImageLoadToken {
            owner: id,
            identity: Arc::new(AtomicBool::new(false)),
        };
        self.image_editors.entries.get_mut(&id).unwrap().load = Some(token.clone());
        Ok(token)
    }
    pub fn complete_image_editor_load(
        &mut self,
        token: &ImageLoadToken,
        value: Result<Bitmap, String>,
    ) -> Result<bool, ControlError> {
        self.sync_image_editors()?;
        let live = !token.is_cancelled()
            && self
                .image_editors
                .entries
                .get(&token.owner)
                .and_then(|e| e.load.as_ref())
                .is_some_and(|t| Arc::ptr_eq(&t.identity, &token.identity));
        if !live {
            return Ok(false);
        }
        match value {
            Ok(value) => self.set_image_editor_draft(token.owner, Some(value))?,
            Err(e) => {
                self.image_editor_report_error(token.owner, &e)?;
                self.cancel_image_load(token.owner);
            }
        }
        Ok(true)
    }
    fn cancel_image_load(&mut self, id: WidgetId) {
        if let Some(token) = self
            .image_editors
            .entries
            .get_mut(&id)
            .and_then(|e| e.load.take())
        {
            token.identity.store(true, Ordering::Relaxed);
        }
    }
    pub fn image_editor_report_error(
        &mut self,
        id: WidgetId,
        error: &str,
    ) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        let text: String = error.chars().take(160).collect();
        let text = if self.measure_text(&text, 1024.0, false).is_ok() {
            text
        } else {
            "Image operation failed".into()
        };
        self.image_editors.entries.get_mut(&id).unwrap().error = Some(text);
        self.refresh_image_editor(id)
    }
    fn refresh_image_editor(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        let e = &self.image_editors.entries[&id];
        let toolbar = e.parents[0].0;
        let viewer = e.viewer;
        let all_buttons = e.buttons.clone();
        for (button, action) in &all_buttons {
            let visible = !viewer
                || matches!(
                    action,
                    Action::Save | Action::Fit | Action::Actual | Action::ZoomOut | Action::ZoomIn
                );
            self.edit_props(*button, |p| {
                p.visible = visible;
                p.collapsed = !visible;
            })?;
        }
        let buttons = all_buttons
            .into_iter()
            .filter(|(_, action)| {
                !viewer
                    || matches!(
                        action,
                        Action::Save
                            | Action::Fit
                            | Action::Actual
                            | Action::ZoomOut
                            | Action::ZoomIn
                    )
            })
            .collect::<Vec<_>>();
        let width = self
            .runtime
            .bounds(id)
            .zip(self.runtime.clip(id))
            .and_then(|(b, c)| b.intersection(c))
            .map_or(640.0, |b| b.width)
            .max(0.0);
        let cell_width = buttons
            .iter()
            .filter_map(|(b, _)| self.runtime.tree().node(*b))
            .filter_map(|n| {
                if let Length::Px(w) = n.props.style.width {
                    Some(w)
                } else {
                    None
                }
            })
            .fold(82.0f32, f32::max);
        let columns = (((width + 4.0) / (cell_width + 4.0)).floor().max(1.0) as usize).min(12);
        let rows = buttons.len().div_ceil(columns);
        self.edit_props(toolbar, |p| {
            p.style.kind = LayoutKind::Grid;
            p.style.width = Length::Px(width);
            p.style.height = Length::Px(rows as f32 * 38.0 - 4.0);
            p.style.grid_columns = vec![GridTrack::Px(cell_width); columns];
            p.style.grid_rows = vec![GridTrack::Px(34.0); rows];
        })?;
        for (index, (button, _)) in buttons.iter().enumerate() {
            self.edit_props(*button, |p| {
                p.style.grid_column = GridPlacement {
                    start: Some((index % columns + 1) as u16),
                    span: 1,
                };
                p.style.grid_row = GridPlacement {
                    start: Some((index / columns + 1) as u16),
                    span: 1,
                };
            })?;
        }
        let e = &self.image_editors.entries[&id];
        let parts = e.parts.clone();
        let buttons = e.buttons.clone();
        let dirty = e.session.is_dirty();
        let image = e.session.draft().is_some();
        let undo = e.session.can_undo();
        let redo = e.session.can_redo();
        let editable = self.image_editor_editable(id);
        let save = e.session.accepted().is_some();
        let text = e.error.clone().unwrap_or_else(|| {
            e.session.draft().map_or_else(
                || "Нет изображения".into(),
                |b| {
                    format!(
                        "{} × {} · {}{}",
                        b.width(),
                        b.height(),
                        if e.view.fit {
                            "Вписать".into()
                        } else {
                            format!("{:.0}%", e.view.zoom * 100.0)
                        },
                        if dirty { " · Не принято" } else { "" }
                    )
                },
            )
        });
        if self
            .controls
            .get(&parts.status)
            .is_some_and(|c| c.label != text)
        {
            self.set_label(parts.status, text.clone())?;
        }
        let accessible = format!("Изображение. {text}");
        if self
            .controls
            .get(&parts.viewport)
            .and_then(|c| c.automation_name.as_deref())
            != Some(accessible.as_str())
        {
            self.set_automation_name(parts.viewport, accessible)?;
        }
        let mut availability_changed = false;
        for (button, action) in buttons {
            let enabled = match action {
                Action::Load => editable,
                Action::Save => save,
                Action::Rotate | Action::Crop => editable && image,
                Action::Undo => editable && undo,
                Action::Redo => editable && redo,
                Action::Apply => editable && dirty,
                Action::Cancel => dirty,
                _ => image,
            };
            if let Some(control) = self.controls.get_mut(&button)
                && control.enabled != enabled
            {
                control.enabled = enabled;
                availability_changed = true;
            }
            self.edit_props(button, |p| p.enabled = enabled)?;
        }
        if availability_changed {
            self.scene_dirty = true;
            self.refresh_paints()?;
        }
        Ok(())
    }
    pub(super) fn sync_image_editors(&mut self) -> Result<(), ControlError> {
        let ids = self
            .image_editors
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            if let Err(error) = self.check_image_editor(id) {
                self.cancel_image_load(id);
                if self.image_editors.drag.is_some_and(|d| d.owner == id) {
                    self.cancel_image_gesture()?;
                }
                return Err(error);
            }
            if self.image_editors.drag.is_some_and(|d| {
                d.owner == id
                    && self.runtime.captured()
                        != Some(self.image_editors.entries[&id].parts.viewport)
            }) {
                self.cancel_image_gesture()?;
            }
            if !self.image_editor_editable(id) {
                self.cancel_image_load(id);
                if self.image_editors.drag.is_some_and(|d| {
                    d.owner == id
                        && (d.crop
                            || !self.effective_enabled(id)
                            || !self
                                .effective_enabled(self.image_editors.entries[&id].parts.viewport))
                }) {
                    self.cancel_image_gesture()?;
                }
            }
            self.refresh_image_editor(id)?;
        }
        Ok(())
    }
    pub(super) fn prune_image_editors(&mut self) {
        let ids = self
            .image_editors
            .entries
            .keys()
            .copied()
            .filter(|id| !self.runtime.tree().contains(*id))
            .collect::<Vec<_>>();
        for id in ids {
            self.cancel_image_load(id);
            self.image_editors.entries.remove(&id);
        }
        self.image_editors.events.retain(|e| {
            let id = match e {
                ImageEditorEvent::Applied { owner, .. }
                | ImageEditorEvent::LoadRequested { owner, .. }
                | ImageEditorEvent::SaveRequested { owner, .. } => owner,
            };
            self.image_editors.entries.contains_key(id)
        });
        if self
            .image_editors
            .drag
            .is_some_and(|d| !self.image_editors.entries.contains_key(&d.owner))
        {
            self.image_editors.drag = None;
        }
    }
    pub(super) fn preflight_image_editor_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        for (owner, e) in &self.image_editors.entries {
            if !self.flyout_descendant(*owner, id)
                && e.parents
                    .iter()
                    .any(|(part, _)| self.flyout_descendant(*part, id))
            {
                return Err(invalid(
                    "Remove the whole image editor, not its required parts",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn activate_image_editor(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let Some((id, action)) = self.image_editors.entries.iter().find_map(|(id, e)| {
            e.buttons
                .iter()
                .find(|(b, _)| *b == button)
                .map(|(_, a)| (*id, *a))
        }) else {
            return Ok(false);
        };
        if !self.effective_enabled(button) {
            return Ok(true);
        }
        let result = self.image_editor_action(id, action);
        if let Err(ControlError::Invalid(text)) = result {
            self.image_editor_report_error(id, &text)?;
        } else {
            result?;
        }
        Ok(true)
    }
    fn image_editor_action(&mut self, id: WidgetId, action: Action) -> Result<(), ControlError> {
        self.check_image_editor(id)?;
        match action {
            Action::Load => {
                if self.image_editors.events.len() >= 256 {
                    return Err(invalid("Image event queue is full"));
                }
                let token = self.begin_image_editor_load(id)?;
                self.image_editors
                    .events
                    .push_back(ImageEditorEvent::LoadRequested { owner: id, token });
            }
            Action::Save => {
                self.check_image_event_budget(self.image_editors.entries[&id].session.accepted())?;
                if let Some(value) = self.image_editors.entries[&id].session.accepted().cloned() {
                    self.image_editors
                        .events
                        .push_back(ImageEditorEvent::SaveRequested { owner: id, value });
                }
            }
            Action::Fit => self.set_image_editor_viewport(id, ImageViewport::default())?,
            Action::Actual => self.set_image_editor_viewport(
                id,
                ImageViewport {
                    fit: false,
                    ..Default::default()
                },
            )?,
            Action::ZoomIn | Action::ZoomOut => {
                let mut v = self.image_editors.entries[&id].view;
                if v.fit {
                    v.zoom = self
                        .image_editor_mapping(id)
                        .map_or(1.0, |m| m.1)
                        .clamp(0.05, 32.0);
                }
                v.fit = false;
                v.zoom = (v.zoom
                    * if matches!(action, Action::ZoomIn) {
                        1.25
                    } else {
                        0.8
                    })
                .clamp(0.05, 32.0);
                self.set_image_editor_viewport(id, v)?;
            }
            Action::Rotate => self.rotate_image_editor(id, QuarterTurn::Clockwise)?,
            Action::Undo => self.undo_image_editor(id)?,
            Action::Redo => self.redo_image_editor(id)?,
            Action::Apply => {
                self.apply_image_editor(id)?;
            }
            Action::Cancel => self.cancel_image_editor(id)?,
            Action::Crop => {
                let e = self.image_editors.entries.get_mut(&id).unwrap();
                if let Some(crop) = e.crop {
                    self.crop_image_editor(id, crop)?;
                } else {
                    e.crop_mode = !e.crop_mode;
                    let viewport = e.parts.viewport;
                    self.try_popup_focus(viewport)?;
                }
                self.scene_dirty = true;
            }
        }
        Ok(())
    }
}
