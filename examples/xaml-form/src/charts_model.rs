use crate::form::Form;
use rust_desktop_ui_controls::{
    ChartModel, ChartRange, ChartScale, ChartSeries, ChartSettings, ChartView,
};
use rust_desktop_ui_core::{LayoutStyle, Length, Size, WidgetId};
use rust_desktop_ui_platform_winit::{DesktopApp, EventResponse, PlatformEvent, accesskit};
use rust_desktop_ui_xaml::WindowDefinition;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[path = "charts_family_model.rs"]
mod family;
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
fn log_sample(generation: u32) -> Vec<ChartSeries> {
    let mut data = sample(generation);
    for series in &mut data {
        for point in &mut series.points {
            point.x = 10.0f64.powf(point.id as f64 / 2.0);
            point.y = point.y.map(|_| {
                10.0f64.powf((point.id as f64 + generation as f64 * 0.1) / 2.0)
                    * if series.id == 1 { 1.0 } else { 0.5 }
            });
        }
    }
    data
}
fn time_sample(generation: u32, calendar: bool) -> Vec<ChartSeries> {
    use rust_desktop_ui_data::{DateValue, ScheduleInstant, TimeValue};
    let noon = TimeValue::new(12, 0, 0, 0).unwrap();
    let start = ScheduleInstant::from_utc(DateValue::new(2024, 2, 28).unwrap(), noon)
        .unwrap()
        .0;
    let mut data = sample(generation);
    for series in &mut data {
        for p in &mut series.points {
            let seconds = if calendar {
                ScheduleInstant::from_utc(DateValue::new(2024, 1 + p.id as u8, 1).unwrap(), noon)
                    .unwrap()
                    .0
            } else {
                start + p.id * 43200
            };
            p.x = (seconds * 1000) as f64;
        }
    }
    data
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
    time_calendar: bool,
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
            ("ChartPie", "Доли"),
            ("ChartHeatmap", "Тепловая карта"),
            ("ChartTreemap", "Дерево площадей"),
            ("ChartSunburst", "Кольца дерева"),
            ("ChartSankey", "Потоки"),
            ("ChartStackedBar", "Накопление"),
            ("ChartArea", "Области"),
            ("ChartStackedArea", "Слои областей"),
            ("ChartLog", "Логарифм"),
            ("ChartTime", "Время UTC"),
            ("ChartCalendar", "Календарь UTC"),
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
            time_calendar: false,
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
                5 if m.family().is_none() => {
                    if s.x_range.is_some() {
                        s.x_range = None;
                        s.y_range = None
                    } else {
                        s.x_range = Some(ChartRange {
                            minimum: if s.x_scale == ChartScale::UtcMilliseconds {
                                m.series()[0].points[1].x
                            } else if s.x_scale == ChartScale::Log10 {
                                10.0
                            } else {
                                0.5
                            },
                            maximum: if s.x_scale == ChartScale::UtcMilliseconds {
                                m.series()[0].points[5].x
                            } else if s.x_scale == ChartScale::Log10 {
                                1000.0
                            } else {
                                5.5
                            },
                        });
                        s.y_range = Some(ChartRange {
                            minimum: if s.y_scale == ChartScale::Log10 {
                                1.0
                            } else {
                                -2.0
                            },
                            maximum: if s.y_scale == ChartScale::Log10 {
                                1000.0
                            } else {
                                12.0
                            },
                        });
                    }
                }
                6 => {
                    self.generation = 0;
                    s = settings(ChartView::Line);
                }
                7 => s.view = ChartView::Pie,
                8 => s.view = ChartView::Heatmap,
                9 => s.view = ChartView::Treemap,
                10 => s.view = ChartView::Sunburst,
                11 => s.view = ChartView::Sankey,
                12 => s.view = ChartView::StackedBar,
                13 => s.view = ChartView::Area,
                14 => s.view = ChartView::StackedArea,
                15 => {
                    s = settings(ChartView::Line);
                    s.x_scale = ChartScale::Log10;
                    s.y_scale = ChartScale::Log10;
                }
                16 | 17 => {
                    self.time_calendar = action == 17;
                    s = settings(ChartView::Line);
                    s.x_scale = ChartScale::UtcMilliseconds;
                }
                _ => {}
            }
            if matches!(
                s.view,
                ChartView::Pie
                    | ChartView::Heatmap
                    | ChartView::Treemap
                    | ChartView::Sunburst
                    | ChartView::Sankey
            ) {
                let view = s.view;
                m.replace_family(
                    family::sample(view, self.generation),
                    ChartSettings {
                        view,
                        ..Default::default()
                    },
                )
                .map_err(error)?;
            } else {
                if (m.family().is_some() && !matches!(action, 15..=17))
                    || matches!(
                        s.view,
                        ChartView::Bar
                            | ChartView::Area
                            | ChartView::StackedBar
                            | ChartView::StackedArea
                    ) && (s.x_scale != ChartScale::Linear || s.y_scale != ChartScale::Linear)
                {
                    s = settings(s.view);
                }
                let data = if s.x_scale == ChartScale::UtcMilliseconds {
                    time_sample(self.generation, self.time_calendar)
                } else if s.x_scale == ChartScale::Log10 || s.y_scale == ChartScale::Log10 {
                    log_sample(self.generation)
                } else {
                    sample(self.generation)
                };
                m.replace(data, s).map_err(error)?;
            }
            let cartesian = m.family().is_none();
            self.form.ui.set_chart_model(self.chart, m).map_err(error)?;
            let summary = self
                .form
                .ui
                .chart_semantic_value(self.chart)
                .unwrap()
                .to_owned();
            self.form
                .ui
                .set_label(self.form.find_name("ChartResult").unwrap(), &summary)
                .map_err(error)?;
            self.form
                .ui
                .commands_mut()
                .set_enabled(&"ChartRange".into(), cartesian);
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
            "chart-state view={:?} selected={:?} generation={} range={} status={:?} x={:?} y={:?}",
            m.settings().view,
            m.selected(),
            self.generation,
            if m.settings().x_range.is_some() {
                "manual"
            } else {
                "auto"
            },
            self.form.ui.chart_display_status(self.chart),
            m.x_range(),
            m.y_range()
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
            for command in [
                "ChartLine",
                "ChartScatter",
                "ChartBar",
                "ChartSparkline",
                "ChartPie",
                "ChartHeatmap",
                "ChartTreemap",
                "ChartSunburst",
                "ChartSankey",
                "ChartLog",
            ] {
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
                    "Доли",
                    "Тепловая карта",
                    "Дерево площадей",
                    "Кольца дерева",
                    "Потоки",
                    "Логарифм",
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
                let result = app
                    .form
                    .ui
                    .semantics()
                    .into_iter()
                    .find(|node| node.id == app.form.find_name("ChartResult").unwrap())
                    .unwrap();
                assert_eq!(
                    Some(result.label.as_str()),
                    app.form.ui.chart_semantic_value(app.chart)
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
