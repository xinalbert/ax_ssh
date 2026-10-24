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
Slint retains its title/clipboard identity and permission checks; egui continues
to discard those effects. Broader application state extraction remains in E3.

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
No profile, known-hosts or credential writes occur. Quick SSH keeps X11 and remote
microphone setup disabled until their settings/permission surface migrates.

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
The existing write worker enforces remote conflicts and atomic replacement. E7
will migrate the configurable edit size limit; egui currently uses 4 MiB. Native
application associations, live edits and remaining drag/drop parity need acceptance.

## E6 bounded drag/drop

The experiment accepts native files/folders only over its visible remote-file
area and supports same-pane local-to-remote or remote-to-local copy drags.
Payloads and intents are bound to pane/connection/directory identities. One
retained blocking task prepares recursive uploads, revalidates before transport
admission and is cancelled/joined at teardown. Shared discovery preserves relative
paths and skips links/filtered names. Native coordinate lookup stays in the egui
adapter; absent or stale coordinates never fall back to an active tab. Wayland
requires a current-frame pointer event. Native drag-out promises, selection
batches, complete SFTP parity and native drop acceptance remain pending.

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

Settings exposes System/Light/Dark, six shared palettes, terminal size/line height
and cursor blink, new-local-tab shell, sidebar/tab widths and three editable app
shortcuts. Draft preview is local; Save uses the owner's bounded revision/request
transaction and existing private atomic ConfigStore writes. Failed/stale writes
retain drafts and saved values. Revert/Discard restore saved rendering; closing a
dirty tab or window offers Save/Discard/Cancel, and opening another workspace
requires resolving that draft first. Unexposed configuration domains are preserved.
Full language/font-family, SFTP/media/X11 settings and multi-window migration remain
later E7/E8 work; native menus and layout/focus still require user acceptance.

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
rules. Before E9, migrate PO key extraction
and static table generation: deleting `.slint` now would remove valid catalog keys.
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
