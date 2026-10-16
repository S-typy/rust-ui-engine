//! Retained calendar presentation, shared by forms and date editor overlays.
use super::calendar_model::CalendarModel;
use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack};
use std::{cell::RefCell, rc::Rc};

const MONTHS: [&str; 12] = [
    "Январь",
    "Февраль",
    "Март",
    "Апрель",
    "Май",
    "Июнь",
    "Июль",
    "Август",
    "Сентябрь",
    "Октябрь",
    "Ноябрь",
    "Декабрь",
];

#[derive(Clone, Debug)]
pub struct Calendar {
    pub container: WidgetId,
    pub previous: WidgetId,
    pub header: WidgetId,
    pub next: WidgetId,
    pub cells: Vec<WidgetId>,
}
type ValidateSelection = dyn Fn(CalendarSelection) -> Result<(), String>;
#[derive(Clone)]
pub struct CalendarValidator(Rc<ValidateSelection>);
impl CalendarValidator {
    pub fn new(callback: impl Fn(CalendarSelection) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(callback))
    }
    pub fn check(&self, value: CalendarSelection) -> Result<(), String> {
        (self.0)(value)
    }
}
#[derive(Clone)]
struct Entry {
    parts: Calendar,
    model: CalendarModel,
    grid: WidgetId,
    headings: WidgetId,
    validator: Option<CalendarValidator>,
    binding_settings: Rc<RefCell<CalendarSettings>>,
}
#[derive(Default)]
pub(super) struct CalendarState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    pub fn calendar(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        selection: CalendarSelection,
        settings: CalendarSettings,
        mut style: LayoutStyle,
    ) -> Result<Calendar, ControlError> {
        let model = CalendarModel::new(selection, settings)?;
        if self.calendars.entries.len() >= 256 {
            return Err(ControlError::Invalid(
                "At most 256 calendars are supported".into(),
            ));
        }
        if style.width == Length::Auto {
            style.width = Length::Px(320.0);
        }
        if style.height == Length::Auto {
            style.height = Length::Px(300.0);
        }
        style.kind = LayoutKind::Flex(Axis::Vertical);
        style.padding = Edges::all(6.0);
        style.gap = 2.0;
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, label.into())?;
            let title = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(34.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            let previous = self.button(
                title,
                "‹",
                None,
                LayoutStyle {
                    width: Length::Px(32.0),
                    height: Length::Percent(1.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            let header = self.button(
                title,
                "",
                None,
                LayoutStyle {
                    width: Length::Px(0.0),
                    height: Length::Percent(1.0),
                    flex_grow: 1.0,
                    ..Default::default()
                },
            )?;
            let next = self.button(
                title,
                "›",
                None,
                LayoutStyle {
                    width: Length::Px(32.0),
                    height: Length::Percent(1.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            for id in [previous, header, next] {
                self.set_button_chrome(id, ButtonChrome::Flat)?;
            }
            self.set_automation_name(previous, "Предыдущая страница календаря")?;
            self.set_automation_name(next, "Следующая страница календаря")?;
            let headings = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(22.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            for day in ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"] {
                self.label(
                    headings,
                    day,
                    LayoutStyle {
                        width: Length::Px(0.0),
                        height: Length::Percent(1.0),
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                )?;
            }
            let grid = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    width: Length::Percent(1.0),
                    height: Length::Px(0.0),
                    flex_grow: 1.0,
                    grid_columns: vec![GridTrack::Fr(1.0); 7],
                    grid_rows: vec![GridTrack::Fr(1.0); 6],
                    gap: 1.0,
                    ..Default::default()
                },
            )?;
            let mut cells = Vec::with_capacity(42);
            for index in 0..42 {
                let cell = self.button(
                    grid,
                    "",
                    None,
                    LayoutStyle {
                        width: Length::Percent(1.0),
                        height: Length::Percent(1.0),
                        grid_column: GridPlacement {
                            start: Some((index % 7 + 1) as u16),
                            span: 1,
                        },
                        grid_row: GridPlacement {
                            start: Some((index / 7 + 1) as u16),
                            span: 1,
                        },
                        ..Default::default()
                    },
                )?;
                self.set_button_chrome(cell, ButtonChrome::Flat)?;
                self.set_role(cell, Role::ListItem);
                cells.push(cell);
            }
            let parts = Calendar {
                container,
                previous,
                header,
                next,
                cells,
            };
            self.calendars.entries.insert(
                container,
                Entry {
                    parts: parts.clone(),
                    binding_settings: Rc::new(RefCell::new(model.settings.clone())),
                    model,
                    grid,
                    headings,
                    validator: None,
                },
            );
            self.refresh_calendar(container)?;
            Ok(parts)
        })();
        if result.is_err() {
            self.calendars.entries.remove(&container);
            self.remove(container)?;
        }
        result
    }
    pub(super) fn replace_popup_calendar(
        &mut self,
        id: WidgetId,
        selection: CalendarSelection,
        settings: CalendarSettings,
    ) -> Result<(), ControlError> {
        let model = CalendarModel::new(selection, settings)?;
        let entry = self
            .calendars
            .entries
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown calendar".into()))?;
        *entry.binding_settings.borrow_mut() = model.settings.clone();
        entry.model = model;
        self.refresh_calendar(id)
    }
    pub fn calendar_selection(&self, id: WidgetId) -> Option<CalendarSelection> {
        self.calendars.entries.get(&id).map(|e| e.model.selection)
    }
    pub fn calendar_settings(&self, id: WidgetId) -> Option<&CalendarSettings> {
        self.calendars.entries.get(&id).map(|e| &e.model.settings)
    }
    pub fn calendar_view(&self, id: WidgetId) -> Option<CalendarView> {
        self.calendars.entries.get(&id).map(|e| e.model.view)
    }
    pub fn calendar_display_date(&self, id: WidgetId) -> Option<DateValue> {
        self.calendars.entries.get(&id).map(|e| e.model.cursor)
    }
    /// Pending first endpoint; the accepted selection stays unchanged until the second choice.
    pub fn calendar_range_anchor(&self, id: WidgetId) -> Option<DateValue> {
        self.calendars
            .entries
            .get(&id)
            .and_then(|e| e.model.range_anchor)
    }
    pub fn set_calendar_selection(
        &mut self,
        id: WidgetId,
        selection: CalendarSelection,
    ) -> Result<(), ControlError> {
        let entry = self
            .calendars
            .entries
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a Calendar".into()))?;
        if std::mem::discriminant(&selection) != std::mem::discriminant(&entry.model.selection) {
            return Err(ControlError::Invalid(
                "Calendar selection mode is fixed at construction".into(),
            ));
        }
        entry.model.settings.check(selection)?;
        if let Some(validator) = &entry.validator {
            validator.check(selection).map_err(ControlError::Invalid)?;
        }
        let changed = self
            .calendars
            .entries
            .get_mut(&id)
            .unwrap()
            .model
            .set_selection(selection)?;
        self.refresh_calendar(id)?;
        if changed {
            self.calendar_changed(id, selection);
        }
        Ok(())
    }
    pub fn set_calendar_settings(
        &mut self,
        id: WidgetId,
        settings: CalendarSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let entry = self
            .calendars
            .entries
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Not a Calendar".into()))?;
        settings.check(entry.model.selection)?;
        entry.model.cursor = settings.clamp(entry.model.cursor);
        entry.model.range_anchor = entry.model.range_anchor.filter(|v| settings.contains(*v));
        *entry.binding_settings.borrow_mut() = settings.clone();
        entry.model.settings = settings;
        self.refresh_calendar(id)
    }
    pub fn set_calendar_display_date(
        &mut self,
        id: WidgetId,
        date: DateValue,
    ) -> Result<(), ControlError> {
        date.validate()?;
        let entry = self
            .calendars
            .entries
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Not a Calendar".into()))?;
        entry.model.cursor = entry.model.settings.clamp(date);
        self.refresh_calendar(id)
    }
    pub fn set_calendar_view(
        &mut self,
        id: WidgetId,
        view: CalendarView,
    ) -> Result<(), ControlError> {
        self.calendars
            .entries
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Not a Calendar".into()))?
            .model
            .view = view;
        self.refresh_calendar(id)
    }
    pub(crate) fn calendar_source_validator(&self, id: WidgetId) -> Option<CalendarValidator> {
        let entry = self.calendars.entries.get(&id)?;
        let settings = Rc::clone(&entry.binding_settings);
        let application = entry.validator.clone();
        Some(CalendarValidator::new(move |value| {
            settings
                .borrow()
                .check(value)
                .map_err(|error| error.to_string())?;
            if let Some(application) = &application {
                application.check(value)?;
            }
            Ok(())
        }))
    }
    pub fn chain_calendar_validator(
        &mut self,
        id: WidgetId,
        validator: CalendarValidator,
    ) -> Result<(), ControlError> {
        let entry = self
            .calendars
            .entries
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Not a Calendar".into()))?;
        let previous = entry.validator.clone();
        entry.validator = Some(CalendarValidator::new(move |value| {
            if let Some(previous) = &previous {
                previous.check(value)?;
            }
            validator.check(value)
        }));
        Ok(())
    }
    fn calendar_changed(&mut self, id: WidgetId, selection: CalendarSelection) {
        let value = match selection {
            CalendarSelection::Single(value) => EditorValue::Date(value),
            CalendarSelection::Range(value) => EditorValue::DateRange(value),
        };
        self.rich.values.push(ValueChanged { id, value });
    }
    fn refresh_calendar(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let entry = self.calendars.entries[&id].clone();
        let model = &entry.model;
        let parts = &entry.parts;
        let columns = if model.view == CalendarView::Days {
            7
        } else {
            3
        };
        let rows = if model.view == CalendarView::Days {
            6
        } else {
            4
        };
        self.edit_props(entry.headings, |props| {
            props.visible = model.view == CalendarView::Days;
            props.collapsed = model.view != CalendarView::Days;
        })?;
        self.edit_props(entry.grid, |props| {
            props.style.grid_columns = vec![GridTrack::Fr(1.0); columns];
            props.style.grid_rows = vec![GridTrack::Fr(1.0); rows];
        })?;
        let title = match model.view {
            CalendarView::Days => format!(
                "{} {}",
                MONTHS[usize::from(model.cursor.month - 1)],
                model.cursor.year
            ),
            CalendarView::Months => model.cursor.year.to_string(),
            CalendarView::Years => {
                let first = ((model.cursor.year - 1) / 12) * 12 + 1;
                format!("{} – {}", first, (first + 11).min(9999))
            }
        };
        self.set_label(parts.header, &title)?;
        self.set_automation_name(
            parts.header,
            format!("{title}. Изменить представление календаря"),
        )?;
        self.set_enabled(parts.previous, model.can_page(-1))?;
        self.set_enabled(parts.next, model.can_page(1))?;
        let days = model.days();
        let mut representative = None;
        let mut enabled_cells = Vec::new();
        for (index, &cell) in parts.cells.iter().enumerate() {
            let (date, end, caption) = match model.view {
                CalendarView::Days => (
                    days[index],
                    days[index],
                    days[index].map_or(String::new(), |d| d.day.to_string()),
                ),
                CalendarView::Months if index < 12 => {
                    let first = DateValue::new(model.cursor.year, index as u8 + 1, 1)?;
                    (
                        Some(first),
                        Some(DateValue {
                            day: first.days_in_month(),
                            ..first
                        }),
                        MONTHS[index].to_owned(),
                    )
                }
                CalendarView::Years if index < 12 => {
                    let year = ((model.cursor.year - 1) / 12) * 12 + 1 + index as i32;
                    if year <= 9999 {
                        (
                            Some(DateValue::new(year, 1, 1)?),
                            Some(DateValue::new(year, 12, 31)?),
                            year.to_string(),
                        )
                    } else {
                        (None, None, String::new())
                    }
                }
                _ => (None, None, String::new()),
            };
            let visible = model.view == CalendarView::Days || index < 12;
            self.edit_props(cell, |props| {
                props.visible = visible;
                props.collapsed = !visible;
            })?;
            let enabled = visible
                && date
                    .zip(end)
                    .is_some_and(|(d, e)| model.interval_enabled(d, e));
            self.set_enabled(cell, enabled)?;
            self.set_label(cell, caption)?;
            let name = date.map(|d| {
                if model.view == CalendarView::Days {
                    d.to_display(model.settings.format)
                } else if model.view == CalendarView::Months {
                    format!("{} {}", MONTHS[index], d.year)
                } else {
                    d.year.to_string()
                }
            });
            self.set_automation_name(cell, name.unwrap_or_default())?;
            let selected = date.is_some_and(|date| match model.view {
                CalendarView::Days => model.selected(date),
                CalendarView::Months => date.month == model.cursor.month,
                CalendarView::Years => date.year == model.cursor.year,
            });
            self.controls.get_mut(&cell).unwrap().checked = selected;
            if enabled {
                enabled_cells.push(cell);
            }
            if enabled
                && date
                    .zip(end)
                    .is_some_and(|(d, e)| model.cursor >= d && model.cursor <= e)
            {
                representative = Some(cell);
            }
            self.edit_props(cell, |props| {
                props.style.grid_column.start = Some((index % columns + 1) as u16);
                props.style.grid_row.start = Some((index / columns + 1) as u16);
                props.tab_stop = false;
            })?;
        }
        if let Some(cell) = representative.or_else(|| enabled_cells.first().copied()) {
            self.edit_props(cell, |props| props.tab_stop = true)?;
        }
        if let Some(owner) = self
            .rich
            .popup
            .as_ref()
            .filter(|p| p.month.is_some() && self.runtime.tree().parent(id) == Some(p.surface))
            .map(|p| p.owner)
        {
            self.rich.popup.as_mut().unwrap().month = Some(model.cursor);
            self.rich.actions.insert(
                parts.previous,
                selection::RichAction::Month {
                    owner,
                    direction: -1,
                },
            );
            self.rich.actions.insert(
                parts.next,
                selection::RichAction::Month {
                    owner,
                    direction: 1,
                },
            );
            for (index, &cell) in parts.cells.iter().enumerate() {
                self.rich.actions.remove(&cell);
                if model.view == CalendarView::Days
                    && let Some(value) = days[index]
                {
                    self.rich
                        .actions
                        .insert(cell, selection::RichAction::Date { owner, value });
                }
            }
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn is_calendar_cell(&self, id: WidgetId) -> bool {
        self.calendars
            .entries
            .values()
            .any(|e| e.parts.cells.contains(&id))
    }
    pub(super) fn calendar_read_only(&self, id: WidgetId) -> bool {
        self.calendars.entries.values().any(|e| {
            e.model.settings.is_read_only
                && (e.parts.container == id
                    || (e.model.view == CalendarView::Days && e.parts.cells.contains(&id)))
        })
    }
    fn calendar_owner(&self, id: WidgetId) -> Option<WidgetId> {
        self.calendars
            .entries
            .iter()
            .find(|(_, e)| {
                e.parts.container == id
                    || e.parts.previous == id
                    || e.parts.header == id
                    || e.parts.next == id
                    || e.parts.cells.contains(&id)
            })
            .map(|(&owner, _)| owner)
    }
    pub(super) fn focus_calendar_cursor(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let target = self.calendars.entries[&id]
            .parts
            .cells
            .iter()
            .copied()
            .find(|cell| {
                self.runtime
                    .tree()
                    .node(*cell)
                    .is_some_and(|node| node.props.tab_stop)
            });
        if let Some(target) = target {
            self.runtime.apply(Mutation::Focus(Some(target)))?;
        }
        Ok(())
    }
    pub(super) fn activate_calendar(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some(owner) = self.calendar_owner(id) else {
            return Ok(false);
        };
        let entry = self.calendars.entries[&owner].clone();
        let chosen = entry.model.view == CalendarView::Days && entry.parts.cells.contains(&id);
        if !self.is_enabled(id) {
            return Ok(true);
        }
        if id == entry.parts.previous || id == entry.parts.next {
            self.calendars
                .entries
                .get_mut(&owner)
                .unwrap()
                .model
                .page(if id == entry.parts.previous { -1 } else { 1 });
        } else if id == entry.parts.header {
            self.calendars.entries.get_mut(&owner).unwrap().model.view = match entry.model.view {
                CalendarView::Days => CalendarView::Months,
                CalendarView::Months => CalendarView::Years,
                CalendarView::Years => CalendarView::Days,
            };
        } else if let Some(index) = entry.parts.cells.iter().position(|cell| *cell == id) {
            let mut model = entry.model.clone();
            match model.view {
                CalendarView::Days => {
                    if let Some(date) = model.days()[index] {
                        let changed = model.activate(date)?;
                        if changed
                            && let Some(validator) = &entry.validator
                            && let Err(error) = validator.check(model.selection)
                        {
                            self.controls.get_mut(&owner).unwrap().validation_error = Some(error);
                            self.scene_dirty = true;
                            return Ok(true);
                        }
                        if changed {
                            self.calendar_changed(owner, model.selection);
                        }
                    }
                }
                CalendarView::Months => {
                    let first = DateValue::new(model.cursor.year, index as u8 + 1, 1)?;
                    model.cursor = model.settings.clamp(DateValue {
                        day: model.cursor.day.min(first.days_in_month()),
                        ..first
                    });
                    model.view = CalendarView::Days;
                }
                CalendarView::Years => {
                    let year = ((model.cursor.year - 1) / 12) * 12 + 1 + index as i32;
                    let first = DateValue::new(year, model.cursor.month, 1)?;
                    model.cursor = model.settings.clamp(DateValue {
                        day: model.cursor.day.min(first.days_in_month()),
                        ..first
                    });
                    model.view = CalendarView::Months;
                }
            }
            self.controls.get_mut(&owner).unwrap().validation_error = None;
            self.calendars.entries.get_mut(&owner).unwrap().model = model;
        }
        self.refresh_calendar(owner)?;
        self.focus_calendar_cursor(owner)?;
        if chosen {
            self.calendar_popup_changed(owner)?;
        }
        Ok(true)
    }
    pub(super) fn calendar_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let InputEvent::KeyDown { key, modifiers, .. } = event else {
            return Ok(false);
        };
        let Some(focus) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(owner) = self.calendar_owner(focus) else {
            return Ok(false);
        };
        if !self.is_enabled(owner) || modifiers.alt || modifiers.super_key {
            return Ok(false);
        }
        let entry = self.calendars.entries[&owner].clone();
        if !entry.parts.cells.contains(&focus) {
            return Ok(false);
        }
        if matches!(key, Key::Enter | Key::Space) {
            return self.activate_calendar(focus);
        }
        let mut model = entry.model;
        match key {
            Key::Escape if model.range_anchor.is_some() => {
                model.range_anchor = None;
            }
            Key::PageUp | Key::PageDown => model.page(if *key == Key::PageUp { -1 } else { 1 }),
            Key::ArrowUp if modifiers.control => {
                model.view = match model.view {
                    CalendarView::Days => CalendarView::Months,
                    CalendarView::Months | CalendarView::Years => CalendarView::Years,
                };
            }
            Key::Home | Key::End => {
                model.cursor = match model.view {
                    CalendarView::Days => DateValue {
                        day: if *key == Key::Home {
                            1
                        } else {
                            model.cursor.days_in_month()
                        },
                        ..model.cursor
                    },
                    CalendarView::Months => DateValue {
                        month: if *key == Key::Home { 1 } else { 12 },
                        day: 1,
                        ..model.cursor
                    },
                    CalendarView::Years => DateValue {
                        year: (((model.cursor.year - 1) / 12) * 12
                            + if *key == Key::Home { 1 } else { 12 })
                        .min(9999),
                        month: 1,
                        day: 1,
                    },
                };
                model.cursor = model.settings.clamp(model.cursor);
            }
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                let columns = if model.view == CalendarView::Days {
                    7
                } else {
                    3
                };
                let delta = match key {
                    Key::ArrowLeft => -1,
                    Key::ArrowRight => 1,
                    Key::ArrowUp => -columns,
                    _ => columns,
                };
                model.cursor = model.settings.clamp(match model.view {
                    CalendarView::Days => model.cursor.day_offset(delta),
                    CalendarView::Months => model.cursor.month_offset(delta),
                    CalendarView::Years => model.cursor.month_offset(delta * 12),
                });
            }
            _ => return Ok(false),
        }
        self.calendars.entries.get_mut(&owner).unwrap().model = model;
        self.refresh_calendar(owner)?;
        self.focus_calendar_cursor(owner)?;
        Ok(true)
    }
    pub(super) fn prune_calendars(&mut self) {
        self.calendars.entries.retain(|owner, e| {
            self.runtime.tree().contains(*owner)
                && [
                    e.parts.previous,
                    e.parts.header,
                    e.parts.next,
                    e.grid,
                    e.headings,
                ]
                .into_iter()
                .chain(e.parts.cells.iter().copied())
                .all(|id| self.runtime.tree().contains(id))
        });
        self.temporal_parsers
            .retain(|owner, _| self.rich.composites.contains_key(owner));
    }
    pub(super) fn preflight_calendar_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.calendars.entries.values().any(|e| {
            [
                e.parts.previous,
                e.parts.header,
                e.parts.next,
                e.grid,
                e.headings,
            ]
            .contains(&id)
                || e.parts.cells.contains(&id)
        }) {
            return Err(ControlError::Invalid(
                "Remove the calendar container, not a required part".into(),
            ));
        }
        Ok(())
    }
}
