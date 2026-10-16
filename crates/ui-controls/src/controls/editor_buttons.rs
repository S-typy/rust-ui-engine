//! Configuration for retained buttons embedded in one editor session.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorButtonPlacement {
    Leading,
    #[default]
    Trailing,
}

/// Button geometry and availability are independent of its command and identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorButtonSettings {
    pub placement: EditorButtonPlacement,
    /// Preferred logical pixels, finite and in `(0, 1024]`. Narrow hosts shrink
    /// the button strip proportionally while reserving space for the editor.
    pub width: f32,
    pub visible: bool,
    pub enabled: bool,
    /// Permits commands while the editor is read-only. Clear never mutates a
    /// read-only editor, even when this option is enabled.
    pub enabled_when_read_only: bool,
    pub tab_stop: bool,
}
impl Default for EditorButtonSettings {
    fn default() -> Self {
        Self {
            placement: EditorButtonPlacement::Trailing,
            width: 30.0,
            visible: true,
            enabled: true,
            enabled_when_read_only: false,
            tab_stop: true,
        }
    }
}

impl Controls {
    pub fn editor_button_settings(&self, button: WidgetId) -> Option<EditorButtonSettings> {
        self.rich.button_settings.get(&button).copied()
    }

    /// Validates first, then updates placement and keyboard order atomically.
    /// The editor document, selection, history and accepted value are retained.
    pub fn set_editor_button_settings(
        &mut self,
        button: WidgetId,
        settings: EditorButtonSettings,
    ) -> Result<(), ControlError> {
        if !settings.width.is_finite() || settings.width <= 0.0 || settings.width > 1024.0 {
            return Err(ControlError::Invalid(
                "Editor button width must be finite and in (0, 1024]".into(),
            ));
        }
        let owner = self
            .rich_owner(button)
            .filter(|owner| self.rich.composites[owner].parts.buttons.contains(&button))
            .ok_or_else(|| ControlError::Invalid("Not a live embedded editor button".into()))?;
        let parts = &self.rich.composites[&owner].parts;
        let placement = |id: WidgetId| {
            if id == button {
                settings
            } else {
                self.rich.button_settings[&id]
            }
        };
        let ordered: Vec<_> = parts
            .buttons
            .iter()
            .copied()
            .filter(|&id| placement(id).placement == EditorButtonPlacement::Leading)
            .chain(std::iter::once(parts.editor))
            .chain(
                parts
                    .buttons
                    .iter()
                    .copied()
                    .filter(|&id| placement(id).placement == EditorButtonPlacement::Trailing),
            )
            .collect();
        let mut mutations = Vec::new();
        if self.runtime.tree().children(owner) != Some(ordered.as_slice()) {
            mutations.extend(ordered.iter().copied().map(|node| Mutation::Reparent {
                node,
                parent: owner,
            }));
        }
        let mut props = self.runtime.tree().node(button).unwrap().props.clone();
        if self.runtime.focused() == Some(button) && (!settings.visible || !settings.enabled) {
            mutations.push(Mutation::Focus(Some(parts.editor)));
        }
        props.visible = settings.visible;
        props.collapsed = !settings.visible;
        props.tab_stop = settings.tab_stop;
        mutations.push(Mutation::SetProps {
            node: button,
            props,
        });
        self.runtime.apply(Mutation::Batch(mutations))?;
        self.rich.button_settings.insert(button, settings);
        self.controls.get_mut(&button).unwrap().enabled = settings.enabled;
        self.refresh_paints()?;
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(())
    }

    /// Installs a bounded noninteractive subtree inside the button. The button
    /// retains its accessible label, command, focus and tooltip. Clearing or
    /// replacing detaches the previous content for reuse by its owner.
    pub fn set_editor_button_content(
        &mut self,
        button: WidgetId,
        content: Option<WidgetId>,
    ) -> Result<(), ControlError> {
        if self.editor_button_settings(button).is_none() || self.rich_owner(button).is_none() {
            return Err(ControlError::Invalid(
                "Not a live embedded editor button".into(),
            ));
        }
        self.set_button_content(button, content)?;
        if let Some(content) = content {
            // A single Grid cell honors both alignment axes. Overlay stacks
            // on its main axis and cannot center a narrow caption horizontally.
            let mut owner_style = self
                .runtime
                .tree()
                .node(button)
                .unwrap()
                .props
                .style
                .clone();
            owner_style.kind = LayoutKind::Grid;
            owner_style.grid_columns = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
            owner_style.grid_rows = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
            let mut content_style = self
                .runtime
                .tree()
                .node(content)
                .unwrap()
                .props
                .style
                .clone();
            content_style
                .horizontal_alignment
                .get_or_insert(rust_desktop_ui_core::Align::Center);
            content_style
                .vertical_alignment
                .get_or_insert(rust_desktop_ui_core::Align::Center);
            self.runtime.apply(Mutation::Batch(vec![
                Mutation::SetStyle {
                    node: button,
                    style: owner_style,
                },
                Mutation::SetStyle {
                    node: content,
                    style: content_style,
                },
            ]))?;
        }
        Ok(())
    }

    pub(super) fn editor_button_allows_read_only(&self, id: WidgetId) -> bool {
        self.rich
            .button_settings
            .get(&id)
            .is_some_and(|settings| settings.enabled_when_read_only)
            && matches!(
                self.rich.actions.get(&id),
                Some(selection::RichAction::Button {
                    action: EditorButtonAction::Command(_),
                    ..
                })
            )
    }

    pub(super) fn editor_button_blocked(&self, id: WidgetId) -> bool {
        self.rich.button_settings.get(&id).is_some_and(|settings| {
            !settings.enabled
                || !settings.visible
                || self.rich_owner(id).is_some_and(|owner| {
                    self.rich.composites[&owner].read_only
                        && !self.editor_button_allows_read_only(id)
                })
        })
    }

    pub(super) fn sync_editor_buttons(&mut self) -> Result<(), ControlError> {
        let automatic: Vec<_> = self
            .rich
            .automatic_heights
            .iter()
            .map(|(&id, &height)| (id, height))
            .collect();
        for (id, previous) in automatic {
            let Some(node) = self.runtime.tree().node(id) else {
                continue;
            };
            let mut style = node.props.style.clone();
            if style.height != Length::Px(previous) && style.height != Length::Auto {
                self.rich.automatic_heights.remove(&id);
                continue;
            }
            let height = self.theme.tokens().control_height;
            if style.height != Length::Px(height) {
                style.height = Length::Px(height);
                self.runtime.apply(Mutation::SetStyle { node: id, style })?;
            }
            self.rich.automatic_heights.insert(id, height);
        }
        let mut mutations = Vec::new();
        for (&owner, composite) in &self.rich.composites {
            let Some(bounds) = self.runtime.bounds(owner) else {
                continue;
            };
            let props = &self.runtime.tree().node(owner).unwrap().props;
            let available =
                (bounds.width - props.style.padding.left - props.style.padding.right).max(0.0);
            let desired: f32 = composite
                .parts
                .buttons
                .iter()
                .filter_map(|id| self.rich.button_settings.get(id))
                .filter(|s| s.visible)
                .map(|s| s.width)
                .sum();
            let budget = (available - available.min(24.0)).max(0.0);
            let scale = if desired > 0.0 {
                (budget / desired).min(1.0)
            } else {
                1.0
            };
            for &id in &composite.parts.buttons {
                let settings = self.rich.button_settings[&id];
                let mut style = self.runtime.tree().node(id).unwrap().props.style.clone();
                let width = Length::Px(settings.width * scale);
                if style.width != width {
                    style.width = width;
                    mutations.push(Mutation::SetStyle { node: id, style });
                }
            }
        }
        if !mutations.is_empty() {
            self.runtime.apply(Mutation::Batch(mutations))?;
        }
        Ok(())
    }
}
