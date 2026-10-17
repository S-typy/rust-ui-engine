//! Application-owned project locations and delayed, cancellable responses.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, BreadcrumbChild, BreadcrumbChildrenRequest, BreadcrumbCompletion,
    BreadcrumbEventKind, BreadcrumbNavigationRequest, BreadcrumbNavigationTarget, BreadcrumbPath,
    BreadcrumbSegment, Controls, DataValue, breadcrumb_path_to_value,
};
use rust_desktop_ui_core::{Mutation, WidgetId};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Location {
    key: String,
    parent: Option<String>,
    label: String,
    enabled: bool,
}
fn locations() -> Vec<Location> {
    let mut values = vec![
        ("root", None, "Проект", true),
        ("projects", Some("root"), "Проекты", true),
        ("engine", Some("projects"), "Редактор интерфейса", true),
        ("src", Some("engine"), "Исходный код", true),
        ("controls", Some("src"), "Компоненты", true),
        ("docs", Some("engine"), "Документация", true),
        ("archive", Some("root"), "Архив", true),
        ("offline", Some("root"), "Недоступный сервер", true),
        ("private", Some("root"), "Закрытый раздел", false),
    ]
    .into_iter()
    .map(|(key, parent, label, enabled)| Location {
        key: key.into(),
        parent: parent.map(str::to_owned),
        label: label.into(),
        enabled,
    })
    .collect::<Vec<_>>();
    for index in 1..=24 {
        values.push(Location {
            key: format!("level{index:02}"),
            parent: Some(if index == 1 {
                "root".into()
            } else {
                format!("level{:02}", index - 1)
            }),
            label: format!("Вложенный раздел {index:02}"),
            enabled: true,
        });
    }
    values
}
fn resolve_key(values: &[Location], key: &str) -> Result<BreadcrumbPath, String> {
    let mut segments = Vec::new();
    let mut cursor = Some(key.to_owned());
    while let Some(key) = cursor {
        let node = values
            .iter()
            .find(|node| node.key == key && node.enabled)
            .ok_or("Расположение не найдено или недоступно")?;
        segments.push(BreadcrumbSegment {
            key: node.key.clone(),
            label: node.label.clone(),
            has_children: node.key == "offline"
                || values
                    .iter()
                    .any(|child| child.parent.as_ref() == Some(&node.key)),
        });
        cursor = node.parent.clone();
    }
    segments.reverse();
    let text = format!(
        "project:/{}",
        segments
            .iter()
            .skip(1)
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>()
            .join("/")
    );
    Ok(BreadcrumbPath { text, segments })
}
pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([
        (
            "Location",
            breadcrumb_path_to_value(&resolve_key(&locations(), "src")?),
        ),
        ("ReadOnly", false.into()),
    ]))
    .map(|model| Rc::new(RefCell::new(model)))
    .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("BreadcrumbHome", "Начало"),
        ("BreadcrumbLong", "Длинный путь"),
        ("BreadcrumbEdit", "Ввести путь"),
        ("BreadcrumbHistory", "История"),
        ("BreadcrumbChildren", "Дочерние разделы"),
        ("BreadcrumbStale", "Поздний ответ"),
        ("BreadcrumbError", "Ошибка источника"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
enum Pending {
    Navigation(BreadcrumbNavigationRequest),
    Children(BreadcrumbChildrenRequest),
}
pub(crate) struct State {
    owner: WidgetId,
    accepted: WidgetId,
    segments: WidgetId,
    status: WidgetId,
    summary: WidgetId,
    locations: Vec<Location>,
    pending: Vec<(Pending, Instant)>,
    applied: u64,
    stale: u64,
    failures: u64,
}
impl State {
    pub(crate) fn install(_ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("ProjectLocation") else {
            return Ok(None);
        };
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        Ok(Some(Self {
            owner,
            accepted: required("BreadcrumbAccepted")?,
            segments: required("BreadcrumbSegments")?,
            status: required("BreadcrumbActivity")?,
            summary: required("BreadcrumbSummary")?,
            locations: locations(),
            pending: vec![],
            applied: 0,
            stale: 0,
            failures: 0,
        }))
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.iter().map(|(_, at)| *at).min()
    }
    fn resolve(&self, target: &BreadcrumbNavigationTarget) -> Result<BreadcrumbPath, String> {
        match target {
            BreadcrumbNavigationTarget::Text(text) => self
                .locations
                .iter()
                .filter(|node| node.enabled)
                .filter_map(|node| resolve_key(&self.locations, &node.key).ok())
                .find(|path| path.text == *text)
                .ok_or("Такого пути нет. Пример: project:/projects/engine/src".into()),
            BreadcrumbNavigationTarget::Segment(keys) => {
                resolve_key(&self.locations, keys.last().ok_or("Пустой путь")?)
            }
            BreadcrumbNavigationTarget::Child { key, .. } => resolve_key(&self.locations, key),
            BreadcrumbNavigationTarget::History(path) => resolve_key(
                &self.locations,
                &path.segments.last().ok_or("Пустой путь")?.key,
            ),
        }
    }
    fn children(
        &self,
        request: &BreadcrumbChildrenRequest,
    ) -> Result<Vec<BreadcrumbChild>, String> {
        let parent = request.parent_keys().last().ok_or("Не задан родитель")?;
        if parent == "offline" {
            return Err("Сервер временно недоступен. Повторите переход позже.".into());
        }
        Ok(self
            .locations
            .iter()
            .filter(|node| node.parent.as_ref() == Some(parent))
            .map(|node| BreadcrumbChild {
                segment: BreadcrumbSegment {
                    key: node.key.clone(),
                    label: node.label.clone(),
                    has_children: node.key == "offline"
                        || self
                            .locations
                            .iter()
                            .any(|child| child.parent.as_ref() == Some(&node.key)),
                },
                enabled: node.enabled,
            })
            .collect())
    }
    fn enqueue(&mut self, pending: Pending, delay: u64) {
        if self.pending.len() == 64 {
            self.pending.remove(0);
        }
        self.pending
            .push((pending, Instant::now() + Duration::from_millis(delay)));
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        for request in ui.drain_breadcrumb_navigation_requests() {
            self.enqueue(Pending::Navigation(request), 300);
        }
        for request in ui.drain_breadcrumb_children_requests() {
            self.enqueue(Pending::Children(request), 750);
        }
        let now = Instant::now();
        let pending = std::mem::take(&mut self.pending);
        for (request, at) in pending {
            if at > now {
                self.pending.push((request, at));
                continue;
            }
            let result = match request {
                Pending::Navigation(request) => {
                    ui.complete_breadcrumb_navigation(&request, self.resolve(request.target()))
                }
                Pending::Children(request) => {
                    ui.complete_breadcrumb_children(&request, self.children(&request))
                }
            }
            .map_err(err)?;
            if result == BreadcrumbCompletion::Stale {
                self.stale += 1;
            }
        }
        for event in ui.drain_breadcrumb_events() {
            match event.kind {
                BreadcrumbEventKind::NavigationAccepted(_) => self.applied += 1,
                BreadcrumbEventKind::NavigationRejected(_)
                | BreadcrumbEventKind::ChildrenRejected(_) => self.failures += 1,
            }
        }
        let path = ui
            .breadcrumb_path(self.owner)
            .ok_or("Location removed")?
            .clone();
        ui.set_label(self.accepted, &path.text).map_err(err)?;
        ui.set_label(
            self.segments,
            path.segments
                .iter()
                .map(|s| s.label.as_str())
                .collect::<Vec<_>>()
                .join(" / "),
        )
        .map_err(err)?;
        let status = match ui.breadcrumb_status(self.owner) {
            Some(rust_desktop_ui_controls::BreadcrumbStatus::Pending) => "Проверка пути…".into(),
            Some(rust_desktop_ui_controls::BreadcrumbStatus::Failed(message)) => message.clone(),
            _ => match ui.breadcrumb_children_status(self.owner) {
                Some(rust_desktop_ui_controls::BreadcrumbStatus::Pending) => {
                    "Загрузка дочерних разделов…".into()
                }
                Some(rust_desktop_ui_controls::BreadcrumbStatus::Failed(message)) => {
                    message.clone()
                }
                _ => "Готово".into(),
            },
        };
        ui.set_label(self.status, status).map_err(err)?;
        ui.set_label(
            self.summary,
            format!(
                "Переходов: {} · устаревших ответов: {} · ошибок: {} · ожидает: {} · история: {}",
                self.applied,
                self.stale,
                self.failures,
                self.pending.len(),
                ui.breadcrumb_history(self.owner)
                    .map_or(0, <[BreadcrumbPath]>::len)
            ),
        )
        .map_err(err)
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        if !command.starts_with("Breadcrumb") {
            return Ok(false);
        }
        let p = ui.breadcrumb_parts(self.owner).ok_or("Location removed")?;
        let focus = ui
            .breadcrumb_path(self.owner)
            .and_then(|path| path.segments.last())
            .and_then(|segment| ui.breadcrumb_segment_button(self.owner, &segment.key))
            .unwrap_or(p.edit_button);
        match command {
            "BreadcrumbHome" | "BreadcrumbLong" => {
                ui.cancel_breadcrumb(self.owner).map_err(err)?;
                ui.set_breadcrumb_path(
                    self.owner,
                    resolve_key(
                        &self.locations,
                        if command == "BreadcrumbHome" {
                            "root"
                        } else {
                            "level24"
                        },
                    )?,
                )
                .map_err(err)?;
            }
            "BreadcrumbEdit" => ui.begin_breadcrumb_edit(self.owner).map_err(err)?,
            "BreadcrumbHistory" => {
                ui.runtime
                    .apply(Mutation::Focus(Some(focus)))
                    .map_err(err)?;
                ui.open_breadcrumb_history(self.owner).map_err(err)?;
            }
            "BreadcrumbChildren" => {
                ui.runtime
                    .apply(Mutation::Focus(Some(focus)))
                    .map_err(err)?;
                ui.request_breadcrumb_children(self.owner, "root")
                    .map_err(err)?;
            }
            "BreadcrumbStale" => {
                ui.runtime
                    .apply(Mutation::Focus(Some(focus)))
                    .map_err(err)?;
                ui.request_breadcrumb_children(self.owner, "root")
                    .map_err(err)?;
                for request in ui.drain_breadcrumb_children_requests() {
                    self.enqueue(Pending::Children(request), 750);
                }
                ui.cancel_breadcrumb(self.owner).map_err(err)?;
            }
            "BreadcrumbError" => {
                ui.cancel_breadcrumb(self.owner).map_err(err)?;
                ui.set_breadcrumb_path(self.owner, resolve_key(&self.locations, "offline")?)
                    .map_err(err)?;
                ui.request_breadcrumb_children(self.owner, "offline")
                    .map_err(err)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
