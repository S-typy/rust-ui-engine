use super::*;
use crate::{ControlError, Controls, composition_ui::*};
use rust_desktop_ui_core::*;

pub struct MediaView {
    pub root: WidgetId,
    pub canvas: WidgetId,
    pub status: WidgetId,
    pub seek_editor: WidgetId,
    pub volume_editor: WidgetId,
    session: MediaSession,
    actions: Vec<(WidgetId, &'static str)>,
    capture: Option<Bitmap>,
}
impl MediaView {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let root = frame(ui, parent, style)?;
        let result = (|| {
            let actions = toolbar(
                ui,
                root,
                1,
                &[
                    ("Воспроизвести", "play"),
                    ("Пауза", "pause"),
                    ("Стоп", "stop"),
                    ("Перейти, сек.", "seek"),
                    ("Громкость", "volume"),
                    ("Снимок кадра", "capture"),
                ],
            )?;
            let body = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0), GridTrack::Px(140.0)],
                    grid_rows: vec![GridTrack::Fr(1.0), GridTrack::Px(36.0)],
                    gap: 8.0,
                    ..cell(2, 1)
                },
            )?;
            let canvas = ui.button(
                body,
                "",
                None,
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 2,
                    },
                    padding: Edges::all(0.0),
                    height: Length::Percent(1.0),
                    width: Length::Percent(1.0),
                    ..cell(1, 1)
                },
            )?;
            ui.set_automation_name(canvas, "Media surface; Space plays or pauses, arrows seek")?;
            let seek_editor = ui.text_box(body, "Позиция в секундах", "0", cell(2, 1))?;
            let volume_editor = ui.text_box(body, "Громкость 0..1", "1", cell(2, 2))?;
            let status = ui.label(
                root,
                "Выберите источник",
                LayoutStyle {
                    height: Length::Px(30.0),
                    ..cell(3, 1)
                },
            )?;
            Ok(Self {
                root,
                canvas,
                status,
                seek_editor,
                volume_editor,
                session: MediaSession::default(),
                actions,
                capture: None,
            })
        })();
        if result.is_err() {
            let _ = ui.remove(root);
        }
        result
    }
    pub fn session(&self) -> &MediaSession {
        &self.session
    }
    pub fn session_mut(&mut self) -> &mut MediaSession {
        &mut self.session
    }
    pub fn take_capture(&mut self) -> Option<Bitmap> {
        self.capture.take()
    }
    pub fn activate(&mut self, ui: &Controls, id: WidgetId) -> Result<bool, ControlError> {
        let Some(action) = self
            .actions
            .iter()
            .find_map(|(i, a)| (*i == id).then_some(*a))
        else {
            return Ok(id == self.canvas);
        };
        let now = Instant::now();
        match action {
            "play" => self.session.play(now).map_err(error)?,
            "pause" => self.session.pause(now).map_err(error)?,
            "stop" => self.session.stop(),
            "capture" => self.capture = Some(self.session.capture().map_err(error)?),
            "seek" => {
                let n = ui
                    .document(self.seek_editor)
                    .unwrap()
                    .text()
                    .parse::<f64>()
                    .map_err(error)?;
                if !n.is_finite() || !(0.0..=7.0 * 24.0 * 3600.0).contains(&n) {
                    return Err(error("Invalid position"));
                }
                self.session
                    .seek(Duration::from_secs_f64(n), now)
                    .map_err(error)?;
            }
            "volume" => {
                let n = ui
                    .document(self.volume_editor)
                    .unwrap()
                    .text()
                    .parse::<f32>()
                    .map_err(error)?;
                self.session.set_volume(n).map_err(error)?;
            }
            _ => {}
        }
        Ok(true)
    }
    pub fn input(&mut self, ui: &Controls, event: &InputEvent) -> Result<bool, ControlError> {
        if ui.runtime.focused() != Some(self.canvas) {
            return Ok(false);
        }
        if let InputEvent::KeyDown {
            key, repeat: false, ..
        } = event
        {
            let now = Instant::now();
            match key {
                Key::Space => {
                    if self.session.state() == MediaState::Playing {
                        self.session.pause(now).map_err(error)?;
                    } else {
                        self.session.play(now).map_err(error)?;
                    }
                }
                Key::ArrowLeft => self
                    .session
                    .seek(
                        self.session
                            .position()
                            .saturating_sub(Duration::from_secs(5)),
                        now,
                    )
                    .map_err(error)?,
                Key::ArrowRight => self
                    .session
                    .seek(
                        self.session
                            .position()
                            .saturating_add(Duration::from_secs(5)),
                        now,
                    )
                    .map_err(error)?,
                Key::Escape => self.session.stop(),
                _ => return Ok(false),
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub fn refresh(&mut self, ui: &mut Controls, now: Instant) -> Result<(), ControlError> {
        self.session.poll(now).map_err(error)?;
        if let Some(bounds) = ui.runtime.bounds(self.canvas) {
            let mut scene = Scene::default();
            scene.fill(bounds, Color::rgb(15, 18, 24));
            if let Some(frame) = self.session.frame() {
                let bitmap = &frame.bitmap;
                let scale = (bounds.width / bitmap.width() as f32)
                    .min(bounds.height / bitmap.height() as f32)
                    .max(0.0);
                let width = bitmap.width() as f32 * scale;
                let height = bitmap.height() as f32 * scale;
                if width > 0.0 && height > 0.0 {
                    scene.image(ImagePrimitive::new(
                        bitmap.clone(),
                        Rect::new(
                            bounds.x + (bounds.width - width) / 2.0,
                            bounds.y + (bounds.height - height) / 2.0,
                            width,
                            height,
                        ),
                    ));
                }
            } else {
                text(
                    &mut scene,
                    "Медиа · источник не воспроизводится",
                    Rect::new(
                        bounds.x + 20.0,
                        bounds.y + 20.0,
                        (bounds.width - 40.0).max(0.0),
                        36.0,
                    ),
                    Color::rgb(220, 225, 235),
                    20.0,
                );
            }
            ui.set_custom_scene(self.canvas, scene)?;
        }
        let info = self.session.info();
        let caps = info.map(|i| i.capabilities).unwrap_or_default();
        let ready = matches!(
            self.session.state(),
            MediaState::Playing | MediaState::Paused
        );
        for (id, a) in &self.actions {
            ui.set_enabled(
                *id,
                match *a {
                    "seek" => caps.seek && ready,
                    "volume" => caps.volume && ready,
                    "capture" => caps.capture && self.session.frame().is_some(),
                    "pause" => self.session.state() == MediaState::Playing,
                    "play" => !matches!(
                        self.session.state(),
                        MediaState::Empty | MediaState::Error | MediaState::Playing
                    ),
                    _ => true,
                },
            )?;
        }
        ui.set_enabled(self.seek_editor, caps.seek && ready)?;
        ui.set_enabled(self.volume_editor, caps.volume && ready)?;
        ui.set_label(
            self.status,
            if let Some(e) = self.session.error() {
                format!("Ошибка: {e}")
            } else {
                format!(
                    "{} · {:?} · {:.2} / {} сек.{}",
                    info.map_or("Медиа", |i| i.title.as_str()),
                    self.session.state(),
                    self.session.position().as_secs_f64(),
                    info.and_then(|i| i.duration)
                        .map_or("live".into(), |d| format!("{:.2}", d.as_secs_f64())),
                    if caps.volume {
                        ""
                    } else {
                        " · источник без аудио"
                    }
                )
            },
        )?;
        Ok(())
    }
}
