//! Native command completion and grouped history verification without input.
use rust_desktop_ui_controls::{
    CommandAsyncOutcome, CommandHistory, CommandId, CommandParameter, CommandTaskError, Controls,
    Theme,
};
use rust_desktop_ui_core::{LayoutStyle, Length, Scene, Size, WidgetId};
use rust_desktop_ui_platform_winit::{
    AppWaker, DesktopApp, DesktopApplication, EventResponse, PlatformEvent, RendererStats,
    RunOptions, WindowKey, WindowSpec, run_multi,
};
use std::{
    cell::RefCell,
    process::ExitCode,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Clone, PartialEq)]
struct Document {
    value: u32,
    label: String,
}
#[derive(Default)]
struct Proof {
    frames: usize,
    ticks: usize,
    result: Option<Result<u32, CommandTaskError>>,
    cancellation: Option<Result<(), CommandTaskError>>,
    history_verified: bool,
    completed: bool,
    glyphs: usize,
    error: Option<String>,
    last_waker: Option<AppWaker>,
    watchdog: Option<thread::JoinHandle<()>>,
}
struct View {
    ui: Controls,
    command: CommandId,
    cancel: CommandId,
    button: WidgetId,
    label: WidgetId,
    phase: u8,
    proof: Rc<RefCell<Proof>>,
    timed_out: Arc<AtomicBool>,
    stop: Option<mpsc::Receiver<()>>,
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
fn style() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(480.0),
        height: Length::Px(42.0),
        ..Default::default()
    }
}
impl View {
    fn new(proof: Rc<RefCell<Proof>>, stop: mpsc::Receiver<()>) -> Result<Self, String> {
        let mut ui = Controls::new(Theme::FluentLight).map_err(err)?;
        let command = CommandId::from("calculate");
        let cancel = CommandId::from("cancel-task");
        ui.commands_mut()
            .register_toggle(command.clone(), "Calculate", false);
        ui.commands_mut()
            .set_parameter(&command, CommandParameter::new(21_u32));
        let completion = proof.clone();
        ui.commands_mut().on_async_execute(
            &command,
            |value: &u32, token| {
                thread::sleep(Duration::from_millis(180));
                token.check()?;
                Ok(value * 2)
            },
            move |result| completion.borrow_mut().result = Some(result),
        );
        ui.commands_mut().register(cancel.clone(), "Cancel task");
        let completion = proof.clone();
        ui.commands_mut().on_async_execute(
            &cancel,
            |_: &(), token| {
                thread::sleep(Duration::from_millis(100));
                token.check()?;
                Ok(())
            },
            move |result| completion.borrow_mut().cancellation = Some(result),
        );
        let button = ui
            .button(ui.root(), "Calculate", Some(command.clone()), style())
            .map_err(err)?;
        let label = ui
            .label(ui.root(), "Waiting for native command completion", style())
            .map_err(err)?;
        Ok(Self {
            ui,
            command,
            cancel,
            button,
            label,
            phase: 0,
            proof,
            timed_out: Arc::new(AtomicBool::new(false)),
            stop: Some(stop),
        })
    }
    fn verify_history(&mut self) -> Result<(), String> {
        let mut document = Document {
            value: 0,
            label: "Before".into(),
        };
        let original = document.clone();
        let mut history = CommandHistory::default();
        let mut group = history
            .begin(&mut document, "Calculation result")
            .map_err(err)?;
        group
            .apply(|draft| {
                draft.value = 42;
                Ok(())
            })
            .map_err(err)?;
        group
            .apply(|draft| {
                draft.label = "Calculated".into();
                Ok(())
            })
            .map_err(err)?;
        group.commit().map_err(err)?;
        let accepted = document.clone();
        history.undo(&mut document).map_err(err)?;
        if document != original {
            return Err("Grouped undo changed only part of the document".into());
        }
        history.redo(&mut document).map_err(err)?;
        if document != accepted {
            return Err("Grouped redo did not restore the document".into());
        }
        self.proof.borrow_mut().history_verified = true;
        Ok(())
    }
}
impl DesktopApp for View {
    fn set_waker(&mut self, waker: Option<AppWaker>) {
        if let Some(waker) = waker {
            self.proof.borrow_mut().last_waker = Some(waker.clone());
            let command_waker = waker.clone();
            self.ui.commands_mut().set_waker(Some(Arc::new(move || {
                command_waker.wake();
            })));
            if let Some(stop) = self.stop.take() {
                let expired = self.timed_out.clone();
                self.proof.borrow_mut().watchdog = Some(thread::spawn(move || {
                    if matches!(
                        stop.recv_timeout(Duration::from_secs(15)),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        expired.store(true, Ordering::Release);
                        waker.wake();
                    }
                }));
            }
        } else {
            self.ui.commands_mut().set_waker(None);
        }
    }
    fn update(&mut self, viewport: Size, _: f32) -> Result<(), String> {
        if self.timed_out.load(Ordering::Acquire) {
            return Err("Native command notification deadline exceeded".into());
        }
        self.ui.update(viewport).map_err(err)?;
        if self.phase == 1 && self.proof.borrow().result.is_some() {
            if self.proof.borrow().result != Some(Ok(42)) {
                return Err("Wrong worker result".into());
            }
            if !self.ui.is_enabled(self.button) || self.ui.checked(self.button) != Some(true) {
                return Err("Command placement did not leave its busy state".into());
            }
            let events = self.ui.commands_mut().drain_async_events();
            if events.len() != 1 || events[0].outcome != CommandAsyncOutcome::Succeeded {
                return Err("Completion was not delivered exactly once".into());
            }
            self.ui
                .set_label(self.label, "Result: 42; verifying cancellation")
                .map_err(err)?;
            if !self.ui.commands_mut().activate(&self.cancel)
                || !self.ui.commands_mut().cancel(&self.cancel)
            {
                return Err("Cancellation task was not accepted".into());
            }
            self.phase = 2;
        }
        if self.phase == 2 && self.proof.borrow().cancellation.is_some() {
            if self.proof.borrow().cancellation != Some(Err(CommandTaskError::Cancelled)) {
                return Err("Cancellation published a success".into());
            }
            let events = self.ui.commands_mut().drain_async_events();
            if events.len() != 1 || events[0].outcome != CommandAsyncOutcome::Cancelled {
                return Err("Cancellation completion was not delivered once".into());
            }
            if self.ui.drain_command_invocations().len() != 2 {
                return Err("Unexpected command invocation count".into());
            }
            self.verify_history()?;
            self.ui
                .set_label(
                    self.label,
                    "Result: 42; cancellation and undo/redo verified",
                )
                .map_err(err)?;
            self.phase = 3;
        }
        self.ui.update(viewport).map_err(err)?;
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.ui.scene()
    }
    fn title(&self) -> String {
        "Command lifecycle verification".into()
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if matches!(event, PlatformEvent::Tick) {
            self.proof.borrow_mut().ticks += 1;
            return Ok(EventResponse::redraw());
        }
        if let PlatformEvent::Input(event) = event {
            self.ui.dispatch(event).map_err(err)?;
        }
        Ok(EventResponse::default())
    }
    fn frame_presented(&mut self, stats: RendererStats) {
        self.proof.borrow_mut().frames += 1;
        self.proof.borrow_mut().glyphs = stats.glyphs;
        if self.phase == 0 {
            match self.ui.activate(self.button) {
                Ok(())
                    if !self.ui.is_enabled(self.button)
                        && self.ui.checked(self.button) == Some(true) =>
                {
                    self.phase = 1;
                }
                other => {
                    self.proof.borrow_mut().error =
                        Some(format!("Async placement activation failed: {other:?}"));
                }
            }
            if self.ui.commands_mut().activate(&self.command) {
                self.proof.borrow_mut().error =
                    Some("Busy command accepted a second invocation".into());
            }
        } else if self.phase == 3 {
            self.proof.borrow_mut().completed = true;
        }
    }
}
struct App {
    view: View,
    closed: bool,
}
impl DesktopApplication for App {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
        if let Some(error) = &self.view.proof.borrow().error {
            return Err(error.clone());
        }
        if self.closed || self.view.proof.borrow().completed {
            return Ok(Vec::new());
        }
        Ok(vec![WindowSpec::new(
            WindowKey::MAIN,
            RunOptions {
                smoke_device_recovery: false,
                width: 520.0,
                height: 180.0,
                min_size: None,
                show_render_stats: false,
                smoke_test: false,
                activate: false,
            },
        )])
    }
    fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp> {
        (key == WindowKey::MAIN).then_some(&self.view as &dyn DesktopApp)
    }
    fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)> {
        if key == WindowKey::MAIN {
            Some(&mut self.view)
        } else {
            None
        }
    }
    fn close_requested(&mut self, _: WindowKey) -> Result<(), String> {
        self.closed = true;
        Ok(())
    }
}
fn start() -> Result<(), String> {
    let proof = Rc::new(RefCell::new(Proof::default()));
    let (stop, receiver) = mpsc::channel();
    let result = run_multi(App {
        view: View::new(proof.clone(), receiver)?,
        closed: false,
    });
    let _ = stop.send(());
    if let Some(watchdog) = proof.borrow_mut().watchdog.take() {
        watchdog.join().map_err(|_| "Watchdog panicked")?;
    }
    result?;
    let proof = proof.borrow();
    if !proof.completed
        || !proof.history_verified
        || proof.frames < 2
        || proof.ticks == 0
        || proof.glyphs == 0
        || proof.last_waker.as_ref().is_none_or(AppWaker::wake)
    {
        return Err(format!(
            "Incomplete command proof: frames={} ticks={} complete={}",
            proof.frames, proof.ticks, proof.completed
        ));
    }
    println!(
        "COMMAND PASS: frames={} ticks={} glyphs={} typed_result=42 busy_rejected=true cancellation=true grouped_undo_redo=true polling=false activate=false disposed=true",
        proof.frames, proof.ticks, proof.glyphs
    );
    Ok(())
}
fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("COMMAND FAIL: {error}");
            ExitCode::FAILURE
        }
    }
}
