//! Mount a retained metadata form over an explicitly registered source.
use super::*;
use rust_desktop_ui_controls::{DataForm, DataFormFactories, DataFormField, DataFormSettings};
use rust_desktop_ui_xaml::DataFormDefinition;

pub(super) fn validate_source(
    ui: &Controls,
    definition: &DataFormDefinition,
) -> Result<(), String> {
    rust_desktop_ui_xaml::validate_data_form(definition)?;
    let source = ui
        .data_source(&definition.data_source)
        .ok_or_else(|| format!("Unknown data source '{}'", definition.data_source))?;
    let source = source.try_borrow().map_err(err)?;
    if source.row(definition.row_key).is_none() {
        return Err(format!("Unknown form row {}", definition.row_key));
    }
    for field in &definition.fields {
        if source.table().schema().field(&field.field).is_none() {
            return Err(format!("Unknown form field '{}'", field.field));
        }
    }
    Ok(())
}
impl MountedView {
    pub(super) fn mount_data_form(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        definition: &DataFormDefinition,
        style: LayoutStyle,
    ) -> Result<WidgetId, String> {
        let source = ui
            .data_source(&definition.data_source)
            .ok_or_else(|| format!("Unknown data source '{}'", definition.data_source))?;
        let settings = DataFormSettings {
            fields: definition
                .fields
                .iter()
                .map(|field| DataFormField {
                    field: field.field.clone(),
                    label: field.label.clone(),
                    visible: field.visible,
                    read_only: field.is_read_only,
                    editor: field.editor.clone(),
                })
                .collect(),
            label_width: definition.label_width,
            row_gap: definition.row_gap,
            read_only: definition.is_read_only,
        };
        let form = DataForm::mount(
            ui,
            parent,
            source,
            definition.row_key,
            settings,
            &DataFormFactories::default(),
            style,
        )
        .map_err(err)?;
        let id = form.container;
        self.data_forms.insert(id, form);
        Ok(id)
    }
}
