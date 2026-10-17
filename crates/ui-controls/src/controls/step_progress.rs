//! Read-only retained step indicators. They never authorize workflow changes.
use super::wizard_model::invalid;
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct StepProgress {
    pub container: WidgetId,
    pub viewer: ScrollViewer,
}
#[derive(Clone)]
struct Item {
    id: String,
    host: WidgetId,
    title: WidgetId,
    status: WidgetId,
}
struct Entry {
    parts: StepProgress,
    label: String,
    value: StepProgressValue,
    items: Vec<Item>,
    reveal: bool,
    last_viewport_width: f32,
    pub(super) wizard: Option<WidgetId>,
}
#[derive(Default)]
pub(super) struct StepProgressState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    pub fn step_progress(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: StepProgressValue,
        mut style: LayoutStyle,
    ) -> Result<StepProgress, ControlError> {
        if self.step_progresses.entries.len() >= 64 {
            return Err(invalid("At most 64 StepProgress controls are supported"));
        }
        let label = label.into();
        self.prepare_step_progress(&label, &value)?;
        style.kind = LayoutKind::Overlay;
        if style.height == Length::Auto {
            style.height = Length::Px(76.0);
        }
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            let viewer = self.scroll_view(
                container,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
                76.0,
            )?;
            self.configure_scroll_view(
                viewer.viewport,
                ScrollSettings {
                    horizontal: ScrollBarVisibility::Auto,
                    vertical: ScrollBarVisibility::Disabled,
                },
            )?;
            self.edit_props(viewer.content, |p| {
                p.style.kind = LayoutKind::Flex(Axis::Horizontal);
                p.style.gap = 8.0;
                p.style.padding = Edges::all(4.0);
            })?;
            let parts = StepProgress { container, viewer };
            self.step_progresses.entries.insert(
                container,
                Entry {
                    parts,
                    label,
                    value: value.clone(),
                    items: Vec::new(),
                    reveal: true,
                    last_viewport_width: 0.0,
                    wizard: None,
                },
            );
            self.apply_step_progress_value(container, value)?;
            Ok(parts)
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        result
    }
    pub fn step_progress_parts(&self, id: WidgetId) -> Option<StepProgress> {
        self.step_progresses.entries.get(&id).map(|e| e.parts)
    }
    pub fn step_progress_value(&self, id: WidgetId) -> Option<&StepProgressValue> {
        self.step_progresses.entries.get(&id).map(|e| &e.value)
    }
    pub fn step_progress_item(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        self.step_progresses
            .entries
            .get(&id)?
            .items
            .iter()
            .find(|item| item.id == key)
            .map(|item| item.host)
    }
    pub fn set_step_progress_value(
        &mut self,
        id: WidgetId,
        value: StepProgressValue,
    ) -> Result<(), ControlError> {
        let entry = self
            .step_progresses
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown StepProgress"))?;
        if entry.wizard.is_some() {
            return Err(invalid(
                "Wizard owns its step presentation; complete the workflow request instead",
            ));
        }
        self.apply_step_progress_value(id, value)
    }
    pub(super) fn own_step_progress(&mut self, id: WidgetId, wizard: WidgetId) {
        self.step_progresses.entries.get_mut(&id).unwrap().wizard = Some(wizard);
    }
    pub(super) fn prepare_step_progress(
        &mut self,
        label: &str,
        value: &StepProgressValue,
    ) -> Result<(), ControlError> {
        value.validate()?;
        self.measure_text(label, 4096.0, false)?;
        self.measure_text(&summary(label, value), 4096.0, false)?;
        for (index, step) in value.steps.iter().enumerate() {
            self.measure_text(&title(index, step), 4096.0, false)?;
            self.measure_text(&accessible(index, value.steps.len(), step), 4096.0, false)?;
        }
        Ok(())
    }
    pub(super) fn check_step_progress_parts(&self, id: WidgetId) -> Result<(), ControlError> {
        let entry = self
            .step_progresses
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown StepProgress"))?;
        if [
            entry.parts.container,
            entry.parts.viewer.viewport,
            entry.parts.viewer.content,
        ]
        .iter()
        .any(|id| !self.runtime.tree().contains(*id))
            || entry.items.iter().any(|i| {
                [i.host, i.title, i.status]
                    .iter()
                    .any(|id| !self.runtime.tree().contains(*id))
            })
        {
            return Err(invalid("StepProgress required content was removed"));
        }
        let p = entry.parts;
        if self.runtime.tree().parent(p.viewer.viewport) != Some(p.container)
            || self.runtime.tree().parent(p.viewer.content) != Some(p.viewer.viewport)
            || entry.items.iter().any(|item| {
                self.runtime.tree().parent(item.host) != Some(p.viewer.content)
                    || self.runtime.tree().parent(item.title) != Some(item.host)
                    || self.runtime.tree().parent(item.status) != Some(item.host)
            })
        {
            return Err(invalid(
                "StepProgress required parts must remain in their owned hosts",
            ));
        }
        Ok(())
    }
    pub(super) fn apply_step_progress_value(
        &mut self,
        id: WidgetId,
        value: StepProgressValue,
    ) -> Result<(), ControlError> {
        self.check_step_progress_parts(id)?;
        let entry = &self.step_progresses.entries[&id];
        let label = entry.label.clone();
        let parent = entry.parts.viewer.content;
        let old = entry.items.clone();
        self.prepare_step_progress(&label, &value)?;
        let mut created = Vec::new();
        let mut items = Vec::new();
        for step in &value.steps {
            if let Some(item) = old.iter().find(|i| i.id == step.id) {
                items.push(item.clone());
            } else {
                let host = self.panel(
                    parent,
                    LayoutStyle {
                        kind: LayoutKind::Flex(Axis::Vertical),
                        width: Length::Px(180.0),
                        height: Length::Percent(1.0),
                        flex_shrink: 0.0,
                        padding: Edges::all(8.0),
                        gap: 3.0,
                        ..Default::default()
                    },
                );
                let result = host.and_then(|host| {
                    created.push(host);
                    self.set_role(host, Role::Group);
                    self.set_surface(host, SurfaceKind::Card)?;
                    let title = self.label(
                        host,
                        "",
                        LayoutStyle {
                            height: Length::Px(24.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?;
                    self.set_text_display(
                        title,
                        TextDisplaySettings {
                            trimming: TextTrimming::CharacterEllipsis,
                            max_lines: Some(1),
                        },
                    )?;
                    let status = self.label(
                        host,
                        "",
                        LayoutStyle {
                            height: Length::Px(20.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?;
                    Ok(Item {
                        id: step.id.clone(),
                        host,
                        title,
                        status,
                    })
                });
                match result {
                    Ok(item) => {
                        items.push(item);
                    }
                    Err(error) => {
                        for host in created {
                            self.remove(host)?;
                        }
                        return Err(error);
                    }
                }
            }
        }
        for (index, (step, item)) in value.steps.iter().zip(&items).enumerate() {
            self.set_label(item.title, title(index, step))?;
            self.set_label(item.status, state_label(step.state))?;
            self.set_text_style(
                item.title,
                TextStyle {
                    font_weight: Some(if step.id == value.current { 600 } else { 400 }),
                    wrap: false,
                    ..Default::default()
                },
            )?;
            self.set_text_style(
                item.status,
                TextStyle {
                    tone: if step.id == value.current {
                        TextTone::Primary
                    } else {
                        TextTone::Secondary
                    },
                    wrap: false,
                    ..Default::default()
                },
            )?;
            self.set_automation_name(item.host, accessible(index, value.steps.len(), step))?;
        }
        if self
            .runtime
            .tree()
            .children(parent)
            .unwrap()
            .iter()
            .copied()
            .ne(items.iter().map(|i| i.host))
        {
            self.runtime.apply(Mutation::Batch(
                items
                    .iter()
                    .map(|i| Mutation::Reparent {
                        node: i.host,
                        parent,
                    })
                    .collect(),
            ))?;
        }
        self.set_automation_name(id, summary(&label, &value))?;
        let entry = self.step_progresses.entries.get_mut(&id).unwrap();
        entry.reveal |= entry.value.current != value.current
            || entry
                .items
                .iter()
                .map(|item| &item.id)
                .ne(items.iter().map(|item| &item.id));
        entry.value = value;
        entry.items = items;
        for item in old {
            if !self.step_progresses.entries[&id]
                .items
                .iter()
                .any(|i| i.id == item.id)
            {
                self.remove(item.host)?;
            }
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn preflight_step_progress_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.step_progresses.entries.values().any(|e| {
            [e.parts.viewer.viewport, e.parts.viewer.content].contains(&id)
                || e.items
                    .iter()
                    .any(|i| [i.host, i.title, i.status].contains(&id))
                || (e.wizard.is_some() && e.parts.container == id)
        }) {
            return Err(invalid(
                "Remove the owner of StepProgress instead of its required content",
            ));
        }
        Ok(())
    }
    pub(super) fn prune_step_progresses(&mut self) {
        self.step_progresses
            .entries
            .retain(|id, _| self.runtime.tree().contains(*id));
    }
    pub(super) fn sync_step_progress_layout(&mut self) -> Result<(), ControlError> {
        let owners: Vec<_> = self.step_progresses.entries.keys().copied().collect();
        for owner in owners {
            self.check_step_progress_parts(owner)?;
            let e = &self.step_progresses.entries[&owner];
            let Some(viewport) = self.runtime.bounds(e.parts.viewer.viewport) else {
                continue;
            };
            let viewer = e.parts.viewer;
            let count = e.items.len();
            let reveal = e.reveal || e.last_viewport_width != viewport.width;
            let current = e
                .value
                .steps
                .iter()
                .position(|s| s.id == e.value.current)
                .unwrap();
            self.set_scroll_extent_size(
                viewer.viewport,
                Size::new((count * 188) as f32, viewport.height),
            )?;
            if reveal && viewport.width > 0.0 {
                self.scroll_rect_into_view(
                    viewer.viewport,
                    Rect::new(4.0 + current as f32 * 188.0, 0.0, 180.0, viewport.height),
                )?;
                self.step_progresses.entries.get_mut(&owner).unwrap().reveal = false;
            }
            self.step_progresses
                .entries
                .get_mut(&owner)
                .unwrap()
                .last_viewport_width = viewport.width;
        }
        Ok(())
    }
}
fn title(index: usize, step: &StepProgressItem) -> String {
    format!("{}. {}", index + 1, step.title)
}
fn accessible(index: usize, count: usize, step: &StepProgressItem) -> String {
    format!(
        "Шаг {} из {}: {} — {}",
        index + 1,
        count,
        step.title,
        state_label(step.state)
    )
}
fn summary(label: &str, value: &StepProgressValue) -> String {
    let index = value
        .steps
        .iter()
        .position(|s| s.id == value.current)
        .unwrap();
    format!(
        "{label}. {}",
        accessible(index, value.steps.len(), &value.steps[index])
    )
}
fn state_label(state: StepState) -> &'static str {
    match state {
        StepState::Upcoming => "Предстоит",
        StepState::Current => "Текущий шаг",
        StepState::Completed => "Завершён",
        StepState::Error => "Ошибка проверки",
        StepState::Skipped => "Пропущен",
    }
}
