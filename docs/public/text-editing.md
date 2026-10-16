# Plain text editors

`Controls::edit_box` creates a single-line editor. `Controls::text_box` creates
a multiline editor with wrapped text. `Controls::memo_edit` keeps a multiline
editor in a reusable popup, with a compact first-line preview in the form.
These controls share `TextDocument`, editor settings, validation and draft/value
transactions. Their text geometry comes from the same shaped layout used to paint.

## Selection, navigation and scrolling

Text positions are UTF-8 offsets at extended grapheme boundaries. Combining
marks, emoji sequences, flags and CRLF remain whole during navigation, selection,
deletion and clipboard replacement. Left/right movement follows visual bidi
order; Ctrl+Left/Right follows Unicode word boundaries. Ctrl+Backspace/Delete
removes a word, or the existing selection, as one undoable edit.

In multiline controls, Home/End use the current wrapped visual line;
Ctrl+Home/End use the document edges. Up/Down and PageUp/PageDown retain the
desired horizontal position across short lines. Shift extends the selection.
The lower-level `TextDocument::move_home` and `move_end` operate on hard lines.

The standard Ctrl+C/X/V shortcuts and Ctrl+Insert, Shift+Insert and Shift+Delete
produce host clipboard requests. Copy and navigation remain available in a
read-only editor; cut, paste, typing, IME commit and user undo/redo are blocked.
Clipboard transfer is performed by the window host.

A multiline editor with overflowing text has a vertical scrollbar and accepts
the mouse wheel. At a scroll boundary, wheel input can continue to the enclosing
ScrollViewer. Dragging the thumb keeps the document selection unchanged.
`editor_scroll_offset` returns the logical offset; `scroll_editor_to` clamps a
finite offset to the currently arranged content. Manual scrolling persists across
repaints. Typing, navigation, selection and focus return the caret to view.
Single-line editors use horizontal caret scrolling.

`TextDocument` retains its grapheme index across navigation and rebuilds it after
text changes and undo/redo. `clear_history` discards undo/redo snapshots while
preserving text, selection and active composition. `undo_text` and `redo_text`
inspect the next history target without moving the history cursor.

## Drafts, acceptance and composition

`TextChanged` describes a draft change. `accepted_value` changes only through
successful `commit_edit` or application `set_editor_value`. The default
`commit_on_focus_lost` attempts validation when focus leaves the editor session;
invalid text retains its draft and does not trap focus. `cancel_edit` restores
the accepted text and clears undo/redo so discarded input cannot reappear.

IME preedit is separate from the committed document and undo history. Committing
replaces the original selection in one undo step. A rejected commit ends the
composition while preserving committed text, selection and history. Native
candidate-window placement and assistive technology behavior require host-level
verification; document tests alone do not establish those results.

## Memo popup

```rust
use rust_desktop_ui_controls::{Controls, MemoEditSettings, Theme};
use rust_desktop_ui_core::{LayoutStyle, Length};

let mut controls = Controls::new(Theme::FluentLight)?;
let memo = controls.memo_edit(
    controls.root(),
    "Notes",
    "First line\nSecond line",
    MemoEditSettings::default(),
    LayoutStyle { width: Length::Px(280.0), ..Default::default() },
)?;
controls.set_label(memo.apply_button, "Apply")?;
controls.set_label(memo.cancel_button, "Cancel")?;
# Ok::<(), rust_desktop_ui_controls::ControlError>(())
```

`MemoEdit` exposes stable `container`, `editor`, `popup`, `apply_button` and
`cancel_button` IDs. Text bindings and editor settings target `editor`; the
container provides the compact disclosure and accessible name. `memo_editor`
resolves that editor from the container. The preview uses at most 80 complete
graphemes from the first line and adds an ellipsis when more text follows.

Opening and closing preserve the document, editor ID, selection and draft.
Enter inserts a newline. Apply or Ctrl+Enter validates the draft and closes after
successful acceptance. Invalid input remains open. Asynchronous validation keeps
the popup open until the same draft is accepted; changing the draft invalidates
the pending result. Disabling/hiding the owner, removing it or losing window
activation prevents pending acceptance.

Cancel or Escape restores the accepted value and closes. With active IME
composition, the first Escape cancels only preedit. Moving focus to the popup's
Apply or Cancel button does not accept the draft. Closing outside the popup uses
the configured commit-on-focus-loss policy; invalid or explicitly uncommitted
drafts remain available when reopened.

A read-only memo can open, select and copy. Its Apply button is disabled and
Escape/Cancel closes without changing a retained draft. Application code can
still explicitly call `commit_memo` or `cancel_memo`, matching the editor's
application-owned transaction APIs.

The default popup size is 480×300 logical pixels. `MemoEditSettings::popup_size`
accepts width 160–16,384 and height 120–16,384. Placement clamps to the window;
settings changes preserve editor state. Remove the memo container to dispose
the whole composite; required parts cannot be removed individually.

Compiled XAML accepts the usual text editor settings and popup dimensions:

```xml
<MemoEdit Name="Notes" Text="{Binding Notes}"
          Width="280" PopupWidth="480" PopupHeight="300"
          MaxLength="2000" CommitOnLostFocus="True" />
```

## Input and rendering limits

The document's default limit remains 1 MiB of UTF-8 text, with an optional
`MaxLength` in complete graphemes. Input is rejected as a whole. Preedit may
exceed `MaxLength`, allowing a candidate to be composed, but not the byte cap.

Controls also check text shaping before accepting initial values, programmatic
updates, pasted/typed text and IME display or commit. `TextEngine::validate_run`
checks the same layout budgets as preparation without creating glyph bitmaps;
a successful ordinary layout is cached for painting. One layout supports at most
65,536 shaped glyphs and 262,144 shaping clusters. A value can therefore fit the
document byte limit and still be rejected by the editor's shaping limit.
Each extended grapheme is limited to 32 Unicode scalars and 128 UTF-8 bytes,
checked before shaping, normalization or font fallback. This bounds unusually
long combining-mark and joiner sequences as well as ordinary editor input.
Rejection preserves the accepted value and draft. Oversized user input is ignored;
application setters return an error.

These controls shape the full document. They do not provide virtualized file
editing, incremental text shaping, rich-text editing or platform-specific text
services beyond the host's existing IME/clipboard/accessibility integration.
Glyph bitmap and aggregate frame budgets still apply independently of layout
validation; see [text preparation](text.md).
