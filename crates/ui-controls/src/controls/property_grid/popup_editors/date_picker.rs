use super::*;
use std::cell::Cell;

pub(super) fn mount(
    ui: &mut Controls,
    host: WidgetId,
    d: &PropertyDescriptor,
    f: &FieldSchema,
    v: &CellValue,
) -> Result<Box<dyn InspectorEditor>, ControlError> {
    let name = d.metadata.label.as_deref().unwrap_or(&f.title);
    let button = ui.button(host, "", None, row_style())?;
    ui.set_automation_name(button, format!("{name}: выбрать дату"))?;
    ui.set_enabled(button, !d.metadata.read_only)?;
    let flyout = ui.flyout(button, Size::new(368., 156.))?;
    ui.set_button_flyout(button, flyout)?;
    ui.edit_props(flyout.content, |p| {
        p.style.kind = LayoutKind::Flex(Axis::Vertical)
    })?;
    let row = ui.transparent_panel(
        flyout.content,
        LayoutStyle {
            kind: LayoutKind::Flex(Axis::Horizontal),
            height: Length::Px(53.),
            width: Length::Percent(1.),
            flex_shrink: 0.,
            gap: 6.,
            ..Default::default()
        },
    )?;
    let mut field = |label: &str, max: f64| -> Result<EditorControl, ControlError> {
        let group = ui.transparent_panel(
            row,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Vertical),
                width: Length::Px(0.),
                flex_grow: 1.,
                ..Default::default()
            },
        )?;
        ui.label(
            group,
            label,
            LayoutStyle {
                height: Length::Px(20.),
                ..Default::default()
            },
        )?;
        ui.spin_box(
            group,
            label,
            1.,
            SpinBoxSettings {
                minimum: 1.,
                maximum: max,
                step: 1.,
            },
            row_style(),
        )
    };
    let day = field("День", 31.)?;
    let month = field("Месяц", 12.)?;
    let year = field("Год", 9999.)?;
    let message = ui.label(
        flyout.content,
        "",
        LayoutStyle {
            height: Length::Px(26.),
            ..Default::default()
        },
    )?;
    let actions = ui.transparent_panel(
        flyout.content,
        LayoutStyle {
            kind: LayoutKind::Flex(Axis::Horizontal),
            height: Length::Px(30.),
            width: Length::Percent(1.),
            flex_shrink: 0.,
            gap: 6.,
            ..Default::default()
        },
    )?;
    let apply = action_button(ui, actions, "Выбрать")?;
    let cancel = action_button(ui, actions, "Отмена")?;
    let picker = DatePicker {
        button,
        flyout,
        day,
        month,
        year,
        message,
        apply,
        cancel,
        value: Cell::new(None),
        was_open: false,
    };
    picker.load(ui, v)?;
    Ok(Box::new(picker))
}

struct DatePicker {
    button: WidgetId,
    flyout: Flyout,
    day: EditorControl,
    month: EditorControl,
    year: EditorControl,
    message: WidgetId,
    apply: WidgetId,
    cancel: WidgetId,
    value: Cell<Option<DateValue>>,
    was_open: bool,
}
impl DatePicker {
    fn reset_parts(&self, ui: &mut Controls) -> Result<(), ControlError> {
        let value = self.value.get().unwrap_or_else(DateValue::today_utc);
        for (part, n) in [
            (&self.year, value.year),
            (&self.month, i32::from(value.month)),
            (&self.day, i32::from(value.day)),
        ] {
            ui.set_spin_value(part.container, f64::from(n))?;
        }
        ui.set_label(self.message, "")
    }
    fn candidate(&self, ui: &Controls) -> Result<DateValue, ControlError> {
        let number = |part: &EditorControl| {
            ui.document(part.editor)
                .and_then(|d| d.text().trim().parse::<i32>().ok())
                .ok_or_else(|| invalid("Введите целые день, месяц и год."))
        };
        let day = u8::try_from(number(&self.day)?).map_err(|_| invalid("Некорректный день."))?;
        let month =
            u8::try_from(number(&self.month)?).map_err(|_| invalid("Некорректный месяц."))?;
        DateValue::new(number(&self.year)?, month, day)
            .map_err(|_| invalid("Такой даты в календаре нет."))
    }
    fn caption(&self, ui: &mut Controls) -> Result<(), ControlError> {
        ui.set_label(
            self.button,
            self.value.get().map_or_else(
                || "Выбрать дату".to_owned(),
                |date| date.to_display(DateFormat::Russian),
            ),
        )
    }
}
impl InspectorEditor for DatePicker {
    fn rest_label(&self, value: &CellValue) -> Option<String> {
        let CellValue::Date(date) = value else {
            return None;
        };
        Some(format!(
            "{:02}.{:02}.{:04}",
            date.day(),
            date.month(),
            date.year()
        ))
    }
    fn owner(&self) -> WidgetId {
        self.button
    }
    fn focus(&self) -> WidgetId {
        self.button
    }
    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        if ui.flyout_is_open(self.flyout) {
            ui.close_flyout(self.flyout)?;
        }
        self.value.set(match value {
            CellValue::Null => None,
            CellValue::Date(d) => Some(DateValue::new(i32::from(d.year()), d.month(), d.day())?),
            _ => return Err(invalid("Expected a date")),
        });
        self.reset_parts(ui)?;
        self.caption(ui)
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        if ui.flyout_is_open(self.flyout) {
            return Err("Завершите выбор даты.".into());
        }
        self.value
            .get()
            .map(|d| {
                rust_desktop_ui_data::DateValue::new(d.year as u16, d.month, d.day)
                    .map(CellValue::Date)
                    .map_err(|e| e.to_string())
            })
            .unwrap_or(Ok(CellValue::Null))
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        if !active && ui.flyout_is_open(self.flyout) {
            ui.close_flyout(self.flyout)?;
        }
        Ok(())
    }
    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let open = ui.flyout_is_open(self.flyout);
        if open && !self.was_open {
            self.reset_parts(ui)?;
        }
        self.was_open = open;
        if !open {
            return Ok(());
        }
        let mut clicked = None;
        ui.activated.retain(|id| {
            if *id == self.apply || *id == self.cancel {
                clicked = Some(*id);
                false
            } else {
                true
            }
        });
        if clicked == Some(self.apply) {
            match self.candidate(ui) {
                Ok(date) => {
                    self.value.set(Some(date));
                    self.caption(ui)?;
                }
                Err(error) => {
                    ui.set_label(self.message, error.to_string())?;
                    return Ok(());
                }
            }
        }
        if clicked.is_some() {
            ui.close_flyout(self.flyout)?;
            self.was_open = false;
        }
        Ok(())
    }
}
