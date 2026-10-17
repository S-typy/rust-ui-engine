//! Retained edge presentation with unchanged pinned placement.
use super::*;

pub const DOCK_AUTO_HIDE_RAIL_SIZE: f32 = 36.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockAutoHideSettings {
    pub edge: DockEdge,
    /// Preferred overlay width for left/right, height for top/bottom.
    pub extent: f32,
}
impl Default for DockAutoHideSettings {
    fn default() -> Self {
        Self {
            edge: DockEdge::Left,
            extent: 320.0,
        }
    }
}
impl DockAutoHideSettings {
    pub fn validate(self) -> Result<(), DockError> {
        if !self.extent.is_finite() || !(120.0..=4096.0).contains(&self.extent) {
            return Err(invalid(
                "Auto-hide extent must be finite and within 120..4096",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DockAutoHide {
    pub pane: DockPaneId,
    pub settings: DockAutoHideSettings,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DockAutoHideRailGeometry {
    pub edge: DockEdge,
    pub bounds: Rect,
    pub panes: Vec<DockPaneId>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DockAutoHidePanelGeometry {
    pub pane: DockPaneId,
    pub edge: DockEdge,
    pub bounds: Rect,
    pub header: Rect,
    pub content: Rect,
}
impl DockGeometry {
    /// Paint order: pinned content first, transient edge panel last.
    pub fn visible_panes(&self) -> impl DoubleEndedIterator<Item = (&DockPaneId, Rect)> {
        self.groups
            .iter()
            .map(|group| (&group.active, group.content))
            .chain(
                self.auto_hide_panel
                    .iter()
                    .map(|panel| (&panel.pane, panel.content)),
            )
    }
    pub fn pane_content(&self, pane: &DockPaneId) -> Option<Rect> {
        self.visible_panes()
            .find_map(|(id, bounds)| (id == pane).then_some(bounds))
    }
    /// Overlay header occludes pinned content but belongs to docking chrome.
    pub fn pane_at(&self, point: rust_desktop_ui_core::Point) -> Option<&DockPaneId> {
        if let Some(panel) = &self.auto_hide_panel
            && panel.bounds.contains(point.x, point.y)
        {
            return panel
                .content
                .contains(point.x, point.y)
                .then_some(&panel.pane);
        }
        // Front group bounds occlude lower content, including the front header.
        self.groups
            .iter()
            .rev()
            .find(|group| group.bounds.contains(point.x, point.y))
            .and_then(|group| {
                group
                    .content
                    .contains(point.x, point.y)
                    .then_some(&group.active)
            })
    }
}
impl DockLayout {
    pub fn auto_hide(&self, pane: &DockPaneId) -> Option<&DockAutoHideSettings> {
        self.snapshot
            .auto_hide
            .iter()
            .find(|item| &item.pane == pane)
            .map(|item| &item.settings)
    }
    /// Pin/unpin without changing the original tree, active tabs or identifier allocation.
    pub fn set_auto_hide(
        &mut self,
        pane: &DockPaneId,
        settings: Option<DockAutoHideSettings>,
    ) -> Result<bool, DockError> {
        self.require_pane(pane)?;
        if self.is_closed(pane) {
            return Err(invalid(
                "A closed pane cannot change auto-hide presentation",
            ));
        }
        if let Some(settings) = settings {
            settings.validate()?;
        }
        if self.auto_hide(pane) == settings.as_ref() {
            return Ok(false);
        }
        let mut next = self.clone();
        next.unmark_auto_hide(pane);
        if let Some(settings) = settings {
            next.snapshot.auto_hide.push(DockAutoHide {
                pane: pane.clone(),
                settings,
            });
            // Metadata order follows catalog order, independent of mutation order.
            next.snapshot
                .auto_hide
                .sort_by_key(|item| next.panes.iter().position(|p| p.id == item.pane).unwrap());
        }
        self.commit(next)
    }
    pub fn reveal_auto_hide(&mut self, pane: &DockPaneId) -> Result<bool, DockError> {
        if self.auto_hide(pane).is_none() {
            return Err(invalid("Pane does not use auto-hide"));
        }
        let window = self
            .window_for(pane)
            .ok_or_else(|| invalid("Auto-hide pane has no window"))?;
        let changed = self.revealed.get(&window) != Some(pane);
        self.revealed.insert(window, pane.clone());
        Ok(changed)
    }
    pub fn dismiss_auto_hide(&mut self, window: DockWindowId) -> Result<bool, DockError> {
        if !self.has_window(window) {
            return Err(invalid("Unknown docking window"));
        }
        Ok(self.revealed.remove(&window).is_some())
    }
    pub fn revealed_auto_hide(&self, window: DockWindowId) -> Option<&DockPaneId> {
        self.revealed.get(&window)
    }
    /// Visibility independent of geometry, suitable for focus/IME ownership.
    pub fn pane_is_visible(&self, pane: &DockPaneId) -> bool {
        if self.auto_hide(pane).is_some() {
            return self
                .window_for(pane)
                .is_some_and(|window| self.revealed.get(&window) == Some(pane));
        }
        self.group_for(pane)
            .and_then(|group| self.group(group))
            .is_some_and(|group| match group {
                DockNode::Tabs { panes, active, .. } => {
                    let selected = if self.auto_hide(active).is_none() {
                        Some(active)
                    } else {
                        panes.iter().find(|p| self.auto_hide(p).is_none())
                    };
                    selected == Some(pane)
                }
                _ => false,
            })
    }
    pub(super) fn unmark_auto_hide(&mut self, pane: &DockPaneId) {
        self.snapshot.auto_hide.retain(|item| &item.pane != pane);
        self.revealed.retain(|_, id| id != pane);
    }
    pub(super) fn prune_revealed(&mut self) {
        let next = self
            .revealed
            .iter()
            .filter(|(window, pane)| {
                self.auto_hide(pane).is_some() && self.window_for(pane) == Some(**window)
            })
            .map(|(window, pane)| (*window, pane.clone()))
            .collect();
        self.revealed = next;
    }
    pub(super) fn visible_root(&self, node: &DockNode) -> Option<DockNode> {
        match node {
            DockNode::Tabs { id, panes, active } => {
                let panes: Vec<_> = panes
                    .iter()
                    .filter(|pane| self.auto_hide(pane).is_none())
                    .cloned()
                    .collect();
                let active = if panes.contains(active) {
                    active.clone()
                } else {
                    panes.first()?.clone()
                };
                Some(DockNode::Tabs {
                    id: *id,
                    panes,
                    active,
                })
            }
            DockNode::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => match (self.visible_root(first), self.visible_root(second)) {
                (Some(first), Some(second)) => Some(DockNode::Split {
                    id: *id,
                    axis: *axis,
                    ratio: *ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (first, second) => first.or(second),
            },
        }
    }
    pub(super) fn arrange_auto_hide(
        &self,
        window: DockWindowId,
        bounds: Rect,
        result: &mut DockGeometry,
    ) -> Rect {
        let order = [
            DockEdge::Top,
            DockEdge::Bottom,
            DockEdge::Left,
            DockEdge::Right,
        ];
        let mut area = bounds;
        for edge in order {
            let panes: Vec<_> = self
                .snapshot
                .auto_hide
                .iter()
                .filter(|item| {
                    item.settings.edge == edge && self.window_for(&item.pane) == Some(window)
                })
                .map(|item| item.pane.clone())
                .collect();
            if panes.is_empty() {
                continue;
            }
            let vertical = matches!(edge, DockEdge::Left | DockEdge::Right);
            let thickness = DOCK_AUTO_HIDE_RAIL_SIZE.min(if vertical {
                area.width / 2.0
            } else {
                area.height / 2.0
            });
            let rail = match edge {
                DockEdge::Top => {
                    let rail = Rect::new(area.x, area.y, area.width, thickness);
                    area.y += thickness;
                    area.height -= thickness;
                    rail
                }
                DockEdge::Bottom => {
                    area.height -= thickness;
                    Rect::new(area.x, area.y + area.height, area.width, thickness)
                }
                DockEdge::Left => {
                    let rail = Rect::new(area.x, area.y, thickness, area.height);
                    area.x += thickness;
                    area.width -= thickness;
                    rail
                }
                DockEdge::Right => {
                    area.width -= thickness;
                    Rect::new(area.x + area.width, area.y, thickness, area.height)
                }
            };
            result.auto_hide_rails.push(DockAutoHideRailGeometry {
                edge,
                bounds: rail,
                panes,
            });
        }
        if let Some(pane) = self.revealed.get(&window)
            && let Some(settings) = self.auto_hide(pane)
        {
            let minimum = self.pane(pane).expect("validated pane").min_size;
            let vertical = matches!(settings.edge, DockEdge::Left | DockEdge::Right);
            let extent = settings
                .extent
                .max(if vertical {
                    minimum.width
                } else {
                    minimum.height + DOCK_TAB_HEIGHT
                })
                .min(if vertical { area.width } else { area.height });
            let panel = match settings.edge {
                DockEdge::Left => Rect::new(area.x, area.y, extent, area.height),
                DockEdge::Right => {
                    Rect::new(area.x + area.width - extent, area.y, extent, area.height)
                }
                DockEdge::Top => Rect::new(area.x, area.y, area.width, extent),
                DockEdge::Bottom => {
                    Rect::new(area.x, area.y + area.height - extent, area.width, extent)
                }
            };
            let height = DOCK_TAB_HEIGHT.min(panel.height);
            result.auto_hide_panel = Some(DockAutoHidePanelGeometry {
                pane: pane.clone(),
                edge: settings.edge,
                bounds: panel,
                header: Rect::new(panel.x, panel.y, panel.width, height),
                content: Rect::new(
                    panel.x,
                    panel.y + height,
                    panel.width,
                    (panel.height - height).max(0.0),
                ),
            });
        }
        area
    }
}
pub(super) fn validate_auto_hide(snapshot: &DockSnapshot) -> Result<(), DockError> {
    if snapshot.auto_hide.len() > MAX_DOCK_PANES {
        return Err(invalid("Too many auto-hide panes"));
    }
    let mut seen = HashSet::new();
    for item in &snapshot.auto_hide {
        item.settings.validate()?;
        if !seen.insert(&item.pane)
            || snapshot.closed.contains(&item.pane)
            || !snapshot
                .root
                .iter()
                .chain(snapshot.floating.iter().map(|f| &f.root))
                .any(|root| pane_group(root, &item.pane).is_some())
        {
            return Err(invalid("Auto-hide requires unique live open panes"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(name: &str) -> DockPaneId {
        DockPaneId::new(name).unwrap()
    }
    fn tabs(key: u64, names: &[&str]) -> DockNode {
        DockNode::Tabs {
            id: DockGroupId(key),
            panes: names.iter().map(|name| id(name)).collect(),
            active: id(names[0]),
        }
    }
    fn fixture() -> DockLayout {
        DockLayout::new(
            ["document", "properties", "output", "search"]
                .into_iter()
                .map(|name| DockPane {
                    id: id(name),
                    title: name.into(),
                    closable: true,
                    min_size: Size::new(100.0, 70.0),
                })
                .collect(),
            DockNode::Split {
                id: DockSplitId(7),
                axis: Axis::Horizontal,
                ratio: 0.7,
                first: Box::new(tabs(3, &["document", "search"])),
                second: Box::new(DockNode::Split {
                    id: DockSplitId(8),
                    axis: Axis::Vertical,
                    ratio: 0.5,
                    first: Box::new(tabs(4, &["properties"])),
                    second: Box::new(tabs(5, &["output"])),
                }),
            },
        )
        .unwrap()
    }
    #[test]
    fn pin_restores_the_exact_tree_and_auto_hide_is_presentation_only() {
        let mut layout = fixture();
        let original = layout.snapshot();
        layout
            .set_auto_hide(&id("properties"), Some(Default::default()))
            .unwrap();
        assert_eq!(layout.snapshot().root, original.root);
        let geometry = layout
            .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 800.0, 600.0))
            .unwrap();
        assert_eq!(geometry.groups.len(), 2);
        assert_eq!(geometry.auto_hide_rails.len(), 1);
        assert!(geometry.groups.iter().all(|g| g.active != id("properties")));
        assert_eq!(layout.group_for(&id("properties")), Some(DockGroupId(4)));
        layout.reveal_auto_hide(&id("properties")).unwrap();
        assert!(
            !layout
                .set_auto_hide(&id("properties"), Some(Default::default()))
                .unwrap()
        );
        assert!(layout.pane_is_visible(&id("properties")));
        let geometry = layout
            .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 800.0, 600.0))
            .unwrap();
        assert_eq!(geometry.visible_panes().count(), 3);
        let overlay = geometry.auto_hide_panel.unwrap();
        assert_eq!(overlay.pane, id("properties"));
        layout.set_auto_hide(&id("properties"), None).unwrap();
        assert_eq!(layout.snapshot(), original);
        assert!(layout.revealed_auto_hide(DockWindowId::MAIN).is_none());
    }
    #[test]
    fn all_four_edges_and_tiny_viewports_have_finite_bounded_overlay_geometry() {
        let mut layout = fixture();
        for (name, edge) in ["document", "properties", "output", "search"]
            .into_iter()
            .zip([
                DockEdge::Left,
                DockEdge::Right,
                DockEdge::Top,
                DockEdge::Bottom,
            ])
        {
            layout
                .set_auto_hide(
                    &id(name),
                    Some(DockAutoHideSettings {
                        edge,
                        extent: 4096.0,
                    }),
                )
                .unwrap();
        }
        for size in [
            Size::ZERO,
            Size::new(1.0, 1.0),
            Size::new(70.0, 50.0),
            Size::new(800.0, 600.0),
        ] {
            for name in ["document", "properties", "output", "search"] {
                layout.reveal_auto_hide(&id(name)).unwrap();
                let geometry = layout
                    .arrange(
                        DockWindowId::MAIN,
                        Rect::new(0.0, 0.0, size.width, size.height),
                    )
                    .unwrap();
                assert!(geometry.groups.is_empty());
                assert_eq!(geometry.auto_hide_rails.len(), 4);
                for rect in geometry
                    .auto_hide_rails
                    .iter()
                    .map(|rail| rail.bounds)
                    .chain(
                        geometry
                            .auto_hide_panel
                            .iter()
                            .flat_map(|panel| [panel.bounds, panel.header, panel.content]),
                    )
                {
                    assert!(rect.is_valid());
                    assert!(rect.x >= 0.0 && rect.y >= 0.0);
                    assert!(
                        rect.x + rect.width <= size.width + 0.001
                            && rect.y + rect.height <= size.height + 0.001
                    );
                }
            }
        }
    }
    #[test]
    fn hidden_active_tab_falls_back_without_changing_saved_active_and_pin_restores_it() {
        let mut layout = fixture();
        let pane = id("document");
        layout
            .set_auto_hide(&pane, Some(Default::default()))
            .unwrap();
        assert!(layout.pane_is_visible(&id("search")));
        assert!(!layout.pane_is_visible(&pane));
        let geometry = layout
            .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 600.0, 400.0))
            .unwrap();
        assert_eq!(geometry.groups[0].active, id("search"));
        layout.reveal_auto_hide(&pane).unwrap();
        assert!(layout.pane_is_visible(&pane));
        layout.dismiss_auto_hide(DockWindowId::MAIN).unwrap();
        assert!(!layout.pane_is_visible(&pane));
        layout.set_auto_hide(&pane, None).unwrap();
        assert!(layout.pane_is_visible(&pane));
        assert!(!layout.pane_is_visible(&id("search")));
    }
    #[test]
    fn floating_move_close_and_redock_keep_metadata_consistent() {
        let mut layout = fixture();
        let pane = id("properties");
        let window = layout
            .float_pane(&pane, Rect::new(-700.0, 80.0, 500.0, 400.0))
            .unwrap();
        layout
            .set_auto_hide(&pane, Some(Default::default()))
            .unwrap();
        layout.reveal_auto_hide(&pane).unwrap();
        assert!(layout.root(window).is_some());
        assert!(
            layout
                .arrange(window, Rect::new(0.0, 0.0, 500.0, 400.0))
                .unwrap()
                .groups
                .is_empty()
        );
        layout.redock_window(window).unwrap();
        assert!(layout.auto_hide(&pane).is_some());
        assert!(layout.revealed_auto_hide(window).is_none());
        layout
            .move_pane(
                &pane,
                DockPlacement::Tab {
                    group: DockGroupId(3),
                    index: 1,
                },
            )
            .unwrap();
        assert!(layout.auto_hide(&pane).is_none());
        layout
            .set_auto_hide(&pane, Some(Default::default()))
            .unwrap();
        layout.close(&pane).unwrap();
        assert!(layout.auto_hide(&pane).is_none());
        assert!(
            layout
                .set_auto_hide(&pane, Some(Default::default()))
                .is_err()
        );
    }
    #[test]
    fn json_v2_roundtrip_keeps_pin_locations_but_not_transient_reveal_and_migrates_v1() {
        let mut layout = fixture();
        layout
            .set_auto_hide(
                &id("properties"),
                Some(DockAutoHideSettings {
                    edge: DockEdge::Right,
                    extent: 240.0,
                }),
            )
            .unwrap();
        let source = layout.snapshot_json().unwrap();
        layout.reveal_auto_hide(&id("properties")).unwrap();
        assert_eq!(layout.snapshot_json().unwrap(), source);
        let mut target = fixture();
        target.restore_json(&source).unwrap();
        assert_eq!(target.snapshot(), layout.snapshot());
        assert!(target.revealed_auto_hide(DockWindowId::MAIN).is_none());
        assert!(layout.restore_json(&source).unwrap());
        assert!(layout.revealed_auto_hide(DockWindowId::MAIN).is_none());
        let mut legacy: serde_json::Value =
            serde_json::from_str(&fixture().snapshot_json().unwrap()).unwrap();
        legacy["version"] = 1.into();
        legacy.as_object_mut().unwrap().remove("auto_hide");
        target.restore_json(&legacy.to_string()).unwrap();
        assert_eq!(target.snapshot(), fixture().snapshot());
        assert_eq!(target.snapshot().version, 2);
    }
    #[test]
    fn invalid_metadata_and_json_leave_snapshot_reveal_and_identifier_allocation_intact() {
        let mut layout = fixture();
        layout
            .set_auto_hide(&id("properties"), Some(Default::default()))
            .unwrap();
        layout.reveal_auto_hide(&id("properties")).unwrap();
        let snapshot = layout.snapshot();
        let source = layout.snapshot_json().unwrap();
        for extent in [0.0, 119.0, 4097.0, f32::NAN, f32::INFINITY] {
            assert!(
                layout
                    .set_auto_hide(
                        &id("properties"),
                        Some(DockAutoHideSettings {
                            extent,
                            ..Default::default()
                        })
                    )
                    .is_err()
            );
            assert_eq!(layout.snapshot(), snapshot);
            assert_eq!(
                layout.revealed_auto_hide(DockWindowId::MAIN),
                Some(&id("properties"))
            );
        }
        let base: serde_json::Value = serde_json::from_str(&source).unwrap();
        let mut cases = Vec::new();
        let mut value = base.clone();
        value["auto_hide"][0]["pane"] = "missing".into();
        cases.push(value);
        let mut value = base.clone();
        value["auto_hide"]
            .as_array_mut()
            .unwrap()
            .push(base["auto_hide"][0].clone());
        cases.push(value);
        let mut value = base.clone();
        value["auto_hide"][0]["extent"] = 0.into();
        cases.push(value);
        let mut value = base.clone();
        value["auto_hide"][0]["edge"] = "diagonal".into();
        cases.push(value);
        let mut value = base.clone();
        value["version"] = 1.into();
        cases.push(value);
        let mut value = base.clone();
        value["auto_hide"] = serde_json::Value::Null;
        cases.push(value);
        for value in cases {
            assert!(layout.restore_json(&value.to_string()).is_err());
            assert_eq!(layout.snapshot(), snapshot);
            assert_eq!(
                layout.revealed_auto_hide(DockWindowId::MAIN),
                Some(&id("properties"))
            );
        }
        let mut baseline = layout.clone();
        let bounds = Rect::new(0.0, 0.0, 500.0, 400.0);
        assert_eq!(
            layout.float_pane(&id("output"), bounds).unwrap(),
            baseline.float_pane(&id("output"), bounds).unwrap()
        );
    }
}
