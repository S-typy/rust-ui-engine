// Copyright 2026 rust-ui-engine contributors. Licensed under Apache-2.0.
//! Passive bounded searches within the caller's current text range.
use super::*;
use windows::Win32::{
    Foundation::{ERROR_SUCCESS, GetLastError},
    Globalization::{FIND_FROMEND, FIND_FROMSTART, FindStringOrdinal},
};

const MAX_SEARCH_UNITS: usize = 1_048_576;
const MAX_SEARCH_RUNS: usize = 65_536;

#[cfg(test)]
mod tests;

pub(super) fn text<'a>(
    range: &'a Range<'_>,
    needle: &BSTR,
    backward: bool,
    ignore_case: bool,
) -> Result<Option<Range<'a>>> {
    if needle.is_empty() || needle.len() > MAX_SEARCH_UNITS || String::from_utf16(needle).is_err() {
        return Err(invalid_arg());
    }
    let mut value = Vec::new();
    let mut runs = 0;
    let exceeded = range.traverse_text(|_, run| {
        runs += 1;
        if runs > MAX_SEARCH_RUNS {
            return Some(());
        }
        for unit in run.encode_utf16() {
            if value.len() == MAX_SEARCH_UNITS {
                return Some(());
            }
            value.push(unit);
        }
        None
    });
    if exceeded.is_some() {
        return Err(invalid_operation());
    }
    if value.len() < needle.len() {
        return Ok(None);
    }
    // Windows ordinal matching is independent of UI language and preserves
    // UTF-16 length. It neither normalizes accents nor expands ligatures.
    let offset = unsafe {
        FindStringOrdinal(
            if backward {
                FIND_FROMEND
            } else {
                FIND_FROMSTART
            },
            &value,
            needle,
            ignore_case,
        )
    };
    if offset < 0 {
        let error = unsafe { GetLastError() };
        return if error == ERROR_SUCCESS {
            Ok(None)
        } else {
            Err(Error::from_hresult(error.to_hresult()))
        };
    }
    let start = range.start().to_global_utf16_index() + offset as usize;
    let end = start + needle.len();
    let owner = range.node();
    let from = owner
        .text_position_from_global_utf16_index(start)
        .ok_or_else(invalid_operation)?;
    let mut to = owner
        .text_position_from_global_utf16_index(end)
        .ok_or_else(invalid_operation)?;
    // Consumer positions address graphemes. Include the complete final
    // grapheme if the exact UTF-16 match ends inside a combining sequence.
    if to.to_global_utf16_index() < end {
        to = to.forward_to_character_end();
    }
    if from < range.start() || to > range.end() {
        return Err(invalid_operation());
    }
    let mut found = from.to_degenerate_range();
    found.set_end(to);
    Ok(Some(found))
}

fn attribute_type(id: UIA_TEXTATTRIBUTE_ID) -> Option<VARENUM> {
    match id {
        UIA_IsReadOnlyAttributeId
        | UIA_IsItalicAttributeId
        | UIA_IsSubscriptAttributeId
        | UIA_IsSuperscriptAttributeId => Some(VT_BOOL),
        UIA_FontNameAttributeId => Some(VT_BSTR),
        UIA_FontSizeAttributeId => Some(VT_R8),
        UIA_CaretPositionAttributeId
        | UIA_CultureAttributeId
        | UIA_FontWeightAttributeId
        | UIA_BackgroundColorAttributeId
        | UIA_ForegroundColorAttributeId
        | UIA_OverlineStyleAttributeId
        | UIA_OverlineColorAttributeId
        | UIA_StrikethroughStyleAttributeId
        | UIA_StrikethroughColorAttributeId
        | UIA_UnderlineStyleAttributeId
        | UIA_UnderlineColorAttributeId
        | UIA_HorizontalTextAlignmentAttributeId => Some(VT_I4),
        _ => None,
    }
}

fn same_value(a: &VARIANT, b: &VARIANT) -> bool {
    // Do not coerce strings/numbers or invoke arbitrary COM objects supplied
    // by a client. Only the scalar type declared for the attribute is read.
    unsafe {
        let a = &a.Anonymous.Anonymous;
        let b = &b.Anonymous.Anonymous;
        if a.vt != b.vt {
            return false;
        }
        match a.vt {
            VT_BOOL => (a.Anonymous.boolVal.0 != 0) == (b.Anonymous.boolVal.0 != 0),
            VT_I4 => a.Anonymous.lVal == b.Anonymous.lVal,
            VT_R8 => a.Anonymous.dblVal == b.Anonymous.dblVal,
            VT_BSTR => a.Anonymous.bstrVal == b.Anonymous.bstrVal,
            _ => false,
        }
    }
}

pub(super) fn attribute<'a>(
    range: &'a Range<'_>,
    id: UIA_TEXTATTRIBUTE_ID,
    value: &VARIANT,
    backward: bool,
) -> Result<Option<Range<'a>>> {
    let Some(expected) = attribute_type(id) else {
        return Ok(None);
    };
    if unsafe { value.Anonymous.Anonymous.vt } != expected {
        return Err(invalid_arg());
    }
    if range.is_degenerate()
        || matches!(id, UIA_IsReadOnlyAttributeId | UIA_CaretPositionAttributeId)
    {
        return Ok(same_value(&attributes::value(range, id)?, value).then_some(*range));
    }
    let mut offset = 0usize;
    let mut pending: Option<(usize, usize)> = None;
    let mut last = None;
    let mut runs = 0;
    // Inspect actual runs: format-unit navigation can merge runs that differ
    // in an individual attribute (for example italic). Map only the selected
    // interval back to positions, avoiding a full-document scan for each run.
    let stopped = range.traverse_text(|node, run| {
        runs += 1;
        if runs > MAX_SEARCH_RUNS {
            return Some(Err(invalid_operation()));
        }
        if run.is_empty() {
            return None;
        }
        let end = offset + run.encode_utf16().count();
        if end > MAX_SEARCH_UNITS {
            return Some(Err(invalid_operation()));
        }
        let actual = match attributes::run_value(node, id) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        if same_value(&actual, value) {
            if let Some(found) = &mut pending {
                found.1 = end;
            } else {
                pending = Some((offset, end));
            }
        } else if let Some(found) = pending.take() {
            if !backward {
                return Some(Ok(found));
            }
            last = Some(found);
        }
        offset = end;
        None
    });
    let found = match stopped {
        Some(result) => Some(result?),
        None => pending.or(last),
    };
    let Some((start, end)) = found else {
        return Ok(None);
    };
    let base = range.start().to_global_utf16_index();
    let node = range.node();
    let start = node
        .text_position_from_global_utf16_index(base + start)
        .ok_or_else(invalid_operation)?;
    let end = node
        .text_position_from_global_utf16_index(base + end)
        .ok_or_else(invalid_operation)?;
    let mut result = start.to_degenerate_range();
    result.set_end(end);
    Ok(Some(result))
}
