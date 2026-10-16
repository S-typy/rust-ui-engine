//! Retained selection of installed font families, faces and logical font size.
use super::*;
pub use rust_desktop_ui_core::{FontDescriptor, FontSlant};
pub use rust_desktop_ui_text::FontFaceInfo;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct FontPickerSettings {
    pub preview_text: String,
    pub read_only: bool,
}
impl Default for FontPickerSettings {
    fn default() -> Self {
        Self {
            preview_text: "Aa Бб 123 · Текст / Preview".into(),
            read_only: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontPicker {
    pub container: WidgetId,
    pub fields: WidgetId,
    pub family: EditorControl,
    pub face: EditorControl,
    pub size: EditorControl,
    pub preview: WidgetId,
    pub status: WidgetId,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FontResolution {
    pub family_available: bool,
    pub actual_fonts: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FontChanged {
    pub id: WidgetId,
    pub value: FontDescriptor,
}
type FontValidation = dyn Fn(&FontDescriptor) -> Result<(), String>;
#[derive(Clone)]
pub struct FontValidator(Rc<FontValidation>);
impl FontValidator {
    pub fn new(f: impl Fn(&FontDescriptor) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(f))
    }
    fn check(&self, value: &FontDescriptor) -> Result<(), ControlError> {
        (self.0)(value).map_err(ControlError::Invalid)
    }
}
struct Entry {
    parts: FontPicker,
    value: FontDescriptor,
    settings: FontPickerSettings,
    faces: Vec<FontFaceInfo>,
    resolution: FontResolution,
    validator: Option<FontValidator>,
    error: Option<String>,
}
#[derive(Default)]
pub(super) struct FontPickers {
    entries: HashMap<WidgetId, Entry>,
    families: Option<Vec<String>>,
    changes: Vec<FontChanged>,
}

impl Controls {
    /// System discovery is cached once for this Controls instance.
    pub fn font_families(&mut self) -> Result<&[String], ControlError> {
        if self.font_pickers.families.is_none() {
            self.font_pickers.families = Some(self.engine.font_families().map_err(text_error)?);
        }
        Ok(self.font_pickers.families.as_deref().unwrap())
    }
    pub fn font_faces(&mut self, family: &str) -> Result<Vec<FontFaceInfo>, ControlError> {
        self.engine.font_faces(family).map_err(text_error)
    }
    pub fn font_picker(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: FontDescriptor,
        settings: FontPickerSettings,
        mut style: LayoutStyle,
    ) -> Result<FontPicker, ControlError> {
        if self.font_pickers.entries.len() >= 64 {
            return Err(font_error("At most 64 FontPicker controls are supported"));
        }
        let (value, faces, resolution) = self.prepare_font_value(value, &settings.preview_text)?;
        let family_items: Vec<_> = self
            .font_families()?
            .iter()
            .map(|name| SelectionItem::new(name, name))
            .collect();
        let label = label.into();
        self.measure_text(&label, 4096.0, false)?;
        style.kind = LayoutKind::Flex(Axis::Vertical);
        style.gap = 8.0;
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, &label)?;
            let row = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        rust_desktop_ui_core::GridTrack::Fr(2.0),
                        rust_desktop_ui_core::GridTrack::Fr(2.0),
                        rust_desktop_ui_core::GridTrack::Fr(1.0),
                    ],
                    gap: 8.0,
                    ..Default::default()
                },
            )?;
            let family = self.combo_box(
                row,
                format!("{label}: family"),
                family_items,
                ComboBoxSettings::default(),
                column(1),
            )?;
            let face = self.combo_box(
                row,
                format!("{label}: style"),
                face_items(&faces),
                ComboBoxSettings::default(),
                column(2),
            )?;
            let size = self.spin_box(
                row,
                format!("{label}: size"),
                f64::from(value.size),
                SpinBoxSettings {
                    minimum: 1.0,
                    maximum: 512.0,
                    step: 1.0,
                },
                column(3),
            )?;
            let preview = self.label(
                container,
                "",
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Px(96.0),
                    ..Default::default()
                },
            )?;
            self.set_text_style(
                preview,
                TextStyle {
                    wrap: false,
                    ..Default::default()
                },
            )?;
            let status = self.label(
                container,
                "",
                LayoutStyle {
                    width: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            self.set_text_style(
                status,
                TextStyle {
                    tone: TextTone::Secondary,
                    wrap: false,
                    ..Default::default()
                },
            )?;
            self.set_text_display(
                status,
                TextDisplaySettings {
                    trimming: TextTrimming::CharacterEllipsis,
                    max_lines: Some(1),
                },
            )?;
            let parts = FontPicker {
                container,
                fields: row,
                family,
                face,
                size,
                preview,
                status,
            };
            self.font_pickers.entries.insert(
                container,
                Entry {
                    parts: parts.clone(),
                    value: value.clone(),
                    settings,
                    faces,
                    resolution,
                    validator: None,
                    error: None,
                },
            );
            self.present_font_value(container, &value)?;
            self.set_font_read_only(
                container,
                self.font_pickers.entries[&container].settings.read_only,
            )?;
            Ok(parts)
        })();
        if result.is_err() {
            self.font_pickers.entries.remove(&container);
            self.remove(container)?;
        }
        result
    }
    pub fn font_parts(&self, id: WidgetId) -> Option<&FontPicker> {
        self.font_pickers.entries.get(&id).map(|s| &s.parts)
    }
    pub fn font_value(&self, id: WidgetId) -> Option<&FontDescriptor> {
        self.font_pickers.entries.get(&id).map(|s| &s.value)
    }
    pub fn font_resolution(&self, id: WidgetId) -> Option<&FontResolution> {
        self.font_pickers.entries.get(&id).map(|s| &s.resolution)
    }
    pub fn font_picker_settings(&self, id: WidgetId) -> Option<&FontPickerSettings> {
        self.font_pickers.entries.get(&id).map(|s| &s.settings)
    }
    pub fn font_error(&self, id: WidgetId) -> Option<&str> {
        self.font_pickers.entries.get(&id)?.error.as_deref()
    }
    pub fn take_font_changes(&mut self) -> Vec<FontChanged> {
        std::mem::take(&mut self.font_pickers.changes)
    }
    pub fn set_font_validator(
        &mut self,
        id: WidgetId,
        validator: Option<FontValidator>,
    ) -> Result<(), ControlError> {
        let entry = self
            .font_pickers
            .entries
            .get_mut(&id)
            .ok_or_else(|| font_error("Not a FontPicker"))?;
        if let Some(v) = &validator {
            v.check(&entry.value)?;
        }
        entry.validator = validator;
        Ok(())
    }
    pub(crate) fn chain_font_validator(
        &mut self,
        id: WidgetId,
        validator: FontValidator,
    ) -> Result<(), ControlError> {
        let entry = self
            .font_pickers
            .entries
            .get_mut(&id)
            .ok_or_else(|| font_error("Not a FontPicker"))?;
        let previous = entry.validator.take();
        entry.validator = Some(FontValidator::new(move |value| {
            if let Some(p) = &previous {
                p.check(value).map_err(|e| e.to_string())?;
            }
            validator.check(value).map_err(|e| e.to_string())
        }));
        Ok(())
    }
    pub fn set_font_read_only(&mut self, id: WidgetId, value: bool) -> Result<(), ControlError> {
        self.check_font_parts(id)?;
        let p = self.font_parts(id).unwrap().clone();
        for editor in [&p.family, &p.face, &p.size] {
            self.set_composite_read_only(editor.container, value)?;
        }
        self.font_pickers
            .entries
            .get_mut(&id)
            .unwrap()
            .settings
            .read_only = value;
        Ok(())
    }
    pub fn set_font_value(
        &mut self,
        id: WidgetId,
        value: FontDescriptor,
    ) -> Result<bool, ControlError> {
        self.check_font_parts(id)?;
        if self.font_composition_active(id) {
            return Err(font_error("Finish composition before changing the font"));
        }
        let text = self.font_pickers.entries[&id].settings.preview_text.clone();
        let (value, faces, resolution) = self.prepare_font_value(value, &text)?;
        let entry = &self.font_pickers.entries[&id];
        if let Some(v) = &entry.validator {
            v.check(&value)?;
        }
        let previous = entry.value.clone();
        let previous_faces = entry.faces.clone();
        let previous_resolution = entry.resolution.clone();
        let changed = previous != value;
        {
            let state = self.font_pickers.entries.get_mut(&id).unwrap();
            state.faces = faces;
            state.resolution = resolution;
        }
        if let Err(error) = self.present_font_value(id, &value) {
            let state = self.font_pickers.entries.get_mut(&id).unwrap();
            state.faces = previous_faces;
            state.resolution = previous_resolution;
            self.present_font_value(id, &previous)?;
            return Err(error);
        }
        let entry = self.font_pickers.entries.get_mut(&id).unwrap();
        entry.value = value.clone();
        entry.error = None;
        if changed {
            self.font_pickers.changes.push(FontChanged { id, value });
        }
        Ok(changed)
    }
    fn prepare_font_value(
        &mut self,
        mut value: FontDescriptor,
        text: &str,
    ) -> Result<(FontDescriptor, Vec<FontFaceInfo>, FontResolution), ControlError> {
        if !value.is_valid() || value.size < 1.0 || text.is_empty() || text.len() > 4096 {
            return Err(font_error(
                "Invalid font descriptor or preview text (1..4096 bytes)",
            ));
        }
        value.family = value.family.trim().into();
        if let Some(name) = self
            .font_families()?
            .iter()
            .find(|f| f.to_lowercase() == value.family.to_lowercase())
        {
            value.family = name.clone();
        }
        self.measure_text(&value.family, 4096.0, false)?;
        let faces = self.font_faces(&value.family)?;
        for face in &faces {
            self.measure_text(&face.label(), 4096.0, false)?;
        }
        let run = TextRun::new(
            text,
            Rect::new(0.0, 0.0, 4096.0, 1024.0),
            Color::rgb(0, 0, 0),
            value.size,
        );
        let span = rust_desktop_ui_core::TextSpan {
            font: Some(value.clone()),
            range: 0..text.len(),
            font_size: None,
            weight: None,
            color: None,
        };
        let prepared = self
            .engine
            .prepare_styled(&run, &[span], 1.0)
            .map_err(text_error)?;
        let mut actual_fonts = Vec::new();
        for run in prepared.font_runs() {
            if !actual_fonts.contains(&run.font_name) {
                actual_fonts.push(run.font_name);
            }
        }
        let resolution = FontResolution {
            family_available: !faces.is_empty(),
            actual_fonts,
        };
        Ok((value, faces, resolution))
    }
    fn font_composition_active(&self, id: WidgetId) -> bool {
        self.font_parts(id).is_some_and(|p| {
            [p.family.editor, p.face.editor, p.size.editor]
                .iter()
                .any(|id| {
                    self.document(*id)
                        .is_some_and(|d| d.composition().is_some())
                })
        })
    }
    fn check_font_parts(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .font_pickers
            .entries
            .get(&id)
            .ok_or_else(|| font_error("Not a FontPicker"))?;
        if [&e.parts.family, &e.parts.face, &e.parts.size]
            .iter()
            .any(|p| {
                !self.runtime.tree().contains(p.container)
                    || !self.runtime.tree().contains(p.editor)
                    || p.buttons
                        .iter()
                        .any(|id| !self.runtime.tree().contains(*id))
                    || self.editor_validator(p.editor).is_some()
                    || self.has_async_editor_validator(p.editor)
                    || self
                        .editor_settings(p.editor)
                        .is_none_or(|s| s.max_length.is_some() || !s.commit_on_focus_lost)
                    || self
                        .editor_validation(p.editor)
                        .is_none_or(|v| v.is_required)
                    || self.rich.selection_validators.contains_key(&p.container)
            })
            || self
                .spin_settings(e.parts.size.container)
                .is_none_or(|s| s.minimum != 1.0 || s.maximum != 512.0 || s.step != 1.0)
            || self
                .combo_settings(e.parts.family.container)
                .is_none_or(|s| s.editable)
            || self
                .combo_settings(e.parts.face.container)
                .is_none_or(|s| s.editable)
            || !self.runtime.tree().contains(e.parts.preview)
            || !self.runtime.tree().contains(e.parts.status)
        {
            return Err(font_error(
                "FontPicker required parts must remain live and owned; attach validation to FontPicker",
            ));
        }
        Ok(())
    }
    fn present_font_value(
        &mut self,
        id: WidgetId,
        value: &FontDescriptor,
    ) -> Result<(), ControlError> {
        let e = &self.font_pickers.entries[&id];
        let p = e.parts.clone();
        let faces = e.faces.clone();
        let text = e.settings.preview_text.clone();
        let resolution = e.resolution.clone();
        let installed = self
            .selection_items(p.family.container)
            .unwrap()
            .iter()
            .any(|i| i.key == value.family);
        self.set_selected_keys(
            p.family.container,
            &if installed {
                vec![value.family.clone()]
            } else {
                vec![]
            },
        )?;
        // The requested missing family is displayed without adding it to the catalog.
        self.set_placeholder(p.family.editor, if installed { "" } else { &value.family })?;
        self.set_selection_items(p.face.container, face_items(&faces))?;
        let face = faces.iter().position(|f| {
            f.weight == value.weight && f.width == value.width && f.slant == value.slant
        });
        self.set_selected_keys(
            p.face.container,
            &face.map(|i| vec![i.to_string()]).unwrap_or_default(),
        )?;
        self.set_placeholder(
            p.face.editor,
            if face.is_some() {
                String::new()
            } else {
                FontFaceInfo {
                    weight: value.weight,
                    width: value.width,
                    slant: value.slant,
                }
                .label()
            },
        )?;
        if self.spin_value(p.size.container).map(|s| s as f32) != Some(value.size) {
            accepted(self.set_editor_value(p.size.editor, &value.size.to_string())?)?;
        }
        self.set_text_inlines(
            p.preview,
            vec![InlineSpan {
                font: Some(value.clone()),
                text,
                ..Default::default()
            }],
        )?;
        let status = if resolution.family_available {
            format!("{} · {} px", value.family, value.size)
        } else {
            format!(
                "{} · fallback: {}",
                value.family,
                resolution.actual_fonts.join(", ")
            )
        };
        self.set_label(p.status, status)?;
        Ok(())
    }
    pub(super) fn prune_font_pickers(&mut self) {
        self.font_pickers
            .entries
            .retain(|id, _| self.runtime.tree().contains(*id));
        self.font_pickers
            .changes
            .retain(|e| self.runtime.tree().contains(e.id));
    }
    pub(super) fn sync_font_pickers(&mut self) -> Result<(), ControlError> {
        self.prune_font_pickers();
        let owners: Vec<_> = self.font_pickers.entries.keys().copied().collect();
        for id in owners {
            self.check_font_parts(id)?;
            if self.font_composition_active(id) {
                continue;
            }
            let e = &self.font_pickers.entries[&id];
            let p = e.parts.clone();
            let previous = e.value.clone();
            let read_only = e.settings.read_only;
            let mut value = previous.clone();
            if let Some(family) = self
                .selected_keys(p.family.container)
                .and_then(|keys| keys.first())
            {
                value.family = family.clone();
            }
            if value.family != previous.family {
                let faces = self.font_faces(&value.family)?;
                if let Some(face) = faces
                    .iter()
                    .find(|f| f.weight == value.weight && f.slant == value.slant)
                    .or(faces.first())
                {
                    value = face.descriptor(&value.family, value.size);
                }
            } else if let Some(face) = self
                .selected_keys(p.face.container)
                .and_then(|keys| keys.first())
                .and_then(|key| key.parse::<usize>().ok())
                .and_then(|i| self.font_pickers.entries[&id].faces.get(i))
            {
                value = face.descriptor(&value.family, value.size);
            }
            if let Some(size) = self.spin_value(p.size.container) {
                value.size = size as f32;
            }
            if value != previous {
                if read_only {
                    self.present_font_value(id, &previous)?;
                    continue;
                }
                if let Err(error) = self.set_font_value(id, value) {
                    self.present_font_value(id, &previous)?;
                    self.font_pickers.entries.get_mut(&id).unwrap().error = Some(error.to_string());
                    self.set_label(p.status, error.to_string())?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn preflight_font_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        for e in self.font_pickers.entries.values() {
            let p = &e.parts;
            if [
                p.fields,
                p.family.container,
                p.face.container,
                p.size.container,
                p.preview,
                p.status,
            ]
            .contains(&id)
                || p.family.buttons.contains(&id)
                || p.face.buttons.contains(&id)
                || p.size.buttons.contains(&id)
            {
                return Err(font_error(
                    "Remove the FontPicker owner, not its required parts",
                ));
            }
        }
        Ok(())
    }
}
fn column(index: u16) -> LayoutStyle {
    LayoutStyle {
        grid_column: rust_desktop_ui_core::GridPlacement {
            start: Some(index),
            span: 1,
        },
        ..Default::default()
    }
}
fn face_items(faces: &[FontFaceInfo]) -> Vec<SelectionItem> {
    faces
        .iter()
        .enumerate()
        .map(|(i, f)| SelectionItem::new(i.to_string(), f.label()))
        .collect()
}
fn font_error(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
fn text_error(error: TextError) -> ControlError {
    ControlError::Text(error.to_string())
}

fn accepted(outcome: CommitOutcome) -> Result<(), ControlError> {
    match outcome {
        CommitOutcome::Committed | CommitOutcome::Unchanged => Ok(()),
        CommitOutcome::Invalid(message) => Err(font_error(message)),
        CommitOutcome::CompositionActive => {
            Err(font_error("Finish composition before changing the font"))
        }
        CommitOutcome::Pending => Err(font_error(
            "FontPicker parts do not support asynchronous validators",
        )),
    }
}
