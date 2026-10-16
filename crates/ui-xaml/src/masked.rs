//! Bounded declarative masks. Runtime validates storage against slot categories.
use crate::{EditBoxDefinition, NumericCultureDefinition};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq)]
pub struct MaskEditDefinition {
    pub editor: EditBoxDefinition,
    pub mask: String,
    /// Raw slot content, without presentation literals or prompt characters.
    pub value: String,
    pub culture: NumericCultureDefinition,
    pub prompt_char: char,
    pub allow_empty: bool,
}
impl Default for MaskEditDefinition {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            mask: String::new(),
            value: String::new(),
            culture: NumericCultureDefinition::Russian,
            prompt_char: '_',
            allow_empty: true,
        }
    }
}

/// Checks the bounded syntax without depending on the runtime control crate.
/// The mount preflight also validates raw storage using the runtime mask model.
pub fn validate_mask_edit(
    mask: &MaskEditDefinition,
    has_value_binding: bool,
) -> Result<(), String> {
    if mask.mask.is_empty() || mask.mask.len() > 4096 {
        return Err("Mask must contain 1..4096 UTF-8 bytes".into());
    }
    if mask.prompt_char.is_control()
        || matches!(mask.prompt_char, '\u{2028}' | '\u{2029}')
        || format!("a{}b", mask.prompt_char).graphemes(true).count() != 3
        || format!("{0}{0}", mask.prompt_char).graphemes(true).count() != 2
    {
        return Err("PromptChar must be one independent printable character".into());
    }
    let mut escaped = false;
    let mut slots = 0;
    for grapheme in mask.mask.graphemes(true) {
        if grapheme
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err("Mask cannot contain control characters".into());
        }
        if escaped {
            escaped = false;
        } else if grapheme == "\\" {
            escaped = true;
        } else if matches!(grapheme, "0" | "9" | "L" | "l" | "A" | "a" | "X" | "x") {
            slots += 1;
        }
    }
    if escaped || !(1..=256).contains(&slots) {
        return Err("Mask requires 1..256 slots and complete backslash escapes".into());
    }
    if mask.value.graphemes(true).count() > slots {
        return Err("MaskEdit Value exceeds the available slots".into());
    }
    if !mask.allow_empty && mask.value.is_empty() && !has_value_binding {
        return Err("MaskEdit requires Value when AllowEmpty=False".into());
    }
    Ok(())
}
