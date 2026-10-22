//! Application-owned offline fixtures and an accessible list beside each canvas.
use crate::form::Form;
use rust_desktop_ui_controls::spatial::*;
use rust_desktop_ui_controls::{
    ChartModel, ChartPoint, ChartPointKey, ChartSeries, ChartSettings, ListItemTemplate,
    ListItemTextSource, ListTemplateKind, ListTemplateNode, SelectionItem, SelectionSettings,
    SpatialCanvasModel, SpatialChangeKind, TextStyle,
};
use rust_desktop_ui_core::{Bitmap, LayoutStyle, Length, Point, Scene, Size, WidgetId};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, EventResponse, PlatformEvent, accesskit,
};
use rust_desktop_ui_xaml::WindowDefinition;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpatialDemoKind {
    Chart3D,
    Map,
    Diagram,
}
pub(crate) struct SpatialDemo {
    form: Form,
    canvas: WidgetId,
    table: WidgetId,
    kind: SpatialDemoKind,
    actions: Rc<RefCell<VecDeque<usize>>>,
    table_rows: Vec<SelectionItem>,
    table_selection: Vec<String>,
    saved: Option<String>,
    generation: u32,
    missing: bool,
    diagnostics: bool,
    last: String,
    fit_diagram: bool,
    fitted_size: Option<Size>,
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
fn compact_number(value: &str) -> String {
    let Ok(number) = value.parse::<f64>() else {
        return value.to_owned();
    };
    if !number.is_finite() {
        return value.to_owned();
    }
    let rounded = format!("{number:.2}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".into()
    } else {
        trimmed.into()
    }
}
fn display_row(kind: SpatialDemoKind, row: &[String]) -> String {
    let value = |index: usize| row.get(index).map_or("", String::as_str);
    match kind {
        SpatialDemoKind::Chart3D => format!(
            "{} / {} · {}; {}; {}",
            value(0),
            value(1),
            compact_number(value(2)),
            compact_number(value(3)),
            compact_number(value(4))
        ),
        SpatialDemoKind::Map | SpatialDemoKind::Diagram => format!(
            "{} · {}; {}",
            value(0),
            compact_number(value(1)),
            compact_number(value(2))
        ),
    }
}
fn fixture(kind: SpatialDemoKind, generation: u32) -> Result<SpatialCanvasModel, String> {
    match kind {
        SpatialDemoKind::Chart3D => {
            let series: Vec<_> = (1..=3)
                .map(|id| ChartSeries {
                    id,
                    label: format!("Серия {id}"),
                    points: (0..12)
                        .map(|i| ChartPoint {
                            id: i,
                            x: i as f64,
                            y: Some(
                                ((i as f64 * 0.7 + id as f64 + generation as f64 * 0.2).sin()
                                    + 1.0)
                                    * 5.0,
                            ),
                        })
                        .collect(),
                })
                .collect();
            let z = series
                .iter()
                .flat_map(|s| {
                    s.points.iter().map(|p| {
                        (
                            ChartPointKey {
                                series: s.id,
                                point: p.id,
                            },
                            s.id as f64 * 2.0 + (p.id as f64 * 0.4).cos(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            Ok(SpatialCanvasModel::Chart3D(
                Chart3D::new(
                    ChartModel::new(series, ChartSettings::default()).map_err(err)?,
                    z,
                )
                .map_err(err)?,
            ))
        }
        SpatialDemoKind::Map => {
            let mut map = Map::new(
                MapProvider::new(
                    "offline-fixture",
                    "Синтетическая сетка: собственный пример; внешние карты не загружаются",
                )
                .map_err(err)?,
            )
            .map_err(err)?;
            map.set_view(GeoPoint::new(25.0, 45.0).map_err(err)?, 3.0)
                .map_err(err)?;
            let markers = [
                (1, 15.0, 48.0, "Западная точка"),
                (2, 37.0, 55.0, "Северная точка"),
                (3, 35.0, 35.0, "Южная точка"),
                (4, 53.0, 49.0, "Восточная точка"),
            ]
            .into_iter()
            .map(|(id, lon, lat, label)| MapMarker {
                id,
                position: GeoPoint::new(lon, lat).unwrap(),
                label: label.into(),
            })
            .collect::<Vec<_>>();
            let route = MapPolyline {
                id: 1,
                label: "Пример линии".into(),
                vertices: markers.iter().map(|m| m.position).collect(),
            };
            map.replace_layers(markers, vec![route]).map_err(err)?;
            Ok(SpatialCanvasModel::Map(map))
        }
        SpatialDemoKind::Diagram => {
            let nodes = [
                (1, "Источник", DiagramShape::RoundedRectangle, 30.0, 45.0),
                (2, "Преобразование", DiagramShape::Rectangle, 245.0, 45.0),
                (3, "Результат", DiagramShape::Ellipse, 245.0, 205.0),
            ]
            .into_iter()
            .map(|(id, label, shape, x, y)| DiagramNode {
                id,
                label: label.into(),
                shape,
                bounds: DiagramRect::new(x, y, 150.0, 70.0),
                locked: false,
            })
            .collect();
            let connectors = vec![
                DiagramConnector {
                    id: 1,
                    from: DiagramEndpoint {
                        node: 1,
                        port: DiagramPort::East,
                    },
                    to: DiagramEndpoint {
                        node: 2,
                        port: DiagramPort::West,
                    },
                    label: "Данные".into(),
                },
                DiagramConnector {
                    id: 2,
                    from: DiagramEndpoint {
                        node: 2,
                        port: DiagramPort::South,
                    },
                    to: DiagramEndpoint {
                        node: 3,
                        port: DiagramPort::North,
                    },
                    label: "Готово".into(),
                },
            ];
            Ok(SpatialCanvasModel::Diagram(
                Diagram::new(DiagramGraph { nodes, connectors }).map_err(err)?,
            ))
        }
    }
}
impl SpatialDemo {
    /// Deterministic application changes for screenshots without native input.
    /// The index is the corresponding visible action button (zero based).
    pub(crate) fn preview_action(&mut self, index: u32) -> Result<(), String> {
        let count = if self.kind == SpatialDemoKind::Chart3D {
            8
        } else if self.kind == SpatialDemoKind::Diagram {
            12
        } else {
            10
        };
        if index >= count {
            return Err("Unknown spatial preview action".into());
        }
        self.action(index as usize)?;
        self.refresh()?;
        println!(
            "spatial-preview action={index} {}",
            self.preview_description()
        );
        Ok(())
    }
    fn preview_description(&self) -> String {
        match self.form.ui.spatial_model(self.canvas).unwrap() {
            SpatialCanvasModel::Chart3D(c) => format!(
                "camera={:?} selected={:?} data_generation={}",
                c.camera(),
                c.selected(),
                self.generation
            ),
            SpatialCanvasModel::Map(m) => format!(
                "projection={:?} center={:?} zoom={} selected={:?} diagnostics={:?}",
                m.projection(),
                m.center(),
                m.zoom(),
                m.selected(),
                m.diagnostics()
            ),
            SpatialCanvasModel::Diagram(d) => format!(
                "nodes={} edges={} selected={:?} undo={} redo={} readonly={}",
                d.graph().nodes.len(),
                d.graph().connectors.len(),
                d.selected(),
                d.can_undo(),
                d.can_redo(),
                d.is_read_only()
            ),
        }
    }
    pub(crate) fn new(
        definition: WindowDefinition,
        diagnostics: bool,
        kind: SpatialDemoKind,
    ) -> Result<Self, String> {
        let mut form = Form::new(definition, false)?;
        let model = fixture(kind, 0)?;
        let style = LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        };
        let canvas = form
            .ui
            .spatial_canvas(form.find_name("SpatialHost").unwrap(), model, style.clone())
            .map_err(err)?;
        form.view.names.insert("SpatialCanvas".into(), canvas);
        let table = form
            .ui
            .list_box(
                form.find_name("SpatialTable").unwrap(),
                "Табличное описание данных",
                vec![],
                SelectionSettings::default(),
                style,
            )
            .map_err(err)?
            .container;
        form.view.names.insert("SpatialDataList".into(), table);
        // The selectable row keeps its full, lossless accessible name. Only
        // the readonly visual child uses a compact numeric presentation.
        form.ui
            .set_list_item_template(
                table,
                Some(ListItemTemplate {
                    root: ListTemplateNode {
                        layout: LayoutStyle {
                            width: Length::Percent(1.0),
                            height: Length::Percent(1.0),
                            ..Default::default()
                        },
                        kind: ListTemplateKind::Text {
                            source: ListItemTextSource::Column(0),
                            style: TextStyle {
                                wrap: false,
                                ..Default::default()
                            },
                        },
                    },
                }),
            )
            .map_err(err)?;
        let (title, help, buttons) = match kind {
            SpatialDemoKind::Chart3D => (
                "Трёхмерные данные",
                "Левая кнопка выбирает точку; правая вращает камеру. Ctrl + стрелки вращают, Page Up / Down меняют расстояние.",
                vec![
                    "Камера влево",
                    "Камера вправо",
                    "Камера вверх",
                    "Камера вниз",
                    "Перспектива",
                    "Ортография",
                    "Обновить данные",
                    "Сброс",
                ],
            ),
            SpatialDemoKind::Map => (
                "Географические слои",
                "Перетаскивание перемещает карту; колесо масштабирует вокруг указателя. Ctrl + стрелки перемещают, обычные стрелки выбирают маркер.",
                vec![
                    "Север",
                    "Юг",
                    "Приблизить",
                    "Отдалить",
                    "Проекция",
                    "Тайлы доступны",
                    "Тайлы отсутствуют",
                    "Поиск точки",
                    "Маршрут",
                    "Сброс",
                ],
            ),
            SpatialDemoKind::Diagram => (
                "Редактор связанной схемы",
                "Перетаскивайте фигуры и нижний правый угол. Shift + стрелки меняют размер; Ctrl + Z / Y отменяют и повторяют.",
                vec![
                    "Прямоугольник",
                    "Эллипс",
                    "Соединить",
                    "Компоновка",
                    "Отменить",
                    "Повторить",
                    "Только чтение",
                    "Сохранить",
                    "Восстановить",
                    "Сброс",
                    "Вписать схему",
                    "Большая схема",
                ],
            ),
        };
        form.ui
            .set_label(form.find_name("SpatialTitle").unwrap(), title)
            .map_err(err)?;
        form.ui
            .set_label(form.find_name("SpatialHelp").unwrap(), help)
            .map_err(err)?;
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let host = form.find_name("SpatialActions").unwrap();
        for (index, label) in buttons.into_iter().enumerate() {
            let name = format!("SpatialAction{index}");
            form.ui.commands_mut().register(name.as_str(), label);
            let queue = actions.clone();
            form.ui
                .commands_mut()
                .on_execute(&name.clone().into(), move || {
                    let mut q = queue.borrow_mut();
                    if q.len() < 32 {
                        q.push_back(index);
                    }
                });
            let id = form
                .ui
                .button(
                    host,
                    label,
                    Some(name.clone().into()),
                    LayoutStyle {
                        width: Length::Px(160.0),
                        height: Length::Px(36.0),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
            form.view.names.insert(name, id);
        }
        let mut app = Self {
            form,
            canvas,
            table,
            kind,
            actions,
            table_rows: vec![],
            table_selection: vec![],
            saved: None,
            generation: 0,
            missing: false,
            diagnostics,
            last: String::new(),
            fit_diagram: true,
            fitted_size: None,
        };
        app.refresh()?;
        Ok(app)
    }
    fn action(&mut self, index: usize) -> Result<(), String> {
        if (self.kind == SpatialDemoKind::Chart3D && index == 7)
            || (self.kind != SpatialDemoKind::Chart3D && index == 9)
        {
            let next = fixture(self.kind, 0)?;
            self.generation = 0;
            self.missing = false;
            self.fit_diagram = true;
            self.fitted_size = None;
            self.form
                .ui
                .edit_spatial(self.canvas, |m| {
                    *m = next;
                    Ok(())
                })
                .map_err(err)?;
            return Ok(());
        }
        if self.kind == SpatialDemoKind::Chart3D && index == 6 {
            self.generation += 1;
            let SpatialCanvasModel::Chart3D(next) = fixture(self.kind, self.generation)? else {
                unreachable!()
            };
            self.form
                .ui
                .edit_spatial(self.canvas, |m| {
                    if let SpatialCanvasModel::Chart3D(current) = m {
                        current.replace_data(next.data().clone(), next.z_values())?;
                    }
                    Ok(())
                })
                .map_err(err)?;
            return Ok(());
        }
        if self.kind == SpatialDemoKind::Diagram && index == 7 {
            if let SpatialCanvasModel::Diagram(d) = self.form.ui.spatial_model(self.canvas).unwrap()
            {
                self.saved = Some(d.to_json().map_err(err)?);
            }
            return Ok(());
        }
        if self.kind == SpatialDemoKind::Diagram && [3, 10, 11].contains(&index) {
            self.fit_diagram = true;
            self.fitted_size = None;
        }
        if self.kind == SpatialDemoKind::Diagram && index == 11 {
            let nodes = (0..500)
                .map(|i| DiagramNode {
                    id: i + 1,
                    label: format!("Узел {}", i + 1),
                    shape: if i % 3 == 0 {
                        DiagramShape::Ellipse
                    } else {
                        DiagramShape::RoundedRectangle
                    },
                    bounds: DiagramRect::new(
                        (i % 25) as f32 * 180.0,
                        (i / 25) as f32 * 110.0,
                        140.0,
                        64.0,
                    ),
                    locked: false,
                })
                .collect();
            let connectors = (1..500)
                .filter(|i| i % 25 != 0)
                .map(|i| DiagramConnector {
                    id: i,
                    from: DiagramEndpoint {
                        node: i,
                        port: DiagramPort::East,
                    },
                    to: DiagramEndpoint {
                        node: i + 1,
                        port: DiagramPort::West,
                    },
                    label: String::new(),
                })
                .collect();
            let next = Diagram::new(DiagramGraph { nodes, connectors }).map_err(err)?;
            self.form
                .ui
                .edit_spatial(self.canvas, |m| {
                    *m = SpatialCanvasModel::Diagram(next);
                    Ok(())
                })
                .map_err(err)?;
            return Ok(());
        }
        let saved = self.saved.clone();
        let b = self
            .form
            .ui
            .runtime
            .bounds(self.canvas)
            .unwrap_or(rust_desktop_ui_core::Rect::new(0.0, 0.0, 600.0, 400.0));
        let center = Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5);
        if self.kind == SpatialDemoKind::Map {
            if index == 5 {
                self.missing = false;
            }
            if index == 6 {
                self.missing = true;
            }
        }
        self.form
            .ui
            .edit_spatial(self.canvas, |model| {
                match model {
                    SpatialCanvasModel::Chart3D(c) => match index {
                        0 => c.orbit(-0.2, 0.0)?,
                        1 => c.orbit(0.2, 0.0)?,
                        2 => c.orbit(0.0, -0.15)?,
                        3 => c.orbit(0.0, 0.15)?,
                        4 | 5 => {
                            let mut camera = c.camera();
                            camera.projection = if index == 4 {
                                Projection3D::Perspective
                            } else {
                                Projection3D::Orthographic
                            };
                            c.set_camera(camera)?;
                        }
                        _ => {}
                    },
                    SpatialCanvasModel::Map(map) => match index {
                        0 => map.pan(Point::new(0.0, 80.0))?,
                        1 => map.pan(Point::new(0.0, -80.0))?,
                        2 => map.zoom_at(1.0, center, b)?,
                        3 => map.zoom_at(-1.0, center, b)?,
                        4 => {
                            map.set_projection(if map.projection() == MapProjection::WebMercator {
                                MapProjection::Equirectangular
                            } else {
                                MapProjection::WebMercator
                            })?
                        }
                        5 | 6 => {
                            let provider = map.provider().clone();
                            map.set_provider(provider)?;
                        }
                        7 => {
                            let request = map.request_service(MapServiceKind::Search(
                                "Локальный пример".into(),
                            ))?;
                            map.complete_service(
                                &request,
                                MapServiceResult::Search(vec![MapMarker {
                                    id: 10,
                                    label: "Найденная точка".into(),
                                    position: map.center(),
                                }]),
                            )?;
                        }
                        8 => {
                            let from = GeoPoint::new(15.0, 48.0)?;
                            let to = GeoPoint::new(37.0, 55.0)?;
                            let request =
                                map.request_service(MapServiceKind::Route { from, to })?;
                            map.complete_service(
                                &request,
                                MapServiceResult::Route(MapPolyline {
                                    id: 9,
                                    label: "Маршрут провайдера".into(),
                                    vertices: vec![from, GeoPoint::new(20.0, 52.0)?, to],
                                }),
                            )?;
                        }
                        _ => {}
                    },
                    SpatialCanvasModel::Diagram(d) => {
                        if d.is_read_only() && index != 6 {
                            return Ok(());
                        }
                        match index {
                            0 | 1 => {
                                let id =
                                    d.graph().nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
                                let world = d.world_point(center, b);
                                d.edit(DiagramEdit::Insert(DiagramNode {
                                    id,
                                    label: format!("Фигура {id}"),
                                    shape: if index == 0 {
                                        DiagramShape::RoundedRectangle
                                    } else {
                                        DiagramShape::Ellipse
                                    },
                                    bounds: DiagramRect::new(world.x, world.y, 140.0, 64.0),
                                    locked: false,
                                }))?;
                                d.select([id])?;
                            }
                            2 => {
                                let nodes = d.selected().iter().copied().collect::<Vec<_>>();
                                if nodes.len() == 2 {
                                    let id = d
                                        .graph()
                                        .connectors
                                        .iter()
                                        .map(|e| e.id)
                                        .max()
                                        .unwrap_or(0)
                                        + 1;
                                    d.edit(DiagramEdit::Connect(DiagramConnector {
                                        id,
                                        from: DiagramEndpoint {
                                            node: nodes[0],
                                            port: DiagramPort::East,
                                        },
                                        to: DiagramEndpoint {
                                            node: nodes[1],
                                            port: DiagramPort::West,
                                        },
                                        label: "Связь".into(),
                                    }))?;
                                }
                            }
                            3 => d.edit(DiagramEdit::Layout)?,
                            4 => {
                                d.undo()?;
                            }
                            5 => {
                                d.redo()?;
                            }
                            6 => d.set_read_only(!d.is_read_only()),
                            8 => {
                                if let Some(saved) = saved {
                                    d.restore_json(&saved)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(())
            })
            .map_err(err)
    }
    fn table_items(&self) -> Vec<SelectionItem> {
        let model = self.form.ui.spatial_model(self.canvas).unwrap();
        let table = model.table();
        let keys: Vec<String> = match model {
            SpatialCanvasModel::Chart3D(c) => c
                .data()
                .series()
                .iter()
                .flat_map(|s| {
                    s.points
                        .iter()
                        .filter(|p| p.y.is_some())
                        .map(|p| format!("{}:{}", s.id, p.id))
                })
                .collect(),
            SpatialCanvasModel::Map(m) => m.markers().iter().map(|m| m.id.to_string()).collect(),
            SpatialCanvasModel::Diagram(d) => {
                d.graph().nodes.iter().map(|n| n.id.to_string()).collect()
            }
        };
        keys.into_iter()
            .zip(table.rows)
            .map(|(key, row)| {
                let display = display_row(self.kind, &row);
                let mut item = SelectionItem::new(
                    key,
                    table
                        .columns
                        .iter()
                        .zip(row)
                        .map(|(name, value)| format!("{name}: {value}"))
                        .collect::<Vec<_>>()
                        .join("; "),
                );
                item.columns = vec![display];
                item
            })
            .collect()
    }
    fn refresh(&mut self) -> Result<(), String> {
        let actions = self.actions.borrow_mut().drain(..).collect::<Vec<_>>();
        for index in actions {
            self.action(index)?;
        }
        let selected = self
            .form
            .ui
            .selected_keys(self.table)
            .unwrap_or_default()
            .to_vec();
        if selected != self.table_selection {
            let key = selected.first().cloned();
            self.form
                .ui
                .edit_spatial(self.canvas, |m| {
                    match m {
                        SpatialCanvasModel::Chart3D(c) => {
                            let key = key.as_deref().and_then(|s| s.split_once(':')).and_then(
                                |(s, p)| {
                                    Some(ChartPointKey {
                                        series: s.parse().ok()?,
                                        point: p.parse().ok()?,
                                    })
                                },
                            );
                            c.select(key)?;
                        }
                        SpatialCanvasModel::Map(m) => m.select(key.and_then(|s| s.parse().ok()))?,
                        SpatialCanvasModel::Diagram(d) => {
                            d.select(key.and_then(|s| s.parse().ok()))?
                        }
                    }
                    Ok(())
                })
                .map_err(err)?;
        }
        let items = self.table_items();
        if items != self.table_rows {
            self.form
                .ui
                .set_selection_items(self.table, items.clone())
                .map_err(err)?;
            self.table_rows = items;
        }
        let model = self.form.ui.spatial_model(self.canvas).unwrap();
        let selection: Vec<_> = match model {
            SpatialCanvasModel::Chart3D(c) => c
                .selected()
                .map(|k| format!("{}:{}", k.series, k.point))
                .into_iter()
                .collect(),
            SpatialCanvasModel::Map(m) => {
                m.selected().map(|id| id.to_string()).into_iter().collect()
            }
            SpatialCanvasModel::Diagram(d) => d
                .selected()
                .first()
                .map(|id| id.to_string())
                .into_iter()
                .collect(),
        };
        let summary = model.summary();
        let columns = match self.kind {
            SpatialDemoKind::Chart3D => "Ряд / точка · X; Y; Z",
            SpatialDemoKind::Map => "Метка · долгота; широта",
            SpatialDemoKind::Diagram => "Фигура · X; Y",
        };
        self.form
            .ui
            .set_selected_keys(self.table, &selection)
            .map_err(err)?;
        self.table_selection = selection;
        self.form
            .ui
            .set_label(self.form.find_name("SpatialColumns").unwrap(), columns)
            .map_err(err)?;
        self.form
            .ui
            .set_label(self.form.find_name("SpatialStatus").unwrap(), &summary)
            .map_err(err)?;
        if self
            .form
            .ui
            .take_spatial_changes()
            .iter()
            .any(|change| change.id == self.canvas && change.kind == SpatialChangeKind::View)
        {
            self.fit_diagram = false;
        }
        Ok(())
    }
    fn offline_tiles(&mut self) -> Result<(), String> {
        if self.kind != SpatialDemoKind::Map {
            return Ok(());
        }
        let missing = self.missing;
        let requests = self.form.ui.map_tile_requests(self.canvas).map_err(err)?;
        for request in requests {
            let result = if missing {
                MapTileResult::Missing("Нет тайла в локальном наборе".into())
            } else {
                let mut pixels = Vec::with_capacity(128 * 128 * 4);
                for y in 0..128 {
                    for x in 0..128 {
                        let line = x % 32 == 0 || y % 32 == 0;
                        let checker = (request.key.x + request.key.y) % 2 == 0;
                        pixels.extend_from_slice(if line {
                            &[177, 200, 216, 255]
                        } else if checker {
                            &[226, 238, 247, 255]
                        } else {
                            &[213, 232, 237, 255]
                        });
                    }
                }
                MapTileResult::Ready(Bitmap::new_rgba8(128, 128, pixels).map_err(err)?)
            };
            self.form
                .ui
                .complete_map_tile(self.canvas, &request, result)
                .map_err(err)?;
        }
        Ok(())
    }
}
impl DesktopApp for SpatialDemo {
    fn title(&self) -> String {
        self.form.title()
    }
    fn scene(&self) -> &Scene {
        self.form.scene()
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.refresh()?;
        self.form.update(size, scale)?;
        if self.kind == SpatialDemoKind::Diagram
            && self.fit_diagram
            && let Some(bounds) = self.form.ui.runtime.bounds(self.canvas)
        {
            let next_size = Size::new(bounds.width, bounds.height);
            if self.fitted_size != Some(next_size) && bounds.width > 48.0 && bounds.height > 48.0 {
                self.form
                    .ui
                    .edit_spatial(self.canvas, |m| {
                        if let SpatialCanvasModel::Diagram(d) = m {
                            d.fit_to_view(bounds, 20.0)?;
                        }
                        Ok(())
                    })
                    .map_err(err)?;
                self.fitted_size = Some(next_size);
            }
        }
        self.offline_tiles()?;
        self.refresh()?;
        self.form.update(size, scale)?;
        if self.diagnostics {
            let text = format!(
                "spatial-state kind={:?} {} {}",
                self.kind,
                self.form.ui.spatial_model(self.canvas).unwrap().summary(),
                self.preview_description()
            );
            let text = format!(
                "{text} bounds={:?} focus={:?}",
                self.form.ui.runtime.bounds(self.canvas),
                self.form.ui.runtime.focused()
            );
            let b = self.form.ui.runtime.bounds(self.canvas).unwrap();
            let details = match self.form.ui.spatial_model(self.canvas).unwrap() {
                SpatialCanvasModel::Chart3D(c) => {
                    serde_json::json!({"kind":"chart3d", "camera":format!("{:?}", c.camera()), "selected":c.selected().map(|k|format!("{}:{}",k.series,k.point)),
                        "points": c.geometry(b).map_err(err)?.points.iter().map(|p|serde_json::json!({"key":format!("{}:{}",p.key.series,p.key.point),"center":[p.center.x,p.center.y],"radius":p.radius})).collect::<Vec<_>>() })
                }
                SpatialCanvasModel::Map(m) => {
                    serde_json::json!({"kind":"map","center":[m.center().longitude,m.center().latitude],"zoom":m.zoom(),"selected":m.selected(),"diagnostics":format!("{:?}",m.diagnostics()),"markers":m.markers().iter().map(|marker|{let p=m.screen_point(marker.position,b).unwrap();serde_json::json!({"id":marker.id,"center":[p.x,p.y]})}).collect::<Vec<_>>()})
                }
                SpatialCanvasModel::Diagram(d) => {
                    serde_json::json!({"kind":"diagram","count":d.graph().nodes.len(),"edges":d.graph().connectors.len(),"zoom":d.zoom(),"pan":[d.pan_offset().x,d.pan_offset().y],"selected":d.selected(),"undo":d.can_undo(),"redo":d.can_redo(),"readonly":d.is_read_only(),"nodes":d.graph().nodes.iter().take(12).map(|n|{let p=d.screen_point(Point::new(n.bounds.x,n.bounds.y),b);serde_json::json!({"id":n.id,"world":[n.bounds.x,n.bounds.y,n.bounds.width,n.bounds.height],"bounds":[p.x,p.y,n.bounds.width*d.zoom(),n.bounds.height*d.zoom()]})}).collect::<Vec<_>>() })
                }
            };
            let frame_key = format!("{text} {details}");
            if self.last != frame_key {
                println!("{text}");
                println!("spatial-details {details}");
                println!(
                    "batch-state {}",
                    crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view)
                );
                self.last = frame_key;
            }
        }
        Ok(())
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if self.diagnostics {
            println!("spatial-input {event:?}");
        }
        let mut result = self.form.event(event)?;
        self.refresh()?;
        result.redraw = true;
        Ok(result)
    }
    fn set_waker(&mut self, wake: Option<platform::AppWaker>) {
        self.form.set_waker(wake)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
    fn wake_after(&self) -> Option<std::time::Duration> {
        self.form.wake_after()
    }
    fn ime_cursor(&self) -> Option<rust_desktop_ui_core::Rect> {
        self.form.ime_cursor()
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.form.ime_target()
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.form.ime_composition_active()
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_theme_and_dpi_preserve_spatial_identity_and_accepted_state() {
        for kind in [
            SpatialDemoKind::Chart3D,
            SpatialDemoKind::Map,
            SpatialDemoKind::Diagram,
        ] {
            let mut app =
                SpatialDemo::new(crate::generated_spatial::window_definition(), false, kind)
                    .unwrap();
            app.update(Size::new(1200.0, 820.0), 1.0).unwrap();
            let canvas = app.canvas;
            app.form
                .ui
                .edit_spatial(canvas, |model| {
                    match model {
                        SpatialCanvasModel::Chart3D(c) => {
                            c.select(Some(ChartPointKey {
                                series: 1,
                                point: 2,
                            }))?;
                            c.orbit(0.7, 0.2)?;
                        }
                        SpatialCanvasModel::Map(m) => {
                            m.select(Some(2))?;
                            m.pan(Point::new(20.0, 10.0))?;
                        }
                        SpatialCanvasModel::Diagram(d) => {
                            d.select([2])?;
                            d.edit(DiagramEdit::Move {
                                nodes: vec![2],
                                delta: Point::new(12.0, 8.0),
                            })?;
                        }
                    }
                    Ok(())
                })
                .unwrap();
            let selected = app.form.ui.spatial_model(canvas).unwrap().summary();
            for (theme, scale) in [
                (rust_desktop_ui_controls::Theme::FluentDark, 1.5),
                (rust_desktop_ui_controls::Theme::FluentLight, 2.0),
            ] {
                app.form.ui.set_theme(theme).unwrap();
                app.update(Size::new(950.0, 720.0), scale).unwrap();
                assert_eq!(app.canvas, canvas);
                assert_eq!(
                    app.form.ui.spatial_model(canvas).unwrap().summary(),
                    selected
                );
            }
            if let SpatialCanvasModel::Chart3D(c) = app.form.ui.spatial_model(canvas).unwrap() {
                let camera = c.camera();
                let selected = c.selected();
                app.action(6).unwrap();
                let SpatialCanvasModel::Chart3D(c) = app.form.ui.spatial_model(canvas).unwrap()
                else {
                    panic!()
                };
                assert_eq!(c.camera(), camera);
                assert_eq!(c.selected(), selected);
            }
        }
    }
    #[test]
    fn fixtures_share_complete_table_identity_and_serialization() {
        for kind in [
            SpatialDemoKind::Chart3D,
            SpatialDemoKind::Map,
            SpatialDemoKind::Diagram,
        ] {
            let m = fixture(kind, 0).unwrap();
            let table = m.table();
            assert!(!table.rows.is_empty());
            assert!(table.rows.iter().all(|r| r.len() == table.columns.len()));
            if let SpatialCanvasModel::Diagram(d) = m {
                assert_eq!(
                    Diagram::from_json(&d.to_json().unwrap()).unwrap().graph(),
                    d.graph()
                );
            }
        }
    }
    #[test]
    fn compiled_spatial_forms_keep_canvas_tables_and_actions_after_resize() {
        for kind in [
            SpatialDemoKind::Chart3D,
            SpatialDemoKind::Map,
            SpatialDemoKind::Diagram,
        ] {
            let mut app =
                SpatialDemo::new(crate::generated_spatial::window_definition(), false, kind)
                    .unwrap();
            app.update(Size::new(1200.0, 820.0), 1.0).unwrap();
            assert!(!app.form.ui.scene().rounded_rectangles.is_empty());
            assert!(!app.table_rows.is_empty());
            let item = &app.table_rows[0];
            assert!(item.columns[0].len() < item.text.len());
            let row = app.form.ui.list_row_widget(app.table, &item.key).unwrap();
            let raw = app.form.ui.spatial_model(app.canvas).unwrap().table();
            let full_description = raw
                .columns
                .iter()
                .zip(&raw.rows[0])
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join("; ");
            assert_eq!(item.text, full_description);
            let semantics = app.form.ui.semantics();
            let accessible_name = &semantics.iter().find(|node| node.id == row).unwrap().label;
            // List rows intentionally aggregate their full text and projected
            // columns. The original fields must remain an exact prefix; the
            // compact visual column may add information, never replace it.
            assert_eq!(
                accessible_name.strip_prefix(&full_description),
                Some(format!(" {}", item.columns.join(" ")).as_str())
            );
            assert!(
                app.form
                    .ui
                    .scene()
                    .texts
                    .iter()
                    .any(|run| run.text == item.columns[0])
            );
            assert!(
                !app.form
                    .ui
                    .scene()
                    .texts
                    .iter()
                    .any(|run| run.text == item.text)
            );
            let first = app.table_rows[0].key.clone();
            app.form
                .ui
                .set_selected_keys(app.table, std::slice::from_ref(&first))
                .unwrap();
            app.refresh().unwrap();
            match app.form.ui.spatial_model(app.canvas).unwrap() {
                SpatialCanvasModel::Chart3D(c) => assert!(c.selected().is_some()),
                SpatialCanvasModel::Map(m) => assert!(m.selected().is_some()),
                SpatialCanvasModel::Diagram(d) => assert!(!d.selected().is_empty()),
            }
            app.preview_action(match kind {
                SpatialDemoKind::Chart3D => 5,
                SpatialDemoKind::Map => 6,
                SpatialDemoKind::Diagram => 3,
            })
            .unwrap();
            app.update(Size::new(760.0, 620.0), 1.0).unwrap();
            let bounds = app.form.ui.runtime.bounds(app.canvas).unwrap();
            assert!(bounds.width > 100.0 && bounds.height > 100.0);
            for index in 0..if kind == SpatialDemoKind::Chart3D {
                8
            } else {
                10
            } {
                let id = app
                    .form
                    .find_name(&format!("SpatialAction{index}"))
                    .unwrap();
                let bounds = app.form.ui.runtime.bounds(id).unwrap();
                let clip = app.form.ui.runtime.clip(id).unwrap();
                assert!(
                    bounds.intersection(clip).is_some_and(
                        |r| r.width >= bounds.width - 0.1 && r.height >= bounds.height - 0.1
                    ),
                    "{kind:?} action {index} clipped"
                );
            }
            assert!(
                app.form
                    .ui
                    .semantics()
                    .iter()
                    .any(|node| node.id == app.canvas && node.value.is_some())
            );
            if let SpatialCanvasModel::Map(map) = app.form.ui.spatial_model(app.canvas).unwrap() {
                assert!(map.diagnostics().missing_tiles > 0);
            }
        }
    }
    #[test]
    fn compact_display_rounds_only_visual_values() {
        assert_eq!(compact_number("9.207354924039483"), "9.21");
        assert_eq!(compact_number("1200"), "1200");
        assert_eq!(compact_number("-0.001"), "0");
        let row = vec![
            "Серия 1".into(),
            "7".into(),
            "1200".into(),
            "9.207354924039483".into(),
            "-0.001".into(),
        ];
        assert_eq!(
            display_row(SpatialDemoKind::Chart3D, &row),
            "Серия 1 / 7 · 1200; 9.21; 0"
        );
        assert_eq!(row[3], "9.207354924039483");
    }
}
