# Rich documents and editing

`rich_document::RichDocument` owns paragraphs, styled text runs, rectangular
tables, embedded images, named run styles and page settings. The model is
independent of windows and GPU resources. UTF-8 selection boundaries must be
extended grapheme boundaries. Each paragraph contains at most 256 runs;
documents contain at most 4 MiB of text, 4096 blocks and 32 MiB of embedded media.

`transact` validates the entire candidate before publication. Paragraph
replacement and formatting preserve unaffected runs; `map_styles` changes
selected attributes without replacing the other typography. Block fragments
carry referenced media and styles. Conflicting resource names fail atomically.
The versioned JSON codec rejects unknown fields and invalid references.

`paginate` uses `TextEngine` for system-font shaping, bidirectional text and
wrapping. Paragraphs break between measured lines. Table rows and image blocks
are indivisible; an object taller than the content area produces an explicit
error. The result contains pages, renderer-independent scenes, source ranges,
actual font names and missing-glyph counts. At most 512 pages are produced.
Requested fonts remain unchanged when a fallback face is used. These are
screen-layout pages; printer integration is outside this API.

## DOCX profile

`from_docx` and `to_docx` read and write real ZIP/OPC WordprocessingML packages.
They support paragraphs; explicit font family, size, bold, italic, underline and
RGB color; alignment and after spacing; text tabs and line breaks; page breaks
before paragraphs; fixed rectangular tables with one paragraph per cell; and
standalone PNG/JPEG image blocks with alternative text. Page dimensions and
individual margins are preserved. Referenced paragraph run styles are resolved.

Import returns diagnostics for reductions such as optional style metadata and
table decoration. Unsupported active content, including tracked changes, fields,
hyperlinks, nested or merged tables, floating images, images mixed with text,
headers, footers, multiple sections and unsupported paragraph properties, fails
explicitly. External relationships are never fetched. Macros are rejected.
Export reports font-size rounding to half points. This is a bounded profile;
it does not promise arbitrary DOCX round-trip fidelity.

Package input and total expanded data are limited to 64 MiB and 512 entries.
Unsafe or duplicate paths, corrupt entries, missing relationships and unsupported
namespaces fail before publication. No ZIP entries are extracted to disk.
The codec follows the
[Office Open XML specification](https://ecma-international.org/publications-and-standards/standards/ecma-376/)
and the
[WordprocessingML document structure](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document).

The `rich_document_codec` example accepts explicit input/output paths:

```text
cargo run -p rust-desktop-ui-controls --example rich_document_codec -- sample example.docx
cargo run -p rust-desktop-ui-controls --example rich_document_codec -- roundtrip example.docx result.docx
```

## RichEditor and Code view

`rich_editor::RichEditor::mount` creates an editable paragraph surface alongside
a paginated formatted preview. Previous/next navigation includes table cells.
The preview is read-only; it is not an inline multi-page editing canvas.
Formatting commands retain the selection and the other attributes of each run.
Undo/redo stores document content, formatting and selection together, with a
64-entry/64-MiB history budget.

The text surface uses the existing TextBox for grapheme and bidi navigation,
selection, scrolling, clipboard and composition. Formatting, document replacement
and view changes reject active preedit; transient composition never changes the
document. Clipboard uses the host's plain-text route. Rich fragments are a model
API, not an operating-system rich clipboard format.

Code view edits JSON. Apply validates parsing, references and complete pagination
before replacing the document as one undoable operation. A failed Apply leaves
both the code draft and accepted document intact. Returning to Document without
Apply discards the code draft. Code text uses the existing 1-MiB editor limit.
Document mutation and document undo/redo are blocked while Code is open; ordinary
Code text undo remains available. Formatting and document-history toolbar actions
return keyboard focus to the editor. Preview page navigation keeps its own focus.
The formatted preview shrinks to fit its area and never enlarges above 1:1.

Applications forward `handle_event` before normal input dispatch, pass activated
widget IDs to `activate`, call `sync` after text changes, and call `refresh` after
layout or resize. Shared event queues are never drained by the component. DOCX
file dialogs, file access and save policy belong to the application.

The editing surface exposes ordinary text semantics and the preview has a
descriptive name. Document-level screen-reader navigation, rich clipboard,
native CJK composition and multi-page inline caret navigation require separate
integration and validation; model tests do not establish those behaviors.
