//! Native dialogs and one bounded application worker for image I/O.
use crate::form::Form;
use rust_desktop_ui_controls::{BindingContext, DataValue, ImageEditorEvent, ImageLoadToken};
use rust_desktop_ui_core::{Bitmap, Rect, Scene, Size, WidgetId};
use rust_desktop_ui_images::{DecodeOptions, ImageSource, encode_bitmap_png};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, DialogCompletion, DialogOptions, DialogOutcome, DialogRequest,
    EventResponse, FileDialog, FileDialogFilter, FileDialogKind, PlatformEvent, accesskit,
};
use std::{
    cell::RefCell,
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};
enum Pending {
    Load(ImageLoadToken),
    Save(Bitmap),
}
enum Completion {
    Load(ImageLoadToken, Result<Bitmap, String>),
    Save(Result<(), String>),
}
pub(crate) struct ImageDemo {
    pub(crate) form: Form,
    editor: WidgetId,
    viewer: WidgetId,
    request: Option<DialogRequest>,
    pending: Option<(u64, Pending)>,
    next: u64,
    tx: SyncSender<Completion>,
    rx: Receiver<Completion>,
    busy: bool,
    alive: Arc<AtomicBool>,
    waker: Option<platform::AppWaker>,
    diagnostics: bool,
    last: String,
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
impl ImageDemo {
    pub(crate) fn new(
        definition: rust_desktop_ui_xaml::WindowDefinition,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Locked", false.into())])).map_err(err)?,
        ));
        let mut form = Form::new_with_model(definition, diagnostics, Some(context))?;
        let editor = form.find_name("ImageEditor").ok_or("Editor missing")?;
        let viewer = form.find_name("AcceptedImage").ok_or("Viewer missing")?;
        let image = sample()?;
        form.ui
            .set_image_editor_value(editor, Some(image.clone()), true)
            .map_err(err)?;
        form.ui
            .set_image_editor_value(viewer, Some(image), true)
            .map_err(err)?;
        for (name, id) in [("ImageEditor", editor), ("AcceptedImage", viewer)] {
            let p = form.ui.image_editor_parts(id).unwrap().clone();
            let toolbar = form.ui.runtime.tree().parent(p.load).unwrap();
            let buttons = form.ui.runtime.tree().children(toolbar).unwrap().to_vec();
            for (suffix, button) in [
                "Open", "Save", "Fit", "Actual", "ZoomOut", "ZoomIn", "Rotate", "Crop", "Undo",
                "Redo", "Apply", "Cancel",
            ]
            .into_iter()
            .zip(buttons)
            {
                form.view.names.insert(format!("{name}{suffix}"), button);
            }
            for (suffix, part) in [
                ("Viewport", p.viewport),
                ("Status", p.status),
                ("Load", p.load),
                ("Save", p.save),
                ("Apply", p.apply),
                ("Cancel", p.cancel),
            ] {
                form.view.names.insert(format!("{name}{suffix}"), part);
            }
        }
        let (tx, rx) = mpsc::sync_channel(1);
        Ok(Self {
            form,
            editor,
            viewer,
            request: None,
            pending: None,
            next: 0,
            tx,
            rx,
            busy: false,
            alive: Arc::new(AtomicBool::new(true)),
            waker: None,
            diagnostics,
            last: String::new(),
        })
    }
    fn result(&mut self, text: &str) -> Result<(), String> {
        let text: String = text.chars().take(512).collect();
        let text = if self.form.ui.measure_text(&text, 1024.0, true).is_ok() {
            text
        } else {
            "Операция завершилась с ошибкой".into()
        };
        if let Some(id) = self.form.find_name("ImageResult") {
            self.form.ui.set_label(id, text).map_err(err)?;
        }
        Ok(())
    }
    fn refresh(&mut self) -> Result<(), String> {
        while let Ok(completion) = self.rx.try_recv() {
            self.busy = false;
            match completion {
                Completion::Load(token, value) => {
                    self.form
                        .ui
                        .complete_image_editor_load(&token, value)
                        .map_err(err)?;
                }
                Completion::Save(result) => self.result(&match result {
                    Ok(()) => "Принятая версия сохранена в PNG".into(),
                    Err(e) => format!("Сохранение не выполнено: {e}"),
                })?,
            }
        }
        for event in self.form.ui.take_image_editor_events() {
            match event {
                ImageEditorEvent::Applied { owner, value } => {
                    if owner == self.editor {
                        self.form
                            .ui
                            .set_image_editor_value(self.viewer, value, true)
                            .map_err(err)?;
                        self.result("Рабочая версия принята; просмотр справа обновлён")?;
                    }
                }
                ImageEditorEvent::LoadRequested { owner, token } => {
                    if self.busy || self.pending.is_some() {
                        self.form
                            .ui
                            .complete_image_editor_load(
                                &token,
                                Err("Дождитесь завершения текущей операции".into()),
                            )
                            .map_err(err)?;
                    } else {
                        self.queue_dialog(Pending::Load(token))?;
                    }
                    let _ = owner;
                }
                ImageEditorEvent::SaveRequested { owner: _, value } => {
                    if self.busy || self.pending.is_some() {
                        self.result("Дождитесь завершения текущей операции")?;
                    } else {
                        self.queue_dialog(Pending::Save(value))?;
                    }
                }
            }
        }
        Ok(())
    }
    fn queue_dialog(&mut self, pending: Pending) -> Result<(), String> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Dialog identity exhausted")?;
        let save = matches!(pending, Pending::Save(_));
        let mut options = FileDialog::new(if save {
            FileDialogKind::Save
        } else {
            FileDialogKind::Open
        });
        options.title = if save {
            "Сохранить принятую версию PNG"
        } else {
            "Открыть изображение в рабочую версию"
        }
        .into();
        options.filters = vec![FileDialogFilter {
            label: if save {
                "PNG"
            } else {
                "Изображения"
            }
            .into(),
            extensions: if save {
                vec!["png".into()]
            } else {
                ["png", "jpg", "jpeg", "bmp", "ico", "svg"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            },
        }];
        if save {
            options.default_extension = Some("png".into());
            options.file_name = Some("image.png".into());
        }
        self.request = Some(DialogRequest {
            id: self.next,
            options: DialogOptions::File(options),
        });
        self.pending = Some((self.next, pending));
        Ok(())
    }
    fn start(&mut self, pending: Pending, path: PathBuf) {
        let tx = self.tx.clone();
        let wake = self.waker.clone();
        let alive = self.alive.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = match pending {
                Pending::Load(token) => {
                    if token.is_cancelled() || !alive.load(Ordering::Relaxed) {
                        Completion::Load(token, Err("Загрузка отменена".into()))
                    } else {
                        let result = ImageSource::from_file(&path)
                            .and_then(|s| s.decode(DecodeOptions::default()))
                            .map(|a| a.bitmap().clone())
                            .map_err(err);
                        Completion::Load(token, result)
                    }
                }
                Pending::Save(value) => {
                    let result = encode_bitmap_png(&value)
                        .map_err(err)
                        .and_then(|bytes| atomic_save(&path, &bytes, &alive));
                    Completion::Save(result)
                }
            };
            if alive.load(Ordering::Relaxed) {
                let _ = tx.send(result);
                if let Some(wake) = wake {
                    wake.wake();
                }
            }
        });
    }
}
impl Drop for ImageDemo {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}
impl DesktopApp for ImageDemo {
    fn set_waker(&mut self, w: Option<platform::AppWaker>) {
        self.waker = w.clone();
        self.form.set_waker(w);
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.refresh()?;
        self.form.update(size, scale)?;
        if self.diagnostics {
            let accepted = self
                .form
                .ui
                .image_editor_value(self.editor)
                .flatten()
                .map(|b| (b.id(), b.width(), b.height()));
            let draft = self
                .form
                .ui
                .image_editor_draft(self.editor)
                .flatten()
                .map(|b| (b.id(), b.width(), b.height()));
            let s = format!(
                "accepted={accepted:?} draft={draft:?} dirty={:?} view={:?} busy={} error={:?}",
                self.form.ui.image_editor_dirty(self.editor),
                self.form.ui.image_editor_viewport(self.editor),
                self.busy,
                self.form.ui.image_editor_error(self.editor)
            );
            let snapshot = crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view);
            let signature = format!("{s}|{snapshot}");
            if signature != self.last {
                println!("image-state {s}");
                println!("batch-state {snapshot}");
                self.last = signature;
            }
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.form.scene()
    }
    fn title(&self) -> String {
        self.form.title()
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        let mut r = self.form.event(event)?;
        self.refresh()?;
        r.redraw = true;
        Ok(r)
    }
    fn take_dialog_request(&mut self) -> Option<DialogRequest> {
        self.request.take()
    }
    fn dialog_completed(&mut self, completion: DialogCompletion) -> Result<EventResponse, String> {
        if self
            .pending
            .as_ref()
            .is_none_or(|(id, _)| *id != completion.id)
        {
            return Ok(EventResponse::default());
        }
        let (_, pending) = self.pending.take().unwrap();
        match completion.result {
            Ok(DialogOutcome::Paths(paths)) if paths.len() == 1 => {
                self.start(pending, paths[0].clone())
            }
            Ok(_) => {
                if let Pending::Load(token) = pending {
                    self.form
                        .ui
                        .complete_image_editor_load(
                            &token,
                            Err("Загрузка отменена; изображение сохранено".into()),
                        )
                        .map_err(err)?;
                }
            }
            Err(error) => {
                if let Pending::Load(token) = pending {
                    self.form
                        .ui
                        .complete_image_editor_load(&token, Err(error.to_string()))
                        .map_err(err)?;
                } else {
                    self.result(&error.to_string())?;
                }
            }
        }
        Ok(EventResponse::redraw())
    }
    fn wake_after(&self) -> Option<Duration> {
        if self.busy {
            Some(Duration::from_millis(40))
        } else {
            self.form.wake_after()
        }
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
    fn ime_cursor(&self) -> Option<Rect> {
        self.form.ime_cursor()
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.form.ime_target()
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.form.ime_composition_active()
    }
}
fn sample() -> Result<Bitmap, String> {
    let mut p = Vec::with_capacity(320 * 180 * 4);
    for y in 0..180 {
        for x in 0..320 {
            p.extend_from_slice(&[
                (x * 255 / 319) as u8,
                (y * 255 / 179) as u8,
                170,
                if x < 80 {
                    0
                } else if x < 160 {
                    128
                } else {
                    255
                },
            ]);
        }
    }
    Bitmap::new_rgba8(320, 180, p).map_err(err)
}
fn atomic_save(path: &Path, bytes: &[u8], alive: &AtomicBool) -> Result<(), String> {
    if bytes.len() > rust_desktop_ui_images::MAX_IMAGE_SOURCE_BYTES {
        return Err("PNG byte budget exceeded".into());
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let token = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "Temporary identity exhausted")?;
    let parent = path
        .parent()
        .ok_or("Save target needs a parent directory")?;
    let temporary = parent.join(format!(".image-{}-{token}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(err)?;
    let result = (|| {
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
        drop(file);
        if !alive.load(Ordering::Relaxed) {
            return Err("Сохранение отменено".into());
        }
        std::fs::rename(&temporary, path).map_err(err)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_compiled_image_demo_keeps_geometry_and_typed_accepted_preview() {
        let mut app =
            ImageDemo::new(crate::generated_image_editor::window_definition(), false).unwrap();
        for size in [Size::new(1200.0, 780.0), Size::new(900.0, 640.0)] {
            app.update(size, 1.0).unwrap();
            let p = app.form.ui.image_editor_parts(app.editor).unwrap();
            let r = app.form.ui.runtime.bounds(p.viewport).unwrap();
            assert!(r.width > 400.0 && r.height > 250.0);
            let c = app.form.ui.runtime.clip(p.viewport).unwrap();
            assert!(r.intersection(c).unwrap().height >= r.height - 0.1);
        }
        let before = app.form.ui.image_editor_value(app.viewer).unwrap().cloned();
        app.form
            .ui
            .rotate_image_editor(app.editor, rust_desktop_ui_images::QuarterTurn::Clockwise)
            .unwrap();
        app.refresh().unwrap();
        assert_eq!(
            app.form.ui.image_editor_value(app.viewer).unwrap(),
            before.as_ref()
        );
        app.form.ui.apply_image_editor(app.editor).unwrap();
        app.refresh().unwrap();
        assert_eq!(
            app.form
                .ui
                .image_editor_value(app.viewer)
                .flatten()
                .unwrap()
                .width(),
            180
        );
    }
    #[test]
    fn atomic_png_save_and_cancel_preserve_existing_file() {
        let path = std::env::temp_dir().join(format!("image-test-{}.png", std::process::id()));
        let alive = AtomicBool::new(true);
        let b = sample().unwrap();
        let bytes = encode_bitmap_png(&b).unwrap();
        atomic_save(&path, &bytes, &alive).unwrap();
        let decoded = ImageSource::from_file(&path)
            .unwrap()
            .decode(Default::default())
            .unwrap();
        assert_eq!(decoded.bitmap().pixels(), b.pixels());
        alive.store(false, Ordering::Relaxed);
        assert!(atomic_save(&path, b"bad", &alive).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    #[test]
    fn actual_worker_load_is_bounded_draft_only_and_late_completion_is_ignored() {
        let mut app =
            ImageDemo::new(crate::generated_image_editor::window_definition(), false).unwrap();
        app.update(Size::new(1200.0, 780.0), 1.0).unwrap();
        let old = app.form.ui.image_editor_value(app.editor).unwrap().cloned();
        let path =
            std::env::temp_dir().join(format!("image-worker-test-{}.png", std::process::id()));
        let image = Bitmap::new_rgba8(2, 3, vec![42; 24]).unwrap();
        atomic_save(
            &path,
            &encode_bitmap_png(&image).unwrap(),
            &AtomicBool::new(true),
        )
        .unwrap();
        let token = app.form.ui.begin_image_editor_load(app.editor).unwrap();
        app.start(Pending::Load(token), path.clone());
        assert_eq!(app.wake_after(), Some(Duration::from_millis(40)));
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while app.busy && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
            app.refresh().unwrap();
        }
        assert!(!app.busy);
        assert_eq!(
            app.form
                .ui
                .image_editor_draft(app.editor)
                .flatten()
                .unwrap()
                .width(),
            2
        );
        assert_eq!(
            app.form.ui.image_editor_value(app.editor).unwrap(),
            old.as_ref()
        );
        let token = app.form.ui.begin_image_editor_load(app.editor).unwrap();
        app.start(Pending::Load(token), path.clone());
        app.form.ui.cancel_image_editor(app.editor).unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while app.busy && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
            app.refresh().unwrap();
        }
        assert!(!app.busy);
        assert_eq!(
            app.form.ui.image_editor_draft(app.editor).unwrap(),
            old.as_ref()
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn missing_or_malformed_registered_source_rejects_form_before_mount() {
        let definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui'><ImageEditor Source='MissingResource'/></Window>").unwrap();
        assert!(Form::new(definition, false).is_err());
    }
}
