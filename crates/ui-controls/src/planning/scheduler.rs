use super::common::*;
use super::*;
use crate::{ButtonContentLayout, CommandId, ControlError, Controls, ScrollViewer};
use rust_desktop_ui_core::{
    Axis, InputEvent, Key, LayoutKind, LayoutStyle, Length, Point, Rect, Scene, WidgetId,
};
use rust_desktop_ui_data::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

#[derive(Clone, Debug)]
enum Action {
    Mode(SchedulerMode),
    Page(i64),
    Create,
    Edit(SchedulerEditScope),
    Delete(SchedulerEditScope),
    Shift(i64, bool),
    Apply,
    Cancel,
    Select(ScheduleOccurrenceId),
}
/// Retained calendar with a bounded model, independently editable draft and
/// accessible appointments. Call `process` after input and `refresh` after layout.
pub struct SchedulerControl {
    pub container: WidgetId,
    pub viewport: WidgetId,
    pub title_editor: WidgetId,
    pub start_editor: WidgetId,
    pub end_editor: WidgetId,
    pub resources_editor: WidgetId,
    pub status: WidgetId,
    state: SchedulerState,
    scroll: ScrollViewer,
    editor: WidgetId,
    actions: Rc<RefCell<VecDeque<Action>>>,
    commands: Vec<CommandId>,
    appointments: BTreeMap<SchedulerPlacementKey, (WidgetId, CommandId)>,
    geometry: Option<SchedulerGeometry>,
    reveal_pending: bool,
    last_viewport_size: Option<rust_desktop_ui_core::Size>,
}
fn command(
    ui: &mut Controls,
    parent: WidgetId,
    label: &str,
    action: Action,
    queue: &Rc<RefCell<VecDeque<Action>>>,
    id: String,
    style: LayoutStyle,
) -> Result<(WidgetId, CommandId), ControlError> {
    let id = CommandId(id);
    ui.commands_mut().register(id.clone(), label);
    let actions = queue.clone();
    ui.commands_mut().on_execute(&id, move || {
        let mut q = actions.borrow_mut();
        if q.len() < 128 {
            q.push_back(action.clone());
        }
    });
    let widget = ui.button(parent, label, Some(id.clone()), style)?;
    Ok((widget, id))
}
impl SchedulerControl {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        state: SchedulerState,
        mut style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        style.kind = LayoutKind::Stack(Axis::Vertical);
        style.gap = 6.0;
        let container = ui.transparent_panel(parent, style)?;
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let mut commands = Vec::new();
        let prefix = format!("planner.scheduler.{container:?}");
        let toolbar = stack(ui, container, 80.0)?;
        for (i, (label, action)) in [
            ("День", Action::Mode(SchedulerMode::Day)),
            ("Неделя", Action::Mode(SchedulerMode::Week)),
            ("Месяц", Action::Mode(SchedulerMode::Month)),
            ("Шкала", Action::Mode(SchedulerMode::Timeline)),
            ("Назад", Action::Page(-1)),
            ("Вперёд", Action::Page(1)),
            ("Создать", Action::Create),
            ("Изменить", Action::Edit(SchedulerEditScope::Instance)),
            ("Вся серия", Action::Edit(SchedulerEditScope::Series)),
            ("Удалить", Action::Delete(SchedulerEditScope::Instance)),
            ("Удалить серию", Action::Delete(SchedulerEditScope::Series)),
        ]
        .into_iter()
        .enumerate()
        {
            let (_, cmd) = command(
                ui,
                toolbar,
                label,
                action,
                &actions,
                format!("{prefix}.{i}"),
                size(if i == 10 { 140.0 } else { 100.0 }, 34.0),
            )?;
            commands.push(cmd);
        }
        let scroll = scroll(ui, container, 420.0)?;
        let editor = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                width: Length::Percent(1.0),
                height: Length::Px(146.0),
                gap: 4.0,
                ..Default::default()
            },
        )?;
        let row = stack(ui, editor, 34.0)?;
        let title_editor = ui.edit_box(row, "Название встречи", "", size(300.0, 32.0))?;
        let resources_editor = ui.edit_box(
            row,
            "Ресурсы: числовые ID через запятую",
            "",
            size(250.0, 32.0),
        )?;
        let row = stack(ui, editor, 34.0)?;
        let start_editor = ui.edit_box(
            row,
            "Начало в часовом поясе серии: ДД.ММ.ГГГГ ЧЧ:ММ:СС",
            "",
            size(250.0, 32.0),
        )?;
        let end_editor = ui.edit_box(
            row,
            "Окончание в часовом поясе серии: ДД.ММ.ГГГГ ЧЧ:ММ:СС",
            "",
            size(250.0, 32.0),
        )?;
        let row = stack(ui, editor, 70.0)?;
        for (i, (label, action)) in [
            ("−15 минут", Action::Shift(-900, false)),
            ("+15 минут", Action::Shift(900, false)),
            ("Короче", Action::Shift(-900, true)),
            ("Дольше", Action::Shift(900, true)),
            ("Применить", Action::Apply),
            ("Отмена", Action::Cancel),
        ]
        .into_iter()
        .enumerate()
        {
            let (_, cmd) = command(
                ui,
                row,
                label,
                action,
                &actions,
                format!("{prefix}.{}", 100 + i),
                size(106.0, 32.0),
            )?;
            commands.push(cmd);
        }
        let status = ui.label(
            container,
            "Выберите встречу; изменения подтверждаются явно.",
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Px(48.0),
                ..Default::default()
            },
        )?;
        set_visible(ui, editor, false)?;
        Ok(Self {
            container,
            viewport: scroll.viewport,
            title_editor,
            start_editor,
            end_editor,
            resources_editor,
            status,
            state,
            scroll,
            editor,
            actions,
            commands,
            appointments: BTreeMap::new(),
            geometry: None,
            reveal_pending: true,
            last_viewport_size: None,
        })
    }
    pub fn state(&self) -> &SchedulerState {
        &self.state
    }
    pub fn geometry(&self) -> Option<&SchedulerGeometry> {
        self.geometry.as_ref()
    }
    pub fn appointment_widget(&self, key: SchedulerPlacementKey) -> Option<WidgetId> {
        self.appointments.get(&key).map(|v| v.0)
    }
    pub fn replace_model(
        &mut self,
        ui: &mut Controls,
        model: SchedulingModel,
    ) -> Result<(), ControlError> {
        self.state.replace_model(model).map_err(error)?;
        self.refresh(ui)?;
        Ok(())
    }
    pub fn set_mode(&mut self, ui: &mut Controls, mode: SchedulerMode) -> Result<(), ControlError> {
        let changed = self.state.settings().mode != mode;
        let mut s = self.state.settings().clone();
        s.mode = mode;
        self.state.set_settings(s).map_err(error)?;
        self.reveal_pending |= changed;
        self.refresh(ui)?;
        Ok(())
    }
    pub fn begin_edit(
        &mut self,
        ui: &mut Controls,
        scope: SchedulerEditScope,
    ) -> Result<(), ControlError> {
        self.state.begin_edit(scope).map_err(error)?;
        self.populate(ui)
    }
    pub fn cancel(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.state.cancel();
        set_visible(ui, self.editor, false)?;
        ui.set_label(self.status, "Изменения отменены.")
    }
    fn populate(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let Some(draft) = self.state.draft() else {
            return set_visible(ui, self.editor, false);
        };
        ui.set_text(self.title_editor, &draft.title)?;
        ui.set_text(self.start_editor, &local_text(draft.interval.start))?;
        ui.set_text(self.end_editor, &local_text(draft.interval.end))?;
        ui.set_text(
            self.resources_editor,
            &draft
                .resources
                .iter()
                .map(|r| r.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )?;
        set_visible(ui, self.editor, true)?;
        ui.set_label(
            self.status,
            if draft.is_series() {
                "Редактирование серии. Применить — сохранить; Escape — отменить."
            } else {
                "Редактирование экземпляра. Применить — сохранить; Escape — отменить."
            },
        )
    }
    fn read_draft(&mut self, ui: &Controls) -> Result<(), ControlError> {
        let title = ui
            .document(self.title_editor)
            .ok_or_else(|| error("missing title editor"))?
            .text()
            .to_owned();
        let from = parse_local(
            ui.document(self.start_editor)
                .ok_or_else(|| error("missing start editor"))?
                .text(),
        )?;
        let to = parse_local(
            ui.document(self.end_editor)
                .ok_or_else(|| error("missing end editor"))?
                .text(),
        )?;
        let interval = ScheduleLocalInterval::new(from, to).map_err(error)?;
        let value = ui
            .document(self.resources_editor)
            .ok_or_else(|| error("missing resource editor"))?
            .text();
        let resources = if value.trim().is_empty() {
            Vec::new()
        } else {
            value
                .split(',')
                .map(|v| {
                    v.trim()
                        .parse::<u64>()
                        .map(ScheduleResourceId)
                        .map_err(error)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let draft = self
            .state
            .draft_mut()
            .ok_or_else(|| error("no appointment draft"))?;
        draft.title = title;
        draft.interval = interval;
        draft.resources = resources;
        Ok(())
    }
    pub fn apply(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.read_draft(ui)?;
        self.state.commit().map_err(error)?;
        set_visible(ui, self.editor, false)?;
        ui.set_label(self.status, "Встреча сохранена.")?;
        self.refresh(ui)?;
        Ok(())
    }
    fn run(&mut self, ui: &mut Controls, action: Action) -> Result<(), ControlError> {
        match action {
            Action::Mode(mode) => self.set_mode(ui, mode)?,
            Action::Page(delta) => {
                let mut s = self.state.settings().clone();
                let days = match s.mode {
                    SchedulerMode::Day => 1,
                    SchedulerMode::Week | SchedulerMode::Timeline => 7,
                    SchedulerMode::Month => 0,
                };
                s.anchor = if s.mode == SchedulerMode::Month {
                    super::scheduler_model::next_month(s.anchor, delta)
                } else {
                    super::scheduler_model::next_date(s.anchor, delta * days)
                }
                .map_err(error)?;
                self.state.set_settings(s).map_err(error)?;
                self.reveal_pending = true;
            }
            Action::Select(id) => {
                self.state.select(Some(id)).map_err(error)?;
                if let Some(o) = self.state.occurrences().iter().find(|o| o.id == id) {
                    ui.set_label(
                        self.status,
                        format!(
                            "{} · {} — {}",
                            o.title,
                            local_text(o.local.start),
                            local_text(o.local.end)
                        ),
                    )?;
                }
            }
            Action::Create => {
                let id = self
                    .state
                    .model()
                    .series_iter()
                    .map(|s| s.id.0)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| error("series identity exhausted"))?;
                let start = ScheduleLocal::new(
                    self.state.settings().anchor,
                    TimeValue::new(9, 0, 0, 0).map_err(error)?,
                )
                .map_err(error)?;
                let series = ScheduleSeries {
                    id: ScheduleSeriesId(id),
                    title: "Новая встреча".into(),
                    zone: self.state.settings().display_zone.clone(),
                    interval: ScheduleLocalInterval::new(
                        start,
                        start.checked_add(3600).map_err(error)?,
                    )
                    .map_err(error)?,
                    resources: self
                        .state
                        .settings()
                        .resources
                        .first()
                        .copied()
                        .or_else(|| self.state.model().resources().next().map(|r| r.id))
                        .into_iter()
                        .collect(),
                    recurrence: Default::default(),
                    policy: Default::default(),
                };
                self.state.begin_create(series).map_err(error)?;
                self.populate(ui)?;
            }
            Action::Edit(scope) => self.begin_edit(ui, scope)?,
            Action::Delete(scope) => {
                self.state.delete_selected(scope).map_err(error)?;
                set_visible(ui, self.editor, false)?;
                ui.set_label(self.status, "Встреча удалена.")?;
            }
            Action::Shift(seconds, resize) => {
                self.read_draft(ui)?;
                self.state.shift_draft(seconds, resize).map_err(error)?;
                self.populate(ui)?;
            }
            Action::Apply => self.apply(ui)?,
            Action::Cancel => self.cancel(ui)?,
        }
        Ok(())
    }
    /// Process all queued actions. User validation errors remain visible and keep
    /// the draft; a rejected operation does not terminate the host event loop.
    pub fn process(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let actions = self.actions.borrow_mut().drain(..).collect::<Vec<_>>();
        for action in actions {
            if let Err(e) = self.run(ui, action) {
                ui.set_label(self.status, e.to_string())?;
            }
        }
        Ok(())
    }
    /// Call before ordinary dispatch. Tab and Enter/Space activation remain owned
    /// by the existing controls; Escape cancels the whole appointment draft.
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        let Some(focus) = ui.runtime.focused() else {
            return Ok(false);
        };
        let mut p = Some(focus);
        let mut inside = false;
        while let Some(id) = p {
            if id == self.container {
                inside = true;
                break;
            }
            p = ui.runtime.tree().parent(id);
        }
        if !inside {
            return Ok(false);
        }
        match event {
            InputEvent::KeyDown {
                key: Key::Escape,
                repeat: false,
                ..
            } if self.state.draft().is_some() => {
                self.cancel(ui)?;
                Ok(true)
            }
            InputEvent::KeyDown {
                key: Key::Enter,
                modifiers,
                repeat: false,
            } if modifiers.control && self.state.draft().is_some() => {
                if let Err(e) = self.apply(ui) {
                    ui.set_label(self.status, e.to_string())?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        let Some(bounds) = ui.runtime.bounds(self.viewport) else {
            return Ok(false);
        };
        let geometry = self
            .state
            .geometry(bounds.width.max(160.0))
            .map_err(error)?;
        let mut changed = self.geometry.as_ref() != Some(&geometry);
        let removed = self
            .appointments
            .keys()
            .filter(|key| !geometry.appointments.iter().any(|p| p.key == **key))
            .copied()
            .collect::<Vec<_>>();
        for key in removed {
            let (widget, command) = self.appointments.remove(&key).unwrap();
            ui.remove(widget)?;
            ui.commands_mut().unregister(&command);
            changed = true;
        }
        for placement in &geometry.appointments {
            let widget = if let Some((widget, _)) = self.appointments.get(&placement.key) {
                *widget
            } else {
                let prefix = format!(
                    "planner.scheduler.{:?}.appointment.{:?}",
                    self.container, placement.key
                );
                let (widget, cmd) = command(
                    ui,
                    self.scroll.content,
                    &placement.caption,
                    Action::Select(placement.key.occurrence),
                    &self.actions,
                    format!("{prefix}.0"),
                    position(placement.bounds),
                )?;
                self.appointments.insert(placement.key, (widget, cmd));
                changed = true;
                widget
            };
            changed |= set_style(ui, widget, position(placement.bounds))?;
            ui.set_label(widget, &placement.caption)?;
            ui.set_button_content_layout(widget, ButtonContentLayout::Above)?;
            if let Some((_, id)) = self.appointments.get(&placement.key)
                && ui.commands().label(id) != Some(placement.caption.as_str())
            {
                ui.commands_mut().register(id.clone(), &placement.caption);
                let q = self.actions.clone();
                let selected = placement.key.occurrence;
                ui.commands_mut().on_execute(id, move || {
                    let mut q = q.borrow_mut();
                    if q.len() < 128 {
                        q.push_back(Action::Select(selected));
                    }
                });
            }
            ui.set_automation_name(
                widget,
                format!(
                    "{}{}",
                    placement.description,
                    if self.state.selected() == Some(placement.key.occurrence) {
                        "; Выбрано"
                    } else {
                        ""
                    }
                ),
            )?;
            accent(
                ui,
                widget,
                self.state.selected() == Some(placement.key.occurrence),
                false,
            )?;
            let mut duration = Scene::default();
            if let Some(mark) = placement.time_span
                && let Some(bounds) = ui.runtime.bounds(widget)
            {
                duration.rounded_fill(
                    Rect::new(
                        bounds.x + mark.x,
                        bounds.y + mark.y,
                        mark.width,
                        mark.height,
                    ),
                    bounds,
                    1.0,
                    ui.theme().tokens().focus,
                );
            }
            ui.set_custom_scene(widget, duration)?;
        }
        let extent = geometry
            .appointments
            .iter()
            .fold(geometry.extent, |extent, appointment| {
                rust_desktop_ui_core::Size::new(
                    extent
                        .width
                        .max(appointment.bounds.x + appointment.bounds.width + 24.0),
                    extent
                        .height
                        .max(appointment.bounds.y + appointment.bounds.height + 24.0),
                )
            });
        ui.set_scroll_extent_size(self.viewport, extent)?;
        let viewport_size = rust_desktop_ui_core::Size::new(bounds.width, bounds.height);
        let viewport_changed = self.last_viewport_size != Some(viewport_size);
        if self.reveal_pending || viewport_changed {
            let previous = ui.scroll_offset(self.viewport);
            let target = self
                .state
                .selected()
                .and_then(|selected| {
                    geometry
                        .appointments
                        .iter()
                        .find(|p| p.key.occurrence == selected)
                })
                .or_else(|| geometry.appointments.first());
            if self.reveal_pending {
                // Show useful time context above the initial appointment rather
                // than aligning its last line with the bottom scrollbar.
                let y = target.map_or(0.0, |p| (p.bounds.y - 48.0).max(0.0));
                ui.scroll_to_point(self.viewport, Point::new(0.0, y))?;
            }
            if let Some(target) = target {
                // Scrollbars overlay content. Reserve breathing room beyond the
                // actual card, and only repeat this on a real viewport resize.
                let x = (target.bounds.x - 24.0).max(0.0);
                let y = (target.bounds.y - 24.0).max(0.0);
                let padded = Rect::new(
                    x,
                    y,
                    target.bounds.x + target.bounds.width + 24.0 - x,
                    target.bounds.y + target.bounds.height + 24.0 - y,
                );
                ui.scroll_rect_into_view(self.viewport, padded)?;
            }
            changed |= previous != ui.scroll_offset(self.viewport);
            self.reveal_pending = false;
        }
        self.last_viewport_size = Some(viewport_size);
        let b = ui.runtime.bounds(self.scroll.content).unwrap_or(bounds);
        let paint = scene(
            ui,
            Point::new(b.x, b.y),
            geometry.extent,
            &geometry.captions,
            &geometry.rules,
        );
        ui.set_custom_scene(self.scroll.content, paint)?;
        self.geometry = Some(geometry);
        Ok(changed)
    }
    /// Explicit unmount releases all private command handlers and retained widgets.
    pub fn unmount(self, ui: &mut Controls) -> Result<(), ControlError> {
        for id in self
            .commands
            .into_iter()
            .chain(self.appointments.into_values().map(|v| v.1))
        {
            ui.commands_mut().unregister(&id);
        }
        ui.remove(self.container)
    }
}
