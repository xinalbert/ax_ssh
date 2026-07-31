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
| `src/app/{workspace,connection,connection_monitor,terminal_bridge,settings_bridge,view}.rs` and `src/app/connection/` | Private application-bridge feature wiring, including focused connection request/probe, host-key, authentication, and worker-start flows | Generated type declaration, transport implementation, or persistence schema |
| `src/app/state.rs` and `src/app/state/` | UI-independent workspace tabs, per-tab terminal/worker state, attempt transitions, and their tests | Slint component/model types or russh protocol details |
| `src/app/{input,session_groups,terminal_render,credential_tasks}.rs` | Testable input/group/render mapping, theme-aware terminal defaults, and blocking credential task boundary | Window ownership, transport handles, or mutable UI state |
| `src/config.rs` and `src/config/` | Stable config entry and explicit exports; session/profile domain, settings, theme normalization, legacy migration, private JSON persistence and atomic replacement | Slint types, network connections, plaintext password storage |
| `src/credentials.rs` | Profile-scoped system-keyring and encrypted-vault records | UI state, plaintext configuration, SSH transport handles |
| `src/terminal.rs` and `src/terminal/input.rs` | Bounded vt100 grid, cell styles, cursor/scrollback state, selection extraction, and terminal key encoding | Slint types, network handles, credentials |
| `src/local_shell.rs` | Cross-platform shell discovery and one bounded worker-owned local PTY process per tab | Slint state, SSH trust, persisted terminal contents |
| `src/ssh.rs` | russh handler, host-key decision, authentication, shell channel boundary | Window updates, persistent session mutation, UI formatting |
| `src/ssh/private_keys.rs` | Local `.ssh` private-key discovery and blocking key loading | Passphrase persistence, UI state, host trust decisions |
| `src/ssh/worker.rs` | Bounded shell input commands, coalesced resize state, batched output events, cancellation, and shutdown | UI state or profile persistence |
| `src/logging.rs` | Global tracing subscriber, log directory, daily rolling writer, retention and flush guard | Credentials, feature state, UI or SSH handles |
| `src/main.rs` | Process startup and logging-guard lifetime | Feature logic |

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
callback. `TerminalPane` receives a read-only `TerminalViewState` and owns only
terminal-local focus, IME proxy, selection, cursor blink, and measured sizing.
It never owns a worker, a terminal buffer, or connection state.
Its internal `TerminalGrid` receives the smaller `TerminalGridView` and
`TerminalSelectionView` DTOs: it renders the bounded snapshot and turns
pointer, scroll, and context-menu gestures into callbacks, while `TerminalPane`
retains the focus, IME input, selection draft, and resize lifecycle.
Its `key-pressed` handler sends only special keys and terminal control chords to
Rust; printable keys, Shift text, and committed IME text remain in the native
`TextInput.edited` path.

`SettingsPane` receives a read-only `SettingsViewState`, copies it into its
private editable draft, and emits the candidate only from Save. A menu or native
platform request provides a read-only requested section; the pane owns the
currently selected section while navigating. `SessionEditorPane` follows the
same pattern with `SessionEditorViewState`: it resets its private fields only
when the incoming draft identity changes, and never mutates the Rust snapshot
while the user types. `in-out` properties remain inside components only where
two nested controls are editing the same local draft. Derived labels, dialog
copy, and visual states are bindings, not duplicate mutable storage.

`OverlayHost` owns the local group/profile-management dialog open state and
draft, deriving its title, message, and button presentation from one action
value. It forwards a management command only after confirmation. It also
composes the SSH host-key and authentication dialogs, but these are intentionally
different: their visibility and prompt identity are read-only Rust-owned
security phase. The UI may submit confirm/reject/authenticate/cancel intent, but
must not locally hide either dialog before the Rust state transition accepts it.

## Event flow

1. A Slint callback produces a small value such as a saved profile ID, unique
   tab ID, terminal key/modifier tuple, draft fields, a trust
   decision, or one transient secret. Authentication secrets travel through the
   dedicated SecretTextInput, whose UI value is cleared after the application
   accepts it or when the prompt is cancelled.
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
   Each SSH Tab also owns its current connection phase: idle, cancellable host-key
   probe, pending host-key confirmation, pending authentication, or stored-
   credential loading. There is no global pending-probe, trust, or authentication
   slot.
3. After explicit confirmation, the controller atomically persists the exact
   fingerprint. Password profiles load a remembered credential on a Tokio
   blocking boundary or open a password prompt. Private-key profiles load the
   selected path off the UI thread and request a transient passphrase only when
   the encrypted key cannot be opened without one. The security overlay renders
   only the active Tab's pending phase; inactive Tabs retain their own prompt
   until activated, and changing an authentication prompt clears its secret
   inputs before it becomes visible.
4. Settings > General owns the default backend for a newly remembered SSH
   password: the platform credential store or the encrypted application vault.
   The session editor never receives a password. A profile stores only an
   optional backend reference after a successful remembered-password write, so
   changing the default neither migrates nor breaks an existing credential.
   The application writes the selected backend only after SSH authentication;
   the backend record and profile reference are rolled back together if either
   persistence step fails. Deleting a profile, switching it to private-key
   authentication, or rejecting a stored password removes its referenced
   credential transactionally without stopping an already-open terminal worker.
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
   shell input; the optional right-click action chooses between them based on
   selection state.
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
   the profile UUID through the existing connection callback. File > New
   Session and the sidebar blank-area context menu remain distinct
   session-editor actions.
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
12. The session navigator owns the Slint-local sidebar expanded/collapsed state
    and each Group's disclosure state. Rust supplies a complete, read-only
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
    two-character badge derived from the group name rather than a folder icon.
    A separate compact panel control is the only action that expands or
    collapses the sidebar. In the expanded sidebar it sits at the trailing
    edge of the Local Shell row; in the collapsed rail it remains a top control.
    Custom group rows are keyboard focusable and use Enter/Space for the same
    local disclosure action as a click; this never changes the sidebar state. Native
    row context menus create a server in a group, rename or delete a group, and
    connect, edit, or delete a server. Ungrouped exposes only its add-server
    action. Right-clicking blank list space creates an empty group or an
    Ungrouped server. `SessionActionMenu` maps these four menu shapes to flat
    `ActionMenuItem` lists. `FlatActionMenu` composes exactly one
    `ContextMenuArea`, emits only an action ID, and exposes `show-at(Point)` so
    the same action list can also back a button-triggered dropdown. Deleting a
    group moves its profiles to Ungrouped; deleting
    a profile removes only its persisted definition and credential. The
    collapsed rail renders a larger Group badge and smaller, tightly stacked
    server badges, while Local Shell keeps its dedicated entry. The application
    formatter masks usernames and IPv4 middle octets before data enters the
    Slint model. Static geometry is in `ui/theme.slint`; the persisted
    single-character mask setting is owned by `WorkspaceSettings`.

## SSH security contract

`russh::client::Handler::check_server_key` is the trust boundary. Unknown and
mismatched keys are rejected before authentication. A rejected first-contact
handshake may expose its SHA-256 fingerprint to the confirmation UI, but only
an explicit user decision adds that exact fingerprint to the profile. A changed
key requires a second explicit decision. Passwords are transient callback
inputs and are not part of `SessionStore`. A password profile contains only an
optional `credential_storage` reference keyed by its stable UUID, never the
password or a vault password. Settings > General selects the backend used by a
future checked **Remember password** action: macOS Keychain, Windows Credential
Manager, or Unix Secret Service for the system backend; or a per-profile
application-vault record. The vault derives a per-record key with Argon2id,
encrypts with XChaCha20-Poly1305 using the profile UUID as associated data, and
keeps the vault password transient. Private-key profiles persist only a path.
The key bytes and optional passphrase are loaded in one blocking task, used for
one authentication attempt, and then dropped without entering configuration,
tracing fields, or UI models.

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
- the bounded command channel carries shell input, disconnect, and cancel
  intent; a watched terminal size coalesces high-frequency resize updates;
- terminal output is capped per batch and backpressured through a bounded event
  channel before entering the bounded terminal model;
- worker events report connected, resize, output, disconnected, host-key
  rejection, credential failure, or a capped error message;
- each SSH Tab independently owns probe cancellation and its authentication
  phase; every UI callback and delayed probe, credential, or worker result
  revalidates the Tab, profile, attempt, and expected phase before changing it;
- cancel interrupts connection/authentication as well as an established session;
- a 20-second keepalive with three missed-reply limit and the 90-second
  transport inactivity boundary decide connection liveness; a quiet shell data
  channel is valid and never has its own output timeout;
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

`SessionStore` writes versioned profiles, non-secret group names, and a
`settings` object to the existing private `sessions.json`. It contains
normalized font, size, line height, terminal
brightness, bold-color and right-click behavior, scrollback, default PTY
dimensions, local-shell choice and bounded discovered-shell cache, the macOS
Option-as-Meta preference, sidebar/tab widths, session mask character,
shortcuts, `ThemeSettings`, and the default remembered-password backend. Schema
version 13 adds `terminal.option_as_meta`; missing values from prior files
remain `false` so Option continues to produce native characters and IME/dead-key
input by default. Schema version 12 replaces the legacy
`credential_stored: true` profile marker with
`credential_storage: "system-keyring"`; profiles without a remembered password
omit that field. The alternative encrypted-vault record is stored separately in
the private application configuration directory and never includes its vault
password. Display strategy
is persisted independently as System, Light, or Dark; the selected color family
is AxSSH, Solarized, or Custom. Custom stores separate Light and Dark sets of 13
canonical `#RRGGBB` or `#RRGGBBAA` semantic UI/terminal-default colors. Schema
version 11 splits the former combined modes: Solarized Dark becomes Dark plus
Solarized, while a legacy Custom palette is assigned to its matching brightness
side and the other side receives a safe AxSSH default. Theme normalization keeps
Light surfaces light and Dark surfaces dark, requires 4.5:1 contrast for text,
focus/accent and status roles, requires 3:1 for essential borders, and repairs
unsafe terminal foreground/selection combinations with same-side defaults.
Schema version 10 promotes legacy profile group values into a normalized,
de-duplicated group list so empty groups and group renames can be persisted.
Schema version 9 migrates the former terminal color scheme into its matching
fixed theme so an upgrade preserves the prior appearance. Shell
discovery validates the saved cache and appends only newly available names after
load. Earlier migrations retain the schema version 7 compact 220px sidebar
default and the schema version 8 `*` mask default without overwriting custom
values. Passwords, passphrases, private-key contents, terminal output, tab
runtime IDs, child processes, and workers are never serialized.

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
toggle, shortcut, and action header primitives. Setting rows keep a stable
title and metadata column while standard controls use one theme-configured height.
`ui/settings.slint` owns the shared draft and one Save transaction behind its
read-only `SettingsViewState` boundary, while the category layouts live in
`ui/settings/*.slint` with only their relevant local draft properties and
callbacks.
`ui/settings/appearance.slint` separates Display mode from Color palette and
uses one shared `ThemePaletteEditor` component for the Custom Light and Dark
fields, preventing the two editors from drifting structurally.
`src/app/view.rs` maps a saved theme into the Slint global and re-renders only
the active terminal snapshot when its resolved colors change. Terminal rendering
uses the resolved default foreground, background, and selection colors while
retaining the existing ANSI 16/256 palette semantics. A theme refresh never
resizes a PTY, sends worker commands, or changes SSH/local-shell lifetimes.
Runtime terminal geometry and user choices remain in versioned `AppSettings`;
the Theme global remains a visual resolver rather than a persistence owner.

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
