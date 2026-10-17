//! Explicit native service demonstration. Clipboard access requires an opt-in
//! command-line switch and never prints its contents. Jump Lists are explicit.
use rust_desktop_ui_core::{Color, InputEvent, Key, Rect, Scene, Size, TextRun};
use rust_desktop_ui_platform_winit::*;
use std::sync::{Arc, Mutex};
const TEXT: &str = "Platform clipboard fixture: Привет 🦀";
const HTML: &str = "<b>Platform clipboard fixture: Привет 🦀</b>";
fn icon() -> ClipboardImage {
    ClipboardImage {
        width: 16,
        height: 16,
        rgba: [20, 110, 220, 255].repeat(256),
    }
}
struct Demo {
    scene: Scene,
    handle: Option<PlatformServiceHandle>,
    retained: Arc<Mutex<Option<PlatformServiceHandle>>>,
    pending: Vec<(ServiceRequestId, String)>,
    clipboard: bool,
    app_id: String,
    status: String,
}
impl Demo {
    fn submit(&mut self, name: &str, request: PlatformServiceRequest) {
        let Some(handle) = &self.handle else { return };
        match handle.submit(request) {
            Ok(id) => self.pending.push((id, name.into())),
            Err(error) => println!("SERVICE {name} ERROR {:?}", error.kind),
        }
    }
    fn action(&mut self, key: &str) {
        match key.to_ascii_lowercase().as_str() {
            "c" if self.clipboard => self.submit(
                "clipboard_write",
                PlatformServiceRequest::WriteClipboard(ClipboardContent {
                    text: Some(TEXT.into()),
                    html: Some(HTML.into()),
                    image: Some(icon()),
                }),
            ),
            "t" if self.clipboard => self.submit(
                "clipboard_text",
                PlatformServiceRequest::ReadClipboard(ClipboardFormat::Text),
            ),
            "h" if self.clipboard => self.submit(
                "clipboard_html",
                PlatformServiceRequest::ReadClipboard(ClipboardFormat::Html),
            ),
            "i" if self.clipboard => self.submit(
                "clipboard_image",
                PlatformServiceRequest::ReadClipboard(ClipboardFormat::Image),
            ),
            "n" => self.submit(
                "tray_set",
                PlatformServiceRequest::SetTray(Some(TrayIcon {
                    image: icon(),
                    tooltip: "Platform services fixture".into(),
                    actions: vec![
                        TrayAction {
                            id: 1,
                            label: "Fixture action".into(),
                            enabled: true,
                        },
                        TrayAction {
                            id: 2,
                            label: "Disabled action".into(),
                            enabled: false,
                        },
                    ],
                })),
            ),
            "p" => {
                self.submit(
                    "taskbar_progress",
                    PlatformServiceRequest::SetTaskbarProgress(TaskbarProgress::Normal {
                        completed: 1,
                        total: 2,
                    }),
                );
                self.submit(
                    "taskbar_overlay",
                    PlatformServiceRequest::SetTaskbarOverlay {
                        image: Some(icon()),
                        description: "Fixture overlay".into(),
                    },
                );
            }
            "j" => self.submit(
                "jump_publish",
                PlatformServiceRequest::PublishJumpList(JumpList {
                    app_id: self.app_id.clone(),
                    tasks: vec![JumpListTask {
                        title: "Platform fixture task".into(),
                        arguments: "--jump-activation".into(),
                    }],
                }),
            ),
            "d" => self.submit(
                "jump_delete",
                PlatformServiceRequest::DeleteJumpList {
                    app_id: self.app_id.clone(),
                },
            ),
            "r" => {
                self.submit("tray_remove", PlatformServiceRequest::SetTray(None));
                self.submit(
                    "taskbar_clear",
                    PlatformServiceRequest::SetTaskbarProgress(TaskbarProgress::None),
                );
                self.submit(
                    "overlay_clear",
                    PlatformServiceRequest::SetTaskbarOverlay {
                        image: None,
                        description: String::new(),
                    },
                );
            }
            "w" => {
                if let Some(h) = self.handle.clone() {
                    std::thread::spawn(move || {
                        let _ = h.submit(PlatformServiceRequest::Dispatch {
                            channel: 75,
                            payload: b"fixture".to_vec(),
                        });
                    });
                }
            }
            "x" => {
                if let Some(h) = &self.handle {
                    let id = h
                        .submit(PlatformServiceRequest::Dispatch {
                            channel: 76,
                            payload: vec![],
                        })
                        .unwrap();
                    println!("CANCEL {}", h.cancel(&id));
                }
            }
            _ => {}
        }
    }
}
impl DesktopApp for Demo {
    fn platform_services_changed(&mut self, handle: Option<PlatformServiceHandle>) {
        if let Some(h) = &handle {
            println!("CAPABILITIES {:?}", h.capabilities());
            *self.retained.lock().unwrap() = Some(h.clone());
        }
        self.handle = handle;
    }
    fn platform_service_completed(
        &mut self,
        completion: PlatformServiceCompletion,
    ) -> Result<EventResponse, String> {
        let name = self
            .pending
            .iter()
            .position(|(id, _)| *id == completion.id)
            .map(|i| self.pending.remove(i).1)
            .unwrap_or_else(|| "dispatch".into());
        match completion.result {
            Ok(PlatformServiceValue::Clipboard(content)) => {
                let matches = match name.as_str() {
                    "clipboard_text" => content.text.as_deref() == Some(TEXT),
                    "clipboard_html" => content.html.as_deref() == Some(HTML),
                    "clipboard_image" => content.image.as_ref() == Some(&icon()),
                    _ => false,
                };
                println!("SERVICE {name} OK fixture_match={matches}");
                self.status = format!("{name}: fixture_match={matches}");
            }
            Ok(PlatformServiceValue::Dispatched { channel, payload }) => {
                println!(
                    "SERVICE {name} OK channel={channel} bytes={}",
                    payload.len()
                );
                self.status = format!("UI dispatch received on channel {channel}");
            }
            Ok(PlatformServiceValue::Done) => {
                println!("SERVICE {name} OK");
                self.status = format!("{name}: OK");
            }
            Err(error) => {
                println!(
                    "SERVICE {name} ERROR {:?} formats_written={}",
                    error.kind, error.clipboard_formats_written
                );
                self.status = format!("{name}: {:?}", error.kind);
            }
        }
        Ok(EventResponse::redraw())
    }
    fn platform_service_event(
        &mut self,
        event: PlatformServiceEvent,
    ) -> Result<EventResponse, String> {
        println!("SHELL_EVENT {event:?}");
        self.status = format!("{event:?}");
        Ok(EventResponse::redraw())
    }
    fn update(&mut self, viewport: Size, _scale: f32) -> Result<(), String> {
        let mut scene = Scene::default();
        scene.fill(
            Rect::new(0.0, 0.0, viewport.width, viewport.height),
            Color::rgb(244, 246, 249),
        );
        for (i, line) in [
            "Platform services — explicit Windows operations",
            "N: tray; P: taskbar; J: publish Jump List; D: delete list",
            "R: remove tray/overlay/progress; W: worker dispatch; X: cancel",
            "C/T/H/I: write/read clipboard fixture (requires --clipboard-fixture)",
            self.status.as_str(),
        ]
        .iter()
        .enumerate()
        {
            scene.text(TextRun::new(
                *line,
                Rect::new(24.0, 24.0 + i as f32 * 34.0, viewport.width - 48.0, 28.0),
                Color::rgb(25, 35, 45),
                16.0,
            ));
        }
        self.scene = scene;
        Ok(())
    }
    fn scene(&self) -> &Scene {
        &self.scene
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if let PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Character(key),
            repeat: false,
            ..
        }) = event
        {
            self.action(&key);
        }
        Ok(EventResponse::default())
    }
    fn title(&self) -> String {
        format!("Platform services {}", std::process::id())
    }
}
fn main() -> Result<(), String> {
    if std::env::args().any(|s| s == "--jump-activation") {
        println!("JUMP_ACTIVATED");
        return Ok(());
    }
    let retained = Arc::new(Mutex::new(None));
    let app_id = format!("RustUiEngine.ServiceFixture.{}", std::process::id());
    println!("APP_ID {app_id}");
    run(
        Demo {
            scene: Scene::default(),
            handle: None,
            retained: retained.clone(),
            pending: vec![],
            clipboard: std::env::args().any(|s| s == "--clipboard-fixture"),
            app_id,
            status: "No clipboard or shell mutations until a key command".into(),
        },
        RunOptions {
            width: 900.0,
            height: 300.0,
            show_render_stats: false,
            ..Default::default()
        },
    )?;
    if let Some(h) = retained.lock().unwrap().as_ref() {
        println!(
            "CLOSED_HANDLE {:?}",
            h.submit(PlatformServiceRequest::Dispatch {
                channel: 0,
                payload: vec![]
            })
            .unwrap_err()
            .kind
        );
    }
    Ok(())
}
