# Spreadsheet

`ui_data::spreadsheet::SpreadsheetSession` owns a Workbook, active sheet, range
selection and bounded undo/redo history. `ui_controls::spreadsheet::Spreadsheet`
mounts a retained formula editor, toolbar and virtual cell viewport. The host
routes `input`, `activate` and clipboard requests, then calls `refresh` after
layout. Run the example with `cargo run --locked -p xaml-form -- --spreadsheet`.

The formula bar accepts typed values and formulas. Enter/F2 focuses it; Apply
commits; Escape restores the selected cell's value. A leading apostrophe forces
text during explicit entry. External TSV paste keeps formulas and leading
apostrophes as text by default. Existing IME composition blocks commands that
would replace the draft. Resize retains the editor and its draft.

Arrow keys move the active cell, Shift extends the selection, PageUp/PageDown
move through rows, and the wheel scrolls (Shift selects the horizontal axis).
Click selects a cell; Shift-click extends the range. Intersected merged ranges
are included in full and editing targets their anchor.

Merge rejects overlapping ranges and nonempty non-anchor cells. Unmerge
preserves the anchor. Copy snapshots cells and merges; typed paste and fill
translate relative formula references while preserving absolute references.
Fill down/right rejects merged selections until explicitly unmerged.
All workbook changes publish atomically and participate in undo/redo, bounded
to 64 snapshots and an estimated 64 MiB.

Ctrl+C/V requests host-owned text clipboard I/O. Ctrl+Shift+V uses the session's
typed copy, including formulas, styles and merges. A delayed clipboard response
is rejected after the selection or document generation changes. Text paste is
bounded to 4 MiB and 16,384 cells. File I/O belongs to the application; the
example loads/saves the [XLSX profile](workbooks.md) on workers and rejects stale
loads after newer edits.

The viewport draws only visible cells, with fixed column widths/row heights,
bold/italic text, wrapping, foreground/background and merged geometry.
Drag selection, a fill handle, adjustable column widths, full number-format
presentation and direct in-cell editing are not implemented. The formula editor
uses the shared accessible text control; the cell canvas does not yet expose
a Windows UIA Grid/Table provider. Native keyboard, IME, clipboard and screen
reader acceptance remains outstanding.

These are Rust composition APIs. There is no `<Spreadsheet>` compiled-XAML
element yet; the example mounts it inside a compiled XAML host.
