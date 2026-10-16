//! The example owns dialog business results; the host owns native modality.
use crate::form::Form;
use rust_desktop_ui_platform_winit::{
    DesktopApp, DesktopApplication, DialogButton, DialogOutcome, DialogSession, RunOptions,
    WindowKey, WindowSpec,
};
use rust_desktop_ui_xaml::WindowDefinition;
use std::collections::HashMap;

pub(crate) struct WorkspaceApplication {
    views: HashMap<WindowKey, Form>,
    options: RunOptions,
    alive: bool,
    modal: Option<WindowKey>,
    next_key: u64,
    diagnostics: bool,
}
impl WorkspaceApplication {
    pub(crate) fn set_dialog_directory(&mut self, directory: Option<std::path::PathBuf>) {
        self.views
            .get_mut(&WindowKey::MAIN)
            .unwrap()
            .workspace
            .initial_directory = directory;
    }
    pub(crate) fn new(
        definition: WindowDefinition,
        options: RunOptions,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let form = Form::new(definition, diagnostics)?;
        Ok(Self {
            views: HashMap::from([(WindowKey::MAIN, form)]),
            options,
            alive: true,
            modal: None,
            next_key: 1,
            diagnostics,
        })
    }
    fn reconcile_result(&mut self) -> Result<(), String> {
        let Some(key) = self.modal else { return Ok(()) };
        let view = self.views.get_mut(&key).ok_or("Modal view absent")?;
        let Some(result) = view
            .workspace
            .session
            .as_mut()
            .and_then(DialogSession::take_result)
        else {
            return Ok(());
        };
        let accepted = (result == DialogOutcome::Button(DialogButton::Ok))
            .then(|| {
                view.find_name("DialogEditor")
                    .and_then(|id| view.ui.document(id))
                    .map(|doc| doc.text().to_owned())
            })
            .flatten();
        self.modal = None;
        let owner = self
            .views
            .get_mut(&WindowKey::MAIN)
            .ok_or("Dialog owner absent")?;
        if let Some(text) = accepted {
            crate::workspace_model::label(owner, "ModalResult", &format!("Принято: {text}"))?;
        }
        crate::workspace_model::label(
            owner,
            "DialogStatus",
            if result == DialogOutcome::Cancelled {
                "Диалог отменён; значение сохранено"
            } else {
                "Значение принято"
            },
        )?;
        println!("modal-completed key={} result={result:?}", key.0);
        Ok(())
    }
}
impl DesktopApplication for WorkspaceApplication {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
        self.reconcile_result()?;
        if !self.alive {
            return Ok(Vec::new());
        }
        let owner = self
            .views
            .get_mut(&WindowKey::MAIN)
            .ok_or("Main view absent")?;
        if std::mem::take(&mut owner.workspace.open_modal) && self.modal.is_none() {
            let mut definition = crate::generated_dialog::window_definition();
            (
                definition.theme,
                definition.density,
                definition.accent_color,
            ) = owner.theme_selection();
            let mut view = Form::new(definition, self.diagnostics)?;
            view.workspace.session = Some(DialogSession::new(
                Some(DialogOutcome::Button(DialogButton::Ok)),
                DialogOutcome::Cancelled,
            ));
            let key = WindowKey(self.next_key);
            self.next_key = self.next_key.checked_add(1).ok_or("Window key exhausted")?;
            self.views.insert(key, view);
            self.modal = Some(key);
        }
        let mut main = WindowSpec::new(WindowKey::MAIN, self.options.clone());
        main.activate = self.options.activate;
        let mut specs = vec![main];
        if let Some(key) = self.modal {
            let mut dialog = WindowSpec::new(
                key,
                RunOptions {
                    width: 560.0,
                    height: 320.0,
                    min_size: Some(rust_desktop_ui_core::Size::new(420.0, 280.0)),
                    show_render_stats: self.diagnostics,
                    activate: self.options.activate,
                    ..Default::default()
                },
            );
            dialog.owner = Some(WindowKey::MAIN);
            dialog.modal = true;
            dialog.activate = self.options.activate;
            specs.push(dialog);
        }
        Ok(specs)
    }
    fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp> {
        self.views.get(&key).map(|view| view as &dyn DesktopApp)
    }
    fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)> {
        self.views
            .get_mut(&key)
            .map(|view| view as &mut dyn DesktopApp)
    }
    fn close_requested(&mut self, key: WindowKey) -> Result<(), String> {
        if key == WindowKey::MAIN {
            if let Some(key) = self.modal
                && let Some(session) = self
                    .views
                    .get_mut(&key)
                    .and_then(|view| view.workspace.session.as_mut())
            {
                session.owner_closed();
            }
            self.alive = false;
        } else if let Some(session) = self
            .views
            .get_mut(&key)
            .and_then(|view| view.workspace.session.as_mut())
        {
            session.cancel();
        }
        Ok(())
    }
    fn window_closed(&mut self, key: WindowKey) {
        self.views.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Size;
    fn app() -> WorkspaceApplication {
        WorkspaceApplication::new(
            crate::generated_workspace::window_definition(),
            RunOptions {
                activate: false,
                ..Default::default()
            },
            false,
        )
        .unwrap()
    }
    fn execute(app: &mut WorkspaceApplication, key: WindowKey, action: &str) {
        crate::workspace_model::command(app.views.get_mut(&key).unwrap(), action).unwrap();
    }
    #[test]
    fn modal_cancel_accept_and_native_close_preserve_owner_document() {
        let mut app = app();
        let owner = app.views.get_mut(&WindowKey::MAIN).unwrap();
        owner.update(Size::new(1200.0, 820.0), 1.0).unwrap();
        let editor = owner.find_name("WorkspaceEditor").unwrap();
        owner
            .ui
            .set_text(editor, "Uncommitted owner draft")
            .unwrap();
        execute(&mut app, WindowKey::MAIN, "WorkspaceDialog");
        let specs = app.windows().unwrap();
        assert_eq!(specs.len(), 2);
        assert!(specs[1].modal);
        assert_eq!(specs[1].owner, Some(WindowKey::MAIN));
        let first = specs[1].key;
        let dialog = app.views.get_mut(&first).unwrap();
        dialog.update(Size::new(560.0, 320.0), 1.0).unwrap();
        let field = dialog.find_name("DialogEditor").unwrap();
        dialog.ui.set_text(field, "").unwrap();
        execute(&mut app, first, "DialogAccept");
        assert_eq!(app.windows().unwrap().len(), 2);
        assert!(app.views[&first].ui.validation_error(field).is_some());
        app.views
            .get_mut(&first)
            .unwrap()
            .ui
            .set_text(field, "Accepted name")
            .unwrap();
        execute(&mut app, first, "DialogAccept");
        assert_eq!(app.windows().unwrap().len(), 1);
        let owner = &app.views[&WindowKey::MAIN];
        assert_eq!(
            owner.ui.document(editor).unwrap().text(),
            "Uncommitted owner draft"
        );
        assert_eq!(
            owner.ui.label_text(owner.find_name("ModalResult").unwrap()),
            Some("Принято: Accepted name")
        );
        app.window_closed(first);
        execute(&mut app, WindowKey::MAIN, "WorkspaceDialog");
        let second = app.windows().unwrap()[1].key;
        assert_ne!(first, second);
        app.close_requested(second).unwrap();
        assert_eq!(app.windows().unwrap().len(), 1);
        let owner = &app.views[&WindowKey::MAIN];
        assert_eq!(
            owner.ui.label_text(owner.find_name("ModalResult").unwrap()),
            Some("Принято: Accepted name")
        );
        app.window_closed(second);
        execute(&mut app, WindowKey::MAIN, "WorkspaceDialog");
        let last = app.windows().unwrap()[1].key;
        app.close_requested(WindowKey::MAIN).unwrap();
        assert!(app.windows().unwrap().is_empty());
        assert!(
            app.views[&last]
                .workspace
                .session
                .as_ref()
                .unwrap()
                .is_finished()
        );
    }
    #[test]
    fn picker_cancel_and_error_preserve_path_and_queries_run_without_io() {
        use rust_desktop_ui_platform_winit::{DialogCompletion, DialogError, DialogErrorKind};
        let mut app = app();
        let form = app.views.get_mut(&WindowKey::MAIN).unwrap();
        crate::workspace_model::label(form, "FilePath", "Existing selection").unwrap();
        for result in [
            Ok(DialogOutcome::Cancelled),
            Err(DialogError {
                kind: DialogErrorKind::Native,
                message: "Fixture error".into(),
            }),
        ] {
            form.dialog_completed(DialogCompletion { id: 7, result })
                .unwrap();
            assert_eq!(
                form.ui.label_text(form.find_name("FilePath").unwrap()),
                Some("Existing selection")
            );
        }
        crate::workspace_model::command(form, "WorkspaceQuery").unwrap();
        let result = form
            .ui
            .label_text(form.find_name("QueryResult").unwrap())
            .unwrap();
        assert!(result.contains("Общий итог: 25"));
        assert!(result.contains("Оборудование: 17"));
        assert!(result.contains("Канцелярия: 8"));
    }
    #[test]
    fn workspace_reorder_and_template_swap_keep_ids_draft_and_selection() {
        let mut app = app();
        let form = app.views.get_mut(&WindowKey::MAIN).unwrap();
        form.update(Size::new(1200.0, 820.0), 1.0).unwrap();
        let editor = form.find_name("WorkspaceEditor").unwrap();
        form.ui.set_text(editor, "Draft").unwrap();
        let tabs = form.find_name("WorkspaceTabs").unwrap();
        let selected = form.ui.selected_tab_id(tabs);
        crate::workspace_model::command(form, "WorkspaceReorder").unwrap();
        assert_eq!(form.ui.selected_tab_id(tabs), selected);
        let button = form.find_name("TemplateAction").unwrap();
        for _ in 0..3 {
            crate::workspace_model::command(form, "WorkspaceTemplate").unwrap();
            form.update(Size::new(800.0, 650.0), 1.0).unwrap();
        }
        assert_eq!(form.find_name("TemplateAction"), Some(button));
        assert_eq!(form.find_name("WorkspaceEditor"), Some(editor));
        assert_eq!(form.ui.document(editor).unwrap().text(), "Draft");
    }
}
