use crate::form::Form;
use rust_desktop_ui_controls::{ChartModel, ChartRange, ChartSeries, ChartSettings, ChartView};
use rust_desktop_ui_core::{LayoutStyle, Length, Size, WidgetId};
use rust_desktop_ui_platform_winit::{DesktopApp, EventResponse, PlatformEvent, accesskit};
use rust_desktop_ui_xaml::WindowDefinition;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
fn sample(generation: u32) -> Vec<ChartSeries> {
    use rust_desktop_ui_data::{
        CellValue, DataRow, DataTable, FieldSchema, FieldType, TableSchema,
    };
    let mut y = FieldSchema::new("Y", FieldType::Real);
    y.nullable = true;
    let schema = TableSchema::new(vec![FieldSchema::new("X", FieldType::Real), y]).unwrap();
    [(1, "Доход"), (2, "Расход")]
        .into_iter()
        .map(|(id, label)| {
            let rows = (0..7)
                .map(|i| {
                    DataRow::new(
                        i,
                        vec![
                            CellValue::Real(i as f64),
                            if i == 3 {
                                CellValue::Null
                            } else {
                                CellValue::Real(if id == 1 {
                                    (i as f64 - 2.0) * 3.0 + generation as f64
                                } else {
                                    (6 - i) as f64 + generation as f64 * 0.5
                                })
                            },
                        ],
                    )
                })
                .collect();
            ChartSeries::from_table(
                &DataTable::new(schema.clone(), rows).unwrap(),
                id,
                label,
                "X",
                "Y",
            )
            .unwrap()
        })
        .collect()
}
fn settings(view: ChartView) -> ChartSettings {
    ChartSettings {
        view,
        x_title: "Период".into(),
        y_title: "Сумма".into(),
        ..Default::default()
    }
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub struct ChartsDemo {
    form: Form,
    chart: WidgetId,
    spark: WidgetId,
    actions: Rc<RefCell<VecDeque<u8>>>,
    generation: u32,
    diagnostics: bool,
    last: String,
}
impl ChartsDemo {
    pub fn new(definition: WindowDefinition, diagnostics: bool) -> Result<Self, String> {
        let mut form = Form::new(definition, false)?;
        let host = form.find_name("ChartHost").unwrap();
        let sparkhost = form.find_name("SparkHost").unwrap();
        let style = LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        };
        let chart = form
            .ui
            .chart(
                host,
                ChartModel::new(sample(0), settings(ChartView::Line)).map_err(error)?,
                style.clone(),
            )
            .map_err(error)?;
        let spark = form
            .ui
            .chart(
                sparkhost,
                ChartModel::new(sample(0), settings(ChartView::Sparkline)).map_err(error)?,
                style,
            )
            .map_err(error)?;
        form.view.names.insert("ChartMain".into(), chart);
        form.view.names.insert("ChartSpark".into(), spark);
        form.ui
            .set_automation_name(chart, "Основная диаграмма")
            .map_err(error)?;
        form.ui
            .set_automation_name(spark, "Компактный график")
            .map_err(error)?;
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let host = form.find_name("ChartActions").unwrap();
        for (index, (name, label)) in [
            ("ChartLine", "Линии"),
            ("ChartScatter", "Точки"),
            ("ChartBar", "Столбцы"),
            ("ChartSparkline", "Sparkline"),
            ("ChartUpdate", "Обновить"),
            ("ChartRange", "Диапазон"),
            ("ChartReset", "Сбросить"),
        ]
        .into_iter()
        .enumerate()
        {
            form.ui.commands_mut().register(name, label);
            let q = actions.clone();
            form.ui.commands_mut().on_execute(&name.into(), move || {
                let mut q = q.borrow_mut();
                if q.len() < 16 {
                    q.push_back(index as u8)
                }
            });
            let id = form
                .ui
                .button(
                    host,
                    label,
                    Some(name.into()),
                    LayoutStyle {
                        width: Length::Px(160.0),
                        height: Length::Px(36.0),
                        ..Default::default()
                    },
                )
                .map_err(error)?;
            form.view.names.insert(name.into(), id);
        }
        Ok(Self {
            form,
            chart,
            spark,
            actions,
            generation: 0,
            diagnostics,
            last: String::new(),
        })
    }
    fn refresh(&mut self) -> Result<(), String> {
        let actions = self.actions.borrow_mut().drain(..).collect::<Vec<_>>();
        for action in actions {
            let mut m = self.form.ui.chart_model(self.chart).unwrap().clone();
            let mut s = m.settings().clone();
            match action {
                0 => s.view = ChartView::Line,
                1 => s.view = ChartView::Scatter,
                2 => s.view = ChartView::Bar,
                3 => s.view = ChartView::Sparkline,
                4 => self.generation = (self.generation + 1) % 5,
                5 => {
                    if s.x_range.is_some() {
                        s.x_range = None;
                        s.y_range = None
                    } else {
                        s.x_range = Some(ChartRange {
                            minimum: 0.5,
                            maximum: 5.5,
                        });
                        s.y_range = Some(ChartRange {
                            minimum: -2.0,
                            maximum: 12.0,
                        });
                    }
                }
                _ => {
                    self.generation = 0;
                    s = settings(ChartView::Line);
                }
            }
            m.replace(sample(self.generation), s).map_err(error)?;
            self.form.ui.set_chart_model(self.chart, m).map_err(error)?;
            let mut spark = self.form.ui.chart_model(self.spark).unwrap().clone();
            spark
                .replace(sample(self.generation), settings(ChartView::Sparkline))
                .map_err(error)?;
            self.form
                .ui
                .set_chart_model(self.spark, spark)
                .map_err(error)?;
        }
        let events = self.form.ui.take_chart_selection_changes();
        if let Some(event) = events.last() {
            let text = self
                .form
                .ui
                .chart_semantic_value(event.control)
                .unwrap()
                .to_owned();
            self.form
                .ui
                .set_label(self.form.find_name("ChartResult").unwrap(), &text)
                .map_err(error)?;
        }
        Ok(())
    }
    fn diagnostic(&mut self) {
        if !self.diagnostics {
            return;
        }
        let m = self.form.ui.chart_model(self.chart).unwrap();
        let state = format!(
            "chart-state view={:?} selected={:?} generation={} range={} status={:?}",
            m.settings().view,
            m.selected(),
            self.generation,
            if m.settings().x_range.is_some() {
                "manual"
            } else {
                "auto"
            },
            self.form.ui.chart_display_status(self.chart)
        );
        let snapshot = crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view);
        let combined = format!("{state}{snapshot}");
        if combined != self.last {
            println!("{state}");
            self.last = combined;
            if let Ok(Some(g)) = self.form.ui.chart_geometry(self.chart) {
                for p in g.points {
                    println!(
                        "chart-point series={} key={} x={} y={}",
                        p.key.series, p.key.point, p.position.x, p.position.y
                    );
                }
            }
            println!("batch-state {snapshot}");
        }
    }
}
impl DesktopApp for ChartsDemo {
    fn title(&self) -> String {
        self.form.title()
    }
    fn scene(&self) -> &rust_desktop_ui_core::Scene {
        self.form.scene()
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.refresh()?;
        self.form.update(size, scale)?;
        self.diagnostic();
        Ok(())
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        let mut response = self.form.event(event)?;
        self.refresh()?;
        response.redraw = true;
        Ok(response)
    }
    fn set_waker(&mut self, w: Option<rust_desktop_ui_platform_winit::AppWaker>) {
        self.form.set_waker(w)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
    fn wake_after(&self) -> Option<std::time::Duration> {
        self.form.wake_after()
    }
    fn ime_cursor(&self) -> Option<rust_desktop_ui_core::Rect> {
        self.form.ime_cursor()
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.form.ime_target()
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.form.ime_composition_active()
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{InputEvent, PointerButton};
    #[test]
    fn charts_compiled_form_all_modes_have_full_real_geometry_and_live_selection() {
        let mut app = ChartsDemo::new(crate::generated_charts::window_definition(), false).unwrap();
        app.update(Size::new(1100.0, 900.0), 1.0).unwrap();
        for width in [760.0, 1100.0] {
            for command in ["ChartLine", "ChartScatter", "ChartBar", "ChartSparkline"] {
                let b = app
                    .form
                    .ui
                    .runtime
                    .bounds(app.form.find_name(command).unwrap())
                    .unwrap();
                let p = rust_desktop_ui_core::Point::new(b.x + 10.0, b.y + 10.0);
                for input in [
                    InputEvent::PointerDown {
                        position: p,
                        button: PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                    InputEvent::PointerUp {
                        position: p,
                        button: PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ] {
                    app.event(PlatformEvent::Input(input)).unwrap();
                }
                app.update(Size::new(width, 900.0), 1.0).unwrap();
                for label in [
                    "Линии",
                    "Точки",
                    "Столбцы",
                    "Sparkline",
                    "Обновить",
                    "Диапазон",
                    "Сбросить",
                ] {
                    assert!(
                        app.form
                            .ui
                            .scene()
                            .texts
                            .iter()
                            .any(|run| run.text == label),
                        "Missing full caption {label}"
                    );
                }
                assert_eq!(
                    app.form.ui.chart_display_status(app.chart),
                    Some(rust_desktop_ui_controls::ChartDisplayStatus::Full)
                );
                for name in ["ChartLine", "ChartUpdate", "ChartReset"] {
                    let b = app
                        .form
                        .ui
                        .runtime
                        .bounds(app.form.find_name(name).unwrap())
                        .unwrap();
                    let c = app
                        .form
                        .ui
                        .runtime
                        .clip(app.form.find_name(name).unwrap())
                        .unwrap();
                    assert!(b.intersection(c).is_some_and(|r|r.width>=b.width-0.1&&r.height>=b.height-0.1));
                }
                let g = app.form.ui.chart_geometry(app.chart).unwrap().unwrap();
                let p = g.points[0];
                for input in [
                    InputEvent::PointerDown {
                        position: p.position,
                        button: PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                    InputEvent::PointerUp {
                        position: p.position,
                        button: PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ] {
                    app.event(PlatformEvent::Input(input)).unwrap();
                }
                assert_eq!(
                    app.form.ui.chart_model(app.chart).unwrap().selected(),
                    Some(p.key)
                );
                app.form.ui.commands_mut().execute(&"ChartUpdate".into());
                app.update(Size::new(width, 900.0), 1.0).unwrap();
                assert_eq!(
                    app.form.ui.chart_model(app.chart).unwrap().selected(),
                    Some(p.key)
                );
            }
        }
    }
}
