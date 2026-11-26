use super::*;
use rust_desktop_ui_data::FieldType;
use std::cell::RefCell;

#[path = "popup_editors/date_picker.rs"]
mod date_picker;
#[path = "popup_editors/image.rs"]
mod image;
#[path = "popup_editors/sparkline.rs"]
mod sparkline;

pub(super) fn supports(kind: &str) -> bool {
    matches!(
        kind,
        "Color" | "Brush" | "Image" | "Calculator" | "Font" | "DatePicker" | "Sparkline"
    )
}

pub(super) fn validate(
    d: &PropertyDescriptor,
    f: &FieldSchema,
    v: &CellValue,
) -> Result<(), ControlError> {
    let name = d.metadata.editor.as_str();
    let expected = match name {
        "Image" => FieldType::Binary,
        "Calculator" => FieldType::Decimal,
        "DatePicker" => FieldType::Date,
        "Brush" | "Color" | "Font" | "Sparkline" => FieldType::Text,
        _ => return Err(invalid("Unknown popup property editor")),
    };
    if f.kind != expected {
        return Err(invalid(format!("{name} requires {expected:?}")));
    }
    if !d.choices.is_empty() && name != "Image" {
        return Err(invalid(
            "Only the image editor accepts named replacement image resources",
        ));
    }
    f.validate(v).map_err(|e| invalid(e.to_string()))?;
    match (name, v) {
        (_, CellValue::Null) => Ok(()),
        ("Sparkline", CellValue::Text(text)) => sparkline::model(text).map(|_| ()),
        ("Color" | "Brush", CellValue::Text(s)) => parse_brush(s, name == "Brush").map(|_| ()),
        ("Font", CellValue::Text(s))
            if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) =>
        {
            Err(invalid("Expected a font family of 1..256 bytes"))
        }
        ("Calculator", CellValue::Decimal(n)) => calculator_settings(d, f)
            .validate_value(Some(*n))
            .map_err(invalid),
        ("Image", CellValue::Binary(bytes)) => image::decode(bytes).map(|_| ()),
        _ => Ok(()),
    }
}

fn row_style() -> LayoutStyle {
    LayoutStyle {
        height: Length::Px(28.),
        ..Default::default()
    }
}

fn action_button(ui: &mut Controls, host: WidgetId, label: &str) -> Result<WidgetId, ControlError> {
    let width = ui.measure_text(label, 1024., false)?.width.ceil() + 24.;
    ui.button(
        host,
        label,
        None,
        LayoutStyle {
            width: Length::Px(width),
            flex_shrink: 0.,
            ..row_style()
        },
    )
}

pub(super) fn mount(
    ui: &mut Controls,
    host: WidgetId,
    d: &PropertyDescriptor,
    f: &FieldSchema,
    value: &CellValue,
) -> Result<Box<dyn InspectorEditor>, ControlError> {
    let name = d.metadata.label.as_deref().unwrap_or(&f.title);
    match d.metadata.editor.as_str() {
        "Color" | "Brush" => {
            let brush = d.metadata.editor == "Brush";
            let initial = brush_value(value, brush)?;
            let settings = ColorEditorSettings {
                is_read_only: d.metadata.read_only,
                allow_empty: f.nullable,
                popup_size: if brush {
                    Size::new(520., 550.)
                } else {
                    Size::new(500., 440.)
                },
                ..Default::default()
            };
            let parts = if brush {
                ui.brush_edit(host, name, initial.clone(), settings, row_style())?
            } else {
                ui.color_edit(host, name, solid(initial.as_ref()), settings, row_style())?
            };
            ui.set_button_chrome(parts.button, ButtonChrome::Flat)?;
            ui.set_edit_box_style(
                parts.editor,
                EditBoxStyle {
                    normal: crate::EditBoxAppearance {
                        background: Some(Srgba8::TRANSPARENT.into()),
                        ..Default::default()
                    },
                    corner_radius: Some(0.),
                    border_width: Some(0.),
                    ..Default::default()
                },
            )?;
            if brush {
                ui.stretch_brush_preview(parts)?;
            }
            Ok(Box::new(PaintEditor {
                parts,
                brush,
                original: RefCell::new((value.clone(), initial)),
            }))
        }
        "Calculator" => {
            let parts = ui.numeric_edit(
                host,
                name,
                as_decimal(value)?,
                calculator_settings(d, f),
                row_style(),
            )?;
            ui.set_composite_read_only(parts.container, d.metadata.read_only)?;
            Ok(Box::new(Calculator(parts)))
        }
        "Font" => {
            let items = ui
                .font_families()?
                .iter()
                .map(|name| SelectionItem::new(name, name))
                .collect();
            let parts = ui.combo_box(
                host,
                name,
                items,
                ComboBoxSettings {
                    editable: true,
                    read_only: d.metadata.read_only,
                },
                row_style(),
            )?;
            let font = FamilyEditor(parts);
            font.load(ui, value)?;
            Ok(Box::new(font))
        }
        "DatePicker" => date_picker::mount(ui, host, d, f, value),
        "Sparkline" => sparkline::mount(ui, host, d, f, value),
        "Image" => image::mount(ui, host, d, f, value),
        _ => Err(invalid("Unknown popup property editor")),
    }
}

fn parse_brush(s: &str, gradient: bool) -> Result<BrushValue, ControlError> {
    if let Some(body) = s
        .strip_prefix("linear(")
        .and_then(|s| s.strip_suffix(')'))
        .filter(|_| gradient)
    {
        let mut pieces = body.split(';');
        let angle = pieces
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .ok_or_else(|| invalid("Invalid gradient angle"))?;
        let stops = pieces
            .take(33)
            .map(|stop| {
                let (offset, color) = stop
                    .split_once('=')
                    .ok_or_else(|| invalid("Expected offset=#color"))?;
                let offset = offset
                    .parse::<f64>()
                    .map_err(|_| invalid("Invalid gradient stop offset"))?;
                let color = color
                    .parse::<Srgba8>()
                    .map_err(|e| invalid(e.to_string()))?;
                Ok(GradientStop::new(offset, color))
            })
            .collect::<Result<Vec<_>, ControlError>>()?;
        LinearGradient::new(angle, stops)
            .map(BrushValue::Linear)
            .map_err(|e| invalid(e.to_string()))
    } else {
        s.parse::<Srgba8>()
            .map(BrushValue::Solid)
            .map_err(|e| invalid(e.to_string()))
    }
}

fn encode_brush(brush: &BrushValue) -> String {
    match brush {
        BrushValue::Solid(color) => color.to_string(),
        BrushValue::Linear(g) => {
            use std::fmt::Write;
            let mut out = format!("linear({}", g.angle());
            for stop in g.stops() {
                write!(out, ";{}={}", stop.offset, stop.color).unwrap();
            }
            out.push(')');
            out
        }
    }
}

fn brush_value(v: &CellValue, brush: bool) -> Result<Option<BrushValue>, ControlError> {
    match v {
        CellValue::Null => Ok(None),
        CellValue::Text(s) => parse_brush(s, brush).map(Some),
        _ => Err(invalid("Expected a color or brush string")),
    }
}

fn solid(brush: Option<&BrushValue>) -> Option<Srgba8> {
    match brush {
        Some(BrushValue::Solid(c)) => Some(*c),
        _ => None,
    }
}

struct PaintEditor {
    parts: ColorEditor,
    brush: bool,
    // Keep the exact source spelling while the same color/gradient remains selected.
    original: RefCell<(CellValue, Option<BrushValue>)>,
}

impl PaintEditor {
    fn property_value(&self, value: Option<&BrushValue>) -> CellValue {
        let original = self.original.borrow();
        if original.1.as_ref() == value {
            original.0.clone()
        } else {
            value
                .map(encode_brush)
                .map_or(CellValue::Null, CellValue::Text)
        }
    }
}

impl InspectorEditor for PaintEditor {
    fn owner(&self) -> WidgetId {
        self.parts.container
    }
    fn focus(&self) -> WidgetId {
        if self.brush {
            self.parts.button
        } else {
            self.parts.editor
        }
    }
    fn inline(&self) -> bool {
        true
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let value = brush_value(v, self.brush)?;
        ui.set_brush_value_with_policy(
            self.owner(),
            value.clone(),
            ColorReplacePolicy::CancelPending,
        )?;
        self.original.replace((v.clone(), value));
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        if ui.color_popup_is_open(self.owner()) {
            return Err("Подтвердите или отмените выбор в палитре.".into());
        }
        let value = if self.brush {
            ui.brush_value(self.owner())
                .ok_or("Brush editor missing")?
                .cloned()
        } else {
            super::super::color_edit::parse_inline(
                ui.document(self.parts.editor)
                    .ok_or("Color editor missing")?
                    .text(),
            )?
        };
        Ok(self.property_value(value.as_ref()))
    }
    fn take_popup_acceptance(&mut self, ui: &mut Controls) -> Option<CellValue> {
        let entry = ui.color_editors.entries.get_mut(&self.parts.container)?;
        std::mem::take(&mut entry.popup_accepted).then(|| self.property_value(entry.value.as_ref()))
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        if !active && ui.color_popup_is_open(self.owner()) {
            ui.cancel_color_popup(self.owner())?;
        }
        Ok(())
    }
}

fn calculator_settings(d: &PropertyDescriptor, f: &FieldSchema) -> NumericEditSettings {
    let mut settings = NumericEditSettings {
        precision: 4,
        trim_trailing_zeros: true,
        allow_empty: f.nullable,
        ..Default::default()
    };
    if let Some(min) = d.minimum {
        settings.minimum = DecimalValue::new(i128::from(min), 0).unwrap();
    }
    if let Some(max) = d.maximum {
        settings.maximum = DecimalValue::new(i128::from(max), 0).unwrap();
    }
    if let Some(step) = d.step {
        settings.step = DecimalValue::new(i128::from(step), 0).unwrap();
    }
    settings
}

fn as_decimal(v: &CellValue) -> Result<Option<DecimalValue>, ControlError> {
    match v {
        CellValue::Null => Ok(None),
        CellValue::Decimal(n) => Ok(Some(*n)),
        _ => Err(invalid("Expected a decimal")),
    }
}

struct Calculator(EditorControl);
impl InspectorEditor for Calculator {
    fn owner(&self) -> WidgetId {
        self.0.container
    }
    fn focus(&self) -> WidgetId {
        self.0.editor
    }
    fn rest_label(&self, value: &CellValue) -> Option<String> {
        let CellValue::Decimal(amount) = value else {
            return None;
        };
        Some(super::super::numeric_model::format_decimal(
            &amount.to_string(),
            crate::UiCulture::Russian,
            false,
            None,
        ))
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        if ui.numeric_calculator_is_open(self.owner()) {
            ui.cancel_numeric_calculator(self.owner())?;
        }
        let result = ui.set_numeric_value(self.owner(), as_decimal(v)?)?;
        match result {
            CommitOutcome::Committed | CommitOutcome::Unchanged => Ok(()),
            CommitOutcome::Invalid(message) => Err(invalid(message)),
            _ => Err(invalid("Calculator could not load its value")),
        }
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        if ui.numeric_calculator_is_open(self.owner()) {
            return Err("Подтвердите результат в калькуляторе.".into());
        }
        let settings = ui
            .numeric_settings(self.owner())
            .ok_or("Calculator missing")?;
        Ok(settings
            .parse(
                ui.document(self.focus())
                    .ok_or("Numeric field missing")?
                    .text(),
            )?
            .map_or(CellValue::Null, CellValue::Decimal))
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        if !active && ui.numeric_calculator_is_open(self.owner()) {
            ui.cancel_numeric_calculator(self.owner())?;
        }
        Ok(())
    }
}

struct FamilyEditor(EditorControl);
impl InspectorEditor for FamilyEditor {
    fn owner(&self) -> WidgetId {
        self.0.container
    }
    fn focus(&self) -> WidgetId {
        self.0.editor
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let keys = match v {
            CellValue::Null => &[][..],
            CellValue::Text(name) => {
                let items = ui
                    .selection_items(self.owner())
                    .ok_or_else(|| invalid("Font list missing"))?;
                if !items.iter().any(|item| item.key == *name) {
                    let mut items = items.to_vec();
                    items.push(SelectionItem::new(name, name));
                    ui.set_selection_items(self.owner(), items)?;
                }
                std::slice::from_ref(name)
            }
            _ => return Err(invalid("Expected a font family")),
        };
        ui.set_selected_keys(self.owner(), keys)?;
        let text = keys.first().map_or("", String::as_str);
        if ui
            .document(self.focus())
            .is_none_or(|doc| doc.text() != text)
        {
            match ui.set_editor_value(self.focus(), text)? {
                CommitOutcome::Committed | CommitOutcome::Unchanged => {}
                _ => return Err(invalid("Could not restore the font family")),
            }
        }
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let keys = ui.selected_keys(self.owner()).ok_or("Font list missing")?;
        let typed = ui
            .document(self.focus())
            .ok_or("Font family input missing")?
            .text();
        if typed.is_empty() {
            return Ok(CellValue::Null);
        }
        if !keys.iter().any(|v| v == typed) {
            if let Some(item) = ui.selection_items(self.owner()).and_then(|items| {
                items
                    .iter()
                    .find(|item| item.key.eq_ignore_ascii_case(typed))
            }) {
                return Ok(CellValue::Text(item.key.clone()));
            }
            return Err("Выберите семейство из списка установленных шрифтов.".into());
        }
        Ok(keys
            .first()
            .cloned()
            .map_or(CellValue::Null, CellValue::Text))
    }
}

#[cfg(test)]
#[path = "popup_editors/tests.rs"]
mod tests;
