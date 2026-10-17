//! Explicit preview execution over an immutable application-owned sample.
use crate::form::Form;
use rust_desktop_ui_controls::{BindingContext, DataValue, QueryBuilderValue};
use rust_desktop_ui_core::{Rect, Scene, Size, WidgetId};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    QueryEngine, QueryRequest, TableSchema,
};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, EventResponse, PlatformEvent, accesskit,
};
use std::{cell::RefCell, rc::Rc, time::Duration};
pub(crate) struct QueryDemo {
    pub(crate) form: Form,
    table: DataTable,
    diagnostics: bool,
    last_snapshot: String,
}
impl QueryDemo {
    pub(crate) fn new(
        definition: rust_desktop_ui_xaml::WindowDefinition,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let schema = TableSchema::new(vec![
            FieldSchema::new("Count", FieldType::Integer),
            FieldSchema::new("Name", FieldType::Text),
            FieldSchema::new("Amount", FieldType::Decimal),
        ])
        .map_err(err)?;
        let table = DataTable::new(
            schema,
            vec![
                DataRow::new(
                    1,
                    vec![
                        CellValue::Integer(2),
                        CellValue::Text("Альфа".into()),
                        CellValue::Decimal("12.50".parse().map_err(err)?),
                    ],
                ),
                DataRow::new(
                    2,
                    vec![
                        CellValue::Integer(8),
                        CellValue::Text("Бета".into()),
                        CellValue::Decimal("25.00".parse().map_err(err)?),
                    ],
                ),
                DataRow::new(
                    3,
                    vec![
                        CellValue::Integer(9007199254740993),
                        CellValue::Text("Гамма".into()),
                        CellValue::Decimal("9007199254740993.01".parse().map_err(err)?),
                    ],
                ),
            ],
        )
        .map_err(err)?;
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([
                ("Filter", r#"ge(field("Count"),integer("5"))"#.into()),
                (
                    "Expression",
                    r#"add(field("Amount"),decimal("0.01"))"#.into(),
                ),
                ("Locked", false.into()),
            ]))
            .map_err(err)?,
        ));
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table.clone())).map_err(err)?,
        ));
        let mut form = Form::new_with_sources(
            definition,
            diagnostics,
            Some(context),
            vec![("QueryRows".into(), source)],
        )?;
        for name in ["Filter", "Expression"] {
            if let Some(owner) = form.find_name(name) {
                let p = form
                    .ui
                    .query_builder_parts(owner)
                    .ok_or("Builder missing")?
                    .clone();
                for (suffix, id) in [
                    ("Text", p.text),
                    ("Tree", p.tree),
                    ("Field", p.field.container),
                    ("Operator", p.operator.container),
                    ("Operand", p.operand),
                    ("Apply", p.apply),
                    ("Cancel", p.cancel),
                    ("Error", p.status),
                ] {
                    form.view.names.insert(format!("{name}{suffix}"), id);
                }
            }
        }
        Ok(Self {
            form,
            table,
            diagnostics,
            last_snapshot: String::new(),
        })
    }
    fn refresh(&mut self) -> Result<(), String> {
        for event in self.form.ui.take_query_builder_applied() {
            let text = match event.value {
                QueryBuilderValue::Criteria(criteria) => match QueryEngine::execute(
                    &self.table,
                    &QueryRequest {
                        filter: Some(criteria),
                        ..Default::default()
                    },
                ) {
                    Ok(result) => format!(
                        "Результат фильтра: ключи {:?}; источник содержит 3 строки",
                        result.row_keys
                    ),
                    Err(e) => e.to_string(),
                },
                QueryBuilderValue::Expression(value) => {
                    match QueryEngine::evaluate_expression(&self.table, 3, &value) {
                        Ok(result) => format!("Выражение для строки 3: {}", result.display()),
                        Err(e) => e.to_string(),
                    }
                }
            };
            if let Some(label) = self.form.find_name("QueryPreview") {
                self.form.ui.set_label(label, text).map_err(err)?;
            }
        }
        Ok(())
    }
}
impl DesktopApp for QueryDemo {
    fn set_waker(&mut self, w: Option<platform::AppWaker>) {
        self.form.set_waker(w);
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.refresh()?;
        self.form.update(size, scale)?;
        if self.diagnostics {
            let s = crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view);
            if s != self.last_snapshot {
                println!("batch-state {s}");
                self.last_snapshot = s;
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
        let mut r = self.form.event(event)?;
        self.refresh()?;
        r.redraw = true;
        Ok(r)
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
    fn wake_after(&self) -> Option<Duration> {
        self.form.wake_after()
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_compiled_demo_applies_exact_query_without_writing_source() {
        let mut app = QueryDemo::new(crate::generated_query::window_definition(), false).unwrap();
        app.update(Size::new(1120.0, 820.0), 1.0).unwrap();
        let owner = app.form.find_name("Filter").unwrap();
        let parts = app.form.ui.query_builder_parts(owner).unwrap().clone();
        for id in [
            parts.field.container,
            parts.operator.container,
            parts.operand,
            parts.text,
        ] {
            assert!(app.form.ui.runtime.bounds(id).unwrap().width >= 380.0);
        }
        for id in [parts.text, parts.operand, parts.apply, parts.cancel] {
            let bounds = app.form.ui.runtime.bounds(id).unwrap();
            assert!(bounds.width > 40.0 && bounds.height >= 30.0);
            let visible = bounds
                .intersection(app.form.ui.runtime.clip(id).unwrap())
                .unwrap();
            assert!((visible.width - bounds.width).abs() < 0.01);
            assert!((visible.height - bounds.height).abs() < 0.01);
        }
        app.form
            .ui
            .set_query_builder_text(owner, r#"eq(field("Count"),integer("9007199254740993"))"#)
            .unwrap();
        assert_eq!(app.table.rows().len(), 3);
        app.form.ui.apply_query_builder(owner).unwrap();
        app.refresh().unwrap();
        let label = app.form.find_name("QueryPreview").unwrap();
        assert!(
            app.form
                .ui
                .semantics()
                .iter()
                .find(|n| n.id == label)
                .unwrap()
                .label
                .contains("[3]")
        );
        assert_eq!(app.table.rows().len(), 3);
    }
}
