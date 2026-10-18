use super::common::*;
use crate::{CommandId, ControlError, Controls, TreeControlSettings, TreeSourceHandle};
use rust_desktop_ui_core::{
    Axis, InputEvent, Key, LayoutKind, LayoutStyle, Length, Mutation, Point, Rect, Scene, Size,
    StrokeSegment, WidgetId,
};
use rust_desktop_ui_data::*;
use rust_desktop_ui_treegrid::{ColumnId, GridColumn, IndexedTreeSource, RowKey, TreeNode};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};
#[derive(Clone, Debug)]
pub struct GanttDraft {
    pub task: GanttTask,
    revision: u64,
    generation: u64,
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Edit,
    Create,
    Apply,
    Cancel,
    Move(i64),
    Shift(i64),
    Resize(i64),
    Select(GanttTaskId),
}
#[derive(Clone, Debug, PartialEq)]
pub struct GanttBar {
    pub task: GanttTaskId,
    pub bounds: Rect,
    pub critical: bool,
    pub progress_percent: u8,
}
/// The existing Tree control owns rows, expansion, selection and scroll anchor.
/// Timeline bars materialize precisely those rows and share their stable keys.
pub struct GanttControl {
    pub container: WidgetId,
    pub tree: WidgetId,
    pub timeline: WidgetId,
    pub title_editor: WidgetId,
    pub start_editor: WidgetId,
    pub duration_editor: WidgetId,
    pub progress_editor: WidgetId,
    pub deadline_editor: WidgetId,
    pub status: WidgetId,
    plan: GanttPlan,
    generation: u64,
    source: Rc<RefCell<IndexedTreeSource>>,
    body: WidgetId,
    editor: WidgetId,
    draft: Option<GanttDraft>,
    selected: Option<GanttTaskId>,
    actions: Rc<RefCell<VecDeque<Action>>>,
    commands: Vec<CommandId>,
    bars: BTreeMap<GanttTaskId, (WidgetId, CommandId)>,
    geometry: Vec<GanttBar>,
}
fn nodes(plan: &GanttPlan) -> Vec<TreeNode> {
    plan.tree_order()
        .iter()
        .map(|(id, _)| {
            let t = plan.task(*id).unwrap();
            TreeNode::new(
                RowKey(id.0),
                t.parent.map(|id| RowKey(id.0)),
                t.title.clone(),
            )
        })
        .collect()
}
fn button(
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
    let q = queue.clone();
    ui.commands_mut().on_execute(&id, move || {
        let mut q = q.borrow_mut();
        if q.len() < 128 {
            q.push_back(action);
        }
    });
    let b = ui.button(parent, label, Some(id.clone()), style)?;
    Ok((b, id))
}
impl GanttControl {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        plan: GanttPlan,
        mut style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        style.kind = LayoutKind::Stack(Axis::Vertical);
        style.gap = 6.0;
        let container = ui.transparent_panel(parent, style)?;
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let mut commands = Vec::new();
        let toolbar = stack(ui, container, 76.0)?;
        for (i, (label, action)) in [
            ("Создать", Action::Create),
            ("Изменить", Action::Edit),
            ("Выше", Action::Move(-1)),
            ("Ниже", Action::Move(1)),
            ("−1 час", Action::Shift(-3600)),
            ("+1 час", Action::Shift(3600)),
            ("Короче", Action::Resize(-3600)),
            ("Дольше", Action::Resize(3600)),
        ]
        .into_iter()
        .enumerate()
        {
            let (_, cmd) = button(
                ui,
                toolbar,
                label,
                action,
                &actions,
                format!("planner.gantt.{container:?}.{i}"),
                size(110.0, 32.0),
            )?;
            commands.push(cmd);
        }
        let body = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                width: Length::Percent(1.0),
                height: Length::Px(440.0),
                ..Default::default()
            },
        )?;
        let source = Rc::new(RefCell::new(
            IndexedTreeSource::new(nodes(&plan)).map_err(error)?,
        ));
        let tree = ui.tree_grid(
            body,
            TreeSourceHandle::from_shared(source.clone()),
            TreeControlSettings {
                columns: vec![GridColumn::new(ColumnId(0), "Задачи", 270.0)],
                ..Default::default()
            },
            position(Rect::new(0.0, 0.0, 280.0, 440.0)),
        )?;
        for (id, _) in plan.tree_order() {
            if plan.tasks().any(|t| t.parent == Some(*id)) {
                ui.tree_set_expanded(tree, RowKey(id.0), true)?;
            }
        }
        let timeline = ui.transparent_panel(
            body,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..position(Rect::new(286.0, 0.0, 700.0, 440.0))
            },
        )?;
        let mut props = ui
            .runtime
            .tree()
            .node(timeline)
            .ok_or_else(|| error("missing task timeline"))?
            .props
            .clone();
        props.clip = true;
        ui.runtime.apply(Mutation::SetProps {
            node: timeline,
            props,
        })?;
        let editor = ui.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                width: Length::Percent(1.0),
                height: Length::Px(112.0),
                gap: 4.0,
                ..Default::default()
            },
        )?;
        let row = stack(ui, editor, 34.0)?;
        let title_editor = ui.edit_box(row, "Название задачи", "", size(240.0, 32.0))?;
        let start_editor = ui.edit_box(
            row,
            "Не начинать раньше UTC: ДД.ММ.ГГГГ ЧЧ:ММ:СС",
            "",
            size(250.0, 32.0),
        )?;
        let duration_editor = ui.edit_box(
            row,
            "Рабочая длительность в секундах",
            "",
            size(180.0, 32.0),
        )?;
        let row = stack(ui, editor, 34.0)?;
        let progress_editor =
            ui.edit_box(row, "Прогресс от 0 до 100 процентов", "", size(180.0, 32.0))?;
        let deadline_editor =
            ui.edit_box(row, "Крайний срок UTC или пусто", "", size(250.0, 32.0))?;
        let row = stack(ui, editor, 34.0)?;
        for (i, (label, action)) in [("Применить", Action::Apply), ("Отмена", Action::Cancel)]
            .into_iter()
            .enumerate()
        {
            let (_, cmd) = button(
                ui,
                row,
                label,
                action,
                &actions,
                format!("planner.gantt.{container:?}.edit.{i}"),
                size(140.0, 32.0),
            )?;
            commands.push(cmd);
        }
        let status = ui.label(
            container,
            "Красная рамка — критическая задача; сроки используют рабочий календарь.",
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Px(48.0),
                ..Default::default()
            },
        )?;
        set_visible(ui, editor, false)?;
        Ok(Self {
            container,
            tree,
            timeline,
            title_editor,
            start_editor,
            duration_editor,
            progress_editor,
            deadline_editor,
            status,
            plan,
            generation: 0,
            source,
            body,
            editor,
            draft: None,
            selected: None,
            actions,
            commands,
            bars: BTreeMap::new(),
            geometry: Vec::new(),
        })
    }
    pub fn plan(&self) -> &GanttPlan {
        &self.plan
    }
    pub fn selected(&self) -> Option<GanttTaskId> {
        self.selected
    }
    pub fn scroll_anchor(
        &self,
        ui: &Controls,
    ) -> Result<rust_desktop_ui_treegrid::TreeScrollAnchor, ControlError> {
        Ok(ui
            .tree_model(self.tree)
            .ok_or_else(|| error("missing task tree"))?
            .capture_scroll_anchor(&*self.source.borrow()))
    }
    pub fn draft(&self) -> Option<&GanttDraft> {
        self.draft.as_ref()
    }
    pub fn draft_mut(&mut self) -> Option<&mut GanttDraft> {
        self.draft.as_mut()
    }
    pub fn geometry(&self) -> &[GanttBar] {
        &self.geometry
    }
    pub fn bar_widget(&self, id: GanttTaskId) -> Option<WidgetId> {
        self.bars.get(&id).map(|v| v.0)
    }
    pub fn select(
        &mut self,
        ui: &mut Controls,
        id: Option<GanttTaskId>,
    ) -> Result<(), ControlError> {
        if id.is_some_and(|id| self.plan.task(id).is_none()) {
            return Err(error("unknown task selection"));
        }
        ui.set_tree_selected_key(self.tree, id.map(|id| RowKey(id.0)))?;
        self.selected = id;
        Ok(())
    }
    /// A fully validated plan is projected without replacing Tree or stable keys.
    /// An open draft keeps its original revision and fails on a conflicting apply.
    pub fn replace_plan(&mut self, ui: &mut Controls, plan: GanttPlan) -> Result<(), ControlError> {
        let rows = nodes(&plan);
        let generation = if self.plan != plan {
            self.generation
                .checked_add(1)
                .ok_or_else(|| error("plan source generation exhausted"))?
        } else {
            self.generation
        };
        self.source.borrow_mut().replace(rows).map_err(error)?;
        self.plan = plan;
        self.generation = generation;
        ui.tree_refresh(self.tree)?;
        if self.selected.is_some_and(|id| self.plan.task(id).is_none()) {
            self.selected = None;
        }
        ui.set_tree_selected_key(self.tree, self.selected.map(|id| RowKey(id.0)))?;
        Ok(())
    }
    pub fn begin_edit(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let id = self.selected.ok_or_else(|| error("Выберите задачу."))?;
        let task = self
            .plan
            .task(id)
            .ok_or_else(|| error("Задача удалена."))?
            .clone();
        self.draft = Some(GanttDraft {
            task,
            revision: self.plan.revision(),
            generation: self.generation,
        });
        self.populate(ui)
    }
    fn populate(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let d = self.draft.as_ref().ok_or_else(|| error("no task draft"))?;
        ui.set_text(self.title_editor, &d.task.title)?;
        ui.set_text(self.start_editor, &utc_text(d.task.start_not_before))?;
        ui.set_text(self.duration_editor, &d.task.work_seconds.to_string())?;
        ui.set_text(self.progress_editor, &d.task.progress_percent.to_string())?;
        ui.set_text(
            self.deadline_editor,
            &d.task.finish_not_after.map(utc_text).unwrap_or_default(),
        )?;
        set_visible(ui, self.editor, true)?;
        ui.set_label(
            self.status,
            "Изменения задачи ещё не применены. Ctrl+Enter — применить; Escape — отменить.",
        )
    }
    pub fn cancel(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.draft = None;
        set_visible(ui, self.editor, false)?;
        ui.set_label(self.status, "Изменения задачи отменены.")
    }
    fn read_draft(&mut self, ui: &Controls) -> Result<(), ControlError> {
        let value = |id| {
            ui.document(id)
                .map(|d| d.text())
                .ok_or_else(|| error("missing task editor"))
        };
        let title = value(self.title_editor)?.to_owned();
        let start = parse_utc(value(self.start_editor)?)?;
        let duration = value(self.duration_editor)?.parse::<i64>().map_err(error)?;
        let progress = value(self.progress_editor)?.parse::<u8>().map_err(error)?;
        let text = value(self.deadline_editor)?;
        let deadline = if text.is_empty() {
            None
        } else {
            Some(parse_utc(text)?)
        };
        let d = self.draft.as_mut().ok_or_else(|| error("no task draft"))?;
        d.task.title = title;
        d.task.start_not_before = start;
        d.task.work_seconds = duration;
        d.task.progress_percent = progress;
        d.task.finish_not_after = deadline;
        Ok(())
    }
    pub fn apply(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.read_draft(ui)?;
        let d = self.draft.as_ref().ok_or_else(|| error("no task draft"))?;
        if d.generation != self.generation {
            return Err(error("task source was replaced during editing"));
        }
        let mut candidate = self.plan.clone();
        candidate
            .put_task(d.revision, d.task.clone())
            .map_err(error)?;
        self.replace_plan(ui, candidate)?;
        self.draft = None;
        set_visible(ui, self.editor, false)?;
        ui.set_label(self.status, "Задача и зависимые сроки пересчитаны.")
    }
    fn reorder(&mut self, ui: &mut Controls, direction: i64) -> Result<(), ControlError> {
        let id = self.selected.ok_or_else(|| error("Выберите задачу."))?;
        let parent = self.plan.task(id).unwrap().parent;
        let mut siblings = self
            .plan
            .tree_order()
            .iter()
            .filter_map(|(id, _)| self.plan.task(*id))
            .filter(|t| t.parent == parent)
            .map(|t| t.id)
            .collect::<Vec<_>>();
        let index = siblings.iter().position(|key| *key == id).unwrap();
        let target = (index as i64 + direction).clamp(0, siblings.len() as i64 - 1) as usize;
        siblings.swap(index, target);
        let mut tasks = self.plan.tasks().cloned().collect::<Vec<_>>();
        for t in &mut tasks {
            if let Some(i) = siblings.iter().position(|id| *id == t.id) {
                t.order = i as i64;
            }
        }
        let mut candidate = self.plan.clone();
        candidate
            .replace(
                candidate.revision(),
                self.plan.calendars().cloned().collect(),
                tasks,
                self.plan.dependencies().to_vec(),
            )
            .map_err(error)?;
        self.replace_plan(ui, candidate)
    }
    fn run(&mut self, ui: &mut Controls, action: Action) -> Result<(), ControlError> {
        match action {
            Action::Select(id) => {
                self.select(ui, Some(id))?;
                let t = self.plan.task(id).unwrap();
                let p = self.plan.placement(id).unwrap();
                ui.set_label(
                    self.status,
                    format!(
                        "{} · {} — {} · резерв {} с",
                        t.title,
                        utc_text(p.start),
                        utc_text(p.end),
                        p.slack_work_seconds
                    ),
                )?;
            }
            Action::Edit => self.begin_edit(ui)?,
            Action::Apply => self.apply(ui)?,
            Action::Cancel => self.cancel(ui)?,
            Action::Move(d) => self.reorder(ui, d)?,
            Action::Shift(delta) | Action::Resize(delta) => {
                if self.draft.is_none() {
                    self.begin_edit(ui)?;
                }
                self.read_draft(ui)?;
                let d = self.draft.as_mut().unwrap();
                if matches!(action, Action::Shift(_)) {
                    d.task.start_not_before =
                        d.task.start_not_before.checked_add(delta).map_err(error)?;
                } else {
                    d.task.work_seconds = d
                        .task
                        .work_seconds
                        .checked_add(delta)
                        .ok_or_else(|| error("duration overflow"))?;
                }
                self.populate(ui)?;
            }
            Action::Create => {
                let id = self
                    .plan
                    .tasks()
                    .map(|t| t.id.0)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| error("task identity exhausted"))?;
                let c = self
                    .plan
                    .calendars()
                    .next()
                    .ok_or_else(|| error("Сначала задайте рабочий календарь."))?;
                let task = GanttTask {
                    id: GanttTaskId(id),
                    parent: None,
                    order: id as i64,
                    title: "Новая задача".into(),
                    calendar: c.id,
                    work_seconds: 3600,
                    start_not_before: c.working[0].start,
                    finish_not_after: None,
                    fixed_start: None,
                    progress_percent: 0,
                };
                self.draft = Some(GanttDraft {
                    task,
                    revision: self.plan.revision(),
                    generation: self.generation,
                });
                self.populate(ui)?;
            }
        }
        Ok(())
    }
    pub fn process(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let selected = ui
            .tree_selected_keys(self.tree)
            .and_then(|ids| ids.first().copied())
            .map(|id| GanttTaskId(id.0));
        self.selected = selected;
        let actions = self.actions.borrow_mut().drain(..).collect::<Vec<_>>();
        for action in actions {
            if let Err(e) = self.run(ui, action) {
                ui.set_label(self.status, e.to_string())?;
            }
        }
        Ok(())
    }
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        if let InputEvent::Wheel {
            position, delta, ..
        } = event
            && ui
                .runtime
                .bounds(self.timeline)
                .is_some_and(|b| b.contains(position.x, position.y))
        {
            let (x, y) = ui
                .tree_model(self.tree)
                .ok_or_else(|| error("missing task tree"))?
                .scroll_offset();
            ui.tree_scroll_to(self.tree, x, y + f64::from(delta.y))?;
            return Ok(true);
        }
        let mut id = ui.runtime.focused();
        let mut inside = false;
        while let Some(current) = id {
            if current == self.container {
                inside = true;
                break;
            }
            id = ui.runtime.tree().parent(current);
        }
        if !inside {
            return Ok(false);
        }
        match event {
            InputEvent::KeyDown {
                key: Key::Escape,
                repeat: false,
                ..
            } if self.draft.is_some() => {
                self.cancel(ui)?;
                Ok(true)
            }
            InputEvent::KeyDown {
                key: Key::Enter,
                modifiers,
                repeat: false,
            } if modifiers.control && self.draft.is_some() => {
                if let Err(e) = self.apply(ui) {
                    ui.set_label(self.status, e.to_string())?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        let Some(body) = ui.runtime.bounds(self.body) else {
            return Ok(false);
        };
        let tree_width = (body.width * 0.3).clamp(150.0, 300.0);
        let mut changed = set_style(
            ui,
            self.tree,
            position(Rect::new(0.0, 0.0, tree_width, body.height)),
        )?;
        changed |= set_style(
            ui,
            self.timeline,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..position(Rect::new(
                    tree_width + 6.0,
                    0.0,
                    (body.width - tree_width - 6.0).max(1.0),
                    body.height,
                ))
            },
        )?;
        if changed {
            return Ok(true);
        }
        let bounds = ui
            .runtime
            .bounds(self.timeline)
            .ok_or_else(|| error("missing task timeline"))?;
        let rows = ui
            .tree_frame(self.tree)
            .ok_or_else(|| error("missing task frame"))?
            .semantics
            .rows
            .clone();
        let min = self
            .plan
            .tasks()
            .filter_map(|t| self.plan.placement(t.id).map(|p| p.start.0))
            .min()
            .unwrap_or(0);
        let max = self
            .plan
            .tasks()
            .filter_map(|t| self.plan.placement(t.id).map(|p| p.end.0))
            .max()
            .unwrap_or(1);
        let span = (i128::from(max) - i128::from(min)).max(1) as f64;
        let project_x = |v: i64| {
            8.0 + ((i128::from(v) - i128::from(min)) as f64 / span) as f32
                * (bounds.width - 16.0).max(1.0)
        };
        let mut geometry = Vec::new();
        let mut captions = Vec::new();
        let mut rules = Vec::new();
        for tick in 0..=4 {
            let x = 8.0 + tick as f32 * (bounds.width - 16.0).max(1.0) / 4.0;
            let value = (min as f64 + span * tick as f64 / 4.0)
                .clamp(i64::MIN as f64, i64::MAX as f64) as i64;
            captions.push(PlannerCaption {
                bounds: Rect::new(x, 3.0, 140.0, 28.0),
                text: utc_text(ScheduleInstant(value)),
                heading: false,
            });
            rules.push(PlannerRule {
                bounds: Rect::new(x, 30.0, 1.0, (bounds.height - 30.0).max(0.0)),
            });
        }
        for row in rows {
            let id = GanttTaskId(row.key.0);
            let Some(p) = self.plan.placement(id) else {
                continue;
            };
            let t = self.plan.task(id).unwrap();
            let y = row.bounds.y - bounds.y;
            let b = Rect::new(
                project_x(p.start.0),
                y + 3.0,
                (project_x(p.end.0) - project_x(p.start.0)).max(4.0),
                (row.bounds.height - 6.0).max(4.0),
            );
            geometry.push(GanttBar {
                task: id,
                bounds: b,
                critical: p.critical,
                progress_percent: t.progress_percent,
            });
            rules.push(PlannerRule {
                bounds: Rect::new(0.0, y, bounds.width, 1.0),
            });
        }
        let obsolete = self
            .bars
            .keys()
            .filter(|id| !geometry.iter().any(|b| b.task == **id))
            .copied()
            .collect::<Vec<_>>();
        for id in obsolete {
            let (widget, command) = self.bars.remove(&id).unwrap();
            ui.remove(widget)?;
            ui.commands_mut().unregister(&command);
            changed = true;
        }
        for bar in &geometry {
            let t = self.plan.task(bar.task).unwrap();
            let widget = if let Some((widget, _)) = self.bars.get(&bar.task) {
                *widget
            } else {
                let (widget, command) = button(
                    ui,
                    self.timeline,
                    &t.title,
                    Action::Select(bar.task),
                    &self.actions,
                    format!("planner.gantt.{:?}.task.{}", self.container, bar.task.0),
                    position(bar.bounds),
                )?;
                self.bars.insert(bar.task, (widget, command));
                changed = true;
                widget
            };
            changed |= set_style(ui, widget, position(bar.bounds))?;
            let id = &self.bars[&bar.task].1;
            if ui.commands().label(id) != Some(t.title.as_str()) {
                ui.commands_mut().register(id.clone(), &t.title);
                let queue = self.actions.clone();
                let task = bar.task;
                ui.commands_mut().on_execute(id, move || {
                    let mut q = queue.borrow_mut();
                    if q.len() < 128 {
                        q.push_back(Action::Select(task));
                    }
                });
            }
            accent(ui, widget, self.selected == Some(bar.task), bar.critical)?;
            let p = self.plan.placement(bar.task).unwrap();
            ui.set_automation_name(
                widget,
                format!(
                    "{}; {} — {}; {}%; {}",
                    t.title,
                    utc_text(p.start),
                    utc_text(p.end),
                    t.progress_percent,
                    if p.critical {
                        "Критическая задача"
                    } else {
                        "Есть резерв"
                    }
                ),
            )?;
            if let Some(b) = ui.runtime.bounds(widget) {
                let mut paint = Scene::default();
                paint.rounded_fill(
                    Rect::new(
                        b.x + 2.0,
                        b.y + b.height - 5.0,
                        (b.width - 4.0).max(0.0) * f32::from(t.progress_percent) / 100.0,
                        3.0,
                    ),
                    b,
                    1.0,
                    ui.theme().tokens().focus,
                );
                ui.set_custom_scene(widget, paint)?;
            }
        }
        let mut paint = scene(
            ui,
            Point::new(bounds.x, bounds.y),
            Size::new(bounds.width, bounds.height),
            &captions,
            &rules,
        );
        for dep in self.plan.dependencies() {
            let a = geometry.iter().find(|b| b.task == dep.predecessor);
            let b = geometry.iter().find(|b| b.task == dep.successor);
            if let (Some(a), Some(b)) = (a, b) {
                let from_end = matches!(
                    dep.kind,
                    GanttDependencyKind::FinishStart | GanttDependencyKind::FinishFinish
                );
                let to_end = matches!(
                    dep.kind,
                    GanttDependencyKind::FinishFinish | GanttDependencyKind::StartFinish
                );
                let x1 = bounds.x + a.bounds.x + if from_end { a.bounds.width } else { 0.0 };
                let x2 = bounds.x + b.bounds.x + if to_end { b.bounds.width } else { 0.0 };
                let y1 = bounds.y + a.bounds.y + a.bounds.height * 0.5;
                let y2 = bounds.y + b.bounds.y + b.bounds.height * 0.5;
                let mid = (x1 + x2) * 0.5;
                paint.stroke_path(
                    &[
                        StrokeSegment {
                            start: Point::new(x1, y1),
                            end: Point::new(mid, y1),
                        },
                        StrokeSegment {
                            start: Point::new(mid, y1),
                            end: Point::new(mid, y2),
                        },
                        StrokeSegment {
                            start: Point::new(mid, y2),
                            end: Point::new(x2, y2),
                        },
                    ],
                    bounds,
                    1.0,
                    ui.theme().tokens().muted,
                );
            }
        }
        ui.set_custom_scene(self.timeline, paint)?;
        changed |= geometry != self.geometry;
        self.geometry = geometry;
        Ok(changed)
    }
    pub fn unmount(self, ui: &mut Controls) -> Result<(), ControlError> {
        for id in self
            .commands
            .into_iter()
            .chain(self.bars.into_values().map(|v| v.1))
        {
            ui.commands_mut().unregister(&id);
        }
        ui.remove(self.container)
    }
}
