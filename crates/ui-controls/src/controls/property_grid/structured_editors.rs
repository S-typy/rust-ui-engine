use super::*;
use rust_desktop_ui_data::FieldType;
use std::cell::{Cell, RefCell};

pub(super) fn supports(name: &str) -> bool {
    matches!(name, "Flags" | "StringCollection" | "Point")
}

fn masks(d: &PropertyDescriptor) -> Result<Vec<i64>, ControlError> {
    if d.choices.is_empty() || d.choices.len() > 63 || d.choice_keys.len() != d.choices.len() {
        return Err(invalid(
            "Flags requires 1..63 captions and one positive single-bit integer key per caption",
        ));
    }
    let mut used = 0;
    d.choice_keys
        .iter()
        .map(|key| {
            let v = key
                .parse::<i64>()
                .ok()
                .filter(|n| {
                    *n > 0
                        && (*n as u64).is_power_of_two()
                        && n.to_string() == *key
                        && used & *n == 0
                })
                .ok_or_else(|| {
                    invalid("Flag keys must be distinct positive powers of two in decimal notation")
                })?;
            used |= v;
            Ok(v)
        })
        .collect()
}

const ITEM_COUNT: usize = 64;
const ITEM_BYTES: usize = 512;
const JSON_BYTES: usize = 16384;

fn item_ok(s: &str) -> bool {
    s.len() <= ITEM_BYTES
        && !s
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}

fn strings(raw: &str) -> Result<Vec<String>, ControlError> {
    if raw.len() > JSON_BYTES {
        return Err(invalid("String collection exceeds 16 KiB"));
    }
    let items: Vec<String> =
        serde_json::from_str(raw).map_err(|_| invalid("Expected a JSON array of strings"))?;
    if items.len() > ITEM_COUNT || !items.iter().all(|s| item_ok(s)) {
        return Err(invalid(
            "Expected at most 64 single-line strings of at most 512 bytes each",
        ));
    }
    Ok(items)
}

fn point(s: &str) -> Result<(f64, f64), ControlError> {
    let coord = |v: &str| v.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    s.split_once(',')
        .and_then(|(x, y)| Some((coord(x)?, coord(y)?)))
        .ok_or_else(|| invalid("Point requires two finite coordinates separated by a comma"))
}

pub(super) fn validate(
    d: &PropertyDescriptor,
    f: &FieldSchema,
    v: &CellValue,
) -> Result<(), ControlError> {
    f.validate(v).map_err(|e| invalid(e.to_string()))?;
    match d.metadata.editor.as_str() {
        "Flags" if f.kind == FieldType::Integer => {
            masks(d)?;
            if matches!(v, CellValue::Integer(n) if *n < 0) {
                return Err(invalid("Flags requires a nonnegative mask"));
            }
        }
        "StringCollection" if f.kind == FieldType::Text => {
            if let CellValue::Text(s) = v {
                strings(s)?;
            }
        }
        "Point" if f.kind == FieldType::Text => {
            if let CellValue::Text(s) = v {
                point(s)?;
            }
        }
        _ => {
            return Err(invalid(
                "Structured editor and source field are incompatible",
            ));
        }
    }
    Ok(())
}

fn row_style() -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.),
        height: Length::Px(28.),
        ..Default::default()
    }
}

pub(super) fn mount(
    ui: &mut Controls,
    host: WidgetId,
    d: &PropertyDescriptor,
    f: &FieldSchema,
    v: &CellValue,
) -> Result<Box<dyn InspectorEditor>, ControlError> {
    validate(d, f, v)?;
    let name = d.metadata.label.as_deref().unwrap_or(&f.title);
    let editor: Box<dyn InspectorEditor> = match d.metadata.editor.as_str() {
        "Flags" => {
            let keys = masks(d)?;
            let container = ui.transparent_panel(host, vertical_stack())?;
            let group = if keys.len() > 4 {
                let view = ui.scroll_view(
                    container,
                    LayoutStyle {
                        height: Length::Px(112.),
                        ..row_style()
                    },
                    keys.len() as f32 * 28.,
                )?;
                ui.edit_props(view.content, |p| {
                    p.style.kind = LayoutKind::Stack(Axis::Vertical)
                })?;
                view.content
            } else {
                container
            };
            let mut checks = Vec::new();
            for (label, mask) in d.choices.iter().zip(keys) {
                checks.push((
                    ui.checkbox(group, label, false, row_style())?,
                    mask,
                    label.clone(),
                ));
            }
            Box::new(Flags {
                container,
                checks,
                unknown: Cell::new(0),
                nullable: f.nullable,
            })
        }
        "Point" => {
            let container = ui.transparent_panel(
                host,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        GridTrack::Auto,
                        GridTrack::Fr(1.),
                        GridTrack::Auto,
                        GridTrack::Fr(1.),
                    ],
                    gap: 3.,
                    ..row_style()
                },
            )?;
            let mut coordinates = Vec::new();
            for axis in ["X", "Y"] {
                ui.label(
                    container,
                    axis,
                    LayoutStyle {
                        width: Length::Px(16.),
                        ..Default::default()
                    },
                )?;
                let edit = ui.edit_box(container, format!("{name} {axis}"), "", row_style())?;
                coordinates.push(edit);
            }
            Box::new(PointEditor {
                container,
                coords: [coordinates[0], coordinates[1]],
                original: RefCell::new(None),
                nullable: f.nullable,
            })
        }
        "StringCollection" => Box::new(Collection::mount(ui, host, name, f.nullable)?),
        _ => return Err(invalid("Unknown structured property editor")),
    };
    editor.load(ui, v)?;
    Ok(editor)
}

struct Flags {
    container: WidgetId,
    checks: Vec<(WidgetId, i64, String)>,
    unknown: Cell<i64>,
    nullable: bool,
}
impl InspectorEditor for Flags {
    fn owner(&self) -> WidgetId {
        self.container
    }
    fn focus(&self) -> WidgetId {
        self.checks[0].0
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let mut raw = match v {
            CellValue::Integer(n) if *n >= 0 => *n,
            CellValue::Null if self.nullable => 0,
            _ => return Err(invalid("Nonnegative flags value required")),
        };
        for &(id, mask, _) in &self.checks {
            ui.set_checked(id, raw & mask != 0)?;
            raw &= !mask;
        }
        self.unknown.set(raw);
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let mut raw = self.unknown.get();
        for &(id, mask, _) in &self.checks {
            if ui.checked(id).ok_or("Flag checkbox removed")? {
                raw |= mask;
            }
        }
        Ok(CellValue::Integer(raw))
    }
    fn rest_label(&self, v: &CellValue) -> Option<String> {
        let &CellValue::Integer(mut bits) = v else {
            return None;
        };
        let mut items = Vec::new();
        for (_, mask, label) in &self.checks {
            if bits & mask != 0 {
                items.push(label.clone());
                bits &= !mask;
            }
        }
        if bits != 0 {
            items.push(format!("{bits:#X}"));
        }
        if items.is_empty() {
            items.push("Нет".into());
        }
        Some(items.join(", "))
    }
}

struct PointEditor {
    container: WidgetId,
    coords: [WidgetId; 2],
    original: RefCell<Option<String>>,
    nullable: bool,
}
impl InspectorEditor for PointEditor {
    fn owner(&self) -> WidgetId {
        self.container
    }
    fn focus(&self) -> WidgetId {
        self.coords[0]
    }
    fn rest_label(&self, value: &CellValue) -> Option<String> {
        let CellValue::Text(raw) = value else {
            return None;
        };
        let (x, y) = point(raw).ok()?;
        Some(format!("X = {x}   Y = {y}"))
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let ((x, y), original) = match v {
            CellValue::Text(s) => (point(s)?, Some(s.clone())),
            CellValue::Null if self.nullable => ((0., 0.), None),
            _ => return Err(invalid("Expected a two-coordinate point")),
        };
        for (id, n) in self.coords.into_iter().zip([x, y]) {
            ui.set_editor_value(id, &n.to_string())?;
        }
        self.original.replace(original);
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let mut ns = [0.; 2];
        for (n, id) in ns.iter_mut().zip(self.coords) {
            *n = ui
                .document(id)
                .and_then(|d| d.text().trim().parse::<f64>().ok())
                .filter(|n| n.is_finite())
                .ok_or("Введите конечные координаты X и Y")?;
        }
        if let Some(raw) = self.original.borrow().as_ref()
            && point(raw).ok() == Some((ns[0], ns[1]))
        {
            return Ok(CellValue::Text(raw.clone()));
        }
        Ok(CellValue::Text(format!("{},{}", ns[0], ns[1])))
    }
}

struct Collection {
    container: WidgetId,
    list: WidgetId,
    entry: WidgetId,
    buttons: [WidgetId; 4],
    message: WidgetId,
    selected: Cell<Option<usize>>,
    items: RefCell<Vec<String>>,
    original: RefCell<Option<(String, Vec<String>)>>,
    nullable: bool,
}
impl Collection {
    fn mount(
        ui: &mut Controls,
        host: WidgetId,
        title: &str,
        nullable: bool,
    ) -> Result<Self, ControlError> {
        let container = ui.transparent_panel(host, vertical_stack())?;
        let list = ui
            .list_box(
                container,
                title,
                Vec::new(),
                SelectionSettings::default(),
                LayoutStyle {
                    height: Length::Px(104.),
                    ..row_style()
                },
            )?
            .container;
        let entry = ui.edit_box(container, "Текст выбранного элемента", "", row_style())?;
        let actions = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Fr(1.); 4],
                gap: 2.,
                ..row_style()
            },
        )?;
        let button =
            |ui: &mut Controls, label: &str, tip: &str| -> Result<WidgetId, ControlError> {
                let id = ui.button(
                    actions,
                    label,
                    None,
                    LayoutStyle {
                        height: Length::Px(26.),
                        ..Default::default()
                    },
                )?;
                ui.set_automation_name(id, tip)?;
                Ok(id)
            };
        let buttons = [
            button(ui, "+", "Добавить строку")?,
            button(ui, "−", "Удалить строку")?,
            button(ui, "↑", "Переместить выше")?,
            button(ui, "↓", "Переместить ниже")?,
        ];
        let message = ui.label(container, "", row_style())?;
        show(ui, message, false)?;
        Ok(Self {
            container,
            list,
            entry,
            buttons,
            message,
            selected: Cell::new(None),
            items: RefCell::default(),
            original: RefCell::default(),
            nullable,
        })
    }

    fn render(
        &self,
        ui: &mut Controls,
        items: &[String],
        selected: Option<usize>,
    ) -> Result<(), ControlError> {
        Self::set_rows(ui, self.list, items)?;
        let keys = selected
            .map(|i| i.to_string())
            .into_iter()
            .collect::<Vec<_>>();
        ui.set_selected_keys(self.list, &keys)?;
        self.selected.set(selected);
        ui.set_editor_value(self.entry, selected.map_or("", |i| items[i].as_str()))?;
        ui.set_enabled(self.entry, selected.is_some())?;
        for (id, enabled) in self.buttons.into_iter().zip([
            items.len() < ITEM_COUNT,
            selected.is_some(),
            selected.is_some_and(|i| i > 0),
            selected.is_some_and(|i| i + 1 < items.len()),
        ]) {
            ui.set_enabled(id, enabled)?;
        }
        show(ui, self.message, false)
    }

    fn set_rows(ui: &mut Controls, list: WidgetId, items: &[String]) -> Result<(), ControlError> {
        let choices = items
            .iter()
            .enumerate()
            .map(|(i, s)| {
                SelectionItem::new(i.to_string(), if s.is_empty() { "(пусто)" } else { s })
            })
            .collect();
        ui.set_selection_items(list, choices)
    }

    fn draft(&self, ui: &Controls) -> Result<(Vec<String>, String), String> {
        let mut items = self.items.borrow().clone();
        if let Some(i) = self.selected.get() {
            let raw = ui
                .document(self.entry)
                .ok_or("Редактор строки недоступен")?
                .text();
            if !item_ok(raw) {
                return Err(
                    "Строка должна занимать не более 512 байт и помещаться в одну строку".into(),
                );
            }
            items[i] = raw.to_owned();
        }
        let encoded = serde_json::to_string(&items).map_err(|e| e.to_string())?;
        if encoded.len() > JSON_BYTES {
            return Err("Коллекция превышает 16 КиБ".into());
        }
        Ok((items, encoded))
    }
}
impl InspectorEditor for Collection {
    fn owner(&self) -> WidgetId {
        self.container
    }
    fn focus(&self) -> WidgetId {
        self.list
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let (items, original) = match v {
            CellValue::Text(s) => {
                let items = strings(s)?;
                let original = Some((s.clone(), items.clone()));
                (items, original)
            }
            CellValue::Null if self.nullable => (Vec::new(), None),
            _ => return Err(invalid("Expected a JSON array of strings")),
        };
        self.render(ui, &items, (!items.is_empty()).then_some(0))?;
        self.items.replace(items);
        self.original.replace(original);
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let (values, encoded) = self.draft(ui)?;
        if let Some((raw, original)) = self.original.borrow().as_ref()
            && &values == original
        {
            return Ok(CellValue::Text(raw.clone()));
        }
        Ok(CellValue::Text(encoded))
    }
    fn rest_label(&self, v: &CellValue) -> Option<String> {
        if let CellValue::Text(s) = v {
            Some(format!("Элементов: {}", strings(s).ok()?.len()))
        } else {
            None
        }
    }
    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let mut actions = Vec::new();
        ui.activated.retain(|id| {
            if let Some(i) = self.buttons.iter().position(|b| b == id) {
                actions.push(i);
                false
            } else {
                true
            }
        });
        let mut selected = ui
            .selected_keys(self.list)
            .and_then(|v| v.first())
            .and_then(|s| s.parse::<usize>().ok());
        let same_selection = actions.is_empty() && selected == self.selected.get();
        if same_selection
            && selected.is_none_or(|i| {
                ui.document(self.entry)
                    .is_some_and(|d| d.text() == self.items.borrow()[i])
            })
        {
            return show(ui, self.message, false);
        }
        let (mut values, _) = match self.draft(ui) {
            Ok(v) => v,
            Err(e) => {
                let keys = self
                    .selected
                    .get()
                    .map(|i| i.to_string())
                    .into_iter()
                    .collect::<Vec<_>>();
                ui.set_selected_keys(self.list, &keys)?;
                ui.set_label(self.message, e)?;
                show(ui, self.message, true)?;
                return Ok(());
            }
        };
        if same_selection {
            // Refresh only list captions while typing so selection, caret,
            // composition and undo of the live text editor stay intact.
            Self::set_rows(ui, self.list, &values)?;
            self.items.replace(values);
            return show(ui, self.message, false);
        }
        for action in actions {
            match (action, selected) {
                (0, _) if values.len() < ITEM_COUNT => {
                    values.push(String::new());
                    selected = Some(values.len() - 1);
                }
                (1, Some(i)) if i < values.len() => {
                    values.remove(i);
                    selected = (!values.is_empty()).then(|| i.min(values.len() - 1));
                }
                (2, Some(i)) if i > 0 && i < values.len() => {
                    values.swap(i, i - 1);
                    selected = Some(i - 1);
                }
                (3, Some(i)) if i + 1 < values.len() => {
                    values.swap(i, i + 1);
                    selected = Some(i + 1);
                }
                _ => {}
            }
        }
        if serde_json::to_string(&values)
            .map_err(|e| invalid(e.to_string()))?
            .len()
            > JSON_BYTES
        {
            ui.set_label(self.message, "Коллекция превышает 16 КиБ")?;
            return show(ui, self.message, true);
        }
        self.render(ui, &values, selected)?;
        self.items.replace(values);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Modifiers, PointerButton};

    fn click(ui: &mut Controls, id: WidgetId) {
        ui.update(Size::new(670., 520.)).unwrap();
        let b = ui.runtime.bounds(id).unwrap();
        let position = Point::new(b.x + b.width / 2., b.y + b.height / 2.);
        for event in [
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
        ] {
            ui.dispatch(event).unwrap();
        }
    }

    #[test]
    fn collection_add_edit_duplicate_move_delete_and_revert() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut c = Collection::mount(&mut ui, root, "Элементы", false).unwrap();
        let source = CellValue::Text(r#"[ "A", "A" ]"#.into());
        c.load(&mut ui, &source).unwrap();
        assert_eq!(c.read(&ui).unwrap(), source);
        click(&mut ui, c.buttons[0]);
        c.update(&mut ui).unwrap();
        ui.set_text(c.entry, "Журнал").unwrap();
        assert_eq!(
            c.read(&ui).unwrap(),
            CellValue::Text(r#"["A","A","Журнал"]"#.into())
        );
        click(&mut ui, c.buttons[2]);
        c.update(&mut ui).unwrap();
        assert_eq!(
            c.read(&ui).unwrap(),
            CellValue::Text(r#"["A","Журнал","A"]"#.into())
        );
        click(&mut ui, c.buttons[3]);
        c.update(&mut ui).unwrap();
        click(&mut ui, c.buttons[1]);
        c.update(&mut ui).unwrap();
        assert_eq!(c.read(&ui).unwrap(), source);
        ui.set_text(c.entry, "Сохранить").unwrap();
        ui.set_selected_keys(c.list, &["0".into()]).unwrap();
        c.update(&mut ui).unwrap();
        assert_eq!(
            c.read(&ui).unwrap(),
            CellValue::Text(r#"["A","Сохранить"]"#.into())
        );
        c.load(&mut ui, &source).unwrap();
        assert!(c.load(&mut ui, &CellValue::Text("[7]".into())).is_err());
        assert_eq!(c.read(&ui).unwrap(), source);
        ui.set_text(c.entry, &"q".repeat(513)).unwrap();
        assert!(c.read(&ui).is_err());
    }

    #[test]
    fn flags_change_independently_and_unlisted_bits_survive() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let f = FieldSchema::new("mode", FieldType::Integer);
        let mut d = PropertyDescriptor::new("mode");
        d.metadata.editor = "Flags".into();
        d.choices = vec!["Чтение".into(), "Запись".into()];
        d.choice_keys = vec!["1".into(), "4".into()];
        let e = mount(&mut ui, root, &d, &f, &CellValue::Integer(9)).unwrap();
        assert_eq!(e.rest_label(&CellValue::Integer(9)).unwrap(), "Чтение, 0x8");
        click(&mut ui, e.focus());
        assert_eq!(e.read(&ui).unwrap(), CellValue::Integer(8));
        d.choice_keys[1] = "1".into();
        assert!(validate(&d, &f, &CellValue::Integer(0)).is_err());
        d.choice_keys[1] = "6".into();
        assert!(validate(&d, &f, &CellValue::Integer(0)).is_err());
    }

    #[test]
    fn point_preserves_source_spelling_and_validates_both_axes_atomically() {
        let mut ui = Controls::new(Theme::FluentDark).unwrap();
        let root = ui.root();
        let mut d = PropertyDescriptor::new("position");
        d.metadata.editor = "Point".into();
        let f = FieldSchema::new("position", FieldType::Text);
        let source = CellValue::Text(" 1.250, -2.0 ".into());
        let e = mount(&mut ui, root, &d, &f, &source).unwrap();
        assert_eq!(e.read(&ui).unwrap(), source);
        ui.set_text(e.focus(), "3.5").unwrap();
        assert_eq!(e.read(&ui).unwrap(), CellValue::Text("3.5,-2".into()));
        assert!(e.load(&mut ui, &CellValue::Text("3,NaN".into())).is_err());
        assert_eq!(e.read(&ui).unwrap(), CellValue::Text("3.5,-2".into()));
        ui.set_text(e.focus(), "").unwrap();
        assert!(e.read(&ui).is_err());
    }
}
