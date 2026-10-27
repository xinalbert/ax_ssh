[简体中文](egui-migration.zh.md) · [Documentation index](README.md)

# egui UI Migration Plan

## Status and deliverables

- Goal ID: `20261022-egui-migration`.
- Branch: `codex/egui-ui-migration`, created from `master` at `15404e1`.
- Implemented: optional egui/eframe 0.37.0 entry, Rust 1.95.0, isolated logs, local PTY/controller and Painter terminal; E3 shared terminal/viewport state and connection identity/reconnect policy; E4 temporary SSH/Telnet/Serial, trust/auth prompts and bounded reconnect; E5 saved sessions/groups, tabs, eight-pane layouts and isolated recovery. Native GUI and performance acceptance remain open.
- Objective: replace the Slint frontend while preserving terminal, connection, configuration, and security behavior.
- [Architecture](architecture.md) describes the implementation; the tracker separates implemented E1–E5 slices from native acceptance and remaining E3R/E6–E9 work.

## Architecture decision D1: independent frontend, terminal first

Keep a runnable Slint baseline during the transition and add a separate egui executable.
Each frontend runs independently; do not run two GUI event loops in one process.
Deliver a real local PTY terminal first, extract shared application control as needed,
and then migrate the full workspace. Extract concrete reusable behavior rather than
building a general UI framework in advance.

egui owns layout, local interaction, font/texture/GPU resources, and platform window
adaptation. The controller owns commands, tabs/connections, background operations,
terminal parsing, persistence, and shutdown. Tokio, russh, and `alacritty_terminal`
retain their responsibilities. Rendering callbacks must not perform network/disk
operations, parse terminal streams, or wait for tasks.

This decision does not satisfy cutover gates. E1 updates `AGENTS.md`, the project
skill, bilingual architecture, and checks. E9 records final cutover. The current
controller owns the experimental temporary terminal and shared mouse encoding,
output effects, viewport/selection state, frame admission, cleanup, and non-secret
connection identity/reconnect state. E4 adds a bounded temporary connection/prompt owner.
Remaining workspace/security/settings extraction proceeds with its feature (E3R).

## Reuse and replacement map

| Current path | Treatment | Constraints |
| --- | --- | --- |
| `src/{ssh,sftp,telnet,serial,local_shell}.rs` and children | Retain transport/domain implementations | Keep existing frameworks and independent SSH/SFTP handle ownership |
| `src/{config,credentials,audio,media,logging,x_server}.rs` and children | Retain domain, storage, and services | Adapt settings only where frontend semantics actually change |
| `src/terminal.rs`, `src/terminal/`, `src/terminal_dimensions.rs` | Retain parsing, snapshots, selection, encoding, dimensions | Font advance must not determine logical column occupancy |
| `src/app/state/`, `panes.rs`, `worker_shutdown.rs` | Extract UI-independent portions | State still references media and external editing services; a directory move alone is insufficient |
| `src/app/terminal_render.rs` and children | Reuse palette, style, font-role, and geometry rules | Existing geometry uses path strings and needs an egui geometry adapter |
| `src/app/{connection,connection_monitor,workspace,sftp_bridge}*` | Separate orchestration from Slint notifications | Preserve attempt/request checks, transactions, and cancellation; avoid duplicate business logic |
| `src/app/{view,input,font_bridge,window_router,window_bridge,text_editing}*` | Replace or split frontend adapters | Input, fonts, windows, and tests contain Slint-specific behavior |
| `src/file_icons.rs`, `src/file_icons/` | Shared platform lookup/cache and bounded owned RGBA | E6 removes the Linux Slint decoder; both frontends consume the same service |
| `ui/` | Reimplement by feature | 47 Slint files and 16,353 lines; avoid mechanical syntax translation |
| `build.rs`, `build/`, `translations/`, `scripts/`, CI/packaging | Migrate alongside each stage | Preserve icon/revision/language generation, documentation checks, and release metadata |

## Proposed modules and build organization

Keep one Cargo package. A candidate layout, refined during E2/E3 review, is:

```text
src/main.rs              existing thin Slint startup
src/app.rs + app/        transitional Slint adapter
src/egui_main.rs         egui logging/startup only
src/egui_app.rs          egui composition and startup
src/egui_app/            workspace, terminal, sftp, settings, theme, fonts, platform
src/controller.rs        intentional shared application facade
src/controller/          state, commands, events, connections, presentation, shutdown
src/lib.rs              existing domains plus explicit minimal controller exports
```

Keep internals private. Expose only the controller facade, commands, and owned
snapshots required by the executable boundary; retain field encapsulation.
Generated Slint declarations stay in `src/app.rs`; egui types stay in its frontend
and platform adapters.

E1 introduces a default `slint-ui` feature and an optional `egui-ui` feature.
The current `ax_ssh` binary requires `slint-ui`; the experimental `ax_ssh_egui`
binary requires `egui-ui`. Both may compile together but run independently.
Set `default-run = "ax_ssh"` explicitly so adding another binary preserves the
existing startup command.
Gate Slint runtime, `i-slint-core`, and `slint-build`; the Slint compilation helper
in `build.rs` itself needs compile-time cfg so disabled dependencies are not referenced.
Continue generating Windows icons, revision, and the common language registry.
An egui-only build must not compile Slint or its vendor patches indirectly.

Both entries are now available:

```bash
cargo run --locked
cargo run --locked --no-default-features --features egui-ui --bin ax_ssh_egui
```

CI covers each frontend and their combined features. The experimental application
defaults to separate configuration, workspace, and log directories. Tests use
isolated storage and injected credentials. Reading existing configuration for
comparison must not implicitly write it back or connect sessions.

## egui version and graphics backend

- The branch uses `egui/eframe 0.37.0` with WGPU because its system-font fallback
  and color emoji improvements matter to terminals. Package MSRV and CI are
  raised together to Rust 1.95.0; the installed minimum toolchain passes the
  egui-only check locally.
- The earlier 0.35/Rust 1.92 alternative was not selected. Slint remains the
  default entry and its version is unchanged.
- Start with egui Painter, cached text layouts, and geometry batches. Consider
  custom WGPU callbacks or glyph atlases only after measuring a bottleneck.
- WGPU/OpenGL are standard eframe paths. Current CPU rendering, damage presentation,
  and GPU-failure fallback do not transfer automatically. E1 records a support
  policy; E9 requires a validated replacement or retained supported path.
  A software GPU driver is not equivalent to the existing CPU renderer.
- Validate Windows DX12, macOS Metal, and Linux Vulkan/GL selection and diagnostics
  natively. Keep diagnostic overrides; do not inherit reference-project driver
  exclusions without AxSSH evidence.

Sources: [0.37 manifest](https://github.com/emilk/egui/blob/0.37.0/Cargo.toml),
[0.35 manifest](https://github.com/emilk/egui/blob/0.35.0/Cargo.toml),
[0.37 release](https://github.com/emilk/egui/releases/tag/0.37.0), and
[eframe features](https://github.com/emilk/egui/blob/0.37.0/crates/eframe/Cargo.toml).

## Controller/UI data flow

1. UI emits typed intents carrying window/tab IDs and required attempt/request
   IDs. Multiple egui layout passes must not duplicate connections, saves,
   keystrokes, or uploads; execute each user action once.
2. A bounded controller command interface executes operations. Report input
   overload explicitly; coalesce resize/motion only under existing contracts.
   Credentials use ephemeral dedicated payloads, excluded from printable commands,
   ordinary snapshots, and logs.
3. Preserve every transport byte in order through parsing and protocol responses.
   Latest-value replacement must never discard terminal bytes or user input.
4. Coalesce only parsed presentation state and refresh notifications. Keep one
   latest bounded snapshot per tab and coalesce wakeups by window. After skipped
   snapshots, compare line revisions, union changed rows, or refresh fully;
   consuming only the final `dirty_rows` loses updates.
5. UI reads owned snapshots through a facade and releases synchronous locks before
   drawing. It receives no worker handles, Tokio receivers, or persistence locks.
   Foreground/background/hidden presentation remains independent of parsing/input.
6. The frontend installs a removable wake callback; the controller knows window
   IDs and notifications, not `egui::Context`. Stable UUID-derived widget IDs
   preserve focus, selection, and edit drafts.
7. Close cancels security/media/clipboard intents, requests disconnect, and
   registers cleanup before removing tab routes. Exit drains cleanup and workspace
   writes; do not terminate the process to bypass shutdown.

SSH trust remains deny-by-default with explicit confirmation. Late results still
validate tab/profile/attempt/phase. Retain SFTP request identity, edit-conflict,
and upload-confirmation behavior. Tests must not disable security checks.

## Terminal implementation

The first E3 slice is now consumed by both frontends. Shared `TerminalOutput`
drains protocol replies, title/bell/clipboard effects and synchronized-output
hold state without platform actions. Shared `TerminalFrames` retains admitted
frames and merges pending row damage; each frontend owns its scheduling policy.
Shared `WorkerShutdowns` retains cleanup even when a tab or completion receipt
is dropped, then drains or aborts and joins tasks at the exit deadline.
Mouse transactions use the same model modes and 256-report wheel bound.
Slint retains its title/clipboard identity and permission checks; egui discards
titles and routes clipboard effects through E7P below. Broader application state extraction remains in E3.

The second E3 slice shares `TerminalViewport`: resize, history scrolling and
returning to live output invalidate stale frames and selection coordinates as
one transition. Repeated dimensions and zero-distance scrolling preserve the
selection identity; output and presentation-only invalidation preserve it too.
Both frontends use the same nonnegative revision, with explicit wrap to 1.
The frame cache is internal to the controller. PTY/controller resize deduplication
records successful admission, so a failed resize remains retryable without
changing the visible model. A real PTY regression rejects copy/selection requests
that still carry the revision from before a resize.

The third E3 slice extracts protocol/profile/attempt identity and the five-attempt
reconnect policy. Existing application adapters consume those controller types;
the egui-only library compiles and tests them without Slint. Tab lookup and
profile/attempt checks still precede state changes; cancellation and manual retry
advance the generation and revoke application-owned media/clipboard work.
The controller types contain no worker, trust decision, secret or UI value.
This slice prepares E4; it does not add remote connection controls to egui.

- Draw the visible grid only. Keep scrollback in the model; do not create a huge
  text widget or one egui widget per cell.
- Derive cells from DPI, font size, and measured monospace metrics. Drawing,
  hit-testing, selection, IME, and remote resize share an origin. Preserve full-row,
  top-remainder, tiny-window clipping, and maximum-dimension rules.
- Cache layouts/drawing data by row revision, theme, size, font generation, and
  DPI. Bound caches and release them for hidden/closed views. Start with styled
  text/background runs and Painter geometry; overlay cursor, selection, links,
  and composition.
- Keep existing JetBrains Mono, Maple, Iosevka resources and licenses. Read/discover
  fonts in background and register on the UI thread. CJK, combining marks, ZWJ
  emoji, box drawing, italic, and fallback glyphs retain model cell occupancy.
- Implement terminal input separately for physical keys, committed text, paste,
  and IME preedit. Send only commits. Verify Option/Command, Control/Alt, keypad,
  undo, and menu consumption without duplicate input or password exposure.
- Preserve application/normal cursor keys, bracketed paste, focus/mouse protocols,
  OSC color/size/title, synchronized output 2026, and OSC 52 permission flow.
- Repaint on demand while idle; schedule blink/visibility changes and retain
  bounded presentation throttling during continuous output.

## E4 temporary remote connections

The experimental terminal now replaces its initial local shell with an explicit
SSH, Telnet or Serial draft. It reuses existing workers rather than implementing
another protocol layer. `TerminalSession` owns the active receiver/worker, parser,
prompt identity, ephemeral secret and cancellable retry deadline. Replacement drops
the old receiver and joins its worker first; input and copy/selection results are
bound to the displayed connection generation. Reset clears old frames and advances
the selection revision rather than reusing coordinates across connections.

SSH probes before authentication, offers trust once for unknown keys, and rechecks
the pinned fingerprint in the actual authenticated transport. Known-hosts changes,
revocation and a probe/worker key race stay blocked. Prompt tokens are exact and
single-use; confirmation and authentication retry require fresh credentials.
No profile, known-hosts or credential writes occur. Quick SSH defaults to X11 Off;
E7X exposes explicit Off/-X/-Y selection and local settings. E7A adds a
default-off microphone choice for the current SSH connection and its retries.
It is not persisted in profiles/workspaces; replacement/disconnect clears it.
Saved PCM rate/channels reach new workers through the settings watch. Existing
SSH/audio owners keep trust, authentication, recording-demand, device-permission
and bounded cancellation/join gates. Remote preparation failure preserves shell
access; native audio acceptance remains open. Image paste is covered by E7I below.

Optional reconnect consumes the shared five-attempt budget with 1/2/4/8/16-second
delays. SSH password/passphrase retries pause for fresh input; agent and direct
transports retry without retained secrets. Manual disconnect/replacement cancels
the deadline. Tests exercise real loopback SSH/Telnet trust, authentication,
input/output/protocol replies, pixel resize, cancellation, replacement and retry;
serial failures are tested without hardware. Native servers/agent/key prompts,
serial devices and GUI acceptance remain open as E4V. E5 adds saved sessions, sidebar/editor, tabs/panes and workspace recovery as described below. This does not mark E2V or broad
application extraction complete.

## E5 saved sessions and workspace

The default application and egui now share `PaneTree`, including directional focus,
divider ratios, the eight-pane bound and validated restore. The experimental
workspace owner adds a 64-command facade, catalog revision/request receipts,
atomic background persistence, close/join ownership and a 32-terminal bound that
includes pending cleanup. egui composes an isolated session/group sidebar,
credential-free profile editor, Tab order, split panes and per-UUID terminal/input
caches. Password widgets are pane-scoped; disabled editor/modal states do not send
text to terminals, and layout passes admit a user action once.

The experiment uses its own ConfigStore and workspace schema. Changed layout
metadata checkpoints every two seconds and on exit; `.axssh` import/export uses
existing bounded atomic file helpers. Invalid/unsupported imports leave the active
workspace intact, and corrupt startup files are preserved. All restored terminals
start idle, including local shells. Saved session references prefill connection
fields; temporary targets/drafts, terminal output/history, credentials and
trust-once pins are omitted. Native file pickers/recent menus, credential stores,
full old-workspace parity and broader settings/translation support remain future
surfaces, and E5V tracks native focus/layout/restore acceptance. This delivers E5I
without claiming the final compatibility/performance cutover gates.

## E6 platform file icons

Both frontends now share `src/file_icons.rs` and its platform resolvers. The
Slint adapter only re-exports the service; the experimental companion adds type
keys to remote/local entries and publishes at most 64 shared 24×24 RGBA images.
One retained blocking prewarm task refreshes the current directory snapshot and
is joined on shutdown. The provider keeps its 128-entry cache, bounded extension
keys, platform/theme identity invalidation and deterministic fallbacks.

egui owns a 64-texture cache per pane, updates changed pixel allocations only,
and releases textures for hidden/closed panes or minimized windows. Linux SVG
uses resvg 0.48.1 with external/embedded image resolution and default features
disabled; SVG alpha is converted to straight RGBA. The host-key receiver remains
borrowed during icon/command wakeups, and its task is cancelled/joined at teardown.
Native theme/visual acceptance and full drag/drop parity remain E6 gates.

## E6 external edits

The experiment now uses the shared edit state/private-copy service with explicit
Edit, Reopen, Upload changes, Stop and automatic-save controls. One retained local
job polls stable file changes; metadata-only snapshots never carry document bytes.
The cache is isolated; copies survive close, while unpublished copies are cleaned.
Exact edit/operation IDs and connection generations reject stale jobs and replies.
The existing write worker enforces remote conflicts and atomic replacement. E7T
now applies the configurable edit limit (default 4 MiB, range 1–64 MiB). Native
application associations, live edits and remaining drag/drop parity need acceptance.

## E6 bounded drag/drop

The experiment accepts native files/folders only over its visible remote-file
area and supports same-pane local-to-remote or remote-to-local copy drags.
Payloads and intents are bound to pane/connection/directory identities. One
retained blocking task prepares recursive uploads, revalidates before transport
admission and is cancelled/joined at teardown. Shared discovery preserves relative
paths and skips links/filtered names. Native coordinate lookup stays in the egui
adapter; absent or stale coordinates never fall back to an active tab. Wayland
requires a current-frame pointer event. Selected rows now carry the current bounded
selection through the existing batch upload/download intents; native drag-out promises,
complete SFTP parity and native drop acceptance remain pending.

## E7/E8 menus, tabs and settings foundation

At the user's request this slice precedes the remaining E6 parity work. It follows
Slint's File/Edit/View/Pane/Window/Help structure and singleton Settings tab.
macOS uses AppKit's application menu (Settings, About, Hide and Quit) and registered
Window/Help menus; Windows attaches a Win32 menu to eframe's HWND. Both use pinned
[muda 0.19.3](https://docs.rs/muda/0.19.3/muda/), with GTK/default features disabled.
Linux uses [egui's MenuBar](https://github.com/emilk/egui/blob/0.37.0/crates/egui/src/containers/menu.rs)
inside the existing winit window on X11 and Wayland; GTK native menus need a GTK
window host and are not claimed here. All platforms retain native window decorations.

One action table supplies menus, enabled states and keyboard routing. AppKit owns
its accelerators; Windows/Linux use exact, non-repeated egui chords, preserving
normal text-edit clipboard events. A bounded 32-event queue wakes the UI without
polling. Native menus detach before window destruction; Quit follows normal cleanup.
Tabs have stable IDs, horizontal scrolling, drag reordering, middle-click/explicit
close, context actions and a new-tab picker. Settings is unique, persists its
identity/order, never owns a transport and cannot join a terminal split.

Settings exposes System/Light/Dark, six shared palettes plus Custom Light/Dark palettes,
terminal size/line height
and cursor blink, new-local-tab shell, sidebar/tab widths and three editable app
shortcuts. Draft preview is local; Save uses the owner's bounded revision/request
transaction and existing private atomic ConfigStore writes. Failed/stale writes
retain drafts and saved values. Revert/Discard restore saved rendering; closing a
dirty tab or window offers Save/Discard/Cancel, and opening another workspace
requires resolving that draft first. Unexposed configuration domains are preserved.
Remaining interface translations, media settings and multi-window migration remain
later E7/E8 work; native menus and layout/focus still require user acceptance.

E7T adds terminal history size, default columns/rows and alternate-scroll Home
behavior, plus SFTP connection concurrency, file size/bandwidth, external-edit
limits and name filters. Copy/paste shortcuts are editable with semantic conflict
checks. Drafts preview appearance only; after a successful disk write, workspace
notifies workers through bounded latest-value watch channels. Default size/shell
apply to new terminals; history and wheel settings update existing terminals.
History changes revoke old selections and pending copy results. SFTP connection
limits are captured on the next connection, with saved profile overrides taking
precedence. Filters/edit limits apply to new operations; old upload preparation is
cancelled and joined. Blocking edit jobs capture their admitted limit, and upload
completion rechecks the current limit. Other settings remain E7R work;
E7P handles Option/Meta and OSC 52 below, E7U global uploads and E7O local opening.

E7B adds Appearance controls for terminal text brightness (60–120%, in 5-point
steps) and bright ANSI colors for bold text. Both frontends share the existing
HSL lightness calculation; egui applies it after inverse and before hidden,
including underline/strikethrough, without dimming alpha or surfaces. Low ANSI
foregrounds alone are promoted for bold; the bold font remains independent.
The settings participate in row cache invalidation, so idle terminals preview
and revert immediately. Failed saves retain the draft and saved values.
Remaining display settings stay in E7R; GUI acceptance remains separate.

E7R-Theme adds the Custom palette selector and separate 13-field Light/Dark drafts.
egui uses stable role IDs and bounded hex inputs, then sends a temporary normalized
`ThemeSettings` to `Context::set_visuals_of` for preview. Config remains the owner of
hex parsing, brightness direction, WCAG contrast fallback, and persistence; failed
saves retain both drafts and saved values.

E6R-Pause adds per-transfer Pause/Resume for active downloads and gates Cancel
by typed eligibility. Uploads, queued/conflict rows and terminal rows do not gain
pause/resume support. The controller owns pending states, generation checks and
worker acknowledgements; the UI does not infer behavior from status text or
mark a request as completed when merely queued. A bounded scroll area keeps
history controls accessible. Pause can wait for outstanding reads; resume stays
within the current connection and never provides persistent restart recovery.
Fixed-trust loopback tests cover byte preservation, stale generation, cancelling
and closing while paused, alongside bounded history/late-event and localized
widget regressions. Selected multi-drag, bulk remote mutation, native drag-out and native
GUI/server acceptance remain E6R work.

E6R-UploadSelect adds pane-local local-file/folder selection (at most 64 roots),
Select All/Clear and Upload selected. A generation/local-directory/revision key
clears stale selections, and stable path IDs plus per-frame guards prevent repeated
pass toggles or duplicate intents. Preparing disables resubmission; selection stays
local to this pane and does not change existing drag payloads.

The owner validates unique UTF-8 paths (4096 bytes each, 256 KiB total), binds them
to private listing snapshots and the remote directory request, then reuses the one
retained blocking preparation task. Source checks run before scanning and after
request construction: ordinary files use the shared opened-file fingerprint and
canonical-parent validator; directory roots use no-follow metadata, type, length,
mtime and canonical-parent observations. These do not freeze an entire tree or
eliminate filesystem races. Listing metadata for directories stays private; local
open still rejects directories. Local/remote navigation cancels preparation,
policy/connection changes discard old results, and shutdown cooperatively cancels
and joins the task. The existing discovery limits and final 64-file companion cap
apply before worker admission/history creation; a preparation error rejects the
whole batch. Prepared files share one batch UUID, native upload identity checks and
existing per-file conflict confirmation. No automatic overwrite, new task type,
channel, persistence or trust path is added. The egui surface now uses a blocking
Modal for Keep both and apply-to-batch choices. Selected multi-drag now reuses the
existing bounded selection and snapshot checks; batch mutation and native drag-out remain pending.

E6R-Select adds pane-local remote path selection (at most 64), Select All/Clear and
Download selected into the loaded local directory. Selection survives ordering
and More pages, prunes missing/link/type-changed rows, and clears on disconnect,
generation or directory-request changes. Stable path IDs and a separate per-frame
selection guard prevent repeated-pass toggles; command admission remains once per
frame. Existing drag payloads and single-entry mutation targets are unchanged.

A single intent carries 1–64 unique paths with at most 256 KiB of UTF-8 path text.
The owner rechecks the remote request and loaded local path/revision before any
send, skips missing/link/filtered entries, and continues after per-root enqueue
failures. One latest bounded summary reports root admission only (up to 64 errors,
512 characters each); it is cleared on navigation/disconnect and never triggers
selection changes. File completion/failure remains in the transfer rows. Existing
root rows keep discovery cancellation; the first discovered file reuses the root
UUID, and empty/fully filtered directories finish with DiscoveryFailed without
creating local directories. Child transfers retain the experimental 64-row budget
and excess cancellation. The shared OpenSftpFile entrance now checks its existing
512 active/opening/queued/discovery slot budget before spawning a discovery,
returning the existing failure event on overload. No new channel, task type, wire
protocol, persistence or trust path is added. Keep both/apply-to-batch semantics are
shared with the worker; selected multi-drag is now implemented in egui, while batch mutation
and native drag-out remain pending.

E6R-Mutate adds single-entry Rename/Delete with frozen path/type/size/mtime and
connection/directory request checks. A single reserved receipt slot covers queued,
running and completed-but-unconsumed work. Full/Closed does not reserve it; owner
rejections and native write results retain an exact UUID result with a 512-character
error limit. Closing abandons the receipt and cannot undo an admitted write;
disconnect cancels/joins the existing worker and clears the slot. The UI owns only
the modal draft, preserves it on failures and disables workspace/terminal actions
while open. It rejects links and invalid names; Delete explicitly confirms a regular
file or empty directory. Existing LSTAT/type/target-absence checks, ordinary RENAME
and nonrecursive REMOVE/RMDIR remain in the shared worker. These observations do
not provide an atomic remote identity guarantee. Success refreshes only the original
still-visible directory request; navigation never jumps back. Bulk remote-file
mutation, native drag-out and native GUI/server acceptance remain open; selected multi-drag
now uses the existing bounded batch intents.

E6R-Batch adds pane-local transfer selection by UUID, Select All/Clear, and batch
Pause/Resume/Cancel. A generation-bound intent carries 1–64 distinct IDs; the owner
rechecks every current row, skips missing/ineligible entries, commits each pending
state only after that worker send succeeds, and continues after individual failures.
One bounded latest summary reports queued/skipped/failed admission, never completion;
at most 64 failures retain 512 characters each. Disconnect/replacement clears it.
The UI prunes finished/missing selections and clears on generation change or disconnect.
Standard selectable buttons admit one selection change per frame, including repeated
passes; worker intents retain their own per-frame guard. No queues, tasks, trust,
persistence or native worker protocols are added. Upload pause and cross-connection
resume remain unsupported. Remote-file batch mutation and native acceptance remain separate work;
selected multi-drag uses the existing selection and snapshot checks.

E7R-Transfer completes the saved session/group JSON exchange and the two remaining
shortcut editors. File > Import from Clipboard opens a modal for explicit paste
and confirmation; File > Export Selected to Clipboard opens a saved server/group
picker and Copy action, defaulting to the focused saved profile when available.
The saved bindings default to Cmd/Ctrl+Shift+I and Cmd/Ctrl+Shift+E. Parsing,
serialization and atomic persistence use the controller's bounded revision/receipt
flow and a joined blocking task, reusing the default frontend's shared version-1
format and sanitization. Imports merge without replacing settings or live panes;
invalid input, stale revisions and failed writes preserve the catalog/draft.
Exports are read-only, and closed/stale/unfocused completions never copy. Both
frontends retain the 256 KiB/128-profile protocol bounds, including empty groups.
The UI draft is capped at 256Ki characters and checked for byte length on submit.
Unlike the default frontend's automatic clipboard read, the experiment requires
explicit paste. File dialogs, directed group import and native acceptance remain.

E7R-SFTP-Key adds View > SFTP and the sixth editable shortcut (default Ctrl+M).
It focuses Remote path in the existing companion for the focused saved SSH pane,
without connecting or listing. Local, Telnet, Serial, temporary SSH and Settings
tabs disable this action; modal gates also apply. Disabled keyboard actions leave
the chord available to the terminal. UUID revalidation, one-shot non-sizing focus
and suppressing terminal refocus prevent stale or duplicate targeting. All eight
persisted chords now participate in semantic alias/order and reserved-key checks
(including Cmd/Meta as Ctrl on Windows/Linux);
session import/export editors remain deferred until their controller actions exist.
Successful Save updates routing; preview and failed saves retain the saved route.
Native menu/focus acceptance remains open.

E7R-Shells adds the saved/discovered shell dropdown alongside the custom executable
field. Discovery runs once before the GUI on a joined blocking worker and remains
Context-local metadata. Current selection takes priority within the existing
32-entry, 256-character domain bounds, including unavailable executables. Opening
Settings and preview keep persisted values unchanged; Save merges the cache into
a submission copy, and failures preserve the draft. New local terminals consume
the committed executable; active terminals keep running. Existing PO labels are
reused. The old collapsed-group-length field currently has no UI consumer in
Slint or egui and is retained for a future compact-sidebar contract, rather than
exposed as an ineffective control. Native keyboard/PATH acceptance remains open.

E7R-Icons adds a Local shell icon selector (Automatic plus the existing catalog)
and a System icons follow theme toggle in Appearance. Draft preview/revert updates
local Tab icons and system icon coloring; Save commits through the existing
transaction and failures retain the draft. Sidebar uses saved override/validated
cache metadata; split Tabs use the focused pane. The shared host identity service
keeps Linux I/O on startup blocking work and uses the secondary os-release file
only when the primary is missing. Version-locked resvg is an optional egui build
dependency, producing bounded 48px pixels rather than decoding SVG in UI callbacks.
Native/mask texture variants preserve alpha and are released on cache clear/drop.
No remote discovery, session-level picker or native titlebar integration is added
by this slice; generic fallback and native/high-DPI acceptance remain.

E7R-FPS adds focused/unfocused refresh controls (1–120 FPS, defaults 60/4).
Startup/restored panes and successful saves configure controller snapshot deadlines;
preview and failed saves keep the running policy. A latest-value focus watch keeps
presentation hints responsive under queue overload, generation changes and
fallible hide cleanup. The UI refreshes this hint each admitted frame without
queueing duplicate protocol reports; generation checks still guard
DEC1004 protocol reports. Hidden/inactive terminals use the unfocused rate, idle
workers have no periodic frame timer, and missed deadlines do not cause catch-up
bursts. Synchronized-output holds remain intact. Parsing, protocol replies and
status prompts are independent of this output ceiling; native frame rate and
focus/performance acceptance remain open.

E7Q adds the semantic-highlighting toggle and four status-color drafts in
Appearance, reusing existing PO translations and config normalization.
Both frontends share the existing bounded ASCII recognition rules. The terminal
controller publishes per-run ranges/classes, reusing current source-row identities;
Painter performs no matching and preserves character positions, program styles
and decorations. Toggle/colors invalidate the row cache for idle preview/revert,
and saved values remain unchanged on failure. Matching stays within an eligible
run of at most 512 bytes; it does not join styles, wrapped rows or Unicode text.
Link interaction, remaining settings and native GUI acceptance
remain open.

E7H adds the Workspace mask character and masked endpoint lines beneath saved
session names, sharing the default frontend's pure formatter. Usernames and the
second IPv4 octet follow existing rules; hostnames, IPv6 and serial ports remain
visible. Preview normalizes a copy without changing profiles or saved settings;
failed saves keep the draft. Full hover/accessibility details and raw endpoint
search/edit/connection remain available. This is a presentation preference.

E7L implements the language foundation and application menu/settings translations
using eleven existing PO catalogs plus source English. All 675 previous keys are
retained; 36 additions bring each catalog to 711 keys, with 84 used by the first
egui slice. Literal Rust markers join Slint/search source keys. The same tooling
generates/checks static egui tables while preserving context, format and translator
comment rules. Language offers System and all bundled locales, saving canonical
locale IDs and switching only after a successful write; preview/failure leaves
the active language unchanged. System locale is captured before the GUI, and
context-local lookup performs no file I/O. Menus, settings and Settings tab/window
titles update while native action IDs, accelerators and Window/Help roles remain.
Further translations and native acceptance remain E7R.

E7F adds independent application/terminal font selectors using the shared bundled
inventory and at most 256 choices, preserving unavailable selected names. Font
discovery and required fallbacks load before GUI startup; subsequent selection
reads run on one retained blocking worker with latest-value requests/results,
24 MiB/file and 96 MiB/request bounds, cancellation and shutdown join. Preview,
Save, Revert and Discard share the effective settings path. Missing fonts and
unequal measured Latin advances warn and use JetBrains Mono without rewriting
the choice. Four terminal style aliases support static/variable faces and retain
CJK/symbol fallbacks. Applying a new font refreshes measured cells, row caches,
selection and PTY dimensions. Native rendering/IME and performance remain E2V.

E7W covers terminal connection fields/trust/authentication prompts, saved sessions,
groups, Tab/pane controls and workspace file dialogs in all eleven catalogs.
It reuses existing messages and adds 30 per locale, reaching 741 PO keys and 145
egui keys. Stable IDs retain connection expansion and dialog identity across
language changes. User data, fingerprints, paths, protocol IDs and technical
status/errors stay intact; exact prompt tokens and confirmable gates remain in
force. Other settings and media remain E7R work (X11 controls are covered by E7X), with native
layout/keyboard acceptance still separate.

E7S adds SFTP directory/transfer/conflict/external-edit labels, reusing existing
messages and adding 15 per locale (756 PO messages, 167 egui keys). SFTP shares
the terminal trust/authentication prompt, displaying target and fingerprints
before explicit trust-once confirmation. Blocked keys offer no confirmation.
Prompt/generation changes clear stale credentials; terminal and SFTP password
widgets have separate scopes. Buttons admit once per frame, skip sizing passes
and show enqueue errors. UUID transfer identities and raw file/path/status data
remain stable; transport trust, conflict rules and cleanup stay unchanged.

E7M implements saved-only mouse selection priority, automatic selection copy
and right-click copy/paste. Shift forces local selection and Alt/Option overrides
local priority; a local drag consumes the gesture, a cancelled click is discarded,
and an unselected click may still reach a reporting TUI. Copy completions carry
connection/viewport identity. Native paste requested by the app has a short-lived
target ticket checked before any widget receives its contents, including across
repeated passes. Direct OS paste keeps normal focused-widget routing.

E7P adds saved-only Option/Meta and OSC 52 settings. The macOS native window
keeps original Alt characters only while a terminal owns focus; settings/editors
retain native Option text and IME commits remain unprefixed. OSC 52 defaults off;
enabled writes and individually approved reads are bounded to 64 KiB. The
controller owns exact request IDs, formatting, a 20-second deadline and
revocation on permission/connection changes. A retained native-read service
uses one blocking job and bounded request/result slots, preserving empty text
and CRLF. Focus/viewport/navigation changes discard late results. Normal paste
cannot answer a remote read. Native Option/IME/clipboard acceptance remains open.

E7U exposes the existing global upload cap (1–32) and applies it after successful
workspace loading and Save. Failed writes, stale edits and appearance preview
keep the previous cap. All SFTP connections share the process-wide limiter;
server overrides do not replace it. Running uploads retain their slots when
the cap is lowered; queued/new uploads use the new capacity without reconnecting.
Remaining media and native SFTP acceptance stay separate; X11 controls are covered by E7X.

E7O shares the default frontend's directory/file validation and adds **Open** for
ordinary local files plus the saved local snapshot limit. One cancellable,
joined blocking job copies the validated handle into an isolated experimental
cache before calling the system opener. Directory revision, identity, size and
cancellation checks reject stale or replaced targets. Copies preserve the source;
editing them does not write back to it. The default 512 MiB cap, zero/unlimited
byte option, 128-file cap and stale cleanup reuse the existing cache engine.
Local-directory navigation/settings/disconnect/close revoke unlaunched work; associated-application
and GUI acceptance remain manual.

E7X exposes SSH Off/-X/-Y and the local provider/custom path/startup/explicit
local no-auth compatibility settings. New drafts default Off; saved sessions keep
their explicit mode. Preview and failed saves preserve worker configuration;
new SSH workers capture saved options, and existing connections apply them on
reconnect. Known installation locations are read on a joined blocking worker
before window creation, with no drawing-time I/O. The existing SSH worker keeps
authenticated preparation, cookie checks, restricted -X authorization, bounded
relays and shutdown cleanup. Rejected X11 requests preserve the shell. The
microphone entry is covered by E7A; real device/X server/GUI acceptance remains
open before a default switch.

E7I adds clipboard images in connected Local/SSH terminals using existing media
and same-connection SFTP. A joined background job converts premultiplied pixels
and encodes bounded PNG. The controller owns preparation/files; a generation and
viewport-bound UI ticket requires the original focused visible terminal before
inserting a quoted path without Enter. New interaction, Cancel and connection
changes revoke results; shutdown joins file work. Limits are 4096²/16 MiB/32 files,
with one conversion and one preparation per terminal. egui prioritizes text when
the clipboard has both representations. Telnet/Serial reject images; OSC 52 is
still text only. No file picker or new dependency is required. Native clipboard
and visual acceptance remain open; remote cleanup after network loss is best effort.
The shared setup waits for an explicit server Success/Failure before enabling X11,
with a 15-second deadline and at most 64 messages/64 KiB of preserved early output.
Disconnect cancels that wait; queued requests alone never enable the dispatcher.

## FigCraft reference map


The user-specified local FigCraft checkout is a read-only implementation reference
at `a86897dc919b8ae7fb0142fe7e0034c10715f7d9`, using egui/eframe 0.36 and Rust 1.95.
Do not add path dependencies, copy branding/font assets, or adopt its workspace.
Check APIs against the chosen egui release rather than assuming source compatibility.

| Reference file | Useful pattern | AxSSH adaptation |
| --- | --- | --- |
| `apps/figcraft/src/main.rs` | App lifecycle, background wakeups, title commands only on change | Keep thin startup, existing logging, and bounded Tokio runtime |
| `apps/figcraft/src/graphics.rs` | Isolated backend selection and environment overrides | Choose policy using AxSSH native tests |
| `crates/ui-egui/src/{lib,theme,widgets}.rs` | Thin UI, tokens, accessible controls | Map existing ThemePalette and keyboard/focus semantics |
| `crates/ui-egui/src/windows.rs` | Stable IDs, local window state, deferred viewports | Preserve tab UUIDs/lifetimes; do not copy global mutable state swapping |
| `crates/ui-egui/src/render_worker.rs` | Coalesced presentation and targeted wakeups | Use bounded channels; do not copy unbounded channels or discard terminal bytes |
| `crates/ui-egui/src/canvas.rs` | Custom content separated from controls; texture lifetime | Its tiny-skia canvas texture is an alternative, not the initial terminal path |
| `crates/ui-egui/src/{i18n,a11y}.rs` and behavior tests | Static translation lookup, semantics, windowless input tests | Preserve AxSSH PO catalogs, contexts, plurals, and placeholders |

Traceable source: [pinned FigCraft tree](https://github.com/storytold/figcraft/tree/a86897dc919b8ae7fb0142fe7e0034c10715f7d9).
FigCraft screenshot acceptance and macOS `process::exit` do not apply here:
AxSSH retains user visual acceptance and explicit shutdown.

## Implementation stages

Proceed in order with reviewable changes. [Tracker](project-implementation-tracker/current.md)
owns actual status.

| Step | Deliverable | Completion condition |
| --- | --- | --- |
| E0 | Branch, bilingual plan, architecture direction, references, tracking | Documentation/skill/tracker checks and baseline Cargo check |
| E1 | Version/MSRV decision, features, experimental entry, isolated storage, build matrix | Both frontends build separately; new entry runs; constraints/checks updated |
| E2 | Single-pane real local PTY terminal | Grid, IME, selection, paste, scroll, resize, synchronized output, shutdown; initial measurements and user visual feedback |
| E3 | Minimal controller facade and extracted state/presentation/cleanup | Existing Slint regressions pass through shared boundary; egui uses the same business logic |
| E4 | SSH/Telnet/Serial, trust/authentication/reconnect | Loopback security and byte-chain regressions with injected credentials; native servers/devices separately verified |
| E5 | Sidebar/groups/editor, tabs, up to eight panes, workspace recovery | Stable UUIDs, atomic persistence, split/close, recovery authentication, keyboard/focus |
| E6 | SFTP panels/icons/transfers/editing/drop/companion | Stale directory responses, conflicts, cancellation, cleanup, independent transport lifetimes |
| E7 | Settings/themes/fonts/shortcuts/translations/audio/media/X11 entry points | Existing settings/preview/save/locales/permissions/cancellation preserved |
| E8 | Detached windows, menus/titlebars, DPI, accessibility, platforms | Five-target CI, native GUI/input/drop tests, stable close/reopen resources |
| E9 | Parity/performance decision, default switch, Slint removal | All cutover gates, bilingual docs/releases/licenses, independent checks after removal |

The E2 slice implements regular/bold/italic text, ANSI colors, cursor, drag/copy,
double-click semantic and triple-click logical-line selection, basic IME, scroll,
SGR pixel mouse, resize and a TextInput accessibility name, with real PTY
input/resize/cleanup and windowless regressions. ASCII and box-drawing batches
use model columns at fractional DPI scales; layouts are reacquired each pass
after any egui glyph-atlas reset. Wheel batches use the model's live mouse and
alternate-screen protocol modes, including horizontal wheel reports. Full typography
(ZWJ/geometry/fallback widths), keypad/AltGr, the complete accessibility tree,
native GUI and performance comparison remain open; the prototype does not
complete all E2 acceptance conditions.

E2 is the first usable milestone. Track platform builds throughout rather than
deferring all Windows/Linux issues to E8. E2 adds only a minimal experimental
local-terminal adapter over existing model/PTY APIs; E3 integrates it into shared
control without retaining duplicate input encoding, protocol-response, or shutdown
rules. E7L adds egui source extraction and static tables. Before E9, migrate
remaining Slint-only references: deleting `.slint` now would still remove needed catalog keys.
`scripts/check_repository.py` and tests currently enforce Slint build/type entries;
E1 teaches both frontend boundaries and E9 enforces the final egui architecture.

## Verification and cutover gates

Run focused behavior tests, then repository fmt/check/strict all-targets Clippy/test
and diff checks for implementation stages. Documentation/rule changes need local
Markdown links, project-skill and tracker validation. Dependency/build changes
require lockfile, feature-isolation, and real compilation checks. Retain target
check/Clippy/build for all five CI targets; run tests only on native runners.

Compare release builds on the same machine with identical fonts, DPI, grid, and
synthetic inputs. Measure Slint GPU, Slint software, and egui independently.
Cover idle/blink, ASCII/CJK streams, Vim/tmux, scrolling, resize, eight panes,
and multiple windows. Repeat fixed workloads at least five times; record CPU,
RSS/GPU memory, P50/P95 UI frame time, receive-to-draw-submission latency,
available power measurements, and resources after close. Draw submission is
distinct from display presentation and remote echo. The 16.7 ms budget at 60 FPS
is a target, not a measured result. Investigate material regressions or document
an explicit tradeoff; a framework name does not establish an improvement.

Cutover requires feature/settings/translation parity, a defined and validated
renderer fallback policy, loopback security/lifecycle regressions, five-target
build gates, native GUI validation, old-config read/write compatibility, and a
reviewable performance report. Visual acceptance belongs to the user; agents
must not capture/inspect their own app screenshots as acceptance evidence.
Record unverified SSH/device/microphone/platform behavior explicitly.

## Rollback and next step

Until E9, Slint remains the default and the experiment has isolated data. Continue
using the original entry if the experiment fails; never let both frontends write
the same workspace concurrently. Preserve existing regressions while extracting
the controller. Review dependency usage and independent licenses before removing
Slint or vendor code.

Next, implement E6 SFTP browser/transfers and independent companion lifetimes,
extracting E3R orchestration where required. E2V/E4V/E5V native acceptance remains
open. E9 is still the final default switch and Slint removal.
