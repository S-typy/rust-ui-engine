use super::*;
use crate::composition_ui::error;
use crate::{composition_ui::*, *};
use rust_desktop_ui_core::*;
use std::{cell::RefCell, rc::Rc};

struct Tile {
    id: String,
    heading: WidgetId,
    control: WidgetId,
    source: Option<DataSourceHandle>,
    selected: Option<i64>,
}
pub struct Dashboard {
    pub root: WidgetId,
    pub status: WidgetId,
    pub title_editor: WidgetId,
    session: DashboardSession,
    body: WidgetId,
    body_host: WidgetId,
    tiles: Vec<Tile>,
    actions: Vec<(WidgetId, &'static str)>,
    loaded: DashboardRevision,
    structure: String,
    designer: bool,
    selected: Option<String>,
}
impl Dashboard {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        session: DashboardSession,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let root = frame(ui, parent, style)?;
        let result = (|| {
            let actions = toolbar(
                ui,
                root,
                1,
                &[
                    ("Дизайнер / просмотр", "mode"),
                    ("Сброс фильтров", "clear"),
                    ("Добавить таблицу", "add"),
                    ("Сдвинуть", "move"),
                    ("Удалить", "delete"),
                    ("Отменить", "undo"),
                    ("Повторить", "redo"),
                    ("Название", "title"),
                ],
            )?;
            let body_host = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    grid_rows: vec![GridTrack::Fr(1.0), GridTrack::Px(36.0)],
                    gap: 5.0,
                    ..cell(2, 1)
                },
            )?;
            let title_editor =
                ui.text_box(body_host, "Название выбранного элемента", "", cell(2, 1))?;
            let (body, tiles) = build(ui, body_host, &session)?;
            let status = ui.label(
                root,
                "",
                LayoutStyle {
                    height: Length::Px(28.0),
                    ..cell(3, 1)
                },
            )?;
            let structure = structure(session.definition());
            let loaded = session.revision();
            Ok(Self {
                root,
                status,
                title_editor,
                session,
                body,
                body_host,
                tiles,
                actions,
                loaded,
                structure,
                designer: false,
                selected: None,
            })
        })();
        if result.is_err() {
            let _ = ui.remove(root);
        }
        result
    }
    pub fn session(&self) -> &DashboardSession {
        &self.session
    }
    pub fn session_mut(&mut self) -> &mut DashboardSession {
        &mut self.session
    }
    pub fn set_designer(&mut self, value: bool) {
        self.designer = value;
    }
    pub fn is_designer(&self) -> bool {
        self.designer
    }
    pub fn widget(&self, id: &str) -> Option<WidgetId> {
        self.tiles.iter().find(|t| t.id == id).map(|t| t.control)
    }
    pub fn activate(&mut self, ui: &mut Controls, id: WidgetId) -> Result<bool, ControlError> {
        if let Some(tile) = self.tiles.iter().find(|t| t.heading == id) {
            self.selected = Some(tile.id.clone());
            let item = self
                .session
                .definition
                .items
                .iter()
                .find(|i| i.id == tile.id)
                .unwrap();
            ui.set_editor_value(self.title_editor, &item.title)?;
            return Ok(true);
        }
        let Some(action) = self
            .actions
            .iter()
            .find_map(|(i, a)| (*i == id).then_some(*a))
        else {
            return Ok(false);
        };
        match action {
            "mode" => self.designer = !self.designer,
            "clear" => {
                self.session.clear_filters().map_err(error)?;
            }
            "undo" if self.designer => {
                self.session.undo().map_err(error)?;
            }
            "redo" if self.designer => {
                self.session.redo().map_err(error)?;
            }
            "add" if self.designer => {
                let mut d = self.session.definition.clone();
                let source = self
                    .session
                    .sources
                    .keys()
                    .next()
                    .ok_or_else(|| error("No data source"))?
                    .clone();
                let row = d
                    .items
                    .iter()
                    .map(|i| i.placement.row + i.placement.row_span)
                    .max()
                    .unwrap_or(0);
                let mut n = 1;
                while d.items.iter().any(|i| i.id == format!("grid-{n}")) {
                    n += 1;
                }
                d.items.push(DashboardItem {
                    id: format!("grid-{n}"),
                    title: "Новая таблица".into(),
                    source,
                    widget: DashboardWidget::Grid,
                    placement: DashboardPlacement {
                        row,
                        column: 0,
                        row_span: 1,
                        column_span: d.columns,
                    },
                    query: QueryRequest::default(),
                });
                self.session
                    .replace(self.session.revision(), d)
                    .map_err(error)?;
            }
            "delete" | "move" | "title" if self.designer => {
                let id = self
                    .selected
                    .as_ref()
                    .ok_or_else(|| error("Select a tile heading"))?;
                let mut d = self.session.definition.clone();
                let index = d
                    .items
                    .iter()
                    .position(|i| &i.id == id)
                    .ok_or_else(|| error("Selected widget was removed"))?;
                match action {
                    "delete" => {
                        d.items.remove(index);
                    }
                    "move" if d.items.len() > 1 => {
                        let other = (index + 1) % d.items.len();
                        let p = d.items[index].placement;
                        d.items[index].placement = d.items[other].placement;
                        d.items[other].placement = p;
                    }
                    "title" => {
                        if ui
                            .document(self.title_editor)
                            .unwrap()
                            .composition()
                            .is_some()
                        {
                            return Err(error("Finish text composition"));
                        }
                        d.items[index].title =
                            ui.document(self.title_editor).unwrap().text().into();
                    }
                    _ => {}
                }
                self.session
                    .replace(self.session.revision(), d)
                    .map_err(error)?;
            }
            _ => {}
        }
        self.refresh(ui)?;
        Ok(true)
    }
    /// Call after dispatch and before refreshing. Rendering does not generate events.
    pub fn selection_changed(&mut self, ui: &Controls) -> Result<bool, ControlError> {
        let mut changes = Vec::new();
        for tile in &mut self.tiles {
            let item = self
                .session
                .definition
                .items
                .iter()
                .find(|i| i.id == tile.id)
                .unwrap();
            let selected = match item.widget {
                DashboardWidget::Chart { .. } => ui
                    .chart_model(tile.control)
                    .and_then(|m| m.selected().map(|s| s.point)),
                DashboardWidget::Grid => ui.grid_selected_key(tile.control),
                _ => None,
            };
            if selected == tile.selected {
                continue;
            }
            tile.selected = selected;
            let table = &self.session.sources[&item.source];
            let field = match &item.widget {
                DashboardWidget::Chart { x, .. } => x.as_str(),
                _ => table.schema().fields()[0].name.as_str(),
            };
            let filter = selected
                .and_then(|key| table.row(key))
                .map(|row| DashboardFilter {
                    field: field.into(),
                    values: vec![row.values[table.schema().field_index(field).unwrap()].clone()],
                });
            changes.push((tile.id.clone(), filter));
        }
        let changed = !changes.is_empty();
        for (id, filter) in changes {
            self.session
                .set_filter(self.session.revision(), &id, filter)
                .map_err(error)?;
        }
        Ok(changed)
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let next_structure = structure(self.session.definition());
        if self.structure != next_structure {
            let (body, tiles) = build(ui, self.body_host, &self.session)?;
            ui.remove(self.body)?;
            self.body = body;
            self.tiles = tiles;
            self.structure = next_structure;
            self.loaded = self.session.revision();
        } else if self.loaded != self.session.revision() {
            for tile in &mut self.tiles {
                let result = &self.session.results[&tile.id];
                let item = self
                    .session
                    .definition
                    .items
                    .iter()
                    .find(|i| i.id == tile.id)
                    .unwrap();
                if let Some(source) = &tile.source {
                    source
                        .borrow_mut()
                        .replace_adapter(
                            CollectionAdapter::new(result.table.clone()),
                            DataQuery::default(),
                        )
                        .map_err(error)?;
                    // Refreshing the source is not a new user selection.
                    ui.set_grid_selected_key(
                        tile.control,
                        tile.selected.filter(|k| result.rows.contains(k)),
                    )?;
                    tile.selected = ui.grid_selected_key(tile.control);
                }
                match &item.widget {
                    DashboardWidget::Chart { x, y } => {
                        let mut model = chart(result, item, x, y)?;
                        if let Some(key) = tile.selected.filter(|k| result.rows.contains(k)) {
                            model
                                .select(Some(ChartPointKey {
                                    series: 1,
                                    point: key,
                                }))
                                .map_err(error)?;
                        }
                        ui.set_chart_model(tile.control, model)?;
                        tile.selected = ui
                            .chart_model(tile.control)
                            .unwrap()
                            .selected()
                            .map(|s| s.point);
                    }
                    DashboardWidget::Metric { aggregate } => {
                        ui.set_label(tile.control, result.totals[*aggregate].display())?
                    }
                    _ => {}
                }
            }
            self.loaded = self.session.revision();
        }
        ui.set_label(
            self.status,
            format!(
                "{} · {} · фильтров {} · элементов {}",
                self.session.definition.title,
                if self.designer {
                    "Дизайнер: выберите заголовок"
                } else {
                    "Просмотр: выбор строки / точки фильтрует соседние элементы"
                },
                self.session.filters.len(),
                self.tiles.len()
            ),
        )?;
        for (id, a) in &self.actions {
            ui.set_enabled(*id, self.designer || matches!(*a, "mode" | "clear"))?;
        }
        ui.set_enabled(self.title_editor, self.designer)?;
        Ok(())
    }
}
fn structure(d: &DashboardDefinition) -> String {
    format!(
        "{}:{:?}",
        d.columns,
        d.items
            .iter()
            .map(|i| (&i.id, &i.title, &i.source, &i.widget, i.placement))
            .collect::<Vec<_>>()
    )
}
fn chart(
    r: &DashboardResult,
    i: &DashboardItem,
    x: &str,
    y: &str,
) -> Result<ChartModel, ControlError> {
    ChartModel::new(
        vec![ChartSeries::from_table(&r.table, 1, &i.title, x, y).map_err(error)?],
        ChartSettings {
            view: ChartView::Bar,
            x_title: x.into(),
            y_title: y.into(),
            ..Default::default()
        },
    )
    .map_err(error)
}
fn build(
    ui: &mut Controls,
    parent: WidgetId,
    session: &DashboardSession,
) -> Result<(WidgetId, Vec<Tile>), ControlError> {
    let d = session.definition();
    let rows = d
        .items
        .iter()
        .map(|i| i.placement.row + i.placement.row_span)
        .max()
        .unwrap_or(1);
    // A full chart needs 266 DIP plus its tile heading and padding.
    let row_height = 320.0;
    let extent =
        (rows as f32 * row_height + (rows.saturating_sub(1)) as f32 * 10.0).max(row_height);
    let scroll = ui.scroll_view(parent, cell(1, 1), extent)?;
    let body = scroll.viewport;
    ui.runtime.apply(Mutation::SetStyle {
        node: scroll.content,
        style: LayoutStyle {
            kind: LayoutKind::Grid,
            grid_columns: vec![GridTrack::Fr(1.0); d.columns],
            grid_rows: vec![GridTrack::Px(row_height); rows],
            height: Length::Px(extent),
            width: Length::Percent(1.0),
            gap: 10.0,
            ..Default::default()
        },
    })?;
    let result = (|| {
        let mut tiles = Vec::new();
        for item in &d.items {
            let p = item.placement;
            let r = &session.results[&item.id];
            let tile = ui.panel(
                scroll.content,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    grid_rows: vec![GridTrack::Px(34.0), GridTrack::Fr(1.0)],
                    padding: Edges::all(5.0),
                    grid_row: GridPlacement {
                        start: Some((p.row + 1) as u16),
                        span: p.row_span as u16,
                    },
                    grid_column: GridPlacement {
                        start: Some((p.column + 1) as u16),
                        span: p.column_span as u16,
                    },
                    ..Default::default()
                },
            )?;
            let heading = ui.button(tile, &item.title, None, cell(1, 1))?;
            let mut source = None;
            let control = match &item.widget {
                DashboardWidget::Chart { x, y } => ui.chart(
                    tile,
                    chart(r, item, x, y)?,
                    LayoutStyle {
                        width: Length::Percent(1.0),
                        height: Length::Percent(1.0),
                        ..cell(2, 1)
                    },
                )?,
                DashboardWidget::Grid => {
                    let s = Rc::new(RefCell::new(
                        DataSource::new(CollectionAdapter::new(r.table.clone())).map_err(error)?,
                    ));
                    let id = ui.grid_control(
                        tile,
                        s.clone(),
                        GridControlSettings {
                            is_read_only: true,
                            ..Default::default()
                        },
                        LayoutStyle {
                            width: Length::Percent(1.0),
                            height: Length::Percent(1.0),
                            ..cell(2, 1)
                        },
                    )?;
                    source = Some(s);
                    id
                }
                DashboardWidget::Metric { aggregate } => {
                    let id = ui.label(tile, r.totals[*aggregate].display(), cell(2, 1))?;
                    ui.set_text_style(
                        id,
                        TextStyle {
                            font_size: Some(32.0),
                            ..Default::default()
                        },
                    )?;
                    id
                }
                DashboardWidget::Missing { .. } => {
                    ui.label(tile, r.diagnostic.clone().unwrap_or_default(), cell(2, 1))?
                }
            };
            tiles.push(Tile {
                id: item.id.clone(),
                heading,
                control,
                source,
                selected: None,
            });
        }
        Ok((body, tiles))
    })();
    if result.is_err() {
        let _ = ui.remove(body);
    }
    result
}
