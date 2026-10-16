//! Retained, noninteractive content templates shared by button-like controls.
use super::*;

#[derive(Clone)]
struct Content {
    root: WidgetId,
    hit_tests: Vec<(WidgetId, bool)>,
}
#[derive(Default)]
pub(super) struct ButtonContents {
    entries: HashMap<WidgetId, Content>,
}
impl Controls {
    pub(super) fn preflight_button_content_replacement(
        &self,
        previous_owners: &[WidgetId],
        next_count: usize,
    ) -> Result<(), ControlError> {
        let released = previous_owners
            .iter()
            .filter(|id| self.button_contents.entries.contains_key(id))
            .count();
        if self
            .button_contents
            .entries
            .len()
            .saturating_sub(released)
            .saturating_add(next_count)
            > 4096
        {
            return Err(ControlError::Invalid(
                "At most 4096 button templates are supported".into(),
            ));
        }
        Ok(())
    }
    /// A live retained content subtree. The owner keeps its command, semantic
    /// label, focus, hit testing and indicator; only its default caption/icon
    /// painting is replaced. Removed or reparented templates return `None`.
    pub fn button_content(&self, id: WidgetId) -> Option<WidgetId> {
        let content = self.button_contents.entries.get(&id)?;
        (self.runtime.tree().parent(content.root) == Some(id)).then_some(content.root)
    }
    /// Installs noninteractive content without cloning its nodes. Replacing or
    /// clearing detaches the old subtree, retaining IDs for later reuse. A
    /// template may contain labels, icons, images and layout, but no input
    /// controls. All validation precedes reparenting.
    pub fn set_button_content(
        &mut self,
        id: WidgetId,
        root: Option<WidgetId>,
    ) -> Result<(), ControlError> {
        self.restore_stale_button_contents()?;
        if !self.runtime.tree().contains(id)
            || !self.controls.get(&id).is_some_and(|control| {
                matches!(
                    control.kind,
                    Kind::Button
                        | Kind::Toggle
                        | Kind::CheckBox
                        | Kind::Radio { .. }
                        | Kind::MenuItem
                        | Kind::Tab { .. }
                ) || matches!(control.kind, Kind::Label) && self.group_owner(id).is_some()
            })
        {
            return Err(ControlError::Invalid(
                "Content templates require a live button-like control".into(),
            ));
        }
        if root == self.button_content(id) {
            return Ok(());
        }
        if root.is_some()
            && self.button_contents.entries.len() >= 4096
            && !self.button_contents.entries.contains_key(&id)
        {
            return Err(ControlError::Invalid(
                "At most 4096 button templates are supported".into(),
            ));
        }
        let mut content = None;
        if let Some(root) = root {
            if !self.runtime.tree().contains(root)
                || root == self.root
                || root == self.overlay
                || self.flyout_descendant(id, root)
                || self.button_contents.entries.iter().any(|(owner, content)| {
                    *owner != id
                        && (self.flyout_descendant(root, content.root)
                            || self.flyout_descendant(content.root, root))
                })
            {
                return Err(ControlError::Invalid(
                    "Invalid or already owned button content subtree".into(),
                ));
            }
            let mut nodes = Vec::new();
            let mut pending = vec![root];
            while let Some(node) = pending.pop() {
                let props = &self.runtime.tree().node(node).unwrap().props;
                if nodes.len() >= 1024
                    || props.focusable
                    || self.controls.get(&node).is_some_and(|control| {
                        control.kind.clickable()
                            || matches!(control.kind, Kind::TextBox(_) | Kind::Scroll { .. })
                    })
                {
                    return Err(ControlError::Invalid(
                        "Button content supports at most 1024 noninteractive nodes".into(),
                    ));
                }
                let hit_test = self
                    .button_contents
                    .entries
                    .get(&id)
                    .and_then(|old| old.hit_tests.iter().find(|(id, _)| *id == node))
                    .map_or(props.hit_test, |(_, original)| *original);
                nodes.push((node, hit_test));
                if nodes.len()
                    + pending.len()
                    + self
                        .runtime
                        .tree()
                        .children(node)
                        .map_or(0, |children| children.len())
                    > 1024
                {
                    return Err(ControlError::Invalid(
                        "Button content supports at most 1024 noninteractive nodes".into(),
                    ));
                }
                pending.extend(
                    self.runtime
                        .tree()
                        .children(node)
                        .unwrap_or(&[])
                        .iter()
                        .copied(),
                );
            }
            content = Some(Content {
                root,
                hit_tests: nodes,
            });
        }
        let mut mutations = Vec::new();
        if let Some(old) = self.button_contents.entries.get(&id) {
            if self.runtime.tree().contains(old.root) {
                mutations.push(Mutation::Detach(old.root));
            }
            for &(node, hit_test) in &old.hit_tests {
                if let Some(node_state) = self.runtime.tree().node(node) {
                    let mut props = node_state.props.clone();
                    props.hit_test = hit_test;
                    mutations.push(Mutation::SetProps { node, props });
                }
            }
        }
        if let Some(content) = &content {
            mutations.push(Mutation::Reparent {
                node: content.root,
                parent: id,
            });
            let mut props = self.runtime.tree().node(id).unwrap().props.clone();
            props.style.kind = LayoutKind::Overlay;
            props.clip = true;
            if matches!(self.controls[&id].kind, Kind::Radio { .. } | Kind::CheckBox) {
                props.style.padding.left = props.style.padding.left.max(34.0);
            }
            mutations.push(Mutation::SetProps { node: id, props });
            for &(node, _) in &content.hit_tests {
                let mut props = self.runtime.tree().node(node).unwrap().props.clone();
                props.hit_test = false;
                mutations.push(Mutation::SetProps { node, props });
            }
        }
        self.runtime.apply(Mutation::Batch(mutations))?;
        if let Some(content) = content {
            self.button_contents.entries.insert(id, content);
        } else {
            self.button_contents.entries.remove(&id);
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn prune_button_contents(&mut self) {
        self.button_contents.entries.retain(|_, content| {
            self.runtime.tree().contains(content.root)
                || content
                    .hit_tests
                    .iter()
                    .any(|(id, _)| self.runtime.tree().contains(*id))
        });
    }
    fn restore_stale_button_contents(&mut self) -> Result<(), ControlError> {
        let stale: Vec<_> = self
            .button_contents
            .entries
            .iter()
            .filter(|(owner, content)| self.runtime.tree().parent(content.root) != Some(**owner))
            .map(|(owner, content)| (*owner, content.clone()))
            .collect();
        for (owner, content) in stale {
            let mut mutations = Vec::new();
            for (node, hit_test) in content.hit_tests {
                if let Some(state) = self.runtime.tree().node(node) {
                    let mut props = state.props.clone();
                    props.hit_test = hit_test;
                    mutations.push(Mutation::SetProps { node, props });
                }
            }
            self.runtime.apply(Mutation::Batch(mutations))?;
            self.button_contents.entries.remove(&owner);
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub(super) fn sync_button_contents(&mut self) -> Result<(), ControlError> {
        self.prune_button_contents();
        self.restore_stale_button_contents()?;
        let roots: Vec<_> = self
            .button_contents
            .entries
            .iter()
            .map(|(owner, content)| (*owner, content.root))
            .collect();
        for (owner, root) in roots {
            let tracked = self.button_contents.entries[&owner].hit_tests.clone();
            let mut retained = Vec::new();
            for (id, hit_test) in tracked {
                if self.flyout_descendant(id, root) && self.runtime.tree().contains(id) {
                    retained.push((id, hit_test));
                } else if self.runtime.tree().contains(id) {
                    self.edit_props(id, |props| props.hit_test = hit_test)?;
                }
            }
            self.button_contents
                .entries
                .get_mut(&owner)
                .unwrap()
                .hit_tests = retained;
            let known: HashSet<_> = self.button_contents.entries[&owner]
                .hit_tests
                .iter()
                .map(|(id, _)| *id)
                .collect();
            let mut additions = Vec::new();
            let mut hits_to_disable = Vec::new();
            let mut pending = vec![root];
            let mut count = 0;
            while let Some(id) = pending.pop() {
                count += 1;
                let node = self.runtime.tree().node(id).unwrap();
                if count > 1024
                    || node.props.focusable
                    || self.controls.get(&id).is_some_and(|control| {
                        control.kind.clickable()
                            || matches!(control.kind, Kind::TextBox(_) | Kind::Scroll { .. })
                    })
                {
                    return Err(ControlError::Invalid(
                        "Button template became interactive or exceeded its node bound".into(),
                    ));
                }
                if count
                    + pending.len()
                    + self
                        .runtime
                        .tree()
                        .children(id)
                        .map_or(0, |children| children.len())
                    > 1024
                {
                    return Err(ControlError::Invalid(
                        "Button content supports at most 1024 noninteractive nodes".into(),
                    ));
                }
                pending.extend(
                    self.runtime
                        .tree()
                        .children(id)
                        .unwrap_or(&[])
                        .iter()
                        .copied(),
                );
                if !known.contains(&id) {
                    additions.push((id, node.props.hit_test));
                }
                if node.props.hit_test {
                    hits_to_disable.push(id);
                }
            }
            let mutations = hits_to_disable
                .into_iter()
                .map(|id| {
                    let mut props = self.runtime.tree().node(id).unwrap().props.clone();
                    props.hit_test = false;
                    Mutation::SetProps { node: id, props }
                })
                .collect();
            self.runtime.apply(Mutation::Batch(mutations))?;
            self.button_contents
                .entries
                .get_mut(&owner)
                .unwrap()
                .hit_tests
                .extend(additions);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn template_owns_visuals_but_button_owns_activation_and_identity() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("save", "Accessible Save");
        let button = ui
            .button(
                ui.root(),
                "Default",
                Some("save".into()),
                LayoutStyle {
                    width: Length::Px(220.0),
                    height: Length::Px(60.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let content = ui
            .transparent_panel(ui.root(), LayoutStyle::default())
            .unwrap();
        let text = ui
            .label(content, "Custom caption", LayoutStyle::default())
            .unwrap();
        ui.set_button_content(button, Some(content)).unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert_eq!(ui.button_content(button), Some(content));
        assert!(!ui.runtime.tree().node(content).unwrap().props.hit_test);
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Custom caption")
        );
        assert!(
            !ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Accessible Save")
        );
        let bounds = ui.runtime.bounds(text).unwrap();
        assert_eq!(
            ui.runtime
                .hit_test(Point::new(bounds.x + 2.0, bounds.y + 2.0)),
            Some(button)
        );
        ui.activate(button).unwrap();
        assert_eq!(ui.drain_activated(), vec![button]);
        assert_eq!(
            ui.semantics()
                .into_iter()
                .find(|node| node.id == button)
                .unwrap()
                .label,
            "Default"
        );
        let interactive = ui
            .edit_box(ui.root(), "Rejected", "", LayoutStyle::default())
            .unwrap();
        assert!(ui.set_button_content(button, Some(interactive)).is_err());
        assert_eq!(ui.button_content(button), Some(content));
        ui.set_button_content(button, None).unwrap();
        assert!(ui.runtime.tree().contains(text));
        assert_eq!(ui.runtime.tree().parent(content), None);
        assert!(ui.runtime.tree().node(content).unwrap().props.hit_test);
    }

    #[test]
    fn dynamic_template_nodes_restore_hit_testing_when_detached_or_cleared() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let button = ui
            .button(ui.root(), "Owner", None, LayoutStyle::default())
            .unwrap();
        let content = ui
            .transparent_panel(ui.root(), LayoutStyle::default())
            .unwrap();
        ui.set_button_content(button, Some(content)).unwrap();
        let child = ui
            .transparent_panel(content, LayoutStyle::default())
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert!(!ui.runtime.tree().node(child).unwrap().props.hit_test);
        ui.runtime
            .apply(Mutation::Reparent {
                node: child,
                parent: ui.root(),
            })
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert!(ui.runtime.tree().node(child).unwrap().props.hit_test);
        assert_eq!(ui.button_contents.entries[&button].hit_tests.len(), 1);

        let retained_child = ui
            .transparent_panel(content, LayoutStyle::default())
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        ui.runtime
            .apply(Mutation::Reparent {
                node: content,
                parent: ui.root(),
            })
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert_eq!(ui.button_content(button), None);
        assert!(ui.runtime.tree().node(content).unwrap().props.hit_test);
        assert!(
            ui.runtime
                .tree()
                .node(retained_child)
                .unwrap()
                .props
                .hit_test
        );
        assert!(!ui.button_contents.entries.contains_key(&button));
    }

    #[test]
    fn nested_template_replacement_preserves_original_hit_flags_and_rejects_new_input() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let button = ui
            .button(ui.root(), "Owner", None, LayoutStyle::default())
            .unwrap();
        let content = ui
            .transparent_panel(ui.root(), LayoutStyle::default())
            .unwrap();
        let child = ui
            .transparent_panel(content, LayoutStyle::default())
            .unwrap();
        ui.set_button_content(button, Some(content)).unwrap();
        ui.set_button_content(button, Some(child)).unwrap();
        ui.set_button_content(button, None).unwrap();
        assert!(ui.runtime.tree().node(child).unwrap().props.hit_test);

        ui.set_button_content(button, Some(content)).unwrap();
        let interactive = ui
            .button(content, "Rejected", None, LayoutStyle::default())
            .unwrap();
        let new_static = ui
            .transparent_panel(content, LayoutStyle::default())
            .unwrap();
        assert!(ui.update(Size::new(400.0, 300.0)).is_err());
        assert!(ui.runtime.tree().node(new_static).unwrap().props.hit_test);
        ui.remove(interactive).unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert!(!ui.runtime.tree().node(new_static).unwrap().props.hit_test);
        ui.set_button_content(button, None).unwrap();
        assert!(ui.runtime.tree().node(new_static).unwrap().props.hit_test);
    }
}
