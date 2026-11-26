use super::*;

const MAX_POINTS: usize = 512;
const MAX_BYTES: usize = 16_384;

pub(super) fn model(text: &str) -> Result<ChartModel, ControlError> {
    if text.len() > MAX_BYTES {
        return Err(invalid("A sparkline requires at most 16 KiB of JSON"));
    }
    let values: Vec<f64> =
        serde_json::from_str(text).map_err(|_| invalid("Введите JSON-массив конечных чисел."))?;
    if values.len() > MAX_POINTS {
        return Err(invalid("Допускается до 512 точек."));
    }
    let points = values
        .into_iter()
        .enumerate()
        .map(|(index, y)| ChartPoint {
            id: index as i64,
            x: index as f64,
            y: Some(y),
        })
        .collect();
    ChartModel::new(
        vec![ChartSeries {
            id: 1,
            label: "Значения".into(),
            points,
        }],
        ChartSettings {
            view: ChartView::Sparkline,
            legend: false,
            ..Default::default()
        },
    )
    .map_err(|e| invalid(e.to_string()))
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
            kind: LayoutKind::Stack(Axis::Vertical),
            ..Default::default()
        },
    )?;
    let chart = ui.chart(
        owner,
        model("[]")?,
        LayoutStyle {
            width: Length::Percent(1.),
            height: Length::Px(28.),
            ..Default::default()
        },
    )?;
    ui.set_automation_name(chart, format!("{name}: мини-график"))?;
    let input = ui.edit_box(owner, name, "", row_style())?;
    ui.set_editor_settings(
        input,
        EditorSettings {
            max_length: Some(MAX_BYTES),
            is_read_only: d.metadata.read_only,
            ..Default::default()
        },
    )?;
    show(ui, input, false)?;
    let editor = Sparkline {
        owner,
        chart,
        input,
        nullable: f.nullable,
        drawn_text: RefCell::new(String::new()),
    };
    editor.load(ui, value)?;
    Ok(Box::new(editor))
}

struct Sparkline {
    owner: WidgetId,
    chart: WidgetId,
    input: WidgetId,
    nullable: bool,
    drawn_text: RefCell<String>,
}
impl Sparkline {
    fn series(&self, text: &str) -> Result<ChartModel, ControlError> {
        model(if text.is_empty() && self.nullable {
            "[]"
        } else {
            text
        })
    }
}
impl InspectorEditor for Sparkline {
    fn owner(&self) -> WidgetId {
        self.owner
    }
    fn focus(&self) -> WidgetId {
        self.input
    }
    fn inline(&self) -> bool {
        true
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let text = match v {
            CellValue::Null if self.nullable => "",
            CellValue::Text(text) => text,
            _ => return Err(invalid("Expected a sparkline series")),
        };
        ui.set_chart_model(self.chart, self.series(text)?)?;
        ui.set_editor_value(self.input, text)?;
        self.drawn_text.replace(text.to_owned());
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let text = ui
            .document(self.input)
            .ok_or("Sparkline data field missing")?
            .text();
        self.series(text).map_err(|e| e.to_string())?;
        if text.is_empty() && self.nullable {
            Ok(CellValue::Null)
        } else {
            Ok(CellValue::Text(text.into()))
        }
    }
    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let text = ui
            .document(self.input)
            .ok_or_else(|| invalid("Sparkline data field missing"))?
            .text();
        if self.drawn_text.borrow().as_str() != text {
            let next = self.series(text);
            self.drawn_text.replace(text.to_owned());
            if let Ok(next) = next {
                ui.set_chart_model(self.chart, next)?;
            }
        }
        Ok(())
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        show(ui, self.input, active)
    }
}
