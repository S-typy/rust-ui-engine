//! Explicit application-owned worker tasks, real splash windows and toast actions.
use crate::form::Form;
use rust_desktop_ui_controls::{LoadingResult, LoadingToken, NotificationAction, NotificationSpec};
use rust_desktop_ui_core::{LayoutStyle, Length, Rect, Scene, Size, WidgetId};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, DesktopApplication, EventResponse, PlatformEvent, RunOptions,
    SplashSession, SplashState, WindowKey, WindowSpec, accesskit,
};
use rust_desktop_ui_xaml::WindowDefinition;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
enum Action {
    Toast,
    Queue,
    Invoked,
    WaitSuccess,
    WaitError,
    WaitCancel,
    SplashSuccess,
    SplashError,
    SplashCancel,
}
struct MainView {
    form: Form,
    waker: Option<platform::AppWaker>,
    diagnostics: bool,
    last_snapshot: String,
}
impl DesktopApp for MainView {
    fn set_waker(&mut self, w: Option<platform::AppWaker>) {
        self.waker = w.clone();
        self.form.set_waker(w);
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.form.update(size, scale)?;
        if self.diagnostics {
            let snapshot = crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view);
            if snapshot != self.last_snapshot {
                println!("batch-state {snapshot}");
                self.last_snapshot = snapshot;
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
        let mut response = self.form.event(event)?;
        response.redraw = true;
        Ok(response)
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
    fn wake_after(&self) -> Option<Duration> {
        self.form.wake_after()
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
}
struct Operation {
    session: SplashSession,
    last: Option<SplashState>,
}
pub(crate) struct NotificationsApplication {
    main: MainView,
    options: RunOptions,
    alive: bool,
    diagnostics: bool,
    queue: Rc<RefCell<VecDeque<Action>>>,
    center: WidgetId,
    wait: WidgetId,
    loading: Option<(LoadingToken, Operation)>,
    splash: Option<(WindowKey, Operation)>,
    views: HashMap<WindowKey, Form>,
    next: u64,
    invocations: usize,
    last_history: usize,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
impl NotificationsApplication {
    pub(crate) fn new(
        definition: WindowDefinition,
        options: RunOptions,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let mut form = Form::new(definition, diagnostics)?;
        let center = form
            .find_name("Notifications")
            .ok_or("Notification center missing")?;
        let wait = form
            .find_name("WaitSurface")
            .ok_or("Loading overlay missing")?;
        let actions = form
            .find_name("NotificationActions")
            .ok_or("Actions host missing")?;
        let queue = Rc::new(RefCell::new(VecDeque::new()));
        for (name, label, action) in [
            ("NotifyToast", "Уведомление", Action::Toast),
            ("NotifyQueue", "Очередь из пяти", Action::Queue),
            ("WaitSuccess", "Ожидание: успех", Action::WaitSuccess),
            ("WaitError", "Ожидание: ошибка", Action::WaitError),
            ("WaitCancel", "Отменить ожидание", Action::WaitCancel),
            ("SplashSuccess", "Окно: успех", Action::SplashSuccess),
            ("SplashError", "Окно: ошибка", Action::SplashError),
            ("SplashCancel", "Отменить окно", Action::SplashCancel),
            ("NotifyInvoked", "Открыть результат", Action::Invoked),
        ] {
            let id = rust_desktop_ui_controls::CommandId::from(name);
            form.ui.commands_mut().register(id.clone(), label);
            let queue = queue.clone();
            form.ui.commands_mut().on_execute(&id, move || {
                let mut q = queue.borrow_mut();
                if q.len() < 32 {
                    q.push_back(action);
                }
            });
            if !matches!(action, Action::Invoked) {
                let width = form
                    .ui
                    .measure_text(label, 1000.0, false)
                    .map_err(err)?
                    .width
                    + 32.0;
                let button = form
                    .ui
                    .button(
                        actions,
                        label,
                        Some(id),
                        LayoutStyle {
                            width: Length::Px(width),
                            height: Length::Px(36.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )
                    .map_err(err)?;
                form.ui.set_automation_name(button, label).map_err(err)?;
                form.view.names.insert(name.into(), button);
            }
        }
        // A scoped shortcut demonstrates an application notification without
        // activating a toolbar button or moving focus away from a draft.
        for (key, action) in [
            ("n", Action::Toast),
            ("q", Action::Queue),
            ("w", Action::WaitSuccess),
            ("e", Action::WaitError),
            ("s", Action::SplashSuccess),
            ("f", Action::SplashError),
            ("c", Action::SplashCancel),
        ] {
            let shortcut_queue = queue.clone();
            form.ui
                .runtime
                .on_shortcut(
                    form.ui.root(),
                    rust_desktop_ui_core::KeyChord::new(
                        rust_desktop_ui_core::Key::Character(key.into()),
                        rust_desktop_ui_core::Modifiers {
                            control: true,
                            shift: true,
                            ..Default::default()
                        },
                    ),
                    false,
                    move |_, _| {
                        let mut queue = shortcut_queue.borrow_mut();
                        if queue.len() < 32 {
                            queue.push_back(action);
                        }
                    },
                )
                .map_err(err)?;
        }
        Ok(Self {
            main: MainView {
                form,
                waker: None,
                diagnostics,
                last_snapshot: String::new(),
            },
            options,
            alive: true,
            diagnostics,
            queue,
            center,
            wait,
            loading: None,
            splash: None,
            views: HashMap::new(),
            next: 1,
            invocations: 0,
            last_history: 0,
        })
    }
    fn label(&mut self, name: &str, text: &str) -> Result<(), String> {
        let id = self
            .main
            .form
            .find_name(name)
            .ok_or("Notification label absent")?;
        self.main.form.ui.set_label(id, text).map_err(err)
    }
    fn status(&mut self, text: &str) -> Result<(), String> {
        self.label("NotificationResult", text)?;
        if self.diagnostics {
            println!("notification-result={text}");
        }
        Ok(())
    }
    fn operation(&self, fail: bool, steps: u32) -> Result<Operation, String> {
        let (session, worker) = SplashSession::new("Подготовка операции")?;
        session.set_waker(self.main.waker.clone());
        // The demonstration worker is cancellable and never joined on the UI thread.
        std::thread::spawn(move || {
            for step in 0..=steps {
                if worker.is_finished() {
                    return;
                }
                let _ = worker.report(
                    format!("Выполнено: {}%", step * 100 / steps),
                    Some(f64::from(step) / f64::from(steps)),
                );
                std::thread::sleep(Duration::from_millis(150));
            }
            if fail {
                let _ = worker.fail("Операция завершилась ошибкой");
            } else {
                worker.succeed();
            }
        });
        Ok(Operation {
            session,
            last: None,
        })
    }
    fn toast(&mut self, number: Option<usize>) -> Result<(), String> {
        let mut spec = NotificationSpec::new(
            number.map_or_else(|| "Документ готов".into(), |n| format!("Уведомление {n}")),
            "Работа завершена. Черновик и фокус сохранены.",
        );
        spec.timeout = Some(Duration::from_secs(8));
        spec.actions.push(NotificationAction {
            label: "Открыть результат".into(),
            command: "NotifyInvoked".into(),
        });
        self.main
            .form
            .ui
            .post_notification(self.center, spec)
            .map_err(err)?;
        Ok(())
    }
    fn actions(&mut self) -> Result<bool, String> {
        let queued: Vec<_> = self.queue.borrow_mut().drain(..).collect();
        let changed = !queued.is_empty();
        for action in queued {
            match action {
                Action::Toast => self.toast(None)?,
                Action::Queue => {
                    for n in 1..=5 {
                        self.toast(Some(n))?;
                    }
                }
                Action::Invoked => {
                    self.invocations += 1;
                    self.status("Действие уведомления выполнено")?;
                }
                Action::WaitSuccess | Action::WaitError => {
                    if self.loading.is_none() {
                        let _ = self.main.form.ui.take_loading_result(self.wait);
                        let token = self
                            .main
                            .form
                            .ui
                            .begin_loading(self.wait, "Подготовка операции", true)
                            .map_err(err)?;
                        self.loading = Some((
                            token,
                            self.operation(matches!(action, Action::WaitError), 20)?,
                        ));
                        self.status("Ожидание началось")?;
                    }
                }
                Action::WaitCancel => {
                    if let Some((token, operation)) = &self.loading {
                        operation.session.cancel();
                        self.main
                            .form
                            .ui
                            .finish_loading(*token, LoadingResult::Cancelled)
                            .map_err(err)?;
                    }
                }
                Action::SplashSuccess | Action::SplashError => {
                    if self.splash.is_none() {
                        self.open_splash(matches!(action, Action::SplashError))?;
                    }
                }
                Action::SplashCancel => {
                    if let Some((_, operation)) = &self.splash {
                        operation.session.cancel();
                    }
                }
            }
        }
        Ok(changed)
    }
    fn open_splash(&mut self, fail: bool) -> Result<(), String> {
        let mut definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' Title='Операция выполняется' Width='440' Height='220' Padding='24'><StackPanel Spacing='18'><TextBlock Text='Выполняется операция' FontSize='22' FontWeight='SemiBold'/><TextBlock x:Name='SplashMessage' Text='Подготовка'/><ProgressBar x:Name='SplashProgress' Minimum='0' Maximum='1' Value='0'/><TextBlock Text='Главное окно остаётся доступным.' TextTone='Secondary'/></StackPanel></Window>").map_err(err)?;
        (
            definition.theme,
            definition.density,
            definition.accent_color,
        ) = self.main.form.theme_selection();
        let form = Form::new(definition, self.diagnostics)?;
        let key = WindowKey(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Window identities exhausted")?;
        self.views.insert(key, form);
        self.splash = Some((key, self.operation(fail, 40)?));
        self.status("Открыто окно ожидания")
    }
    fn poll(&mut self) -> Result<bool, String> {
        let mut changed = false;
        if let Some((token, operation)) = &mut self.loading {
            if let Some(result) = self.main.form.ui.take_loading_result(self.wait) {
                operation.session.cancel();
                let text = match result {
                    LoadingResult::Succeeded => "Ожидание завершено".into(),
                    LoadingResult::Cancelled => "Ожидание отменено".into(),
                    LoadingResult::Failed(message) => message,
                };
                self.loading = None;
                self.status(&text)?;
                changed = true;
            } else {
                let state = operation.session.state();
                if operation.last.as_ref() != Some(&state) {
                    operation.last = Some(state.clone());
                    changed = true;
                    match state {
                        SplashState::Running { message, progress } => {
                            if !self
                                .main
                                .form
                                .ui
                                .update_loading(*token, &message, progress)
                                .map_err(err)?
                            {
                                operation.session.cancel();
                            }
                        }
                        SplashState::Succeeded => {
                            self.main
                                .form
                                .ui
                                .finish_loading(*token, LoadingResult::Succeeded)
                                .map_err(err)?;
                        }
                        SplashState::Failed(message) => {
                            self.main
                                .form
                                .ui
                                .finish_loading(*token, LoadingResult::Failed(message))
                                .map_err(err)?;
                        }
                        SplashState::Cancelled => {
                            self.main
                                .form
                                .ui
                                .finish_loading(*token, LoadingResult::Cancelled)
                                .map_err(err)?;
                        }
                    }
                }
            }
        }
        if let Some((key, operation)) = &mut self.splash {
            let state = operation.session.state();
            if operation.last.as_ref() != Some(&state) {
                operation.last = Some(state.clone());
                changed = true;
                match state {
                    SplashState::Running { message, progress } => {
                        let view = self.views.get_mut(key).ok_or("Splash view absent")?;
                        let label = view.find_name("SplashMessage").unwrap();
                        let bar = view.find_name("SplashProgress").unwrap();
                        view.ui.set_label(label, &message).map_err(err)?;
                        view.ui
                            .set_progress_indeterminate(bar, progress.is_none())
                            .map_err(err)?;
                        if let Some(value) = progress {
                            view.ui.set_progress_value(bar, value).map_err(err)?;
                        }
                    }
                    state => {
                        self.splash = None;
                        self.status(match &state {
                            SplashState::Succeeded => "Окно ожидания завершено",
                            SplashState::Cancelled => "Окно ожидания отменено",
                            SplashState::Failed(message) => message,
                            _ => unreachable!(),
                        })?;
                    }
                }
            }
        }
        let history = self
            .main
            .form
            .ui
            .notification_history(self.center)
            .map_or(0, VecDeque::len);
        if changed || history != self.last_history {
            changed = true;
            self.last_history = history;
            self.label(
                "NotificationHistory",
                &format!("История: {history}; действий: {}", self.invocations),
            )?;
        }
        Ok(changed)
    }
}
impl DesktopApplication for NotificationsApplication {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
        if !self.alive {
            return Ok(Vec::new());
        }
        let changed = self.actions()? | self.poll()?;
        if changed && let Some(waker) = &self.main.waker {
            waker.wake();
        }
        let mut main = WindowSpec::new(WindowKey::MAIN, self.options.clone());
        main.activate = self.options.activate;
        let mut result = vec![main];
        if let Some((key, operation)) = &self.splash
            && let Some(spec) = operation.session.window_spec(
                *key,
                WindowKey::MAIN,
                RunOptions {
                    width: 440.0,
                    height: 220.0,
                    min_size: Some(Size::new(400.0, 200.0)),
                    show_render_stats: self.diagnostics,
                    ..Default::default()
                },
            )?
        {
            result.push(spec);
        }
        Ok(result)
    }
    fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp> {
        if key == WindowKey::MAIN {
            Some(&self.main)
        } else {
            self.views.get(&key).map(|v| v as &dyn DesktopApp)
        }
    }
    fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)> {
        if key == WindowKey::MAIN {
            Some(&mut self.main)
        } else {
            self.views.get_mut(&key).map(|v| v as &mut dyn DesktopApp)
        }
    }
    fn close_requested(&mut self, key: WindowKey) -> Result<(), String> {
        if key == WindowKey::MAIN {
            self.alive = false;
            if let Some((_, operation)) = &self.loading {
                operation.session.cancel();
            }
            if let Some((_, operation)) = &self.splash {
                operation.session.cancel();
            }
        } else if let Some((active, operation)) = &self.splash
            && *active == key
        {
            operation.session.cancel();
        }
        Ok(())
    }
    fn window_closed(&mut self, key: WindowKey) {
        if key != WindowKey::MAIN {
            self.views.remove(&key);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> NotificationsApplication {
        let mut app = NotificationsApplication::new(
            crate::generated_notifications::window_definition(),
            RunOptions::default(),
            false,
        )
        .unwrap();
        app.main.update(Size::new(1060.0, 760.0), 1.0).unwrap();
        app
    }
    #[test]
    fn splash_window_is_real_owned_nonactivating_and_retained_until_closed() {
        let mut app = app();
        let editor = app.main.form.find_name("NotificationDraft").unwrap();
        app.main
            .form
            .ui
            .runtime
            .apply(rust_desktop_ui_core::Mutation::Focus(Some(editor)))
            .unwrap();
        app.queue.borrow_mut().push_back(Action::SplashSuccess);
        let specs = app.windows().unwrap();
        assert_eq!(specs.len(), 2);
        let key = specs[1].key;
        assert!(!specs[1].activate && !specs[1].modal);
        assert_eq!(specs[1].owner, Some(WindowKey::MAIN));
        assert_eq!(app.main.form.ui.runtime.focused(), Some(editor));
        app.close_requested(key).unwrap();
        assert_eq!(app.windows().unwrap().len(), 1);
        assert!(app.window(key).is_some());
        app.window_closed(key);
        assert!(app.window(key).is_none());
        assert_eq!(app.main.form.ui.runtime.focused(), Some(editor));
    }
    #[test]
    fn compiled_demo_wait_preserves_draft_and_owner_close_cancels_worker() {
        let mut app = app();
        let editor = app.main.form.find_name("NotificationDraft").unwrap();
        app.main.form.ui.set_text(editor, "Pending").unwrap();
        app.queue.borrow_mut().push_back(Action::WaitSuccess);
        app.windows().unwrap();
        app.main.update(Size::new(1060.0, 760.0), 1.0).unwrap();
        assert!(app.loading.is_some());
        assert_eq!(app.main.form.ui.document(editor).unwrap().text(), "Pending");
        app.close_requested(WindowKey::MAIN).unwrap();
        assert!(app.windows().unwrap().is_empty());
        assert_eq!(
            app.loading.as_ref().unwrap().1.session.state(),
            SplashState::Cancelled
        );
    }
    #[test]
    fn compiled_zero_height_center_shortcut_toast_is_visible_and_preserves_draft_focus() {
        use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation};
        let mut app = app();
        let editor = app.main.form.find_name("NotificationDraft").unwrap();
        app.main
            .form
            .ui
            .runtime
            .apply(Mutation::Focus(Some(editor)))
            .unwrap();
        let accepted = app.main.form.ui.accepted_value(editor).unwrap().to_owned();
        app.main
            .form
            .ui
            .set_text(editor, "Pending notification draft")
            .unwrap();
        app.main
            .event(PlatformEvent::Input(InputEvent::KeyDown {
                key: Key::Character("N".into()),
                modifiers: Modifiers {
                    control: true,
                    shift: true,
                    ..Default::default()
                },
                repeat: false,
            }))
            .unwrap();
        app.windows().unwrap();
        app.main.update(Size::new(1060.0, 760.0), 1.0).unwrap();
        let ui = &app.main.form.ui;
        assert_eq!(ui.runtime.bounds(app.center).unwrap().height, 0.0);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(
            ui.document(editor).unwrap().text(),
            "Pending notification draft"
        );
        assert_eq!(ui.accepted_value(editor), Some(accepted.as_str()));
        let status = ui
            .semantics()
            .into_iter()
            .find(|n| ui.notification_message(n.id))
            .unwrap();
        let card = ui.runtime.tree().parent(status.id).unwrap();
        let bounds = ui.runtime.bounds(card).unwrap();
        assert!(bounds.width > 200.0 && bounds.height > 50.0);
        assert_eq!(
            bounds.intersection(ui.runtime.clip(card).unwrap()),
            Some(bounds)
        );
        assert!(
            bounds.x >= 0.0
                && bounds.y >= 0.0
                && bounds.x + bounds.width <= 1060.0
                && bounds.y + bounds.height <= 760.0
        );
    }
    #[test]
    fn compiled_wait_shortcut_preserves_draft_and_restores_focus_after_cancel() {
        use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation};
        let mut app = app();
        let editor = app.main.form.find_name("NotificationDraft").unwrap();
        app.main
            .form
            .ui
            .runtime
            .apply(Mutation::Focus(Some(editor)))
            .unwrap();
        let accepted = app.main.form.ui.accepted_value(editor).unwrap().to_owned();
        app.main
            .form
            .ui
            .set_text(editor, "Pending loading draft")
            .unwrap();
        app.main
            .event(PlatformEvent::Input(InputEvent::KeyDown {
                key: Key::Character("W".into()),
                modifiers: Modifiers {
                    control: true,
                    shift: true,
                    ..Default::default()
                },
                repeat: false,
            }))
            .unwrap();
        app.windows().unwrap();
        app.main.update(Size::new(1060.0, 760.0), 1.0).unwrap();
        assert!(app.loading.is_some());
        assert_eq!(
            app.main.form.ui.document(editor).unwrap().text(),
            "Pending loading draft"
        );
        assert_eq!(
            app.main.form.ui.accepted_value(editor),
            Some(accepted.as_str())
        );
        app.queue.borrow_mut().push_back(Action::WaitCancel);
        app.windows().unwrap();
        app.main.update(Size::new(1060.0, 760.0), 1.0).unwrap();
        assert_eq!(app.main.form.ui.runtime.focused(), Some(editor));
        assert_eq!(
            app.main.form.ui.document(editor).unwrap().text(),
            "Pending loading draft"
        );
        assert_eq!(
            app.main.form.ui.accepted_value(editor),
            Some(accepted.as_str())
        );
    }
    #[test]
    fn toast_action_is_visible_to_app_once_and_native_projection_is_live_status() {
        let mut app = app();
        app.toast(None).unwrap();
        app.main.update(Size::new(1060.0, 760.0), 1.0).unwrap();
        let tree = app.main.accessibility(1.0);
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == accesskit::Role::Status
                    && node.live() == Some(accesskit::Live::Polite))
        );
        app.queue.borrow_mut().push_back(Action::Invoked);
        app.windows().unwrap();
        app.windows().unwrap();
        assert_eq!(app.invocations, 1);
    }
}
