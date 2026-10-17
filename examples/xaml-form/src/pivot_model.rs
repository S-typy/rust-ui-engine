//! Application-owned accepted source and explicit drill-down projection.
use crate::form::Form;
#[cfg(test)]
use rust_desktop_ui_controls::PivotAxis;
use rust_desktop_ui_controls::{DataSourceHandle, PivotView};
use rust_desktop_ui_core::{LayoutStyle, Length, Rect, Scene, Size, WidgetId};
use rust_desktop_ui_data::{
    Aggregate, AggregateFunction, CellValue, CollectionAdapter, ComparisonOperator, Criteria,
    DataQuery, DataRow, DataSource, DataTable, FieldSchema, FieldType, PivotDefinition,
    PivotMeasure, QueryExpression, TableSchema,
};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, EventResponse, PlatformEvent, accesskit,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Duration};
pub(crate) struct PivotDemo {
    pub(crate) form: Form,
    pivot: PivotView,
    source: DataSourceHandle,
    drill: DataSourceHandle,
    actions: Rc<RefCell<VecDeque<u8>>>,
    diagnostics: bool,
    last_snapshot: String,
    drill_count: usize,
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
fn sample(count: usize) -> Result<DataTable, String> {
    let mut region = FieldSchema::new("Region", FieldType::Text);
    region.nullable = true;
    let mut amount = FieldSchema::new("Amount", FieldType::Decimal);
    amount.nullable = true;
    let schema = TableSchema::new(vec![
        region,
        FieldSchema::new("City", FieldType::Text),
        FieldSchema::new("Year", FieldType::Integer),
        amount,
    ])
    .map_err(err)?;
    DataTable::new(
        schema,
        (0..count)
            .map(|i| {
                DataRow::new(
                    if i == 0 {
                        i64::MIN
                    } else if i == 1 {
                        i64::MAX
                    } else {
                        i as i64
                    },
                    vec![
                        if i % 17 == 0 {
                            CellValue::Null
                        } else if i % 19 == 0 {
                            CellValue::Text(String::new())
                        } else {
                            CellValue::Text(format!(
                                "Регион {}",
                                i % (if count >= 100_000 { 250 } else { 5 }) + 1
                            ))
                        },
                        CellValue::Text(format!("Город {}", i % 6 + 1)),
                        CellValue::Integer(2025 + (i % 2) as i64),
                        if i % 11 == 0 {
                            CellValue::Null
                        } else {
                            CellValue::Decimal(format!("{}.25", i % 100).parse().unwrap())
                        },
                    ],
                )
            })
            .collect(),
    )
    .map_err(err)
}
fn definition() -> PivotDefinition {
    PivotDefinition {
        row_fields: vec!["Region".into(), "City".into()],
        column_fields: vec!["Year".into()],
        measures: vec![
            PivotMeasure {
                title: "Сумма".into(),
                aggregate: Aggregate {
                    function: AggregateFunction::Sum,
                    expression: Some(QueryExpression::field("Amount")),
                },
            },
            PivotMeasure {
                title: "Среднее".into(),
                aggregate: Aggregate {
                    function: AggregateFunction::Average,
                    expression: Some(QueryExpression::field("Amount")),
                },
            },
            PivotMeasure {
                title: "Записей".into(),
                aggregate: Aggregate {
                    function: AggregateFunction::CountRows,
                    expression: None,
                },
            },
        ],
        filter: None,
    }
}
impl PivotDemo {
    pub(crate) fn new(
        definition: rust_desktop_ui_xaml::WindowDefinition,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let table = sample(240)?;
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table.clone())).map_err(err)?,
        ));
        let drill = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(
                DataTable::new(table.schema().clone(), Vec::new()).map_err(err)?,
            ))
            .map_err(err)?,
        ));
        let mut form = Form::new_with_sources(
            definition,
            diagnostics,
            None,
            vec![("PivotDrill".into(), drill.clone())],
        )?;
        let host = form.find_name("PivotHost").ok_or("PivotHost missing")?;
        let pivot = PivotView::mount(
            &mut form.ui,
            host,
            source.clone(),
            self::definition(),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .map_err(err)?;
        for (name, id) in [
            ("PivotGrid", pivot.parts.grid),
            ("PivotRows", pivot.parts.row_fields),
            ("PivotColumns", pivot.parts.column_fields),
            ("PivotStatus", pivot.parts.status),
            ("PivotExpandRow", pivot.parts.expand_row),
            ("PivotCollapseRow", pivot.parts.collapse_row),
            ("PivotExpandColumn", pivot.parts.expand_column),
            ("PivotCollapseColumn", pivot.parts.collapse_column),
            ("PivotDrill", pivot.parts.drill_down),
        ] {
            form.view.names.insert(name.into(), id);
        }
        for field in ["Region", "City", "Year"] {
            form.view.names.insert(
                format!("PivotField{field}"),
                pivot.field_widget(field).unwrap(),
            );
        }
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let host = form.find_name("PivotActions").unwrap();
        for (index, (name, label)) in [
            ("PivotSmall", "240 записей"),
            ("PivotMany", "100 000 записей"),
            ("PivotFilter", "Только 2026"),
            ("PivotAll", "Все годы"),
        ]
        .into_iter()
        .enumerate()
        {
            form.ui.commands_mut().register(name, label);
            let queue = actions.clone();
            form.ui.commands_mut().on_execute(&name.into(), move || {
                let mut q = queue.borrow_mut();
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
                        width: Length::Px(180.0),
                        height: Length::Px(36.0),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
            form.view.names.insert(name.into(), id);
        }
        Ok(Self {
            form,
            pivot,
            source,
            drill,
            actions,
            diagnostics,
            last_snapshot: String::new(),
            drill_count: 0,
        })
    }
    fn refresh(&mut self) -> Result<(), String> {
        let actions = self.actions.borrow_mut().drain(..).collect::<Vec<_>>();
        for action in actions {
            match action {
                0 | 1 => {
                    self.source
                        .borrow_mut()
                        .replace_adapter(
                            CollectionAdapter::new(sample(if action == 1 {
                                100_000
                            } else {
                                240
                            })?),
                            DataQuery::default(),
                        )
                        .map_err(err)?;
                }
                _ => {
                    let mut d = self.pivot.snapshot().definition().clone();
                    d.filter = if action == 2 {
                        Some(Criteria::Compare {
                            left: QueryExpression::field("Year"),
                            operator: ComparisonOperator::Equal,
                            right: QueryExpression::literal(CellValue::Integer(2026)),
                        })
                    } else {
                        None
                    };
                    self.pivot
                        .set_definition(&mut self.form.ui, d)
                        .map_err(err)?;
                }
            }
        }
        self.pivot.update(&mut self.form.ui).map_err(err)?;
        for event in self.pivot.take_drill_down_events() {
            let source = self.source.borrow();
            if source.revision() != event.data.source.revision {
                continue;
            }
            let rows = event
                .data
                .row_keys
                .iter()
                .filter_map(|key| source.table().row(*key).cloned())
                .collect::<Vec<_>>();
            self.drill_count = rows.len();
            let table = DataTable::new(source.table().schema().clone(), rows).map_err(err)?;
            self.drill
                .borrow_mut()
                .replace_adapter(CollectionAdapter::new(table), DataQuery::default())
                .map_err(err)?;
            let text = format!(
                "Исходные записи: {} · Версия {} · Ключи: {:?}",
                self.drill_count,
                event.data.source.revision,
                event.data.row_keys.iter().take(5).collect::<Vec<_>>()
            );
            self.form
                .ui
                .set_label(self.form.find_name("PivotResult").unwrap(), text)
                .map_err(err)?;
        }
        Ok(())
    }
}
impl DesktopApp for PivotDemo {
    fn set_waker(&mut self, w: Option<platform::AppWaker>) {
        self.form.set_waker(w)
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.refresh()?;
        self.form.update(size, scale)?;
        if self.diagnostics {
            let state = format!(
                "matched={} rows={} visible={} columns={} row_fields={:?} column_fields={:?} drill={} error={:?}",
                self.pivot.snapshot().matched_row_count(),
                self.form
                    .ui
                    .grid_frame(self.pivot.parts.grid)
                    .unwrap()
                    .stats
                    .logical_rows,
                self.form
                    .ui
                    .grid_frame(self.pivot.parts.grid)
                    .unwrap()
                    .stats
                    .materialized_rows,
                self.form
                    .ui
                    .grid_settings(self.pivot.parts.grid)
                    .unwrap()
                    .columns
                    .len(),
                self.pivot.snapshot().definition().row_fields,
                self.pivot.snapshot().definition().column_fields,
                self.drill_count,
                self.pivot.error()
            );
            let s = crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view);
            let combined = format!("{state}{s}");
            if combined != self.last_snapshot {
                println!("pivot-state {state}");
                println!("batch-state {s}");
                self.last_snapshot = combined;
            }
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.form.scene()
    }
    fn title(&self) -> String {
        self.form.title()
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if let PlatformEvent::Input(input) = &event
            && self
                .pivot
                .before_input(&mut self.form.ui, input)
                .map_err(err)?
        {
            self.refresh()?;
            return Ok(EventResponse::redraw());
        }
        let mut response = self.form.event(event)?;
        self.refresh()?;
        response.redraw = true;
        Ok(response)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.pivot.cancel_pointer(&mut self.form.ui).map_err(err)?;
        self.form.pointer_capture_lost()
    }
    fn wake_after(&self) -> Option<Duration> {
        self.form.wake_after()
    }
    fn ime_cursor(&self) -> Option<Rect> {
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
    #[test]
    fn actual_pivot_form_aggregates_filters_expands_and_drills_without_changing_source() {
        let mut app = PivotDemo::new(crate::generated_pivot::window_definition(), false).unwrap();
        app.update(Size::new(1400.0, 900.0), 1.0).unwrap();
        let grid = app.pivot.parts.grid;
        let b = app.form.ui.runtime.bounds(grid).unwrap();
        assert!(b.width > 1000.0 && b.height > 250.0, "{b:?}");
        app.pivot.request_drill_down(&mut app.form.ui).unwrap();
        app.refresh().unwrap();
        assert_eq!(app.drill_count, 240);
        assert_eq!(app.source.borrow().table().rows().len(), 240);
        app.form.ui.commands_mut().execute(&"PivotFilter".into());
        app.update(Size::new(900.0, 760.0), 1.0).unwrap();
        assert_eq!(app.pivot.snapshot().matched_row_count(), 120);
        app.pivot
            .move_field(&mut app.form.ui, "City", PivotAxis::Columns, 1)
            .unwrap();
        app.update(Size::new(900.0, 760.0), 1.0).unwrap();
        assert_eq!(app.pivot.parts.grid, grid);
        for id in [app.pivot.parts.expand_row, app.pivot.parts.drill_down] {
            let b = app.form.ui.runtime.bounds(id).unwrap();
            let c = app.form.ui.runtime.clip(id).unwrap();
            assert!(
                b.intersection(c)
                    .is_some_and(|v| v.height >= b.height - 0.1 && v.width >= b.width - 0.1),
                "{b:?} / {c:?}"
            );
        }
    }
    #[test]
    fn actual_pivot_many_command_uses_typed_source_without_binding_limits() {
        let mut app = PivotDemo::new(crate::generated_pivot::window_definition(), false).unwrap();
        app.update(Size::new(1400.0, 900.0), 1.0).unwrap();
        let id = app.form.find_name("PivotMany").unwrap();
        let b = app.form.ui.runtime.bounds(id).unwrap();
        let point = rust_desktop_ui_core::Point::new(b.x + 10.0, b.y + 10.0);
        for input in [
            rust_desktop_ui_core::InputEvent::PointerDown {
                position: point,
                button: rust_desktop_ui_core::PointerButton::Primary,
                modifiers: Default::default(),
            },
            rust_desktop_ui_core::InputEvent::PointerUp {
                position: point,
                button: rust_desktop_ui_core::PointerButton::Primary,
                modifiers: Default::default(),
            },
        ] {
            app.event(PlatformEvent::Input(input)).unwrap();
        }
        app.update(Size::new(1400.0, 900.0), 1.0).unwrap();
        assert_eq!(app.pivot.snapshot().matched_row_count(), 100_000);
        assert!(
            app.form
                .ui
                .grid_frame(app.pivot.parts.grid)
                .unwrap()
                .stats
                .materialized_rows
                < 50
        );
    }
}
