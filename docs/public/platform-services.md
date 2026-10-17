# Native application services

`DesktopApp::platform_services_changed` receives a window-scoped
`PlatformServiceHandle`. The handle can be cloned into workers. `submit` queues
an owned `PlatformServiceRequest`; the host calls `platform_service_completed`
on the UI thread. Shell actions arrive through `platform_service_event`.
Both callbacks return the usual `EventResponse`. A service error is data and
does not terminate the application.

`capabilities()` reports compiled support. An available service can still return
`Busy`, `NotReady`, `Native` or `OwnerClosed`. The structured native services
currently have Windows implementations. Other systems return `Unsupported`;
the existing text-only `ClipboardRequest`/`PlatformEvent::Paste` path remains
available through its previous platform adapter.

The dispatcher accepts a numeric channel and at most 64 KiB of opaque bytes.
The native host delivers these bytes without interpreting them or executing
worker closures. There are at most 64 queued requests and 24 MiB of logical
payload. These limits count string/vector lengths, not allocator capacity.
Each UI turn processes at most 64 completions and 128 shell events. `cancel`
returns true only if it removed a request before execution; cancelled requests
produce no completion. Synchronous OS operations already running cannot be
undone. Closing or suspending the owner revokes its handles and queued work.
Request identities are private to that lifetime, even if Windows reuses a HWND.
Locks are released before native calls, wakers and application callbacks.
If Windows cannot install the owner's native lifetime observer, the handle is
revoked before its first delivery to the application. Submission then returns
`OwnerClosed`; no request is accepted without that observer, including dispatch.

## Clipboard

`ClipboardContent` contains optional Unicode text, an HTML fragment and an
RGBA8 image. An absent format differs from empty text. At least one format is
required for a write. Limits are 1 MiB of UTF-8 text, 4 MiB of HTML and an image
up to 2048×2048 pixels with exactly four bytes per pixel. Embedded NUL is
rejected. Diagnostic `Debug` output omits clipboard and dispatch content.

Windows text uses `CF_UNICODETEXT`. HTML uses the registered `HTML Format`
with validated UTF-8 byte offsets; fragments are returned as data and never
executed. The reader supports bounded ASCII headers with explicit context and
fragment offsets. Images use a 124-byte DIBV5 header, 32-bit BGRA bitfields,
sRGB, explicit alpha, no palette and no embedded/external color profile.
Top-down and bottom-up images are read. Other image layouts return
`Unsupported`; malformed dimensions, offsets and truncated storage are errors.

All outgoing buffers are prepared and zero-initialized before opening and
clearing the clipboard. The OS does not provide a transaction across successive
`SetClipboardData` calls: a native failure after clearing can leave only some
new formats. The error reports `clipboard_formats_written`; it never reports
success or promises restoration of previous content. Incoming allocation
slack is bounded to 64 KiB beyond the format limit, and only the bounded logical
prefix is copied. No delayed-rendering callback retains a closed owner.

## Shell

`SetTray` installs or replaces one icon per owner window. The icon is at most
256×256 RGBA pixels. A tooltip has at most 127 UTF-16 code units; a menu has at
most 16 uniquely identified, enabled/disabled actions with 64-unit labels.
Activation and selected actions are separate events. Replacing/removing an
icon discards its queued signals; owner destruction removes it. The observer
handles shell recreation and negotiates notification-icon version 4.
The service does not change autostart, window visibility or minimize policy.

Taskbar services expose none/indeterminate/normal/error/paused progress and an
optional overlay icon. Determinate progress requires a nonzero total and
completed ≤ total. Operations before `TaskbarButtonCreated` return `NotReady`.
Progress and overlays are cleared when the service owner is retired. Explorer
recreation makes the button temporarily unready; the application can reapply
its current taskbar state after a successful later request.

Jump List publication is explicit. `PublishJumpList` takes a caller-owned
application ID and up to 16 tasks. IDs contain 1–128 ASCII letters, digits,
periods, underscores or hyphens; task titles are 1–128 UTF-16 units and arguments
are 1–2048 units. Each task targets the current executable and its icon; no
arbitrary executable or shell command is accepted. Only the Tasks category is
included. The service does not register file handlers, change process defaults,
pin an application, or populate recent/custom destination lists.

The first successful publication binds the ID to this owned window. Later
publications require the same ID. The original window property is restored on
failure/retirement if it still has the value installed by the service; a newer
external property is preserved. Shell list publication uses Begin/Commit/Abort.
The window property and persistent shell list are distinct OS effects, not a
general distributed transaction. `DeleteJumpList` explicitly removes a list
for the caller-owned ID. Closing a window does not delete the persistent list.
A Jump List task is an OS launch declaration, not an in-process callback; it can
launch the application after its previous process has ended.

## Example and verification

Run `cargo run -p rust-desktop-ui-platform-winit --example platform_services`.
The window lists explicit keyboard commands for tray, taskbar, Jump List and
worker dispatch. Clipboard commands are disabled unless the process was started
with `--clipboard-fixture`. The example never prints clipboard contents. Use
the delete and remove commands to clean up a deliberately published fixture.

Unit tests cover byte/format bounds, Unicode offsets, orientation/alpha,
queue limits, cancellation, checked identity exhaustion, concurrent posting,
owner revocation and a reentrant waker. These tests do not replace native shell
or clipboard checks. Windows availability and visual behavior must be checked
on the actual desktop. Linux/macOS rich clipboard and shell implementations
and native verification remain outside the implemented profile.
