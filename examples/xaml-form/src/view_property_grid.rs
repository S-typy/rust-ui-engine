//! Retained property inspector over explicitly registered typed objects.
use super::*;
use rust_desktop_ui_controls::{
    DataFormFactories, DataFormField, PropertyDescriptor, PropertyGrid, PropertyGridSettings,
};
use rust_desktop_ui_data::{CellValue, FieldType};
use rust_desktop_ui_xaml::PropertyGridDefinition;

fn settings(ui: &Controls, value: &PropertyGridDefinition) -> Result<PropertyGridSettings, String> {
    let source = ui
        .data_source(&value.data_source)
        .ok_or_else(|| format!("Unknown source '{}'", value.data_source))?;
    let source = source.try_borrow().map_err(err)?;
    if value
        .object_keys
        .iter()
        .any(|key| source.row(*key).is_none())
    {
        return Err("PropertyGrid names an unknown object key".into());
    }
    let properties = value
        .properties
        .iter()
        .map(|p| {
            let field = source
                .table()
                .schema()
                .field(&p.metadata.field)
                .ok_or_else(|| format!("Unknown property '{}'", p.metadata.field))?;
            let default = if p.default_is_null {
                Some(CellValue::Null)
            } else if let Some(raw) = &p.default_value {
                let v = match field.kind {
                    FieldType::Date => CellValue::Date(raw.parse().map_err(err)?),
                    FieldType::Bool => CellValue::Bool(raw.parse::<bool>().map_err(err)?),
                    _ => field.parse(raw).map_err(err)?,
                };
                // Static values use exact canonical storage spelling; Text is literal.
                let canonical = match &v {
                    CellValue::Date(value) => value.to_string(),
                    CellValue::Time(value) => value.to_string(),
                    CellValue::Bool(value) => value.to_string(),
                    _ => v.display(),
                };
                if v == CellValue::Null || (field.kind != FieldType::Text && canonical != *raw) {
                    return Err(format!(
                        "DefaultValue for '{}' must use canonical storage text",
                        field.name
                    ));
                }
                Some(v)
            } else {
                None
            };
            if let Some(v) = &default {
                field.validate(v).map_err(err)?;
            }
            Ok(PropertyDescriptor {
                metadata: DataFormField {
                    field: p.metadata.field.clone(),
                    label: p.metadata.label.clone(),
                    visible: p.metadata.visible,
                    read_only: p.metadata.is_read_only,
                    editor: p.metadata.editor.clone(),
                },
                category: p.category.clone(),
                groups: p.groups.clone(),
                default,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PropertyGridSettings {
        properties,
        label_width: value.label_width,
        read_only: value.is_read_only,
    })
}
pub(super) fn validate_source(
    ui: &Controls,
    value: &PropertyGridDefinition,
    common: &rust_desktop_ui_xaml::CommonProperties,
) -> Result<(), String> {
    rust_desktop_ui_xaml::validate_property_grid(value, common)?;
    settings(ui, value)?;
    Ok(())
}
impl MountedView {
    pub(super) fn mount_property_grid(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &PropertyGridDefinition,
        style: LayoutStyle,
    ) -> Result<WidgetId, String> {
        let settings = settings(ui, value)?;
        let source = ui
            .data_source(&value.data_source)
            .ok_or("Property source is not registered")?;
        let grid = PropertyGrid::mount(
            ui,
            parent,
            source,
            &value.object_keys,
            settings,
            &DataFormFactories::default(),
            style,
        )
        .map_err(err)?;
        let id = grid.container;
        self.property_grids.insert(id, grid);
        Ok(id)
    }
    pub(crate) fn property_grid_input(
        &mut self,
        ui: &mut Controls,
        event: &rust_desktop_ui_core::InputEvent,
    ) -> Result<bool, String> {
        for grid in self.property_grids.values_mut() {
            if grid.input(ui, event).map_err(err)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn sync_property_grids(&mut self, ui: &mut Controls) -> bool {
        self.property_grids
            .retain(|id, _| ui.runtime.tree().contains(*id));
        let mut changed = false;
        for grid in self.property_grids.values_mut() {
            match grid.update(ui) {
                Ok(value) => changed |= value,
                Err(error) => eprintln!("PropertyGrid update: {error}"),
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{PropertyCommit, PropertyValue, Theme};
    use rust_desktop_ui_core::{InputEvent, Key, Mutation, Size};
    use rust_desktop_ui_data::{
        CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, TableSchema,
    };
    use std::{cell::RefCell, rc::Rc};
    fn source() -> rust_desktop_ui_controls::DataSourceHandle {
        Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(
                DataTable::new(
                    TableSchema::new(vec![FieldSchema::new("count", FieldType::Integer)]).unwrap(),
                    vec![
                        DataRow::new(1, vec![CellValue::Integer(i64::MIN)]),
                        DataRow::new(2, vec![CellValue::Integer(i64::MAX)]),
                    ],
                )
                .unwrap(),
            ))
            .unwrap(),
        ))
    }
    #[test]
    fn compiled_inspector_mixed_exact_values_and_keyboard_cancel_retain_ids() {
        let window=rust_desktop_ui_xaml::parse(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><PropertyGrid x:Name="Inspector" DataSource="Objects" ObjectKeys="1,2"><PropertyGrid.Properties><Property FieldName="count" Category="Layout" Group="Dimensions" DefaultValue="9223372036854775807"/></PropertyGrid.Properties></PropertyGrid></Window>"#).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let source = source();
        ui.register_data_source("Objects", source.clone()).unwrap();
        let root = ui.root();
        let mut view = MountedView::mount(&mut ui, root, &window.content).unwrap();
        ui.update(Size::new(960.0, 600.0)).unwrap();
        let owner = view.names["Inspector"];
        let grid = view.property_grids.get_mut(&owner).unwrap();
        let editor = grid.property_editor("count").unwrap();
        assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
        grid.begin_edit(&mut ui, "count").unwrap();
        ui.set_text(editor, "7").unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        assert!(
            view.property_grid_input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Escape,
                    modifiers: Default::default(),
                    repeat: false
                }
            )
            .unwrap()
        );
        let grid = view.property_grids.get_mut(&owner).unwrap();
        assert_eq!(grid.draft(&ui, "count").unwrap(), None);
        assert_eq!(grid.reset(&mut ui, "count").unwrap(), PropertyCommit::Saved);
        assert_eq!(grid.property_editor("count"), Some(editor));
        assert_eq!(
            source.borrow().value(1, "count"),
            Some(&CellValue::Integer(i64::MAX))
        );
    }
    #[test]
    fn incompatible_metadata_or_unknown_identity_rolls_back_whole_mount() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.register_data_source("Objects", source()).unwrap();
        let root = ui.root();
        let before = ui.runtime.tree().preorder();
        for body in [
            r#"<PropertyGrid DataSource="Objects" ObjectKeys="1,3"/>"#,
            r#"<PropertyGrid DataSource="Objects" ObjectKeys="1,2"><PropertyGrid.Properties><Property FieldName="count" DefaultValue="1.5"/></PropertyGrid.Properties></PropertyGrid>"#,
        ] {
            let window = rust_desktop_ui_xaml::parse(&format!(
                "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
            ))
            .unwrap();
            assert!(MountedView::mount(&mut ui, root, &window.content).is_err());
            assert_eq!(ui.runtime.tree().preorder(), before);
        }
    }

    #[test]
    fn property_defaults_use_storage_dates_and_booleans_instead_of_display_text() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(
                DataTable::new(
                    TableSchema::new(vec![
                        FieldSchema::new("date", FieldType::Date),
                        FieldSchema::new("enabled", FieldType::Bool),
                    ])
                    .unwrap(),
                    vec![DataRow::new(
                        1,
                        vec![
                            CellValue::Date("2026-10-17".parse().unwrap()),
                            CellValue::Bool(false),
                        ],
                    )],
                )
                .unwrap(),
            ))
            .unwrap(),
        ));
        ui.register_data_source("Objects", source.clone()).unwrap();
        let root = ui.root();
        let xml = r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><PropertyGrid x:Name="Inspector" DataSource="Objects" ObjectKeys="1"><PropertyGrid.Properties><Property FieldName="date" DefaultValue="2026-01-01"/><Property FieldName="enabled" DefaultValue="true"/></PropertyGrid.Properties></PropertyGrid></Window>"#;
        let window = rust_desktop_ui_xaml::parse(xml).unwrap();
        let mut view = MountedView::mount(&mut ui, root, &window.content).unwrap();
        let grid = view
            .property_grids
            .get_mut(&view.names["Inspector"])
            .unwrap();
        grid.reset(&mut ui, "date").unwrap();
        grid.reset(&mut ui, "enabled").unwrap();
        assert_eq!(
            source.borrow().value(1, "date"),
            Some(&CellValue::Date("2026-01-01".parse().unwrap()))
        );
        assert_eq!(
            source.borrow().value(1, "enabled"),
            Some(&CellValue::Bool(true))
        );
        for invalid in [
            xml.replace("2026-01-01", "01.01.2026"),
            xml.replace("true", "Да"),
        ] {
            let window = rust_desktop_ui_xaml::parse(&invalid).unwrap();
            assert!(MountedView::mount(&mut ui, root, &window.content).is_err());
        }
    }

    #[test]
    fn property_demo_allocates_editor_width_and_draws_mixed_captions_in_real_form() {
        let definition =
            rust_desktop_ui_xaml::parse(include_str!("../PropertyWindow.xaml")).unwrap();
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let mut form =
                crate::form::Form::new_with_model(definition.clone(), false, None).unwrap();
            form.ui.set_theme(theme).unwrap();
            let owner = form.view.find_name("DocumentProperties").unwrap();
            let editor = form.view.find_name("PropertyNameEditor").unwrap();
            for width in [1180.0, 1000.0] {
                form.ui.update(Size::new(width, 880.0)).unwrap();
                let grid = &form.view.property_grids[&owner];
                for field in ["name", "enabled", "identifier", "count"] {
                    let row = form
                        .ui
                        .runtime
                        .bounds(grid.property_row(field).unwrap())
                        .unwrap();
                    let run = form
                        .ui
                        .scene()
                        .texts
                        .iter()
                        .find(|run| {
                            run.text == "Разные значения" && run.bounds.intersection(row).is_some()
                        })
                        .unwrap_or_else(|| panic!("Missing mixed caption for {field} at {width}"));
                    let visible = run.bounds.intersection(run.clip).unwrap();
                    assert!(
                        visible.width >= 100.0 && visible.height >= 16.0,
                        "Mixed caption collapsed for {field}: {visible:?}"
                    );
                    assert!(
                        visible.width + 0.1 >= run.bounds.width
                            && visible.height + 0.1 >= run.bounds.height,
                        "Mixed caption clipped for {field}"
                    );
                }
                for field in ["price", "created"] {
                    let editor = grid.property_editor(field).unwrap();
                    let rect = form.ui.runtime.bounds(editor).unwrap();
                    assert!(rect.width >= 200.0, "Narrow {field} editor: {rect:?}");
                    let text = form.ui.document(editor).unwrap().text();
                    assert!(
                        form.ui
                            .scene()
                            .texts
                            .iter()
                            .any(|run| run.text == text && run.bounds.intersection(rect).is_some()),
                        "Missing {field} display"
                    );
                }
            }
            form.view
                .property_grids
                .get_mut(&owner)
                .unwrap()
                .begin_edit(&mut form.ui, "name")
                .unwrap();
            form.ui.set_text(editor, "Локальный черновик").unwrap();
            for width in [1180.0, 1000.0, 1180.0] {
                form.ui.update(Size::new(width, 880.0)).unwrap();
                assert_eq!(
                    form.view.property_grids[&owner].property_editor("name"),
                    Some(editor)
                );
                assert_eq!(
                    form.ui.document(editor).unwrap().text(),
                    "Локальный черновик"
                );
                let null = form.view.property_grids[&owner]
                    .property_null_toggle("name")
                    .unwrap();
                let null_bounds = form.ui.runtime.bounds(null).unwrap();
                let null_run = form
                    .ui
                    .scene()
                    .texts
                    .iter()
                    .find(|run| {
                        run.text == "NULL" && run.bounds.intersection(null_bounds).is_some()
                    })
                    .expect("Nullable control must draw its complete caption")
                    .clone();
                let natural = form
                    .ui
                    .measure_text_with_font_size(&null_run.text, 4096.0, false, null_run.font_size)
                    .unwrap();
                let visible = null_run.bounds.intersection(null_run.clip).unwrap();
                assert!(
                    natural.width <= visible.width + 0.1,
                    "NULL caption clipped: required={natural:?}, visible={visible:?}, width={width}, theme={theme:?}"
                );
                let rect = form.ui.runtime.bounds(editor).unwrap();
                assert!(rect.width >= 200.0, "Editing width collapsed: {rect:?}");
                let run = form
                    .ui
                    .scene()
                    .texts
                    .iter()
                    .find(|run| run.text == "Локальный черновик")
                    .expect("Editable draft must have a real text run");
                let visible = run.bounds.intersection(run.clip).unwrap();
                assert!(
                    visible.width + 0.1 >= run.bounds.width
                        && visible.height + 0.1 >= run.bounds.height,
                    "Draft clipped: {run:?}"
                );
            }
        }
    }
}
