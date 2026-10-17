//! Schema references and accepted canonical query bindings.
use super::*;
use rust_desktop_ui_controls::{QueryBuilderMode, QueryBuilderSettings, QueryBuilderValue};
use rust_desktop_ui_data::{TableSchema, parse_criteria_text, parse_expression_text};
use rust_desktop_ui_xaml::{QueryBuilderDefinition, QueryBuilderModeDefinition};
fn prepare(
    ui: &Controls,
    value: &QueryBuilderDefinition,
    expression: bool,
) -> Result<(TableSchema, QueryBuilderValue), String> {
    let source = ui
        .data_source(&value.data_source)
        .ok_or_else(|| format!("Unknown query DataSource '{}'", value.data_source))?;
    let schema = source.try_borrow().map_err(err)?.table().schema().clone();
    let ast = if expression {
        QueryBuilderValue::Expression(parse_expression_text(&schema, &value.value).map_err(err)?)
    } else {
        QueryBuilderValue::Criteria(parse_criteria_text(&schema, &value.value).map_err(err)?)
    };
    Ok((schema, ast))
}
pub(super) fn validate(
    ui: &Controls,
    value: &QueryBuilderDefinition,
    expression: bool,
) -> Result<(), String> {
    prepare(ui, value, expression).map(|_| ())
}
impl MountedView {
    pub(super) fn mount_query_builder(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &QueryBuilderDefinition,
        expression: bool,
        style: LayoutStyle,
    ) -> Result<WidgetId, String> {
        let (schema, ast) = prepare(ui, value, expression)?;
        let parts = ui
            .query_builder(
                parent,
                schema,
                ast,
                QueryBuilderSettings {
                    read_only: value.is_read_only,
                    mode: match value.mode {
                        QueryBuilderModeDefinition::Visual => QueryBuilderMode::Visual,
                        QueryBuilderModeDefinition::Text => QueryBuilderMode::Text,
                    },
                },
                style,
            )
            .map_err(err)?;
        Ok(parts.container)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, Theme};
    use rust_desktop_ui_core::Size;
    use rust_desktop_ui_data::{
        CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    };
    use std::{cell::RefCell, rc::Rc};
    fn source(ui: &mut Controls) {
        let table = DataTable::new(
            TableSchema::new(vec![FieldSchema::new("Count", FieldType::Integer)]).unwrap(),
            vec![DataRow::new(1, vec![CellValue::Integer(5)])],
        )
        .unwrap();
        ui.register_data_source(
            "Rows",
            Rc::new(RefCell::new(
                DataSource::new(CollectionAdapter::new(table)).unwrap(),
            )),
        )
        .unwrap();
    }
    #[test]
    fn compiled_filter_keeps_model_unchanged_until_apply_and_reports_invalid() {
        let w=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><FilterBuilder x:Name='Filter' DataSource='Rows' Value='{Binding Filter}' Width='900' Height='520'/></Window>").unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        source(&mut ui);
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Filter", "true()".into())])).unwrap(),
        ));
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &w.content, context.clone()).unwrap();
        // The explicitly sized child must fit inside Controls' root inset.
        ui.update(Size::new(916.0, 536.0)).unwrap();
        view.update_form_layout(&mut ui, Size::new(916.0, 536.0))
            .unwrap();
        let owner = view.find_name("Filter").unwrap();
        let parts = ui.query_builder_parts(owner).unwrap().clone();
        ui.set_query_builder_text(owner, "false()").unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(
            context.borrow().get("Filter").unwrap(),
            &DataValue::String("true()".into())
        );
        ui.apply_query_builder(owner).unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(
            context.borrow().get("Filter").unwrap(),
            &DataValue::String("false()".into())
        );
        ui.set_query_builder_text(owner, "unknown(").unwrap();
        assert!(ui.apply_query_builder(owner).is_err());
        ui.cancel_query_builder(owner).unwrap();
        assert_eq!(ui.query_builder_parts(owner).unwrap().text, parts.text);
        let b = ui.runtime.bounds(parts.apply).unwrap();
        assert!(b.width > 50.0 && b.height > 20.0);
        assert_eq!(
            b.intersection(ui.runtime.clip(parts.apply).unwrap()),
            Some(b)
        );
        assert_eq!(
            ui.data_source("Rows")
                .unwrap()
                .borrow()
                .row(1)
                .unwrap()
                .values,
            vec![CellValue::Integer(5)]
        );
    }
    #[test]
    fn missing_schema_or_invalid_typed_expression_fails_before_mount() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        source(&mut ui);
        for value in ["Missing", "Rows"] {
            let text = format!(
                "<Window xmlns='urn:rust-ui-engine:ui'><ExpressionBuilder DataSource='{value}' Value='add(field(&quot;Count&quot;),text(&quot;bad&quot;))'/></Window>"
            );
            let w = rust_desktop_ui_xaml::parse(&text).unwrap();
            let before = ui.runtime.tree().children(ui.root()).unwrap().to_vec();
            let root = ui.root();
            assert!(MountedView::mount(&mut ui, root, &w.content).is_err());
            assert_eq!(ui.runtime.tree().children(root).unwrap(), before);
        }
    }
}
