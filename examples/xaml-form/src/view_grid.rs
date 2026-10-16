//! Named typed data sources are registered by the Rust application before mount.
use super::*;
use rust_desktop_ui_controls::{
    DataFormFactories, DataSourceHandle, GridBand, GridCellTemplate, GridControlColumn,
    GridControlSettings, GridDetailSettings, GridPresentationSettings, GridSummary, GridViewMode,
};
use rust_desktop_ui_data::{Aggregate, AggregateFunction, QueryExpression};
use rust_desktop_ui_xaml::GridControlDefinition;
use rust_desktop_ui_xaml::{GridColumnDefinition, GridSummaryFunction, GridViewDefinition};
use std::{cell::RefCell, rc::Rc};

pub(super) fn validate_source(
    ui: &Controls,
    definition: &GridControlDefinition,
) -> Result<(), String> {
    rust_desktop_ui_xaml::validate_grid_control(definition)?;
    let source = ui
        .data_source(&definition.data_source)
        .ok_or_else(|| format!("Unknown data source '{}'", definition.data_source))?;
    let source_handle = source.clone();
    let source = source.try_borrow().map_err(err)?;
    if definition.columns.is_empty()
        && source.table().schema().fields().len() > rust_desktop_ui_xaml::MAX_GRID_COLUMNS
    {
        return Err("Automatic GridControl columns exceed the 64-column limit".into());
    }
    for column in &definition.columns {
        if source.table().schema().field(&column.field).is_none() {
            return Err(format!(
                "Unknown grid field '{}' in source '{}'",
                column.field, definition.data_source
            ));
        }
    }
    if let Some(key) = &definition.selected_value {
        let key = key.parse::<i64>().map_err(err)?;
        let visible = if definition.filter_text.is_empty() {
            // Existing shared queries may hide a selected row; identity still
            // belongs to the accepted source snapshot.
            source.row(key).is_some()
        } else {
            // Preview a local query without modifying the registered source
            // or issuing I/O through its adapter.
            let mut preview = rust_desktop_ui_data::DataSource::new(
                rust_desktop_ui_data::CollectionAdapter::new(source.table().clone()),
            )
            .map_err(err)?;
            let mut query = source.query().clone();
            query.filter.clone_from(&definition.filter_text);
            preview.set_query(query).map_err(err)?;
            preview.position(key).is_some()
        };
        if !visible {
            return Err(format!(
                "Selected grid row '{key}' is absent from the initial query"
            ));
        }
    }
    let columns = columns(&definition.columns, &source_handle)?;
    let presentation = presentation(definition);
    let detail = detail(ui, definition)?;
    if detail
        .as_ref()
        .is_some_and(|d| Rc::ptr_eq(&d.source, &source_handle))
    {
        return Err("Grid detail source must be distinct from its master source".into());
    }
    let mut preview = rust_desktop_ui_data::DataSource::new(
        rust_desktop_ui_data::CollectionAdapter::new(source.table().clone()),
    )
    .map_err(err)?;
    let mut query = source.query().clone();
    if !definition.filter_text.is_empty() {
        query.filter.clone_from(&definition.filter_text);
    }
    preview.set_query(query).map_err(err)?;
    drop(source);
    presentation
        .validate_source(&Rc::new(RefCell::new(preview)), &columns, detail.as_ref())
        .map_err(err)?;
    for editor in &definition.editors {
        Controls::validate_grid_editor_factory(
            &source_handle,
            &editor.field,
            &editor.factory,
            &DataFormFactories::default(),
        )
        .map_err(err)?;
    }
    let known: std::collections::HashSet<_> = columns.iter().map(|c| c.field.as_str()).collect();
    if definition
        .cell_templates
        .iter()
        .any(|t| !known.contains(t.field.as_str()))
        || definition
            .editors
            .iter()
            .any(|e| !known.contains(e.field.as_str()))
    {
        return Err("Grid cell template/editor must reference a visible declared field".into());
    }
    Ok(())
}

fn column(column: &GridColumnDefinition) -> GridControlColumn {
    GridControlColumn {
        field: column.field.clone(),
        title: column.header.clone(),
        width: column.width,
        min_width: column.min_width,
        max_width: column.max_width,
        pinned: column.is_pinned,
        read_only: column.is_read_only,
    }
}
fn columns(
    definitions: &[GridColumnDefinition],
    source: &DataSourceHandle,
) -> Result<Vec<GridControlColumn>, String> {
    if !definitions.is_empty() {
        return Ok(definitions.iter().map(column).collect());
    }
    let source = source.try_borrow().map_err(err)?;
    Ok(source
        .table()
        .schema()
        .fields()
        .iter()
        .map(|f| GridControlColumn {
            read_only: f.read_only,
            ..GridControlColumn::new(&f.name, &f.title, 160.0)
        })
        .collect())
}
fn presentation(definition: &GridControlDefinition) -> GridPresentationSettings {
    GridPresentationSettings {
        view: match definition.view {
            GridViewDefinition::Table => GridViewMode::Table,
            GridViewDefinition::Cards => GridViewMode::Cards,
        },
        group_fields: definition.group_fields.clone(),
        bands: definition
            .bands
            .iter()
            .map(|b| GridBand {
                title: b.title.clone(),
                fields: b.fields.clone(),
            })
            .collect(),
        summaries: definition
            .summaries
            .iter()
            .map(|s| GridSummary {
                title: s.title.clone(),
                aggregate: Aggregate {
                    function: match s.function {
                        GridSummaryFunction::CountRows => AggregateFunction::CountRows,
                        GridSummaryFunction::Count => AggregateFunction::CountValues,
                        GridSummaryFunction::Sum => AggregateFunction::Sum,
                        GridSummaryFunction::Average => AggregateFunction::Average,
                        GridSummaryFunction::Min => AggregateFunction::Minimum,
                        GridSummaryFunction::Max => AggregateFunction::Maximum,
                    },
                    expression: s.field.as_ref().map(QueryExpression::field),
                },
            })
            .collect(),
    }
}
fn detail(
    ui: &Controls,
    definition: &GridControlDefinition,
) -> Result<Option<GridDetailSettings>, String> {
    definition
        .detail
        .as_ref()
        .map(|d| {
            Ok(GridDetailSettings {
                source: ui
                    .data_source(&d.data_source)
                    .ok_or_else(|| format!("Unknown detail source '{}'", d.data_source))?,
                master_field: d.master_field.clone(),
                foreign_field: d.foreign_field.clone(),
                columns: d.columns.iter().map(column).collect(),
            })
        })
        .transpose()
}

impl MountedView {
    pub(super) fn mount_grid_control(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        definition: &GridControlDefinition,
        mut style: LayoutStyle,
        height_is_bound: bool,
    ) -> Result<WidgetId, String> {
        let source = ui
            .data_source(&definition.data_source)
            .ok_or_else(|| format!("Unknown data source '{}'", definition.data_source))?;
        let settings = GridControlSettings {
            columns: columns(&definition.columns, &source)?,
            is_read_only: definition.is_read_only,
        };
        if style.height == Length::Auto
            && style.vertical_alignment == Some(rust_desktop_ui_core::Align::Stretch)
            && !height_is_bound
            && ui
                .runtime
                .tree()
                .node(parent)
                .is_some_and(|node| node.props.style.kind == LayoutKind::Grid)
        {
            style.height = Length::Percent(1.0);
        }
        let id = ui
            .grid_control(parent, source, settings, style)
            .map_err(err)?;
        if !definition.filter_text.is_empty() {
            ui.set_grid_filter(id, &definition.filter_text)
                .map_err(err)?;
        }
        ui.set_grid_presentation(id, presentation(definition))
            .map_err(err)?;
        if let Some(detail) = detail(ui, definition)? {
            ui.set_grid_detail(id, Some(detail)).map_err(err)?;
        }
        for template in &definition.cell_templates {
            ui.set_grid_cell_template(
                id,
                &template.field,
                Some(GridCellTemplate {
                    prefix: template.prefix.clone(),
                    suffix: template.suffix.clone(),
                    icon: template.icon.map(icon_symbol),
                    emphasis: template.emphasis,
                }),
            )
            .map_err(err)?;
        }
        for editor in &definition.editors {
            ui.set_grid_editor_factory(
                id,
                &editor.field,
                &editor.factory,
                &DataFormFactories::default(),
            )
            .map_err(err)?;
        }
        if let Some(key) = &definition.selected_value {
            ui.set_grid_selected_key(id, Some(key.parse::<i64>().map_err(err)?))
                .map_err(err)?;
        }
        Ok(id)
    }
}

#[cfg(test)]
use crate::grid_workspace_model as demo;
#[cfg(test)]
#[path = "grid_workspace_tests.rs"]
mod tests;
