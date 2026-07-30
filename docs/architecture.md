[简体中文](architecture.zh.md) · [Documentation index](README.md)

# AxSSH Architecture

## Boundary

AxSSH is an independent Rust binary. The reference checkout at
`third_package/axshell` is deliberately outside the build graph. It may inform
product behavior and review questions, but no source file, type, or dependency
is imported from it.

The implementation keeps UI, application, persistence, transport, and process
services in separate ownership boundaries:

```text
Slint UI (.slint)
       │ generated callbacks / properties
       ▼
Application controller (src/app.rs)
       │ tab IDs + domain values + UI event-loop dispatch
       ├──────────────► Config store (src/config.rs)
       │                 versioned settings/profile JSON + atomic replace
       ├──────────────► Credential store (src/credentials.rs)
       │                 blocking platform keyring API
       ├──────────────► Terminal model (src/terminal.rs)
       │                 bounded vt100 grid + scrollback
       ├──────────────► Local PTY (src/local_shell.rs)
       │                 bounded thread + portable-pty process
       └──────────────► SSH boundary (src/ssh.rs)
                         Tokio tasks + russh handles/channels + key loading

Process startup (src/main.rs)
       └──────────────► Logging lifecycle (src/logging.rs)
                         rolling writer + flush guard
```

## Module responsibilities

| Area | Owns | Must not own |
| --- | --- | --- |
| `ui/` | Main composition, feature components, Settings category pages, visual states, user gestures, generated callback contracts | Filesystem access, Tokio tasks, russh handles |
| `src/app.rs` | Generated Slint type declaration, process-level UI startup, and top-level callback composition | Feature implementations, SSH protocol details, or JSON schema details |
| `src/app/macos_window.rs` | Main-thread AppKit title-bar setup and standard application-menu action binding | Generated Slint types, persisted settings, SSH or worker state |
| `src/app/{workspace,connection,connection_monitor,terminal_bridge,settings_bridge,view}.rs` | Private application-bridge feature wiring, worker-event consumption, and Slint model/snapshot mapping | Generated type declaration, transport implementation, or persistence schema |
| `src/app/state.rs` and `src/app/state/` | UI-independent workspace tabs, per-tab terminal/worker state, attempt transitions, and their tests | Slint component/model types or russh protocol details |
| `src/app/{input,session_groups,terminal_render,credential_tasks}.rs` | Testable input/group/render mapping and blocking credential task boundary | Window ownership, transport handles, or mutable UI state |
| `src/config.rs` | `SessionProfile`, versioned `AppSettings`, validation, legacy migration, JSON persistence, atomic replacement | Slint types, network connections, plaintext password storage |
| `src/credentials.rs` | Profile-scoped access to the platform credential store | UI state, plaintext configuration, SSH transport handles |
| `src/terminal.rs` and `src/terminal/input.rs` | Bounded vt100 grid, cell styles, cursor/scrollback state, selection extraction, and terminal key encoding | Slint types, network handles, credentials |
| `src/local_shell.rs` | Cross-platform shell discovery and one bounded worker-owned local PTY process per tab | Slint state, SSH trust, persisted terminal contents |
| `src/ssh.rs` | russh handler, host-key decision, authentication, shell channel boundary | Window updates, persistent session mutation, UI formatting |
| `src/ssh/private_keys.rs` | Local `.ssh` private-key discovery and blocking key loading | Passphrase persistence, UI state, host trust decisions |
| `src/ssh/worker.rs` | Bounded shell input commands, coalesced resize state, batched output events, cancellation, and shutdown | UI state or profile persistence |
| `src/logging.rs` | Global tracing subscriber, log directory, daily rolling writer, retention and flush guard | Credentials, feature state, UI or SSH handles |
| `src/main.rs` | Process startup and logging-guard lifetime | Feature logic |

## Event flow

1. A Slint callback produces a small value such as a saved profile ID, unique
   tab ID, terminal key/modifier tuple, draft fields, a trust
   decision, or one transient password.
2. Opening a profile or local shell always creates a new terminal tab UUID,
   even when another tab uses the same target. SSH input, resize, output, retry,
   and close operations route by `tab_id + attempt_id`; local operations route
   by `tab_id`. An unknown SSH host starts a cancellable probe tied to that tab
   while transport remains rejected. Workspace Tab order is in-memory
   presentation state: a drag completion passes a tab UUID and bounded target
   index to `AppState`, which reorders only the existing Tab list. While held,
   Slint keeps a translucent source slot, highlights the prospective target,
   and renders a non-interactive Tab copy at the pointer; it never creates a
   second runtime Tab. The leading UI ordinal derives from that list index,
   while an instance suffix such as `#1` remains part of the Tab's stable title.
3. After explicit confirmation, the controller atomically persists the exact
   fingerprint. Password profiles load a remembered credential on a Tokio
   blocking boundary or open a password prompt. Private-key profiles load the
   selected path off the UI thread and request a transient passphrase only when
   the encrypted key cannot be opened without one.
4. A password explicitly saved with a new profile is written together with
   that profile operation. A password entered in the authentication prompt is
   written only after SSH authentication succeeds. Missing or rejected stored
   credentials clear the non-secret marker and fall back to one manual prompt.
5. The terminal surface maps Slint special keys to UI-independent terminal key
   values and applies a narrow shifted-hyphen fallback when the platform still
   reports `-` for `Shift+-`. `src/terminal/input.rs` emits control bytes,
   normal CSI or application-cursor SS3 arrows, and xterm modified sequences.
   At the application boundary, macOS restores physical Control and Command
   after Slint's Apple mapping swaps their semantic modifier fields. A
   transparent, cursor-positioned `TextInput` is used only as the native IME
   proxy; committed text enters the terminal encoder while preedit remains UI
   state.
   Terminal Ctrl combinations take priority while the terminal is focused;
   `Ctrl+C` remains PTY input. Clipboard actions keep `Cmd+C/V` on macOS and
   `Ctrl+Shift+C/V` elsewhere. Workspace commands use the platform modifier.
   Selection copy remains local while paste becomes bounded shell input; the
   optional right-click action chooses between them based on selection state.
6. After authentication, each terminal tab owns one worker, and that worker
   exclusively owns one PTY shell plus its russh handle/channel. Bounded command
   queues and single-slot watched sizes remain independent between duplicate
   profile tabs. Closing a tab removes its routing state before asynchronously
   shutting down that worker, so late events cannot update another tab.
7. A local terminal tab instead owns one `portable-pty` worker thread. That
   worker owns its child, reader, writer, resize state, bounded command/event
   queues, cancellation flag, and timeout-bounded join for the tab lifetime.
8. Each terminal tab also owns one bounded `TerminalModel`. `vt100` owns the
   rows, cell styles, cursor, scrollback, wide characters, and application
   cursor mode. The checked-in `vendor/vt100` patch keeps its locked `0.16.2`
   API but clears a wide character whose continuation cell would be removed
   during a column shrink, for both normal and alternate screens. Output for
   inactive tabs stays in Rust state; only the active cell snapshot crosses the
   Slint event loop. UI updates use
   `slint::invoke_from_event_loop` and `Weak<AppWindow>` so shutdown does not
   keep a window alive.
   The small-screen window floor is `520x360`; terminal layout, persisted
   default sizes, and the model use the same non-zero `10x3` grid floor. This
   permits a compact window without ever issuing an invalid PTY resize. Users
   can collapse the existing session sidebar to reserve additional terminal
   columns on narrow displays.
   `TerminalPane` coalesces changes to its measured grid, configured font
   metrics, active terminal-tab identity, and connection state until the next
   UI turn, then requests one final PTY size. This keeps a Settings font
   change and a later return to a connected terminal on the same current-grid
   path as a window resize.
   Once a local or SSH worker accepts a resize request, the application resizes
   the active `TerminalModel` and schedules an active-terminal refresh. When
   that UI task executes, it copies the current snapshot from `AppState` rather
   than applying a snapshot captured by an earlier worker event. Therefore an
   already queued Output update cannot restore an older grid while the user is
   still dragging the window. The worker's later `Resized` acknowledgement
   remains transport confirmation only.
9. On macOS, AxSSH keeps the standard native title bar and disables
   movable-window-background behavior. AppKit alone owns window movement from
   that title bar; the Slint workspace Tab strip is regular client content
   immediately below it. This prevents native window dragging from competing
   with a Tab reorder gesture.
10. Platform-menu Settings and About intents open one singleton Settings
    workbench tab at General or About respectively. It remains in the visible
    workspace-tab model alongside running SSH and local-terminal tabs, so
    activating Settings never removes the route back to a live terminal. Its
    Close action removes only that singleton tab; it never affects a terminal
    worker. Unsaved drafts stay in Slint while pages change; only the header
    Save action crosses the application boundary. About presents a static
    product-purpose description and receives the compile-time package version
   as read-only UI metadata. The session sidebar does not duplicate Settings
   or About. It spans the full client height directly below the native title
   bar, while the workspace Tab strip occupies only the column to its right.
   Its `+` is pinned to the outer right edge and opens a Slint-local picker containing a
   masked, read-only snapshot of every saved SSH profile; selection routes only
   the profile UUID through the existing connection callback. The sidebar `+`
   and File > New Session remain the distinct session-editor action.
11. One declarative Slint `MenuBar` owns the cross-platform business-menu tree.
    The locked winit/muda backend installs it in the macOS screen menu bar and
    the Windows native window menu; Linux backends without native menu support
    render the same tree at the top of the client window. On macOS,
    `src/app/macos_window.rs` reuses the backend-created standard application
    menu, binds its existing About item to the internal page, and inserts
    `Settings...` with `Cmd+,`. The AppKit target is main-thread-only, captures
    only `Weak<AppWindow>`, and is retained by each menu item's represented
    object because AppKit target references are weak. The macOS close-tab item
    intentionally has no dynamic active-tab binding, so Muda does not rebuild
    the native menu when tab identity or kind changes; Settings and About are
    installed once through the AppKit bridge. Windows/Linux retain the dynamic
    close-tab enabled state, keep Settings in Edit, and keep About in Help. File,
    View, Pane, Window, and Help reuse existing new-session, sidebar, local-shell,
    close-tab, and shortcut intents.
12. The session navigator has one Slint-owned sidebar expanded/collapsed state
    and application-owned, in-memory group expansion state. `AppState` stores
    normalized expanded group names in a `BTreeSet`; that set is neither a new
    dependency nor persisted configuration. The expanded view renders a Local
    Shell card, then collapsible parent group rows and their single-line server
    children. The expanded parent shows its name, count, and a centered drawn
    down chevron; a collapsed parent shows the matching up chevron. The compact
    rail alone uses a two-character badge derived from the group name rather
    than a folder icon.
    A separate compact panel control is the only action that expands or
    collapses the sidebar. In the expanded sidebar it sits at the trailing
    edge of the Local Shell row; in the collapsed rail it remains a top control.
    Custom group rows are keyboard focusable and use
    Enter/Space for the same group-toggle intent as a click; this never changes
    the sidebar state. Server rows only connect. The collapsed rail renders a
    larger Group badge and smaller, tightly stacked server badges, while Local
    Shell keeps its dedicated entry. The application formatter masks usernames
    and IPv4 middle octets before data enters the Slint model. Static geometry
    is in `ui/theme.slint`; the persisted single-character mask setting is owned
    by `WorkspaceSettings`.

## SSH security contract

`russh::client::Handler::check_server_key` is the trust boundary. Unknown and
mismatched keys are rejected before authentication. A rejected first-contact
handshake may expose its SHA-256 fingerprint to the confirmation UI, but only
an explicit user decision adds that exact fingerprint to the profile. A changed
key requires a second explicit decision. Passwords are transient callback
inputs and are not part of `SessionStore`. The profile contains only a
`credential_stored` marker; the password itself is keyed by the stable profile
UUID in the platform credential store. Private-key profiles persist only a
path. The key bytes and optional passphrase are loaded in one blocking task,
used for one authentication attempt,
and then dropped without entering configuration, tracing fields, or UI models.

Authenticated connections follow this lifecycle:

- every terminal tab has a unique runtime UUID and one worker owns its russh
  handle for the full lifetime;
- the bounded command channel carries shell input, disconnect, and cancel
  intent; a watched terminal size coalesces high-frequency resize updates;
- terminal output is capped per batch and backpressured through a bounded event
  channel before entering the bounded terminal model;
- worker events report connected, resize, output, disconnected, host-key
  rejection, credential failure, or a capped error message;
- cancel interrupts connection/authentication as well as an established session;
- a 20-second keepalive with three missed-reply limit keeps healthy idle
  sessions open while retaining a 90-second inactivity bound;
- tab close invalidates the tab/attempt route before requesting worker shutdown;
- window shutdown requests disconnect for every remaining worker, waits for
  each join with a timeout, and only then shuts down Tokio.

## Logging lifecycle

`src/main.rs` creates exactly one `LoggingGuard` before constructing the UI and
keeps it alive until after the Slint and Tokio lifecycles finish. `src/logging.rs`
writes through a bounded non-lossy queue to daily UTC files, retains at most 15
files, and mirrors `INFO` and higher events to stderr. Dropping the guard writes
the shutdown event, drains the queue, flushes the active file, and joins the
writer thread. Operational fields may include session ID, host, port, and host
fingerprint; credentials and terminal contents are forbidden.

## Persistent settings and font resources

`assets/fonts/JetBrainsMono-Regular.ttf` is a project-owned static resource
registered by the Slint compiler. Its OFL license and author notice are kept in
the same directory. No font is loaded from `third_package/axshell` during build
or runtime. Slint measures the configured font and uses the measured cell width
plus the configured line-height percentage for rendering, selection, cursor,
and floor-based PTY dimensions; the terminal batches the resulting resize only
after those metrics and its layout have settled.

`SessionStore` writes a versioned `settings` object to the existing private
`sessions.json`. It contains normalized font, size, line height, color scheme,
brightness, bold-color and right-click behavior, scrollback, default PTY
dimensions, local-shell choice and bounded discovered-shell cache, sidebar/tab
widths, session mask character, and shortcuts. Shell discovery validates the
saved cache and appends only newly available names after load. Older settings
migrate during deserialization; schema version 7 replaces only the previous
260px sidebar default with the compact 220px default and preserves custom
widths. Schema version 8 adds the mask setting with `*` as its default.
Passwords,
passphrases, private-key contents, terminal output, tab runtime IDs, child
processes, and workers are never serialized.

The expanded session sidebar participates in layout only when the session model
is not empty and the user has not collapsed it. Otherwise the narrow rail keeps
Local Shell and new-session actions available. Settings and About remain in the
platform menu and shortcuts instead of the rail.

Static interface styling is configured only by semantic tokens in
`ui/theme.slint`: palette roles, type scale, spacing, radii, standard workspace
geometry, Settings control dimensions, editor widths, and overlay sizes.
`ui/components/settings-controls.slint` consumes those tokens to provide the
shared Settings glyph, navigation, page, compact right-aligned field, row,
toggle, shortcut, and action header primitives. Setting rows keep a stable
title and metadata column while standard controls use one theme-configured height.
`ui/settings.slint` owns the shared draft and one Save transaction, while the
category layouts live in `ui/settings/*.slint` with only their relevant draft
properties and callbacks.
Runtime terminal geometry and user choices remain in versioned
`AppSettings`; the Theme global is visual configuration, not mutable domain
state.

## Staged scope

The current application validates and persists profiles, confirms per-profile
host fingerprints, authenticates with transient passwords or local private
keys, and owns multiple independent SSH or local tab-scoped PTY shells,
including duplicate targets. New-session editing and the singleton Settings
workbench remain visible workspace tabs; only short-lived trust and secret
prompts remain overlays. The following remain
separate steps:

- shared OpenSSH-compatible known-hosts storage and host-key revocation;
- SFTP as a separate worker sharing an authenticated transport policy;
- SSH agent integration, reconnect, and persisted workspace restoration;
- richer full-screen terminal compatibility and mouse reporting.
