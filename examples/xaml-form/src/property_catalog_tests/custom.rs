use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, ControlError, Controls, DataValue, PropertyCellEditor,
    PropertyCellEditorFactory, PropertyCellEditors, PropertyCommit, PropertyDescriptor, Theme,
};
use rust_desktop_ui_core::{Size, WidgetId};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    TableSchema,
};
use std::{cell::RefCell, rc::Rc};

const MARKUP: &str = r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
    <PropertyGrid x:Name="Inspector" DataSource="Jobs" ObjectKeys="1">
        <PropertyGrid.Properties>
            <Property FieldName="weight" Label="Выполнено" Editor="Custom.Completion"/>
        </PropertyGrid.Properties>
    </PropertyGrid>
</Window>"#;

/// The application's protocol stores completion as the integer weights 0 and
/// 100. Its adapter presents that domain with an ordinary retained checkbox.
struct Completion;
impl PropertyCellEditorFactory for Completion {
    fn validate(
        &self,
        _: &PropertyDescriptor,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<(), ControlError> {
        if field.kind == FieldType::Integer && matches!(value, CellValue::Integer(0 | 100)) {
            Ok(())
        } else {
            Err(ControlError::Invalid(
                "Completion weight requires 0 or 100".into(),
            ))
        }
    }
    fn mount(
        &self,
        ui: &mut Controls,
        host: WidgetId,
        def: &PropertyDescriptor,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<Box<dyn PropertyCellEditor>, ControlError> {
        self.validate(def, field, value)?;
        let label = def.metadata.label.as_deref().unwrap_or(&field.title);
        let id = ui.checkbox(
            host,
            label,
            *value == CellValue::Integer(100),
            Default::default(),
        )?;
        Ok(Box::new(CompletionCell(id)))
    }
}

struct CompletionCell(WidgetId);
impl PropertyCellEditor for CompletionCell {
    fn owner(&self) -> WidgetId {
        self.0
    }
    fn focus(&self) -> WidgetId {
        self.0
    }
    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        ui.set_checked(self.0, *value == CellValue::Integer(100))
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let checked = ui
            .checked(self.0)
            .ok_or("Completion checkbox was removed")?;
        Ok(CellValue::Integer(if checked { 100 } else { 0 }))
    }
}

#[test]
fn custom_editor_in_markup_resolves_the_application_widget_and_commits_an_integer() {
    let window = rust_desktop_ui_xaml::parse(MARKUP).unwrap();
    assert!(
        rust_desktop_ui_xaml::compile(MARKUP)
            .unwrap()
            .contains("Custom.Completion")
    );
    assert!(
        rust_desktop_ui_xaml::parse(&MARKUP.replace("Custom.Completion", "Custom.4bad")).is_err()
    );
    let table = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("weight", FieldType::Integer)]).unwrap(),
        vec![DataRow::new(1, vec![CellValue::Integer(0)])],
    )
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    ui.register_data_source("Jobs", source.clone()).unwrap();
    let before = ui.runtime.tree().len();
    assert!(MountedView::mount(&mut ui, root, &window.content).is_err());
    assert_eq!(ui.runtime.tree().len(), before);
    let mut registry = PropertyCellEditors::default();
    registry.register("Custom.Completion", Completion).unwrap();
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::Object(Default::default())).unwrap(),
    ));
    let mut view = MountedView::mount_with_registries(
        &mut ui,
        root,
        &window.content,
        context,
        Default::default(),
        registry,
    )
    .unwrap();
    ui.update(Size::new(600., 300.)).unwrap();
    let grid = view
        .property_grids
        .get_mut(&view.names["Inspector"])
        .unwrap();
    grid.begin_edit(&mut ui, "weight").unwrap();
    let checkbox = grid.property_editor("weight").unwrap();
    assert_eq!(ui.checked(checkbox), Some(false));
    ui.set_checked(checkbox, true).unwrap();
    assert_eq!(
        grid.draft(&ui, "weight").unwrap(),
        Some(CellValue::Integer(100))
    );
    assert_eq!(
        source.borrow().value(1, "weight"),
        Some(&CellValue::Integer(0))
    );
    assert_eq!(
        grid.commit(&mut ui, "weight").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(
        source.borrow().value(1, "weight"),
        Some(&CellValue::Integer(100))
    );
}
