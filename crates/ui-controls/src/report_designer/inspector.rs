use super::*;
use crate::{
    ControlError, Controls, DataFormFactories, PropertyCommit, PropertyDescriptor, PropertyGrid,
    PropertyGridSettings, composition_ui::error,
};
use std::{cell::RefCell, rc::Rc};

/// The property source is a working copy. Only Apply publishes to the report.
pub(super) struct Inspector {
    pub grid: PropertyGrid,
    source: Rc<RefCell<DataSource>>,
    fields: Vec<String>,
    revision: ReportRevision,
    selection: Option<ReportSelection>,
}
impl Inspector {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        session: &ReportDesignerSession,
    ) -> Result<Self, ControlError> {
        let d = session.definition();
        let mut schema = Vec::new();
        let mut values = Vec::new();
        let mut descriptors = Vec::new();
        let mut add = |name: &str, title: &str, category: &str, kind, value| {
            schema.push(FieldSchema::new(name, kind));
            values.push(value);
            let mut p = PropertyDescriptor::new(name);
            p.category = category.into();
            p.metadata.label = Some(title.into());
            descriptors.push(p);
        };
        add(
            "title",
            "Название",
            "Страница",
            FieldType::Text,
            CellValue::Text(d.title.clone()),
        );
        for (name, title, value) in [
            ("width", "Ширина страницы", d.page_size.width),
            ("height", "Высота страницы", d.page_size.height),
            ("top", "Верхнее поле", d.margins[0]),
            ("right", "Правое поле", d.margins[1]),
            ("bottom", "Нижнее поле", d.margins[2]),
            ("left", "Левое поле", d.margins[3]),
        ] {
            add(
                name,
                title,
                "Страница",
                FieldType::Real,
                CellValue::Real(value as f64),
            );
        }
        for (i, band) in ReportBandKind::ALL.into_iter().enumerate() {
            add(
                &format!("band{i}"),
                band.title(),
                "Секции",
                FieldType::Real,
                CellValue::Real(band.band(d).height as f64),
            );
        }
        if let Some(s) = session.selection() {
            let item = &s.band.band(d).items[s.item];
            if let ReportItem::Text { bold, .. } = item {
                add(
                    "bold",
                    "Полужирный",
                    "Элемент",
                    FieldType::Bool,
                    CellValue::Bool(*bold),
                );
            }
            if let ReportItem::Text { color, .. } | ReportItem::Rectangle { color, .. } = item {
                for (name, title, value) in [
                    ("red", "Красный", color.r),
                    ("green", "Зелёный", color.g),
                    ("blue", "Синий", color.b),
                    ("alpha", "Непрозрачность", color.a),
                ] {
                    add(
                        name,
                        title,
                        "Элемент",
                        FieldType::Real,
                        CellValue::Real(value as f64),
                    );
                }
            }
        }
        // Reuse typed editors and explicit NULL toggles from the shared inspector.
        for (i, p) in d.parameters.iter().enumerate() {
            add(
                &format!("parameter{i}"),
                &format!("{} ({:?})", p.name, p.kind),
                "Параметры",
                p.kind,
                session
                    .parameters()
                    .get(&p.name)
                    .cloned()
                    .unwrap_or(CellValue::Null),
            );
        }
        for (name, title, value) in [
            ("new_name", "Имя нового параметра", ""),
            ("new_type", "Тип нового параметра", "Text"),
            ("new_value", "Начальное значение", ""),
            ("remove_name", "Удалить параметр по имени", ""),
        ] {
            add(
                name,
                title,
                "Параметры",
                FieldType::Text,
                CellValue::Text(value.into()),
            );
        }
        add(
            "new_nullable",
            "Допускает NULL",
            "Параметры",
            FieldType::Bool,
            CellValue::Bool(false),
        );
        for (i, p) in d.parameters.iter().enumerate() {
            let field = schema
                .iter_mut()
                .find(|f| f.name == format!("parameter{i}"))
                .unwrap();
            field.nullable = p.nullable;
            field.read_only = p.read_only;
        }
        let fields = schema
            .iter()
            .filter(|f| !f.read_only)
            .map(|f| f.name.clone())
            .collect();
        let table = DataTable::new(
            TableSchema::new(schema).map_err(error)?,
            vec![DataRow::new(1, values)],
        )
        .map_err(error)?;
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table)).map_err(error)?,
        ));
        let grid = PropertyGrid::mount(
            ui,
            parent,
            source.clone(),
            &[1],
            PropertyGridSettings {
                properties: descriptors,
                label_width: 210.0,
                ..Default::default()
            },
            &DataFormFactories::default(),
            LayoutStyle {
                width: Length::Percent(1.0),
                ..Default::default()
            },
        )?;
        Ok(Self {
            grid,
            source,
            fields,
            revision: session.revision(),
            selection: session.selection(),
        })
    }
    pub fn apply(
        &mut self,
        ui: &mut Controls,
        session: &mut ReportDesignerSession,
    ) -> Result<(), ControlError> {
        if session.revision() != self.revision || session.selection() != self.selection {
            return Err(error(
                "Report changed while properties were being edited; cancel and reopen settings",
            ));
        }
        for name in &self.fields {
            match self.grid.commit(ui, name)? {
                PropertyCommit::Pending => {
                    return Err(error("Finish text composition before applying settings"));
                }
                PropertyCommit::Rejected(e) => return Err(error(e)),
                _ => {}
            }
        }
        self.grid.take_changes();
        let source = self.source.borrow();
        let table = source.table();
        let value = |name: &str| {
            table
                .value(1, name)
                .ok_or_else(|| error("Missing inspector property"))
        };
        let text = |name: &str| match value(name)? {
            CellValue::Text(s) => Ok(s.clone()),
            _ => Err(error("Expected text")),
        };
        let number = |name: &str| match value(name)? {
            CellValue::Real(v) if v.is_finite() && (*v as f32).is_finite() => Ok(*v as f32),
            _ => Err(error("Expected finite number")),
        };
        let boolean = |name: &str| match value(name)? {
            CellValue::Bool(v) => Ok(*v),
            _ => Err(error("Expected boolean")),
        };
        let mut d = session.definition().clone();
        d.title = text("title")?;
        d.page_size = Size::new(number("width")?, number("height")?);
        d.margins = [
            number("top")?,
            number("right")?,
            number("bottom")?,
            number("left")?,
        ];
        for (i, b) in ReportBandKind::ALL.into_iter().enumerate() {
            b.band_mut(&mut d).height = number(&format!("band{i}"))?;
        }
        if let Some(s) = self.selection {
            let item = &mut s.band.band_mut(&mut d).items[s.item];
            if let ReportItem::Text { bold, .. } = item {
                *bold = boolean("bold")?;
            }
            if let ReportItem::Text { color, .. } | ReportItem::Rectangle { color, .. } = item {
                *color = Color {
                    r: number("red")?,
                    g: number("green")?,
                    b: number("blue")?,
                    a: number("alpha")?,
                };
            }
        }
        let mut parameters = session.parameters().clone();
        for (i, p) in d.parameters.iter().enumerate() {
            let v = value(&format!("parameter{i}"))?.clone();
            p.validate(&v).map_err(error)?;
            parameters.insert(p.name.clone(), v);
        }
        let remove = text("remove_name")?;
        if !remove.is_empty() {
            if !d.parameters.iter().any(|p| p.name == remove) {
                return Err(error("Unknown parameter to remove"));
            }
            d.parameters.retain(|p| p.name != remove);
            parameters.remove(&remove);
        }
        let name = text("new_name")?;
        if !name.is_empty() {
            if d.parameters.iter().any(|p| p.name == name) {
                return Err(error("Duplicate parameter name"));
            }
            let kind = match text("new_type")?.to_ascii_lowercase().as_str() {
                "text" => FieldType::Text,
                "bool" => FieldType::Bool,
                "integer" => FieldType::Integer,
                "real" => FieldType::Real,
                "decimal" => FieldType::Decimal,
                "date" => FieldType::Date,
                "time" => FieldType::Time,
                "binary" => FieldType::Binary,
                _ => {
                    return Err(error(
                        "Parameter type: Text, Bool, Integer, Real, Decimal, Date, Time or Binary",
                    ));
                }
            };
            let mut p = FieldSchema::new(&name, kind);
            p.nullable = boolean("new_nullable")?;
            parameters.insert(name, p.parse(&text("new_value")?).map_err(error)?);
            d.parameters.push(p);
        }
        session
            .replace_configuration(self.revision, d, parameters)
            .map_err(error)
    }
}
