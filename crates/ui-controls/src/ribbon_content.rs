//! Stable content roots survive command presenter reconstruction.
use super::*;
use crate::{Flyout, ScrollBarVisibility, ScrollSettings, ScrollViewer};
use rust_desktop_ui_core::{Align, Size};

/// A retained editor or custom composition appended after a group's commands.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonContentSpec {
    pub id: String,
    pub tab_id: String,
    pub group_id: String,
    pub label: String,
    pub key_tip: String,
    pub preferred_width: f32,
    pub minimum_width: f32,
    pub height: f32,
}

#[derive(Default)]
pub(super) struct RetainedContent {
    pub specs: Vec<RibbonContentSpec>,
    pub parking: Option<WidgetId>,
    hosts: HashMap<String, WidgetId>,
    focus_targets: HashMap<String, WidgetId>,
    overlay: Option<(Flyout, ScrollViewer)>,
    opened: Option<(String, Vec<String>)>,
    mounting_overlay: bool,
}

pub(super) fn descendant(ui: &Controls, id: WidgetId, root: WidgetId) -> bool {
    std::iter::successors(Some(id), |id| ui.runtime.tree().parent(*id)).any(|id| id == root)
}
pub(super) fn collapsed(ui: &mut Controls, id: WidgetId, value: bool) -> Result<(), ControlError> {
    let mut props = ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| ControlError::Invalid("Missing retained Ribbon host".into()))?
        .props
        .clone();
    if props.collapsed != value {
        props.collapsed = value;
        ui.runtime.apply(Mutation::SetProps { node: id, props })?;
    }
    Ok(())
}
pub(super) fn stable_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}

impl Ribbon {
    /// Registers a bounded slot before mounting. Commands and slots have separate
    /// identities; KeyTips share the owning tab's prefix-free namespace.
    pub fn add_content(&mut self, mut spec: RibbonContentSpec) -> Result<(), ControlError> {
        spec.key_tip = spec.key_tip.to_uppercase();
        let group_exists =
            self.model.tabs.iter().any(|tab| {
                tab.id == spec.tab_id && tab.groups.iter().any(|g| g.id == spec.group_id)
            });
        let mut tips: Vec<_> = self
            .model
            .tabs
            .iter()
            .filter(|tab| tab.id == spec.tab_id)
            .flat_map(|tab| &tab.groups)
            .flat_map(|g| &g.items)
            .map(|item| item.key_tip.clone())
            .collect();
        tips.extend(
            self.retained
                .specs
                .iter()
                .filter(|s| s.tab_id == spec.tab_id)
                .map(|s| s.key_tip.clone()),
        );
        if self.container.is_some()
            || self.retained.specs.len() >= 64
            || !stable_id(&spec.id)
            || self.retained.specs.iter().any(|s| s.id == spec.id)
            || !group_exists
            || spec.label.is_empty()
            || spec.label.len() > 4096
            || spec.key_tip.len() > 32
            || !valid_key_tip(&spec.key_tip, &tips)
            || !spec.preferred_width.is_finite()
            || !spec.minimum_width.is_finite()
            || !(24.0..=4096.0).contains(&spec.preferred_width)
            || !(24.0..=spec.preferred_width).contains(&spec.minimum_width)
            || !spec.height.is_finite()
            || !(24.0..=512.0).contains(&spec.height)
        {
            return Err(ControlError::Invalid(
                "Invalid Ribbon content slot, size, identity, KeyTip or registration lifecycle"
                    .into(),
            ));
        }
        self.retained.specs.push(spec);
        self.dirty = true;
        Ok(())
    }
    /// Stable host available after mount. Mount the ordinary control subtree once.
    pub fn content_host(&self, id: &str) -> Option<WidgetId> {
        self.retained.hosts.get(id).copied()
    }
    pub fn set_content_focus_target(
        &mut self,
        ui: &Controls,
        id: &str,
        target: Option<WidgetId>,
    ) -> Result<(), ControlError> {
        let host = self
            .content_host(id)
            .ok_or_else(|| ControlError::Invalid("Unknown Ribbon content host".into()))?;
        if let Some(target) = target {
            if !descendant(ui, target, host)
                || !ui
                    .runtime
                    .tree()
                    .node(target)
                    .is_some_and(|n| n.props.focusable)
            {
                return Err(ControlError::Invalid(
                    "Ribbon focus target must be a focusable descendant".into(),
                ));
            }
            self.retained.focus_targets.insert(id.into(), target);
        } else {
            self.retained.focus_targets.remove(id);
        }
        Ok(())
    }
    pub(super) fn validate_retained_tree(&self, ui: &Controls) -> Result<(), ControlError> {
        let Some(owner) = self.container else {
            return Ok(());
        };
        let mut ids = vec![owner];
        ids.extend(self.retained.parking);
        ids.extend(self.retained.hosts.values().copied());
        if let Some((flyout, viewer)) = self.retained.overlay {
            ids.extend([
                flyout.container,
                flyout.content,
                viewer.viewport,
                viewer.content,
            ]);
        }
        if ids.iter().any(|id| !ui.runtime.tree().contains(*id)) {
            return Err(ControlError::Invalid(
                "A required Ribbon part was removed; remove its owner and create a new Ribbon"
                    .into(),
            ));
        }
        self.validate_backstage_tree(ui)
    }
    pub(super) fn preflight_retained(&self, ui: &mut Controls) -> Result<(), ControlError> {
        for slot in &self.retained.specs {
            ui.measure_text(&slot.label, 4096.0, false)?;
            ui.measure_text(&slot.key_tip, 4096.0, false)?;
        }
        for tab in &self.model.tabs {
            let mut tips: Vec<_> = tab
                .groups
                .iter()
                .flat_map(|g| &g.items)
                .map(|i| i.key_tip.clone())
                .collect();
            for slot in self
                .retained
                .specs
                .iter()
                .filter(|s| s.tab_id == tab.id && tab.groups.iter().any(|g| g.id == s.group_id))
            {
                if !valid_key_tip(&slot.key_tip, &tips) {
                    return Err(ControlError::Invalid(
                        "Merged Ribbon content KeyTip collision".into(),
                    ));
                }
                tips.push(slot.key_tip.clone());
            }
        }
        self.preflight_backstage(ui)
    }
    pub(super) fn retained_owns(&self, ui: &Controls, id: WidgetId) -> bool {
        self.retained
            .hosts
            .values()
            .any(|host| descendant(ui, id, *host))
            || self
                .retained
                .overlay
                .is_some_and(|(flyout, _)| descendant(ui, id, flyout.container))
            || self.backstage_owns(ui, id)
    }
    pub(super) fn retained_overlay_open(&self, ui: &Controls) -> bool {
        self.retained
            .overlay
            .is_some_and(|(f, _)| ui.flyout_is_open(f))
            || self.backstage_is_open(ui)
    }
    pub(super) fn mount_retained_roots(
        &mut self,
        ui: &mut Controls,
        container: WidgetId,
    ) -> Result<(), ControlError> {
        if self.retained.specs.is_empty() {
            return Ok(());
        }
        let parking = if let Some(id) = self.retained.parking {
            id
        } else {
            let id = ui.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    width: Length::Px(0.0),
                    height: Length::Px(0.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            self.retained.parking = Some(id);
            id
        };
        for slot in &self.retained.specs {
            if !self.retained.hosts.contains_key(&slot.id) {
                let host = ui.transparent_panel(
                    parking,
                    LayoutStyle {
                        kind: LayoutKind::Overlay,
                        width: Length::Px(slot.preferred_width),
                        height: Length::Px(slot.height),
                        flex_shrink: 0.0,
                        ..Default::default()
                    },
                )?;
                ui.set_role(host, Role::Group);
                ui.set_automation_name(host, &slot.label)?;
                collapsed(ui, host, true)?;
                self.retained.hosts.insert(slot.id.clone(), host);
            }
        }
        if self.retained.overlay.is_none() {
            let flyout = ui.flyout(container, Size::new(480.0, 180.0))?;
            ui.set_automation_name(flyout.container, "Ribbon group contents")?;
            let viewer = ui.scroll_view(
                flyout.content,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
                160.0,
            )?;
            ui.configure_scroll_view(
                viewer.viewport,
                ScrollSettings {
                    horizontal: ScrollBarVisibility::Auto,
                    vertical: ScrollBarVisibility::Auto,
                },
            )?;
            self.retained.overlay = Some((flyout, viewer));
        }
        Ok(())
    }
    pub(super) fn park_content(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let Some(parking) = self.retained.parking else {
            return Ok(());
        };
        if !ui.runtime.tree().contains(parking) {
            return Err(ControlError::Invalid(
                "Ribbon owner was removed; create a new Ribbon to remount".into(),
            ));
        }
        collapsed(ui, parking, false)?;
        let mutations: Vec<_> = self
            .retained
            .hosts
            .values()
            .filter(|host| ui.runtime.tree().contains(**host))
            .map(|&host| Mutation::Reparent {
                node: host,
                parent: parking,
            })
            .collect();
        ui.runtime.apply(Mutation::Batch(mutations))?;
        Ok(())
    }
    fn slot_in_overlay(&self, slot: &RibbonContentSpec) -> bool {
        self.retained
            .opened
            .as_ref()
            .is_some_and(|(tab, groups)| slot.tab_id == *tab && groups.contains(&slot.group_id))
    }
    pub(super) fn group_has_content(&self, group: &str) -> bool {
        self.retained
            .specs
            .iter()
            .any(|s| s.tab_id == self.active && s.group_id == group)
    }
    pub(super) fn mount_group_content(
        &mut self,
        ui: &mut Controls,
        row: WidgetId,
        group: &str,
        measured: &MeasuredGroup,
    ) -> Result<(), ControlError> {
        for (id, width) in &measured.contents {
            let slot = self.retained.specs.iter().find(|s| s.id == *id).unwrap();
            if slot.group_id != group
                || (!self.retained.mounting_overlay && self.slot_in_overlay(slot))
            {
                continue;
            }
            let host = self.retained.hosts[id];
            let style = LayoutStyle {
                kind: LayoutKind::Overlay,
                width: Length::Px(*width),
                height: Length::Px(slot.height),
                vertical_alignment: Some(Align::Center),
                flex_shrink: 0.0,
                ..Default::default()
            };
            ui.runtime.apply(Mutation::Batch(vec![
                Mutation::SetStyle { node: host, style },
                Mutation::Reparent {
                    node: host,
                    parent: row,
                },
            ]))?;
            collapsed(ui, host, false)?;
            self.tip_nodes
                .insert(host, (Tips::Commands, slot.key_tip.clone()));
        }
        Ok(())
    }
    pub(super) fn finish_content_layout(
        &mut self,
        ui: &mut Controls,
        metrics: RibbonMetrics,
        widths: &RibbonWidths,
    ) -> Result<(), ControlError> {
        if let Some((tab_id, groups)) = self.retained.opened.clone() {
            let eligible = tab_id == self.active
                && self.is_expanded()
                && self.visible_tabs().iter().any(|tab| tab.id == tab_id);
            if eligible {
                self.rebuild_content_overlay(ui, &groups, metrics, widths)?;
            } else {
                self.close_content_groups(ui)?;
            }
        }
        if let Some(parking) = self.retained.parking {
            for host in self.retained.hosts.values().copied().collect::<Vec<_>>() {
                if ui.runtime.tree().parent(host) == Some(parking) {
                    collapsed(ui, host, true)?;
                }
            }
            collapsed(ui, parking, true)?;
        }
        Ok(())
    }
    pub(super) fn sync_retained_lifecycle(
        &mut self,
        ui: &mut Controls,
    ) -> Result<(), ControlError> {
        if self.retained.opened.is_some()
            && self
                .retained
                .overlay
                .is_some_and(|(f, _)| !ui.flyout_is_open(f))
        {
            self.retained.opened = None;
            self.dirty = true;
        }
        self.sync_backstage_geometry(ui)
    }
    fn rebuild_content_overlay(
        &mut self,
        ui: &mut Controls,
        groups: &[String],
        metrics: RibbonMetrics,
        widths: &RibbonWidths,
    ) -> Result<(), ControlError> {
        let Some((flyout, viewer)) = self.retained.overlay else {
            return Ok(());
        };
        let tab = self
            .model
            .tabs
            .iter()
            .find(|t| t.id == self.active)
            .cloned()
            .unwrap();
        let groups: Vec<_> = tab
            .groups
            .iter()
            .filter(|g| groups.contains(&g.id))
            .cloned()
            .collect();
        if groups.is_empty() {
            self.close_content_groups(ui)?;
            return Ok(());
        }
        let old = ui
            .runtime
            .tree()
            .children(viewer.content)
            .unwrap_or_default()
            .to_vec();
        for child in old {
            ui.remove(child)?;
        }
        let width: f32 = groups
            .iter()
            .map(|g| widths.group(&tab.id, &g.id).width + 6.0)
            .sum();
        let height = metrics.groups;
        ui.runtime.apply(Mutation::SetStyle {
            node: viewer.content,
            style: LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Px(width),
                height: Length::Px(height),
                gap: 6.0,
                ..Default::default()
            },
        })?;
        ui.set_scroll_extent_size(viewer.viewport, Size::new(width, height))?;
        ui.set_flyout_placement(
            flyout,
            Size::new((width + 16.0).min(4096.0), (height + 16.0).min(1024.0)),
            crate::FlyoutPlacement::Below,
        )?;
        self.retained.mounting_overlay = true;
        let result = (|| {
            for group in &groups {
                self.mount_group(
                    ui,
                    viewer.content,
                    group,
                    metrics,
                    widths.group(&tab.id, &group.id),
                )?;
            }
            Ok(())
        })();
        self.retained.mounting_overlay = false;
        result
    }
    /// Opens a collapsed group's retained content without accepting editor drafts.
    pub fn open_content_group(
        &mut self,
        ui: &mut Controls,
        group_id: &str,
    ) -> Result<(), ControlError> {
        self.open_content_groups(ui, vec![group_id.into()])
    }
    pub(super) fn open_content_groups(
        &mut self,
        ui: &mut Controls,
        groups: Vec<String>,
    ) -> Result<(), ControlError> {
        let tab = self
            .model
            .tabs
            .iter()
            .find(|t| t.id == self.active)
            .ok_or_else(|| ControlError::Invalid("No active Ribbon tab".into()))?;
        if groups.is_empty()
            || groups
                .iter()
                .any(|g| !tab.groups.iter().any(|candidate| candidate.id == *g))
            || !groups.iter().any(|g| self.group_has_content(g))
        {
            return Err(ControlError::Invalid("Unknown Ribbon content group".into()));
        }
        self.validate_retained_tree(ui)?;
        let flyout = self
            .retained
            .overlay
            .ok_or_else(|| ControlError::Invalid("Ribbon is not mounted".into()))?
            .0;
        ui.preflight_open_flyout(flyout)?;
        let measured = self.preflight_metrics(ui, self.width)?;
        if !self.is_expanded() {
            self.reveal(ui);
        }
        self.finish_key_tips(ui, false);
        self.park_content(ui)?;
        self.retained.opened = Some((self.active.clone(), groups.clone()));
        self.rebuild_content_overlay(ui, &groups, measured.metrics, &measured.widths)?;
        // Restore non-overlay slots into their ordinary presenters on the next reflow.
        self.dirty = true;
        let flyout = self
            .retained
            .overlay
            .ok_or_else(|| ControlError::Invalid("Ribbon is not mounted".into()))?
            .0;
        ui.open_flyout(flyout)?;
        Ok(())
    }
    pub fn close_content_groups(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.retained.opened = None;
        if let Some((flyout, _)) = self.retained.overlay {
            ui.close_flyout(flyout)?;
        }
        self.dirty = true;
        Ok(())
    }
    pub fn content_popup_is_open(&self, ui: &Controls) -> bool {
        self.retained
            .overlay
            .is_some_and(|(flyout, _)| ui.flyout_is_open(flyout))
    }
    pub(super) fn handle_content_input(&mut self, ui: &mut Controls, event: &InputEvent) -> bool {
        if !self.content_popup_is_open(ui) {
            return false;
        }
        if matches!(
            event,
            InputEvent::KeyDown {
                key: Key::Escape,
                repeat: false,
                ..
            }
        ) && ui
            .runtime
            .focused()
            .and_then(|id| ui.document(id))
            .is_some_and(|doc| doc.composition().is_some())
        {
            ui.cancel_preedit();
            return true;
        }
        false
    }
    pub(super) fn content_overlay_owns(&self, ui: &Controls, id: WidgetId) -> bool {
        self.retained
            .overlay
            .is_some_and(|(flyout, _)| descendant(ui, id, flyout.container))
    }
    pub(super) fn content_has_prefix(&self, prefix: &str) -> bool {
        self.retained
            .specs
            .iter()
            .any(|s| s.tab_id == self.active && s.key_tip.starts_with(prefix))
    }
    pub(super) fn activate_content_tip(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        let Some(slot) = self
            .retained
            .specs
            .iter()
            .find(|s| s.tab_id == self.active && s.key_tip == self.tip_prefix)
            .cloned()
        else {
            return Ok(false);
        };
        let host = self.retained.hosts[&slot.id];
        if !ui.is_enabled(host) {
            self.open_content_group(ui, &slot.group_id)?;
        }
        self.finish_key_tips(ui, false);
        let target = self
            .retained
            .focus_targets
            .get(&slot.id)
            .copied()
            .filter(|id| ui.is_enabled(*id))
            .or_else(|| {
                ui.runtime.tree().preorder().into_iter().find(|id| {
                    descendant(ui, *id, host)
                        && ui.is_enabled(*id)
                        && ui
                            .runtime
                            .tree()
                            .node(*id)
                            .is_some_and(|n| n.props.focusable && n.props.tab_stop)
                })
            });
        if let Some(target) = target {
            ui.runtime.apply(Mutation::Focus(Some(target)))?;
        }
        Ok(true)
    }
}
