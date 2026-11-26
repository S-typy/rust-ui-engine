use super::*;
use std::rc::Rc;

/// Creates an application-defined editor for a named property.
/// Validation has no UI side effects and also receives NULL and mixed values
/// individually. The grid owns validation-before-write and source transactions.
pub trait PropertyCellEditorFactory {
    fn validate(
        &self,
        descriptor: &PropertyDescriptor,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<(), ControlError>;

    /// Create a new row-local editor under `host`. Its focus widget must be
    /// within its owner. Load `value` before returning the instance.
    fn mount(
        &self,
        ui: &mut Controls,
        host: WidgetId,
        descriptor: &PropertyDescriptor,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<Box<dyn PropertyCellEditor>, ControlError>;
}

/// Application-owned registrations for `PropertyGrid::mount_with_editors`.
/// A mounted inspector retains its selected factories. Registering later
/// editors affects subsequent mounts.
#[derive(Default, Clone)]
pub struct PropertyCellEditors {
    factories: HashMap<String, Rc<dyn PropertyCellEditorFactory>>,
}

impl PropertyCellEditors {
    /// Register a unique `Custom.Name` of at most 64 bytes. The suffix starts
    /// with an ASCII letter or underscore and contains ASCII identifier characters.
    pub fn register(
        &mut self,
        name: &str,
        factory: impl PropertyCellEditorFactory + 'static,
    ) -> Result<(), ControlError> {
        let valid = name.strip_prefix("Custom.").is_some_and(|suffix| {
            let mut chars = suffix.bytes();
            chars
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == b'_')
                && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == b'_')
        });
        if !valid || name.len() > 64 {
            return Err(invalid(
                "Property cell editor name requires Custom.<identifier> within 64 bytes",
            ));
        }
        if self.factories.contains_key(name) {
            return Err(invalid(format!(
                "Property cell editor already registered: {name}"
            )));
        }
        self.factories.insert(name.into(), Rc::new(factory));
        Ok(())
    }

    pub(super) fn get(&self, name: &str) -> Option<Rc<dyn PropertyCellEditorFactory>> {
        self.factories.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_data::{CollectionAdapter, DataRow, DataSource, DataTable, FieldType};
    use std::cell::RefCell;

    struct Uppercase {
        allow_null: bool,
    }
    impl PropertyCellEditorFactory for Uppercase {
        fn validate(
            &self,
            _: &PropertyDescriptor,
            f: &FieldSchema,
            v: &CellValue,
        ) -> Result<(), ControlError> {
            if f.kind == FieldType::Text
                && (matches!(v,CellValue::Text(s) if s.chars().all(|c|c.is_ascii_uppercase()))
                    || self.allow_null && *v == CellValue::Null)
            {
                Ok(())
            } else {
                Err(invalid("Expected uppercase ASCII text"))
            }
        }
        fn mount(
            &self,
            ui: &mut Controls,
            host: WidgetId,
            d: &PropertyDescriptor,
            f: &FieldSchema,
            v: &CellValue,
        ) -> Result<Box<dyn PropertyCellEditor>, ControlError> {
            self.validate(d, f, v)?;
            let editor = TextCell(ui.edit_box(host, "Code", "", row_style())?);
            editor.load(ui, v)?;
            Ok(Box::new(editor))
        }
    }
    fn row_style() -> LayoutStyle {
        LayoutStyle {
            height: Length::Px(28.),
            ..Default::default()
        }
    }
    struct TextCell(WidgetId);
    impl PropertyCellEditor for TextCell {
        fn owner(&self) -> WidgetId {
            self.0
        }
        fn focus(&self) -> WidgetId {
            self.0
        }
        fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
            let text = match v {
                CellValue::Text(s) => s.as_str(),
                CellValue::Null => "",
                _ => return Err(invalid("Expected text")),
            };
            ui.set_editor_value(self.0, text)?;
            Ok(())
        }
        fn read(&self, ui: &Controls) -> Result<CellValue, String> {
            Ok(CellValue::Text(
                ui.document(self.0).ok_or("removed")?.text().into(),
            ))
        }
    }

    #[test]
    fn registered_editor_keeps_its_factory_validates_mixed_inputs_before_mount_and_writes_only_on_apply()
     {
        let mut field = FieldSchema::new("code", FieldType::Text);
        field.nullable = true;
        let table = DataTable::new(
            TableSchema::new(vec![field]).unwrap(),
            vec![
                DataRow::new(1, vec![CellValue::Text("ONE".into())]),
                DataRow::new(2, vec![CellValue::Null]),
            ],
        )
        .unwrap();
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table)).unwrap(),
        ));
        let mut d = PropertyDescriptor::new("code");
        d.metadata.editor = "Custom.Code".into();
        let settings = PropertyGridSettings {
            properties: vec![d],
            ..Default::default()
        };
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let n = ui.runtime.tree().len();
        let mut registry = PropertyCellEditors::default();
        for name in ["Slider", "Custom.", "Custom.two.parts", "Custom.1A"] {
            assert!(
                registry
                    .register(name, Uppercase { allow_null: true })
                    .is_err()
            );
        }
        registry
            .register("Custom.Code", Uppercase { allow_null: false })
            .unwrap();
        assert!(
            registry
                .register("Custom.Code", Uppercase { allow_null: true })
                .is_err()
        );
        let factories = DataFormFactories::default();
        assert!(
            PropertyGrid::mount_with_editors(
                &mut ui,
                root,
                source.clone(),
                &[1, 2],
                settings.clone(),
                &factories,
                &registry,
                LayoutStyle::default()
            )
            .is_err()
        );
        assert_eq!(ui.runtime.tree().len(), n);
        let mut allowed = PropertyCellEditors::default();
        allowed
            .register("Custom.Code", Uppercase { allow_null: true })
            .unwrap();
        let mut grid = PropertyGrid::mount_with_editors(
            &mut ui,
            root,
            source.clone(),
            &[1, 2],
            settings,
            &factories,
            &allowed,
            LayoutStyle::default(),
        )
        .unwrap();
        drop(allowed);
        grid.begin_edit(&mut ui, "code").unwrap();
        let editor = grid.property_editor("code").unwrap();
        ui.set_text(editor, "PREPARED").unwrap();
        assert_eq!(
            source.borrow().value(1, "code"),
            Some(&CellValue::Text("ONE".into()))
        );
        grid.cancel(&mut ui, "code").unwrap();
        assert_eq!(source.borrow().value(2, "code"), Some(&CellValue::Null));
        grid.begin_edit(&mut ui, "code").unwrap();
        ui.set_text(editor, "invalid").unwrap();
        assert!(matches!(
            grid.commit(&mut ui, "code").unwrap(),
            PropertyCommit::Rejected(_)
        ));
        ui.set_text(editor, "ACCEPTED").unwrap();
        assert_eq!(grid.commit(&mut ui, "code").unwrap(), PropertyCommit::Saved);
        for key in [1, 2] {
            assert_eq!(
                source.borrow().value(key, "code"),
                Some(&CellValue::Text("ACCEPTED".into()))
            )
        }
    }
}
