use super::*;

type PreparedHeader = (String, String, Rect);

struct PreparedRow {
    key: String,
    item: TileItem,
    rect: Rect,
    recipe: ListItemTemplate,
}
struct Stage {
    rows: HashMap<String, (WidgetId, WidgetId, MountedTemplate)>,
    contents: HashMap<String, MountedTemplate>,
    headers: HashMap<String, Header>,
}
impl Stage {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
            contents: HashMap::new(),
            headers: HashMap::new(),
        }
    }
}
fn default_recipe(item: &TileItem) -> ListItemTemplate {
    let mut children = Vec::new();
    if let Some(symbol) = item.icon {
        children.push(ListTemplateNode {
            layout: LayoutStyle {
                width: Length::Px(28.0),
                height: Length::Px(28.0),
                ..Default::default()
            },
            kind: ListTemplateKind::Icon {
                symbol,
                style: IconStyle::default(),
            },
        });
    }
    children.push(ListTemplateNode {
        layout: LayoutStyle {
            width: Length::Percent(1.0),
            ..Default::default()
        },
        kind: ListTemplateKind::Text {
            source: ListItemTextSource::Text,
            style: TextStyle::default(),
        },
    });
    if !item.item.columns.is_empty() {
        children.push(ListTemplateNode {
            layout: LayoutStyle {
                width: Length::Percent(1.0),
                ..Default::default()
            },
            kind: ListTemplateKind::Text {
                source: ListItemTextSource::Column(0),
                style: TextStyle {
                    tone: TextTone::Secondary,
                    ..Default::default()
                },
            },
        });
    }
    ListItemTemplate {
        root: ListTemplateNode {
            layout: LayoutStyle {
                width: Length::Percent(1.0),
                gap: 4.0,
                ..Default::default()
            },
            kind: ListTemplateKind::Stack {
                orientation: Axis::Vertical,
                children,
            },
        },
    }
}
impl Controls {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn install_tile_candidate(
        &mut self,
        id: WidgetId,
        model: Rc<TileModel>,
        settings: TileSettings,
        template: Option<ListItemTemplate>,
        geometry: TileGeometry,
        keys: Vec<String>,
        cursor: Option<String>,
        offset: f32,
    ) -> Result<(), ControlError> {
        let height = self.runtime.bounds(id).map_or(320.0, |r| r.height);
        let offset = offset.min((geometry.extent - height).max(0.0));
        let (rows, headers) = self.prepare_tile_rows(
            id,
            &model,
            &settings,
            template.as_ref(),
            &geometry,
            offset,
            height,
        )?;
        let stage = self.stage_tile_rows(id, &rows, &headers)?;
        let e = self.tiles.entries.get_mut(&id).unwrap();
        e.model = model;
        e.settings = settings;
        e.template = template;
        e.geometry = geometry;
        e.keys = keys;
        e.cursor = cursor;
        e.error = None;
        {
            let mut source = e.source.borrow_mut();
            source.model = e.model.clone();
            source.settings = e.settings.clone();
        }
        self.publish_tile_rows(id, rows, headers, stage)?;
        self.set_scroll_extent(id, self.tiles.entries[&id].geometry.extent)?;
        self.scroll_to(id, offset)?;
        self.controls.get_mut(&id).unwrap().validation_error = None;
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn realize_tiles(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.tiles.entries[&id];
        let model = e.model.clone();
        let settings = e.settings.clone();
        let template = e.template.clone();
        let offset = self.scroll_offset(id).map_or(0.0, |p| p.y);
        let height = self.runtime.bounds(id).map_or(320.0, |r| r.height);
        // Geometry is not rebuilt or cloned while scrolling.
        let range = e
            .geometry
            .visible(offset, height, settings.overscan, settings.max_realized)?;
        let rows: Vec<_> = range
            .map(|pos| {
                let i = e.geometry.order[pos];
                PreparedRow {
                    key: model.items()[i].item.key.clone(),
                    item: model.items()[i].clone(),
                    rect: e.geometry.rects[i],
                    recipe: template
                        .clone()
                        .unwrap_or_else(|| default_recipe(&model.items()[i])),
                }
            })
            .collect();
        let headers =
            self.visible_tile_headers(&model, &e.geometry, offset, height, settings.overscan);
        self.preflight_tile_rows(id, &rows)?;
        let stage = self.stage_tile_rows(id, &rows, &headers)?;
        self.publish_tile_rows(id, rows, headers, stage)
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_tile_rows(
        &mut self,
        id: WidgetId,
        model: &TileModel,
        settings: &TileSettings,
        template: Option<&ListItemTemplate>,
        geometry: &TileGeometry,
        offset: f32,
        height: f32,
    ) -> Result<(Vec<PreparedRow>, Vec<PreparedHeader>), ControlError> {
        let range = geometry.visible(offset, height, settings.overscan, settings.max_realized)?;
        let rows = range
            .map(|pos| {
                let i = geometry.order[pos];
                PreparedRow {
                    key: model.items()[i].item.key.clone(),
                    item: model.items()[i].clone(),
                    rect: geometry.rects[i],
                    recipe: template
                        .cloned()
                        .unwrap_or_else(|| default_recipe(&model.items()[i])),
                }
            })
            .collect::<Vec<_>>();
        let headers = self.visible_tile_headers(model, geometry, offset, height, settings.overscan);
        self.preflight_tile_rows(id, &rows)?;
        Ok((rows, headers))
    }
    fn visible_tile_headers(
        &self,
        model: &TileModel,
        geometry: &TileGeometry,
        offset: f32,
        height: f32,
        overscan: f32,
    ) -> Vec<PreparedHeader> {
        geometry
            .headers
            .iter()
            .filter(|h| {
                h.bounds.y + h.bounds.height > offset - overscan
                    && h.bounds.y < offset + height + overscan
            })
            .map(|h| {
                let g = &model.groups()[h.group];
                (g.key.clone(), g.header.clone(), h.bounds)
            })
            .collect()
    }
    fn preflight_tile_rows(
        &mut self,
        id: WidgetId,
        rows: &[PreparedRow],
    ) -> Result<(), ControlError> {
        self.tiles.entries[&id]
            .slots
            .can_reconcile(&rows.iter().map(|r| r.key.clone()).collect::<Vec<_>>())
            .map_err(|e| tile_error(e.to_string()))?;
        for row in rows {
            self.preflight_item_template(
                &row.recipe,
                &row.item.item,
                (row.rect.width - 20.0).max(0.0),
            )?;
            let name = tile_name(&row.item);
            validate_tile_text(&name)?;
            // Names are not drawn, but share the text safety boundary.
            self.measure_text(&name, 4096.0, false)?;
        }
        Ok(())
    }
    fn stage_tile_rows(
        &mut self,
        id: WidgetId,
        rows: &[PreparedRow],
        headers: &[PreparedHeader],
    ) -> Result<Stage, ControlError> {
        let mut stage = Stage::new();
        let parent = self.tiles.entries[&id].parts.content;
        let result = (|| {
            for row in rows {
                if let Some(old) = self.tiles.entries[&id].mounted.get(&row.key) {
                    if old.recipe != row.recipe {
                        let content = self.create_list_item_template(
                            old.button,
                            &row.recipe,
                            &row.item.item,
                        )?;
                        stage.contents.insert(row.key.clone(), content);
                    }
                } else {
                    let host = self.button(
                        parent,
                        "",
                        None,
                        LayoutStyle {
                            kind: LayoutKind::Overlay,
                            padding: Edges::all(2.0),
                            ..Default::default()
                        },
                    )?;
                    let constructed = (|| {
                        let button = self.button(
                            host,
                            "",
                            None,
                            LayoutStyle {
                                kind: LayoutKind::Overlay,
                                width: Length::Percent(1.0),
                                height: Length::Percent(1.0),
                                padding: Edges::all(8.0),
                                ..Default::default()
                            },
                        )?;
                        self.set_button_style(
                            host,
                            ButtonStyle {
                                padding: Some(Edges::all(2.0)),
                                ..Default::default()
                            },
                        )?;
                        self.set_button_style(
                            button,
                            ButtonStyle {
                                padding: Some(Edges::all(8.0)),
                                ..Default::default()
                            },
                        )?;
                        self.edit_props(host, |p| {
                            p.tab_stop = false;
                            p.clip = true;
                        })?;
                        self.edit_props(button, |p| p.tab_stop = false)?;
                        self.set_role(host, Role::ListItem);
                        let content =
                            self.create_list_item_template(button, &row.recipe, &row.item.item)?;
                        Ok::<_, ControlError>((button, content))
                    })();
                    match constructed {
                        Ok((button, content)) => {
                            stage.rows.insert(row.key.clone(), (host, button, content));
                        }
                        Err(error) => {
                            self.remove(host)?;
                            return Err(error);
                        }
                    }
                }
            }
            for (key, caption, _) in headers {
                if self.tiles.entries[&id]
                    .headers
                    .get(key)
                    .is_none_or(|h| h.caption != *caption)
                {
                    let widget = self.label(parent, caption, LayoutStyle::default())?;
                    self.set_text_style(
                        widget,
                        TextStyle {
                            font_weight: Some(600),
                            ..Default::default()
                        },
                    )?;
                    self.edit_props(widget, |p| {
                        p.focusable = false;
                        p.hit_test = false;
                    })?;
                    stage.headers.insert(
                        key.clone(),
                        Header {
                            widget,
                            caption: caption.clone(),
                        },
                    );
                }
            }
            Ok::<_, ControlError>(())
        })();
        if let Err(error) = result {
            for (_, (row, _, _)) in stage.rows {
                self.remove(row)?;
            }
            for (_, content) in stage.contents {
                self.remove(content.root)?;
            }
            for (_, header) in stage.headers {
                self.remove(header.widget)?;
            }
            return Err(error);
        }
        Ok(stage)
    }
    fn publish_tile_rows(
        &mut self,
        id: WidgetId,
        rows: Vec<PreparedRow>,
        headers: Vec<PreparedHeader>,
        mut stage: Stage,
    ) -> Result<(), ControlError> {
        let keys: Vec<_> = rows.iter().map(|r| r.key.clone()).collect();
        let changes = self
            .tiles
            .entries
            .get_mut(&id)
            .unwrap()
            .slots
            .reconcile(&keys)
            .map_err(|e| tile_error(e.to_string()))?;
        for released in changes.released {
            if let Some(row) = self
                .tiles
                .entries
                .get_mut(&id)
                .unwrap()
                .mounted
                .remove(&released.key)
            {
                if self
                    .runtime
                    .focused()
                    .is_some_and(|focus| self.flyout_descendant(focus, row.row))
                {
                    self.runtime.apply(Mutation::Focus(Some(id)))?;
                }
                self.tiles.actions.remove(&row.row);
                self.tiles.actions.remove(&row.button);
                self.remove(row.row)?;
                self.tiles.entries.get_mut(&id).unwrap().released += 1;
            }
        }
        let mut order = Vec::new();
        for (row, lease) in rows.into_iter().zip(changes.active) {
            if let Some((host, button, content)) = stage.rows.remove(&row.key) {
                let e = self.tiles.entries.get_mut(&id).unwrap();
                e.created += 1;
                let parents = template_parents(self, content.root);
                self.tiles.entries.get_mut(&id).unwrap().mounted.insert(
                    row.key.clone(),
                    MountedTile {
                        row: host,
                        button,
                        content,
                        recipe: row.recipe.clone(),
                        lease: lease.lease,
                        parents,
                    },
                );
                self.tiles
                    .actions
                    .insert(host, (id, row.key.clone(), false));
                self.tiles
                    .actions
                    .insert(button, (id, row.key.clone(), true));
            } else if let Some(content) = stage.contents.remove(&row.key) {
                let parents = template_parents(self, content.root);
                let old = std::mem::replace(
                    &mut self
                        .tiles
                        .entries
                        .get_mut(&id)
                        .unwrap()
                        .mounted
                        .get_mut(&row.key)
                        .unwrap()
                        .content,
                    content,
                );
                self.remove(old.root)?;
                self.tiles
                    .entries
                    .get_mut(&id)
                    .unwrap()
                    .mounted
                    .get_mut(&row.key)
                    .unwrap()
                    .recipe = row.recipe.clone();
                self.tiles
                    .entries
                    .get_mut(&id)
                    .unwrap()
                    .mounted
                    .get_mut(&row.key)
                    .unwrap()
                    .parents = parents;
            }
            let mounted = &self.tiles.entries[&id].mounted[&row.key];
            let host = mounted.row;
            let button = mounted.button;
            let texts = mounted.content.texts.clone();
            self.edit_props(host, |p| {
                p.style.offset = Point::new(row.rect.x, row.rect.y);
                p.style.width = Length::Px(row.rect.width);
                p.style.height = Length::Px(row.rect.height);
                p.style.flex_shrink = 0.0;
            })?;
            self.set_automation_name(host, tile_name(&row.item))?;
            self.set_automation_name(button, format!("Открыть {}", row.item.item.text))?;
            self.set_enabled(host, row.item.item.enabled)?;
            self.set_enabled(
                button,
                row.item.item.enabled
                    && row
                        .item
                        .command
                        .as_ref()
                        .is_none_or(|command| self.commands.enabled(command)),
            )?;
            for text in texts {
                self.set_label(text.widget, text.source.text(&row.item.item))?;
            }
            order.push(host);
        }
        let wanted: HashSet<_> = headers.iter().map(|(key, _, _)| key.as_str()).collect();
        let removed: Vec<_> = self.tiles.entries[&id]
            .headers
            .keys()
            .filter(|key| !wanted.contains(key.as_str()) || stage.headers.contains_key(*key))
            .cloned()
            .collect();
        for key in removed {
            let old = self
                .tiles
                .entries
                .get_mut(&id)
                .unwrap()
                .headers
                .remove(&key)
                .unwrap();
            self.remove(old.widget)?;
        }
        self.tiles
            .entries
            .get_mut(&id)
            .unwrap()
            .headers
            .extend(stage.headers);
        for (key, _, rect) in headers {
            let widget = self.tiles.entries[&id].headers[&key].widget;
            self.edit_props(widget, |p| {
                p.style.offset = Point::new(rect.x, rect.y);
                p.style.width = Length::Px(rect.width);
                p.style.height = Length::Px(rect.height);
            })?;
            order.push(widget);
        }
        let parent = self.tiles.entries[&id].parts.content;
        if self.runtime.tree().children(parent) != Some(order.as_slice()) {
            self.runtime.apply(Mutation::Batch(
                order
                    .iter()
                    .map(|&node| Mutation::Reparent { node, parent })
                    .collect(),
            ))?;
        }
        self.refresh_tile_selection(id)
    }
    pub(super) fn refresh_tile_selection(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.tiles.entries[&id];
        for (key, row) in &e.mounted {
            let selected = e.keys.contains(key);
            self.controls.get_mut(&row.row).unwrap().checked = selected;
            self.controls.get_mut(&row.button).unwrap().checked = selected;
        }
        self.scene_dirty = true;
        Ok(())
    }
}
fn tile_name(tile: &TileItem) -> String {
    if tile.item.columns.is_empty() {
        tile.item.text.clone()
    } else {
        format!("{} · {}", tile.item.text, tile.item.columns.join(" · "))
    }
}
fn template_parents(ui: &Controls, root: WidgetId) -> Vec<(WidgetId, WidgetId)> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if let Some(parent) = ui.runtime.tree().parent(node) {
            result.push((node, parent));
        }
        if let Some(children) = ui.runtime.tree().children(node) {
            pending.extend_from_slice(children);
        }
    }
    result
}
