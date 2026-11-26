use super::*;
use rust_desktop_ui_core::Bitmap;
use rust_desktop_ui_images::{
    DecodeOptions, ImageFormat, ImageSource, decode as decode_image, detect_format,
    encode_bitmap_png,
};

pub(super) fn decode(bytes: &[u8]) -> Result<Option<Bitmap>, ControlError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let format = detect_format(bytes).map_err(|e| invalid(e.to_string()))?;
    decode_image(bytes, format, DecodeOptions::default())
        .map(|asset| Some(asset.bitmap().clone()))
        .map_err(|e| invalid(e.to_string()))
}

struct ImageField {
    owner: WidgetId,
    thumbnail: WidgetId,
    button: WidgetId,
    flyout: Flyout,
    parts: ImageEditorParts,
    finish: WidgetId,
    clear: WidgetId,
    resources: Option<(WidgetId, WidgetId)>,
    nullable: bool,
    /// Image identity is immutable. Preserve input bytes, profiles and encoding until pixels are edited.
    encoded: RefCell<(Option<u64>, CellValue)>,
    was_open: bool,
}

pub(super) fn mount(
    ui: &mut Controls,
    host: WidgetId,
    d: &PropertyDescriptor,
    f: &FieldSchema,
    value: &CellValue,
) -> Result<Box<dyn InspectorEditor>, ControlError> {
    let name = d.metadata.label.as_deref().unwrap_or(&f.title);
    let owner = ui.transparent_panel(
        host,
        LayoutStyle {
            kind: LayoutKind::Flex(Axis::Horizontal),
            gap: 6.,
            ..row_style()
        },
    )?;
    let blank = ImageSource::from_bytes(
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#.to_vec(),
        ImageFormat::Svg,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let thumbnail = ui.image(
        owner,
        blank,
        Default::default(),
        LayoutStyle {
            width: Length::Px(38.),
            height: Length::Px(26.),
            ..Default::default()
        },
        name,
    )?;
    let button = ui.button(
        owner,
        "Изображение",
        None,
        LayoutStyle {
            flex_grow: 1.,
            ..row_style()
        },
    )?;
    ui.set_automation_name(button, format!("{name}: открыть редактор изображения"))?;
    ui.set_enabled(button, !d.metadata.read_only)?;
    let flyout = ui.flyout(button, Size::new(640., 420.))?;
    ui.set_button_flyout(button, flyout)?;
    ui.edit_props(flyout.content, |p| {
        p.style.kind = LayoutKind::Flex(Axis::Vertical)
    })?;
    let parts = ui.image_editor(
        flyout.content,
        None,
        ImageEditorSettings {
            read_only: d.metadata.read_only,
            ..Default::default()
        },
        LayoutStyle {
            height: Length::Px(0.),
            flex_grow: 1.,
            ..Default::default()
        },
    )?;
    let footer = ui.transparent_panel(
        flyout.content,
        LayoutStyle {
            kind: LayoutKind::Flex(Axis::Horizontal),
            height: Length::Px(30.),
            width: Length::Percent(1.),
            flex_shrink: 0.,
            gap: 6.,
            ..Default::default()
        },
    )?;
    let resources = if d.choices.is_empty() {
        None
    } else {
        let items = d
            .choices
            .iter()
            .map(|key| SelectionItem::new(key, key))
            .collect();
        let select = ui.combo_box(
            footer,
            "Изображения приложения",
            items,
            ComboBoxSettings {
                editable: false,
                read_only: d.metadata.read_only,
            },
            LayoutStyle {
                flex_grow: 1.,
                ..row_style()
            },
        )?;
        ui.set_selected_keys(select.container, &d.choices[..1])?;
        let replace = action_button(ui, footer, "Заменить")?;
        Some((select.container, replace))
    };
    let clear = action_button(ui, footer, "Очистить")?;
    let finish = action_button(ui, footer, "Готово")?;
    let field = ImageField {
        owner,
        thumbnail,
        button,
        flyout,
        parts,
        finish,
        clear,
        resources,
        nullable: f.nullable,
        encoded: RefCell::new((None, CellValue::Null)),
        was_open: false,
    };
    field.load(ui, value)?;
    Ok(Box::new(field))
}

impl ImageField {
    fn thumbnail(
        &self,
        ui: &mut Controls,
        bitmap: Option<&Bitmap>,
        value: &CellValue,
    ) -> Result<(), ControlError> {
        if let (Some(image), CellValue::Binary(bytes)) = (bitmap, value) {
            let format = detect_format(bytes).map_err(|e| invalid(e.to_string()))?;
            let source = ImageSource::from_bytes(bytes.clone(), format)
                .map_err(|e| invalid(e.to_string()))?;
            ui.set_image_source(self.thumbnail, source)?;
            ui.set_label(
                self.button,
                format!("{} × {}", image.width(), image.height()),
            )?;
        } else {
            ui.set_label(self.button, "Выбрать изображение")?;
        }
        show(ui, self.thumbnail, bitmap.is_some())
    }
    fn value(&self, bitmap: Option<&Bitmap>) -> Result<CellValue, String> {
        let cache = self.encoded.borrow();
        if bitmap.map(Bitmap::id) == cache.0 {
            return Ok(cache.1.clone());
        }
        match bitmap {
            Some(image) => encode_bitmap_png(image)
                .map(CellValue::Binary)
                .map_err(|e| e.to_string()),
            None if self.nullable => Ok(CellValue::Null),
            None => Ok(CellValue::Binary(Vec::new())),
        }
    }
}

impl InspectorEditor for ImageField {
    fn owner(&self) -> WidgetId {
        self.owner
    }
    fn focus(&self) -> WidgetId {
        self.button
    }
    fn inline(&self) -> bool {
        true
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        if ui.flyout_is_open(self.flyout) {
            ui.close_flyout(self.flyout)?;
        }
        let bitmap = match v {
            CellValue::Null => None,
            CellValue::Binary(bytes) => decode(bytes)?,
            _ => return Err(invalid("Image requires encoded image bytes")),
        };
        ui.set_image_editor_value(self.parts.container, bitmap.clone(), true)?;
        self.thumbnail(ui, bitmap.as_ref(), v)?;
        self.encoded
            .replace((bitmap.as_ref().map(Bitmap::id), v.clone()));
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        if ui.flyout_is_open(self.flyout) {
            return Err("Завершите редактирование изображения.".into());
        }
        self.value(
            ui.image_editor_value(self.parts.container)
                .ok_or("Image editor missing")?,
        )
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        if !active && ui.flyout_is_open(self.flyout) {
            ui.close_flyout(self.flyout)?;
        }
        Ok(())
    }
    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let open = ui.flyout_is_open(self.flyout);
        if !open && self.was_open {
            ui.cancel_image_editor(self.parts.container)?;
        }
        self.was_open = open;
        if open {
            let mut actions = Vec::new();
            ui.activated.retain(|id| {
                if *id == self.finish
                    || *id == self.clear
                    || self.resources.is_some_and(|(_, r)| r == *id)
                {
                    actions.push(*id);
                    false
                } else {
                    true
                }
            });
            for action in actions {
                if action == self.clear {
                    ui.set_image_editor_draft(self.parts.container, None)?;
                } else if action == self.finish {
                    ui.apply_image_editor(self.parts.container)?;
                    if ui.image_editor_error(self.parts.container).is_none() {
                        ui.close_flyout(self.flyout)?;
                    }
                } else if let Some((select, _)) = self.resources
                    && let Some(key) = ui.selected_keys(select).and_then(|v| v.first()).cloned()
                {
                    match ui.image_resource_asset(&key) {
                        Ok(asset) => ui.set_image_editor_draft(
                            self.parts.container,
                            Some(asset.bitmap().clone()),
                        )?,
                        Err(e) => {
                            ui.image_editor_report_error(self.parts.container, &e.to_string())?
                        }
                    }
                }
            }
        }
        let bitmap = ui
            .image_editor_value(self.parts.container)
            .ok_or_else(|| invalid("Image editor missing"))?
            .cloned();
        let id = bitmap.as_ref().map(Bitmap::id);
        if id != self.encoded.borrow().0 {
            let value = self.value(bitmap.as_ref()).map_err(invalid)?;
            self.thumbnail(ui, bitmap.as_ref(), &value)?;
            self.encoded.replace((id, value));
        }
        Ok(())
    }
}
