//! Passive native wake verification. No polling timer or synthetic input.
use rust_desktop_ui_controls::{
    BindingContext, CollectionWakeSubscription, Controls, DataValue, GridControlSettings, Theme,
};
use rust_desktop_ui_core::{LayoutStyle, Length, Scene, Size, WidgetId};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    ObservableCollection, TableSchema,
};
use rust_desktop_ui_platform_winit::{
    AppWaker, DesktopApp, DesktopApplication, EventResponse, PlatformEvent, RendererStats,
    RunOptions, WindowKey, WindowSpec, run_multi,
};
use std::{cell::RefCell, process::ExitCode, rc::Rc, sync::mpsc, thread, time::Duration};

#[derive(Default)]
struct Proof {
    frames: usize,
    ticks: usize,
    applied: bool,
    complete: bool,
    data_wakes: usize,
    binding_wakes: usize,
    source_revision: u64,
    glyphs: usize,
    error: Option<String>,
    closed_waker: Option<AppWaker>,
    worker: Option<thread::JoinHandle<()>>,
}
enum Work {
    Update,
    Timeout,
}
struct View {
    ui: Controls,
    grid: WidgetId,
    collection: ObservableCollection,
    bindings: BindingContext,
    binding_wake: Option<CollectionWakeSubscription>,
    messages: mpsc::Receiver<Work>,
    sender: mpsc::Sender<Work>,
    stop: Option<mpsc::Receiver<()>>,
    initial_frame: Option<mpsc::Sender<()>>,
    initial_frame_ready: Option<mpsc::Receiver<()>>,
    ready: bool,
    proof: Rc<RefCell<Proof>>,
}
fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl View {
    fn new(
        collection: ObservableCollection,
        proof: Rc<RefCell<Proof>>,
        stop: mpsc::Receiver<()>,
    ) -> Result<Self, String> {
        let mut ui = Controls::new(Theme::FluentLight).map_err(error)?;
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::from_collection(collection.clone()))
                .map_err(error)?,
        ));
        let grid = ui
            .grid_control(
                ui.root(),
                source,
                GridControlSettings::default(),
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )
            .map_err(error)?;
        let mut bindings =
            BindingContext::new(DataValue::object([("Rows", DataValue::List(Vec::new()))]))
                .map_err(error)?;
        bindings
            .register_collection("Rows", collection.clone(), "Key")
            .map_err(error)?;
        let (sender, messages) = mpsc::channel();
        let (initial_frame, initial_frame_ready) = mpsc::channel();
        Ok(Self {
            ui,
            grid,
            collection,
            bindings,
            binding_wake: None,
            messages,
            sender,
            stop: Some(stop),
            initial_frame: Some(initial_frame),
            initial_frame_ready: Some(initial_frame_ready),
            ready: false,
            proof,
        })
    }
}
impl DesktopApp for View {
    fn set_waker(&mut self, waker: Option<AppWaker>) {
        self.binding_wake = None;
        if let Some(waker) = waker {
            self.proof.borrow_mut().closed_waker = Some(waker.clone());
            if let (Some(stop), Some(initial)) = (self.stop.take(), self.initial_frame_ready.take())
            {
                let worker_waker = waker.clone();
                let sender = self.sender.clone();
                self.proof.borrow_mut().worker = Some(thread::spawn(move || {
                    if initial.recv_timeout(Duration::from_secs(12)).is_err() {
                        let _ = sender.send(Work::Timeout);
                        worker_waker.wake();
                        return;
                    }
                    // Collection values stay on the UI thread. Native events,
                    // rather than a wake_after timer, deliver this message.
                    thread::sleep(Duration::from_millis(700));
                    if sender.send(Work::Update).is_err() {
                        return;
                    }
                    for _ in 0..64 {
                        worker_waker.wake();
                    }
                    if stop.recv_timeout(Duration::from_secs(12)).is_err() {
                        let _ = sender.send(Work::Timeout);
                        worker_waker.wake();
                    }
                }));
            }
            let data_waker = waker.clone();
            let proof = self.proof.clone();
            if let Err(error) = self.ui.set_data_waker(Some(Rc::new(move || {
                proof.borrow_mut().data_wakes += 1;
                data_waker.wake();
            }))) {
                self.proof.borrow_mut().error = Some(error.to_string());
            }
            let proof = self.proof.clone();
            match self.bindings.subscribe_collection_wake(move || {
                proof.borrow_mut().binding_wakes += 1;
                waker.wake();
            }) {
                Ok(guard) => self.binding_wake = Some(guard),
                Err(error) => self.proof.borrow_mut().error = Some(error.to_string()),
            }
        } else if let Err(error) = self.ui.set_data_waker(None) {
            self.proof.borrow_mut().error = Some(error.to_string());
        }
    }
    fn update(&mut self, viewport: Size, _: f32) -> Result<(), String> {
        self.bindings.synchronize_collections().map_err(error)?;
        self.ui.update(viewport).map_err(error)?;
        if self.proof.borrow().applied {
            let source = self
                .ui
                .grid_source(self.grid)
                .ok_or("Grid source missing")?;
            if source.borrow().table().value(42, "Value")
                != Some(&CellValue::Text("Updated through wake".into()))
            {
                return Err("Source did not synchronize".into());
            }
            if self.bindings.get("Rows[0].Value").map_err(error)?.as_str()
                != Some("Updated through wake")
            {
                return Err("Binding projection did not synchronize".into());
            }
            let frame = self.ui.grid_frame(self.grid).ok_or("Grid frame missing")?;
            if !frame
                .semantics
                .rows
                .iter()
                .flat_map(|row| &row.cells)
                .any(|cell| cell.label == "Updated through wake")
            {
                return Err("Grid still displays stale data".into());
            }
            self.proof.borrow_mut().source_revision = source.borrow().revision();
            self.ready = true;
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.ui.scene()
    }
    fn title(&self) -> String {
        "Collection wake verification".into()
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if matches!(event, PlatformEvent::Tick) {
            self.proof.borrow_mut().ticks += 1;
            while let Ok(message) = self.messages.try_recv() {
                match message {
                    Work::Update => {
                        self.collection
                            .update_cell(
                                42,
                                "Value",
                                CellValue::Text("Updated through wake".into()),
                            )
                            .map_err(error)?;
                        self.proof.borrow_mut().applied = true;
                    }
                    Work::Timeout => return Err("Native notification deadline exceeded".into()),
                }
            }
            Ok(EventResponse::redraw())
        } else {
            // This probe creates no input. Ordinary native lifecycle events are
            // still routed through the same Controls adapter as real forms.
            if let PlatformEvent::Input(input) = event {
                self.ui.dispatch(input).map_err(error)?;
            }
            Ok(EventResponse::default())
        }
    }
    fn frame_presented(&mut self, stats: RendererStats) {
        self.proof.borrow_mut().frames += 1;
        self.proof.borrow_mut().glyphs = stats.glyphs;
        if self.ready {
            self.proof.borrow_mut().complete = true;
        } else if let Some(initial) = self.initial_frame.take() {
            let _ = initial.send(());
        }
    }
}
struct App {
    view: View,
    retired: bool,
}
impl DesktopApplication for App {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
        if let Some(error) = &self.view.proof.borrow().error {
            return Err(error.clone());
        }
        if self.view.proof.borrow().complete || self.retired {
            return Ok(Vec::new());
        }
        Ok(vec![WindowSpec::new(
            WindowKey::MAIN,
            RunOptions {
                width: 520.0,
                height: 240.0,
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
        self.retired = true;
        Ok(())
    }
}
fn start() -> Result<(), String> {
    let schema =
        TableSchema::new(vec![FieldSchema::new("Value", FieldType::Text)]).map_err(error)?;
    let collection = ObservableCollection::new(
        DataTable::new(
            schema,
            vec![DataRow::new(
                42,
                vec![CellValue::Text("Waiting without polling".into())],
            )],
        )
        .map_err(error)?,
    );
    let proof = Rc::new(RefCell::new(Proof::default()));
    let (stop, stopped) = mpsc::channel();
    let view = View::new(collection.clone(), proof.clone(), stopped)?;
    let result = run_multi(App {
        view,
        retired: false,
    });
    let _ = stop.send(());
    let worker = proof.borrow_mut().worker.take();
    if let Some(worker) = worker {
        worker.join().map_err(|_| "Wake worker panicked")?;
    }
    result?;
    let before = (proof.borrow().data_wakes, proof.borrow().binding_wakes);
    collection
        .update_cell(42, "Value", CellValue::Text("After view disposal".into()))
        .map_err(error)?;
    let proof = proof.borrow();
    if !proof.complete
        || proof.frames < 2
        || proof.ticks == 0
        || proof.source_revision == 0
        || proof.glyphs == 0
        || before.0 == 0
        || before.1 == 0
        || (proof.data_wakes, proof.binding_wakes) != before
        || proof.closed_waker.as_ref().is_none_or(AppWaker::wake)
    {
        return Err(format!(
            "Wake proof incomplete: frames={} ticks={} data={} bindings={} complete={}",
            proof.frames, proof.ticks, proof.data_wakes, proof.binding_wakes, proof.complete
        ));
    }
    println!(
        "WAKE PASS: presented={} ticks={} data_notifications={} binding_notifications={} source_revision={} glyphs={} polling=false activate=false disposed=true",
        proof.frames,
        proof.ticks,
        proof.data_wakes,
        proof.binding_wakes,
        proof.source_revision,
        proof.glyphs
    );
    Ok(())
}
fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("WAKE FAIL: {error}");
            ExitCode::FAILURE
        }
    }
}
