//! Named typed data sources are registered by the Rust application before mount.
use super::*;
use rust_desktop_ui_controls::{
    DataFormFactories, DataSourceHandle, GridBand, GridCellTemplate, GridControlColumn,
    GridControlSettings, GridDetailSettings, GridPresentationSettings, GridSummary, GridViewMode,
};
use rust_desktop_ui_data::{Aggregate, AggregateFunction, QueryExpression};
use rust_desktop_ui_xaml::GridControlDefinition;
use rust_desktop_ui_xaml::{GridColumnDefinition, GridSummaryFunction, GridViewDefinition};
use std::rc::Rc;

fn choice_editor(
    ui: &Controls,
    def: &rust_desktop_ui_xaml::GridEditorDefinition,
) -> Result<Option<rust_desktop_ui_controls::GridChoiceEditor>, String> {
    def.choice
        .as_ref()
        .map(|(source, key, fields)| {
            Ok(rust_desktop_ui_controls::GridChoiceEditor {
                source: ui
                    .data_source(source)
                    .ok_or_else(|| format!("Unknown choice source '{source}'"))?,
                value_field: key.clone(),
                display_fields: fields.clone(),
                searchable: def.factory == "Lookup",
            })
        })
        .transpose()
}

fn custom_editor(
    ui: &Controls,
    def: &rust_desktop_ui_xaml::GridEditorDefinition,
    field: &rust_desktop_ui_data::FieldSchema,
) -> Result<Option<Rc<dyn rust_desktop_ui_controls::GridCellEditor>>, String> {
    use rust_desktop_ui_controls::{
        EditorButton, EditorButtonAction, GridTextCellEditor, MaskSettings,
    };
    if let Some(choice) = choice_editor(ui, def)? {
        choice.validate(field).map_err(err)?;
        return Ok(Some(Rc::new(choice)));
    }
    if !matches!(def.factory.as_str(), "Memo" | "Masked" | "Button") {
        return Ok(None);
    }
    if field.kind != rust_desktop_ui_data::FieldType::Text {
        return Err("Text editor requires a text field".into());
    }
    let recipe = match def.factory.as_str() {
        "Masked" => GridTextCellEditor::Masked(
            MaskSettings::new(def.mask.as_deref().ok_or("Mask required")?).map_err(err)?,
        ),
        "Memo" => GridTextCellEditor::Multiline,
        _ => {
            let (label, action) = if let Some(name) = &def.command {
                let cmd = rust_desktop_ui_controls::CommandId::from(name.as_str());
                (
                    ui.commands()
                        .label(&cmd)
                        .ok_or("Unknown GridEditor command")?
                        .to_owned(),
                    EditorButtonAction::Command(cmd),
                )
            } else {
                ("Очистить".into(), EditorButtonAction::Clear)
            };
            GridTextCellEditor::Buttons(vec![EditorButton {
                label,
                icon: None,
                action,
            }])
        }
    };
    Ok(Some(Rc::new(recipe)))
}
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
    let mut query = source.query().clone();
    drop(source);
    let columns = columns(ui, &definition.columns, &source_handle)?;
    let detail = detail(ui, definition)?;
    let filter = (!definition.filter_text.is_empty()).then_some(definition.filter_text.as_str());
    let table = presentation(definition)
        .preview(&source_handle, &columns, detail.as_ref(), filter)
        .map_err(err)?;
    for (_, detail) in &definition.relations {
        let child = ui
            .data_source(&detail.data_source)
            .ok_or("Missing relation source")?;
        let child = child.try_borrow().map_err(err)?;
        let master = table
            .schema()
            .field(&detail.master_field)
            .ok_or("Missing master field")?;
        let foreign = child
            .table()
            .schema()
            .field(&detail.foreign_field)
            .ok_or("Missing foreign field")?;
        if master.kind != foreign.kind
            || detail
                .columns
                .iter()
                .any(|c| child.table().schema().field(&c.field).is_none())
        {
            return Err("Invalid relation columns or link types".into());
        }
    }
    if !definition.criteria.is_empty() {
        rust_desktop_ui_data::parse_criteria_text(table.schema(), &definition.criteria)
            .map_err(err)?;
    }
    if let Some(field) = &definition.parent_field {
        if !table
            .schema()
            .field(field)
            .is_some_and(|f| f.nullable && f.kind == rust_desktop_ui_data::FieldType::Integer)
        {
            return Err("ParentField requires a nullable integer".into());
        }
        if !definition.group_fields.is_empty()
            || definition.detail.is_some()
            || !definition.summaries.is_empty()
        {
            return Err(
                "ParentField requires ungrouped records without inline details or summaries".into(),
            );
        }
    }
    if let Some(key) = &definition.selected_value {
        let key = key.parse::<i64>().map_err(err)?;
        if let Some(filter) = filter {
            query.filter = filter.into();
        }
        if table.row(key).is_none()
            || filter.is_some() && !query.row_keys(&table).map_err(err)?.contains(&key)
        {
            return Err(format!(
                "Selected grid row '{key}' is absent from the initial query"
            ));
        }
    }
    for editor in &definition.editors {
        let field = table
            .schema()
            .field(&editor.field)
            .ok_or_else(|| format!("Unknown editor field '{}'", editor.field))?;
        if let Some((name, value)) = &editor.when {
            table
                .schema()
                .field(name)
                .ok_or("Unknown editor condition field")?
                .parse(value)
                .map_err(err)?;
        }
        if custom_editor(ui, editor, field)?.is_none() {
            DataFormFactories::default()
                .resolve(&editor.factory, field)
                .map_err(err)?;
        }
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
        expression: column.expression.clone(),
        unbound: None,
        title: column.header.clone(),
        width: column.width,
        min_width: column.min_width,
        max_width: column.max_width,
        pinned: column.is_pinned,
        read_only: column.is_read_only,
    }
}
fn columns(
    ui: &Controls,
    definitions: &[GridColumnDefinition],
    source: &DataSourceHandle,
) -> Result<Vec<GridControlColumn>, String> {
    if !definitions.is_empty() {
        return definitions
            .iter()
            .map(|c| {
                let mut out = column(c);
                if let Some((source, member)) = c.value_source.as_ref().zip(c.value_member.as_ref())
                {
                    out.unbound = Some(rust_desktop_ui_controls::GridUnboundColumn {
                        source: ui
                            .data_source(source)
                            .ok_or_else(|| format!("Unknown unbound source '{source}'"))?,
                        member: member.clone(),
                    });
                }
                Ok(out)
            })
            .collect();
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
fn band(b: &rust_desktop_ui_xaml::GridBandDefinition) -> GridBand {
    GridBand {
        title: b.title.clone(),
        fields: b.fields.clone(),
    }
}
fn presentation(definition: &GridControlDefinition) -> GridPresentationSettings {
    GridPresentationSettings {
        view: match definition.view {
            GridViewDefinition::Table => GridViewMode::Table,
            GridViewDefinition::Cards => GridViewMode::Cards,
        },
        hierarchical_groups: definition.hierarchical_groups,
        group_sort_summary: definition.group_sort_summary,
        group_descending: definition.group_descending,
        group_footer: definition.group_footer,
        show_group_panel: definition.show_group_panel,
        stacked_fields: definition.stacked_fields.clone(),
        right_pinned_fields: definition.right_pinned_fields.clone(),
        group_fields: definition.group_fields.clone(),
        group_expressions: definition
            .columns
            .iter()
            .filter_map(|c| Some((c.field.clone(), c.group_expression.clone()?)))
            .collect(),
        bands: definition.bands.iter().map(band).collect(),
        band_rows: definition
            .band_rows
            .iter()
            .map(|row| row.iter().map(band).collect())
            .collect(),
        custom_summaries: vec![],
        summary_columns: definition
            .summaries
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((i, s.display_column.clone()?)))
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
            columns: columns(ui, &definition.columns, &source)?,
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
        ui.set_grid_workflow_rows(
            id,
            definition.show_auto_filter_row,
            definition.show_row_selector,
        )
        .map_err(err)?;
        if definition.criteria_disabled {
            ui.set_grid_criteria_enabled(id, false).map_err(err)?;
        }
        if !definition.criteria.is_empty() {
            ui.set_grid_criteria(id, &definition.criteria)
                .map_err(err)?;
        }
        if let Some(parent) = &definition.parent_field {
            ui.set_grid_parent_field(id, Some(parent)).map_err(err)?;
        }
        if !definition.filter_text.is_empty() {
            ui.set_grid_filter(id, &definition.filter_text)
                .map_err(err)?;
        }
        ui.set_grid_presentation(id, presentation(definition))
            .map_err(err)?;
        for (name, relation) in &definition.relations {
            let source = ui
                .data_source(&relation.data_source)
                .ok_or("Missing relation source")?;
            ui.register_grid_relation(
                id,
                rust_desktop_ui_controls::GridRelation {
                    name: name.clone(),
                    settings: GridControlSettings {
                        columns: columns(ui, &relation.columns, &source)?,
                        is_read_only: definition.is_read_only,
                    },
                    source,
                    master_field: relation.master_field.clone(),
                    foreign_field: relation.foreign_field.clone(),
                },
            )
            .map_err(err)?;
        }
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
            let schema = ui
                .grid_field_schema(id, &editor.field)
                .ok_or("Missing editor field")?
                .clone();
            if let Some(recipe) = custom_editor(ui, editor, &schema)? {
                if let Some((name, value)) = &editor.when {
                    let typed = ui
                        .grid_field_schema(id, name)
                        .ok_or("Unknown editor condition field")?
                        .parse(value)
                        .map_err(err)?;
                    ui.set_grid_conditional_cell_editor(id, &editor.field, name, typed, recipe)
                        .map_err(err)?;
                } else {
                    ui.set_grid_cell_editor(id, &editor.field, recipe)
                        .map_err(err)?;
                }
                continue;
            }
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
