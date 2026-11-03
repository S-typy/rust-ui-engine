use super::*;
use crate::composition_ui::error;
use crate::{ControlError, Controls, composition_ui::*};

pub struct ReportDesigner {
    pub root: WidgetId,
    pub canvas: WidgetId,
    pub status: WidgetId,
    pub properties: [WidgetId; 5],
    body: WidgetId,
    settings_host: crate::ScrollViewer,
    inspector: Option<super::inspector::Inspector>,
    items: Vec<(ReportSelection, WidgetId)>,
    session: ReportDesignerSession,
    actions: Vec<(WidgetId, &'static str)>,
    band: ReportBandKind,
    preview: bool,
    page: usize,
    scale: f32,
    geometry: Vec<(ReportSelection, Rect)>,
    loaded: Option<(ReportRevision, Option<ReportSelection>)>,
    drag: Option<(ReportRevision, Point, Rect, bool)>,
    drag_position: Option<Point>,
    last_bounds: Option<Rect>,
}
impl ReportDesigner {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        session: ReportDesignerSession,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let root = frame(ui, parent, style)?;
        let result = (|| {
            let actions = toolbar(
                ui,
                root,
                1,
                &[
                    ("Текст", "text"),
                    ("Прямоугольник", "rectangle"),
                    ("Изображение", "image"),
                    ("Секция", "band"),
                    ("Следующий", "next"),
                    ("Удалить", "delete"),
                    ("Отменить", "undo"),
                    ("Повторить", "redo"),
                    ("Просмотр", "preview"),
                    ("Настройки", "settings"),
                    ("Применить настройки", "settings_apply"),
                    ("Отмена настроек", "settings_cancel"),
                ],
            )?;
            let body = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0), GridTrack::Px(230.0)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    gap: 8.0,
                    ..cell(2, 1)
                },
            )?;
            let canvas = ui.button(
                body,
                "",
                None,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    padding: Edges::all(0.0),
                    height: Length::Percent(1.0),
                    width: Length::Percent(1.0),
                    ..cell(1, 1)
                },
            )?;
            ui.set_automation_name(
                canvas,
                "Report design surface; arrows move selected item, Shift resizes",
            )?;
            let inspector = ui
                .scroll_view(
                    body,
                    LayoutStyle {
                        kind: LayoutKind::Stack(Axis::Vertical),
                        gap: 4.0,
                        ..cell(1, 2)
                    },
                    540.0,
                )?
                .content;
            ui.label(
                inspector,
                "Свойства элемента",
                LayoutStyle {
                    height: Length::Px(30.0),
                    ..Default::default()
                },
            )?;
            let mut properties = Vec::new();
            for title in [
                "X",
                "Y",
                "Ширина",
                "Размер текста / высота",
                "Текст или =выражение",
            ] {
                ui.label(
                    inspector,
                    title,
                    LayoutStyle {
                        height: Length::Px(23.0),
                        ..Default::default()
                    },
                )?;
                properties.push(ui.text_box(
                    inspector,
                    title,
                    "",
                    LayoutStyle {
                        height: Length::Px(34.0),
                        ..Default::default()
                    },
                )?);
            }
            let apply = ui.button(
                inspector,
                "Применить",
                None,
                LayoutStyle {
                    height: Length::Px(34.0),
                    ..Default::default()
                },
            )?;
            let cancel = ui.button(
                inspector,
                "Вернуть свойства",
                None,
                LayoutStyle {
                    height: Length::Px(34.0),
                    ..Default::default()
                },
            )?;
            let status = ui.label(
                root,
                "",
                LayoutStyle {
                    height: Length::Px(30.0),
                    ..cell(3, 1)
                },
            )?;
            let settings_host = ui.scroll_view(
                root,
                LayoutStyle {
                    height: Length::Percent(1.0),
                    ..cell(2, 1)
                },
                4000.0,
            )?;
            ui.configure_scroll_view(
                settings_host.viewport,
                crate::ScrollSettings {
                    horizontal: crate::ScrollBarVisibility::Auto,
                    vertical: crate::ScrollBarVisibility::Auto,
                },
            )?;
            visible(ui, settings_host.viewport, false)?;
            let mut actions = actions;
            actions.extend([(apply, "apply"), (cancel, "cancel")]);
            Ok(Self {
                root,
                canvas,
                status,
                properties: properties.try_into().unwrap(),
                body,
                settings_host,
                inspector: None,
                items: Vec::new(),
                session,
                actions,
                band: ReportBandKind::Detail,
                preview: false,
                page: 0,
                scale: 1.0,
                geometry: Vec::new(),
                loaded: None,
                drag: None,
                drag_position: None,
                last_bounds: None,
            })
        })();
        if result.is_err() {
            let _ = ui.remove(root);
        }
        result
    }
    pub fn session(&self) -> &ReportDesignerSession {
        &self.session
    }
    pub fn session_mut(&mut self) -> &mut ReportDesignerSession {
        &mut self.session
    }
    pub fn is_preview(&self) -> bool {
        self.preview
    }
    pub fn set_preview(&mut self, value: bool) {
        self.preview = value;
        self.drag = None;
        self.page = 0;
    }
    pub fn item_widgets(&self) -> &[(ReportSelection, WidgetId)] {
        &self.items
    }
    pub fn property_grid(&self) -> Option<&crate::PropertyGrid> {
        self.inspector.as_ref().map(|i| &i.grid)
    }
    pub fn property_grid_mut(&mut self) -> Option<&mut crate::PropertyGrid> {
        self.inspector.as_mut().map(|i| &mut i.grid)
    }
    pub fn action_widget(&self, action: &str) -> Option<WidgetId> {
        self.actions
            .iter()
            .find_map(|(id, a)| (*a == action).then_some(*id))
    }
    /// Consume PropertyGrid actions before draining the host's generic activations.
    pub fn process(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(i) = &mut self.inspector {
            i.grid.update(ui)?;
            i.grid.take_changes();
        }
        Ok(())
    }
    pub fn cancel_interaction(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.drag = None;
        self.drag_position = None;
        if ui.runtime.captured() == Some(self.canvas) {
            ui.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    fn close_settings(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(i) = self.inspector.take() {
            ui.remove(i.grid.container)?;
        }
        visible(ui, self.settings_host.viewport, false)?;
        visible(ui, self.body, true)?;
        ui.perform_action(self.canvas, crate::SemanticAction::Focus, None)
    }
    fn selected(&self) -> Option<&ReportItem> {
        let s = self.session.selection()?;
        s.band.band(self.session.definition()).items.get(s.item)
    }
    fn load_properties(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let key = (self.session.revision(), self.session.selection());
        if self.loaded == Some(key) {
            return Ok(());
        }
        let values = match self.selected() {
            Some(ReportItem::Text {
                x,
                y,
                width,
                font_size,
                value,
                ..
            }) => vec![
                x.to_string(),
                y.to_string(),
                width.to_string(),
                font_size.to_string(),
                match value {
                    ReportValue::Literal(s) if s.starts_with(['=', '@', '#', '\'']) => {
                        format!("'{s}")
                    }
                    ReportValue::Literal(s) => s.clone(),
                    ReportValue::Expression(e) => {
                        format!("={}", format_expression_text(e).map_err(error)?)
                    }
                    ReportValue::Parameter(p) => format!("@{p}"),
                    v => format!("#{v:?}"),
                },
            ],
            Some(ReportItem::Rectangle { bounds, .. } | ReportItem::Image { bounds, .. }) => vec![
                bounds.x.to_string(),
                bounds.y.to_string(),
                bounds.width.to_string(),
                bounds.height.to_string(),
                String::new(),
            ],
            None => vec![String::new(); 5],
        };
        for (id, value) in self.properties.into_iter().zip(values) {
            ui.set_editor_value(id, &value)?;
        }
        self.loaded = Some(key);
        Ok(())
    }
    fn apply_properties(&mut self, ui: &Controls) -> Result<(), ControlError> {
        let Some(mut item) = self.selected().cloned() else {
            return Err(error("Select an item"));
        };
        if self
            .properties
            .iter()
            .any(|id| ui.document(*id).is_some_and(|d| d.composition().is_some()))
        {
            return Err(error("Finish text composition"));
        }
        let values = self
            .properties
            .iter()
            .map(|id| ui.document(*id).unwrap().text().to_owned())
            .collect::<Vec<_>>();
        let numbers = values[..4]
            .iter()
            .map(|s| s.parse::<f32>().map_err(error))
            .collect::<Result<Vec<_>, _>>()?;
        match &mut item {
            ReportItem::Text {
                x,
                y,
                width,
                font_size,
                value,
                ..
            } => {
                *x = numbers[0];
                *y = numbers[1];
                *width = numbers[2];
                *font_size = numbers[3];
                let preserve = !matches!(
                    value,
                    ReportValue::Literal(_)
                        | ReportValue::Expression(_)
                        | ReportValue::Parameter(_)
                ) && values[4] == format!("#{value:?}");
                if preserve {
                    // The opaque display of a total/page/group value is unchanged.
                } else if let Some(literal) = values[4].strip_prefix('\'') {
                    *value = ReportValue::Literal(literal.into());
                } else if let Some(e) = values[4].strip_prefix('=') {
                    *value = ReportValue::Expression(
                        parse_expression_text(self.session.table().schema(), e).map_err(error)?,
                    );
                } else if let Some(p) = values[4].strip_prefix('@') {
                    *value = ReportValue::Parameter(p.into());
                } else {
                    *value = ReportValue::Literal(values[4].clone());
                }
            }
            ReportItem::Rectangle { bounds, .. } | ReportItem::Image { bounds, .. } => {
                *bounds = Rect::new(numbers[0], numbers[1], numbers[2], numbers[3])
            }
        }
        self.session.replace_selected(item).map_err(error)
    }
    pub fn activate(&mut self, ui: &mut Controls, id: WidgetId) -> Result<bool, ControlError> {
        if let Some(selection) = self
            .items
            .iter()
            .find_map(|(s, w)| (*w == id).then_some(*s))
        {
            self.session.select(Some(selection)).map_err(error)?;
            self.band = selection.band;
            self.load_properties(ui)?;
            return Ok(true);
        }
        let Some(action) = self
            .actions
            .iter()
            .find_map(|(i, a)| (*i == id).then_some(*a))
        else {
            return Ok(id == self.canvas);
        };
        match action {
            "settings" => {
                self.cancel_interaction(ui)?;
                if self.inspector.is_none() {
                    self.inspector = Some(super::inspector::Inspector::mount(
                        ui,
                        self.settings_host.content,
                        &self.session,
                    )?);
                    visible(ui, self.body, false)?;
                    visible(ui, self.settings_host.viewport, true)?;
                }
            }
            "settings_apply" => {
                if let Some(i) = &mut self.inspector {
                    i.apply(ui, &mut self.session)?;
                }
                self.close_settings(ui)?;
            }
            "settings_cancel" => self.close_settings(ui)?,
            "text" => self
                .session
                .add(
                    self.band,
                    ReportItem::Text {
                        x: 8.0,
                        y: 0.0,
                        width: 180.0,
                        font_size: 14.0,
                        bold: false,
                        color: Color::rgb(30, 35, 42),
                        value: ReportValue::Literal("Новый текст".into()),
                    },
                )
                .map_err(error)?,
            "rectangle" => self
                .session
                .add(
                    self.band,
                    ReportItem::Rectangle {
                        bounds: Rect::new(8.0, 0.0, 120.0, 20.0),
                        color: Color::rgb(118, 180, 225),
                    },
                )
                .map_err(error)?,
            "image" => self
                .session
                .add(
                    self.band,
                    ReportItem::Image {
                        bounds: Rect::new(8.0, 0.0, 24.0, 24.0),
                        bitmap: Bitmap::new_rgba8(
                            2,
                            2,
                            vec![
                                0, 100, 200, 255, 20, 140, 230, 255, 20, 140, 230, 255, 0, 100,
                                200, 255,
                            ],
                        )
                        .map_err(error)?,
                    },
                )
                .map_err(error)?,
            "band" => {
                let n = ReportBandKind::ALL
                    .iter()
                    .position(|b| *b == self.band)
                    .unwrap();
                self.band = ReportBandKind::ALL[(n + 1) % 7];
            }
            "next" => {
                if self.preview {
                    self.page = (self.page + 1) % self.session.preview().document.pages().len();
                } else {
                    let all = ReportBandKind::ALL
                        .into_iter()
                        .flat_map(|band| {
                            (0..band.band(self.session.definition()).items.len())
                                .map(move |item| ReportSelection { band, item })
                        })
                        .collect::<Vec<_>>();
                    let n = all
                        .iter()
                        .position(|s| Some(*s) == self.session.selection())
                        .map_or(0, |n| (n + 1) % all.len().max(1));
                    self.session.select(all.get(n).copied()).map_err(error)?;
                }
            }
            "delete" => self.session.delete_selected().map_err(error)?,
            "undo" => {
                self.session.undo().map_err(error)?;
            }
            "redo" => {
                self.session.redo().map_err(error)?;
            }
            "preview" => self.set_preview(!self.preview),
            "apply" => {
                self.apply_properties(ui)?;
                ui.perform_action(self.canvas, crate::SemanticAction::Focus, None)?;
            }
            "cancel" => {
                self.loaded = None;
                ui.perform_action(self.canvas, crate::SemanticAction::Focus, None)?;
            }
            _ => {}
        }
        self.load_properties(ui)?;
        Ok(true)
    }
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        if let Some(i) = &mut self.inspector {
            return i.grid.input(ui, event);
        }
        if self
            .properties
            .iter()
            .any(|id| ui.document(*id).is_some_and(|d| d.composition().is_some()))
        {
            return Ok(false);
        }
        match event {
            InputEvent::WindowFocused(false) => {
                self.cancel_interaction(ui)?;
                Ok(false)
            }
            InputEvent::PointerMoved { position } if self.drag.is_some() => {
                self.drag_position = Some(*position);
                Ok(true)
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if !self.preview
                && self
                    .last_bounds
                    .is_some_and(|b| b.contains(position.x, position.y)) =>
            {
                if let Some((selection, bounds)) = self
                    .geometry
                    .iter()
                    .rev()
                    .find(|(_, r)| r.contains(position.x, position.y))
                    .copied()
                {
                    self.session.select(Some(selection)).map_err(error)?;
                    self.band = selection.band;
                    self.drag = Some((self.session.revision(), *position, bounds, modifiers.shift));
                    self.drag_position = Some(*position);
                    ui.runtime.apply(Mutation::Focus(Some(self.canvas)))?;
                    ui.runtime
                        .apply(Mutation::CapturePointer(Some(self.canvas)))?;
                    self.load_properties(ui)?;
                } else {
                    self.session.select(None).map_err(error)?;
                    self.drag = None;
                }
                Ok(true)
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } if self.drag.is_some() => {
                let (revision, start, _, resize) = self.drag.take().unwrap();
                self.cancel_interaction(ui)?;
                if revision != self.session.revision() {
                    return Ok(true);
                }
                self.move_or_resize(
                    (position.x - start.x) / self.scale,
                    (position.y - start.y) / self.scale,
                    resize,
                )?;
                self.load_properties(ui)?;
                Ok(true)
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if !self.preview
                    && (ui.runtime.focused() == Some(self.canvas)
                        || self
                            .items
                            .iter()
                            .any(|(_, id)| Some(*id) == ui.runtime.focused())) =>
            {
                if let Some(s) = self
                    .items
                    .iter()
                    .find_map(|(s, id)| (Some(*id) == ui.runtime.focused()).then_some(*s))
                {
                    self.session.select(Some(s)).map_err(error)?;
                    self.band = s.band;
                }
                let step = if modifiers.control { 1.0 } else { 8.0 };
                match key {
                    Key::ArrowLeft => self.move_or_resize(-step, 0.0, modifiers.shift)?,
                    Key::ArrowRight => self.move_or_resize(step, 0.0, modifiers.shift)?,
                    Key::ArrowUp => self.move_or_resize(0.0, -step, modifiers.shift)?,
                    Key::ArrowDown => self.move_or_resize(0.0, step, modifiers.shift)?,
                    Key::Delete => self.session.delete_selected().map_err(error)?,
                    Key::Escape => self.cancel_interaction(ui)?,
                    _ => return Ok(false),
                }
                self.load_properties(ui)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn move_or_resize(&mut self, dx: f32, dy: f32, resize: bool) -> Result<(), ControlError> {
        let Some(mut item) = self.selected().cloned() else {
            return Ok(());
        };
        match &mut item {
            ReportItem::Text {
                x,
                y,
                width,
                font_size,
                ..
            } => {
                if resize {
                    *width += dx;
                    *font_size += dy
                } else {
                    *x += dx;
                    *y += dy
                }
            }
            ReportItem::Rectangle { bounds, .. } | ReportItem::Image { bounds, .. } => {
                if resize {
                    bounds.width += dx;
                    bounds.height += dy
                } else {
                    bounds.x += dx;
                    bounds.y += dy
                }
            }
        }
        self.session.replace_selected(item).map_err(error)
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(i) = &mut self.inspector {
            i.grid.refresh(ui)?;
            if let (Some(grid), Some(view)) = (
                ui.runtime.bounds(i.grid.container),
                ui.runtime.bounds(self.settings_host.viewport),
            ) {
                ui.set_scroll_extent_size(
                    self.settings_host.viewport,
                    Size::new(view.width.max(760.0), grid.height + 8.0),
                )?;
            }
            return Ok(());
        }
        self.load_properties(ui)?;
        let Some(bounds) = ui.runtime.bounds(self.canvas) else {
            return Ok(());
        };
        if self.last_bounds != Some(bounds) {
            self.cancel_interaction(ui)?;
        }
        if self.drag.is_none() && ui.runtime.captured() == Some(self.canvas) {
            self.cancel_interaction(ui)?;
        }
        self.last_bounds = Some(bounds);
        let tokens = ui.theme().tokens();
        let mut scene = Scene::default();
        scene.fill(bounds, tokens.canvas);
        self.geometry.clear();
        if self.preview {
            self.page = self
                .page
                .min(self.session.preview().document.pages().len() - 1);
            let page = &self.session.preview().document.pages()[self.page];
            let scale = ((bounds.width - 16.0) / page.size.width)
                .min((bounds.height - 16.0) / page.size.height)
                .clamp(0.01, 4.0);
            scene
                .append_view(
                    &pages::transform_page(
                        &page.scene,
                        scale,
                        Point::new(bounds.x + 8.0, bounds.y + 8.0),
                        bounds,
                    )
                    .map_err(error)?,
                    Point::ZERO,
                    bounds,
                )
                .map_err(error)?;
        } else {
            let d = self.session.definition();
            let width = d.page_size.width - d.margins[1] - d.margins[3];
            let height: f32 = ReportBandKind::ALL
                .iter()
                .map(|b| b.band(d).height.max(32.0) + 26.0)
                .sum();
            self.scale = ((bounds.width - 20.0) / width)
                .min((bounds.height - 16.0) / height)
                .clamp(0.01, 2.0);
            let x = bounds.x + 10.0;
            let mut y = bounds.y + 8.0;
            for band in ReportBandKind::ALL {
                let b = band.band(d);
                let h = b.height.max(32.0) * self.scale;
                let header = 26.0 * self.scale;
                scene.fill(
                    Rect::new(x, y, width * self.scale, header),
                    if band == self.band {
                        tokens.selected
                    } else {
                        tokens.hover
                    },
                );
                text(
                    &mut scene,
                    format!("{} · {} px", band.title(), b.height),
                    Rect::new(x + 4.0, y, width * self.scale - 8.0, header),
                    tokens.text,
                    (13.0 * self.scale).max(8.0),
                );
                y += header;
                scene.fill(
                    Rect::new(x, y, width * self.scale, h),
                    Color::rgb(255, 255, 255),
                );
                for n in 1..(width / 40.0) as usize {
                    scene.fill(
                        Rect::new(x + n as f32 * 40.0 * self.scale, y, 0.5, h),
                        Color::rgb(230, 234, 240),
                    );
                }
                for (index, item) in b.items.iter().enumerate() {
                    let r = match item {
                        ReportItem::Text {
                            x: ix,
                            y: iy,
                            width,
                            font_size,
                            color,
                            value,
                            bold,
                        } => {
                            let r = Rect::new(
                                x + ix * self.scale,
                                y + iy * self.scale,
                                width * self.scale,
                                font_size * 1.5 * self.scale,
                            );
                            let label = match value {
                                ReportValue::Literal(s) => s.clone(),
                                ReportValue::Expression(e) => {
                                    format_expression_text(e).map_err(error)?
                                }
                                v => format!("{v:?}"),
                            };
                            let mut run = TextRun::new(label, r, *color, *font_size * self.scale);
                            run.weight = if *bold { 700 } else { 400 };
                            scene.text(run);
                            r
                        }
                        ReportItem::Rectangle { bounds: r, color } => {
                            let r = Rect::new(
                                x + r.x * self.scale,
                                y + r.y * self.scale,
                                r.width * self.scale,
                                r.height * self.scale,
                            );
                            scene.fill(r, *color);
                            r
                        }
                        ReportItem::Image { bounds: r, bitmap } => {
                            let r = Rect::new(
                                x + r.x * self.scale,
                                y + r.y * self.scale,
                                r.width * self.scale,
                                r.height * self.scale,
                            );
                            scene.image(ImagePrimitive::new(bitmap.clone(), r));
                            r
                        }
                    };
                    let selection = ReportSelection { band, item: index };
                    self.geometry.push((selection, r));
                    scene.rounded_stroke(
                        r,
                        bounds,
                        0.0,
                        if self.session.selection() == Some(selection) {
                            2.0
                        } else {
                            0.5
                        },
                        if self.session.selection() == Some(selection) {
                            tokens.focus
                        } else {
                            tokens.border
                        },
                    );
                }
                y += h;
            }
        }
        if let (Some((revision, start, rect, resize)), Some(p)) = (self.drag, self.drag_position)
            && revision == self.session.revision()
        {
            let mut r = rect;
            if resize {
                r.width = (r.width + p.x - start.x).max(1.0);
                r.height = (r.height + p.y - start.y).max(1.0);
            } else {
                r.x += p.x - start.x;
                r.y += p.y - start.y;
            }
            scene.rounded_stroke(r, bounds, 0.0, 2.0, tokens.focus);
        }
        self.refresh_items(ui, bounds)?;
        ui.set_custom_scene(self.canvas, scene)?;
        ui.set_label(
            self.status,
            format!(
                "{} · секция {} · страниц {} · {}",
                self.session.definition().title,
                self.band.title(),
                self.session.preview().document.pages().len(),
                if self.preview {
                    "Предпросмотр принятой версии"
                } else {
                    "Стрелки — сдвиг; Shift — размер; Ctrl — шаг 1"
                }
            ),
        )?;
        for (id, action) in &self.actions {
            let enabled = match *action {
                "undo" => self.session.can_undo(),
                "redo" => self.session.can_redo(),
                "apply" | "delete" => self.session.selection().is_some() && !self.preview,
                _ => true,
            };
            ui.set_enabled(*id, enabled)?;
        }
        Ok(())
    }
    fn refresh_items(&mut self, ui: &mut Controls, bounds: Rect) -> Result<(), ControlError> {
        let mut retained = Vec::new();
        for (s, id) in self.items.drain(..) {
            if self.geometry.iter().any(|(selection, _)| *selection == s) {
                retained.push((s, id));
            } else {
                if ui.runtime.focused() == Some(id) {
                    ui.perform_action(self.canvas, crate::SemanticAction::Focus, None)?;
                }
                ui.remove(id)?;
            }
        }
        self.items = retained;
        for &(selection, rect) in &self.geometry {
            let style = LayoutStyle {
                width: Length::Px(rect.width),
                height: Length::Px(rect.height),
                offset: Point::new(rect.x - bounds.x, rect.y - bounds.y),
                ..Default::default()
            };
            let id = if let Some((_, id)) = self.items.iter().find(|(s, _)| *s == selection) {
                *id
            } else {
                let id = ui.button(self.canvas, "", None, style.clone())?;
                let appearance = crate::ButtonAppearance {
                    background: Some(Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    border: Some(Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    ..Default::default()
                };
                ui.set_button_style(
                    id,
                    crate::ButtonStyle {
                        normal: appearance,
                        hovered: appearance,
                        pressed: appearance,
                        checked: appearance,
                        disabled: appearance,
                        padding: Some(Edges::all(0.0)),
                        ..Default::default()
                    },
                )?;
                self.items.push((selection, id));
                id
            };
            let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
            if props.style != style {
                props.style = style;
                ui.runtime.apply(Mutation::SetProps { node: id, props })?;
            }
            let item = &selection.band.band(self.session.definition()).items[selection.item];
            let description = match item {
                ReportItem::Text {
                    value,
                    x,
                    y,
                    width,
                    font_size,
                    bold,
                    ..
                } => {
                    let value = match value {
                        ReportValue::Literal(s) => s.clone(),
                        ReportValue::Expression(e) => {
                            format!("Выражение {}", format_expression_text(e).map_err(error)?)
                        }
                        ReportValue::Parameter(p) => format!("Параметр {p}"),
                        ReportValue::GroupKey(i) => format!("Ключ группы {i}"),
                        ReportValue::GroupTotal(i) => format!("Итог группы {i}"),
                        ReportValue::Total(i) => format!("Общий итог {i}"),
                        ReportValue::PageNumber => "Номер страницы".into(),
                        ReportValue::PageCount => "Количество страниц".into(),
                    };
                    format!(
                        "Текст {value}; X {x}, Y {y}, ширина {width}, размер {font_size}, полужирный {}",
                        if *bold { "да" } else { "нет" }
                    )
                }
                ReportItem::Rectangle { bounds, .. } => format!("Прямоугольник {bounds:?}"),
                ReportItem::Image { bounds, .. } => format!("Изображение {bounds:?}"),
            };
            ui.set_automation_name(
                id,
                format!(
                    "{}; элемент {}; {}; {}",
                    selection.band.title(),
                    selection.item + 1,
                    description,
                    if self.session.selection() == Some(selection) {
                        "Выбран"
                    } else {
                        "Не выбран"
                    }
                ),
            )?;
        }
        Ok(())
    }
}
fn visible(ui: &mut Controls, id: WidgetId, value: bool) -> Result<(), ControlError> {
    let mut props = ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| error("Designer was removed"))?
        .props
        .clone();
    props.collapsed = !value;
    ui.runtime.apply(Mutation::SetProps { node: id, props })?;
    Ok(())
}
