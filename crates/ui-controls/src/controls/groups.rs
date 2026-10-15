//! Retained groups and disclosure panels with theme-relative chrome.
use super::*;

/// Decoration shared by static groups and expandable sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupAppearance {
    #[default]
    Card,
    Outline,
    Plain,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupSettings {
    /// Plain text; at most 4096 UTF-8 bytes. Long captions are clipped.
    pub header: String,
    pub appearance: GroupAppearance,
    /// Insets of the retained content host, independent of header spacing.
    pub padding: Edges,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            header: String::new(),
            appearance: GroupAppearance::Card,
            padding: Edges::all(16.0),
        }
    }
}

impl GroupSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.header.len() > 4096 {
            return Err(ControlError::Invalid(
                "Group Header exceeds 4096 UTF-8 bytes".into(),
            ));
        }
        if [
            self.padding.left,
            self.padding.right,
            self.padding.top,
            self.padding.bottom,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=16_384.0).contains(&value))
        {
            return Err(ControlError::Invalid(
                "Group Padding must be finite and in 0..=16384".into(),
            ));
        }
        Ok(())
    }
}

/// The header and content host belong to this group and cannot be removed alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupBox {
    pub container: WidgetId,
    pub header: WidgetId,
    pub content: WidgetId,
}

/// Expansion hides the content host without rebuilding its descendants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expander {
    pub container: WidgetId,
    pub header: WidgetId,
    pub content: WidgetId,
}

pub(super) struct GroupState {
    parts: GroupBox,
    settings: GroupSettings,
    expanded: Option<bool>,
}

impl Controls {
    pub fn group_box(
        &mut self,
        parent: WidgetId,
        settings: GroupSettings,
        style: LayoutStyle,
    ) -> Result<GroupBox, ControlError> {
        self.add_group(parent, settings, None, style)
    }

    pub fn expander(
        &mut self,
        parent: WidgetId,
        settings: GroupSettings,
        is_expanded: bool,
        style: LayoutStyle,
    ) -> Result<Expander, ControlError> {
        let parts = self.add_group(parent, settings, Some(is_expanded), style)?;
        Ok(Expander {
            container: parts.container,
            header: parts.header,
            content: parts.content,
        })
    }

    fn add_group(
        &mut self,
        parent: WidgetId,
        settings: GroupSettings,
        expanded: Option<bool>,
        mut style: LayoutStyle,
    ) -> Result<GroupBox, ControlError> {
        settings.validate()?;
        style.kind = LayoutKind::Flex(Axis::Vertical);
        style.padding = Edges::default();
        style.gap = 0.0;
        let container = self.panel(parent, style)?;
        self.set_role(container, Role::Group);
        self.controls.get_mut(&container).unwrap().label = settings.header.clone();
        self.set_surface(container, SurfaceKind::Transparent)?;
        let header_style = LayoutStyle {
            height: Length::Px(48.0),
            flex_shrink: 0.0,
            ..Default::default()
        };
        let header = if expanded.is_some() {
            self.button(container, &settings.header, None, header_style)?
        } else {
            self.label(container, &settings.header, header_style)?
        };
        let content = self.panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                padding: settings.padding,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                ..Default::default()
            },
        )?;
        self.set_surface(content, SurfaceKind::Transparent)?;
        if expanded == Some(false) {
            self.edit_props(content, |props| props.collapsed = true)?;
        }
        let parts = GroupBox {
            container,
            header,
            content,
        };
        self.groups.insert(
            container,
            GroupState {
                parts,
                settings,
                expanded,
            },
        );
        Ok(parts)
    }

    pub(crate) fn group_owner(&self, id: WidgetId) -> Option<WidgetId> {
        self.groups.get(&id).map(|_| id).or_else(|| {
            self.groups
                .iter()
                .find_map(|(&owner, group)| (group.parts.header == id).then_some(owner))
        })
    }

    /// Settings can be queried through the container or its header.
    pub fn group_settings(&self, id: WidgetId) -> Option<&GroupSettings> {
        let owner = self.group_owner(id)?;
        self.runtime.tree().node(owner)?;
        Some(&self.groups[&owner].settings)
    }

    pub fn group_header(&self, id: WidgetId) -> Option<&str> {
        Some(&self.group_settings(id)?.header)
    }

    pub fn set_group_header(
        &mut self,
        id: WidgetId,
        header: impl Into<String>,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .group_settings(id)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Unknown group".into()))?;
        settings.header = header.into();
        self.set_group_settings(id, settings)
    }

    /// Validation precedes changes to geometry, labels and theme-relative chrome.
    pub fn set_group_settings(
        &mut self,
        id: WidgetId,
        settings: GroupSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let owner = self
            .group_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown group".into()))?;
        let group = &self.groups[&owner];
        if group.settings == settings {
            return Ok(());
        }
        let parts = group.parts;
        self.edit_props(parts.content, |props| {
            props.style.padding = settings.padding
        })?;
        self.controls.get_mut(&owner).unwrap().label = settings.header.clone();
        self.controls.get_mut(&parts.header).unwrap().label = settings.header.clone();
        self.groups.get_mut(&owner).unwrap().settings = settings;
        self.scene_dirty = true;
        Ok(())
    }

    /// `None` distinguishes a static GroupBox or an unrelated control.
    pub fn expander_expanded(&self, id: WidgetId) -> Option<bool> {
        let owner = self.group_owner(id)?;
        self.runtime.tree().node(owner)?;
        self.groups[&owner].expanded
    }

    pub fn set_expander_expanded(
        &mut self,
        id: WidgetId,
        expanded: bool,
    ) -> Result<(), ControlError> {
        let owner = self
            .group_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown expander".into()))?;
        let group = &self.groups[&owner];
        let previous = group
            .expanded
            .ok_or_else(|| ControlError::Invalid("GroupBox cannot be expanded".into()))?;
        if previous == expanded {
            return Ok(());
        }
        let parts = group.parts;
        let focus_within = self.runtime.focused().is_some_and(|focused| {
            self.group_descendant(focused, parts.content)
                || self.rich.popup.as_ref().is_some_and(|popup| {
                    self.group_descendant(popup.owner, parts.content)
                        && self.group_descendant(focused, popup.surface)
                })
                || (self.group_popup_focused()
                    && self
                        .popup_anchor()
                        .is_some_and(|anchor| self.group_descendant(anchor, parts.content)))
        });
        let capture_within = self
            .runtime
            .captured()
            .is_some_and(|captured| self.group_descendant(captured, parts.content));
        self.edit_props(parts.content, |props| props.collapsed = !expanded)?;
        self.groups.get_mut(&owner).unwrap().expanded = Some(expanded);
        if !expanded {
            if capture_within {
                self.scroll_drag = None;
            }
            if self
                .rich
                .popup
                .as_ref()
                .is_some_and(|popup| self.group_descendant(popup.owner, parts.content))
            {
                self.close_rich_popup(false)?;
            }
            self.sync_popup_availability()?;
            self.synchronize_editor_focus()?;
            self.cancel_inactive_preedit();
            if self
                .pressed
                .is_some_and(|pressed| self.group_descendant(pressed, parts.content))
            {
                self.pressed = None;
                self.keyboard_pressed = false;
            }
            if self
                .selecting
                .is_some_and(|selecting| self.group_descendant(selecting, parts.content))
            {
                self.selecting = None;
            }
            self.tooltip_sync();
            if focus_within && self.is_enabled(parts.header) && self.window_focused {
                self.runtime.apply(Mutation::Focus(Some(parts.header)))?;
            }
        }
        self.scene_dirty = true;
        Ok(())
    }

    fn group_descendant(&self, mut id: WidgetId, ancestor: WidgetId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            let Some(parent) = self.runtime.tree().parent(id) else {
                return false;
            };
            id = parent;
        }
    }

    pub(super) fn preflight_group_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self
            .groups
            .values()
            .any(|group| group.parts.content == id || group.parts.header == id)
        {
            return Err(ControlError::Invalid("Required group parts cannot be removed individually; remove their container or ancestor".into()));
        }
        Ok(())
    }

    pub(super) fn activate_group_header(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some(owner) = self.group_owner(id) else {
            return Ok(false);
        };
        let group = &self.groups[&owner];
        let Some(expanded) = group.expanded else {
            return Ok(false);
        };
        if group.parts.header != id {
            return Ok(false);
        }
        self.set_expander_expanded(owner, !expanded)?;
        self.activated.push(id);
        Ok(true)
    }

    pub(super) fn group_automation_name(&self, id: WidgetId) -> Option<&str> {
        let owner = self.group_owner(id)?;
        // A specific header override takes precedence over the group's name.
        self.controls.get(&id)?.automation_name.as_deref().or(self
            .controls
            .get(&owner)?
            .automation_name
            .as_deref())
    }

    pub(super) fn paint_group(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(owner) = self.group_owner(id) else {
            return Ok(false);
        };
        let group = &self.groups[&owner];
        let chrome = self.theme.chrome();
        let radius = if self.theme.is_fluent() { 8.0 } else { 0.0 };
        if id == owner {
            if group.settings.appearance == GroupAppearance::Card {
                editor_fill(&mut self.scene, bounds, clip, radius, chrome.card_surface);
            }
            if group.settings.appearance != GroupAppearance::Plain {
                shape_border(
                    &mut self.scene,
                    bounds,
                    clip,
                    radius,
                    1.0,
                    chrome.surface_border,
                );
            }
            return Ok(true);
        }
        let enabled = self.is_enabled(id);
        let expanded = group.expanded;
        let appearance = group.settings.appearance;
        let label = group
            .settings
            .header
            .chars()
            .map(|character| {
                if single_line_separator(character) {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>();
        let tokens = self.theme.tokens();
        let color = if enabled {
            tokens.text
        } else {
            chrome.disabled_text
        };
        if expanded.is_some() && enabled {
            let fill = if self.pressed == Some(id) {
                Some(chrome.subtle_pressed)
            } else if self.runtime.hovered() == Some(id) {
                Some(chrome.subtle_hover)
            } else {
                None
            };
            if let Some(fill) = fill {
                editor_fill(
                    &mut self.scene,
                    inset(bounds, 2.0),
                    clip,
                    (radius - 2.0).max(0.0),
                    fill,
                );
            }
            if self.runtime.focused() == Some(id) {
                shape_border(
                    &mut self.scene,
                    inset(bounds, 3.0),
                    clip,
                    4.0,
                    2.0,
                    tokens.focus,
                );
            }
        }
        if appearance != GroupAppearance::Plain && expanded != Some(false) {
            fill_clipped(
                &mut self.scene,
                Rect::new(
                    bounds.x + 1.0,
                    bounds.y + bounds.height - 1.0,
                    (bounds.width - 2.0).max(0.0),
                    1.0,
                ),
                clip,
                chrome.divider,
            );
        }
        let end_inset = if expanded.is_some() { 46.0 } else { 16.0 };
        let text_bounds = Rect::new(
            bounds.x + 16.0,
            bounds.y,
            (bounds.width - 16.0 - end_inset).max(0.0),
            bounds.height,
        );
        let mut run = TextRun {
            bounds: text_bounds,
            clip: text_bounds
                .intersection(clip)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            text: label,
            font_size: tokens.font_size,
            color,
            weight: 600,
            align: TextAlign::Start,
            wrap: false,
        };
        let prepared = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        run.bounds.y += ((bounds.height - prepared.height) / 2.0).max(0.0);
        self.scene.text(run);
        if let Some(expanded) = expanded {
            let symbol = if expanded {
                IconSymbol::ChevronDown
            } else {
                IconSymbol::ChevronRight
            };
            let arrow = Rect::new(
                bounds.x + (bounds.width - 32.0).max(0.0),
                bounds.y + (bounds.height - 16.0) / 2.0,
                16.0,
                16.0,
            );
            crate::primitives::paint_icon(&mut self.scene, symbol, arrow, clip, color);
        }
        Ok(true)
    }
}

fn inset(bounds: Rect, amount: f32) -> Rect {
    let x = amount.min(bounds.width / 2.0);
    let y = amount.min(bounds.height / 2.0);
    Rect::new(
        bounds.x + x,
        bounds.y + y,
        (bounds.width - x * 2.0).max(0.0),
        (bounds.height - y * 2.0).max(0.0),
    )
}
