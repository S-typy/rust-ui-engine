//! Property editors that retain text drafts or an item key separately from its caption.
use super::*;
#[path = "text_editors/choices.rs"]
mod choices;
use choices::{Choices, Tokens, token_keys};
use regex_lite::Regex;
use rust_desktop_ui_data::FieldType;

pub(super) fn supports(kind: &str) -> bool {
    matches!(
        kind,
        "NumericText"
            | "MaskedText"
            | "Password"
            | "ButtonText"
            | "Memo"
            | "AutoComplete"
            | "AutoSuggest"
            | "TokenList"
            | "Lookup"
            | "SearchLookup"
            | "ListBox"
            | "Hyperlink"
    )
}

fn choice_editor(kind: &str) -> bool {
    matches!(
        kind,
        "AutoComplete" | "AutoSuggest" | "TokenList" | "Lookup" | "SearchLookup" | "ListBox"
    )
}

fn layout() -> LayoutStyle {
    LayoutStyle {
        height: Length::Px(28.),
        ..Default::default()
    }
}

fn raw_text(value: &CellValue) -> Result<&str, ControlError> {
    match value {
        CellValue::Text(v) => Ok(v),
        CellValue::Null => Ok(""),
        _ => Err(invalid("This editor requires text")),
    }
}

fn accepted(outcome: CommitOutcome) -> Result<(), ControlError> {
    match outcome {
        CommitOutcome::Committed | CommitOutcome::Unchanged => Ok(()),
        CommitOutcome::Invalid(reason) => Err(invalid(reason)),
        _ => Err(invalid("The property editor cannot load the new value yet")),
    }
}

enum TextInput {
    Digits { integer: bool, width: usize },
    Slots(MaskSettings),
    Pattern(Regex),
    Password,
    Button,
    Memo,
}

impl TextInput {
    fn settings(d: &PropertyDescriptor, f: &FieldSchema) -> Result<Self, ControlError> {
        let kind = d.metadata.editor.as_str();
        if f.kind != FieldType::Text && !(kind == "NumericText" && f.kind == FieldType::Integer) {
            return Err(invalid("Incompatible field type for text editor"));
        }
        Ok(match kind {
            "NumericText" => {
                let width = match d.mask.as_deref() {
                    None | Some("d" | "D") => 0,
                    Some(mask) if f.kind == FieldType::Integer => mask
                        .strip_prefix(['d', 'D'])
                        .and_then(|s| s.parse::<usize>().ok())
                        .filter(|n| (1..=19).contains(n))
                        .ok_or_else(|| invalid("Expected numeric mask D or D1..D19"))?,
                    _ => {
                        return Err(invalid(
                            "Digit strings accept the D mask; padding requires an integer field",
                        ));
                    }
                };
                Self::Digits {
                    integer: f.kind == FieldType::Integer,
                    width,
                }
            }
            "MaskedText" => {
                let mask = d
                    .mask
                    .as_deref()
                    .filter(|s| !s.is_empty() && s.len() <= 4096)
                    .ok_or_else(|| invalid("MaskedText requires a mask of 1..4096 UTF-8 bytes"))?;
                if let Some(expression) = mask
                    .strip_prefix("regex:")
                    .or_else(|| (mask.starts_with(r"\d") || mask.starts_with('^')).then_some(mask))
                {
                    if expression.is_empty() {
                        return Err(invalid("The regular expression is empty"));
                    }
                    let expr = format!(r"\A(?:{expression})\z");
                    Self::Pattern(
                        Regex::new(&expr)
                            .map_err(|e| invalid(format!("Invalid property mask: {e}")))?,
                    )
                } else {
                    Self::Slots(MaskSettings::new(mask)?)
                }
            }
            "Password" => Self::Password,
            "ButtonText" => Self::Button,
            "Memo" => Self::Memo,
            _ => return Err(invalid("Unknown text editor")),
        })
    }

    fn check(&self, value: &str) -> Result<(), String> {
        match self {
            Self::Digits { integer, .. } => {
                let digits = if *integer {
                    value.strip_prefix('-').unwrap_or(value)
                } else {
                    value
                };
                if !digits.is_empty()
                    && digits.bytes().all(|b| b.is_ascii_digit())
                    && (!integer || value.parse::<i64>().is_ok())
                {
                    Ok(())
                } else {
                    Err("Введите целое число допустимой длины.".into())
                }
            }
            Self::Slots(settings) => settings.validate_storage(value).map_err(|e| e.to_string()),
            Self::Pattern(expression) if !expression.is_match(value) => {
                Err("Значение не соответствует маске.".into())
            }
            Self::Password
                if value
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')) =>
            {
                Err("Пароль должен занимать одну строку.".into())
            }
            Self::Memo => Ok(()),
            Self::Button | Self::Pattern(_) if value.contains(['\r', '\n']) => {
                Err("Введите однострочное значение.".into())
            }
            _ => Ok(()),
        }
    }
    fn source_text(&self, v: &CellValue) -> Result<String, ControlError> {
        if let (
            Self::Digits {
                integer: true,
                width,
            },
            CellValue::Integer(n),
        ) = (self, v)
        {
            let sign = if *n < 0 { "-" } else { "" };
            return Ok(format!(
                "{sign}{:0width$}",
                n.unsigned_abs(),
                width = *width
            ));
        }
        Ok(raw_text(v)?.into())
    }
}

fn as_key(value: &CellValue) -> Result<Option<String>, ControlError> {
    match value {
        CellValue::Null => Ok(None),
        CellValue::Text(s) => Ok(Some(s.clone())),
        CellValue::Integer(n) => Ok(Some(n.to_string())),
        _ => Err(invalid("Choice keys must be text or integer")),
    }
}

fn keys(d: &PropertyDescriptor) -> &[String] {
    if d.choice_keys.is_empty() {
        &d.choices
    } else {
        &d.choice_keys
    }
}

fn valid_url(target: &str) -> bool {
    let Some((scheme, rest)) = target.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        && !authority.is_empty()
        && !authority.contains('@')
        && !target.contains('\\')
        && !target.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Keep a fixed HTTP origin while interpolating one caption into the path, query or fragment.
fn link_target(template: Option<&str>, caption: &str) -> Result<String, ControlError> {
    let Some(template) = template else {
        return valid_url(caption)
            .then(|| caption.into())
            .ok_or_else(|| invalid("Введите ссылку HTTP или HTTPS."));
    };
    let (head, tail) = template
        .split_once("{0}")
        .ok_or_else(|| invalid("UriFormat requires one {0} placeholder"))?;
    let beyond_authority = head
        .split_once("://")
        .is_some_and(|(_, s)| s.contains(['/', '?', '#']));
    if template.len() > 4096
        || !beyond_authority
        || head.contains(['{', '}'])
        || tail.contains(['{', '}'])
        || !valid_url(template)
        || caption.len() > 8192
        || caption.chars().any(char::is_control)
    {
        return Err(invalid(
            "UriFormat needs a fixed HTTP(S) origin and one {0} in its path, query or fragment",
        ));
    }
    let mut target = String::from(head);
    for &byte in caption.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            target.push(char::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            target.push('%');
            target.push(char::from(HEX[usize::from(byte >> 4)]));
            target.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    target.push_str(tail);
    Ok(target)
}

pub(super) fn validate(
    d: &PropertyDescriptor,
    f: &FieldSchema,
    value: &CellValue,
) -> Result<(), ControlError> {
    f.validate(value).map_err(|e| invalid(e.to_string()))?;
    let kind = d.metadata.editor.as_str();
    if (!matches!(kind, "MaskedText" | "NumericText") && d.mask.is_some())
        || (!choice_editor(kind) && (!d.choices.is_empty() || !d.choice_keys.is_empty()))
    {
        return Err(invalid("Incompatible mask or choices for this editor"));
    }
    if choice_editor(kind) {
        let keys = keys(d);
        if !matches!(f.kind, FieldType::Text | FieldType::Integer)
            || d.choices.is_empty()
            || keys.len() != d.choices.len()
            || keys
                .iter()
                .any(|k| k.is_empty() || k.len() > 256 || k.chars().any(char::is_control))
            || keys.iter().collect::<HashSet<_>>().len() != keys.len()
        {
            return Err(invalid(
                "Choices require matching unique keys and a text or integer field",
            ));
        }
        if f.kind == FieldType::Integer
            && keys
                .iter()
                .any(|k| k.parse::<i64>().map(|n| n.to_string()).ok().as_ref() != Some(k))
        {
            return Err(invalid(
                "Integer choice keys must be canonical decimal integers",
            ));
        }
        if kind == "TokenList" {
            if f.kind != FieldType::Text {
                return Err(invalid(
                    "TokenList requires a text field containing an array of keys",
                ));
            }
            token_keys(value, keys)?;
        } else if kind == "AutoSuggest" {
            if f.kind != FieldType::Text
                || !d.choice_keys.is_empty()
                || raw_text(value)?.contains(['\r', '\n'])
            {
                return Err(invalid(
                    "AutoSuggest requires single-line text and suggestions without separate keys",
                ));
            }
        } else if let Some(key) = as_key(value)?
            && !keys.contains(&key)
        {
            return Err(invalid("The property value is absent from the choices"));
        }
    } else if kind == "Hyperlink" {
        if f.kind != FieldType::Text {
            return Err(invalid("A hyperlink requires a text field"));
        }
        if d.uri_format.is_some() || !matches!(value, CellValue::Null) {
            link_target(d.uri_format.as_deref(), raw_text(value)?)?;
        }
    } else {
        let settings = TextInput::settings(d, f)?;
        if !matches!(value, CellValue::Null) {
            match value {
                CellValue::Integer(n) => settings.check(&n.to_string()),
                _ => settings.check(raw_text(value)?),
            }
            .map_err(invalid)?;
        }
    }
    Ok(())
}

pub(super) fn mount(
    ui: &mut Controls,
    host: WidgetId,
    d: &PropertyDescriptor,
    f: &FieldSchema,
    value: &CellValue,
) -> Result<Box<dyn InspectorEditor>, ControlError> {
    validate(d, f, value)?;
    let kind = d.metadata.editor.as_str();
    let label = d.metadata.label.as_deref().unwrap_or(&f.title);
    if kind == "TokenList" {
        return Tokens::mount(ui, host, label, d, value);
    }
    if choice_editor(kind) {
        return Choices::mount(ui, host, label, d, f, value);
    }
    if kind == "Hyperlink" {
        return Link::mount(ui, host, label, value, d);
    }
    let settings = TextInput::settings(d, f)?;
    let text = settings.source_text(value)?;
    let (owner, focus) = match &settings {
        TextInput::Slots(mask) => {
            let id = ui.masked_edit(host, label, &text, mask.clone(), layout())?;
            (id, id)
        }
        TextInput::Password => {
            let id = ui.password_box(
                host,
                label,
                PasswordBoxSettings {
                    max_length: super::super::password::MAX_PASSWORD_GRAPHEMES,
                    allow_reveal: false,
                    is_read_only: d.metadata.read_only,
                    ..Default::default()
                },
                layout(),
            )?;
            ui.set_password_value(id, &text)?;
            (id, id)
        }
        TextInput::Button => {
            let parts = ui.button_edit(
                host,
                label,
                &text,
                vec![EditorButton {
                    label: "Очистить".into(),
                    icon: Some(IconSymbol::Close),
                    action: EditorButtonAction::Clear,
                }],
                layout(),
            )?;
            ui.set_composite_read_only(parts.container, d.metadata.read_only)?;
            (parts.container, parts.editor)
        }
        TextInput::Memo => {
            let parts = ui.memo_edit(host, label, &text, MemoEditSettings::default(), layout())?;
            ui.set_button_chrome(parts.container, ButtonChrome::Flat)?;
            ui.controls
                .get_mut(&parts.container)
                .unwrap()
                .inspector_part = Some(InspectorPart::Value(ValueMark::Plain));
            (parts.container, parts.editor)
        }
        _ => {
            let id = ui.edit_box(host, label, &text, layout())?;
            let validation = TextInput::settings(d, f)?;
            let nullable = f.nullable;
            ui.set_editor_validator(
                id,
                Some(crate::EditorValidator::new(move |v| {
                    if nullable && v.is_empty() {
                        Ok(())
                    } else {
                        validation.check(v)
                    }
                })),
            )?;
            (id, id)
        }
    };
    if !matches!(settings, TextInput::Password) {
        ui.set_read_only(focus, d.metadata.read_only)?;
    }
    Ok(Box::new(Text {
        owner,
        focus,
        settings,
    }))
}

struct Text {
    owner: WidgetId,
    focus: WidgetId,
    settings: TextInput,
}

impl InspectorEditor for Text {
    fn owner(&self) -> WidgetId {
        self.owner
    }
    fn focus(&self) -> WidgetId {
        self.focus
    }
    fn inline(&self) -> bool {
        matches!(self.settings, TextInput::Memo)
    }
    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        let text = self.settings.source_text(value)?;
        match &self.settings {
            TextInput::Password => ui.set_password_value(self.focus, &text),
            TextInput::Slots(_) => accepted(ui.set_masked_value(self.focus, &text)?),
            _ => accepted(ui.set_editor_value(self.focus, &text)?),
        }
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        if matches!(self.settings, TextInput::Password) {
            return ui
                .password_value(self.focus)
                .map(|v| CellValue::Text(v.expose_secret().into()))
                .ok_or_else(|| "Password editor is unavailable".into());
        }
        let draft = ui
            .document(self.focus)
            .ok_or("Text editor is unavailable")?
            .text();
        if let TextInput::Slots(settings) = &self.settings {
            let parsed = settings.parse_display(draft)?;
            return if parsed.complete {
                Ok(CellValue::Text(parsed.storage))
            } else {
                Err("Заполните обязательные позиции маски.".into())
            };
        }
        self.settings.check(draft)?;
        if matches!(self.settings, TextInput::Digits { integer: true, .. }) {
            return draft
                .parse::<i64>()
                .map(CellValue::Integer)
                .map_err(|_| "Некорректное целое число.".into());
        }
        Ok(CellValue::Text(draft.into()))
    }
    fn rest_label(&self, value: &CellValue) -> Option<String> {
        if matches!(self.settings, TextInput::Digits { integer: true, .. })
            && matches!(value, CellValue::Integer(_))
        {
            return self.settings.source_text(value).ok();
        }
        let CellValue::Text(text) = value else {
            return None;
        };
        match &self.settings {
            TextInput::Password => Some(if text.is_empty() {
                String::new()
            } else {
                "••••••".into()
            }),
            TextInput::Slots(settings) => settings.format(text).ok(),
            TextInput::Memo => Some(text.lines().next().unwrap_or_default().into()),
            _ => None,
        }
    }
}

struct Link {
    owner: WidgetId,
    address: WidgetId,
    button: WidgetId,
    request: Option<String>,
    template: Option<String>,
}

impl Link {
    fn mount(
        ui: &mut Controls,
        host: WidgetId,
        label: &str,
        v: &CellValue,
        d: &PropertyDescriptor,
    ) -> Result<Box<dyn InspectorEditor>, ControlError> {
        let target = raw_text(v)?;
        let owner = ui.transparent_panel(
            host,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..Default::default()
            },
        )?;
        let line = ui.transparent_panel(
            owner,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(28.),
                ..Default::default()
            },
        )?;
        let button = ui.button(line, target, None, layout())?;
        ui.set_button_chrome(button, ButtonChrome::Link)?;
        Self::size_caption(ui, button, target)?;
        let address = ui.edit_box(owner, label, target, layout())?;
        ui.set_read_only(address, d.metadata.read_only)?;
        let template = d.uri_format.clone();
        ui.set_editor_validator(
            address,
            Some(crate::EditorValidator::new(move |v| {
                if v.is_empty() {
                    Ok(())
                } else {
                    link_target(template.as_deref(), v)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                }
            })),
        )?;
        show(ui, address, false)?;
        Ok(Box::new(Self {
            owner,
            button,
            address,
            request: None,
            template: d.uri_format.clone(),
        }))
    }
    fn size_caption(
        ui: &mut Controls,
        button: WidgetId,
        caption: &str,
    ) -> Result<(), ControlError> {
        let width = ui
            .measure_button_content(
                caption,
                &ButtonStyle::default(),
                None,
                ButtonContentLayout::Leading,
                16_384.,
            )?
            .width
            .ceil()
            + 2.;
        ui.edit_props(button, |p| {
            p.style.width = Length::Px(width);
            p.style.flex_shrink = 1.;
            p.style.min_size.width = 0.;
        })
    }
}
impl InspectorEditor for Link {
    fn owner(&self) -> WidgetId {
        self.owner
    }
    fn focus(&self) -> WidgetId {
        self.address
    }
    fn inline(&self) -> bool {
        true
    }
    fn load(&self, ui: &mut Controls, v: &CellValue) -> Result<(), ControlError> {
        let target = raw_text(v)?;
        if !matches!(v, CellValue::Null) {
            link_target(self.template.as_deref(), target)?;
        }
        accepted(ui.set_editor_value(self.address, target)?)?;
        ui.set_label(self.button, target)?;
        Self::size_caption(ui, self.button, target)?;
        ui.set_automation_name(self.button, format!("Открыть {target}"))
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let target = ui
            .document(self.address)
            .ok_or("Link editor is unavailable")?
            .text();
        link_target(self.template.as_deref(), target).map_err(|e| e.to_string())?;
        Ok(CellValue::Text(target.into()))
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        show(ui, self.address, active)
    }
    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let mut requested = false;
        ui.activated.retain(|id| {
            if *id == self.button {
                requested = true;
                false
            } else {
                true
            }
        });
        if requested {
            // Navigate to the caption actually activated. A pending edit is a separate transaction.
            let caption = &ui.controls[&self.button].label;
            self.request = link_target(self.template.as_deref(), caption).ok();
        }
        Ok(())
    }
    fn take_navigation(&mut self) -> Option<String> {
        self.request.take()
    }
}

#[cfg(test)]
#[path = "text_editors/tests.rs"]
mod tests;
