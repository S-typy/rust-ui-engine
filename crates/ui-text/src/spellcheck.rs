//! Versioned spelling requests. Providers run only when called by the application.
use crate::{DocumentVersion, TextDocument};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use unicode_segmentation::UnicodeSegmentation;
mod lexicon;
pub use lexicon::*;
#[cfg(test)]
mod tests;

pub const MAX_SPELL_TEXT_BYTES: usize = 65_536;
pub const MAX_SPELL_WORDS: usize = 4096;
pub const MAX_SPELL_ISSUES: usize = 256;
pub const MAX_SPELL_SUGGESTIONS: usize = 8;
pub const MAX_LEXICON_WORDS: usize = 16_384;
pub const MAX_SPELL_POLICY_WORDS: usize = 4096;
pub const MAX_SPELL_WORK: usize = 2_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellError(pub String);
impl fmt::Display for SpellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SpellError {}
fn err(message: &str) -> SpellError {
    SpellError(message.to_owned())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpellComparison {
    Ordinal,
    #[default]
    UnicodeLowercase,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellSettings {
    pub language: String,
    pub comparison: SpellComparison,
    pub ignore_numbers: bool,
    pub ignore_uppercase: bool,
}
impl Default for SpellSettings {
    fn default() -> Self {
        Self {
            language: "ru-RU".into(),
            comparison: SpellComparison::UnicodeLowercase,
            ignore_numbers: true,
            ignore_uppercase: false,
        }
    }
}
impl SpellSettings {
    pub fn validate(&self) -> Result<(), SpellError> {
        if !(2..=64).contains(&self.language.len())
            || self
                .language
                .split('-')
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            return Err(err(
                "Language must be an explicit 2..64 byte alphanumeric/hyphen identifier",
            ));
        }
        Ok(())
    }
    fn normalize(&self, word: &str) -> Result<String, SpellError> {
        validate_word(word)?;
        let value = match self.comparison {
            SpellComparison::Ordinal => word.to_owned(),
            SpellComparison::UnicodeLowercase => word.to_lowercase(),
        };
        validate_word(&value)?;
        Ok(value)
    }
    fn skip(&self, word: &str) -> bool {
        (self.ignore_numbers && word.chars().all(char::is_numeric))
            || (self.ignore_uppercase
                && word.chars().any(char::is_uppercase)
                && !word.chars().any(char::is_lowercase))
    }
}
fn validate_word(word: &str) -> Result<(), SpellError> {
    if word.is_empty()
        || word.len() > 512
        || word.graphemes(true).count() > 64
        || word.chars().any(char::is_control)
    {
        return Err(err(
            "Spelling words require 1..64 graphemes and at most 512 UTF-8 bytes",
        ));
    }
    let mut words = word.unicode_word_indices();
    if words.next() != Some((0, word)) || words.next().is_some() {
        return Err(err(
            "Spelling entries must contain exactly one Unicode word",
        ));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellWord {
    pub range: Range<usize>,
    pub text: String,
}
pub fn spelling_words(text: &str) -> Result<Vec<SpellWord>, SpellError> {
    if text.len() > MAX_SPELL_TEXT_BYTES {
        return Err(err("Spelling text exceeds 64 KiB"));
    }
    let mut words = Vec::new();
    for (start, word) in text.unicode_word_indices() {
        if words.len() == MAX_SPELL_WORDS {
            return Err(err("Spelling text exceeds 4096 words"));
        }
        validate_word(word)?;
        words.push(SpellWord {
            range: start..start + word.len(),
            text: word.to_owned(),
        });
    }
    Ok(words)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellIssue {
    pub range: Range<usize>,
    pub word: String,
    pub suggestions: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct SpellRequest {
    owner: Arc<()>,
    identity: Arc<()>,
    cancelled: Arc<AtomicBool>,
    version: DocumentVersion,
    text: Arc<str>,
    settings: SpellSettings,
    ignored: Arc<BTreeSet<String>>,
    custom: Arc<BTreeSet<String>>,
    once: Vec<Range<usize>>,
}
impl SpellRequest {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn settings(&self) -> &SpellSettings {
        &self.settings
    }
    pub fn version(&self) -> &DocumentVersion {
        &self.version
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn is_ignored(&self, word: &SpellWord) -> Result<bool, SpellError> {
        let normalized = self.settings.normalize(&word.text)?;
        Ok(self.settings.skip(&word.text)
            || self.once.contains(&word.range)
            || self.ignored.contains(&normalized)
            || self.custom.contains(&normalized))
    }
}
/// The application may run this on its worker and send the result to the UI.
pub trait SpellProvider: Send + Sync {
    fn check(&self, request: &SpellRequest) -> Result<Vec<SpellIssue>, SpellError>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellCompletion {
    Accepted,
    Stale,
}
#[derive(Clone, Debug)]
struct Accepted {
    version: DocumentVersion,
    issues: Vec<SpellIssue>,
}
#[derive(Default, Debug)]
struct Policy {
    ignored: BTreeSet<String>,
    custom: BTreeSet<String>,
}
#[derive(Debug)]
pub struct SpellSession {
    owner: Arc<()>,
    settings: SpellSettings,
    pending: Option<SpellRequest>,
    accepted: Option<Accepted>,
    policies: BTreeMap<(String, SpellComparison), Policy>,
    once: Option<(DocumentVersion, Vec<Range<usize>>)>,
    error: Option<String>,
}
impl SpellSession {
    pub fn new(settings: SpellSettings) -> Result<Self, SpellError> {
        settings.validate()?;
        Ok(Self {
            owner: Arc::new(()),
            settings,
            pending: None,
            accepted: None,
            policies: BTreeMap::new(),
            once: None,
            error: None,
        })
    }
    pub fn settings(&self) -> &SpellSettings {
        &self.settings
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn cancel(&mut self) {
        if let Some(request) = self.pending.take() {
            request.cancelled.store(true, Ordering::Release);
        }
    }
    pub fn invalidate(&mut self) {
        self.cancel();
        self.accepted = None;
        self.error = None;
    }
    /// Observe direct document edits without relying on an editor event queue.
    pub fn synchronize(&mut self, document: &TextDocument) {
        let version = document.version();
        if document.composition().is_some()
            || self.pending.as_ref().is_some_and(|p| p.version != version)
            || self.accepted.as_ref().is_some_and(|a| a.version != version)
        {
            self.invalidate();
        }
    }
    pub fn set_settings(&mut self, settings: SpellSettings) -> Result<(), SpellError> {
        settings.validate()?;
        if self.settings != settings {
            self.invalidate();
            self.once = None;
            self.settings = settings;
        }
        Ok(())
    }
    pub fn request(&mut self, document: &TextDocument) -> Result<SpellRequest, SpellError> {
        if document.composition().is_some() {
            return Err(err("Finish composition before checking spelling"));
        }
        spelling_words(document.text())?;
        let version = document.version();
        let once = self
            .once
            .as_ref()
            .filter(|(v, _)| *v == version)
            .map(|(_, r)| r.clone())
            .unwrap_or_default();
        let policy = self
            .policies
            .get(&(self.settings.language.clone(), self.settings.comparison));
        let request = SpellRequest {
            owner: self.owner.clone(),
            identity: Arc::new(()),
            cancelled: Arc::new(AtomicBool::new(false)),
            version,
            text: Arc::from(document.text()),
            settings: self.settings.clone(),
            ignored: Arc::new(policy.map(|p| p.ignored.clone()).unwrap_or_default()),
            custom: Arc::new(policy.map(|p| p.custom.clone()).unwrap_or_default()),
            once,
        };
        self.cancel();
        self.accepted = None;
        self.error = None;
        self.pending = Some(request.clone());
        Ok(request)
    }
    /// Stale and foreign tokens are rejected before inspecting their payload.
    pub fn complete(
        &mut self,
        document: &TextDocument,
        request: &SpellRequest,
        result: Result<Vec<SpellIssue>, SpellError>,
    ) -> Result<SpellCompletion, SpellError> {
        if request.is_cancelled()
            || !Arc::ptr_eq(&request.owner, &self.owner)
            || !self
                .pending
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(&p.identity, &request.identity))
        {
            return Ok(SpellCompletion::Stale);
        }
        if document.composition().is_some() || document.version() != request.version {
            self.invalidate();
            return Ok(SpellCompletion::Stale);
        }
        match result {
            Ok(issues) => {
                validate_issues(request, &issues)?;
                self.accepted = Some(Accepted {
                    version: request.version.clone(),
                    issues,
                });
                self.error = None;
            }
            Err(error) => {
                if error.0.len() > 4096 {
                    return Err(err("Spelling provider error exceeds 4096 bytes"));
                }
                self.error = Some(error.0);
                self.accepted = None;
            }
        }
        self.cancel();
        Ok(SpellCompletion::Accepted)
    }
    pub fn issues<'a>(&'a self, document: &TextDocument) -> Option<&'a [SpellIssue]> {
        self.accepted
            .as_ref()
            .filter(|a| a.version == document.version() && document.composition().is_none())
            .map(|a| a.issues.as_slice())
    }
    pub fn suggestion(
        &self,
        document: &TextDocument,
        issue: usize,
        suggestion: usize,
    ) -> Result<SpellReplacement, SpellError> {
        let issue = self
            .issues(document)
            .and_then(|v| v.get(issue))
            .ok_or_else(|| err("Spelling issue is stale or unknown"))?;
        let text = issue
            .suggestions
            .get(suggestion)
            .ok_or_else(|| err("Unknown spelling suggestion"))?
            .clone();
        validate_word(&text)?;
        Ok(SpellReplacement {
            version: document.version(),
            range: issue.range.clone(),
            word: issue.word.clone(),
            text,
        })
    }
    pub fn ignore_once(&mut self, document: &TextDocument, issue: usize) -> Result<(), SpellError> {
        let range = self
            .issues(document)
            .and_then(|v| v.get(issue))
            .ok_or_else(|| err("Spelling issue is stale or unknown"))?
            .range
            .clone();
        let version = document.version();
        let mut ranges = self
            .once
            .as_ref()
            .filter(|(v, _)| *v == version)
            .map(|(_, r)| r.clone())
            .unwrap_or_default();
        if ranges.len() == MAX_SPELL_ISSUES {
            return Err(err("Ignore-once limit reached"));
        }
        if !ranges.contains(&range) {
            ranges.push(range.clone());
        }
        self.once = Some((version, ranges));
        self.cancel();
        if let Some(a) = &mut self.accepted {
            a.issues.retain(|i| i.range != range);
        }
        Ok(())
    }
    pub fn ignore_all(&mut self, word: &str) -> Result<(), SpellError> {
        self.add_policy_word(word, false)
    }
    pub fn add_custom_word(&mut self, word: &str) -> Result<(), SpellError> {
        self.add_policy_word(word, true)
    }
    fn add_policy_word(&mut self, word: &str, custom: bool) -> Result<(), SpellError> {
        let value = self.settings.normalize(word)?;
        let key = (self.settings.language.clone(), self.settings.comparison);
        if !self.policies.contains_key(&key) && self.policies.len() == 16 {
            return Err(err("At most 16 language/comparison policies are retained"));
        }
        let p = self.policies.get(&key);
        let values = p.map(|p| if custom { &p.custom } else { &p.ignored });
        let total: usize = self
            .policies
            .values()
            .map(|p| p.custom.len() + p.ignored.len())
            .sum();
        if total == MAX_SPELL_POLICY_WORDS && values.is_none_or(|v| !v.contains(&value)) {
            return Err(err("Dictionary policy limit reached"));
        }
        let p = self.policies.entry(key).or_default();
        if custom {
            p.custom.insert(value);
        } else {
            p.ignored.insert(value);
        }
        self.invalidate();
        Ok(())
    }
}
impl Drop for SpellSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Typed, validated replacement proposal. Applying it still requires the exact
/// document revision, editability, composition and final candidate limits.
#[derive(Clone, Debug)]
pub struct SpellReplacement {
    version: DocumentVersion,
    range: Range<usize>,
    word: String,
    text: String,
}
impl SpellReplacement {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn candidate(&self, document: &TextDocument) -> Result<String, SpellError> {
        if document.version() != self.version
            || document.composition().is_some()
            || document.text().get(self.range.clone()) != Some(self.word.as_str())
        {
            return Err(err("Spelling replacement is stale"));
        }
        let mut value = document.text().to_owned();
        value.replace_range(self.range.clone(), &self.text);
        Ok(value)
    }
}
fn validate_issues(request: &SpellRequest, issues: &[SpellIssue]) -> Result<(), SpellError> {
    if issues.len() > MAX_SPELL_ISSUES {
        return Err(err("Provider returned more than 256 spelling issues"));
    }
    let words = spelling_words(request.text())?;
    let boundaries: BTreeSet<_> = request
        .text()
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(request.text().len()))
        .collect();
    let mut end = 0;
    for issue in issues {
        if issue.range.start < end
            || issue.range.start >= issue.range.end
            || !boundaries.contains(&issue.range.start)
            || !boundaries.contains(&issue.range.end)
        {
            return Err(err(
                "Spelling ranges must be ordered, disjoint and grapheme-aligned",
            ));
        }
        let word = words
            .binary_search_by_key(&issue.range.start, |w| w.range.start)
            .ok()
            .and_then(|i| words.get(i))
            .filter(|w| w.range == issue.range && w.text == issue.word)
            .ok_or_else(|| err("Spelling range does not match an exact Unicode word"))?;
        if request.is_ignored(word)? || issue.suggestions.len() > MAX_SPELL_SUGGESTIONS {
            return Err(err(
                "Provider returned an ignored word or too many suggestions",
            ));
        }
        let mut seen = BTreeSet::new();
        for suggestion in &issue.suggestions {
            validate_word(suggestion)?;
            if !seen.insert(suggestion) {
                return Err(err("Duplicate spelling suggestion"));
            }
        }
        end = issue.range.end;
    }
    Ok(())
}
