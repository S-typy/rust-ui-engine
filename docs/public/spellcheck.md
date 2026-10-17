# Spelling requests and editor adapters

`ui_text::spellcheck` checks explicitly supplied words and returns typed spelling
issues. It does not bundle a language dictionary, install a service, create a
worker or change text automatically. Applications supply a `SpellProvider`, run
it directly or on their own worker, and deliver the result to a `SpellSession`
or the Controls adapter.

`SpellSession::request` captures the exact committed text of a `TextDocument`,
its opaque `DocumentVersion`, language and comparison policy. A request has a
unique session/request identity and a cooperative cancellation flag. A newer
request, policy change, cancellation or session drop cancels outstanding work.
`synchronize` also observes direct document changes. The text version changes on
edits, undo and redo. Cloning creates a new identity, so restoring a previous
copy cannot resurrect an outstanding check. Caret movement and IME preedit do
not change committed text; composition nevertheless blocks requests, completion
and replacement.

`complete` rejects foreign, cancelled or outdated requests before inspecting
their payload. A result is a list of ordered, disjoint `SpellIssue` ranges in
UTF-8 bytes, exact words and suggestion strings. Every range must match a whole
Unicode word and extended grapheme boundaries in that request. Malformed current
results return an error while preserving the pending request for a corrected
retry. Provider failures terminate the current request and expose a bounded
diagnostic. A stale completion never changes current ranges or diagnostics.

## Dictionaries and reproducible policy

`LexiconProvider` is a small, deterministic exact-word provider. Applications
supply `LexiconMetadata` with a language identifier, dictionary ID, origin and
license, plus the words. Metadata records the application's declared provenance;
it is not automatic legal verification. The repository examples and tests use
handwritten words under the project's Apache-2.0 license. No external dictionary
or platform spelling engine is distributed or downloaded.

Language is an explicit case-sensitive identifier, such as `ru-RU`; there is no
automatic language detection or fallback. Its syntax is bounded alphanumeric
segments separated by hyphens, not a complete language-tag registry validator.
Word boundaries use the existing `unicode-segmentation` Unicode word algorithm.
`Ordinal` compares exact strings; `UnicodeLowercase` compares Unicode lowercase
strings. Neither performs canonical normalization, locale collation, morphology,
grammar correction or language-specific tokenization beyond those boundaries.
Settings explicitly control ignoring all-numeric and all-uppercase words.

Ignore-once applies to one exact range in one document version. Ignore-all and
custom words are separate, bounded in-memory sets keyed by language and
comparison policy. Applications decide whether and how to persist their own
custom dictionaries. Changing such a policy invalidates current results and
requires a new explicit request; no filesystem write occurs.

Suggestions use extended-grapheme Levenshtein distance at most two, ordered by
distance and then ordinal original spelling. They are proposals, not trusted
editor commands. The bounded profile is:

- At most 64 KiB of request text, 4,096 words and 256 reported issues.
- At most 64 graphemes and 512 UTF-8 bytes per word or suggestion; at most eight
  distinct suggestions per issue.
- At most 16,384 submitted lexicon entries, counting duplicates.
- A two-million-unit work budget covering normalization/candidate byte scans
  and distance-matrix cells. Exceeding it returns an error, not a partial result.
- At most 16 language/comparison policies and 4,096 total ignore/custom words per
  session; ignore-once retains at most 256 ranges.

## Plain-text editor integration

`Controls::set_spellcheck` attaches a session to a plain TextBox or Memo owner.
`request_spellcheck`, `complete_spellcheck`, `spellcheck_issues` and
`spellcheck_error` form the explicit request/result interface. Ignore and custom
word operations are `ignore_spelling_once`, `ignore_spelling_all` and
`add_spelling_word`. `cancel_spellcheck` cancels outstanding work. At most 128
editor sessions may be attached to one Controls instance.

`apply_spelling_suggestion` verifies the exact draft revision, availability and
absence of composition before constructing the full candidate. Grapheme range,
normalization, MaxLength, UTF-8 byte and shaping limits are checked before any
change. The replacement is one ordinary undoable edit, preserves the previous
selection in undo history, emits the ordinary TextChanged event and leaves the
accepted business value untouched. Existing bindings retain their configured
update trigger; a PropertyChanged binding can therefore publish the new draft.
Enter, explicit commit, focus-loss policy and Memo Apply/Cancel remain the
editor's responsibility.

Read-only, disabled, hidden or removed editors cannot publish results or apply
suggestions. A Memo's retained editor must be open and available. Password,
masked and typed composite editors are rejected. The service supplies ranges;
applications choose how to present them. Automatic squiggle painting, a spelling
context menu and a general document-wide review dialog are not supplied.

Run `cargo run -p rust-desktop-ui-text --example spellcheck` for an application
worker, exact UTF-8 ranges, correction, one-step undo and stale-result rejection.
`xaml-form --spellcheck` demonstrates the same flow in a compiled form using
ordinary existing controls, including an explicit Memo correction action. Its
small dictionary is a demonstration of the contract, not a Russian dictionary.
