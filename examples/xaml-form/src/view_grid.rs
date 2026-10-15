//! Named typed data sources are registered by the Rust application before mount.
use super::*;
use rust_desktop_ui_controls::{GridControlColumn, GridControlSettings};
use rust_desktop_ui_xaml::GridControlDefinition;

pub(super) fn validate_source(
    ui: &Controls,
    definition: &GridControlDefinition,
) -> Result<(), String> {
    rust_desktop_ui_xaml::validate_grid_control(definition)?;
    let source = ui
        .data_source(&definition.data_source)
        .ok_or_else(|| format!("Unknown data source '{}'", definition.data_source))?;
    let source = source.borrow();
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
    Ok(())
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
            columns: definition
                .columns
                .iter()
                .map(|column| GridControlColumn {
                    field: column.field.clone(),
                    title: column.header.clone(),
                    width: column.width,
                    min_width: column.min_width,
                    max_width: column.max_width,
                    pinned: column.is_pinned,
                    read_only: column.is_read_only,
                })
                .collect(),
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
        if let Some(key) = &definition.selected_value {
            ui.set_grid_selected_key(id, Some(key.parse::<i64>().map_err(err)?))
                .map_err(err)?;
        }
        Ok(id)
    }
}
