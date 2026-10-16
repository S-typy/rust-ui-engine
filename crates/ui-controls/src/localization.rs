//! Explicit UI cultures and bounded translation resources. Storage formats are independent.
use crate::{ControlError, Controls, DateValue, TimeValue};
use rust_desktop_ui_core::WidgetId;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_LOCALIZATION_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiCulture {
    #[default]
    Russian,
    English,
    Arabic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadingDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl UiCulture {
    pub fn parse(tag: &str) -> Result<Self, ControlError> {
        match tag {
            "ru" | "ru-RU" => Ok(Self::Russian),
            "en" | "en-US" => Ok(Self::English),
            "ar" | "ar-SA" => Ok(Self::Arabic),
            _ => Err(invalid("Unsupported UI culture")),
        }
    }
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Russian => "ru-RU",
            Self::English => "en-US",
            Self::Arabic => "ar-SA",
        }
    }
    /// Direction is presentation metadata; controls opt into mirroring separately.
    pub const fn direction(self) -> ReadingDirection {
        match self {
            Self::Arabic => ReadingDirection::RightToLeft,
            _ => ReadingDirection::LeftToRight,
        }
    }
    /// Cardinal rules for an exact non-negative integer count; fractional counts use Other.
    pub fn plural(self, count: f64) -> PluralCategory {
        use PluralCategory::*;
        if !count.is_finite()
            || count < 0.0
            || count.fract() != 0.0
            || count > 9_007_199_254_740_991.0
        {
            return Other;
        }
        let n = count as u64;
        match self {
            Self::English => {
                if n == 1 {
                    One
                } else {
                    Other
                }
            }
            Self::Russian => {
                if n % 10 == 1 && n % 100 != 11 {
                    One
                } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
                    Few
                } else {
                    Many
                }
            }
            Self::Arabic => match n {
                0 => Zero,
                1 => One,
                2 => Two,
                _ if (3..=10).contains(&(n % 100)) => Few,
                _ if (11..=99).contains(&(n % 100)) => Many,
                _ => Other,
            },
        }
    }
    /// Fixed decimal presentation, with optional three-digit grouping. Arabic uses Latin digits.
    pub fn format_number(
        self,
        value: f64,
        decimals: usize,
        grouping: bool,
    ) -> Result<String, ControlError> {
        if !value.is_finite() || decimals > 9 {
            return Err(invalid(
                "Number must be finite; decimal places must be in 0..=9",
            ));
        }
        let invariant = format!("{value:.decimals$}");
        let (whole, fraction) = invariant.split_once('.').unwrap_or((&invariant, ""));
        let (separator, decimal) = match self {
            Self::Russian => ('\u{a0}', ','),
            Self::English => (',', '.'),
            Self::Arabic => ('\u{66c}', '\u{66b}'),
        };
        let mut result = String::new();
        let digits = whole.strip_prefix('-').unwrap_or(whole);
        if whole.starts_with('-') {
            result.push('-');
        }
        for (index, ch) in digits.chars().enumerate() {
            if grouping && index > 0 && (digits.len() - index).is_multiple_of(3) {
                result.push(separator);
            }
            result.push(ch);
        }
        if decimals > 0 {
            result.push(decimal);
            result.push_str(fraction);
        }
        Ok(result)
    }
    pub fn parse_number(self, text: &str) -> Result<f64, ControlError> {
        if text.len() > 512 {
            return Err(invalid("Localized number exceeds 512 bytes"));
        }
        let (group, decimal) = match self {
            Self::Russian => ('\u{a0}', ','),
            Self::English => (',', '.'),
            Self::Arabic => ('\u{66c}', '\u{66b}'),
        };
        let text = text.trim();
        let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
        let parts = unsigned.split(decimal).collect::<Vec<_>>();
        if parts.len() > 2
            || parts[0].is_empty()
            || (parts.len() == 2
                && (parts[1].is_empty() || !parts[1].bytes().all(|b| b.is_ascii_digit())))
        {
            return Err(invalid("Invalid localized number"));
        }
        let groups = parts[0].split(group).collect::<Vec<_>>();
        if groups
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
            || (groups.len() > 1
                && (groups[0].len() > 3 || groups[1..].iter().any(|part| part.len() != 3)))
        {
            return Err(invalid("Invalid number grouping"));
        }
        let normalized = text.replace(group, "").replace(decimal, ".");
        normalized
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| invalid("Invalid finite localized number"))
    }
    pub fn format_date(self, date: DateValue) -> Result<String, ControlError> {
        date.validate()?;
        Ok(match self {
            Self::Russian => format!("{:02}.{:02}.{:04}", date.day, date.month, date.year),
            Self::English => format!("{:02}/{:02}/{:04}", date.month, date.day, date.year),
            Self::Arabic => format!("{:02}/{:02}/{:04}", date.day, date.month, date.year),
        })
    }
    pub fn parse_date(self, text: &str) -> Result<DateValue, ControlError> {
        let parts = text
            .split(if self == Self::Russian { '.' } else { '/' })
            .collect::<Vec<_>>();
        if parts.len() != 3
            || parts[0].len() != 2
            || parts[1].len() != 2
            || parts[2].len() != 4
            || parts.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(invalid("Invalid localized date"));
        }
        let first = parts[0].parse().map_err(|_| invalid("Invalid date"))?;
        let second = parts[1].parse().map_err(|_| invalid("Invalid date"))?;
        let (month, day) = if self == Self::English {
            (first, second)
        } else {
            (second, first)
        };
        DateValue::new(
            parts[2].parse().map_err(|_| invalid("Invalid date"))?,
            month,
            day,
        )
    }
    /// A 24-hour clock, independent of zone and invariant storage.
    pub fn format_time(self, time: TimeValue, seconds: bool) -> Result<String, ControlError> {
        time.validate()?;
        if !seconds && time.second != 0 {
            return Err(invalid("Hiding nonzero seconds would lose precision"));
        }
        Ok(if seconds {
            format!("{:02}:{:02}:{:02}", time.hour, time.minute, time.second)
        } else {
            format!("{:02}:{:02}", time.hour, time.minute)
        })
    }
    pub fn parse_time(self, text: &str) -> Result<TimeValue, ControlError> {
        TimeValue::parse(text)
    }
}

/// A message optionally has cardinal forms. Every plural message requires Other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalizedMessage {
    Text(String),
    Plural(BTreeMap<PluralCategory, String>),
}
impl From<&str> for LocalizedMessage {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

#[derive(Clone, Debug)]
pub struct Localizer {
    culture: UiCulture,
    fallback: UiCulture,
    revision: u64,
    resources: BTreeMap<(UiCulture, String, String), LocalizedMessage>,
}
impl Default for Localizer {
    fn default() -> Self {
        Self::new(UiCulture::Russian, UiCulture::English)
    }
}
impl Localizer {
    pub fn new(culture: UiCulture, fallback: UiCulture) -> Self {
        Self {
            culture,
            fallback,
            revision: NEXT_LOCALIZATION_REVISION.fetch_add(1, Ordering::Relaxed),
            resources: BTreeMap::new(),
        }
    }
    pub fn culture(&self) -> UiCulture {
        self.culture
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn set_culture(&mut self, culture: UiCulture) -> bool {
        if culture == self.culture {
            return false;
        }
        self.culture = culture;
        self.revision = NEXT_LOCALIZATION_REVISION.fetch_add(1, Ordering::Relaxed);
        true
    }
    /// Registration is atomic; 4096 entries and 4 MiB aggregate UTF-8 are allowed.
    pub fn register(
        &mut self,
        culture: UiCulture,
        context: &str,
        key: &str,
        message: LocalizedMessage,
    ) -> Result<(), ControlError> {
        if context.len() > 256
            || key.is_empty()
            || key.len() > 256
            || context.chars().chain(key.chars()).any(char::is_control)
        {
            return Err(invalid("Invalid localization context/key"));
        }
        let texts: Vec<&str> = match &message {
            LocalizedMessage::Text(text) => vec![text],
            LocalizedMessage::Plural(forms) => {
                if !forms.contains_key(&PluralCategory::Other) {
                    return Err(invalid("Plural resource requires Other"));
                }
                forms.values().map(String::as_str).collect()
            }
        };
        if texts.iter().any(|text| text.len() > 16_384) {
            return Err(invalid("Localized message exceeds 16384 bytes"));
        }
        let id = (culture, context.to_owned(), key.to_owned());
        let mut candidate = self.resources.clone();
        candidate.insert(id, message.clone());
        let bytes: usize = candidate
            .iter()
            .map(|((_, context, key), message)| {
                context.len()
                    + key.len()
                    + match message {
                        LocalizedMessage::Text(t) => t.len(),
                        LocalizedMessage::Plural(forms) => forms.values().map(String::len).sum(),
                    }
            })
            .sum();
        if candidate.len() > 4096 || bytes > 4 * 1024 * 1024 {
            return Err(invalid("Localization resource limit exceeded"));
        }
        self.resources = candidate;
        self.revision = NEXT_LOCALIZATION_REVISION.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    /// Lookup order: current context, current neutral, fallback context, fallback neutral, key.
    /// `{count}` is replaced only when an explicit finite count was supplied.
    pub fn text(&self, context: &str, key: &str, count: Option<f64>) -> String {
        if context.len() > 256 || key.len() > 256 {
            return key[..key.floor_char_boundary(256)].to_owned();
        }
        for culture in [self.culture, self.fallback] {
            for context in [context, ""] {
                if let Some(message) = self.resources.get(&(culture, context.into(), key.into())) {
                    let text = match message {
                        LocalizedMessage::Text(text) => text,
                        LocalizedMessage::Plural(forms) => forms
                            .get(&culture.plural(count.unwrap_or(f64::NAN)))
                            .unwrap_or(&forms[&PluralCategory::Other]),
                    };
                    let mut result = match count.filter(|c| c.is_finite()) {
                        Some(count) => text.replace("{count}", &count.to_string()),
                        None => text.clone(),
                    };
                    result.truncate(result.floor_char_boundary(65_536));
                    return result;
                }
            }
        }
        key.chars().take(256).collect()
    }
}

/// Labels, placeholders and accessible names can change without replacing editor documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalizedProperty {
    Label,
    Placeholder,
    AutomationName,
}
#[derive(Clone, Debug)]
struct LocalizedTarget {
    widget: WidgetId,
    property: LocalizedProperty,
    context: String,
    key: String,
    count: Option<f64>,
}
#[derive(Default, Debug)]
pub struct LocalizationBindings {
    targets: Vec<LocalizedTarget>,
    revision: Option<u64>,
}
impl LocalizationBindings {
    pub fn add(
        &mut self,
        widget: WidgetId,
        property: LocalizedProperty,
        context: &str,
        key: &str,
        count: Option<f64>,
    ) -> Result<(), ControlError> {
        if self.targets.len() >= 1024
            || context.len() > 256
            || key.is_empty()
            || key.len() > 256
            || count.is_some_and(|n| !n.is_finite())
        {
            return Err(invalid("Invalid localized target or target limit"));
        }
        self.targets
            .retain(|t| t.widget != widget || t.property != property);
        self.targets.push(LocalizedTarget {
            widget,
            property,
            context: context.into(),
            key: key.into(),
            count,
        });
        self.revision = None;
        Ok(())
    }
    pub fn sync(
        &mut self,
        controls: &mut Controls,
        localizer: &Localizer,
    ) -> Result<bool, ControlError> {
        if self.revision == Some(localizer.revision()) {
            return Ok(false);
        }
        self.targets
            .retain(|target| controls.runtime.tree().node(target.widget).is_some());
        for target in &self.targets {
            let value = localizer.text(&target.context, &target.key, target.count);
            match target.property {
                LocalizedProperty::Label => controls.set_label(target.widget, value)?,
                LocalizedProperty::Placeholder => controls.set_placeholder(target.widget, value)?,
                LocalizedProperty::AutomationName => {
                    controls.set_automation_name(target.widget, value)?
                }
            }
        }
        self.revision = Some(localizer.revision());
        Ok(true)
    }
}

fn invalid(message: &str) -> ControlError {
    ControlError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn culture_storage_and_strict_grouping() {
        let date = DateValue::new(2026, 10, 16).unwrap();
        for culture in [UiCulture::Russian, UiCulture::English, UiCulture::Arabic] {
            assert_eq!(
                culture
                    .parse_date(&culture.format_date(date).unwrap())
                    .unwrap(),
                date
            );
            assert_eq!(
                culture
                    .parse_number(&culture.format_number(-12345.75, 2, true).unwrap())
                    .unwrap(),
                -12345.75
            );
        }
        assert_eq!(date.to_iso(), "2026-10-16");
        assert!(UiCulture::English.parse_number("12,34.5").is_err());
        assert!(UiCulture::Russian.parse_number("12.5").is_err());
        assert!(
            UiCulture::Russian
                .format_number(f64::INFINITY, 2, false)
                .is_err()
        );
    }
    #[test]
    fn cardinal_fallback_and_context() {
        use PluralCategory::*;
        let mut localizer = Localizer::default();
        localizer
            .register(UiCulture::English, "", "open", "Open".into())
            .unwrap();
        localizer
            .register(UiCulture::Russian, "file", "open", "Открыть файл".into())
            .unwrap();
        assert_eq!(localizer.text("file", "open", None), "Открыть файл");
        assert_eq!(localizer.text("other", "open", None), "Open");
        assert_eq!(localizer.text("", "missing", None), "missing");
        for (n, expected) in [
            (0., Many),
            (1., One),
            (2., Few),
            (11., Many),
            (21., One),
            (22., Few),
            (1.5, Other),
        ] {
            assert_eq!(UiCulture::Russian.plural(n), expected);
        }
        assert_eq!(UiCulture::Arabic.direction(), ReadingDirection::RightToLeft);
        assert!(
            localizer
                .register(
                    UiCulture::Russian,
                    "",
                    "bad",
                    LocalizedMessage::Plural(BTreeMap::new())
                )
                .is_err()
        );
    }
    #[test]
    fn language_updates_preserve_editor_session_and_change_only_presentation() {
        use crate::Theme;
        use rust_desktop_ui_core::LayoutStyle;
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .edit_box(ui.root(), "Name", "Accepted", LayoutStyle::default())
            .unwrap();
        ui.set_text(id, "Draft").unwrap();
        let mut localizer = Localizer::default();
        localizer
            .register(UiCulture::Russian, "", "name", "Имя".into())
            .unwrap();
        localizer
            .register(UiCulture::English, "", "name", "Name".into())
            .unwrap();
        let mut bindings = LocalizationBindings::default();
        bindings
            .add(id, LocalizedProperty::Placeholder, "", "name", None)
            .unwrap();
        bindings.sync(&mut ui, &localizer).unwrap();
        assert_eq!(ui.editor_settings(id).unwrap().placeholder, "Имя");
        localizer.set_culture(UiCulture::English);
        bindings.sync(&mut ui, &localizer).unwrap();
        assert_eq!(ui.editor_settings(id).unwrap().placeholder, "Name");
        assert_eq!(ui.document(id).unwrap().text(), "Draft");
        assert_eq!(ui.accepted_value(id), Some("Accepted"));
    }
}
