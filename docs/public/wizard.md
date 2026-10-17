# Wizard and StepProgress

`Controls::wizard` mounts a bounded workflow as retained pages with Next, Back,
Finish and Cancel actions. An application supplies `WizardModel`, whose stable
step IDs form a directed acyclic graph. Every step is reachable from the start;
only final steps have no outgoing targets. Back follows the accepted route,
including the actual branch taken.

Actions create `WizardRequest` values. Drain requests, validate application data
outside UI/model borrows, then call `complete_wizard` with `Step`, `Reject`,
`Finish` or `Cancel`. The completion must match the immutable request intent and
declared targets. A Back request cannot finish the workflow. Invalid completions
leave the current page and pending request intact; stale completions return
`WizardCompletion::Stale`. Finish and Cancel emit distinct, exactly-once results.

Requests carry owner identity, generation and application context version.
`set_wizard_context_version` invalidates old work when external validation data
changes. Edits, input in the active page, window blur, disabling the owner and
removal also cancel pending work. Cancel may replace another pending request;
Next, Back and Finish do not overlap. Active IME composition blocks transitions,
including an editor in an owned flyout. Escape first cancels composition.

Pages are mounted once. Successful transitions preserve editor IDs, drafts,
selection and undo history; footer actions do not implicitly accept an ordinary
editor. Applications decide when their business data is committed. Validation
failure keeps the current page and its input. `reset_wizard` explicitly returns
to the start and clears the workflow result, while preserving page data. Public
tab-selection APIs cannot bypass the workflow guard.

`StepProgress` is a separate read-only indicator with Upcoming, Current,
Completed, Error and Skipped states. `set_step_progress_value` updates standalone
indicators atomically and preserves item IDs by key. It cannot change an indicator
owned by a Wizard. The current item is revealed in a horizontal viewport; items
do not expose activation actions. Current step, ordinal, state and validation
message are available through accessibility semantics.

Keyboard: Alt+Right requests Next, Alt+Left requests Back, Ctrl+Enter requests
Finish, and Escape requests Cancel when available. Ordinary Tab navigation
remains available within the current retained page and action bar.
The action bar wraps at narrow widths, and resizing the progress viewport
reveals its current step without replacing the retained indicators.

Compiled XAML supports `Wizard` with `StartStep`, `ShowProgress`, `AllowCancel`
and `IsReadOnly`, and `WizardStep` with `Id`, `Header`, comma-separated `Next`,
`IsFinal` and exactly one content element. Named content and its bindings remain
scoped to the mounted form. Current-step selection is controlled by application
requests, not a writable selection binding. Standalone `StepProgress` uses
`CurrentStep` and `Step` metadata with `Id`, `Header` and `State`.

Bounds: 64 steps, 32 KiB workflow metadata, 128 queued events, 128-byte IDs,
128-grapheme/512-byte titles and 512-grapheme/2048-byte validation messages.
Generated captions pass the same shaping limits as other controls before
publication. Cyclic workflows, automatic persistence and application-specific
validation or I/O are outside this contract.

The `xaml-form --wizard` example contains local and remote branches, delayed
validation, explicit reset, read-only mode and a cancelled late response.
No network connection or filesystem change is made by the example. Its request
deadline participates in the application's wake and redraw schedule, so a guard
completion does not require another mouse or keyboard event.
