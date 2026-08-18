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
       ├──────────────► Config store (src/config.rs + src/config/)
       │                 versioned settings/profile JSON + atomic replace
       ├──────────────► Credential store (src/credentials.rs)
       │                 blocking system-keyring and encrypted-vault APIs
       ├──────────────► Terminal model (src/terminal.rs)
       │                 bounded vt100 grid + scrollback
       ├──────────────► Local PTY (src/local_shell.rs)
       │                 bounded thread + portable-pty process
       ├──────────────► SSH boundary (src/ssh.rs)
       │                 Tokio tasks + russh handles/channels + X11 relay + key/agent signing
       ├──────────────► SFTP domain (src/sftp.rs + src/sftp/)
    │                 bounded browser + worker-owned download/upload and edit operations
       ├──────────────► Telnet boundary (src/telnet.rs)
       │                 bounded TCP worker + RFC 854 parser + NAWS
       └──────────────► Serial boundary (src/serial.rs)
                         metadata discovery + bounded device worker

Process startup (src/main.rs)
       └──────────────► Logging lifecycle (src/logging.rs)
                         rolling writer + flush guard
```

## Module responsibilities

| Area | Owns | Must not own |
| --- | --- | --- |
| `ui/` | Main composition, feature components, Settings category pages, visual states, user gestures, generated callback contracts | Filesystem access, Tokio tasks, russh handles |
| `src/app.rs` and `src/app/window_router.rs` | Generated Slint type declaration, process-level UI startup and callback composition; private multi-window route, detached-transfer and pane-tree ownership | Feature implementations, SSH protocol details, or JSON schema details |
| `src/app/macos_window.rs` | Main-thread AppKit title-bar setup, running-application icon, and standard application-menu action binding | Generated Slint types, persisted settings, SSH or worker state |
| `src/app/workspace.rs` and `src/app/workspace/` | Private workspace facade plus focused Tab lifecycle, Session Editor transaction, and profile/group management wiring | Generated type declaration, transport implementation, persistence schema, or broader public API |
| `src/app/{connection,connection_monitor,terminal_bridge,settings_bridge,view,serial_bridge,sftp_bridge}.rs`, `src/app/{connection,view}/` | Private application-bridge feature wiring and cohesive snapshot/Slint mapping modules, including protocol dispatch, SSH trust/authentication, direct workers, serial discovery, SFTP intents, detached opener dispatch, pane models and settings/options mapping | Generated type declaration, transport implementation, or persistence schema |
| `src/app/file_icons.rs` and `src/app/file_icons/platform/` | Bounded process-local file-icon keys/cache and owned RGBA fallbacks; cfg-scoped platform resolvers | Slint models, SFTP sessions, arbitrary path inspection, or persistent cache state |
| `src/app/local_files.rs` | Bounded local directory metadata discovery and regular-file revalidation for the SFTP local pane | Slint types, file mutation, persistence, or SSH handles |
| `src/app/state.rs` and `src/app/state/` | UI-independent workspace tabs, per-tab terminal/worker state, attempt transitions, and their tests | Slint component/model types or russh protocol details |
| `src/app/{input,session_groups,terminal_render,credential_tasks}.rs` | Testable input/group/render mapping, theme-aware terminal defaults, and blocking credential task boundary | Window ownership, transport handles, or mutable UI state |
| `src/app/diagnostics.rs` | Redacted keyboard classification, fixed diagnostic route/action fields, and the dedicated tracing target | Raw terminal/clipboard text, paths, profile labels, hosts, credentials, or transport state |
| `src/config.rs` and `src/config/` | Stable config entry and explicit exports; session/profile domain, settings, theme normalization, legacy migration, private JSON persistence and atomic replacement | Slint types, network connections, plaintext password storage |
| `src/credentials.rs` | Profile-scoped system-keyring and encrypted-vault records | UI state, plaintext configuration, SSH transport handles |
| `src/terminal.rs`, `src/terminal_dimensions.rs`, and `src/terminal/input.rs` | Bounded terminal grid, shared dimension contract, cell styles, cursor/scrollback state, selection extraction, and terminal key encoding | Slint types, network handles, credentials |
| `src/local_shell.rs` | Cross-platform shell discovery and one bounded worker-owned local PTY process per tab | Slint state, SSH trust, persisted terminal contents |
| `src/x_server.rs` | Platform X-server provider options, system application discovery with standard-path fallback, local display candidates, and bounded process startup | SSH channels, UI state, cookies, profile mutation, or remote server configuration |
| `src/ssh.rs` | russh handler, host-key decision, password/private-key/runtime-agent authentication, shell and server-opened X11 channel boundary | Window updates, persistent session mutation, UI formatting, agent identity management |
| `src/ssh/private_keys.rs` | Local `.ssh` private-key discovery and blocking key loading | Passphrase persistence, UI state, host trust decisions |
| `src/ssh/x11.rs` | Local DISPLAY resolution, exact xauth cookie lookup, X11 setup validation/rewrite, local endpoint connection, and relay | UI state, profile mutation, cookie persistence, X-server startup, or access-control changes |
| `src/ssh/worker.rs` and `src/ssh/worker/` | Bounded session startup/commands, plus private shell/X11 and SFTP-only lifecycle modules; coalesced resize, batched events, cancellation and shutdown | UI state or profile persistence |
| `src/sftp.rs`, `src/sftp/transfer.rs`, and `src/sftp/transfer/cache.rs` | Bounded SFTP v3 packet adapter, directory browser, worker-owned chunked download/upload, text edit, rename/delete and private temporary publication/cleanup | Slint types, credentials, profile persistence, detached opener calls, or russh trust decisions |
| `src/telnet.rs` | Plaintext TCP lifetime, RFC 854 option filtering, NAWS, bounded input/output, cancellation and shutdown | Credentials, SSH trust, UI state or terminal rendering |
| `src/serial.rs` | Non-opening port discovery, stable USB identity matching, serial parameter mapping, and one bounded device worker | Automatic device opening/probing, UI state or persisted profile mutation |
| `src/logging.rs` | Global tracing subscriber, log directory, daily rolling writer, retention and flush guard | Credentials, feature state, UI or SSH handles |
| `src/main.rs` | Process startup and logging-guard lifetime | Feature logic |

## Application icon ownership

`assets/ion/terminal_icon.svg` is the only canonical icon source. The generated
PNG, ICO, and ICNS files are build/package inputs, not UI or transport state.
`ui/app.slint` selects the 256px PNG for the Slint/winit window. Windows
compiles the multi-size ICO into the executable from
`packaging/windows/axssh.rc`. On macOS, the application bridge sets the running
Dock icon on the UI thread, while the `.app` bundle uses the ICNS named by its
`Info.plist`. Linux package metadata installs the desktop entry and matching
PNG sizes in the hicolor hierarchy. None of these paths reads the reference
checkout or bypasses the font resource-loading contract.

## Slint component state ownership

`ui/app.slint` exports `AppWindow` as the sole Rust-facing Slint contract. It
owns top-level composition, the cross-platform menu tree, and generated
callbacks/properties only. The root maps the existing Rust-facing flat snapshot
properties declaratively into small UI DTOs, rather than letting Rust reach into
arbitrary internal component instances:

```text
Rust
  <-> AppWindow properties / callbacks
  <-> WorkspaceShell / OverlayHost
  <-> TerminalPane / SettingsPane / SessionEditorPane
```

`WorkspaceShell` owns the sidebar collapse state, saved-connection picker
visibility, picker dismissal, and sidebar/tab/content composition. Its tab,
profile, terminal, and settings data is a read-only `WorkspaceViewState`; it
sends user intent such as activation, close, connect, save, or cancel upward by
callback. `TerminalPaneGroup` renders bounded per-window lists of normalized
`TerminalPane` placements and internal split dividers. Each `TerminalPane` receives a read-only
`TerminalViewState` and owns only terminal-local focus, IME proxy, selection,
cursor blink, and measured sizing. Its selection clears when the logical pane or
transparent native input loses focus, while the terminal context menu retains it
through a Copy action. It never owns a worker, a terminal buffer, or connection
state. Terminal panes intentionally have no visual frame;
`AppWindow` draws the one client-area frame around the whole application window.
The Rust-owned terminal snapshot may also carry one small, tab-local connection
notice. A failed connection, unexpected disconnect, reconnect countdown, or
exhausted retry budget appears as a non-blocking banner in that terminal pane,
including split panes and detached Terminal windows. Its Retry and Close intents
carry the pane UUID back to the application, which revalidates the window route
before restarting the existing worker route or closing the affected tab. The
notice is deliberately absent while a host-key or authentication security phase
is active; those existing blocking security overlays remain authoritative.
Only a newly created `TerminalPane` queues one IME focus retry until its first
layout pass completes, then rechecks visible, focused, and connected state
before focusing the native proxy. Identity-preserving terminal identity,
split-pane focus, connection, visibility, and divider-release requests focus the existing
native proxy synchronously. Terminal input, resize, scroll, and selection callbacks
carry the terminal Tab UUID, which the application validates against the
current window's pane tree before acting.
Mouse input follows the same ownership boundary. `TerminalModel` exposes only
the active private mouse modes and emits bounded SGR, UTF-8, or legacy X10
events. `TerminalPane` uses an xterm.js-style `mouseEventsRequireAlt` policy
for button gestures: left dragging selects local text by default, while holding
`Alt` (`Option` on macOS) with reporting active forwards click, release, drag,
and cell-motion coordinates through the pane UUID callback. The bridge validates
the pane and sends those bytes to its worker. Wheel events still follow active
reporting so full-screen TUIs can scroll normally; `Shift` + wheel uses local
scrollback. When reporting is off, the existing direct local selection and scroll
fallback remains in control. Moving focus to another pane, a divider, or another
window control clears that local selection; the terminal context-menu Copy action
retains it until the action completes.
Alternate-screen alternate-scroll is treated as reporting only while the
terminal is on its alternate screen.
Terminal Edit-menu intent stays in Slint as a validated command plus bounded
revision. Every pane observes that signal, but only the focused pane invokes its
existing local copy, paste, or select-all operation; selection coordinates and
text are not promoted into application state for menu routing.
Divider gestures carry only the stable preorder divider ID and a normalized
ratio. `WindowRouter` validates them against the active `PaneTree`, which owns
the bounded ratio and republishes both leaf geometry and divider geometry. When
the pane UUIDs, divider identities, and row counts still match, the bridge
updates those existing Slint model rows in place, preserving the divider's
repeater instance and pointer capture; a mismatch falls back to the normal
full snapshot refresh.
Pane focus callbacks use the same identity-checked in-place layout update, so
clicking a split pane does not replace its repeater or transparent IME proxy
before the next key event. A stale or structurally changed model falls back to
the full snapshot refresh path.
Worker-driven multi-window refreshes share the bounded `AppState` pending gate.
One UI event resolves the latest routed views, updates identity-matched pane and
divider model rows in place, and schedules at most one follow-up when requests
arrived during that application. This prevents terminal output from building an
unbounded Slint event queue or repeatedly replacing the focused IME proxy.
For a matching pane, the bridge also retains the existing render-line and run
`VecModel` identities. It writes new rows through those subscribed models, or
resets the same model when its row count changes, before updating the outer pane
row. The visible `TerminalGrid` therefore receives an immediate model
notification for remote output without waiting for a later focus change.
The cursor uses a retained, bounded one-row model for the same reason. Each
snapshot updates its row, column, visibility, and displayed cell through that
model before publishing terminal rows, so cursor movement does not depend on a
focus-triggered outer DTO refresh.
Only non-root leaves of a `PaneTree` are independently closable. Their close
intent is revalidated against the owning window route, collapses that leaf in
the tree, removes exactly that runtime Tab, cancels a pending probe, and shuts
down any surviving worker asynchronously. A normal local-shell exit or SSH or
Telnet disconnect uses the same path for a child pane. The workspace root and
connection, authentication, or transport failures remain visible; closing the
visible Terminal Tab still owns whole-tree shutdown.
Its internal `TerminalGrid` receives the smaller `TerminalGridView` and
`TerminalSelectionView` DTOs: it renders the bounded snapshot and turns
pointer, scroll, and context-menu gestures into callbacks, while `TerminalPane`
retains the focus, IME input, selection draft, and resize lifecycle.
Terminal target activation follows the same boundary. While the platform primary
modifier is held (`Cmd` on macOS and `Ctrl` elsewhere), a pointer move or primary
modifier press asks the application bridge about the current visible row and
cell. The private parser returns the complete bounded target character range,
which `TerminalModel` maps back to a half-open cell range; `TerminalPane` holds
only that short-lived `TerminalTargetHighlight`, and `TerminalGrid` draws its
accent underline and pointer cursor across the complete target. The highlight is
cleared on modifier release, pointer exit, text selection, or scrolling. A
primary-modifier click revalidates the pane UUID and reads that single row from
the terminal model before parsing it. The private parser accepts only
`http://`/`https://` URLs and Unix-style remote paths beginning with `/`, `./`,
or `../`, removes terminal punctuation and `:line[:column]` diagnostics, and
rejects controls and overlong text. URLs are handed to the local default opener
on a blocking worker, never fetched by AxSSH. A remote path stays within
the existing SSH/SFTP companion route: an available companion is activated and
navigated, while a companion still in trust, authentication, or SFTP-browser
startup keeps the bounded path on that runtime Tab until its normal flow is
ready. A new SFTP Tab carries the path as tab-local initial state until its
normal, independent SSH authentication starts. Neither target text nor that
initial path is persisted, logged, or sent through Slint as a terminal buffer.
The private terminal render mapper can add bounded semantic color to plain
visible cells only when that option is enabled: URLs and actionable Unix paths use the link color; HTTP
`2xx`/`3xx`/`4xx`/`5xx` and common success, informational, warning, and error
tokens use their corresponding semantic colors. Each category uses the selected
terminal palette by default; an optional normalized `#RRGGBB` Settings override
can replace it. Explicit ANSI 16/256/true-color foregrounds are not replaced by
semantic highlighting. The renderer resolves program colors and inverse first,
selects an optional semantic foreground, and then applies one HSL-lightness
adjustment to the final visible foreground. `dim` is folded into that adjustment;
backgrounds, selection, and the cursor remain unchanged.
Its `key-pressed` handler sends only special keys and terminal control chords to
Rust; printable keys, Shift text, and committed IME text remain in the native
`TextInput.edited` path.
`AppWindow.log-keyboard-event` reports a handled transient-control key after
shortcut recording and security prompts have been excluded. Native menu
commands report through the separate fixed-ID menu-action route because Slint
does not expose whether activation came from the pointer or an accelerator.
The diagnostics boundary converts every text key or paste to the fixed `Text`
label and accepts only whitelisted route/action values; no raw text or text
length becomes a tracing field.

`SettingsPane` receives a read-only `SettingsViewState`, copies it into its
private editable draft, and emits the candidate when its Settings tab is closed.
All category detail areas are individually scrollable, while the Settings
navigation and search header remain fixed. Its non-persistent global search
matches a bounded static catalog of category names, setting titles, and
descriptions case-insensitively; selecting a result clears the query, opens the
matching category, and returns that category to its top. The query and result
model are UI-local and never enter `AppSettings`, persistence, diagnostics,
workers, or transport.
The tab close intent carries the stable Settings tab ID; Rust persists the
candidate asynchronously and closes that tab only after persistence succeeds. A menu or native
platform request provides a read-only requested section; the pane owns the
currently selected section while navigating. `SessionEditorPane` follows the
same pattern with `SessionEditorViewState`: it resets its private fields only
when the incoming draft identity changes, and never mutates the Rust snapshot
while the user types. Its scroll view sets an explicit viewport height from the
editor content's preferred height, so all fields remain reachable when the
editor is taller than the current window. `in-out` properties remain inside components only where
two nested controls are editing the same local draft. Derived labels, dialog
copy, and visual states are bindings, not duplicate mutable storage.
Password and vault-password fields are local secret drafts: they are blank on
every editor open, are cleared after submit, and never enter the read-only
source snapshot. A password may be left empty when saving a profile, and the
save-password toggle is explicit intent only; the storage choice is ignored
until saving is enabled.
The editor also carries SSH-only, non-secret `sftp_remote_path` and
`sftp_local_path` fields. They are ordinary local drafts: changing them does
not open either directory, and saving them does not mutate a running Tab.

`OverlayHost` owns the local group/profile-management dialog open state and
draft, deriving its title, message, and button presentation from one action
value. It forwards a management command only after confirmation. It also
composes the SSH host-key and authentication dialogs, but these are intentionally
different: their visibility and prompt identity are read-only Rust-owned
security phase. The UI may submit confirm/reject/authenticate/cancel intent, but
must not locally hide either dialog before the Rust state transition accepts it.

## Event flow

1. A Slint callback produces a small value such as a saved profile ID, unique
   tab ID, terminal key/modifier tuple, draft fields, a save-and-connect intent, a trust
   decision, or one transient secret. Authentication secrets travel through the
   dedicated SecretTextInput, whose UI value is cleared after the application
   accepts it or when the prompt is cancelled.
2. Opening a profile or local shell always creates a new terminal tab UUID,
   even when another tab uses the same target. Saved-connection input, resize,
   output, retry, and close operations route by `tab_id + profile_id +
   attempt_id`; local operations route by `tab_id`. An unknown SSH host starts
   a cancellable probe tied to that tab
   while transport remains rejected. Workspace Tab order is in-memory
   presentation state: a drag completion passes a tab UUID and bounded target
   index to `AppState`, which reorders only the existing Tab list. While held,
   Slint keeps a translucent source slot, highlights the prospective target,
   and renders a non-interactive Tab copy at the pointer; it never creates a
   second runtime Tab. The leading UI ordinal derives from that list index,
   while an instance suffix such as `#1` remains part of the Tab's stable title.
   Previous/Next Tab intent asks `AppState` to activate the adjacent UUID in
   this same list and wraps at either end; zero or one Tab leaves state unchanged.
   Each SSH Tab also owns its current connection phase: idle, cancellable host-key
   probe, pending host-key confirmation, pending authentication, or stored-
   credential loading. There is no global pending-probe, trust, or authentication
   slot.
3. After explicit confirmation, the controller atomically persists the exact
   fingerprint. Password profiles load a remembered credential on a Tokio
   blocking boundary or open a password prompt. The session editor may instead
   submit a new password inline; a blank value preserves the existing backend
   reference. A non-empty value is available to **Save & connect** as a
   Tab-scoped one-time secret, and it updates the selected backend before the
   profile save only when **Save password (optional)** is checked. A requested
   encrypted-vault save without a vault password falls back to the system
   credential store and records that effective backend; existing vault records
   remain vault records and still require their vault password to unlock.
   Private-key profiles
   load the selected path off the UI thread and
   request a transient passphrase only when the encrypted key cannot be opened
   without one. SSH-agent profiles bypass credential storage and the secret
   prompt, then let the worker connect to the current runtime agent after host
   trust is established. The security overlay renders only the active Tab's pending
   phase; inactive Tabs retain their own prompt until activated, and changing an
   authentication prompt clears its secret inputs before it becomes visible.
4. Settings > General owns the default backend for a newly remembered SSH
   password: the platform credential store or the encrypted application vault.
   Each ordinary password prompt initializes its backend selector from that
   setting and may override it for that prompt; the selector is ignored unless
   **Save password (optional)** is checked. The session editor uses the profile's
   existing backend or the Settings default only to initialize its selector.
   Without **Save password (optional)**, an inline password is used once by **Save &
   connect** and a save-only action discards it. With **Save password (optional)**
   enabled, the
   selected backend is updated transactionally with the profile. A missing
   vault password deliberately selects the system credential store instead of
   creating an unusable vault record. The secret is
   never returned in the source snapshot or serialized profile, and changing
   the default neither migrates nor breaks an existing credential. Deleting a profile, switching it
   to private-key or SSH-agent authentication, or rejecting a stored password removes its
   referenced credential transactionally without stopping an already-open
   terminal worker. Profile save and delete operations share one asynchronous
   credential gate and assign a latest-mutation token per profile. They validate
   the original profile before changing a credential and again before replacing
   `SessionStore`; a superseded operation restores its own credential backup
   before releasing the gate. A completed save closes only the editor Tab whose
   Tab and draft identities initiated that save.
5. The terminal surface maps Slint special keys, including F1-F12, to
   UI-independent terminal key values and applies a narrow shifted-hyphen
   fallback when the platform still reports `-` for `Shift+-`.
   `src/terminal/input.rs` emits control bytes, normal CSI or
   application-cursor SS3 arrow/Home/End sequences, and modified xterm
   navigation/function-key sequences. A transparent, cursor-positioned
   `TextInput` is the native text and IME proxy: special keys and terminal
   control chords use `key-pressed`, while printable text, Shift text, and IME
   commits enter only through `edited`; preedit remains local UI state. At the
   application boundary, physical macOS key events read AppKit's current
   aggregate modifier state before restoring Control and Command semantics
   after Slint's Apple mapping; this keeps the two Control keys equivalent
   when a side-specific modifier event is absent. Committed IME and pasted text
   explicitly use empty modifiers, so they cannot inherit a still-held shortcut
   key.
   `TerminalGrid` displays that local preedit value only while the connected
   cursor is visible; no composition text crosses its gesture callbacks.
   `TerminalSettings.option_as_meta` is disabled by default, so Option text and
   dead keys use the text path; when enabled, Option-modified keys are terminal
   Meta input. Windows/Linux retain Alt terminal input while Ctrl+Alt printable
   text can remain AltGr text. Terminal Ctrl combinations take priority while
   the terminal is focused; `Ctrl+C` remains PTY input. Clipboard actions keep
   `Cmd+C/V` on macOS and `Ctrl+Shift+C/V` elsewhere. Workspace commands use the
   platform modifier. Selection copy remains local while paste becomes bounded
   shell input. The default optional right-click action chooses between them
   based on selection state. When `copy_selection_on_select` is enabled, a
   completed pointer selection and Select All copy locally, and direct
   right-click always pastes; this mode supersedes the separate right-click
   preference without promoting selection or clipboard text outside Slint.
   Native text/IME and application terminal-key routing are disabled until the
   active terminal reports connected. This UI guard is repeated at the Rust
   bridge, so focus or a stale callback cannot enqueue terminal input during
   connection setup.
   Keyboard routing and major application callbacks use the dedicated
   `ax_ssh::diagnostics` debug target. Special keys have stable labels, while
   all printable, IME, password, and pasted text is recorded only as `Text`.
   Function-call events contain fixed action IDs and outcomes, never callback
   path/name/host/secret values. The target is disabled by the default INFO
   filter and is enabled explicitly for troubleshooting.
   The separate `ax_ssh::latency` debug target records only local sequence
   numbers, fixed stages, outcomes, and monotonic microsecond durations. It
   measures UI-to-worker request time, SSH command queue time, russh call time,
   and remote-output-to-UI dispatch/apply time without recording input/output
   values or lengths. `first-output-after-input` is a temporal observation,
   not proof that an asynchronous output chunk is the server echo for that key.
6. Each saved-connection terminal tab owns at most one transport worker. SSH
   starts it only after trust and authentication; Telnet starts a plaintext TCP
   worker immediately; Serial first enumerates metadata and resolves the saved
   USB identity, then opens the selected device only because the user requested
   connection. Bounded command/event queues remain independent between
   duplicate profile tabs. Closing a tab removes its attempt route before
   asynchronously shutting down the worker, so late events cannot update
   another tab.
   While SSH trust and authentication are still in progress, the worker keeps
   waiting and discards any already-queued shell or SFTP operation. Only an
   explicit `Disconnect` or a dropped controller cancels that connection
   attempt; operational commands become valid only after `Connected`.
   The shared russh client config explicitly enables `TCP_NODELAY`, so small
   interactive channel-data writes are not held for Nagle aggregation. The
   bounded input queue adds no batching timer: the worker sends dequeued input
   immediately. This removes client-side waiting but cannot remove the network
   round trip required for a remote PTY to echo input. Interactive SSH PTY
   requests also enable `OPOST` and `ONLCR`, so ordinary remote line feeds
   return to column zero instead of accumulating a column offset in the local
   terminal model.
7. A local terminal tab instead owns one `portable-pty` worker thread. That
   worker owns its child, reader, writer, resize state, bounded command/event
   queues, cancellation flag, child-killer handle, and every thread join for
   the tab lifetime. Shutdown sets cancellation, wakes the worker, force-stops
   the isolated Unix PTY process group or platform child, closes PTY resources,
   and waits asynchronously until the worker is joinable. Repeated requests for
   the already applied row/column size are discarded before calling the platform
   PTY resize operation. A full event queue is cancellation-aware and cannot
   strand the reader. Worker shutdown has a fixed timeout and never waits forever;
   the controller retains its child-killer fallback until worker cleanup clears it.
8. Each tab that renders a terminal owns one bounded `TerminalModel`. An
   SFTP-only tab deliberately keeps this model absent because it never renders
   terminal cells; its browser state remains independent. `vt100` owns the
   rows, cell styles, cursor, scrollback, wide characters, and application
   cursor mode. Terminal-generated `PtyWrite` protocol responses, including
   cursor-position reports required by Windows ConPTY startup, are collected in
   a bounded private queue and written back only through that Tab's current
   transport worker. They do not enter Slint, persistence, or logs. The checked-in
   `vendor/vt100` patch keeps its locked `0.16.2`
   API but clears a wide character whose continuation cell would be removed
   during a column shrink, for both normal and alternate screens. `TerminalModel`
   delegates height changes to the locked `alacritty_terminal::Term::resize`:
   growth restores only actual scrollback rows above the viewport. When history
   is exhausted, existing primary-screen content remains top-aligned and newly
   exposed blank rows stay below it; the model must not scroll content down or
   synthesize blank history to force the cursor to the new bottom edge. Shrinks,
   alternate screens, an active scroll region, a non-bottom cursor, and a user
   viewing scrollback retain upstream resize semantics. Output for inactive
   tabs stays in Rust state; each visible pane contributes only its bounded cell
   snapshot across the Slint event loop. UI updates use
   `slint::invoke_from_event_loop` and `Weak<AppWindow>` so shutdown does not
   keep a window alive.
   The small-screen window floor is `520x360`; terminal layout, persisted
   default sizes, and the model use the same non-zero `10x3` grid floor. The
   Rust `terminal_dimensions` module is the source for the model, settings,
   and backend maximums; Slint keeps a compile-time mirror for layout because
   it cannot import Rust constants. PTY and worker entry points retain their
   separate non-zero `1x1` minimum while sharing the same `300x100` maximum.
   This permits a compact window without ever issuing an invalid PTY resize. Users
   can collapse the existing session sidebar to reserve additional terminal
   columns on narrow displays.
   `TerminalPane` coalesces changes to its measured grid, configured font
   metrics, terminal-tab identity, and connection state until the next
   UI turn, then requests one final PTY size. Its initialization also schedules
   that same coalesced update, so an already-connected pane reaches its settled
   initial grid without waiting for a later window or divider resize. This keeps
   a Settings font change and a later return to a connected terminal on the
   same current-grid path as a window resize. Its vertical row count rounds up,
   so the final terminal row always meets the pane bottom. A fractional cell
   clips only the first row at the top; height beyond the maximum row count
   remains above the grid. The same local origin is applied to grid cells,
   cursor/IME preedit, and pointer row mapping, and that rounded row count is
   the one sent through the existing PTY resize request.
   `AppState::resize_terminal(tab_id, ...)` is the single application entry for
   a UI grid change: it requests the specified visible pane's existing worker
   resize first and then immediately resizes that Tab's local `TerminalModel`.
   Local and SSH workers receive PTY
   resize requests; Telnet sends NAWS only after the peer accepts that option.
   Serial has no remote terminal-size contract, so its worker request is a no-op
   and the same entry changes only the local model. After any accepted UI
   resize, the application schedules a visible-pane refresh. When
   that UI task executes, it copies the current snapshot from `AppState` rather
   than applying a snapshot captured by an earlier worker event. Therefore an
   already queued Output update cannot restore an older grid while the user is
   still dragging the window. The worker's later `Resized` acknowledgement
   remains transport confirmation only.
   SSH output normally crosses the worker boundary in bounded 16 ms/16 KiB
   batches. The first output observed after terminal input flushes the current
   batch immediately, reducing interactive echo rendering delay without local
   prediction or duplicate echo; sustained unrelated output retains batching.
9. On macOS, AxSSH keeps the standard native title bar and disables
   movable-window-background behavior. AppKit alone owns window movement from
   that title bar; the Slint workspace Tab strip is regular client content
   immediately below it. This prevents native window dragging from competing
   with a Tab reorder gesture.
10. Platform-menu Settings and About intents open one singleton Settings
    workbench tab at General or About respectively. It remains in the visible
    workspace-tab model alongside running SSH and local-terminal tabs, so
    activating Settings never removes the route back to a live terminal. If
    the Settings shortcut is used while that tab already exists, the existing
    tab is activated instead of creating another Settings instance. Its
    draft previews in the current application immediately, while the Close
    action persists that candidate before removing only the singleton tab; it
    never affects a terminal worker. The coalesced preview crosses the
    application boundary only to update in-memory settings and visual state;
    the close transaction performs the resource loading and persistence.
    About presents the product-purpose description, the package version and
    build revision as read-only UI metadata, identifies the application license
    as `GPL-3.0-only`, and embeds Slint's standard `AboutSlint` attribution
    component. Its support actions cross the existing AppWindow callback only:
    Report a bug opens the AxSSH issue tracker, Open log folder opens the
    process-owned rolling-log directory, and Copy diagnostics places only
    version, revision, OS, architecture, and build profile on the clipboard.
    None of these actions uploads data or exposes configuration, host, path, or
    credential fields to Slint.
   The session sidebar does not duplicate Settings
   or About. It spans the full client height directly below the native title
   bar, while the workspace Tab strip occupies only the column to its right.
   Its `+` is pinned to the outer right edge and opens a Slint-local picker containing a
   masked, read-only snapshot of every saved connection profile; selection routes only
   the profile UUID through the existing connection callback. File > New
   Server, its configurable `Cmd+N`/`Ctrl+N` shortcut, and the sidebar
   blank-area context menu remain distinct session-editor actions. File also
   owns clipboard import and selected-object export, with configurable
   `Cmd/Ctrl+Shift+I` and `Cmd/Ctrl+Shift+E` defaults respectively.
11. One declarative Slint `MenuBar` owns the cross-platform business-menu tree.
    The locked winit/muda backend installs it in the macOS screen menu bar and
    the Windows native window menu; Linux backends without native menu support
    render the same tree at the top of the client window. On macOS,
    `src/app/macos_window.rs` reuses the backend-created standard application
    menu, binds its existing About item to the internal page when that item is
    present, and installs `Settings...` independently of About. Its key
    equivalent follows the live configurable Settings shortcut rather than a
    hard-coded label.
    The AppKit target is main-thread-only, captures
    only `Weak<AppWindow>`, and is retained by each menu item's represented
    object because AppKit target references are weak. Configured strings are
    converted in one application-boundary parser to `slint::Keys`; on Apple,
    persisted `Cmd` maps to Slint `Control`, while physical `Ctrl` maps to Slint
    `Meta`. Muda therefore renders and activates native accelerators instead of
    appending shortcut text to titles. Edit exposes terminal-only **Copy**,
    **Paste**, and **Select All** commands and removes the permanently disabled
    Undo placeholder. Copy/Paste reuse their configurable terminal shortcuts;
    Select All is fixed to `Cmd+A` on macOS and `Ctrl+Shift+A` on Windows/Linux,
    preserving plain terminal `Ctrl+A`, `Ctrl+C`, and `Ctrl+V` on those platforms.
    The commands are disabled outside a Terminal Tab, so ordinary non-secret
    text fields retain native editing shortcuts and context menus; secret fields
    remain non-copyable. No generic text-focus bridge, Cut, or Undo command is
    introduced. **Previous Tab** and **Next Tab** use the
    same parser for fixed `Cmd+Shift+[` / `Cmd+Shift+]` accelerators on macOS
    and `Ctrl+Shift+[` / `Ctrl+Shift+]` on Windows/Linux. They are enabled only
    with more than one Tab and share the shortcut-recording/security gate. The
    macOS close-tab item and the cross-platform fixed **Switch SSH/SFTP Tab**
    item intentionally have no dynamic active-tab menu properties. Their
    application callbacks resolve the active runtime Tab only when invoked.
    Terminal Edit, previous/next Tab, move, and close enablement consume small
    Rust-published menu-state booleans for the active Terminal surface, multiple
    Tabs, and active Tab presence. Those booleans are only written when the
    underlying workspace snapshot state changes, so terminal output, notices,
    status refreshes, and replacement of the workspace Tab model do not rebuild
    an already-open native menu. Shortcut/security-state changes and real
    menu-relevant workspace changes still let the existing AppKit bridge
    idempotently rebind the current Settings/About items. Rebinding scans the current native menu
    tree for the application submenu and About title, accepts the platform's
    ellipsis spelling for Settings, and retries briefly when AppKit has not
    published the rebuilt menu yet. Transient lookup failures remain silent
    within the bounded retry budget; only exhaustion emits one warning with
    the total attempt count.
    Windows/Linux retain dynamic close-tab state, keep Settings in Edit, and
    keep About in Help. File, View, Pane, Window, and Help reuse existing
    new-session, sidebar, local-shell, close-tab, clipboard-transfer, and
    shortcut intents. Import always invokes the automatic bounded transfer path.
    Export asks `SessionNavigation` for the currently selected persisted
    Group/server object and reports a fixed status when no valid selection exists.
    Menu activation logs a fixed action ID; Slint's `MenuItem.activated` callback
    does not expose whether activation came from a pointer or accelerator.
12. The session navigator owns the Slint-local sidebar expanded/collapsed state,
    each Group's disclosure state, and the current selected kind/ID/group name.
    Expanded and compact rows consume the same local selection identity, so
    selection remains visible across sidebar-mode changes while hover and focus
    remain separate transient states. Selection is not serialized, copied into
    `AppState`, logged with target values, or sent to any transport. Rust supplies a complete, read-only
    `SessionGroupRow` snapshot with a nested bounded profile model; it does not
    retain an expanded-group set or receive a group-toggle callback.
    `SessionNavigationGroup` and `CompactSessionNavigationGroup` independently
    expand/collapse their own Group rows, so either click or Enter/Space changes
    only that component's presentation state. Persisted group names belong to
    `SessionStore`, so empty groups survive restart. The expanded view renders a
    Local Shell card, then collapsible parent group rows and their single-line
    server children. Only the masked endpoint crosses into Slint. The expanded
    parent shows its name, count, and a centered drawn down chevron; a collapsed
    parent shows the matching up chevron. The compact rail alone uses a
    configurable 1-4 character badge derived from the group name, or the full
    group name in Full-name mode, rather than a folder icon. Full-name mode
    widens the collapsed rail to a bounded 180px and switches to a dense list:
    the header keeps the sidebar control at the trailing edge, Local Shell uses
    an icon-and-label row, groups use a single-line label with disclosure and
    count, and indented servers use their single-line names. Long labels elide
    within stable-height rows and remain available through tooltips.
    A separate compact panel control is the only action that expands or
    collapses the sidebar. In the expanded sidebar it sits at the trailing
    edge of the Local Shell row; in the collapsed rail it remains a top control.
    Custom group rows are keyboard focusable and use Enter/Space for the same
    local disclosure action as a click; this never changes the sidebar state. Native
    row context menus create servers in a group, copy or duplicate a group,
    rename or delete it, and connect, copy-address, copy-config, duplicate,
    edit, or delete a server. Ungrouped exposes only its add-server action.
    Right-clicking blank list space creates an empty group or an Ungrouped
    server. Clipboard import/export remain File-menu commands.
    `SessionActionMenu` maps
    these four menu shapes to flat
    `ActionMenuItem` lists. `FlatActionMenu` composes exactly one
    `ContextMenuArea`, emits only an action ID, and exposes `show-at(Point)` so
    the same action list can also back a button-triggered dropdown. Group/server
    copy and File-menu export use a versioned JSON envelope bounded to 256 KiB and 128
    profiles. Exported identities, credential references, and host-key
    fingerprints are removed. Import always creates new UUIDs, resolves
    name/group collisions, validates bounded profile fields, and removes
    credential/trust fields again before the candidate `SessionStore` is saved.
    Group Duplicate follows the existing server Duplicate contract: it creates
    new profile IDs and removes remembered-password references while preserving
    trust for the unchanged endpoint. Deleting a group moves its profiles to
    Ungrouped; deleting
    a profile removes only its persisted definition and credential. The
    collapsed rail renders a larger Group badge and smaller, tightly stacked
    server badges, while Local Shell keeps its dedicated entry. The application
    formatter masks usernames and IPv4 middle octets before data enters the
    Slint model. Static geometry is in `ui/theme.slint`; the persisted
    single-character mask setting is owned by `WorkspaceSettings`.

## Multi-window workspace transfer

The inline action on each SSH Terminal/SFTP Tab and the Window menu can detach
that workspace into a second native Slint window. A detached window uses the
active connection title as its native title and, on macOS, makes the native
title bar transparent over the active client surface: Terminal background for a
Terminal view and application background for SFTP. Its icon-only return button uses the system overlapping-window
symbol, with the matching AppKit multiple-documents template fallback, and
exposes its purpose through a tooltip and accessibility description. Its client content contains only the active
`TerminalPaneGroup` or `SftpPane`: it has no Tab strip, session sidebar, saved-
connection picker, Settings, session editor, or client menu. `AppState` remains
the sole owner of the Tab
runtime objects, terminal models, pending trust/authentication phases, and
transport workers. The `WorkspaceTransfer` payload contains only the source
window ID, terminal-pane UUIDs with their SSH/SFTP companions, and the active
Tab UUID; it never
contains a Slint component, russh handle, Tokio receiver, terminal buffer, or
secret.
Theme preview and save propagate the existing `AppSettings` value to each live
detached UI, then update its AppKit title-bar background from that UI's resolved
client surface color. This appearance-only path keeps each window's local
Slint theme coherent without routing AppKit state through `AppState`.
Detached Terminal panes keep the same direct Copy/Paste/Select All keyboard
handling even though no client menu is added.

`WindowRouter` maps each transferred Tab UUID to the current window's weak UI
handle. Refreshes publish a filtered Tab model and the snapshot for each route,
so late worker events can repaint the window that currently owns the workspace.
Within each route, it owns one volatile `PaneTree` per visible Terminal Tab.
The tree keeps a stable workspace Tab UUID plus at most eight terminal leaf
UUIDs, bounded split ratios, layout, and focus state. Only the stable workspace UUID is published in
the top-level Tab model; child pane sessions remain independent in `AppState`
but are filtered from the Tab strip. Closing that visible Terminal Tab closes
every terminal session in its tree, while an SFTP companion remains a separate
visible Tab. Detached return or close restores the same pane tree and focused
child to the main route without reconnecting or stopping workers.
The inline and menu controls pass their selected Tab UUID directly to the Rust
route handler, which validates that it belongs to the invoking window and makes
it active before creating or returning the native window. Detaching and
returning only change this route map. Closing a detached window returns its
transfer to the main route and hides the native window; it does not disconnect
or re-authenticate SSH/SFTP. The paired Terminal/SFTP UUIDs move together,
while their two independent russh workers remain independent.

Every internal split publishes one divider overlay. Its idle hairline uses the
semantic divider color; hover, drag, and keyboard focus use the accent color
and a thicker stroke without changing the hit-area geometry. Mouse dragging,
matching arrow keys, Home/End, accessibility slider actions, and double-click
or Enter/Space reset are mapped to ratios between 0.1 and 0.9. Updating a ratio
reuses each pane's UUID-directed terminal resize path, so PTY/NAWS/local model
sizes follow the new geometry. Ratios remain while switching Tabs or moving the
tree through detach/return, but are intentionally reset after application
restart and never enter settings, workers, or transport state.
The divider keeps its local drag state through pointer release or cancellation,
then requests focus only for the currently focused, connected terminal pane's
IME proxy. Keyboard and accessibility divider actions retain their own focus.

The main workspace Tab toolbar places one fixed-size, keyboard-accessible pair
of vertical and horizontal split controls beside the saved-connection button.
They emit the active pane UUID with `split-right` or `split-down` through
the same `pane-command` callback as the keyboard route; Slint does not create a
worker, mutate the layout, or add another top-level Tab. On macOS, a detached
Terminal places the same image-only pair in its native title bar immediately
before the Return icon, while its client area remains a full-height pane
surface. Each native action captures only a weak `AppWindow` and invokes the
same callback, so `WindowRouter` continues to validate the focused pane. Terminal panes also use `Alt+H/J/K/L` to focus the left/down/up/right
neighbor and `Alt+Shift+H/J/K/L` to create a fresh terminal session on that
side. A split of a local shell creates a new PTY; a split of SSH, Telnet, or
Serial repeats the normal profile connection flow. A split SSH child repeats
host-key and authentication handling and never inherits a one-time password or
private-key passphrase. SFTP is a standalone surface and cannot be a terminal
pane. The UI consumes these Alt combinations only after the router accepts the
command, so an unsupported direction or a full pane tree preserves ordinary
terminal Meta input.

## SSH security contract

`russh::client::Handler::check_server_key` is the trust boundary. Unknown and
mismatched keys are rejected before authentication. A rejected first-contact
handshake may expose its SHA-256 fingerprint to the confirmation UI, but only
an explicit user decision adds that exact fingerprint to the profile. A changed
key requires a second explicit decision. Passwords are transient callback
inputs and are not part of `SessionStore`. A password profile contains only an
optional `credential_storage` reference keyed by its stable UUID, never the
password or a vault password. The session editor keeps its masked password and
vault-password fields blank on every open and clears them after submission. A
non-empty editor password stays transient by default and is carried only by the
corresponding Tab until host-key confirmation completes and the SSH worker takes
ownership. Checking **Save password (optional)** additionally writes it through
the selected backend, with rollback if profile persistence fails. If an
encrypted-vault save has no vault password, it deliberately uses the system
credential store and persists that effective reference; it never creates an
empty-password vault record. Existing vault records still require their vault
password to unlock. Settings > General initializes the backend for a future
checked save-password action: macOS Keychain, Windows Credential
Manager, or Unix Secret Service for the system backend; or a per-profile
application-vault record. The vault derives a per-record key with Argon2id,
encrypts with XChaCha20-Poly1305 using the profile UUID as associated data, and
keeps the vault password transient. Private-key profiles persist only a path.
The SSH transport also reads the bounded platform OpenSSH `known_hosts` file.
Non-revoked exact matches are shared trust; profile conflicts, changed keys,
malformed records, and unreadable files never broaden trust. Exact `@revoked`
matches are rejected before authentication and cannot be bypassed by the normal
confirmation action. Unknown confirmation appends the observed key; changed
confirmation atomically replaces matching non-revoked host records while
preserving unrelated and revoked records. Revoked record removal is a separate
explicit action.
The key bytes and optional passphrase are loaded in one blocking task, used for
one authentication attempt, and then dropped without entering configuration,
tracing fields, or UI models. The separate, non-secret `.ssh` candidate-path
scan starts only when the Session Editor enters Private key mode. Leaving that
mode or closing the editor clears its option model and advances a generation so
an in-flight scan cannot repopulate released UI state.

An SSH-agent profile persists only `AuthMethod::SshAgent`; it cannot contain a
password credential reference and never stores an agent socket path, identity
comment, public key, private key, or passphrase. After the real SSH handshake
has passed the same exact host-key check described above, `src/ssh.rs` connects
to the runtime agent for that connection only. Unix and macOS use the current
`SSH_AUTH_SOCK`; Windows uses that variable or the default OpenSSH agent named
pipe. The agent lists identities and signs authentication requests while the
russh worker retains sole ownership of the client. AxSSH attempts at most five
identities and applies one 30-second timeout to agent connection, identity
listing, algorithm negotiation, signing, and authentication. The client drops
on success, failure, cancellation, or timeout. Application-owned errors use
fixed categories and do not include socket paths, identity comments, or key
data; any unlock or confirmation UI remains owned by the system agent. This is
client authentication only and does not implement agent forwarding or agent key
management.

X11 forwarding is an SSH-profile setting that defaults on for new profiles and
for legacy data that omitted the field; an explicit saved `false` remains off.
It applies only to terminal mode: SFTP-only, Telnet, and Serial workers never
request X11. Global `X11Settings` stores only the non-secret provider,
Custom-only application path, launch preference, and explicit no-auth
compatibility choice. `src/x_server.rs` resolves Auto to platform providers,
discovers macOS applications by bundle identifier through `NSWorkspace`,
searches the Windows process `PATH` before Program Files, and returns a bounded
read-only snapshot of known installed locations for Settings. Standard
installation paths remain existence-checked fallbacks. macOS Auto prefers
XQuartz, then MacXServer; Windows Auto prefers VcXsrv, then Xming; Linux exposes
the system `DISPLAY` and Custom choices. Every known provider ignores a saved
Custom path, and no provider is downloaded or installed. A Custom target is
launched without a command shell and must be a regular file with executable
permission on Unix.

The shell creation phase sends an X11 forwarding request with one random 128-bit
fake cookie, but it does not read local `DISPLAY`, run `xauth`, probe a local
endpoint, or start an X server. Only when the remote server opens an X11 channel
does the relay resolve local display candidates, run a timed, output-limited
`xauth list <DISPLAY>`, and, if needed and enabled, launch and poll the selected
provider behind the existing timeout. MacXServer is started only with explicit
no-auth compatibility and forced to `127.0.0.1:6000`; VcXsrv and Xming receive
`-multiwindow -clipboard -ac` only under the same explicit choice. The relay
still accepts only local endpoints and validates the SSH fake cookie before
stripping X authority for a compatible local server. Local preparation, channel,
or server-request failure rejects only that X11 channel, leaves the SSH shell
connected, and publishes a persistent unavailable status. Remote `sshd` policy
is not modified: it must independently allow X11 forwarding and accept the
request before it assigns remote `DISPLAY`.

Each enabled terminal creates a random 128-bit fake cookie for the SSH request.
`ClientHandler` rejects server-opened X11 channels by default and dispatches
them only after the request succeeds. The dispatch queue and active relay set
are both capped at eight; disabled/closed channels are administratively rejected
and resource exhaustion is reported explicitly. A relay connects to the
prevalidated local endpoint, reads an X11 setup packet under a timeout and size
limits, accepts only the expected byte order, protocol, and fake cookie, replaces
it with the real cookie, then uses bounded-buffer bidirectional copying. Fake
and real cookies remain in zeroizing worker-owned memory, are never persisted or
logged, and all relay tasks are aborted and joined before the SSH worker closes.

Authentication secrets use `ui/components/secret-text-input.slint`, not the
general-purpose text input. It retains native password masking, IME, focus, and
password-input accessibility semantics, but does not publish an
`accessible-value`, offer an edit context menu, allow copy/cut shortcuts, or
allow pointer selection to reach the platform selection clipboard. Its
accessibility contract permits setting a value, not reading one. At the Slint to
application boundary the accepted `SharedString` is copied immediately into
`Zeroizing<String>`; the SSH worker, private-key loader, vault task, and
credential rollback keep AxSSH-owned secret buffers zeroized on drop. This
shortens AxSSH-owned lifetimes, but does not claim to erase temporary copies
inside Slint, the IME, russh, or a platform credential backend.

Authenticated connections follow this lifecycle:

- every terminal tab has a unique runtime UUID and one worker owns its russh
  handle for the full lifetime;
- the bounded command channel carries shell input, disconnect, cancel, and SFTP
  browser intents; a watched terminal size coalesces high-frequency resize updates;
- an opt-in X11 terminal owns one bounded server-channel receiver and at most
  eight relay tasks; SFTP-only mode has no X11 dispatcher;
- terminal output is capped per batch and backpressured through a bounded event
  channel before entering the bounded terminal model;
- worker events report connected, resize, output, disconnected, host-key
  rejection, credential failure, or a capped error message;
- each SSH Tab independently owns probe cancellation and its authentication
  phase; every UI callback and delayed probe, credential, or worker result
  revalidates the Tab, profile, attempt, and expected phase before changing it;
- before authentication finishes, queued shell and SFTP operations are ignored
  without ending the connection attempt; `Disconnect` remains immediately
  effective;
- cancel interrupts connection/authentication as well as an established session;
- a 20-second keepalive with three missed-reply limit and the 90-second
  transport inactivity boundary decide connection liveness; a quiet shell data
  channel is valid and never has its own output timeout;
- tab close invalidates the tab/attempt route before requesting worker shutdown;
- window shutdown requests disconnect for every remaining worker, waits for
  each join with a timeout, and only then shuts down Tokio.

## SFTP browsing and write contract

An SFTP Tab owns one SSH transport whose worker opens only a separate `sftp`
subsystem channel after authentication. It does not allocate a PTY or terminal
shell. The SSH worker remains the sole owner of that russh connection;
application state and Slint see only owned directory DTOs and small browse
intents. A paired Terminal Tab and SFTP Tab never share a russh handle or worker.

The server context-menu **Open SFTP** action creates a standalone SFTP Tab. The
configurable **Switch SSH/SFTP Tab** command instead uses an `AppState`-owned,
non-persistent pair of runtime Tab UUIDs. From an unpaired SSH Terminal it creates
an independently authenticated SFTP Tab immediately after the Terminal; from an
unpaired standalone SFTP Tab it creates an independently authenticated Terminal
immediately before the SFTP Tab. Both paths reuse the normal deny-by-default
host-key and credential flow. Once the pair exists, the command only activates
the companion Tab and does not reconnect or authenticate again. Closing either
Tab unlinks the pair and shuts down only that Tab's browser, subsystem, worker,
and transport; the surviving Tab remains open and may create a new companion
later.

Remote navigation and selection controls are interactive only after that SFTP
Tab reports connected. `AppState` publishes no available remote snapshot before
then, and the application bridge independently rejects operations from a
disconnected or non-SFTP Tab.

When a new SFTP Tab is created, its SSH profile supplies the initial remote
directory to the worker-owned browser and the initial local directory to the
application-owned local snapshot. Missing legacy remote values use `~`; an
empty local value resolves to the platform home directory. These defaults are
used only at Tab initialization, while later navigation remains Tab-local.

The first phase provides a dual-pane directory browser. Slint owns two bounded
splitters: one changes the remote/local widths and one changes the files/transfer
heights. `WorkspaceShell` retains their ratios and the collapsed transfer state
only for the current process lifetime, while `SftpPane` clamps both sides to
responsive minimum sizes. The splitters expose pointer resize cursors, keyboard
focus and arrow adjustment, and slider accessibility actions. Double-clicking
the directory splitter restores equal widths; double-clicking the transfer
splitter collapses or expands the queue. No splitter state enters Rust, the
configuration schema, or the SFTP transport, and the Name/Size/Modified columns
remain fixed responsive columns in this phase.
Each directory header also emits only its current, already-bounded path through
the existing clipboard callback; the copy button does not read a directory or
access an SFTP worker.

The remote side remains the bounded SFTP browser, while
`src/app/local_files.rs` reads local directory metadata only on a Tokio blocking
boundary. Local results carry a Tab-local request identity so late reads cannot
replace a newer path. They are bounded to 250 entries, 256 characters per name,
a 64 KiB aggregate name budget, and 4 KiB paths before reaching Slint. The
remote browser keeps a bounded per-Tab back/forward path history in application
state. History entries are committed only after a directory page succeeds, so a
failed request cannot consume a navigation step; navigation controls are disabled
while a request is in flight. Remote and local rows expose real per-Tab selection
state, with header controls for select-all and clear-all; selection is retained
only for entries still present in the current directory snapshot and does not
start a transfer. Commands and events use bounded channels, requests are
serialized and timed out, inbound SFTP frames are rejected above 256 KiB before
`russh-sftp` parsing, and a raw directory cursor emits at most 250 entries per
page. One directory stops at 2,000 accepted entries or 2 MiB of names and paths;
individual paths/names are also validated and bounded before they enter the
application snapshot. `russh-sftp` still has an internal unbounded packet
sender, so AxSSH limits the browser exposure to one session with one request in
flight.

Each row receives a 24x24 owned RGBA icon from `src/app/file_icons.rs`. The UI
only reads an in-memory result or a built-in folder, symlink, or generic-file
fallback. Platform lookups and image decoding run on a blocking worker, prewarm
at most 64 unique keys per batch, and retain at most 128 process-local entries.
macOS resolves UTType icons through NSWorkspace, Windows uses synthetic file
attributes with the Shell API, and Linux maps extensions to MIME and freedesktop
icon themes. No remote name is treated as a local path for icon resolution, and
Slint never calls a platform or filesystem icon API. The provider is first
created by SFTP icon prewarm, not process startup. Closing the final SFTP Tab
clears resolved extension icons and invalidates pending prewarm generations;
fixed fallback icons remain available.

Double-clicking a local regular-file row is a read-only open intent. The bridge
first requires that exact path in the active SFTP Tab's current local snapshot,
then a blocking worker rechecks the directory and entry with non-following
metadata, rejects directories and symbolic links, and opens a read-only handle.
The handle's platform file identity and length, modification-time, and creation-time
fingerprint must match the fingerprint captured during listing. This fingerprint can
detect only changes observable on the current platform; it is not a content-integrity
guarantee. AxSSH copies from that validated handle into the bounded private open cache and atomically
publishes the snapshot before calling the platform default application through
`open::that_detached`; it never reopens the validated source path. A stale Tab,
directory request, path, or identity/fingerprint mismatch is rejected before dispatch,
and a later path replacement cannot redirect the opener to a different file identity.

Each remote file row owns a Slint-only context menu backed by the shared
`FlatActionMenu`. Activating Download or Delete on an unselected row first
replaces the remote selection with that row; activating it on a selected row
preserves the current multi-selection. The menu then reuses the existing
selection, download, and remove callbacks, so no path, handle, worker, or
filesystem state is added to Slint. Selecting remote files or directories for
Download sends a small download-root intent to the worker. Worker-owned
recursive discovery opens its own SFTP subsystem, rejects
links and unsafe/non-regular entries, and produces owned file requests rooted
in the current Local files directory. A directory retains its relative tree.
Discovery scans at most 4,096 entries and is bounded to 512 files, 256
directories, depth 16, 512 KiB of path text, 1 GiB aggregate bytes, and 512
MiB per file. Each SFTP Tab permits at
most two active or opening transfers, and each transfer owns a separate SFTP
subsystem stream.

Each request revalidates remote path and handle metadata, reads at most 64 KiB
per request, uses a two-chunk writer queue, applies 15-second operation
timeouts and a 30-minute overall timeout, and reports owned queue, state,
progress, and terminal events. The application state owns bounded rows split
into active, failed (including cancelled), and successful snapshots; Slint only
renders those DTOs and sends checkbox/batch pause, resume, or cancel intent.
The active-page batch actions share the right side of the transfer page bar;
they do not reserve a second toolbar row or introduce another callback path.
Pause/resume is a worker-lifetime contract: the writer retains its partial file
and the stream resumes at its current offset only while that worker lives.

The local writer validates every path component, rejects symlink traversal and
existing targets, creates a task-specific `0600` `.part` file on Unix, then
flushes, fsyncs, and atomically publishes the final name without replacing a
concurrent local file.
Cancellation and failures remove the partial data; a cancellation observed after
publication removes the completed target before it can be reported successful.
Completed local downloads are retained. Tab shutdown cancels and joins pending
discovery, subsystem openings, and active transfers. The remote row context
menu owns bounded Download and Delete intents; the remote toolbar retains
rename, UTF-8 edit, and Save As operations. Local regular files
can be uploaded through the same transfer queue. Editor monitoring polls a
remote size/mtime fingerprint while the editor is open. Automatic upload is
explicit and off by default, debounced, and still guarded by the observed
fingerprint. Drag/drop accepts only a bounded path intent and reuses the normal
bridge validation and transfer queue.

## Telnet and serial transport contract

Telnet is intentionally marked as plaintext and never shares SSH credential or
trust fields. `libmudtelnet-rs` owns RFC 854 events, option state, negotiation
responses, IAC escaping, and subnegotiation encoding. A local 64 KiB framing
adapter assembles complete commands, negotiations, and subnegotiations before
calling the parser; it also restores doubled `IAC IAC` bytes as terminal data.
This isolates the parser's confirmed cross-call fragmentation boundary without
reimplementing option semantics. Negotiation commands never enter
`TerminalModel`; supported Echo, Suppress-Go-Ahead, Binary, and NAWS options
receive explicit responses, unknown options are rejected, and NAWS is sent only
after peer acceptance. TCP connect, protocol frames, input/output batches,
errors, queues, and shutdown waits are bounded.

Serial discovery calls the operating system enumeration API on a blocking Tokio
boundary and returns descriptors only. It does not open candidate devices,
toggle modem lines, write probe bytes, or infer baud/parity settings. The
Session Editor requests a scan when Serial is selected or the user explicitly
refreshes the list; application startup performs no serial enumeration. A
connect action performs a fresh scan, resolves a unique saved USB identity when
available, and only then starts one worker-owned serial handle. Missing and
ambiguous matches fail closed. Manual port names remain supported. Serial
parameters and optional non-secret USB identity metadata may be persisted;
device handles and traffic never are. Leaving Serial mode or closing the editor
clears both descriptors and the Slint option model; generation checks discard
late discovery results.

## Logging lifecycle

`src/main.rs` creates exactly one `LoggingGuard` before constructing the UI and
keeps it alive until after the Slint and Tokio lifecycles finish. `src/logging.rs`
writes through a bounded non-lossy queue to daily UTC files, retains at most 15
files, and mirrors `INFO` and higher events to stderr. Dropping the guard writes
the shutdown event, drains the queue, flushes the active file, and joins the
writer thread. Operational fields may include session ID, host, port, and host
fingerprint; credentials and terminal contents are forbidden. About receives
the guard's already-created log directory as an owned path and can open it
through the application bridge without changing the logging owner.

## Persistent settings and font resources

Runtime workspace state is stored separately from `sessions.json` in a private,
atomically replaced `workspace.json`. Its versioned snapshot contains bounded
Tab order and identity, window/pane layout, active/focused Tabs, text-only
terminal contents, and SFTP remote/local paths. It never stores Tokio/russh/PTY
workers, live handles, passwords, vault unlock material, private-key
passphrases, or temporary host-key decisions. On startup, saved profile Tabs
create new workers through the normal host-key and authentication flow; unknown
host keys still require explicit confirmation. Terminal restore is bounded text
replay and does not recreate remote processes or alternate-screen state. Missing
profiles are skipped while the remaining workspace is restored.

`assets/fonts/` contains project-owned Maple Mono NF CN, Iosevka Term,
JetBrains Mono, and Monaspace Neon files with their family-specific notices.
They are not Slint imports. The four JetBrains Mono faces are compiled into the
executable as the always-available application and Terminal default; a Tokio
blocking task reads any selected external bundled family from the AxSSH resource
path. The Slint UI thread registers all bytes with its shared collection. The
first Terminal or local shell Tab uses one application-owned loading path to
ensure that its selected bundled primary family, when applicable, and Maple Mono
NF CN are registered. `FontRegistry::register_loaded_font` is the sole
registration boundary; when Maple is registered, it replaces the shared
Fontique `Hani` fallback list with that one family. There is no renderer-side
font substitution path. Later bundled selections are loaded through the same
registry when a live Settings preview first selects them. All external reads
remain on Tokio blocking tasks. The UI applies the candidate immediately, then
reapplies the current in-memory settings after registration so a delayed font
read cannot restore stale choices. Appearance
owns the application font, display mode, and palette; Terminal owns its font, size, line height,
text brightness, bold-color behavior, optional semantic highlighting and its five color overrides, and terminal interactions. Both font lists
place bundled families first, then a bounded, case-insensitively deduplicated
alphabetical list of system monospace families discovered by `fontdb` on a
Tokio blocking task. `Theme.application-font-family` drives the window default
and explicit non-terminal monospace labels, while `TerminalViewState.font_family`
remains the only primary family source for terminal cell measurement and
rendering; the single `Hani` fallback only supplies glyphs absent from that
family. No font is
loaded from `third_package/axshell` during build or runtime;
release packages must retain `assets/fonts/` by the executable or platform
resource path. Maple Mono NF CN is required there for deterministic Han glyph
rendering; Iosevka Term and Monaspace Neon remain optional primary families,
and all font notices remain required.
Slint measures the configured primary font with 50 Latin cells, the registered
Han fallback with 25 double-width glyphs, and three box-drawing glyphs.
`TerminalPane` uses the largest resulting single-cell advance so Latin,
Han, and box-drawing rendering share one conservative grid metric. Rust
preserves the terminal's logical columns, keeps ASCII text batched, and
publishes non-ASCII cells as independent render runs. The grid centers each
non-ASCII glyph inside its one- or two-cell span, so fallback shaping cannot
move a following ASCII cell. This shared cell width and the configured
line-height percentage drive rendering, selection, cursor, floor-based PTY
columns, and ceiling-based PTY rows. `TerminalPane` computes one
content-space cursor-cell y position; the grid, pre-edit overlay, and native
IME proxy all consume it, while the pane clip is the only vertical overflow
boundary. Every pane bottom-aligns its final terminal row: a partial cell is
clipped from the first row at the top, while space above the maximum row count
remains above the grid. IME and pointer coordinates use that same
origin. The pane group clips every terminal surface to its assigned split
rectangle, and each pane also clips its grid, cursor, preedit overlay, and
transparent IME proxy so an undersized nested split cannot paint into a
neighbor or outside the workspace. The terminal batches the resulting resize
only after those metrics and its layout have settled.

The first Settings opening discovers local shells, system monospace families,
and known X-server installations on blocking workers. Reactivating the existing
singleton tab does not repeat those scans. Closing Settings drops discovered
system font and X-server option models while retaining bundled font choices and
the bounded in-memory shell list. Registered Fontique/Slint families have no
reliable unload API and remain process-wide; platform font/application/icon
databases and the allocator may also retain their own caches after AxSSH drops
its references, so process RSS is not expected to fall immediately.

`UiLanguage` is a config-owned policy with stable `system`, `english`, and
`simplified-chinese` persisted values. Schema version 21 defaults older or
unknown values to System. System resolves Chinese locale families to the
bundled `zh-CN` catalog and every other locale to English. `build.rs` embeds the
reviewed PO catalog; the translation validator requires every static Slint
`@tr` message to have a non-empty translation with the same numbered
placeholders. Slint sends only a stable selector index. The application bridge
persists that language in a dedicated blocking transaction before selecting the
process-wide bundled locale on the UI thread and updating all live main and
detached components. Ordinary Settings preview/save preserves the last committed
language so concurrent preview work cannot overwrite it. Remote terminal
content, user values, logs, and runtime technical error details are not
translated or used as translation keys.

Release automation owns distribution metadata, not application runtime state.
The release author creates an annotated date tag on the default branch. Its
first public tag uses `YYYY-MM-DD`; a positive same-day revision suffix
produces `YYYY-MM-DD-N`. The base date maps to Cargo/Debian
`YYYY.M.D` and macOS build `YYYYMMDD`; a revision maps to Cargo `YYYY.M.D+N`,
Debian `YYYY.M.D-N`, and macOS build `YYYYMMDD.N`, while macOS short version
stays `YYYY.M.D`. `scripts/release_version.py sync` updates the lockfile and
macOS bundle metadata before they are committed with the tag. A direct push
matching the candidate `20*-*-*` pattern starts the single Release workflow. That workflow
verifies the ref is an annotated tag and checks all version representations
before building; `scripts/release_version.py` remains the strict date and
metadata validator. There is no Create or Retry workflow, tag CI dispatch, or
polling chain. The release workflow builds Windows x86_64, Linux
x86_64/aarch64, and arm64/x86_64 macOS binaries; it assembles a universal macOS
bundle and retains `assets/fonts/`, icons, and the independent license notices
in each applicable package. CI writes the shared target-specific Cargo cache
only after a successful default-branch run; failed, pull-request, and tag jobs
cannot save it, while release jobs restore but never write the cache. The
workflow does not read or package
`third_package/axshell`. Before publication, the workflow gives
`scripts/generate_release_highlights.py` only the checked-out tag history and
repository URL; the script returns Markdown, not application state or release
assets. Its curated, de-duplicated commit categories are passed as the Release
body prefix while GitHub-generated notes retain the full commit list.

`SessionStore` writes versioned profiles, non-secret group names, and a
`settings` object to the existing private `sessions.json`. It contains
separate normalized application and Terminal fonts, terminal size, line height,
text brightness, bold-color, optional semantic highlighting and its color overrides, and mouse copy/paste preferences, scrollback, default PTY
    dimensions, local-shell choice and bounded discovered-shell cache, the macOS
    Option-as-Meta preference, sidebar/tab widths, session mask character,
    collapsed group-label character count, shortcuts, `ThemeSettings`, the
    non-secret X11 provider/path/launch/compatibility settings, SSH authentication
    method (including agent selection but no agent endpoint or identity), and the default
    remembered-password backend and interface-language policy. Application callers submit raw values through
    `AppSettingsInput`, grouped into appearance, terminal, workspace, and shortcut
    ownership domains. These inputs contain no Slint values and normalize into the
    same persisted `AppSettings`; they do not leak Slint values into the JSON
    schema. Schema version 22 adds `terminal_text_brightness_percent`, stored from
    60 through 120 with a default of 100, and the default-disabled
    `terminal_semantic_highlighting` switch. Versions through 21 discard the old
    minimum-contrast field and migrate to 100 because there is no safe numeric
    mapping; saved semantic override colors remain available but inactive until
    enabled. Schema version 21 adds the system-aware interface-language policy;
    missing or invalid values follow the system. Schema version 19 adds the SSH SFTP default-directory fields;
    missing remote values default to `~` and an empty local value means the
    platform home directory. Schema version 20 adds five optional Terminal semantic
    color overrides. Empty or invalid values follow the active ANSI palette; non-empty
    values normalize to opaque `#RRGGBB`. Schema version 18 adds the default-disabled
    `copy_selection_on_select` preference; older files preserve their existing
    right-click behavior. Schema version 16 adds the collapsed
    group-label character count; `0` means Full name and missing values retain
    the default of two characters. Schema version 15 adds the independent application font; older files default it to
JetBrains Mono without changing their Terminal font. Schema version 14 replaces
flat SSH-only profile fields with an explicit tagged
`ConnectionProfile::{Ssh,Telnet,Serial}`; legacy flat profiles migrate to SSH,
and only that variant can contain trust or credential references. Schema version
13 adds `terminal.option_as_meta`; missing values from prior files
remain `false` so Option continues to produce native characters and IME/dead-key
input by default. Schema version 12 replaces the legacy
`credential_stored: true` profile marker with
`credential_storage: "system-keyring"`; profiles without a remembered password
omit that field. The alternative encrypted-vault record is stored separately in
the private application configuration directory and never includes its vault
password. Display strategy
is persisted independently as System, Light, or Dark; the selected color family
is AxSSH, Solarized, Arctic, Tokyo, Ember, Forest, or Custom. The fixed
families supply independent Light/Dark semantic palettes; when a palette
resolves Dark, it uses one shared axshell-compatible ANSI-16 table while
retaining palette-specific terminal background, default foreground, and
selection colors. Custom stores
separate Light and Dark sets of 13 canonical `#RRGGBB` or `#RRGGBBAA` semantic
UI/terminal-default colors. Schema version 11 splits the former combined modes:
Solarized Dark becomes Dark plus Solarized, while a legacy Custom palette is
assigned to its matching Light/Dark side and the other side receives a safe
AxSSH default. Theme normalization keeps Light surfaces light and Dark surfaces
dark, requires 4.5:1 contrast for text, focus/accent and status roles, requires
3:1 for essential borders, and repairs unsafe terminal foreground/selection
combinations with same-side defaults.
Schema version 10 promotes legacy profile group values into a normalized,
de-duplicated group list so empty groups and group renames can be persisted.
Schema version 9 migrates the former terminal color scheme into its matching
fixed theme so an upgrade preserves the prior appearance. Shell discovery runs
when Settings is first opened (and is merged into the in-memory settings); the
next explicit settings save persists the bounded list. Earlier migrations retain the schema version 7 compact 220px sidebar
default and the schema version 8 `*` mask default without overwriting custom
values. Passwords, passphrases, private-key contents, terminal output, tab
runtime IDs, child processes, and workers are never serialized.

The config domain rejects control characters and applies shared character
limits to profile names, hosts, usernames, private-key paths, host-key
fingerprints, serial identifiers, and group names. A store contains at most
1,024 profiles and 256 groups, and `sessions.json` is capped at 8 MiB before
deserialization and after encoding. Every deserialized profile and every store
save passes the same domain validation. Private configuration and vault writes
create a hidden, same-directory UUID temporary file with `create_new`, Unix
mode `0600`, and regular-file verification; the guard removes it on any write
or replacement failure. File sync, atomic platform replacement, final private
permissions, and parent-directory sync remain part of the commit, and no fixed
`.tmp` path is followed.

The expanded session sidebar remains available even with no saved profiles so
its blank-area context menu can create an empty group or add a first server.
Manual collapse switches it to the narrow rail, which keeps Local Shell and the
same row/list context menus available. Settings and About remain in the platform
menu and shortcuts instead of the rail.

`src/app/view.rs` sends both validated Light and Dark sides of the selected
palette to `ui/theme.slint`. System leaves the standard-widget
`Palette.color-scheme` as `ColorScheme.unknown` so Slint follows the runtime
platform palette; manual Light and Dark modes set it explicitly. One
`resolved-dark` selects the matching palette side, standard-widget direction,
custom AxSSH surfaces, and terminal ANSI palette. Theme state tokens name
dividers, frame/control borders, focus, hover, and selected surfaces so shared
components do not reinterpret base colors independently. Native
`ContextMenuArea` rendering remains
platform-owned, so its exact colors can differ even though its mode selection is
consistent. The theme also owns type scale, spacing, radii, standard workspace
geometry, Settings control dimensions, editor widths, and overlay sizes.
`ui/components/themed-combo-box.slint` owns every in-app selection control that
requires exact AxSSH colors. Its control surface, popup, hover and selected
rows, focus border, chevron, and scroll indicator consume semantic `Theme`
tokens rather than the Slint widget palette. It preserves the bounded string
model, current-index, selected callback, keyboard navigation, outside-click
close behavior, and combobox accessibility contract. Other standard widgets
continue to use the synchronized Slint `Palette`; native `ContextMenuArea`
menus remain platform-owned.
`ui/components/elided-controls.slint` owns the shared single-line text-button
presentation contract. `ElidedLabel` measures the configured font separately
from its visible elided label and exposes `natural-width`, `line-height`, and
`overflowed`; it creates a bounded, wrapping full-text tooltip only when the
label actually overflows. `ElidedButton` keeps the standard Slint `Button` as
the focus, keyboard, enabled, pressed, accessibility, and click owner while
overlaying that label. Callers pass display `text`, optional `tooltip-text`,
independent `accessible-name`, `enabled`, and `clicked()` explicitly. Icon-only
buttons keep their purpose-specific tooltips. These UI-local values do not
cross into Rust, persistence, diagnostics, workers, or transports.
`ui/components/flat-text-input.slint` owns the matching theme-native single-line
control for non-secret Settings, session-editor, and management-dialog fields.
It keeps native cursor placement, selection, IME, keyboard focus, accessibility,
and the standard edit context menu.
`ui/components/secret-text-input.slint` is the separate password-only control
for SSH passwords, vault passwords, and private-key passphrases; it deliberately
does not inherit ordinary selection or copy/cut behavior. Numeric inputs remain
standard `SpinBox` controls so their range and increment semantics are not
reimplemented.
`ui/components/settings-controls.slint` consumes those tokens to provide the
shared Settings glyph, navigation, page, compact right-aligned field, row,
toggle, shortcut, action header, fixed search field, and result row primitives.
Setting rows keep a stable title and metadata column while standard controls use
one theme-configured height. `SettingsPage` provides the common detail scroll
container, while the longer Appearance, Terminal, and About pages retain their
equivalent page-local scroll containers.
`ui/settings.slint` owns the shared draft behind its read-only
`SettingsViewState` boundary, the non-persistent selected category/search
query, and a bounded result list generated by the application bridge. It
coalesces edits into an immediate in-memory preview. Closing its tab starts the
separate close-to-save transaction; the category layouts live in
`ui/settings/*.slint` with only their relevant local draft properties and
callbacks.
`ui/settings/appearance.slint` separates Display mode from Color palette and
uses one shared `ThemePaletteEditor` component for the Custom Light and Dark
fields, preventing the two editors from drifting structurally.
`src/app/view.rs` maps the current settings theme into the Slint global and
re-renders only the active terminal snapshot when its resolved colors change. Terminal rendering
uses the resolved default foreground, background, and selection colors while
retaining ANSI 16/256/true-color semantics. When explicitly enabled, its bounded
semantic overlay only changes eligible plain visible cells and derives distinct
link, success, information, warning, and error colors from the active terminal
palette unless a normalized Settings override is present. The single foreground
pipeline resolves ANSI/indexed/true color, bold-color selection and inverse,
chooses the optional semantic foreground, and finally applies the configured
0.60-1.20 HSL-lightness factor once. Factor 1.00 preserves every non-dim resolved
foreground exactly. `dim` multiplies that final factor; backgrounds, selection,
and cursor colors bypass the adjustment. A coalesced appearance refresh is
scheduled after all Slint settings properties are applied. A theme refresh never
resizes a PTY, sends worker commands, or changes SSH/local-shell lifetimes.
Runtime terminal geometry and user choices remain in versioned `AppSettings`;
the Theme global remains a visual resolver rather than a persistence owner.

## Staged scope

The current application validates and persists SSH, Telnet, and Serial profiles;
confirms per-profile SSH host fingerprints and reads the user's OpenSSH
`~/.ssh/known_hosts` as a bounded shared trust source; authenticates SSH with transient
passwords, local private keys, or a bounded runtime SSH agent; provides bounded remote SFTP and local metadata
directory browsing plus regular-file download-to-open for an authenticated SSH
Tab; and owns multiple independent transport or
local-shell terminal tabs, including duplicate targets. New-session editing and the singleton Settings
workbench remain visible workspace tabs; only short-lived trust and secret
prompts remain overlays. The following remain
separate steps:

- richer known-hosts administration beyond the shared parser, including an in-app
  revoke/replace UI and system-wide policy editing;
- richer SFTP conflict resolution and cross-process edit recovery;
- persisted workspace restoration beyond the bounded in-process reconnect policy;
- richer full-screen terminal compatibility and mouse reporting.
