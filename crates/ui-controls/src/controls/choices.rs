//! Nullable checks, switches and stable-key radio collections.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckPresentation {
    #[default]
    CheckBox,
    Toggle,
    Switch,
}

#[derive(Clone, Debug)]
pub struct RadioChoiceGroup {
    pub container: WidgetId,
    pub group: RadioGroupId,
}

#[derive(Clone, Copy)]
struct Choice {
    state: CheckState,
    three_state: bool,
    presentation: CheckPresentation,
}

struct RadioChoices {
    group: RadioGroupId,
    items: Vec<(SelectionItem, WidgetId)>,
}

#[derive(Default)]
pub(super) struct ChoiceControls {
    checks: HashMap<WidgetId, Choice>,
    groups: HashMap<WidgetId, RadioChoices>,
}

impl Controls {
    pub fn checkbox_tristate(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        state: CheckState,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let id = self.checkbox(parent, label, state == CheckState::Checked, style)?;
        self.set_three_state(id, true)?;
        self.set_check_state(id, state)?;
        Ok(id)
    }

    /// A two-state switch uses the same activation, focus and command contract as a check box.
    pub fn switch(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        checked: bool,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let id = self.checkbox(parent, label, checked, style)?;
        self.set_check_presentation(id, CheckPresentation::Switch)?;
        Ok(id)
    }

    fn require_check(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_some()
            && self
                .controls
                .get(&id)
                .is_some_and(|c| matches!(c.kind, Kind::CheckBox | Kind::Toggle))
        {
            Ok(())
        } else {
            Err(ControlError::Invalid(
                "A check or toggle control is required".into(),
            ))
        }
    }

    pub fn check_state(&self, id: WidgetId) -> Option<CheckState> {
        self.runtime.tree().node(id)?;
        let control = self.controls.get(&id)?;
        if control.command.is_none()
            && let Some(choice) = self.choices.checks.get(&id)
        {
            return Some(choice.state);
        }
        control.check_state(&self.commands).map(|value| {
            if value {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            }
        })
    }

    pub fn is_three_state(&self, id: WidgetId) -> bool {
        self.runtime.tree().node(id).is_some()
            && self.choices.checks.get(&id).is_some_and(|c| c.three_state)
    }

    /// Nullable state is local. A command registry toggle remains boolean.
    pub fn set_three_state(&mut self, id: WidgetId, enabled: bool) -> Result<(), ControlError> {
        self.require_check(id)?;
        let control = &self.controls[&id];
        if enabled && control.command.is_some() {
            return Err(ControlError::Invalid(
                "Three-state checks cannot bind a boolean command".into(),
            ));
        }
        let state = self.check_state(id).unwrap_or_default();
        let presentation = if matches!(control.kind, Kind::Toggle) {
            CheckPresentation::Toggle
        } else {
            CheckPresentation::CheckBox
        };
        let choice = self.choices.checks.entry(id).or_insert(Choice {
            state,
            three_state: false,
            presentation,
        });
        choice.three_state = enabled;
        if !enabled && choice.state == CheckState::Indeterminate {
            choice.state = CheckState::Unchecked;
            self.controls.get_mut(&id).unwrap().checked = false;
        }
        self.scene_dirty = true;
        Ok(())
    }

    pub fn set_check_state(&mut self, id: WidgetId, state: CheckState) -> Result<(), ControlError> {
        self.require_check(id)?;
        if state == CheckState::Indeterminate && !self.is_three_state(id) {
            return Err(ControlError::Invalid(
                "Indeterminate requires IsThreeState".into(),
            ));
        }
        self.set_checked(id, state == CheckState::Checked)?;
        if let Some(choice) = self.choices.checks.get_mut(&id) {
            choice.state = state;
        }
        self.scene_dirty = true;
        Ok(())
    }

    pub fn set_check_presentation(
        &mut self,
        id: WidgetId,
        presentation: CheckPresentation,
    ) -> Result<(), ControlError> {
        self.require_check(id)?;
        let state = self.check_state(id).unwrap_or_default();
        self.choices
            .checks
            .entry(id)
            .or_insert(Choice {
                state,
                three_state: false,
                presentation,
            })
            .presentation = presentation;
        self.controls.get_mut(&id).unwrap().kind = if presentation == CheckPresentation::Toggle {
            Kind::Toggle
        } else {
            Kind::CheckBox
        };
        self.set_role(
            id,
            match presentation {
                CheckPresentation::CheckBox => Role::CheckBox,
                CheckPresentation::Toggle => Role::ToggleButton,
                CheckPresentation::Switch => Role::Switch,
            },
        );
        self.scene_dirty = true;
        Ok(())
    }
    pub fn check_presentation(&self, id: WidgetId) -> Option<CheckPresentation> {
        self.require_check(id).ok()?;
        Some(self.choices.checks.get(&id).map_or(
            if matches!(self.controls[&id].kind, Kind::Toggle) {
                CheckPresentation::Toggle
            } else {
                CheckPresentation::CheckBox
            },
            |choice| choice.presentation,
        ))
    }

    pub(super) fn set_choice_bool(&mut self, id: WidgetId, value: bool) {
        if let Some(choice) = self.choices.checks.get_mut(&id) {
            choice.state = if value {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            };
        }
    }

    pub(super) fn cycle_choice(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some(choice) = self.choices.checks.get(&id).copied() else {
            return Ok(false);
        };
        if self.controls[&id].command.is_some() {
            return Ok(false);
        }
        let state = match choice.state {
            CheckState::Unchecked => CheckState::Checked,
            CheckState::Checked if choice.three_state => CheckState::Indeterminate,
            _ => CheckState::Unchecked,
        };
        self.set_check_state(id, state)?;
        Ok(true)
    }

    /// Selection is a stable item key. Replacing or reordering data retains surviving widgets.
    pub fn radio_choice_group(
        &mut self,
        parent: WidgetId,
        items: Vec<SelectionItem>,
        selected: Option<&str>,
        style: LayoutStyle,
    ) -> Result<RadioChoiceGroup, ControlError> {
        validate_items(&items)?;
        if selected.is_some_and(|key| !items.iter().any(|item| item.key == key)) {
            return Err(ControlError::Invalid(
                "Selected radio key is absent from ItemsSource".into(),
            ));
        }
        let container = self.transparent_panel(parent, style)?;
        self.set_role(container, Role::Group);
        let group = self.radio_group();
        self.choices.groups.insert(
            container,
            RadioChoices {
                group,
                items: Vec::new(),
            },
        );
        self.set_radio_items(container, items)?;
        self.set_radio_selected_key(container, selected)?;
        Ok(RadioChoiceGroup { container, group })
    }

    pub fn radio_items(&self, id: WidgetId) -> Option<Vec<SelectionItem>> {
        self.runtime.tree().node(id)?;
        Some(
            self.choices
                .groups
                .get(&id)?
                .items
                .iter()
                .map(|(item, _)| item.clone())
                .collect(),
        )
    }

    pub fn radio_item_widget(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        let widget = self
            .choices
            .groups
            .get(&id)?
            .items
            .iter()
            .find(|(item, _)| item.key == key)?
            .1;
        self.runtime.tree().node(widget)?;
        Some(widget)
    }

    pub fn radio_selected_key(&self, id: WidgetId) -> Option<&str> {
        self.runtime.tree().node(id)?;
        self.choices
            .groups
            .get(&id)?
            .items
            .iter()
            .find(|(_, widget)| self.checked(*widget) == Some(true))
            .map(|(item, _)| item.key.as_str())
    }

    pub fn set_radio_selected_key(
        &mut self,
        id: WidgetId,
        key: Option<&str>,
    ) -> Result<(), ControlError> {
        if !self.runtime.tree().contains(id) {
            return Err(ControlError::Invalid("Unknown RadioGroup".into()));
        }
        let choices = self
            .choices
            .groups
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown RadioGroup".into()))?;
        let selected = key
            .map(|key| {
                choices
                    .items
                    .iter()
                    .find(|(item, _)| item.key == key)
                    .map(|(_, id)| *id)
                    .ok_or_else(|| {
                        ControlError::Invalid(
                            "Selected radio key is absent from ItemsSource".into(),
                        )
                    })
            })
            .transpose()?;
        let widgets: Vec<_> = choices.items.iter().map(|(_, id)| *id).collect();
        if widgets
            .iter()
            .any(|widget| !self.runtime.tree().contains(*widget))
        {
            return Err(ControlError::Invalid(
                "RadioGroup contains a removed item; refresh ItemsSource first".into(),
            ));
        }
        for widget in widgets {
            self.set_checked(widget, Some(widget) == selected)?;
        }
        Ok(())
    }

    pub fn set_radio_items(
        &mut self,
        id: WidgetId,
        items: Vec<SelectionItem>,
    ) -> Result<(), ControlError> {
        validate_items(&items)?;
        self.runtime
            .tree()
            .node(id)
            .ok_or_else(|| ControlError::Invalid("Unknown RadioGroup".into()))?;
        let old = self
            .choices
            .groups
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown RadioGroup".into()))?;
        let group = old.group;
        let selected = self.radio_selected_key(id).map(str::to_owned);
        let old_ids: HashMap<_, _> = old
            .items
            .iter()
            .map(|(item, widget)| (item.key.clone(), *widget))
            .collect();
        let old_focus = self.runtime.focused();
        let mut next = Vec::with_capacity(items.len());
        for item in items {
            let widget = if let Some(&widget) = old_ids
                .get(&item.key)
                .filter(|widget| self.runtime.tree().node(**widget).is_some())
            {
                self.set_label(widget, item.text.clone())?;
                widget
            } else {
                self.radio_button(
                    id,
                    item.text.clone(),
                    group,
                    false,
                    LayoutStyle {
                        height: Length::Px(32.0),
                        flex_shrink: 0.0,
                        ..Default::default()
                    },
                )?
            };
            self.set_enabled(widget, item.enabled)?;
            next.push((item, widget));
        }
        let remaining: HashSet<_> = next.iter().map(|(_, id)| *id).collect();
        // Reparent appends and preserves every surviving subtree, including custom item content.
        for (_, widget) in &next {
            self.runtime.apply(Mutation::Reparent {
                node: *widget,
                parent: id,
            })?;
        }
        for widget in old_ids.values().filter(|id| !remaining.contains(id)) {
            self.runtime.apply(Mutation::Remove(*widget))?;
        }
        self.choices.groups.get_mut(&id).unwrap().items = next;
        if let Some(key) = selected.filter(|key| self.radio_item_widget(id, key).is_some()) {
            self.set_radio_selected_key(id, Some(&key))?;
        }
        if old_focus.is_some_and(|focus| {
            old_ids.values().any(|id| *id == focus)
                && (!remaining.contains(&focus) || !self.is_enabled(focus))
        }) {
            let focus = self.choices.groups[&id]
                .items
                .iter()
                .find(|(item, _)| item.enabled)
                .map(|(_, id)| *id);
            self.runtime.apply(Mutation::Focus(focus))?;
        }
        self.prune_removed_controls()?;
        self.scene_dirty = true;
        Ok(())
    }

    pub(super) fn retain_choices(&mut self) {
        let tree = self.runtime.tree();
        self.choices.checks.retain(|id, _| tree.node(*id).is_some());
        self.choices.groups.retain(|id, group| {
            group.items.retain(|(_, id)| tree.node(*id).is_some());
            tree.node(*id).is_some()
        });
    }

    pub(super) fn paint_choice(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(choice) = self.choices.checks.get(&id).copied() else {
            return Ok(false);
        };
        let state = self.check_state(id).unwrap_or_default();
        if choice.presentation != CheckPresentation::Switch && state != CheckState::Indeterminate {
            return Ok(false);
        }
        let Some(clip) = clip.intersection(bounds) else {
            return Ok(true);
        };
        let switch = choice.presentation == CheckPresentation::Switch;
        let active = state != CheckState::Unchecked;
        let toggle = choice.presentation == CheckPresentation::Toggle;
        let mut visual = self.button_visual_state(id).unwrap_or_default();
        visual.checked = active;
        let control = &self.controls[&id];
        let style = if toggle {
            control
                .button_style
                .resolve_chrome(self.theme, visual, control.button_chrome)
        } else {
            control.button_style.resolve(self.theme, visual, true)
        };
        let content = Rect::new(
            bounds.x + style.padding.left.min(bounds.width),
            bounds.y + style.padding.top.min(bounds.height),
            (bounds.width - style.padding.horizontal()).max(0.0),
            (bounds.height - style.padding.vertical()).max(0.0),
        );
        let height = if switch { 20.0_f32 } else { 18.0 }.min(content.height);
        let width = (if switch { 38.0_f32 } else { 18.0 }).min(content.width);
        let indicator = Rect::new(
            content.x,
            content.y + (content.height - height) / 2.0,
            width,
            height,
        );
        let mut text_bounds;
        if toggle {
            text_bounds = paint_button(&mut self.scene, bounds, clip, style, true, false, false);
            let dash_width = 12.0_f32.min(text_bounds.width);
            fill_clipped(
                &mut self.scene,
                Rect::new(
                    text_bounds.x,
                    text_bounds.y + (text_bounds.height - 2.0).max(0.0) / 2.0,
                    dash_width,
                    2.0_f32.min(text_bounds.height),
                ),
                clip,
                style.foreground,
            );
            let inset = (dash_width + 6.0).min(text_bounds.width);
            text_bounds.x += inset;
            text_bounds.width -= inset;
        } else {
            editor_fill(
                &mut self.scene,
                indicator,
                clip,
                if switch {
                    height / 2.0
                } else {
                    style.corner_radius
                },
                style.background,
            );
            shape_border(
                &mut self.scene,
                indicator,
                clip,
                if switch {
                    height / 2.0
                } else {
                    style.corner_radius
                },
                style.border_width,
                style.border,
            );
            if style.focus_visible {
                let inset = 2.0_f32.min(bounds.width * 0.5).min(bounds.height * 0.5);
                shape_border(
                    &mut self.scene,
                    Rect::new(
                        bounds.x + inset,
                        bounds.y + inset,
                        (bounds.width - inset * 2.0).max(0.0),
                        (bounds.height - inset * 2.0).max(0.0),
                    ),
                    clip,
                    style.corner_radius,
                    2.0,
                    style.accent,
                );
            }
            text_bounds = Rect::new(
                indicator.x + width + 8.0,
                content.y,
                (content.width - width - 8.0).max(0.0),
                content.height,
            );
        }
        if switch {
            let diameter = (height - 6.0).max(0.0);
            let x = match state {
                CheckState::Unchecked => 3.0,
                CheckState::Checked => (width - diameter - 3.0).max(3.0),
                CheckState::Indeterminate => (width - diameter) / 2.0,
            };
            editor_fill(
                &mut self.scene,
                Rect::new(indicator.x + x, indicator.y + 3.0, diameter, diameter),
                clip,
                diameter / 2.0,
                if active {
                    style.checkmark
                } else {
                    style.foreground
                },
            );
        } else if !toggle {
            editor_fill(
                &mut self.scene,
                Rect::new(
                    indicator.x + 4.0,
                    indicator.y + height / 2.0 - 1.0,
                    (width - 8.0).max(0.0),
                    2.0,
                ),
                clip,
                1.0,
                style.checkmark,
            );
        }
        if self.button_content(id).is_some() {
            return Ok(true);
        }
        let Some(text_clip) = text_bounds.intersection(clip) else {
            return Ok(true);
        };
        let mut run = TextRun {
            bounds: text_bounds,
            clip: text_clip,
            text: self.controls[&id].display_label(&self.commands).to_owned(),
            font_size: style.font_size,
            color: style.foreground,
            weight: 400,
            align: if toggle {
                TextAlign::Center
            } else {
                TextAlign::Start
            },
            wrap: false,
        };
        let prepared = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        let top = (text_bounds.height - prepared.height).max(0.0) / 2.0;
        run.bounds.y += top;
        run.bounds.height = (run.bounds.height - top).max(0.0);
        self.scene.text(run);
        Ok(true)
    }
}

fn validate_items(items: &[SelectionItem]) -> Result<(), ControlError> {
    let mut keys = HashSet::new();
    let mut bytes = 0usize;
    if items.len() > 256 {
        return Err(ControlError::Invalid(
            "RadioGroup supports at most 256 items".into(),
        ));
    }
    for item in items {
        bytes = bytes
            .saturating_add(item.key.len())
            .saturating_add(item.text.len());
        if item.key.is_empty() || !keys.insert(&item.key) || bytes > 1_048_576 {
            return Err(ControlError::Invalid(
                "Radio items require unique nonempty keys and at most one MiB of text".into(),
            ));
        }
        TextLimits::default()
            .validate(&item.text)
            .map_err(edit_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ButtonAppearance;
    use rust_desktop_ui_core::Modifiers;
    #[test]
    fn mixed_cycles_once_per_activation_and_boolean_command_rejection_is_atomic() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .checkbox_tristate(
                ui.root(),
                "Nullable",
                CheckState::Unchecked,
                LayoutStyle::default(),
            )
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        for state in [
            CheckState::Checked,
            CheckState::Indeterminate,
            CheckState::Unchecked,
        ] {
            ui.activate(id).unwrap();
            assert_eq!(ui.check_state(id), Some(state));
        }
        assert_eq!(ui.drain_activated().len(), 3);
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        assert_eq!(ui.check_state(id), Some(CheckState::Unchecked));
        ui.dispatch(InputEvent::KeyUp {
            key: Key::Space,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        assert_eq!(ui.check_state(id), Some(CheckState::Checked));
        ui.set_check_state(id, CheckState::Indeterminate).unwrap();
        let semantic = ui
            .semantics()
            .into_iter()
            .find(|node| node.id == id)
            .unwrap();
        assert!(semantic.checked_mixed);
        ui.set_three_state(id, false).unwrap();
        assert_eq!(ui.check_state(id), Some(CheckState::Unchecked));
        assert!(ui.set_check_state(id, CheckState::Indeterminate).is_err());
        ui.commands_mut().register("flag", "Flag");
        ui.commands_mut().set_checked(&"flag".into(), Some(true));
        let bound = ui
            .toggle(
                ui.root(),
                "Flag",
                Some("flag".into()),
                true,
                LayoutStyle::default(),
            )
            .unwrap();
        assert!(ui.set_three_state(bound, true).is_err());
        assert_eq!(ui.checked(bound), Some(true));
        assert!(!ui.is_three_state(bound));
    }
    #[test]
    fn presentation_changes_keep_state_and_use_shared_disabled_and_hover_styles() {
        for theme in [
            Theme::FluentLight,
            Theme::FluentDark,
            Theme::HighContrastLight,
            Theme::HighContrastDark,
        ] {
            let mut ui = Controls::new(theme).unwrap();
            let id = ui
                .checkbox_tristate(
                    ui.root(),
                    "Choice",
                    CheckState::Indeterminate,
                    LayoutStyle {
                        width: Length::Px(160.0),
                        height: Length::Px(40.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            let highlight = Color::rgb(180, 30, 80);
            let disabled = Color::rgb(20, 170, 60);
            ui.set_button_style(
                id,
                ButtonStyle {
                    hovered: ButtonAppearance {
                        background: Some(highlight),
                        ..Default::default()
                    },
                    disabled: ButtonAppearance {
                        background: Some(disabled),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            for presentation in [
                CheckPresentation::Switch,
                CheckPresentation::Toggle,
                CheckPresentation::CheckBox,
            ] {
                ui.set_check_presentation(id, presentation).unwrap();
                ui.set_enabled(id, true).unwrap();
                ui.update(Size::new(300.0, 180.0)).unwrap();
                let bounds = ui.runtime.bounds(id).unwrap();
                ui.dispatch(InputEvent::PointerMoved {
                    position: Point::new(bounds.x + 10.0, bounds.y + 10.0),
                })
                .unwrap();
                ui.update(Size::new(300.0, 180.0)).unwrap();
                assert_eq!(ui.check_state(id), Some(CheckState::Indeterminate));
                assert_eq!(ui.check_presentation(id), Some(presentation));
                assert!(
                    ui.scene()
                        .rounded_rectangles
                        .iter()
                        .any(|shape| shape.color == highlight)
                );
                if presentation == CheckPresentation::Toggle {
                    assert!(
                        ui.scene()
                            .rounded_rectangles
                            .iter()
                            .any(|shape| shape.bounds.width == bounds.width
                                && shape.color == highlight)
                    );
                }
                ui.set_enabled(id, false).unwrap();
                ui.update(Size::new(300.0, 180.0)).unwrap();
                assert!(
                    ui.scene()
                        .rounded_rectangles
                        .iter()
                        .any(|shape| shape.color == disabled)
                );
                ui.activate(id).unwrap();
                assert_eq!(ui.check_state(id), Some(CheckState::Indeterminate));
            }
        }
    }
    #[test]
    fn radio_items_reorder_keep_custom_content_and_selection_and_reject_duplicates() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let group = ui
            .radio_choice_group(
                ui.root(),
                vec![
                    SelectionItem::new("a", "Alpha"),
                    SelectionItem::new("b", "Beta"),
                ],
                Some("b"),
                LayoutStyle::default(),
            )
            .unwrap();
        let b = ui.radio_item_widget(group.container, "b").unwrap();
        let template = ui
            .transparent_panel(ui.root(), LayoutStyle::default())
            .unwrap();
        let label = ui
            .label(template, "Custom Beta", LayoutStyle::default())
            .unwrap();
        ui.set_button_content(b, Some(template)).unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        ui.runtime.apply(Mutation::Focus(Some(b))).unwrap();
        ui.set_radio_items(
            group.container,
            vec![
                SelectionItem::new("b", "Beta new"),
                SelectionItem::new("c", "Gamma"),
            ],
        )
        .unwrap();
        assert_eq!(ui.radio_item_widget(group.container, "b"), Some(b));
        assert_eq!(ui.radio_selected_key(group.container), Some("b"));
        assert_eq!(ui.runtime.focused(), Some(b));
        assert_eq!(ui.button_content(b), Some(template));
        assert!(ui.runtime.tree().contains(label));
        assert!(
            ui.set_radio_items(
                group.container,
                vec![
                    SelectionItem::new("x", "X"),
                    SelectionItem::new("x", "Duplicate")
                ]
            )
            .is_err()
        );
        assert_eq!(ui.radio_item_widget(group.container, "b"), Some(b));
        ui.set_radio_items(group.container, vec![SelectionItem::new("c", "Gamma")])
            .unwrap();
        let c = ui.radio_item_widget(group.container, "c").unwrap();
        assert_eq!(ui.runtime.focused(), Some(c));
        assert_eq!(ui.radio_selected_key(group.container), None);
        assert!(!ui.runtime.tree().contains(label));
    }
}
