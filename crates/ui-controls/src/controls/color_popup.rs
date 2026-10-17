//! Retained palette/channel/gradient editor with a separately accepted owner value.
use super::color_model::{compact_number, error};
use super::*;
use rust_desktop_ui_core::{Bitmap, GridTrack, ImagePrimitive, Modifiers};

#[derive(Clone, Debug)]
pub struct ColorPopup {
    pub flyout: Flyout,
    pub scroll: ScrollViewer,
    pub(super) revealed_focus: Option<WidgetId>,
    pub(super) viewport_size: Size,
    pub preview: WidgetId,
    pub hex: WidgetId,
    pub channels: [EditorControl; 4],
    pub palette: Vec<WidgetId>,
    pub recent: Vec<WidgetId>,
    pub stops: ListBox,
    pub angle: WidgetId,
    pub offset: WidgetId,
    pub solid: WidgetId,
    pub linear: WidgetId,
    pub add_stop: WidgetId,
    pub remove_stop: WidgetId,
    pub empty: WidgetId,
    pub apply: WidgetId,
    pub cancel: WidgetId,
    pub error: WidgetId,
    pub(super) gradient: WidgetId,
    pub(super) required: Vec<WidgetId>,
    pub(super) parents: Vec<(WidgetId, WidgetId)>,
}
impl Controls {
    pub(super) fn check_color_parts(&self, id: WidgetId) -> Result<(), ControlError> {
        let entry = self
            .color_editors
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a color/brush editor"))?;
        for part in [
            entry.parts.container,
            entry.parts.editor,
            entry.parts.button,
            entry.parts.preview,
        ] {
            if !self.runtime.tree().contains(part) {
                return Err(error("Color editor required part is missing"));
            }
        }
        if [entry.parts.editor, entry.parts.button, entry.parts.preview]
            .iter()
            .any(|part| self.runtime.tree().parent(*part) != Some(entry.parts.container))
        {
            return Err(error("Color inline parts must retain their owned parents"));
        }
        self.check_color_text_recipe(entry.parts.editor, 128, None)?;
        if let Some(popup) = &entry.popup {
            if popup
                .required
                .iter()
                .any(|p| !self.runtime.tree().contains(*p))
            {
                return Err(error("Color popup required part is missing"));
            }
            if popup
                .parents
                .iter()
                .any(|(part, parent)| self.runtime.tree().parent(*part) != Some(*parent))
            {
                return Err(error("Color popup parts must retain their owned parents"));
            }
            if self.selection_settings(popup.stops.container) != Some(&SelectionSettings::default())
                || self
                    .rich
                    .selection_validators
                    .contains_key(&popup.stops.container)
                || self.list_item_template(popup.stops.container).is_some()
                || self.list_item_presentation(popup.stops.container)
                    != Some(ListItemPresentation::Plain)
            {
                return Err(error("Color stop ListBox recipe was replaced"));
            }
            self.check_color_text_recipe(popup.hex, 9, Some(false))?;
            for editor in [popup.offset, popup.angle] {
                self.check_color_text_recipe(editor, 64, Some(false))?;
            }
            let expected = NumericEditSettings {
                minimum: DecimalValue::new(0, 0).unwrap(),
                maximum: DecimalValue::new(255, 0).unwrap(),
                precision: 0,
                allow_empty: false,
                calculator: false,
                ..Default::default()
            };
            for channel in &popup.channels {
                self.check_color_text_recipe(channel.editor, 3, Some(false))?;
                if self.numeric_settings(channel.container) != Some(&expected)
                    || self.composite_read_only(channel.container) != Some(false)
                {
                    return Err(error("Color channel NumericEdit recipe was replaced"));
                }
            }
        }
        Ok(())
    }
    fn check_color_text_recipe(
        &self,
        id: WidgetId,
        max: usize,
        commit: Option<bool>,
    ) -> Result<(), ControlError> {
        let c = self
            .controls
            .get(&id)
            .ok_or_else(|| error("Color field is missing"))?;
        if self.document(id).is_none()
            || !c.single_line
            || c.editor.max_length != Some(max)
            || commit.is_some_and(|expected| {
                c.editor.commit_on_focus_lost != expected || c.editor.is_read_only
            })
            || c.validation != EditorValidation::default()
            || c.validator.is_some()
            || self.has_async_editor_validator(id)
            || self.editor_mask_settings(id).is_some()
        {
            return Err(error(
                "Color-owned editor recipe was replaced; configure the color owner instead",
            ));
        }
        Ok(())
    }
    fn write_color_field(&mut self, id: WidgetId, text: &str) -> Result<(), ControlError> {
        match self.set_editor_value(id, text)? {
            CommitOutcome::Committed | CommitOutcome::Unchanged => Ok(()),
            CommitOutcome::Invalid(message) => Err(error(message)),
            _ => Err(error("Color field rejected a controlled assignment")),
        }
    }
    fn refresh_color_stops(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let entry = &self.color_editors.entries[&id];
        let list = entry.popup.as_ref().unwrap().stops.container;
        let items = entry
            .draft
            .stops
            .iter()
            .map(|s| {
                SelectionItem::new(
                    s.id.0.to_string(),
                    format!("{} · {}", compact_number(s.offset), s.color),
                )
            })
            .collect();
        let keys = entry
            .draft
            .selected
            .map(|s| vec![s.0.to_string()])
            .unwrap_or_default();
        self.set_selection_items(list, items)?;
        self.set_selected_keys(list, &keys)?;
        self.color_editors
            .entries
            .get_mut(&id)
            .unwrap()
            .observed_stop = keys.first().cloned();
        Ok(())
    }
    pub fn color_popup(&self, id: WidgetId) -> Option<&ColorPopup> {
        self.color_editors.entries.get(&id)?.popup.as_ref()
    }
    pub fn color_popup_is_open(&self, id: WidgetId) -> bool {
        self.color_popup(id)
            .is_some_and(|p| self.flyout_is_open(p.flyout))
    }
    pub fn brush_draft(&self, id: WidgetId) -> Option<Option<&BrushValue>> {
        self.color_editors
            .entries
            .get(&id)
            .filter(|e| e.pending)
            .map(|e| e.draft.value.as_ref())
    }
    pub fn gradient_stops(&self, id: WidgetId) -> Option<&[PendingGradientStop]> {
        self.color_editors
            .entries
            .get(&id)
            .filter(|e| e.pending)
            .map(|e| e.draft.stops.as_slice())
    }
    pub fn selected_gradient_stop(&self, id: WidgetId) -> Option<GradientStopId> {
        self.color_editors
            .entries
            .get(&id)
            .filter(|e| e.pending)?
            .draft
            .selected
    }
    pub fn open_color_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_color_parts(id)?;
        if !self.color_editable(id) {
            return Err(error("Color editor is not available for editing"));
        }
        if self.color_popup_is_open(id) {
            return Ok(());
        }
        if !self.window_focused
            || self.viewport.width <= 0.0
            || self.viewport.height <= 0.0
            || self
                .runtime
                .bounds(id)
                .zip(self.runtime.clip(id))
                .and_then(|(b, c)| b.intersection(c))
                .is_none()
        {
            return Err(error(
                "Color popup owner must be visible, active and laid out",
            ));
        }
        let entry = &self.color_editors.entries[&id];
        let value = if entry.brush {
            entry.value.clone()
        } else {
            let document = self
                .document(entry.parts.editor)
                .ok_or_else(|| error("Missing color editor"))?;
            if document.composition().is_some() {
                return Err(error("Finish composition before opening the color popup"));
            }
            super::color_edit::parse_inline(document.text()).map_err(error)?
        };
        entry.settings.validate_value(value.as_ref())?;
        let image = bitmap(value.as_ref())?;
        let mut draft = entry.draft.clone();
        draft.replace(value)?;
        self.ensure_color_popup(id)?;
        let popup = self.color_popup(id).unwrap().clone();
        self.preflight_open_flyout(popup.flyout)?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        entry.error = None;
        entry.pending = true;
        entry.popup.as_mut().unwrap().revealed_focus = None;
        self.refresh_color_popup(id)?;
        self.open_flyout(popup.flyout)?;
        self.runtime.apply(Mutation::Focus(Some(popup.hex)))?;
        self.synchronize_editor_focus()?;
        Ok(())
    }
    pub fn cancel_color_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let Some(entry) = self.color_editors.entries.get_mut(&id) else {
            return Err(error("Not a color/brush editor"));
        };
        entry.pending = false;
        entry.draft.reset(entry.value.clone());
        entry.preview_bitmap = None;
        entry.error = None;
        let popup = entry.popup.clone();
        if let Some(popup) = popup
            && self.flyout_is_open(popup.flyout)
        {
            self.close_flyout(popup.flyout)?;
        }
        self.scene_dirty = true;
        self.refresh_color_source_status();
        Ok(())
    }
    pub fn apply_color_popup(&mut self, id: WidgetId) -> Result<CommitOutcome, ControlError> {
        if !self.color_editable(id) || !self.color_popup_is_open(id) {
            self.cancel_color_popup(id)?;
            return Ok(CommitOutcome::Unchanged);
        }
        if !self
            .color_editors
            .entries
            .get(&id)
            .is_some_and(|e| e.pending)
        {
            return Err(error("Color popup is not open"));
        }
        let popup = self.color_popup(id).unwrap().clone();
        if self
            .color_field_ids(&popup)
            .iter()
            .any(|&id| self.document(id).is_some_and(|d| d.composition().is_some()))
        {
            return Ok(CommitOutcome::CompositionActive);
        }
        let candidate = self.read_color_popup_fields(id).and_then(|()| {
            let value = self.color_editors.entries[&id].draft.value.clone();
            self.validate_brush_candidate(id, value.as_ref(), true)?;
            Ok(value)
        });
        let value = match candidate {
            Ok(v) => v,
            Err(err) => {
                let message = self.safe_color_error(&err.to_string());
                self.color_popup_error(id, Some(message.clone()))?;
                return Ok(CommitOutcome::Invalid(message));
            }
        };
        let changed = self.color_editors.entries[&id].value != value;
        let image = bitmap(value.as_ref())?;
        let text = super::color_edit::display(value.as_ref());
        self.preflight_editor_text(self.color_editors.entries[&id].parts.editor, &text)?;
        self.write_color_inline(id, &text)?;
        self.publish_brush(id, value, image, changed);
        self.cancel_color_popup(id)?;
        Ok(if changed {
            CommitOutcome::Committed
        } else {
            CommitOutcome::Unchanged
        })
    }
    /// Change pending presentation only. Accepted value, history and bindings are untouched.
    pub fn set_brush_draft(
        &mut self,
        id: WidgetId,
        value: Option<BrushValue>,
    ) -> Result<(), ControlError> {
        self.require_color_pending(id)?;
        let entry = &self.color_editors.entries[&id];
        entry.settings.validate_value(value.as_ref())?;
        if !entry.brush && matches!(value, Some(BrushValue::Linear(_))) {
            return Err(error("ColorEdit accepts only solid colors"));
        }
        let image = bitmap(value.as_ref())?;
        let mut draft = self.color_editors.entries[&id].draft.clone();
        draft.replace(value)?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        entry.error = None;
        self.refresh_color_popup(id)
    }
    pub fn set_color_draft(&mut self, id: WidgetId, color: Srgba8) -> Result<(), ControlError> {
        self.require_color_pending(id)?;
        let mut draft = self.color_editors.entries[&id].draft.clone();
        draft.set_color(color);
        let image = bitmap(draft.value.as_ref())?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        entry.error = None;
        self.refresh_color_popup(id)
    }
    pub fn select_gradient_stop(
        &mut self,
        id: WidgetId,
        stop: GradientStopId,
    ) -> Result<(), ControlError> {
        self.require_color_pending(id)?;
        if !self.color_editors.entries[&id]
            .draft
            .stops
            .iter()
            .any(|s| s.id == stop)
        {
            return Err(error("Unknown gradient stop"));
        }
        self.read_color_popup_fields(id)?;
        self.color_editors
            .entries
            .get_mut(&id)
            .unwrap()
            .draft
            .select(stop)?;
        self.refresh_color_popup(id)
    }
    pub fn set_gradient_stop(
        &mut self,
        id: WidgetId,
        stop: GradientStopId,
        offset: f64,
        color: Srgba8,
    ) -> Result<(), ControlError> {
        self.require_color_pending(id)?;
        let mut draft = self.color_editors.entries[&id].draft.clone();
        draft.update_stop(stop, offset, color)?;
        let image = bitmap(draft.value.as_ref())?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        self.refresh_color_popup(id)
    }
    pub fn add_gradient_stop(
        &mut self,
        id: WidgetId,
        offset: f64,
        color: Srgba8,
    ) -> Result<GradientStopId, ControlError> {
        self.require_color_pending(id)?;
        let mut draft = self.color_editors.entries[&id].draft.clone();
        let stop = draft.add(offset, color)?;
        let image = bitmap(draft.value.as_ref())?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        self.refresh_color_popup(id)?;
        Ok(stop)
    }
    pub fn remove_gradient_stop(
        &mut self,
        id: WidgetId,
        stop: GradientStopId,
    ) -> Result<(), ControlError> {
        self.require_color_pending(id)?;
        let mut draft = self.color_editors.entries[&id].draft.clone();
        draft.remove(stop)?;
        let image = bitmap(draft.value.as_ref())?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        self.refresh_color_popup(id)
    }
    pub fn set_gradient_angle(&mut self, id: WidgetId, angle: f64) -> Result<(), ControlError> {
        self.require_color_pending(id)?;
        let mut draft = self.color_editors.entries[&id].draft.clone();
        draft.angle(angle)?;
        let image = bitmap(draft.value.as_ref())?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        self.refresh_color_popup(id)
    }
    fn require_color_pending(&self, id: WidgetId) -> Result<(), ControlError> {
        self.check_color_parts(id)?;
        if self.color_editable(id)
            && self.color_popup_is_open(id)
            && self
                .color_editors
                .entries
                .get(&id)
                .is_some_and(|e| e.pending)
        {
            Ok(())
        } else {
            Err(error("A writable open color popup is required"))
        }
    }
    fn ensure_color_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.color_popup(id).is_some() {
            return Ok(());
        }
        let settings = self.color_editors.entries[&id].settings.clone();
        let flyout = self.flyout(id, settings.popup_size)?;
        let result = (|| {
            self.edit_props(flyout.content, |p| {
                p.style.kind = LayoutKind::Flex(Axis::Vertical);
                p.style.gap = 8.0;
            })?;
            let scroll = self.scroll_view(
                flyout.content,
                LayoutStyle {
                    height: Length::Px(0.0),
                    flex_grow: 1.0,
                    width: Length::Percent(1.0),
                    ..Default::default()
                },
                0.0,
            )?;
            self.edit_props(scroll.content, |p| {
                p.clip = false;
                p.style.kind = LayoutKind::Flex(Axis::Vertical);
                p.style.gap = 8.0;
                // The shared ScrollViewer paints a 10px overlay scrollbar.
                // Reserve its strip in this form so channel buttons and swatches
                // remain wholly visible and clickable while the body scrolls.
                p.style.padding = Edges {
                    right: 16.0,
                    ..Edges::all(4.0)
                };
                p.style.height = Length::Auto;
            })?;
            let content = scroll.content;
            let preview = self.transparent_panel(content, row(72.0))?;
            self.set_automation_name(preview, "Предпросмотр цвета или кисти")?;
            let hex = self.edit_box(content, "Цвет: #AARRGGBB", "#FF000000", row(36.0))?;
            self.set_editor_settings(
                hex,
                EditorSettings {
                    max_length: Some(9),
                    commit_on_focus_lost: false,
                    ..Default::default()
                },
            )?;
            let channel_row = self.transparent_panel(
                content,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0); 4],
                    gap: 8.0,
                    ..row(62.0)
                },
            )?;
            let mut channels = Vec::new();
            for name in ["R · Красный", "G · Зелёный", "B · Синий", "A · Альфа"]
            {
                let host = self.transparent_panel(
                    channel_row,
                    LayoutStyle {
                        kind: LayoutKind::Flex(Axis::Vertical),
                        gap: 2.0,
                        ..Default::default()
                    },
                )?;
                self.label(host, name, row(20.0))?;
                let editor = self.numeric_edit(
                    host,
                    name,
                    Some(DecimalValue::new(0, 0).unwrap()),
                    NumericEditSettings {
                        minimum: DecimalValue::new(0, 0).unwrap(),
                        maximum: DecimalValue::new(255, 0).unwrap(),
                        precision: 0,
                        allow_empty: false,
                        calculator: false,
                        ..Default::default()
                    },
                    row(36.0),
                )?;
                self.set_editor_settings(
                    editor.editor,
                    EditorSettings {
                        commit_on_focus_lost: false,
                        max_length: Some(3),
                        ..Default::default()
                    },
                )?;
                channels.push(editor);
            }
            self.label(content, "Палитра", row(22.0))?;
            let palette_host =
                self.transparent_panel(content, swatch_grid(settings.palette.len()))?;
            let mut palette = Vec::new();
            for color in &settings.palette {
                palette.push(self.button(palette_host, color.to_string(), None, row(30.0))?);
            }
            self.label(content, "Последние принятые цвета", row(22.0))?;
            let recent_host =
                self.transparent_panel(content, swatch_grid(settings.recent_limit))?;
            let mut recent = Vec::new();
            for _ in 0..settings.recent_limit {
                recent.push(self.button(recent_host, "", None, row(30.0))?);
            }
            let mode = self.transparent_panel(content, horizontal(36.0))?;
            let solid = self.button(mode, "Сплошной", None, grow())?;
            let linear = self.button(mode, "Линейный", None, grow())?;
            let empty = self.button(mode, "Нет значения", None, grow())?;
            let gradient = self.transparent_panel(
                content,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    gap: 6.0,
                    ..row(260.0)
                },
            )?;
            let stops = self.list_box(
                gradient,
                "Точки градиента",
                Vec::new(),
                SelectionSettings::default(),
                row(110.0),
            )?;
            let coordinates = self.transparent_panel(gradient, horizontal(62.0))?;
            let offset_host = self.transparent_panel(
                coordinates,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    ..grow()
                },
            )?;
            self.label(offset_host, "Положение · 0…1", row(22.0))?;
            let offset =
                self.edit_box(offset_host, "Положение точки · от 0 до 1", "0", row(36.0))?;
            let angle_host = self.transparent_panel(
                coordinates,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    ..grow()
                },
            )?;
            self.label(angle_host, "Угол · 0…360°", row(22.0))?;
            let angle =
                self.edit_box(angle_host, "Угол градиента · от 0 до 360", "0", row(36.0))?;
            for editor in [offset, angle] {
                self.set_editor_settings(
                    editor,
                    EditorSettings {
                        max_length: Some(64),
                        commit_on_focus_lost: false,
                        ..Default::default()
                    },
                )?;
            }
            let stop_actions = self.transparent_panel(gradient, horizontal(36.0))?;
            let add_stop = self.button(stop_actions, "Добавить точку", None, grow())?;
            let remove_stop = self.button(stop_actions, "Удалить точку", None, grow())?;
            let error_label = self.label(flyout.content, "", row(40.0))?;
            let footer = self.transparent_panel(flyout.content, horizontal(36.0))?;
            let apply = self.button(footer, "Применить", None, grow())?;
            let cancel = self.button(footer, "Отмена", None, grow())?;
            let mut required = Vec::new();
            let mut stack = vec![flyout.container];
            while let Some(node) = stack.pop() {
                required.push(node);
                if let Some(children) = self.runtime.tree().children(node) {
                    stack.extend(children.iter().copied());
                }
            }
            let parents = required
                .iter()
                .map(|&part| (part, self.runtime.tree().parent(part).unwrap()))
                .collect();
            Ok(ColorPopup {
                flyout,
                scroll,
                revealed_focus: None,
                viewport_size: Size::ZERO,
                preview,
                hex,
                channels: channels.try_into().unwrap(),
                palette,
                recent,
                stops,
                angle,
                offset,
                solid,
                linear,
                add_stop,
                remove_stop,
                empty,
                apply,
                cancel,
                error: error_label,
                gradient,
                required,
                parents,
            })
        })();
        match result {
            Ok(popup) => {
                self.color_editors.entries.get_mut(&id).unwrap().popup = Some(popup);
                Ok(())
            }
            Err(error) => {
                self.remove(flyout.container)?;
                Err(error)
            }
        }
    }
    fn color_field_ids(&self, popup: &ColorPopup) -> Vec<WidgetId> {
        vec![
            popup.hex,
            popup.channels[0].editor,
            popup.channels[1].editor,
            popup.channels[2].editor,
            popup.channels[3].editor,
            popup.offset,
            popup.angle,
        ]
    }
    pub(super) fn color_popup_fields_dirty(&self, entry: &super::color_edit::Entry) -> bool {
        entry.popup.as_ref().is_some_and(|p| {
            self.color_field_ids(p).iter().any(|&id| {
                self.document(id).is_some_and(|d| {
                    d.text() != self.accepted_value(id).unwrap_or("") || d.composition().is_some()
                })
            })
        })
    }
    fn refresh_color_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let entry = &self.color_editors.entries[&id];
        let popup = entry.popup.as_ref().unwrap().clone();
        let color = entry.draft.color();
        let linear = matches!(entry.draft.value, Some(BrushValue::Linear(_)));
        let angle = match &entry.draft.value {
            Some(BrushValue::Linear(g)) => g.angle(),
            _ => 0.0,
        };
        let selected = entry.draft.selected;
        let offset = entry
            .draft
            .stops
            .iter()
            .find(|s| Some(s.id) == selected)
            .map_or(0.0, |s| s.offset);
        let fields = vec![
            color.to_string(),
            color.r.to_string(),
            color.g.to_string(),
            color.b.to_string(),
            color.a.to_string(),
            compact_number(offset),
            compact_number(angle),
        ];
        let stop_items: Vec<_> = entry
            .draft
            .stops
            .iter()
            .map(|s| {
                SelectionItem::new(
                    s.id.0.to_string(),
                    format!("{} · {}", compact_number(s.offset), s.color),
                )
            })
            .collect();
        let recent = entry.recent.clone();
        let brush = entry.brush;
        let allow_empty = entry.settings.allow_empty;
        for (editor, text) in self.color_field_ids(&popup).into_iter().zip(&fields) {
            self.write_color_field(editor, text)?;
        }
        self.set_selection_items(popup.stops.container, stop_items)?;
        let keys = selected.map(|s| vec![s.0.to_string()]).unwrap_or_default();
        self.set_selected_keys(popup.stops.container, &keys)?;
        self.edit_props(popup.gradient, |p| p.visible = brush && linear)?;
        self.edit_props(popup.solid, |p| p.visible = brush)?;
        self.edit_props(popup.linear, |p| p.visible = brush)?;
        self.edit_props(popup.empty, |p| p.enabled = allow_empty)?;
        let removable = self.color_editors.entries[&id].draft.stops.len() > 2;
        self.edit_props(popup.remove_stop, |p| p.enabled = removable)?;
        for (i, button) in popup.recent.iter().copied().enumerate() {
            self.edit_props(button, |p| p.visible = i < recent.len())?;
            if let Some(color) = recent.get(i) {
                self.set_label(button, color.to_string())?;
            }
        }
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.last_fields = fields;
        entry.observed_stop = keys.first().cloned();
        self.color_popup_error(id, None)?;
        self.refresh_color_source_status();
        self.scene_dirty = true;
        Ok(())
    }
    fn read_color_popup_fields(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_color_parts(id)?;
        let entry = &self.color_editors.entries[&id];
        let popup = entry.popup.as_ref().unwrap().clone();
        let fields: Vec<_> = self
            .color_field_ids(&popup)
            .iter()
            .map(|id| {
                self.document(*id)
                    .map(|d| d.text().to_owned())
                    .ok_or_else(|| error("Color popup field missing"))
            })
            .collect::<Result<_, _>>()?;
        if fields == entry.last_fields {
            return Ok(());
        }
        let mut draft = entry.draft.clone();
        let old = &entry.last_fields;
        if old.first() != fields.first() {
            draft.set_color(
                fields[0]
                    .parse::<Srgba8>()
                    .map_err(|e| error(e.to_string()))?,
            );
        } else if old.get(1..5) != fields.get(1..5) {
            let channel = |i: usize| -> Result<u8, ControlError> {
                if fields[i].is_empty() || !fields[i].bytes().all(|b| b.is_ascii_digit()) {
                    return Err(error("Каналы RGBA должны быть целыми числами 0…255"));
                }
                fields[i]
                    .parse::<u8>()
                    .map_err(|_| error("Каналы RGBA должны быть целыми числами 0…255"))
            };
            draft.set_color(Srgba8::new(
                channel(1)?,
                channel(2)?,
                channel(3)?,
                channel(4)?,
            ));
        }
        if let Some(stop) = draft.selected {
            if old.get(5) != fields.get(5) {
                let offset = number(&fields[5], 0.0, 1.0)?;
                draft.update_stop(stop, offset, draft.color())?;
            }
            if old.get(6) != fields.get(6) {
                draft.angle(number(&fields[6], 0.0, 360.0)?)?;
            }
        }
        let changed = draft.value != entry.draft.value;
        let image = if changed {
            bitmap(draft.value.as_ref())?
        } else {
            entry.preview_bitmap.clone()
        };
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.draft = draft;
        entry.preview_bitmap = image;
        // Preserve the active field and its undo history; synchronize only peers.
        let color = entry.draft.color();
        let canonical = [
            color.to_string(),
            color.r.to_string(),
            color.g.to_string(),
            color.b.to_string(),
            color.a.to_string(),
        ];
        let mut seen = fields.clone();
        for (i, editor) in self.color_field_ids(&popup).into_iter().take(5).enumerate() {
            if self.runtime.focused() != Some(editor) && canonical[i] != fields[i] {
                self.write_color_field(editor, &canonical[i])?;
                seen[i] = canonical[i].clone();
            }
        }
        self.color_editors.entries.get_mut(&id).unwrap().last_fields = seen;
        if changed {
            self.refresh_color_stops(id)?;
        }
        self.color_popup_error(id, None)?;
        self.refresh_color_source_status();
        self.scene_dirty |= changed;
        Ok(())
    }
    pub(super) fn safe_color_error(&mut self, message: &str) -> String {
        let bounded = &message[..message.floor_char_boundary(4096.min(message.len()))];
        let candidate = bounded.replace(['\0', '\r'], " ");
        if self.measure_text(&candidate, 4096.0, true).is_err() {
            "Invalid color or brush value.".into()
        } else {
            candidate
        }
    }
    fn color_popup_error(
        &mut self,
        id: WidgetId,
        message: Option<String>,
    ) -> Result<(), ControlError> {
        let message = message.map(|message| self.safe_color_error(&message));
        let entry = &self.color_editors.entries[&id];
        if entry.error == message
            && entry
                .popup
                .as_ref()
                .is_none_or(|p| self.label_text(p.error) == Some(message.as_deref().unwrap_or("")))
        {
            return Ok(());
        }
        if let Some(p) = entry.popup.clone() {
            self.set_label(p.error, message.clone().unwrap_or_default())?;
        }
        self.color_editors.entries.get_mut(&id).unwrap().error = message;
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn sync_color_popup_layout(&mut self) -> Result<(), ControlError> {
        let owners: Vec<_> = self
            .color_editors
            .entries
            .keys()
            .copied()
            .filter(|&id| self.color_popup_is_open(id))
            .collect();
        for id in owners {
            self.check_color_parts(id)?;
            let popup = self.color_popup(id).unwrap().clone();
            let Some(viewport) = self.runtime.bounds(popup.scroll.viewport) else {
                continue;
            };
            let mut extent = Size::ZERO;
            let mut pending = vec![(popup.scroll.content, Point::ZERO)];
            while let Some((part, origin)) = pending.pop() {
                let Some(node) = self.runtime.tree().node(part) else {
                    continue;
                };
                if node.props.collapsed {
                    continue;
                }
                let Some(local) = self.runtime.layout_bounds(part) else {
                    continue;
                };
                let origin = Point::new(origin.x + local.x, origin.y + local.y);
                extent.width = extent
                    .width
                    .max(origin.x + local.width + node.props.style.margin.right);
                extent.height = extent
                    .height
                    .max(origin.y + local.height + node.props.style.margin.bottom);
                if part == popup.scroll.content || !node.props.clip {
                    pending.extend(
                        self.runtime
                            .tree()
                            .children(part)
                            .unwrap_or_default()
                            .iter()
                            .map(|&child| (child, origin)),
                    );
                }
            }
            self.set_scroll_measured_extent(
                popup.scroll.viewport,
                Size::new(viewport.width, extent.height.max(viewport.height)),
            )?;
            let focus = self.runtime.focused();
            let size = Size::new(viewport.width, viewport.height);
            if (focus != popup.revealed_focus || size != popup.viewport_size)
                && let Some(focus) = focus
                && self.flyout_descendant(focus, popup.scroll.content)
            {
                self.bring_into_view(focus)?;
            }
            let popup = self
                .color_editors
                .entries
                .get_mut(&id)
                .unwrap()
                .popup
                .as_mut()
                .unwrap();
            popup.revealed_focus = focus;
            popup.viewport_size = size;
        }
        Ok(())
    }
    pub(super) fn sync_color_editors(&mut self) -> Result<(), ControlError> {
        self.prune_color_editors()?;
        for id in self
            .color_editors
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            let height = self.theme.tokens().control_height;
            let cols = vec![
                GridTrack::Px(height),
                GridTrack::Fr(1.0),
                GridTrack::Px(height),
            ];
            if self
                .runtime
                .tree()
                .node(id)
                .is_some_and(|n| n.props.style.grid_columns != cols)
            {
                self.edit_props(id, |p| p.style.grid_columns = cols)?;
            }
            if !self.color_editors.entries[&id].pending {
                continue;
            }
            if !self.color_editable(id) || !self.color_popup_is_open(id) {
                self.cancel_color_popup(id)?;
                continue;
            }
            let popup = self.color_popup(id).unwrap().clone();
            if self.color_field_ids(&popup).iter().any(|&field| {
                self.document(field)
                    .is_some_and(|d| d.composition().is_some())
            }) {
                continue;
            }
            if let Err(error) = self.read_color_popup_fields(id) {
                self.color_popup_error(id, Some(error.to_string()))?;
                continue;
            }
            let selected = self
                .selected_keys(popup.stops.container)
                .and_then(|keys| keys.first())
                .cloned();
            if selected != self.color_editors.entries[&id].observed_stop
                && let Some(key) = selected
                && let Ok(key) = key.parse::<u64>()
            {
                self.select_gradient_stop(id, GradientStopId(key))?;
            }
        }
        Ok(())
    }
    pub(super) fn activate_color(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let owner = self.color_editors.entries.iter().find_map(|(&id, e)| {
            (e.parts.button == button
                || e.popup.as_ref().is_some_and(|p| {
                    [
                        p.solid,
                        p.linear,
                        p.empty,
                        p.apply,
                        p.cancel,
                        p.add_stop,
                        p.remove_stop,
                    ]
                    .contains(&button)
                        || p.palette.contains(&button)
                        || p.recent.contains(&button)
                }))
            .then_some(id)
        });
        let Some(id) = owner else {
            return Ok(false);
        };
        if !self.color_editable(id) {
            self.cancel_color_popup(id)?;
            return Ok(true);
        }
        if self.color_editors.entries[&id].parts.button == button {
            let result = if self.color_popup_is_open(id) {
                self.cancel_color_popup(id)
            } else {
                self.open_color_popup(id)
            };
            if let Err(error) = result {
                self.color_editors.entries.get_mut(&id).unwrap().error = Some(error.to_string());
            }
            return Ok(true);
        }
        let popup = self.color_popup(id).unwrap().clone();
        let result = if button == popup.cancel {
            self.cancel_color_popup(id)
        } else if button == popup.apply {
            self.apply_color_popup(id).map(|_| ())
        } else if button == popup.empty {
            self.set_brush_draft(id, None)
        } else if button == popup.solid {
            let color = self.color_editors.entries[&id].draft.color();
            self.set_brush_draft(id, Some(BrushValue::Solid(color)))
        } else if button == popup.linear {
            let color = self.color_editors.entries[&id].draft.color();
            self.set_brush_draft(
                id,
                Some(BrushValue::Linear(
                    LinearGradient::new(
                        0.0,
                        vec![
                            GradientStop::new(0.0, color),
                            GradientStop::new(1.0, Srgba8::WHITE),
                        ],
                    )
                    .unwrap(),
                )),
            )
        } else if button == popup.add_stop {
            let color = self.color_editors.entries[&id].draft.color();
            self.add_gradient_stop(id, 0.5, color).map(|_| ())
        } else if button == popup.remove_stop {
            if let Some(stop) = self.selected_gradient_stop(id) {
                self.remove_gradient_stop(id, stop)
            } else {
                Ok(())
            }
        } else if let Some(index) = popup.palette.iter().position(|b| *b == button) {
            let color = self.color_editors.entries[&id].settings.palette[index];
            self.set_color_draft(id, color)
        } else if let Some(index) = popup.recent.iter().position(|b| *b == button) {
            if let Some(color) = self.color_editors.entries[&id].recent.get(index).copied() {
                self.set_color_draft(id, color)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.color_popup_error(id, Some(error.to_string()))?;
        }
        Ok(true)
    }
    pub(super) fn color_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if matches!(event, InputEvent::WindowFocused(false)) {
            for id in self
                .color_editors
                .entries
                .keys()
                .copied()
                .collect::<Vec<_>>()
            {
                self.cancel_color_popup(id)?;
            }
            return Ok(false);
        }
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let owner = self.color_editors.entries.iter().find_map(|(&id, e)| {
            (self.flyout_descendant(focused, id)
                || e.popup
                    .as_ref()
                    .is_some_and(|p| self.flyout_descendant(focused, p.flyout.container)))
            .then_some(id)
        });
        let Some(id) = owner else {
            return Ok(false);
        };
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        if *key == Key::Escape && *modifiers == Modifiers::default() && self.color_popup_is_open(id)
        {
            if !repeat {
                if self
                    .document(focused)
                    .is_some_and(|d| d.composition().is_some())
                {
                    self.cancel_preedit();
                } else {
                    self.cancel_color_popup(id)?;
                }
            }
            return Ok(true);
        }
        if *key == Key::Enter
            && modifiers.control
            && !modifiers.alt
            && !modifiers.super_key
            && self.color_popup_is_open(id)
        {
            if !repeat {
                self.apply_color_popup(id)?;
            }
            return Ok(true);
        }
        if *key == Key::ArrowDown && modifiers.alt && !modifiers.control && !modifiers.super_key {
            if !repeat && let Err(error) = self.open_color_popup(id) {
                self.color_editors.entries.get_mut(&id).unwrap().error = Some(error.to_string());
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn paint_color_editor(&mut self, id: WidgetId, bounds: Rect, clip: Rect) -> bool {
        let paint = self.color_editors.entries.values().find_map(|e| {
            if e.parts.preview == id {
                return Some((e.bitmap.clone(), None));
            }
            let p = e.popup.as_ref()?;
            if p.preview == id {
                return Some((e.preview_bitmap.clone(), None));
            }
            if let Some(i) = p.palette.iter().position(|b| *b == id) {
                return Some((None, Some(e.settings.palette[i])));
            }
            if let Some(i) = p.recent.iter().position(|b| *b == id) {
                return e.recent.get(i).copied().map(|c| (None, Some(c)));
            }
            None
        });
        let Some((image, color)) = paint else {
            return false;
        };
        let inset = if color.is_some() { 4.0 } else { 1.0 };
        let area = Rect::new(
            bounds.x + inset,
            bounds.y + inset,
            (bounds.width - inset * 2.0).max(0.0),
            (bounds.height - inset * 2.0).max(0.0),
        );
        // At most 16×8 checker cells per preview, independent of window size.
        let columns = 16usize;
        let rows = 8usize;
        for y in 0..rows {
            for x in 0..columns {
                let tile = Rect::new(
                    area.x + area.width * x as f32 / columns as f32,
                    area.y + area.height * y as f32 / rows as f32,
                    area.width / columns as f32 + 0.01,
                    area.height / rows as f32 + 0.01,
                );
                if let Some(tile) = tile.intersection(clip) {
                    self.scene.fill(
                        tile,
                        if (x + y) % 2 == 0 {
                            Color::rgb(230, 230, 230)
                        } else {
                            Color::rgb(170, 170, 170)
                        },
                    );
                }
            }
        }
        if let Some(color) = color {
            if let Some(area) = area.intersection(clip) {
                self.scene.fill(area, color.into());
            }
        } else if let Some(image) = image {
            let mut primitive = ImagePrimitive::new(image, area);
            primitive.clip = clip;
            self.scene.image(primitive);
        } else {
            let token = self.theme.tokens().muted;
            let line = Rect::new(
                area.x + area.width * 0.4,
                area.y + area.height * 0.5,
                area.width * 0.2,
                2.0,
            );
            if let Some(line) = line.intersection(clip) {
                self.scene.fill(line, token);
            }
        }
        let border = if self.runtime.focused() == Some(id) {
            self.theme.tokens().focus
        } else {
            self.theme.tokens().border
        };
        for edge in [
            Rect::new(bounds.x, bounds.y, bounds.width, 1.0),
            Rect::new(bounds.x, bounds.y + bounds.height - 1.0, bounds.width, 1.0),
            Rect::new(bounds.x, bounds.y, 1.0, bounds.height),
            Rect::new(bounds.x + bounds.width - 1.0, bounds.y, 1.0, bounds.height),
        ] {
            if let Some(edge) = edge.intersection(clip) {
                self.scene.fill(edge, border);
            }
        }
        true
    }
}
pub(super) fn bitmap(value: Option<&BrushValue>) -> Result<Option<Bitmap>, ControlError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let (width, height) = (256u32, 64u32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let color = match value {
                BrushValue::Solid(c) => *c,
                BrushValue::Linear(g) => g
                    .sample(
                        g.position(
                            f64::from(x) / f64::from(width - 1),
                            f64::from(y) / f64::from(height - 1),
                        )
                        .unwrap(),
                    )
                    .unwrap(),
            };
            pixels.extend([color.r, color.g, color.b, color.a]);
        }
    }
    Bitmap::new_rgba8(width, height, pixels)
        .map(Some)
        .map_err(|e| error(e.to_string()))
}
fn number(text: &str, min: f64, max: f64) -> Result<f64, ControlError> {
    let number = text
        .replace(',', ".")
        .parse::<f64>()
        .map_err(|_| error("Введите число"))?;
    if !number.is_finite() || !(min..=max).contains(&number) {
        Err(error(format!("Введите число от {min} до {max}")))
    } else {
        Ok(number)
    }
}
fn row(height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn horizontal(height: f32) -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Flex(Axis::Horizontal),
        gap: 6.0,
        ..row(height)
    }
}
fn grow() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(0.0),
        height: Length::Percent(1.0),
        flex_grow: 1.0,
        ..Default::default()
    }
}
fn swatch_grid(count: usize) -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Grid,
        grid_columns: vec![GridTrack::Fr(1.0); 8],
        gap: 4.0,
        ..row((count.div_ceil(8).max(1) * 34) as f32)
    }
}
