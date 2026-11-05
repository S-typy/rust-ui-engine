use super::*;
use platform::{
    ClipboardContent, ClipboardFormat, PlatformServiceCompletion, PlatformServiceHandle,
    PlatformServiceRequest, PlatformServiceValue, ServiceErrorKind, ServiceRequestId,
};
use rust_desktop_ui_controls::rich_editor::{RichClipboardTarget, RichEditorView};
use rust_desktop_ui_core::{InputEvent, Key};

enum Operation {
    Copy,
    Cut(RichClipboardTarget),
    Html(RichClipboardTarget),
    Text(RichClipboardTarget),
}
#[derive(Default)]
pub(super) struct ClipboardState {
    pub(super) handle: Option<PlatformServiceHandle>,
    pending: Option<(ServiceRequestId, Operation)>,
}
impl ClipboardState {
    pub(super) fn cancel(&mut self) {
        if let Some((id, _)) = self.pending.take()
            && let Some(handle) = &self.handle
        {
            handle.cancel(&id);
        }
    }
    fn submit(
        &mut self,
        request: PlatformServiceRequest,
        operation: Operation,
    ) -> Result<(), String> {
        let handle = self
            .handle
            .as_ref()
            .ok_or("Служба буфера обмена недоступна")?;
        let id = handle.submit(request).map_err(err)?;
        self.pending = Some((id, operation));
        Ok(())
    }
}
impl RichDocumentDemo {
    pub(super) fn clipboard_input(&mut self, input: &InputEvent) -> Result<bool, String> {
        // A new action or lost focus invalidates the old destination even when
        // the selection subsequently returns to its original position.
        if matches!(
            input,
            InputEvent::PointerDown { .. }
                | InputEvent::KeyDown { .. }
                | InputEvent::WindowFocused(false)
        ) {
            self.clipboard.cancel();
        }
        if self.editor.view() != RichEditorView::Document
            || self.form.ui.runtime.focused() != Some(self.editor.parts.editor)
        {
            return Ok(false);
        }
        let InputEvent::KeyDown {
            key,
            modifiers: m,
            repeat,
        } = input
        else {
            return Ok(false);
        };
        if m.alt {
            return Ok(false);
        }
        let command = match key {
            Key::Character(c) if m.control || m.super_key => {
                match c.to_ascii_lowercase().as_str() {
                    "c" => "copy",
                    "x" => "cut",
                    "v" if m.shift => "text",
                    "v" => "html",
                    _ => return Ok(false),
                }
            }
            Key::Insert if m.control && !m.shift => "copy",
            Key::Insert if m.shift && !m.control => "html",
            Key::Delete if m.shift && !m.control && !m.super_key => "cut",
            _ => return Ok(false),
        };
        if *repeat {
            return Ok(true);
        }
        let target = self
            .editor
            .clipboard_target(&mut self.form.ui)
            .map_err(err)?;
        match command {
            "copy" | "cut" => {
                if command == "cut"
                    && self
                        .form
                        .ui
                        .editor_settings(self.editor.parts.editor)
                        .is_none_or(|s| s.is_read_only)
                {
                    return Ok(true);
                }
                let fragment = self
                    .editor
                    .copy_fragment(&self.form.ui, &target)
                    .map_err(err)?;
                let text = fragment.plain_text();
                if text.is_empty() {
                    return Ok(true);
                }
                let html = fragment.to_html().map_err(err)?;
                self.clipboard.submit(
                    PlatformServiceRequest::WriteClipboard(ClipboardContent {
                        text: Some(text),
                        html: Some(html),
                        image: None,
                    }),
                    if command == "cut" {
                        Operation::Cut(target)
                    } else {
                        Operation::Copy
                    },
                )?;
            }
            "text" => self.clipboard.submit(
                PlatformServiceRequest::ReadClipboard(ClipboardFormat::Text),
                Operation::Text(target),
            )?,
            _ => self.clipboard.submit(
                PlatformServiceRequest::ReadClipboard(ClipboardFormat::Html),
                Operation::Html(target),
            )?,
        }
        Ok(true)
    }
    pub(super) fn clipboard_completed(
        &mut self,
        completion: PlatformServiceCompletion,
    ) -> Result<(), String> {
        if self
            .clipboard
            .pending
            .as_ref()
            .is_none_or(|(id, _)| *id != completion.id)
        {
            return Ok(());
        }
        let (_, operation) = self.clipboard.pending.take().unwrap();
        if self.form.ui.runtime.focused() != Some(self.editor.parts.editor) {
            return Ok(());
        }
        // Only absence of the OS HTML format falls back to Unicode text. A
        // malformed/unsupported HTML payload never silently loses its style.
        if matches!(completion.result, Err(ref e) if e.kind == ServiceErrorKind::Unsupported)
            && let Operation::Html(target) = operation
        {
            return self.clipboard.submit(
                PlatformServiceRequest::ReadClipboard(ClipboardFormat::Text),
                Operation::Text(target),
            );
        }
        let value = completion.result.map_err(err)?;
        match (operation, value) {
            (Operation::Copy, PlatformServiceValue::Done) => {
                self.result("Фрагмент скопирован с оформлением")?
            }
            (Operation::Cut(target), PlatformServiceValue::Done) => {
                self.editor
                    .complete_cut(&mut self.form.ui, target)
                    .map_err(err)?;
                self.result("Фрагмент вырезан")?;
            }
            (Operation::Html(target), PlatformServiceValue::Clipboard(content)) => {
                self.editor
                    .paste_html(
                        &mut self.form.ui,
                        target,
                        content.html.as_deref().ok_or("HTML в ответе отсутствует")?,
                    )
                    .map_err(|e| {
                        format!("Вставка не выполнена: {e}. Ctrl+Shift+V — вставить обычный текст")
                    })?;
                self.result("Фрагмент вставлен с оформлением")?;
            }
            (Operation::Text(target), PlatformServiceValue::Clipboard(content)) => {
                self.editor
                    .paste_text(
                        &mut self.form.ui,
                        target,
                        content
                            .text
                            .as_deref()
                            .ok_or("Текст в ответе отсутствует")?,
                    )
                    .map_err(err)?;
                self.result("Текст вставлен")?;
            }
            _ => return Err("Неожиданный ответ службы буфера обмена".into()),
        }
        Ok(())
    }
}
