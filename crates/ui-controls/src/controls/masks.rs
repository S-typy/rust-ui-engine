//! Masked drafts retain ordinary editor selection, history and commit semantics.
use super::masks_model::{Atom, MaskLayout, graphemes};
use super::*;
use std::{cell::RefCell, ops::Range, rc::Rc};

struct Entry {
    settings: Option<MaskSettings>,
    shared: Rc<RefCell<Option<MaskSettings>>>,
}
#[derive(Default)]
pub(super) struct MasksState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    /// Construct a single-line editor from raw storage, excluding mask literals.
    pub fn masked_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        storage: &str,
        settings: MaskSettings,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let display = settings.format(storage)?;
        let id = self.edit_box(parent, label, display, style)?;
        self.masks.entries.insert(
            id,
            Entry {
                shared: Rc::new(RefCell::new(Some(settings.clone()))),
                settings: Some(settings),
            },
        );
        Ok(id)
    }
    pub fn editor_mask_settings(&self, id: WidgetId) -> Option<&MaskSettings> {
        self.masks.entries.get(&id)?.settings.as_ref()
    }
    /// Last accepted raw value. Incomplete drafts do not change this value.
    pub fn masked_value(&self, id: WidgetId) -> Option<String> {
        self.editor_mask_settings(id)?
            .parse_display(self.accepted_value(id)?)
            .ok()
            .filter(|value| value.complete)
            .map(|value| value.storage)
    }
    pub fn set_masked_value(
        &mut self,
        id: WidgetId,
        storage: &str,
    ) -> Result<CommitOutcome, ControlError> {
        let settings = self
            .editor_mask_settings(id)
            .ok_or_else(|| ControlError::Invalid("Not a masked editor".into()))?;
        let display = match settings.format(storage) {
            Ok(display) => display,
            Err(error) => return Ok(CommitOutcome::Invalid(error.to_string())),
        };
        self.set_editor_value(id, &display)
    }
    /// Attach, replace or remove a mask on a clean, single-line EditBox or
    /// ButtonEdit editor. Attach before installing Value bindings. Reformatting
    /// preserves raw storage and slot-relative selection, and clears history.
    pub fn set_editor_mask(
        &mut self,
        id: WidgetId,
        settings: Option<MaskSettings>,
    ) -> Result<(), ControlError> {
        let control = self.editor_control(id)?;
        if !control.single_line
            || self.password_settings(id).is_some()
            || self.rich.composites.values().any(|c| {
                c.parts.editor == id && !matches!(c.kind, selection::CompositeKind::Buttons)
            })
        {
            return Err(ControlError::Invalid(
                "Masks require a plain single-line editor or ButtonEdit".into(),
            ));
        }
        if self.is_edit_dirty(id) == Some(true)
            || self.document(id).unwrap().composition().is_some()
        {
            return Err(ControlError::Invalid(
                "Accept or cancel the draft before changing its mask".into(),
            ));
        }
        let old = self.editor_mask_settings(id).cloned();
        if old == settings {
            return Ok(());
        }
        let storage = if old.is_some() {
            self.masked_value(id).unwrap()
        } else {
            self.accepted_value(id).unwrap().to_owned()
        };
        let display = settings
            .as_ref()
            .map_or_else(|| Ok(storage.clone()), |s| s.format(&storage))?;
        let selection = self.document(id).unwrap().selection();
        let old_slots = old
            .as_ref()
            .and_then(|s| {
                s.display_cells(self.accepted_value(id)?)
                    .ok()
                    .and_then(|cells| s.layout(&cells).ok())
            })
            .map(|v| v.slots);
        let new_slots = settings
            .as_ref()
            .and_then(|s| {
                s.storage_cells(&storage)
                    .ok()
                    .and_then(|cells| s.layout(&cells).ok())
            })
            .map(|v| v.slots);
        let map_position = |position: usize| {
            let index = old_slots.as_ref().map_or_else(
                || graphemes(&storage[..position.min(storage.len())]).len(),
                |slots| {
                    slots
                        .iter()
                        .take_while(|slot| slot.start < position)
                        .count()
                },
            );
            new_slots.as_ref().map_or_else(
                || {
                    graphemes(&storage)
                        .iter()
                        .take(index)
                        .map(|v| v.len())
                        .sum()
                },
                |slots| {
                    slots
                        .get(index)
                        .map_or_else(|| slots.last().unwrap().end, |r| r.start)
                },
            )
        };
        let anchor = map_position(selection.anchor);
        let focus = map_position(selection.focus);
        let shared = self
            .masks
            .entries
            .get(&id)
            .map(|e| Rc::clone(&e.shared))
            .unwrap_or_else(|| Rc::new(RefCell::new(None)));
        *shared.borrow_mut() = settings.clone();
        self.masks.entries.insert(
            id,
            Entry {
                settings: settings.clone(),
                shared: Rc::clone(&shared),
            },
        );
        let result = self.set_editor_value(id, &display);
        match result {
            Ok(CommitOutcome::Committed | CommitOutcome::Unchanged) => {
                self.mask_selection(id, anchor, focus)?;
                Ok(())
            }
            other => {
                *shared.borrow_mut() = old.clone();
                self.masks.entries.insert(
                    id,
                    Entry {
                        settings: old,
                        shared,
                    },
                );
                match other {
                    Err(error) => Err(error),
                    Ok(value) => Err(ControlError::Invalid(format!(
                        "Mask settings rejected: {value:?}"
                    ))),
                }
            }
        }
    }
    pub(crate) fn mask_binding_settings(
        &self,
        id: WidgetId,
    ) -> Option<Rc<RefCell<Option<MaskSettings>>>> {
        let id = self.composite_editor(id).unwrap_or(id);
        self.masks.entries.get(&id).map(|e| Rc::clone(&e.shared))
    }
    pub(super) fn validate_mask_value(&self, id: WidgetId, display: &str) -> Result<(), String> {
        if let Some(settings) = self.editor_mask_settings(id)
            && !settings.parse_display(display)?.complete
        {
            return Err("Complete all required mask slots".into());
        }
        Ok(())
    }
    pub(super) fn mask_draft(
        &self,
        id: WidgetId,
        display: &str,
    ) -> Result<Option<String>, ControlError> {
        let Some(settings) = self.editor_mask_settings(id) else {
            return Ok(None);
        };
        let display = if display.is_empty() {
            settings
                .layout(&vec![None; settings.pattern.slot_count()])
                .map_err(ControlError::Text)?
                .display
        } else {
            settings
                .parse_display(display)
                .map_err(ControlError::Text)?;
            display.to_owned()
        };
        Ok(Some(display))
    }
    pub(super) fn prune_masks(&mut self) {
        self.masks
            .entries
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
    }
    fn mask_selection(
        &mut self,
        id: WidgetId,
        anchor: usize,
        focus: usize,
    ) -> Result<(), ControlError> {
        let control = self.controls.get_mut(&id).unwrap();
        let Kind::TextBox(document) = &mut control.kind else {
            unreachable!()
        };
        document.set_selection(anchor, focus).map_err(edit_error)?;
        control.caret = TextCaret {
            index: focus,
            ..Default::default()
        };
        control.caret_goal_x = None;
        control.caret_reveal = true;
        self.caret_visible = true;
        self.scene_dirty = true;
        Ok(())
    }
    fn apply_mask_draft(
        &mut self,
        id: WidgetId,
        layout: MaskLayout,
        caret: usize,
    ) -> Result<(), ControlError> {
        // Preflight the entire formatted candidate before changing text or undo.
        let limits = editor_limits(self.controls[&id].editor.max_length);
        limits.validate(&layout.display).map_err(edit_error)?;
        self.preflight_editor_text(id, &layout.display)?;
        let before = self.document(id).unwrap().text().to_owned();
        if before != layout.display {
            let Kind::TextBox(document) = &mut self.controls.get_mut(&id).unwrap().kind else {
                unreachable!()
            };
            document
                .try_set_text(&layout.display, limits)
                .map_err(edit_error)?;
        }
        self.mask_selection(id, caret, caret)?;
        self.record_text_change(id, &before);
        Ok(())
    }
    pub(super) fn mask_text_input(&mut self, text: &str, ime: bool) -> Result<bool, ControlError> {
        let Some(id) = self
            .runtime
            .focused()
            .filter(|&id| self.is_text_editable(id))
        else {
            return Ok(false);
        };
        let Some(settings) = self.editor_mask_settings(id).cloned() else {
            return Ok(false);
        };
        let document = self.document(id).unwrap();
        if !ime && document.composition().is_some() {
            return Ok(true);
        }
        let range = document.composition().filter(|_| ime).map_or_else(
            || document.selection().range(),
            |composition| composition.replacement.clone(),
        );
        let before = document.text().to_owned();
        let result = (|| {
            let normalized = normalize_editor_text(text, true).map_err(edit_error)?;
            if normalized.is_empty() {
                return Ok(());
            }
            let cells = settings
                .display_cells(&before)
                .map_err(ControlError::Text)?;
            let layout = settings.layout(&cells).map_err(ControlError::Text)?;
            let (cells, next) = insert(&settings, &layout, cells, range, &normalized)
                .map_err(ControlError::Text)?;
            let layout = settings.layout(&cells).map_err(ControlError::Text)?;
            let caret = layout
                .slots
                .get(next)
                .map_or_else(|| layout.slots.last().unwrap().end, |slot| slot.start);
            self.apply_mask_draft(id, layout, caret)
        })();
        if ime {
            if let Kind::TextBox(document) = &mut self.controls.get_mut(&id).unwrap().kind {
                document.cancel_preedit();
            }
            self.scene_dirty = true;
        }
        match result {
            Ok(()) | Err(ControlError::Text(_)) | Err(ControlError::Edit(_)) => Ok(true),
            Err(error) => Err(error),
        }
    }
    pub(super) fn mask_edit_key(
        &mut self,
        id: WidgetId,
        key: &Key,
        modifiers: rust_desktop_ui_core::Modifiers,
        repeat: bool,
    ) -> Result<bool, ControlError> {
        let Some(settings) = self.editor_mask_settings(id).cloned() else {
            return Ok(false);
        };
        let document = self.document(id).unwrap();
        if document.composition().is_some() {
            return Ok(false);
        }
        let shortcut = (modifiers.control && !modifiers.alt) || modifiers.super_key;
        let selection = document.selection();
        let range = selection.range();
        let mut cells = settings
            .display_cells(document.text())
            .map_err(ControlError::Text)?;
        let layout = settings.layout(&cells).map_err(ControlError::Text)?;
        let slots = &layout.slots;
        let first = slots.first().unwrap().start;
        let last = slots.last().unwrap().end;
        let navigation = match key {
            Key::Home => Some(first),
            Key::End => Some(last),
            Key::ArrowLeft if !modifiers.shift && !range.is_empty() => Some(range.start),
            Key::ArrowRight if !modifiers.shift && !range.is_empty() => Some(range.end),
            Key::ArrowLeft if shortcut => Some(segment_edge(slots, selection.focus, false)),
            Key::ArrowRight if shortcut => Some(segment_edge(slots, selection.focus, true)),
            Key::ArrowLeft => Some(
                slots
                    .iter()
                    .rev()
                    .find(|slot| slot.start < selection.focus)
                    .map_or(first, |slot| slot.start),
            ),
            Key::ArrowRight => Some(
                slots
                    .iter()
                    .find(|slot| slot.start > selection.focus)
                    .map_or(last, |slot| slot.start),
            ),
            _ => None,
        };
        if let Some(position) = navigation {
            self.mask_selection(
                id,
                if modifiers.shift {
                    selection.anchor
                } else {
                    position
                },
                position,
            )?;
            return Ok(true);
        }
        let cut = matches!(key, Key::Character(value) if shortcut && value.eq_ignore_ascii_case("x"))
            || (*key == Key::Delete && modifiers.shift && !modifiers.alt);
        if !matches!(key, Key::Backspace | Key::Delete) && !cut {
            return Ok(false);
        }
        if !self.is_text_editable(id) || (cut && repeat) {
            return Ok(true);
        }
        let copy = cut.then(|| document.selected_text().to_owned());
        let selected = if !range.is_empty() || cut {
            range.clone()
        } else if shortcut && *key == Key::Backspace {
            segment_edge(slots, range.start, false)..range.start
        } else if shortcut {
            range.start..segment_edge(slots, range.start, true)
        } else if *key == Key::Backspace {
            slots
                .iter()
                .rev()
                .find(|slot| slot.start < range.start)
                .cloned()
                .unwrap_or(range.clone())
        } else {
            slots
                .iter()
                .find(|slot| slot.end > range.start)
                .cloned()
                .unwrap_or(range.clone())
        };
        let mut next = None;
        for (index, slot) in slots.iter().enumerate() {
            if slot.start < selected.end && slot.end > selected.start {
                cells[index] = None;
                next.get_or_insert(index);
            }
        }
        if let Some(index) = next {
            let layout = settings.layout(&cells).map_err(ControlError::Text)?;
            let caret = layout.slots[index].start;
            // Deleting a wide glyph can produce a wider placeholder; limits still apply.
            match self.apply_mask_draft(id, layout, caret) {
                Ok(()) => {}
                Err(ControlError::Text(_)) | Err(ControlError::Edit(_)) => return Ok(true),
                Err(error) => return Err(error),
            }
        }
        if let Some(text) = copy {
            self.clipboard.push(ClipboardRequest::Copy(text));
        }
        Ok(true)
    }
}

fn segment_edge(slots: &[Range<usize>], focus: usize, forward: bool) -> usize {
    if forward {
        let Some(index) = slots.iter().position(|slot| slot.end > focus) else {
            return slots.last().unwrap().end;
        };
        for next in index + 1..slots.len() {
            if slots[next].start != slots[next - 1].end {
                return slots[next].start;
            }
        }
        slots.last().unwrap().end
    } else {
        let Some(index) = slots.iter().rposition(|slot| slot.start < focus) else {
            return slots[0].start;
        };
        for previous in (0..index).rev() {
            if slots[previous].end != slots[previous + 1].start {
                return slots[previous + 1].start;
            }
        }
        slots[0].start
    }
}

fn insert(
    settings: &MaskSettings,
    layout: &MaskLayout,
    mut cells: Vec<Option<String>>,
    range: Range<usize>,
    text: &str,
) -> Result<(Vec<Option<String>>, usize), String> {
    if text.len() > MAX_MASK_VALUE_BYTES {
        return Err("Mask input exceeds 16 KiB".into());
    }
    let start = layout
        .slots
        .iter()
        .position(|slot| slot.end > range.start)
        .unwrap_or(cells.len());
    for (slot, cell) in layout.slots.iter().zip(&mut cells) {
        if slot.start < range.end && slot.end > range.start {
            *cell = None;
        }
    }
    // A full raw value replacing all data must use the same optional-slot
    // assignment as format/storage loading. Partial editing keeps its slots.
    if start == 0
        && cells.iter().all(Option::is_none)
        && let Ok(assigned) = settings.storage_cells(text)
    {
        let next = assigned
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1);
        return Ok((assigned, next));
    }
    // A copied complete display may contain unfilled optional positions.
    // Prompts are parsed as holes, never as data; a prompt-only keystroke is
    // not a clear command and an incomplete display is not a raw value.
    if start == 0
        && cells.iter().all(Option::is_none)
        && let Ok(assigned) = settings.display_cells(text)
        && assigned.iter().any(Option::is_some)
        && settings.parsed(&assigned).complete
    {
        let next = assigned.iter().rposition(Option::is_some).unwrap() + 1;
        return Ok((assigned, next));
    }
    // Prefer raw data. A literal that is also a valid input character must not
    // silently consume it; formatted input is a fallback after raw rejection.
    insert_mode(settings, cells.clone(), start, text, false)
        .or_else(|_| insert_mode(settings, cells, start, text, true))
}
fn insert_mode(
    settings: &MaskSettings,
    mut cells: Vec<Option<String>>,
    start: usize,
    text: &str,
    formatted: bool,
) -> Result<(Vec<Option<String>>, usize), String> {
    let mut input = text;
    let mut slot = 0usize;
    let mut inserted = 0usize;
    for atom in &settings.pattern.atoms {
        match atom {
            Atom::Slot { class, required } => {
                if slot < start {
                    slot += 1;
                    continue;
                }
                if input.is_empty() {
                    break;
                }
                let glyph = graphemes(input)[0];
                if class.accepts(glyph, settings.placeholder) {
                    cells[slot] = Some(glyph.into());
                    input = &input[glyph.len()..];
                    inserted += 1;
                } else if *required {
                    return Err("Input does not match the mask slot".into());
                }
                slot += 1;
            }
            _ if formatted && slot >= start => {
                let literal = match atom {
                    Atom::Literal(text) => text.as_str(),
                    Atom::Separator(separator) => separator.text(settings.culture),
                    _ => unreachable!(),
                };
                if let Some(rest) = input.strip_prefix(literal) {
                    input = rest;
                }
            }
            _ => {}
        }
    }
    if !input.is_empty() || inserted == 0 {
        return Err("Input does not fit the mask".into());
    }
    Ok((cells, slot))
}
