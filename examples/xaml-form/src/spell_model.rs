//! Application-owned worker and explicitly supplied demonstration dictionary.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    Controls, LexiconMetadata, LexiconProvider, SpellCompletion, SpellError, SpellIssue,
    SpellProvider, SpellRequest,
};
use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation, WidgetId};
use std::{
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
const INITIAL: &str = "Привт, мир! Это пример текста.";
fn lexicon() -> Result<LexiconProvider, String> {
    LexiconProvider::new(
        LexiconMetadata {
            id: "example-russian".into(),
            language: "ru-RU".into(),
            source: "Handwritten project example words".into(),
            license: "Apache-2.0".into(),
        },
        ["привет", "мир", "это", "пример", "текста"].map(String::from),
    )
    .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("SpellCheck", "Проверить"),
        ("SpellApply", "Исправить первое"),
        ("SpellUndo", "Отменить исправление"),
        ("SpellAccept", "Принять текст"),
        ("SpellCancel", "Отменить проверку"),
        ("SpellIgnoreOnce", "Пропустить один раз"),
        ("SpellIgnoreAll", "Игнорировать слово"),
        ("SpellCustom", "В свой словарь"),
        ("SpellStale", "Поздний ответ"),
        ("SpellReset", "Сбросить пример"),
        ("SpellMemoFix", "Исправить в заметке"),
        ("SpellReadOnly", "Только чтение"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
type Completion = Result<Vec<SpellIssue>, SpellError>;
struct Pending {
    request: SpellRequest,
    receiver: Receiver<Completion>,
    poll_at: Instant,
}
pub(crate) struct State {
    editor: WidgetId,
    memo: WidgetId,
    status: WidgetId,
    issues: WidgetId,
    accepted: WidgetId,
    pending: Option<Pending>,
    message: String,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(editor) = view.find_name("SpellEditor") else {
            return Ok(None);
        };
        let get = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let memo = get("SpellMemo")?;
        ui.set_spellcheck(editor, Some(Default::default()))
            .map_err(err)?;
        ui.set_spellcheck(memo, Some(Default::default()))
            .map_err(err)?;
        let mut state = Self {
            editor,
            memo,
            status: get("SpellStatus")?,
            issues: get("SpellIssues")?,
            accepted: get("SpellAccepted")?,
            pending: None,
            message: "Готово".into(),
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    fn start(&mut self, ui: &mut Controls, stale: bool) -> Result<(), String> {
        let request = ui.request_spellcheck(self.editor).map_err(err)?;
        let work = request.clone();
        let provider = lexicon()?;
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            // The demonstration owns this delay and worker; the library owns neither.
            std::thread::sleep(Duration::from_millis(60));
            let _ = sender.send(provider.check(&work));
        });
        self.pending = Some(Pending {
            request,
            receiver,
            poll_at: Instant::now() + Duration::from_millis(16),
        });
        self.message = "Проверка выполняется".into();
        if stale {
            let text = format!("{}!", ui.document(self.editor).unwrap().text());
            ui.set_text(self.editor, &text).map_err(err)?;
        }
        Ok(())
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        if !matches!(
            command,
            "SpellCheck"
                | "SpellStale"
                | "SpellApply"
                | "SpellAccept"
                | "SpellCancel"
                | "SpellUndo"
                | "SpellIgnoreOnce"
                | "SpellIgnoreAll"
                | "SpellCustom"
                | "SpellReset"
                | "SpellReadOnly"
                | "SpellMemoFix"
        ) {
            return Ok(false);
        }
        let result = (|| -> Result<(), String> {
            match command {
                "SpellCheck" => self.start(ui, false),
                "SpellStale" => self.start(ui, true),
                "SpellApply" => ui
                    .apply_spelling_suggestion(self.editor, 0, 0)
                    .map_err(err)
                    .map(|()| self.message = "Исправлен черновик; доступна отмена".into()),
                "SpellAccept" => ui
                    .commit_edit(self.editor)
                    .map_err(err)
                    .map(|_| self.message = "Текст принят".into()),
                "SpellCancel" => {
                    self.pending = None;
                    ui.cancel_spellcheck(self.editor)
                        .map_err(err)
                        .map(|()| self.message = "Проверка отменена".into())
                }
                "SpellUndo" => {
                    ui.runtime
                        .apply(Mutation::Focus(Some(self.editor)))
                        .map_err(err)?;
                    ui.dispatch(InputEvent::KeyDown {
                        key: Key::Character("z".into()),
                        modifiers: Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        repeat: false,
                    })
                    .map_err(err)?;
                    self.message = "Операция отменена".into();
                    Ok(())
                }
                "SpellIgnoreOnce" => ui
                    .ignore_spelling_once(self.editor, 0)
                    .map_err(err)
                    .map(|()| self.message = "Диапазон пропущен в текущей версии".into()),
                "SpellIgnoreAll" | "SpellCustom" => {
                    let word = ui
                        .spellcheck_issues(self.editor)
                        .and_then(|v| v.first())
                        .map(|i| i.word.clone())
                        .ok_or("Сначала выполните проверку")?;
                    if command == "SpellCustom" {
                        ui.add_spelling_word(self.editor, &word)
                    } else {
                        ui.ignore_spelling_all(self.editor, &word)
                    }
                    .map_err(err)?;
                    self.start(ui, false)
                }
                "SpellReset" => {
                    self.pending = None;
                    ui.set_read_only(self.editor, false).map_err(err)?;
                    ui.set_editor_value(self.editor, INITIAL).map_err(err)?;
                    ui.set_spellcheck(self.editor, None).map_err(err)?;
                    ui.set_spellcheck(self.editor, Some(Default::default()))
                        .map_err(err)?;
                    self.message = "Пример сброшен".into();
                    Ok(())
                }
                "SpellReadOnly" => {
                    let value = ui.is_read_only(self.editor) == Some(false);
                    ui.set_read_only(self.editor, value).map_err(err)?;
                    self.message = format!("Только чтение: {value}");
                    Ok(())
                }
                "SpellMemoFix" => {
                    let parts = ui.memo_parts(self.memo).ok_or("Memo removed")?;
                    ui.open_memo(parts).map_err(err)?;
                    let request = ui.request_spellcheck(self.memo).map_err(err)?;
                    ui.complete_spellcheck(self.memo, &request, lexicon()?.check(&request))
                        .map_err(err)?;
                    ui.apply_spelling_suggestion(self.memo, 0, 0).map_err(err)?;
                    self.message =
                        "Заметка исправлена как черновик; Apply/Cancel остаются доступны".into();
                    Ok(())
                }
                _ => unreachable!(),
            }
        })();
        if let Err(message) = result {
            self.message = message;
        }
        self.refresh(ui)?;
        Ok(true)
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|p| p.poll_at)
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        if let Some(pending) = &mut self.pending {
            match pending.receiver.try_recv() {
                Ok(result) => {
                    self.message = match ui
                        .complete_spellcheck(self.editor, &pending.request, result)
                        .map_err(err)?
                    {
                        SpellCompletion::Accepted => ui
                            .spellcheck_error(self.editor)
                            .unwrap_or("Проверка завершена")
                            .into(),
                        SpellCompletion::Stale => "Поздний ответ отброшен; текст сохранён".into(),
                    };
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.message = "Worker завершился без ответа".into();
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    pending.poll_at = Instant::now() + Duration::from_millis(16);
                }
            }
        }
        let issues = ui
            .spellcheck_issues(self.editor)
            .map(|v| {
                if v.is_empty() {
                    "Ошибок не найдено в словаре примера".to_owned()
                } else {
                    v.iter()
                        .take(4)
                        .map(|i| {
                            format!(
                                "UTF-8 {}..{} · {} → {}",
                                i.range.start,
                                i.range.end,
                                i.word,
                                i.suggestions.join(", ")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            })
            .unwrap_or_else(|| "Нет актуальных диапазонов".into());
        let accepted = format!(
            "Бизнес-значение: {}",
            ui.accepted_value(self.editor).unwrap_or("")
        );
        for (id, text) in [
            (self.status, self.message.clone()),
            (self.issues, issues),
            (self.accepted, accepted),
        ] {
            let safe = if ui.measure_text(&text, 4096.0, false).is_ok() {
                text
            } else {
                "Текст недоступен для отображения".into()
            };
            ui.set_label(id, safe).map_err(err)?;
        }
        Ok(())
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Size;
    use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent};
    fn invoke(form: &mut crate::form::Form, name: &str) {
        let id = form.view.find_name(name).unwrap();
        form.ui.activate(id).unwrap();
        form.event(PlatformEvent::Tick).unwrap();
    }
    #[test]
    fn spell_demo_real_form_worker_deadline_draft_undo_and_stale_completion() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../SpellWindow.xaml")).unwrap();
        let mut form = crate::form::Form::new_with_model(definition, false, None).unwrap();
        form.update(Size::new(1040.0, 720.0), 1.0).unwrap();
        let editor = form.view.find_name("SpellEditor").unwrap();
        for name in ["SpellEditor", "SpellCheck", "SpellApply", "SpellStatus"] {
            let id = form.view.find_name(name).unwrap();
            let bounds = form.ui.runtime.bounds(id).unwrap();
            assert!(bounds.width > 0.0 && bounds.height > 0.0);
            assert_eq!(
                bounds.intersection(form.ui.runtime.clip(id).unwrap()),
                Some(bounds)
            );
        }
        invoke(&mut form, "SpellCheck");
        assert!(form.wake_after().is_some());
        let until = Instant::now() + Duration::from_secs(3);
        while form.ui.spellcheck_issues(editor).is_none() {
            assert!(Instant::now() < until);
            std::thread::sleep(
                form.wake_after().unwrap_or(Duration::from_millis(16)) + Duration::from_millis(1),
            );
            let response = form.event(PlatformEvent::Tick).unwrap();
            if form.ui.spellcheck_issues(editor).is_some() {
                assert!(
                    response.redraw,
                    "worker reply must redraw without further input"
                );
            }
        }
        assert_eq!(form.ui.spellcheck_issues(editor).unwrap().len(), 1);
        invoke(&mut form, "SpellApply");
        assert_eq!(
            form.ui.document(editor).unwrap().text(),
            "привет, мир! Это пример текста."
        );
        assert_eq!(form.ui.accepted_value(editor), Some(INITIAL));
        invoke(&mut form, "SpellUndo");
        assert_eq!(form.ui.document(editor).unwrap().text(), INITIAL);
        invoke(&mut form, "SpellStale");
        let changed = form.ui.document(editor).unwrap().text().to_owned();
        let status = form.view.find_name("SpellStatus").unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            std::thread::sleep(Duration::from_millis(20));
            form.event(PlatformEvent::Tick).unwrap();
            let label = form
                .ui
                .semantics()
                .into_iter()
                .find(|n| n.id == status)
                .unwrap()
                .label;
            if label.contains("Поздний ответ отброшен") {
                break;
            }
            assert!(Instant::now() < until, "{label}");
        }
        assert_eq!(form.ui.document(editor).unwrap().text(), changed);
        assert_eq!(form.ui.accepted_value(editor), Some(INITIAL));
        invoke(&mut form, "SpellMemoFix");
        let memo = form
            .ui
            .memo_parts(form.view.find_name("SpellMemo").unwrap())
            .unwrap();
        assert!(form.ui.memo_is_open(memo));
        assert_eq!(
            form.ui.document(memo.editor).unwrap().text(),
            "привет, мир!"
        );
        assert_eq!(form.ui.accepted_value(memo.editor), Some("Привт, мир!"));
        form.ui.cancel_memo(memo).unwrap();
        invoke(&mut form, "SpellReadOnly");
        invoke(&mut form, "SpellCheck");
        assert_eq!(form.ui.document(editor).unwrap().text(), changed);
    }

    #[test]
    fn spell_demo_commands_draw_captions_and_remain_reachable_after_resize() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../SpellWindow.xaml")).unwrap();
        for theme in [
            rust_desktop_ui_controls::Theme::FluentLight,
            rust_desktop_ui_controls::Theme::FluentDark,
        ] {
            let mut form =
                crate::form::Form::new_with_model(definition.clone(), false, None).unwrap();
            form.ui.set_theme(theme).unwrap();
            for width in [1040.0, 760.0] {
                let size = Size::new(width, 720.0);
                form.update(size, 1.0).unwrap();
                for (name, caption) in [
                    ("SpellCheck", "Проверить"),
                    ("SpellApply", "Исправить первое"),
                    ("SpellUndo", "Отменить исправление"),
                    ("SpellAccept", "Принять текст"),
                    ("SpellCancel", "Отменить проверку"),
                    ("SpellIgnoreOnce", "Пропустить один раз"),
                    ("SpellIgnoreAll", "Игнорировать слово"),
                    ("SpellCustom", "В свой словарь"),
                    ("SpellStale", "Поздний ответ"),
                    ("SpellReset", "Сбросить пример"),
                    ("SpellMemoFix", "Исправить в заметке"),
                    ("SpellReadOnly", "Только чтение"),
                    ("SpellLight", "Светлая"),
                    ("SpellDark", "Тёмная"),
                ] {
                    let id = form.view.find_name(name).unwrap();
                    form.ui.bring_into_view(id).unwrap();
                    form.ui.update(size).unwrap();
                    let button = form.ui.runtime.bounds(id).unwrap();
                    let run = form
                        .ui
                        .scene()
                        .texts
                        .iter()
                        .find(|r| r.text == caption && r.bounds.intersection(button).is_some())
                        .unwrap_or_else(|| panic!("Missing drawn caption {name} at {width}"));
                    let visible = run.bounds.intersection(run.clip).unwrap();
                    assert!(visible.width > 0.0 && visible.height > 0.0);
                    assert!(
                        visible.width + 0.01 >= run.bounds.width
                            && visible.height + 0.01 >= run.bounds.height,
                        "Clipped {name} at {width}"
                    );
                }
            }
        }
    }
}
