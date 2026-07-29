[中文说明](architecture.zh.md)

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
| `ui/` | Top tab bar, page layout, visual states, user gestures, generated callback contracts | Filesystem access, Tokio tasks, russh handles |
| `src/app.rs` | Slint setup, domain-to-row mapping, callback wiring, event-loop updates | SSH protocol details or JSON schema details |
| `src/app/` | UI-independent workspace tabs, per-tab terminal/worker state, attempt transitions, group aggregation, and blocking credential task boundary | Generated Slint component/model types |
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
   tab ID, group name, terminal key/modifier tuple, draft fields, a trust
   decision, or one transient password.
2. Opening a profile or local shell always creates a new terminal tab UUID,
   even when another tab uses the same target. SSH input, resize, output, retry,
   and close operations route by `tab_id + attempt_id`; local operations route
   by `tab_id`. An unknown SSH host starts a cancellable probe tied to that tab
   while transport remains rejected.
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
   cursor mode. Output for inactive tabs stays in Rust state; only the active
   cell snapshot crosses the Slint event loop. UI updates use
   `slint::invoke_from_event_loop` and `Weak<AppWindow>` so shutdown does not
   keep a window alive.
9. On macOS, the application bridge enables full-size title-bar content but
   disables AppKit's movable-window-background behavior. Slint reports a
   mouse-down only from the empty zero-tab strip or dedicated trailing space,
   and the UI-thread callback hands the current event to
   `NSWindow::performWindowDragWithEvent`. Tabs, the activity bar, sidebar,
   and terminal never invoke that callback.
10. The activity-bar Settings and About intents open one singleton Settings
    workbench view at General or About respectively. The bridge omits that
    internal singleton from the visible workspace-tab model, and Slint replaces
    the tab strip with a drag-only title-bar region while Settings is active.
    Unsaved drafts stay in Slint while pages change; only the header Save action
    crosses the application boundary. About presents a static product-purpose
    description and receives the compile-time package version as read-only UI
    metadata.

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
and floor-based PTY dimensions.

`SessionStore` writes a versioned `settings` object to the existing private
`sessions.json`. It contains normalized font, size, line height, color scheme,
brightness, bold-color and right-click behavior, scrollback, default PTY
dimensions, local-shell choice and bounded discovered-shell cache, sidebar/tab
widths, and shortcuts. Shell discovery validates the saved cache and appends
only newly available names after load. Older settings migrate during
deserialization; schema version 7 replaces only the previous 260px sidebar
default with the compact 220px default and preserves custom widths. Passwords,
passphrases, private-key contents, terminal output, tab runtime IDs, child
processes, and workers are never serialized.

The session sidebar participates in layout only when the session model is not
empty and the user has not collapsed it. Local Shell, Settings, About, and
new-session activity-bar actions remain available in the empty state.

Static interface styling is configured only by semantic tokens in
`ui/theme.slint`: palette roles, type scale, spacing, radii, standard workspace
geometry, Settings control dimensions, editor widths, and overlay sizes.
`ui/components/settings-controls.slint` consumes those tokens to provide the
shared Settings glyph, navigation, page, compact right-aligned field, row,
toggle, shortcut, and action header primitives. Setting rows keep a stable
title and metadata column while standard controls use one theme-configured height.
Runtime terminal geometry and user choices remain in versioned
`AppSettings`; the Theme global is visual configuration, not mutable domain
state.

## Staged scope

The current application validates and persists profiles, confirms per-profile
host fingerprints, authenticates with transient passwords or local private
keys, and owns multiple independent SSH or local tab-scoped PTY shells,
including duplicate targets. New-session editing remains a workspace tab;
Settings is a singleton workbench view outside the visible tab strip, and only
short-lived trust and secret prompts remain overlays. The following remain
separate steps:

- shared OpenSSH-compatible known-hosts storage and host-key revocation;
- SFTP as a separate worker sharing an authenticated transport policy;
- SSH agent integration, reconnect, and persisted workspace restoration;
- richer full-screen terminal compatibility and mouse reporting.
