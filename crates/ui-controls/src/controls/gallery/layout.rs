use super::*;
fn recipe() -> ListItemTemplate {
    ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle {
                width: Length::Percent(1.0),
                ..Default::default()
            },
            kind: ListTemplateKind::Text {
                source: ListItemTextSource::Text,
                style: TextStyle::default(),
            },
        },
    }
}
fn readonly_parents(ui: &Controls, root: WidgetId) -> Vec<(WidgetId, WidgetId)> {
    ui.runtime
        .tree()
        .preorder()
        .into_iter()
        .filter(|node| ui.flyout_descendant(*node, root))
        .filter_map(|node| ui.runtime.tree().parent(node).map(|parent| (node, parent)))
        .collect()
}
impl Controls {
    pub(super) fn preflight_gallery(
        &mut self,
        model: &GalleryModel,
        settings: &GallerySettings,
        template: Option<&ListItemTemplate>,
    ) -> Result<(), ControlError> {
        model.validate()?;
        settings.validate()?;
        let default = recipe();
        let template = template.unwrap_or(&default);
        template.validate()?;
        for group in model.groups() {
            self.measure_text(&group.header, 4096.0, false)?;
        }
        for item in model.items() {
            self.preflight_item_template(
                template,
                &item.item,
                (settings.sample_width - 12.0).max(1.0),
            )?;
            self.measure_text(&item.item.text, 4096.0, false)?;
            if let Some(key) = &item.image_resource {
                if self.image_source(key).is_none() {
                    return Err(invalid("Gallery requires a registered image resource"));
                }
                self.image_resource_size(key)
                    .map_err(|e| invalid(e.to_string()))?;
            }
        }
        Ok(())
    }
    fn gallery_visual(
        &mut self,
        parent: WidgetId,
        item: &GalleryItem,
        template: Option<&ListItemTemplate>,
    ) -> Result<WidgetId, ControlError> {
        let root = self.transparent_panel(
            parent,
            LayoutStyle {
                // The caption host fills the space left by the fixed-size image.
                // Stack intentionally ignores flex_grow, leaving an Auto-height
                // overlay with an Auto-height TextBlock at zero height.
                kind: LayoutKind::Flex(Axis::Vertical),
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                gap: 4.0,
                ..Default::default()
            },
        )?;
        let result = (|| {
            self.edit_props(root, |p| {
                p.hit_test = false;
                p.clip = true;
            })?;
            let image_style = LayoutStyle {
                width: Length::Px(32.0),
                height: Length::Px(32.0),
                horizontal_alignment: Some(rust_desktop_ui_core::Align::Center),
                flex_shrink: 0.0,
                ..Default::default()
            };
            if let Some(symbol) = item.icon {
                let icon = self.icon(root, symbol, image_style.clone())?;
                self.edit_props(icon, |p| p.hit_test = false)?;
            }
            if let Some(key) = &item.image_resource {
                let image =
                    self.image_from_resource(root, key, ImageSettings::default(), image_style, "")?;
                self.edit_props(image, |p| p.hit_test = false)?;
            }
            let default = recipe();
            let content =
                self.create_list_item_template(root, template.unwrap_or(&default), &item.item)?;
            self.edit_props(content.root, |p| {
                p.style.height = Length::Auto;
                p.style.flex_grow = 1.0;
            })?;
            Ok::<_, ControlError>(root)
        })();
        if result.is_err() {
            self.remove(root)?;
        }
        result
    }
    pub(super) fn install_gallery(
        &mut self,
        id: WidgetId,
        model: GalleryModel,
        settings: GallerySettings,
        template: Option<ListItemTemplate>,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        self.preflight_gallery(&model, &settings, template.as_ref())?;
        let e = &self.galleries.entries[&id];
        let selected = e
            .selected
            .clone()
            .filter(|key| model.item(key).is_some_and(|i| i.item.enabled));
        self.validate_gallery_key(id, &model, selected.as_deref())?;
        let epoch = e
            .epoch
            .checked_add(1)
            .ok_or_else(|| invalid("Gallery source revision exhausted"))?;
        let parent = e.parts.content;
        let mut staged: HashMap<String, Sample> = HashMap::new();
        let mut headers = Vec::new();
        let mut new_rows = Vec::new();
        let result = (|| {
            for item in model.items() {
                let widget =
                    if let Some(old) = self.galleries.entries[&id].samples.get(&item.item.key) {
                        old.widget
                    } else {
                        let w = self.button(
                            parent,
                            "",
                            None,
                            LayoutStyle {
                                kind: LayoutKind::Overlay,
                                padding: Edges::all(6.0),
                                ..Default::default()
                            },
                        )?;
                        new_rows.push(w);
                        self.set_button_style(
                            w,
                            ButtonStyle {
                                padding: Some(Edges::all(6.0)),
                                ..Default::default()
                            },
                        )?;
                        self.edit_props(w, |p| {
                            p.clip = true;
                            p.tab_stop = false;
                        })?;
                        self.set_role(w, Role::ListItem);
                        w
                    };
                let visual = self.gallery_visual(widget, item, template.as_ref())?;
                let parents = readonly_parents(self, visual);
                staged.insert(
                    item.item.key.clone(),
                    Sample {
                        widget,
                        visual,
                        parents,
                    },
                );
            }
            for group in model.groups() {
                let header = self.label(parent, &group.header, LayoutStyle::default())?;
                headers.push(header);
                self.set_text_style(
                    header,
                    TextStyle {
                        font_weight: Some(600),
                        ..Default::default()
                    },
                )?;
                self.edit_props(header, |p| p.hit_test = false)?;
            }
            Ok::<_, ControlError>(())
        })();
        if let Err(error) = result {
            for sample in staged.values() {
                if !new_rows.contains(&sample.widget) {
                    self.remove(sample.visual)?;
                }
            }
            for row in new_rows {
                self.remove(row)?;
            }
            for h in headers {
                self.remove(h)?;
            }
            return Err(error);
        }
        let e = self.galleries.entries.get_mut(&id).unwrap();
        let old = std::mem::replace(&mut e.samples, staged);
        let old_headers = std::mem::replace(&mut e.headers, headers);
        e.model = model;
        e.settings = settings;
        e.template = template;
        e.selected = selected;
        e.cursor = e.selected.clone();
        e.epoch = epoch;
        e.width = -1.0;
        e.error = None;
        e.policy.borrow_mut().model = e.model.clone();
        self.cancel_gallery_preview(id)?;
        for (key, sample) in old {
            if self.galleries.entries[&id].samples.contains_key(&key) {
                self.remove(sample.visual)?;
            } else {
                self.galleries.actions.remove(&sample.widget);
                self.remove(sample.widget)?;
            }
        }
        for h in old_headers {
            self.remove(h)?;
        }
        let data = self.galleries.entries[&id].model.items().to_vec();
        for item in data {
            let widget = self.galleries.entries[&id].samples[&item.item.key].widget;
            self.set_automation_name(widget, &item.item.text)?;
            self.set_enabled(widget, item.item.enabled)?;
            self.galleries.actions.insert(widget, (id, item.item.key));
        }
        self.controls.get_mut(&id).unwrap().validation_error = None;
        self.refresh_gallery_selection(id)?;
        self.layout_gallery(id)?;
        Ok(())
    }
    fn layout_gallery(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.galleries.entries[&id];
        let width = self.runtime.bounds(id).map_or(360.0, |r| r.width.max(1.0));
        if e.width == width {
            return Ok(());
        }
        let s = &e.settings;
        let sample_width = s.sample_width.min(width);
        let columns = ((width + s.gap) / (sample_width + s.gap)).floor().max(1.0) as usize;
        let mut y = 0.0;
        let mut mutations = Vec::new();
        let mut order = Vec::new();
        for (group_index, group) in std::iter::once("")
            .chain(e.model.groups().iter().map(|g| g.key.as_str()))
            .enumerate()
        {
            let items = e
                .model
                .items()
                .iter()
                .filter(|i| i.group == group)
                .collect::<Vec<_>>();
            if items.is_empty() {
                if group_index > 0 {
                    let header = e.headers[group_index - 1];
                    let mut props = self.runtime.tree().node(header).unwrap().props.clone();
                    props.collapsed = true;
                    mutations.push(Mutation::SetProps {
                        node: header,
                        props,
                    });
                }
                continue;
            }
            if group_index > 0 {
                let header = e.headers[group_index - 1];
                let mut props = self.runtime.tree().node(header).unwrap().props.clone();
                props.collapsed = false;
                props.style.offset = Point::new(0.0, y);
                props.style.width = Length::Px(width);
                props.style.height = Length::Px(s.group_header_height);
                mutations.push(Mutation::SetProps {
                    node: header,
                    props,
                });
                order.push(header);
                y += s.group_header_height + s.gap;
            }
            for (index, item) in items.iter().enumerate() {
                let widget = e.samples[&item.item.key].widget;
                let mut style = self
                    .runtime
                    .tree()
                    .node(widget)
                    .unwrap()
                    .props
                    .style
                    .clone();
                style.width = Length::Px(sample_width);
                style.height = Length::Px(s.sample_height);
                style.offset = Point::new(
                    (index % columns) as f32 * (sample_width + s.gap),
                    y + (index / columns) as f32 * (s.sample_height + s.gap),
                );
                mutations.push(Mutation::SetStyle {
                    node: widget,
                    style,
                });
                order.push(widget);
            }
            y += items.len().div_ceil(columns) as f32 * (s.sample_height + s.gap);
        }
        for node in order {
            mutations.push(Mutation::Reparent {
                node,
                parent: e.parts.content,
            });
        }
        self.runtime.apply_batch(&mutations)?;
        self.set_scroll_extent(id, y.max(0.0))?;
        let e = self.galleries.entries.get_mut(&id).unwrap();
        e.width = width;
        e.columns = columns;
        e.reveal = true;
        self.scene_dirty = true;
        Ok(())
    }
    pub(in crate::controls) fn sync_galleries(&mut self) -> Result<(), ControlError> {
        self.prune_galleries()?;
        for id in self.galleries.entries.keys().copied().collect::<Vec<_>>() {
            self.check_gallery(id)?;
            let unavailable = !self.is_enabled(id)
                || !self.window_focused
                || !self.runtime.tree().is_attached(id);
            let home_unavailable = self.galleries.entries[&id].popup.as_ref().is_some_and(|p| {
                p.open && (!self.effective_enabled(p.home) || !self.is_enabled(p.anchor))
            });
            if unavailable || home_unavailable {
                self.cancel_gallery_preview(id)?;
                if let Some(popup) = self.galleries.entries[&id]
                    .popup
                    .as_ref()
                    .filter(|p| p.open)
                    .map(|p| p.handle)
                {
                    self.close_flyout(popup)?;
                }
            }
            if !self.is_enabled(id) || !self.runtime.tree().is_attached(id) {
                continue;
            }
            let changed = self
                .runtime
                .bounds(id)
                .is_some_and(|r| r.width.max(1.0) != self.galleries.entries[&id].width);
            self.layout_gallery(id)?;
            if !changed && self.galleries.entries[&id].reveal {
                self.galleries.entries.get_mut(&id).unwrap().reveal = false;
                self.reveal_gallery_now(id)?;
            }
        }
        Ok(())
    }
}
