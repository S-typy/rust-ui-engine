# Dialogs and file selection

The native host provides owned message dialogs and file pickers on Windows.
Applications enqueue a `DialogRequest` through `DesktopApp::take_dialog_request`
and receive exactly one `DialogCompletion` with the same ID. A completion contains
`Cancelled`, a selected `DialogButton`, selected `PathBuf` values, or a typed
`DialogError`. Cancellation is an ordinary outcome and must not overwrite a
previous application value.

`MessageDialog` supports Ok, OkCancel, YesNo, YesNoCancel and RetryCancel button
sets. The default button must belong to the selected set. Labels and presentation
come from Windows. Escape/Close follow that button set's native policy; YesNo
requires an explicit choice.

`FileDialog` supports Open, Save and Folder selection. Open can select multiple
files. Filters contain a label and extension names without the leading dot;
`*` selects all files. The initial filter index is zero-based. Optional initial
directories must be absolute. Invalid options fail before opening a window or
changing focus, so they do not commit an editor draft.
Windows uses the Common Item Dialog and filesystem-only results; paths preserve
the native UTF-16 representation. Save confirms replacement of an existing file
but does not write it. File reading, creation and persistence belong to the
application. The service neither changes the working directory nor adds selections
to the recent-files list.

Dialogs run on the host UI thread outside paint/application callbacks. The native
modal loop temporarily blocks ordinary application event processing. The host
applies the editor's normal focus-loss policy before opening, then restores the
window focus state on return. Concurrent requests should be queued by the
application. Other platforms currently return `DialogErrorKind::Unavailable`;
they do not silently substitute a different picker or a shell command.

## Application-rendered modal windows

Use `run_multi`, set `WindowSpec.owner`, and set `WindowSpec.modal = true` for a
retained GPU dialog with arbitrary controls. Owner and modality are fixed for the
window lifetime. Nested modal windows are supported. A missing owner, ownership
cycle, or two immediate modal children of one owner is rejected before changing
native windows.

The host blocks user input and accessibility actions in the owner family, while
continuing updates and rendering. Windows additionally disables those native
windows. Removing the modal restores the owner's enabled state and restores
native focus when the closed dialog held it. If several nested dialogs close
together, focus returns to the first surviving owner. Views remain available until
`window_closed`, including during owner shutdown. The application closes dependent
dialogs when it closes their owner.

`DialogSession<T>` provides an exactly-once result with optional default and
required cancel values. `accept_default`, `cancel`, and `owner_closed` resolve it;
`take_result` delivers the result once. Map Enter/Escape and buttons to these
operations after the application's validation policy. A result does not itself
commit business data. The `--workspace` example accepts valid text on Enter,
keeps invalid required text open, and leaves the previous result untouched on
Escape or native Close.

The examples are `WorkspaceWindow.xaml`, `DialogWindow.xaml`, and
`src/workspace_app.rs`. Native file/message presentation follows Windows;
application-rendered dialogs follow the selected UI theme.

Windows integration uses the documented [Common Item Dialog](https://learn.microsoft.com/en-us/windows/win32/shell/common-file-dialog)
and [MessageBox](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-messageboxw)
system APIs.
