use super::*;

/// Application-declared provenance; the product distributes no external lexicon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexiconMetadata {
    pub id: String,
    pub language: String,
    pub source: String,
    pub license: String,
}
#[derive(Clone, Debug)]
pub struct LexiconProvider {
    metadata: LexiconMetadata,
    words: Vec<String>,
}
impl LexiconProvider {
    pub fn new(
        metadata: LexiconMetadata,
        words: impl IntoIterator<Item = String>,
    ) -> Result<Self, SpellError> {
        SpellSettings {
            language: metadata.language.clone(),
            ..Default::default()
        }
        .validate()?;
        for value in [&metadata.id, &metadata.source, &metadata.license] {
            if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
                return Err(err(
                    "Lexicon provenance fields require 1..1024 printable UTF-8 bytes",
                ));
            }
        }
        let mut unique = BTreeSet::new();
        for (submitted, word) in words.into_iter().enumerate() {
            if submitted == MAX_LEXICON_WORDS {
                return Err(err(
                    "Lexicon input exceeds 16384 entries, including duplicates",
                ));
            }
            validate_word(&word)?;
            if unique.len() == MAX_LEXICON_WORDS && !unique.contains(&word) {
                return Err(err("Lexicon exceeds 16384 words"));
            }
            unique.insert(word);
        }
        Ok(Self {
            metadata,
            words: unique.into_iter().collect(),
        })
    }
    pub fn metadata(&self) -> &LexiconMetadata {
        &self.metadata
    }
}
impl SpellProvider for LexiconProvider {
    fn check(&self, request: &SpellRequest) -> Result<Vec<SpellIssue>, SpellError> {
        if request.is_cancelled() {
            return Err(err("Spelling request cancelled"));
        }
        if request.settings.language != self.metadata.language {
            return Err(err("No lexicon for the requested language"));
        }
        let words = spelling_words(request.text())?;
        let mut lexicon = BTreeMap::new();
        let mut work = 0;
        for word in &self.words {
            if request.is_cancelled() {
                return Err(err("Spelling request cancelled"));
            }
            charge(&mut work, word.len())?;
            lexicon
                .entry(request.settings.normalize(word)?)
                .or_insert(word.as_str());
        }
        let mut issues = Vec::new();
        for word in words {
            if request.is_cancelled() {
                return Err(err("Spelling request cancelled"));
            }
            if request.is_ignored(&word)? {
                continue;
            }
            charge(&mut work, word.text.len())?;
            let normalized = request.settings.normalize(&word.text)?;
            if lexicon.contains_key(&normalized) {
                continue;
            }
            if issues.len() == MAX_SPELL_ISSUES {
                return Err(err("Spelling request exceeds 256 misspellings"));
            }
            let target: Vec<_> = normalized.graphemes(true).collect();
            let mut suggestions = Vec::new();
            for (candidate, original) in &lexicon {
                if request.is_cancelled() {
                    return Err(err("Spelling request cancelled"));
                }
                charge(&mut work, candidate.len().max(1))?;
                let candidate: Vec<_> = candidate.graphemes(true).collect();
                if target.len().abs_diff(candidate.len()) > 2 {
                    continue;
                }
                let distance = distance(&target, &candidate, &mut work)?;
                if distance <= 2 {
                    suggestions.push((distance, (*original).to_owned()));
                    suggestions.sort();
                    suggestions.truncate(MAX_SPELL_SUGGESTIONS);
                }
            }
            issues.push(SpellIssue {
                range: word.range,
                word: word.text,
                suggestions: suggestions.into_iter().map(|(_, s)| s).collect(),
            });
        }
        Ok(issues)
    }
}
pub(super) fn distance(a: &[&str], b: &[&str], work: &mut usize) -> Result<usize, SpellError> {
    let cells = a.len().saturating_mul(b.len());
    charge(work, cells)?;
    let mut previous: Vec<_> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, left) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + usize::from(left != right));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    Ok(previous[b.len()])
}
fn charge(work: &mut usize, amount: usize) -> Result<(), SpellError> {
    if amount > MAX_SPELL_WORK.saturating_sub(*work) {
        return Err(err("Spelling suggestion work budget exceeded"));
    }
    *work += amount;
    Ok(())
}
