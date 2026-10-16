//! Retained groups and disclosure panels with theme-relative chrome.
use super::*;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExpandDirection {
    #[default]
    Down,
    Up,
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpanderAnimation {
    pub duration: Duration,
}
impl Default for ExpanderAnimation {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(160),
        }
    }
}
#[derive(Clone)]
struct ExpansionTransition {
    started: Instant,
    deadline: Instant,
    duration: Duration,
    size: f32,
    original_style: LayoutStyle,
    opening: bool,
}

/// Decoration shared by static groups and expandable sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupAppearance {
    #[default]
    Card,
    Outline,
    Plain,
    /// The caption interrupts the top edge of an otherwise transparent frame.
    Standard,
}

/// Presentation of the retained group header, independent of its outer frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupHeaderStyle {
    #[default]
    Normal,
    /// A compact 36-pixel header with lighter typography.
    Light,
    /// A theme-relative highlighted header.
    Accent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupSettings {
    /// Plain text; at most 4096 UTF-8 bytes. Long captions are clipped.
    pub header: String,
    pub appearance: GroupAppearance,
    pub header_style: GroupHeaderStyle,
    /// A decorative symbol from the built-in vector icon set.
    pub header_icon: Option<IconSymbol>,
    /// Only static groups may hide their header.
    pub show_header: bool,
    /// `None` follows the theme. Explicit values use logical pixels.
    pub corner_radius: Option<f32>,
    pub header_foreground: Option<Color>,
    pub header_background: Option<Color>,
    /// Insets of the retained content host, independent of header spacing.
    pub padding: Edges,
    pub expand_direction: ExpandDirection,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            header: String::new(),
            appearance: GroupAppearance::Card,
            header_style: GroupHeaderStyle::Normal,
            header_icon: None,
            show_header: true,
            corner_radius: None,
            header_foreground: None,
            header_background: None,
            padding: Edges::all(16.0),
            expand_direction: ExpandDirection::Down,
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
        if self
            .corner_radius
            .is_some_and(|value| !value.is_finite() || !(0.0..=16_384.0).contains(&value))
        {
            return Err(ControlError::Invalid(
                "Group CornerRadius must be finite and in 0..=16384".into(),
            ));
        }
        for color in [self.header_foreground, self.header_background]
            .into_iter()
            .flatten()
        {
            if [color.r, color.g, color.b, color.a]
                .into_iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            {
                return Err(ControlError::Invalid(
                    "Group header colors must have finite channels in 0..=1".into(),
                ));
            }
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

    fn header_height(&self) -> f32 {
        if self.header_style == GroupHeaderStyle::Light {
            36.0
        } else {
            48.0
        }
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
    animation: Option<ExpanderAnimation>,
    transition: Option<ExpansionTransition>,
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

    /// A GroupBox with the same retained disclosure lifecycle as an Expander.
    pub fn collapsible_group_box(
        &mut self,
        parent: WidgetId,
        settings: GroupSettings,
        is_expanded: bool,
        style: LayoutStyle,
    ) -> Result<GroupBox, ControlError> {
        self.add_group(parent, settings, Some(is_expanded), style)
    }

    fn add_group(
        &mut self,
        parent: WidgetId,
        settings: GroupSettings,
        expanded: Option<bool>,
        mut style: LayoutStyle,
    ) -> Result<GroupBox, ControlError> {
        settings.validate()?;
        if expanded.is_some() && !settings.show_header {
            return Err(ControlError::Invalid(
                "A collapsible group must show its header".into(),
            ));
        }
        style.kind = LayoutKind::Flex(Axis::Vertical);
        style.padding = Edges::default();
        style.gap = 0.0;
        let container = self.panel(parent, style)?;
        self.set_role(container, Role::Group);
        self.controls.get_mut(&container).unwrap().label = settings.header.clone();
        self.set_surface(container, SurfaceKind::Transparent)?;
        let header_style = LayoutStyle {
            height: Length::Px(settings.header_height()),
            flex_shrink: 0.0,
            ..Default::default()
        };
        let header = if expanded.is_some() {
            self.button(container, &settings.header, None, header_style)?
        } else {
            self.label(container, &settings.header, header_style)?
        };
        if !settings.show_header {
            self.edit_props(header, |props| props.collapsed = true)?;
        }
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
                animation: None,
                transition: None,
            },
        );
        self.apply_expand_direction(container)?;
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
        if group.expanded.is_some() && !settings.show_header {
            return Err(ControlError::Invalid(
                "A collapsible group must show its header".into(),
            ));
        }
        if group.settings == settings {
            return Ok(());
        }
        let parts = group.parts;
        self.finish_expander_transition(owner)?;
        self.edit_props(parts.header, |props| {
            props.style.height = Length::Px(settings.header_height());
            props.collapsed = !settings.show_header;
        })?;
        self.edit_props(parts.content, |props| {
            props.style.padding = settings.padding
        })?;
        self.controls.get_mut(&owner).unwrap().label = settings.header.clone();
        self.controls.get_mut(&parts.header).unwrap().label = settings.header.clone();
        self.groups.get_mut(&owner).unwrap().settings = settings;
        self.apply_expand_direction(owner)?;
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
        self.finish_expander_transition(owner)?;
        let group = &self.groups[&owner];
        let previous = group
            .expanded
            .ok_or_else(|| ControlError::Invalid("GroupBox cannot be expanded".into()))?;
        if previous == expanded {
            return Ok(());
        }
        let parts = group.parts;
        let animation = group
            .animation
            .filter(|_| !self.reduced_motion() && self.window_focused);
        let original_style = self
            .runtime
            .tree()
            .node(parts.content)
            .unwrap()
            .props
            .style
            .clone();
        let old_size = self.runtime.bounds(parts.content).map(|bounds| {
            if matches!(
                group.settings.expand_direction,
                ExpandDirection::Left | ExpandDirection::Right
            ) {
                bounds.width
            } else {
                bounds.height
            }
        });
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
            self.sync_flyouts()?;
            if focus_within && self.is_enabled(parts.header) && self.window_focused {
                self.runtime.apply(Mutation::Focus(Some(parts.header)))?;
            }
        }
        if let Some(animation) = animation {
            // Measure an opening subtree once before shrinking its layout box.
            // The retained widgets are never rebuilt and interaction is paused
            // until the transition completes.
            self.runtime.update(&mut self.layout, self.viewport)?;
            let horizontal = matches!(
                self.groups[&owner].settings.expand_direction,
                ExpandDirection::Left | ExpandDirection::Right
            );
            let size = if expanded {
                self.runtime.bounds(parts.content).map(|bounds| {
                    if horizontal {
                        bounds.width
                    } else {
                        bounds.height
                    }
                })
            } else {
                old_size
            };
            if let Some(size) = size.filter(|size| *size > 0.0) {
                let started = Instant::now();
                self.edit_props(parts.content, |props| {
                    props.collapsed = false;
                    props.style.flex_grow = 0.0;
                    props.style.flex_shrink = 0.0;
                    if horizontal {
                        props.style.width = Length::Px(if expanded { 0.0 } else { size });
                    } else {
                        props.style.height = Length::Px(if expanded { 0.0 } else { size });
                    }
                })?;
                self.groups.get_mut(&owner).unwrap().transition = Some(ExpansionTransition {
                    started,
                    deadline: started + Duration::from_millis(16),
                    duration: animation.duration,
                    size,
                    original_style,
                    opening: expanded,
                });
            }
        }
        self.scene_dirty = true;
        Ok(())
    }

    /// Enables bounded reveal/contraction transitions. `None` is immediate.
    /// Reduced-motion mode and inactive windows always finish immediately.
    pub fn set_expander_animation(
        &mut self,
        id: WidgetId,
        animation: Option<ExpanderAnimation>,
    ) -> Result<(), ControlError> {
        let owner = self
            .group_owner(id)
            .filter(|owner| self.groups[owner].expanded.is_some())
            .ok_or_else(|| ControlError::Invalid("Unknown expander".into()))?;
        if animation.is_some_and(|value| {
            !(Duration::from_millis(16)..=Duration::from_secs(2)).contains(&value.duration)
        }) {
            return Err(ControlError::Invalid(
                "Expander animation duration must be 16..2000ms".into(),
            ));
        }
        self.finish_expander_transition(owner)?;
        self.groups.get_mut(&owner).unwrap().animation = animation;
        Ok(())
    }
    pub fn expander_deadline(&self) -> Option<Instant> {
        self.groups
            .values()
            .filter_map(|group| {
                group
                    .transition
                    .as_ref()
                    .map(|transition| transition.deadline)
            })
            .min()
    }
    pub fn expander_tick(&mut self, now: Instant) -> Result<bool, ControlError> {
        let active: Vec<_> = self
            .groups
            .iter()
            .filter_map(|(id, group)| {
                group.transition.clone().map(|transition| {
                    (
                        *id,
                        group.parts,
                        group.settings.expand_direction,
                        transition,
                    )
                })
            })
            .collect();
        let mut changed = false;
        for (owner, parts, direction, transition) in active {
            if self.reduced_motion()
                || !self.window_focused
                || now.saturating_duration_since(transition.started) >= transition.duration
            {
                self.finish_expander_transition(owner)?;
                changed = true;
                continue;
            }
            if now < transition.deadline {
                continue;
            }
            let fraction = now
                .saturating_duration_since(transition.started)
                .as_secs_f32()
                / transition.duration.as_secs_f32();
            let smooth = fraction * fraction * (3.0 - 2.0 * fraction);
            let size = transition.size
                * if transition.opening {
                    smooth
                } else {
                    1.0 - smooth
                };
            self.edit_props(parts.content, |props| {
                if matches!(direction, ExpandDirection::Left | ExpandDirection::Right) {
                    props.style.width = Length::Px(size);
                } else {
                    props.style.height = Length::Px(size);
                }
            })?;
            self.groups
                .get_mut(&owner)
                .unwrap()
                .transition
                .as_mut()
                .unwrap()
                .deadline = now + Duration::from_millis(16);
            changed = true;
        }
        if changed {
            self.scene_dirty = true;
        }
        Ok(changed)
    }
    pub(super) fn sync_expander_motion(&mut self) -> Result<(), ControlError> {
        if self.reduced_motion() || !self.window_focused {
            let owners: Vec<_> = self
                .groups
                .iter()
                .filter_map(|(id, group)| group.transition.is_some().then_some(*id))
                .collect();
            for owner in owners {
                self.finish_expander_transition(owner)?;
            }
        }
        Ok(())
    }
    pub(super) fn expander_transition_blocks(&self, id: WidgetId) -> bool {
        self.groups.values().any(|group| {
            group.transition.is_some() && self.group_descendant(id, group.parts.content)
        })
    }
    fn finish_expander_transition(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let Some(group) = self.groups.get_mut(&owner) else {
            return Ok(());
        };
        let Some(transition) = group.transition.take() else {
            return Ok(());
        };
        let content = group.parts.content;
        if self.runtime.tree().contains(content) {
            self.edit_props(content, |props| {
                props.style = transition.original_style;
                props.collapsed = !transition.opening;
            })?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    fn apply_expand_direction(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let group = &self.groups[&owner];
        let parts = group.parts;
        let direction = group.settings.expand_direction;
        let horizontal = matches!(direction, ExpandDirection::Left | ExpandDirection::Right);
        let extent = group.settings.header_height();
        self.edit_props(owner, |props| {
            props.style.kind = LayoutKind::Flex(if horizontal {
                Axis::Horizontal
            } else {
                Axis::Vertical
            })
        })?;
        self.edit_props(parts.header, |props| {
            props.style.width = if horizontal {
                Length::Px(120.0)
            } else {
                Length::Auto
            };
            props.style.height = if horizontal {
                Length::Percent(1.0)
            } else {
                Length::Px(extent)
            };
        })?;
        let [first, last] = if matches!(direction, ExpandDirection::Up | ExpandDirection::Left) {
            [parts.content, parts.header]
        } else {
            [parts.header, parts.content]
        };
        if self.runtime.tree().children(owner) != Some([first, last].as_slice()) {
            self.runtime.apply(Mutation::Batch(vec![
                Mutation::Reparent {
                    node: first,
                    parent: owner,
                },
                Mutation::Reparent {
                    node: last,
                    parent: owner,
                },
            ]))?;
        }
        Ok(())
    }
    /// A noninteractive retained header subtree; disclosure/focus remains on the
    /// original header. Header text continues to name the accessible group.
    pub fn set_group_header_content(
        &mut self,
        id: WidgetId,
        content: Option<WidgetId>,
    ) -> Result<(), ControlError> {
        let owner = self
            .group_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown group".into()))?;
        let header = self.groups[&owner].parts.header;
        self.set_button_content(header, content)?;
        self.edit_props(header, |props| {
            props.style.padding.left = 16.0;
            props.style.padding.right = 46.0;
        })?;
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
        let settings = group.settings.clone();
        let expanded = group.expanded;
        let header_id = group.parts.header;
        let chrome = self.theme.chrome();
        let radius =
            settings
                .corner_radius
                .unwrap_or(if self.theme.is_fluent() { 8.0 } else { 0.0 });
        if id == owner {
            if settings.appearance == GroupAppearance::Card {
                editor_fill(&mut self.scene, bounds, clip, radius, chrome.card_surface);
            }
            if settings.appearance == GroupAppearance::Standard
                && settings.expand_direction == ExpandDirection::Down
                && settings.show_header
                && let Some(header) = self.runtime.bounds(header_id)
            {
                let (_, caption_end) =
                    self.group_caption(&settings, header, clip, expanded, true)?;
                let top = (header.y + header.height / 2.0).min(bounds.y + bounds.height);
                let frame = Rect::new(bounds.x, top, bounds.width, bounds.y + bounds.height - top);
                let gap_start = (header.x + 12.0).min(bounds.x + bounds.width);
                let gap_end = caption_end.max(gap_start).min(bounds.x + bounds.width);
                // Three disjoint clips leave an actual transparent caption gap.
                // No assumed ancestor background is painted over the frame.
                for region in [
                    Rect::new(bounds.x, top, gap_start - bounds.x, frame.height),
                    Rect::new(
                        gap_end,
                        top,
                        bounds.x + bounds.width - gap_end,
                        frame.height,
                    ),
                    Rect::new(
                        gap_start,
                        top + 1.0,
                        gap_end - gap_start,
                        (frame.height - 1.0).max(0.0),
                    ),
                ] {
                    if let Some(region) = region.intersection(clip) {
                        shape_border(
                            &mut self.scene,
                            frame,
                            region,
                            radius,
                            1.0,
                            chrome.surface_border,
                        );
                    }
                }
            } else if settings.appearance != GroupAppearance::Plain {
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
        let tokens = self.theme.tokens();
        let color = if enabled {
            settings.header_foreground.unwrap_or(tokens.text)
        } else {
            chrome.disabled_text
        };
        let standard = settings.appearance == GroupAppearance::Standard
            && settings.expand_direction == ExpandDirection::Down;
        let (run, caption_end) = self.group_caption(&settings, bounds, clip, expanded, enabled)?;
        let header_surface = if standard {
            Rect::new(
                bounds.x + 12.0_f32.min(bounds.width),
                bounds.y + (bounds.height - 28.0).max(0.0) / 2.0,
                (caption_end - bounds.x - 12.0).max(0.0),
                28.0_f32.min(bounds.height),
            )
        } else {
            inset(bounds, 2.0)
        };
        if let Some(background) = settings.header_background.or_else(|| {
            (settings.header_style == GroupHeaderStyle::Accent).then_some(tokens.selected)
        }) {
            editor_fill(
                &mut self.scene,
                header_surface,
                clip,
                if standard {
                    4.0_f32.min(radius)
                } else {
                    (radius - 2.0).max(0.0)
                },
                background,
            );
        }
        if settings.header_style == GroupHeaderStyle::Accent {
            fill_clipped(
                &mut self.scene,
                Rect::new(
                    header_surface.x,
                    header_surface.y + 8.0,
                    3.0_f32.min(header_surface.width),
                    (header_surface.height - 16.0).max(0.0),
                ),
                clip,
                if enabled {
                    tokens.focus
                } else {
                    chrome.disabled_text
                },
            );
        }
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
                    header_surface,
                    clip,
                    (radius - 2.0).max(0.0),
                    fill,
                );
            }
            if self.runtime.focused() == Some(id) {
                shape_border(
                    &mut self.scene,
                    inset(header_surface, 1.0),
                    clip,
                    4.0,
                    2.0,
                    tokens.focus,
                );
            }
        }
        if matches!(
            settings.appearance,
            GroupAppearance::Card | GroupAppearance::Outline
        ) && expanded != Some(false)
        {
            let divider = match settings.expand_direction {
                ExpandDirection::Down => Rect::new(
                    bounds.x + 1.0,
                    bounds.y + bounds.height - 1.0,
                    (bounds.width - 2.0).max(0.0),
                    1.0,
                ),
                ExpandDirection::Up => {
                    Rect::new(bounds.x + 1.0, bounds.y, (bounds.width - 2.0).max(0.0), 1.0)
                }
                ExpandDirection::Right => Rect::new(
                    bounds.x + bounds.width - 1.0,
                    bounds.y + 1.0,
                    1.0,
                    (bounds.height - 2.0).max(0.0),
                ),
                ExpandDirection::Left => Rect::new(
                    bounds.x,
                    bounds.y + 1.0,
                    1.0,
                    (bounds.height - 2.0).max(0.0),
                ),
            };
            fill_clipped(&mut self.scene, divider, clip, chrome.divider);
        }
        let custom_header = self.button_content(id).is_some();
        if !custom_header {
            self.scene.text(run);
        }
        let glyph_clip = bounds
            .intersection(clip)
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        if let Some(icon) = settings.header_icon.filter(|_| !custom_header) {
            let start = 16.0
                + if standard && expanded.is_some() {
                    24.0
                } else {
                    0.0
                };
            crate::primitives::paint_icon(
                &mut self.scene,
                icon,
                Rect::new(
                    bounds.x + start,
                    bounds.y + (bounds.height - 16.0) / 2.0,
                    16.0,
                    16.0,
                ),
                glyph_clip,
                color,
            );
        }
        if let Some(expanded) = expanded {
            let symbol = if expanded {
                IconSymbol::ChevronDown
            } else {
                IconSymbol::ChevronRight
            };
            let arrow = Rect::new(
                bounds.x
                    + if standard {
                        16.0
                    } else {
                        (bounds.width - 32.0).max(0.0)
                    },
                bounds.y + (bounds.height - 16.0) / 2.0,
                16.0,
                16.0,
            );
            if expanded
                && matches!(
                    settings.expand_direction,
                    ExpandDirection::Up | ExpandDirection::Left
                )
            {
                use rust_desktop_ui_core::StrokeSegment;
                let points = if settings.expand_direction == ExpandDirection::Up {
                    [(3.0, 10.0), (8.0, 5.0), (13.0, 10.0)]
                } else {
                    [(10.0, 3.0), (5.0, 8.0), (10.0, 13.0)]
                };
                let point = |(x, y): (f32, f32)| Point::new(arrow.x + x, arrow.y + y);
                self.scene.stroke_path(
                    &[
                        StrokeSegment {
                            start: point(points[0]),
                            end: point(points[1]),
                        },
                        StrokeSegment {
                            start: point(points[1]),
                            end: point(points[2]),
                        },
                    ],
                    glyph_clip,
                    1.5,
                    color,
                );
            } else if expanded && settings.expand_direction == ExpandDirection::Right {
                crate::primitives::paint_icon(
                    &mut self.scene,
                    IconSymbol::ChevronRight,
                    arrow,
                    glyph_clip,
                    color,
                );
            } else {
                crate::primitives::paint_icon(&mut self.scene, symbol, arrow, glyph_clip, color);
            }
        }
        Ok(true)
    }

    fn group_caption(
        &mut self,
        settings: &GroupSettings,
        bounds: Rect,
        clip: Rect,
        expanded: Option<bool>,
        enabled: bool,
    ) -> Result<(TextRun, f32), ControlError> {
        let standard = settings.appearance == GroupAppearance::Standard
            && settings.expand_direction == ExpandDirection::Down;
        let leading =
            16.0 + if settings.header_icon.is_some() {
                24.0
            } else {
                0.0
            } + if standard && expanded.is_some() {
                24.0
            } else {
                0.0
            };
        let trailing = if expanded.is_some() && !standard {
            46.0
        } else {
            16.0
        };
        let text_bounds = Rect::new(
            bounds.x + leading,
            bounds.y,
            (bounds.width - leading - trailing).max(0.0),
            bounds.height,
        );
        let tokens = self.theme.tokens();
        let mut run = TextRun {
            bounds: text_bounds,
            clip: text_bounds
                .intersection(clip)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            text: settings
                .header
                .chars()
                .map(|c| if single_line_separator(c) { ' ' } else { c })
                .collect(),
            font_size: tokens.font_size,
            color: if enabled {
                settings.header_foreground.unwrap_or(tokens.text)
            } else {
                self.theme.chrome().disabled_text
            },
            weight: if settings.header_style == GroupHeaderStyle::Light {
                500
            } else {
                600
            },
            align: TextAlign::Start,
            wrap: false,
        };
        let prepared = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        run.bounds.y += ((bounds.height - prepared.height) / 2.0).max(0.0);
        let caption_end = (text_bounds.x + prepared.width.min(text_bounds.width) + 8.0)
            .min(bounds.x + (bounds.width - 12.0).max(0.0));
        Ok((run, caption_end))
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
