use super::*;
impl Controls {
    fn preflight_gallery_host(
        &self,
        id: WidgetId,
        parent: WidgetId,
        style: &LayoutStyle,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        style.validate().map_err(invalid)?;
        if !self.runtime.tree().contains(parent)
            || !self.runtime.tree().is_attached(parent)
            || !self.effective_enabled(parent)
            || self.flyout_descendant(parent, id)
            || self
                .runtime
                .tree()
                .node(parent)
                .is_some_and(|n| n.props.style.kind == LayoutKind::Leaf)
        {
            return Err(invalid(
                "Gallery target must be a visible enabled host outside its own subtree",
            ));
        }
        Ok(())
    }
    /// Transfer the same retained gallery to an ordinary or Ribbon content host.
    /// The caller owns the host and must supply its complete layout style.
    pub fn set_gallery_host(
        &mut self,
        id: WidgetId,
        parent: WidgetId,
        mut style: LayoutStyle,
    ) -> Result<(), ControlError> {
        style.kind = LayoutKind::Overlay;
        self.preflight_gallery_host(id, parent, &style)?;
        if self.galleries.entries[&id]
            .popup
            .as_ref()
            .is_some_and(|p| self.flyout_descendant(parent, p.handle.container))
        {
            return Err(invalid("Use open_gallery_popup for its owned popup host"));
        }
        if let Some(p) = self.galleries.entries[&id]
            .popup
            .as_ref()
            .filter(|p| p.open)
            .map(|p| p.handle)
        {
            self.close_flyout(p)?;
        }
        self.runtime.apply_batch(&[
            Mutation::Reparent { node: id, parent },
            Mutation::SetStyle {
                node: id,
                style: style.clone(),
            },
        ])?;
        let e = self.galleries.entries.get_mut(&id).unwrap();
        e.parent = parent;
        e.width = -1.0;
        if let Some(p) = &mut e.popup {
            p.home = parent;
            p.style = style;
        }
        self.cancel_gallery_preview(id)?;
        self.scene_dirty = true;
        Ok(())
    }
    /// Allocate one retained popup. Its anchor and size are immutable; host
    /// transfers update the return parent without recreating any samples.
    pub fn configure_gallery_popup(
        &mut self,
        id: WidgetId,
        anchor: WidgetId,
        size: Size,
    ) -> Result<Flyout, ControlError> {
        self.check_gallery(id)?;
        if self.galleries.entries[&id].popup.is_some() {
            return Err(invalid("Gallery already has a configured popup"));
        }
        if self.flyout_descendant(anchor, id) {
            return Err(invalid("Gallery popup anchor must be outside the gallery"));
        }
        let style = self.runtime.tree().node(id).unwrap().props.style.clone();
        let home = self.galleries.entries[&id].parent;
        let handle = self.flyout(anchor, size)?;
        self.galleries.entries.get_mut(&id).unwrap().popup = Some(Popup {
            handle,
            anchor,
            home,
            style,
            open: false,
        });
        Ok(handle)
    }
    pub fn gallery_popup(&self, id: WidgetId) -> Option<Flyout> {
        Some(self.galleries.entries.get(&id)?.popup.as_ref()?.handle)
    }
    pub fn open_gallery_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let p = self.galleries.entries[&id]
            .popup
            .as_ref()
            .ok_or_else(|| invalid("Gallery popup is not configured"))?;
        if p.open {
            return Ok(());
        }
        self.preflight_open_flyout(p.handle)?;
        if !self.effective_enabled(p.home) || !self.is_enabled(id) {
            return Err(invalid("Gallery home must remain available"));
        }
        let handle = p.handle;
        let home = p.home;
        let original = p.style.clone();
        let mut style = original.clone();
        style.width = Length::Percent(1.0);
        style.height = Length::Percent(1.0);
        style.offset = Point::ZERO;
        style.margin = Edges::default();
        style.min_size = Size::ZERO;
        style.max_size = Size::new(f32::INFINITY, f32::INFINITY);
        self.runtime.apply_batch(&[
            Mutation::Reparent {
                node: id,
                parent: handle.content,
            },
            Mutation::SetStyle { node: id, style },
        ])?;
        {
            let e = self.galleries.entries.get_mut(&id).unwrap();
            e.parent = handle.content;
            e.width = -1.0;
            e.popup.as_mut().unwrap().open = true;
        }
        if let Err(error) = self.open_flyout(handle) {
            if self.flyout_is_open(handle) {
                self.close_flyout(handle)?;
            }
            self.runtime.apply_batch(&[
                Mutation::Reparent {
                    node: id,
                    parent: home,
                },
                Mutation::SetStyle {
                    node: id,
                    style: original,
                },
            ])?;
            let e = self.galleries.entries.get_mut(&id).unwrap();
            e.parent = home;
            e.width = -1.0;
            e.popup.as_mut().unwrap().open = false;
            return Err(error);
        }
        self.cancel_gallery_preview(id)?;
        self.reveal_gallery_selection(id)?;
        if let Some(widget) = self.galleries.entries[&id]
            .selected
            .as_ref()
            .and_then(|key| self.gallery_item_widget(id, key))
        {
            self.runtime.apply(Mutation::Focus(Some(widget)))?;
            self.bring_into_view(widget)?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn close_gallery_popup(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let handle = self
            .gallery_popup(id)
            .ok_or_else(|| invalid("Gallery popup is not configured"))?;
        self.close_flyout(handle)
    }
    pub(in crate::controls) fn gallery_will_close(
        &mut self,
        flyout: WidgetId,
    ) -> Result<(), ControlError> {
        let Some(id) = self.galleries.entries.iter().find_map(|(&id, e)| {
            e.popup
                .as_ref()
                .filter(|p| p.open && p.handle.container == flyout)
                .map(|_| id)
        }) else {
            return Ok(());
        };
        let p = self.galleries.entries[&id].popup.as_ref().unwrap();
        let parent = p.home;
        let style = p.style.clone();
        if self
            .runtime
            .focused()
            .is_some_and(|focus| self.flyout_descendant(focus, id))
        {
            self.runtime.apply(Mutation::Focus(None))?;
        }
        if self
            .galleries
            .pointer
            .as_ref()
            .is_some_and(|(owner, _, _)| *owner == id)
        {
            self.galleries.pointer = None;
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        if self.runtime.tree().contains(parent) && self.runtime.tree().contains(id) {
            self.runtime.apply_batch(&[
                Mutation::Reparent { node: id, parent },
                Mutation::SetStyle { node: id, style },
            ])?;
        }
        let e = self.galleries.entries.get_mut(&id).unwrap();
        e.parent = parent;
        e.width = -1.0;
        e.popup.as_mut().unwrap().open = false;
        self.cancel_gallery_preview(id)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub(in crate::controls) fn prune_galleries(&mut self) -> Result<(), ControlError> {
        let dead = self
            .galleries
            .entries
            .iter()
            .filter_map(|(&id, e)| {
                (!self.runtime.tree().contains(id)
                    || e.popup.as_ref().is_some_and(|p| {
                        !self.runtime.tree().contains(p.home)
                            || !self.runtime.tree().contains(p.anchor)
                            || !self.runtime.tree().contains(p.handle.container)
                    }))
                .then_some(id)
            })
            .collect::<Vec<_>>();
        for id in dead {
            if let Some(e) = self.galleries.entries.remove(&id) {
                e.policy.borrow_mut().alive = false;
                self.galleries.actions.retain(|_, (owner, _)| *owner != id);
                if self.runtime.tree().contains(id) {
                    self.remove(id)?;
                }
                if let Some(p) = e.popup
                    && self.runtime.tree().contains(p.handle.container)
                {
                    self.remove(p.handle.container)?;
                }
            }
        }
        self.galleries.actions.retain(|widget, (owner, _)| {
            self.runtime.tree().contains(*widget) && self.galleries.entries.contains_key(owner)
        });
        if self
            .galleries
            .pointer
            .as_ref()
            .is_some_and(|(id, _, epoch)| {
                self.galleries
                    .entries
                    .get(id)
                    .is_none_or(|e| e.epoch != *epoch)
            })
        {
            self.galleries.pointer = None;
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
}
