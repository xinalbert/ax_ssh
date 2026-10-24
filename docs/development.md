[简体中文](development.zh.md) · [Documentation index](README.md)

# Development

## Requirements

- Rust `1.95.0` or newer
- Cargo
- A desktop backend supported by Slint's winit backend
- Target-platform serial drivers and device permissions for real Serial tests
- Linux builds require ALSA development headers and pkg-config for CPAL

On Debian/Ubuntu, install the desktop and audio build dependencies with
`sudo apt-get install pkg-config libfontconfig1-dev libxkbcommon-dev libasound2-dev`.
macOS microphone testing uses a packaged `.app` with the microphone usage
description; device permissions and real audio behavior need native testing.

The implicit root Cargo workspace contains only the `ax_ssh` package.
`third_package/axshell` is a reference submodule and is not a workspace member
or build dependency.

## Dependency baseline

The lockfile uses Tokio 1.53.3, arboard 3.7.0, serde_json 1.0.154, uuid 1.28.0,
rubato 5.0.1, and base64 0.23.1. CPAL stays pinned to 0.17.3 to preserve macOS 11
support. Slint and its local patches remain on 1.18.1.

The audio worker uses rubato's `Fft` with `FixedSync::Both` and borrowed planar
buffer adapters. It retains preallocated processing buffers, startup-delay
removal, tail flushing, and final-packet zero padding. Tests compare fragmented
streaming output with upstream whole-clip conversion and check anti-aliasing,
sample rates, channel order, backpressure, and cancellation. Real microphone and
clipboard behavior still require native platform verification.

## Commands

```bash
cargo run --locked
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

Offline commands require the dependencies in `Cargo.lock` to be present in the
local Cargo cache. Remove `--offline` when the cache must be populated from the
registry.

Repository maintenance also runs these read-only checks:

```bash
python3 scripts/check_repository.py
python3 scripts/generate_system_icons.py --check
python3 -m unittest scripts/test_repository.py scripts/test_translations.py scripts/test_release_version.py scripts/test_generate_release_highlights.py -v
```

`check_repository.py` checks local Markdown targets and paired Chinese pages,
modern Rust module filenames, and the unique Slint type/build entries.
It skips vendor/reference documentation,
code examples and external URLs, and rejects links into the reference checkout;
it does not validate heading fragments or parse
Rust semantics. CI runs it and the generated-icon check on every build target.
Cargo/Clippy and the existing native test matrix remain authoritative.

Keep larger test suites in their owner's private `tests.rs` child module with
one `#[cfg(test)] mod tests;` declaration. Preserve platform cfgs and test helper
privacy; do not expose runtime APIs just to move tests. Environment records use
a current snapshot plus append-only history. For legacy tracker timestamps,
retain the original date and distinguish a documented Git capture time from an
unknown event time; never invent missing event hours.

The CI coverage job runs an instrumented test suite on Ubuntu with
`cargo-llvm-cov` and uploads the resulting `lcov.info` report to Codecov when the
`CODECOV_TOKEN` secret is configured. Add the repository's Codecov token as a
GitHub Actions secret under Settings > Secrets and variables > Actions. Upload
is skipped when the secret is unavailable, as on pull requests from forks; the
coverage report is still generated. This job is separate from the cross-platform
build matrix and creates one coverage upload.

## Experimental egui frontend

The default `slint-ui` feature and `ax_ssh` binary retain the complete application.
The separate `egui-ui` feature builds `ax_ssh_egui`, a workspace with saved sessions and Local/SSH/Telnet/Serial terminals
using egui/eframe 0.37.0 and WGPU. Both frontends require Rust 1.95.0; the MSRV CI
checks each independently. An egui-only build does not compile Slint or softbuffer.

```bash
cargo run --locked --no-default-features --features egui-ui --bin ax_ssh_egui
cargo check --locked --offline --no-default-features --features egui-ui
cargo clippy --all-targets --locked --offline --no-default-features --features egui-ui -- -D warnings
cargo test --locked --offline --no-default-features --features egui-ui
cargo build --locked --offline --no-default-features --features egui-ui
cargo check --locked --offline --all-features --all-targets
```

A fresh experiment starts the configured local shell with a 10,000-line history.
Its saved sessions/layout use a separate `ax_ssh_egui` configuration directory; it
does not access default-frontend files or credential stores. Logs use the independent
`ax_ssh_egui` platform data directory. Bundled font discovery happens before the
event loop on a blocking task, with existing resources and native system fallback.
The controller owns the parser/PTY and bounded command queue; UI reads complete
visible snapshots and caches rows by revision/geometry. Cancellation and worker join
finish after the event loop returns. WGPU has no equivalent CPU/damage fallback
in this experiment; continue using the Slint application on unsupported graphics.

Mouse drag selection and copy, paste, scrolling, terminal key encoding, basic
IME preedit/commit and resize are implemented. macOS uses Command+C/V, other
platforms use Ctrl+C/V; Ctrl+C without a selection sends the terminal interrupt.
Shift overrides terminal mouse reporting for local history/selection. This is an
initial prototype: native IME/keypad/AltGr, full typography, mouse protocols,
accessibility, GUI appearance, power/latency and cross-platform acceptance still
need verification. SFTP, settings and detached windows are later stages.
The connection draft is frontend state; `controller/terminal_session.rs` serializes
transport replacement, parser/input, prompt tokens and bounded reconnect deadlines.
SSH probes before authentication and pins the confirmed key for this connection;
changed/revoked known-hosts entries and a key change after probing remain blocked.
Unknown-key confirmation requires fresh credentials. Passwords/passphrases are
zeroizing payloads capped at 16 KiB; snapshots contain only public prompt metadata.
The password editor clears pane-scoped undo history. Connection attempts do not
write profiles or trust pins. Workspace transactions separately persist saved
profile details and layout, excluding credentials and terminal output. Existing system known_hosts is read under the transport trust policy;
trust once does not write it. Quick SSH disables X11 and microphone setup; the
Slint default retains its microphone behavior.

Optional automatic reconnect uses the shared five-attempt budget with 1/2/4/8/16s
delays. SSH passwords/passphrases are requested again; agent and direct transports
can retry without retaining a secret. Explicit disconnect/replacement cancels the
deadline and joins the previous transport before exposing a replacement.
Connection generations reject stale input, prompt responses and copy/selection
results. Loopback tests cover unknown-key refusal, authentication retry, probe/worker
key changes, cancelled probes, byte/protocol replies, resize and reconnect; serial
open failures are covered without opening real hardware. Native GUI, SSH-agent,
server and serial-device acceptance remain open. See [migration plan](egui-migration.md).

E5 extracts the existing pane tree into `controller/panes.rs`, consumed by both
frontends through the default adapter. `controller/workspace.rs` owns tabs,
terminal routes, versioned catalog transactions, bounded completion receipts,
checkpoint/import/export and joined cleanup. The GUI owns drafts, stable UUID
widget IDs, per-pane text caches and keyboard focus. Commands are bounded at 64;
active plus closing terminals are capped at 32, with eight panes per tab.
Catalog writes run on blocking tasks and publish only after success; stale editor
revisions are rejected and exact request results cannot be overwritten by layout
updates. Existing config schemas/atomic writers preserve unrelated settings.

Layout checkpoints compare metadata every two seconds, skip unchanged content,
and drain on exit. `.axssh` export uses the user-file writer without changing the
parent directory's permissions. Import validates before teardown and joins the
old workspace before restoring idle models. Every recovered terminal needs an
explicit resume; temporary targets, output/history, draft secrets and trust-once
pins are omitted. Unsupported feature/window snapshots and invalid startup files
are preserved rather than overwritten. Tests use isolated stores and fixed
workers, including real PTY focus isolation and repeated egui layout passes.
Native layout/input acceptance and old-application feature parity remain open.

## Interface translations

English is the source language; all registered PO catalogs under `translations/`
are compiled into the executable by Slint. Edit PO files directly and regenerate
the static settings-search tables:

```bash
python3 scripts/build_translations.py
python3 scripts/check_translations.py
python3 scripts/build_translations.py --check
python3 -m unittest scripts/test_translations.py -v
```

These development tools need Python 3.10+ and `rustfmt`, with no Python packages.
CI and release preflight run read-only translation validation. See the
[translation workflow](../translations/README.md) for adding a language and
preserving context, placeholders, native names, and stable locale identifiers.
Catalog-local `language.json` metadata drives the build-generated registry and
Settings options; adding a locale requires no language-specific Rust/Slint edits.

## Renderer selection

AxSSH enables Slint's Skia and software renderers. **Settings > Appearance >
Renderer** persists Automatic, GPU, or Software for the next launch. Automatic
uses `winit-skia` on macOS, which is Metal-backed and retains Slint's
softbuffer fallback; Windows and Linux use `winit-software` to preserve their
existing behavior. GPU selects `winit-skia` and Software selects
`winit-software` on every supported desktop platform. The setting is read
before the first `AppWindow` and takes effect only after restart.

On macOS, **Settings > Appearance > Software presentation** separately persists
`layer-images` or `damage-backing-store` for the Software renderer. The default
uses the lower-CPU `CALayerDelegate` damage backing store. The independently
owned layer `CGImage` path remains available as a compatibility fallback. Both
choices take effect only for surfaces created after restart.

`SLINT_BACKEND` takes precedence over the saved preference and explicitly
selects a renderer; for example, this forces the software path on macOS when
collecting a comparison sample:

```bash
SLINT_BACKEND=winit-software cargo run --locked
```

When no `SLINT_BACKEND` override is present, a configured Skia selection that
fails before Slint binds a platform is retried once with `winit-software`.
AxSSH also records renderer faults surfaced while creating, showing, or running
the UI. If Automatic on macOS surfaces a GPU/Skia/Metal fault, the next
Automatic launch uses the software renderer; a clean Skia run clears that
marker, and an explicit GPU or `SLINT_BACKEND` choice remains authoritative.
This cannot intercept Skia's own stderr-only shader diagnostics after a Metal
surface is already active, because Slint cannot replace its process-wide
renderer at that point.

**Help > Copy Diagnostic Info** includes the requested and selected renderer,
selection source/fallback reason, Metal device name when Skia is active, live
window count, and bounded application-observed renderer/shader fault counts.
`renderer-shader-error-capture: application-errors-only` means the count does
not include raw shader source emitted directly by Skia; attach the matching
`ax_ssh::diagnostics` log lines and stderr output when reporting such a fault.

`AXSSH_EXPERIMENT_CA_BACKING_STORE` overrides the saved Software presentation
mode when present. `1`, `true`, `yes`, and `on` select the damage backing store;
any other value selects the layer-image fallback for that process.

The repository patches the locked `i-slint-backend-winit` and `softbuffer`
sources under `vendor/`. The winit patch forwards the renderer's individual
damage rectangles. On macOS, the softbuffer patch keeps one persistent CPU
framebuffer and presents it through fixed 256x128-physical-pixel CoreAnimation
tiles. Only tiles intersecting a damage rectangle receive a new owned
`CGImage`; the first frame, resize, Retina scale change, occlusion, and restore
refresh every tile. This is intentionally kept below the Slint application API
so the terminal model and UI contracts do not depend on backend-specific types.
Keep these patches source-available and recheck them when upgrading Slint or
softbuffer.

`softbuffer::Surface::damage_support()` is the cross-platform capability probe
for this path. `Rectangles` covers Win32, X11 with XShm, Wayland protocol 4+,
and the Core Graphics tile path; `BoundingRect` is used by Web, while KMS is
`DriverDependent`. X11 without SHM, Wayland protocol 3 and older, Orbital, and
Android are explicit full-frame or lock-time fallbacks. The winit bridge uses
the probe to call `present()` directly for those fallbacks. This API describes
presentation behavior only; the persisted Software presentation selector and
the pane layout hint remain macOS-only.

The environment override and saved preference are consumed before the first
`AppWindow` is created, so renderer initialization failures are reported during
startup and can use the bounded software fallback described above.

The development profile disables rustc incremental code generation. AxSSH's
large generated Slint application unit can otherwise accumulate incompatible
code-generation objects across repeated macOS rebuilds and fail at the final
arm64 link with missing internal `_anon...llvm...` symbols. Dependency artifacts
remain cached, and the release profile keeps its existing ThinLTO settings. If
an older checkout already has affected artifacts, remove only this package's
development output with `cargo clean --profile dev --package ax_ssh`.

## Change rules

- Keep Slint generated types in `src/app.rs`; domain and transport modules must
  stay UI-free.
- Do not store passwords in JSON. `src/credentials.rs` may load or save one
  profile-scoped password through the platform credential store, and must pass
  only an ephemeral secret to the SSH worker.
- Private-key profiles may persist a filesystem path only. Load key contents
  and passphrases off the UI thread, and never log or persist either value.
- SSH-agent profiles persist only their authentication method. Keep the runtime
  agent client worker-owned, open it only after exact host-key verification,
  retain the five-identity/30-second limits, drop it after authentication, and
  never persist or log its socket path, identity comments, or key data. Agent
  forwarding and key management remain outside this boundary.
- Do not accept an unknown SSH host key for convenience or tests. Tests should
  inject a deterministic trust policy.
- Keep the process-owned logging guard alive through application shutdown so
  its bounded non-blocking queue is flushed. Never log credentials or terminal
  contents.
- Keep payloads crossing the UI boundary bounded and owned; never expose a
  russh channel or Tokio receiver to Slint.
- Use the terminal tab UUID, not the saved profile UUID, as the runtime
  instance key. Route saved-connection input, resize, output, retry, close, and
  late events by `tab_id + profile_id + attempt_id`.
- A detached native window is a view owner, not a transport owner. Move SSH
  Terminal/SFTP companion groups with an owned `WorkspaceTransfer` containing
  only bounded UUIDs; route snapshots by `WindowRouter` and keep russh handles,
  receivers, terminal buffers, and secrets in `AppState`/workers. Returning or
  closing a detached window must remove only its route and must not reconnect or
  shut down the transferred workers. Its client view may render only active
  Terminal/SFTP content; the detached native title carries the connection name,
  and the macOS title-bar icon-only return button stays on that same row, exposes
  a tooltip/accessibility description, and invokes the existing route handler.
  Inline main-window actions must pass their Tab UUID directly to that handler
  rather than relying on callback order.
- Keep terminal input, output batches, event queues, and scrollback bounded.
  Clipboard paste is a single typed `KeyboardEvent` transaction with a 4 MiB
  encoded limit. Normalize CRLF/LF to CR, strip ESC, apply DEC 2004 wrappers
  around the complete payload, and let each transport write the accepted bytes
  in bounded 16 KiB chunks.
- Keep SFTP on a child subsystem channel of the authenticated SSH worker. Do
  not expose the russh handle or `RawSftpSession` to application state or Slint.
  SFTP-only Tabs must not allocate a PTY or interactive shell, while retaining
  the same host-key and credential gates as terminal Tabs. Preserve the inbound
  packet, path/name, page, directory-budget, request, and shutdown limits.
  Regular downloads must retain their 512 MiB file cap, 64 KiB request chunks,
  bounded writer/event queues, per-operation and overall timeouts, per-Tab
  concurrency cap, cancellation, validated no-replace local publication, and
  owned joins. A completed download remains in Local files; only an explicit
  local-file action may invoke the platform opener through its private cache.
- The SFTP local-file pane is a read-only application-bridge snapshot. Directory
  reads run in a bounded blocking task and return only name, path, type, size,
  and modification metadata; Slint must not access the filesystem. Reject stale
  results by Tab and request identity, and preserve entry, name, and path limits
  before data reaches the UI. Accepted names remain complete in Rust DTOs, while
  Slint elides them to the column width and exposes a wrapped hover tooltip.
  The application exposes local snapshots in bounded 250-entry pages and keeps
  Modified descending as the default sort. A local open intent must match the current active
  Tab snapshot, open a non-symlink regular-file handle on a blocking worker,
  compare its platform identity and length, modification-time, and creation-time
  fingerprint with the listing snapshot, and copy from that handle into the
  bounded private cache before invoking the detached platform opener. That
  fingerprint detects only platform-observable changes and is not a content
  integrity guarantee. Never reopen the validated source path for dispatch.
- SSH profile `sftp_remote_path` and `sftp_local_path` values are non-secret
  initialization inputs only. Keep them out of credentials, logs, and running
  Tab mutation; validate their bounded text before persistence, pass the remote
  value into the worker-owned browser, and use the local value only to seed the
  application snapshot. Empty legacy values must retain the `~`/platform-home
  defaults.
- Keep file-icon platform APIs, theme detection, file reads, and image decoding
  in `src/file_icons.rs` blocking work. Both frontends consume bounded owned
  RGBA images from the process-local cache. Remote names are extension/type
  hints, never local paths, and every platform resolver must retain a built-in
  fallback and release native handles deterministically.
- Keep Telnet explicitly plaintext. Parse and filter IAC negotiation before
  terminal output, reject unsupported options, and send NAWS only after peer
  acceptance. Do not add credential persistence to Telnet profiles.
- Serial enumeration must remain metadata-only and run outside the UI thread.
  Never open or probe a device during automatic discovery; only an explicit
  connect action may resolve the saved identity and create a device worker.
  Serial resize changes the local terminal grid only.
- Preserve a detached main-screen terminal viewport's bounded display offset
  across model resize. A newly mounted `TerminalPane` must settle its layout
  before its first resize callback so a transient minimum grid cannot reset a
  retained Tab's scroll position.
- Terminal model changes belong in `alacritty_terminal` integration and
  AxSSH-owned DTO/rendering tests. The unused historical vt100 copy has been removed.
- Keep Slint key values out of `src/terminal/input.rs`; map them in `src/app.rs`
  and test normal/application-cursor byte sequences without constructing a
  window. Platform-specific printable-key fallbacks belong in the Slint bridge.
- Keep unassigned Ctrl combinations available to the focused PTY on every
  platform, including `Ctrl+C` and tmux prefixes. Terminal clipboard defaults
  use `Cmd` on macOS and `Ctrl+Shift` elsewhere. Global UI commands use native
  menu accelerators, which Slint handles before terminal input; do not assign a
  UI command to a terminal-control chord required by the user's workflow. The
  locked Slint 1.18.1 swaps Command/Control modifier fields on Apple platforms. While handling a
  macOS keyboard event, `src/app.rs` must use AppKit's current physical
  modifier state before shortcut matching or terminal encoding, so either
  Control key has the same meaning even when Slint misses a side-specific
  `flagsChanged` event.
- Convert persisted menu shortcuts to `slint::Keys` only in `src/app/input.rs`.
  Map Apple `Cmd` to Slint `Control` and physical `Ctrl` to Slint `Meta`. Keep
  menu action diagnostics to fixed IDs; `MenuItem.activated` cannot distinguish
  a pointer click from an accelerator.
- Keep the visible terminal as a rendered grid. The hidden Slint `TextInput`
  is an IME proxy only: position it at the terminal cursor, leave unmodified
  preedit keys to the input method, and send committed text exactly once.
- Keep local PTY child, reader, writer, cancellation, and join ownership inside
  `src/local_shell.rs`; do not move blocking PTY operations onto the UI thread.
- Keep macOS movable-window-background disabled. Only a left-button down in
  the empty zero-tab strip or dedicated trailing space may invoke the
  UI-thread native drag callback; tabs, the activity bar, sidebar, and
  terminal must not become drag regions.
- Bundled fonts must live under `assets/fonts/` with their independent license
  and notices. The four JetBrains Mono faces are compiled into the executable
  as the always-available application and Terminal default. Maple Mono NF CN is
  the single Han fallback and remains a required runtime resource; Iosevka Term
  and Monaspace Neon are optional primary families. Release packages must retain
  the font directory beside the executable or in the platform resource path
  resolved by `src/app/font_bridge.rs`. Read external files on a Tokio blocking
  task and register all font bytes through `FontRegistry` only on the Slint UI
  thread; do not add a renderer-side fallback path, and never load static
  resources from `third_package/axshell` at build time or runtime.
- `assets/ion/terminal_icon.svg` is the canonical application-icon source.
  Regenerate all PNG, ICO, and ICNS variants from it as one set. The Slint
  window uses the 256px PNG; Windows embeds the ICO through
  `packaging/windows/axssh.rc`; macOS bundles the ICNS through
  `packaging/macos/Info.plist`; Linux installs the hicolor PNG set with its
  desktop entry. Do not substitute or load an icon from the reference project.
- Keep `AboutSlint` in the top-level-menu-accessible About page. AxSSH selects
  Slint's `GPL-3.0-only` option, while the standard component keeps toolkit
  attribution visible. About support actions use the existing AppWindow bridge:
  they open the issue tracker or log directory, or copy only non-sensitive build
  metadata. They must not upload logs or expose profile, host, path, or secret
  values.
- Update both language pages when changing user-facing documentation.

## Platform packages

Build a macOS application bundle with:

```bash
packaging/macos/build-app.sh
```

### Cross-compile Windows on macOS

The repository's Windows CI and release target is `x86_64-pc-windows-msvc`.
The `aws-lc-sys` dependency also needs NASM when it builds its Windows
assembly. The MSVC cross-link also needs the full Homebrew LLVM toolchain:
`llvm-lib` is supplied by LLVM and `lld-link` by the separate `lld` formula.
On macOS, install these tools, the Rust target, and `cargo-xwin` once:

```bash
brew install nasm llvm lld
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
```

Homebrew keeps LLVM and LLD keg-only. Put their tools first on `PATH` for the
build (the `brew --prefix` form works on both Apple Silicon and Intel macOS):

```bash
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
```

Build the Windows release binary from the repository root:

```bash
cargo xwin build --release --locked --target x86_64-pc-windows-msvc
```

The first `cargo xwin` build may download the Windows SDK/CRT files, so it
requires network access. If the build reports `NASM command not found`,
`llvm-lib` not found, or `lld-link` not found, install the tools above, export
the `PATH`, and rerun the same command. The executable is written to:

```text
target/x86_64-pc-windows-msvc/release/ax_ssh.exe
```

Create a portable ZIP with the runtime font resources and license notices:

```bash
stage="AxSSH-windows-x86_64"
rm -rf "$stage" "$stage.zip"
mkdir -p "$stage/assets/fonts"
cp target/x86_64-pc-windows-msvc/release/ax_ssh.exe "$stage/AxSSH.exe"
cp -R assets/fonts/. "$stage/assets/fonts/"
cp LICENSE THIRD_PARTY_NOTICES.md "$stage/"
ditto -c -k --sequesterRsrc --keepParent "$stage" "$stage.zip"
```

Copy the resulting ZIP to the Windows host and extract it as a directory.
Keep `assets/fonts/` beside `AxSSH.exe`; Maple Mono NF CN is required for the
single deterministic Han fallback, while Iosevka Term and Monaspace Neon remain
selectable primary families. JetBrains Mono is embedded and remains available
when testing the executable alone, but its Terminal Chinese fallback requires
the Maple runtime files. The cross-compiled binary still
needs manual validation on Windows for ConPTY, native window behavior,
credentials, and real SSH connections.

On Windows, a normal Cargo build embeds the executable resource through
`build.rs`. On Linux, `cargo deb` uses `[package.metadata.deb]` to install the
desktop entry, executable, hicolor icon sizes, runtime fonts and their licenses,
`LICENSE`, and `THIRD_PARTY_NOTICES.md`. Release CI uses the same metadata with
`cargo-deb` 3.7.0 and `--no-build --no-strip --target <target>` after the release
build. Keep the asset source prefix `target/release/ax_ssh`: cargo-deb resolves
it to the selected target directory. Runtime package dependencies are detected
on the native Linux runner. See the [upstream asset contract](https://github.com/kornelski/cargo-deb#packagemetadatadeb-options). The macOS bundle keeps both notice files in
`Contents/Resources`. A Windows distribution must ship them beside the
executable or in its installer documentation. Platform shell caches may need
to be refreshed before a replaced icon appears.

## GitHub releases

The repository uses date-based releases. The first public tag for a date uses
`YYYY-MM-DD`; a later release that day uses a positive revision suffix such as
`YYYY-MM-DD-1`. The first form maps to Cargo/Debian `YYYY.M.D` and macOS build
`YYYYMMDD`; the revision example maps to Cargo `YYYY.M.D+1`, Debian
`YYYY.M.D-1`, and macOS build `YYYYMMDD.1`, while its macOS short version stays
`YYYY.M.D`. On the default branch, synchronize the date tag with the committed
release metadata, then create and push an annotated tag:

```bash
python3 scripts/release_version.py sync --tag 2026-08-12
git add Cargo.toml Cargo.lock packaging/macos/Info.plist
git commit -m "Release 2026-08-12"
git tag -a 2026-08-12 -m "AxSSH 2026-08-12"
git push
git push origin 2026-08-12
```

Pushing a valid annotated `YYYY-MM-DD[-N]` tag directly starts the Release
workflow. It verifies the tag and checked-in metadata before building and
publishes these assets:

- Windows x86_64 ZIP with the executable, bundled fonts, and license notices
- Linux x86_64 and aarch64 TAR.GZ archives plus matching `.deb` packages
- macOS Apple Silicon (`macos-aarch64`), Intel (`macos-x86_64`), and universal
  `.app` ZIPs; every bundle contains the same icon, runtime fonts, and license
  notices, while the universal executable is assembled from the two native
  binaries

CI writes the shared Cargo cache only after successful default-branch runs;
failed jobs and pull-request/tag runs cannot save it. Release jobs restore that
cache but never save into it. The cache key includes the target triple, Rust
version, and `Cargo.lock` fingerprint, so a changed lockfile or different
architecture cannot reuse an incompatible cache.
CI independently checks, lints, and links Windows x86_64, Linux x86_64/aarch64,
and macOS arm64/x86_64. Every matrix job runs `cargo check --target`, strict
`cargo clippy --all-targets --target`, and `cargo build --target`; tests use
`cargo test --target` only when that target is native to its GitHub runner. The
Intel macOS job therefore compiles but does not execute x86_64 tests on the
Apple Silicon runner.
Releases always compile a fresh `--release --locked` binary and never publish
CI's check or debug artifacts.

The release workflow verifies that the pushed ref names an annotated release
tag and that the Cargo package, lockfile, and macOS bundle metadata agree
before compiling. There is no Create or Retry workflow, tag CI dispatch, or
polling chain. Re-run the failed Release run from GitHub Actions for the same
immutable tag. The local
`packaging/macos/build-app.sh` script consumes the checked-in version and does
not mutate release metadata.

Before creating the GitHub Release, the workflow asks GitHub for the most recent
published, non-draft, non-prerelease AxSSH date tag and passes it to
`scripts/generate_release_highlights.py`. This explicit baseline prevents a
failed tag that never produced a GitHub Release from becoming the next
comparison point. The helper then writes a short, categorized **Highlights**
prefix with immutable commit links and a full-changelog comparison link. It
excludes implementation-tracking commit subjects and lists each selected commit
once, with at most eight recent commits per category. `softprops/action-gh-release` receives that file through `body_path` while
`generate_release_notes: true` remains enabled, so GitHub provides the complete
automatic change list below the curated prefix. The helper and its Git-backed
range behavior are covered by `scripts/test_generate_release_highlights.py` in CI.

## Runtime logs

`src/main.rs` initializes one global tracing subscriber through
`src/logging.rs`. The file writer rotates daily in UTC, keeps at most 15 files,
and flushes when the process-owned guard is dropped. Logs live in the `logs`
subdirectory of the platform-local AxSSH application data directory. The
default filter is `ax_ssh=info,russh=warn`; `RUST_LOG` overrides it.
The About page receives this already-created directory from the process boundary
and can open it without performing filesystem work on the Slint thread.
The same directory contains `ax_ssh-crash.log`, a private append-only report
written synchronously by the process panic hook. It records the panic message,
source location, process/thread/platform metadata, `SLINT_BACKEND`, and a forced
Rust backtrace. Check this file first for an abort that follows a macOS
Objective-C callback panic; it is separate from the buffered daily logs.

Enable redacted keyboard/UI diagnostics and SSH latency stages for one run with:

```bash
RUST_LOG='ax_ssh=info,ax_ssh::diagnostics=debug,ax_ssh::latency=debug,ax_ssh::sftp_drag=debug,russh=warn' cargo run --locked
```

`terminal-input` reports total UI-to-worker time plus `state_lock_us` and
`worker_request_us`. Multi-window `workspace-refresh` reports
`coalesced_refreshes`, `views_built_us`, `ui_queue_us`, `ui_apply_us`, and the
optional output-to-UI time. These fields contain no key text, terminal content,
host, path, profile label, or credential.

For a wheel issue that differs between Windows and macOS, rebuild the same
revision on both hosts and enable only the wheel target for a short reproduction.
Launch the compiled Windows executable from PowerShell:

```powershell
$env:RUST_LOG = 'ax_ssh=info,ax_ssh::terminal_scroll=debug,russh=warn'
.\ax_ssh.exe
```

On macOS, launch the compiled executable from a shell:

```bash
RUST_LOG='ax_ssh=info,ax_ssh::terminal_scroll=debug,russh=warn' ./ax_ssh
```

Use **About > Copy diagnostics** to record each build revision and
**About > Open log folder** to locate the daily log. Keep the remote host,
Codex version/start command and any tmux session/options the same. Scroll a few
steps over the output, then over the composer, and close AxSSH to flush the log.
Compare `ax_ssh::terminal_scroll` records by window/terminal UUID and timestamp:

- `native-wheel` records the Winit `LineDelta` or `PixelDelta`, phase, scale
  factor and modifiers before Slint dispatch. Residual pixel accumulation means
  native events and terminal callbacks need not correspond one for one.
- `local-history` records local scroll requests, ownership, viewport mode,
  mouse modes and before/after history offsets.
- `wheel-route` records the actual model modes, bounded step count and chosen
  `mouse-report`, `alternate-scroll` or `ignored-by-mode` route.
- `wheel-enqueue` records whether the encoded wheel input entered the worker
  queue; it does not confirm delivery to or handling by the remote program.

If Windows selects `alternate-scroll` while macOS selects `mouse-report`, find
why the active mouse modes differ before changing wheel conversion. If both
select `mouse-report` and enqueue succeeds, continue through the transport,
tmux and CLI handling. A native event without a terminal callback can also
mean a small accumulated delta, another control under the pointer, or an input
gate. These records contain no terminal text, encoded bytes, clipboard data,
host, path or credentials and are disabled by the default INFO filter.

The Windows CI job runs the platform's check, Clippy, build and tests, but the
terminal wheel tests start with `PointerScrolled` on `MinimalSoftwareWindow`.
The `app::behavior_tests::terminal_wheel` tests continue through the generated
main/detached UI, production callbacks, encoder and real SSH worker, then assert
the bytes received by a loopback SSH server with a pinned fixture host key.
They cover SGR and legacy reports, wheel direction and repetition, the 256-step
limit, fractional delta accumulation, alternate-screen cursor keys, the optional
Home suffix, local history and Shift bypass. A FIFO input barrier confirms that
local-only operations sent no wheel bytes; a quiet socket timeout is not treated
as proof. Fixture output traverses the SSH connection and production terminal
parser before updating the UI's mouse modes.

After all receive and viewport assertions pass, the fixture marks explicit
worker shutdown. A peer I/O `ConnectionReset` observed after that mark is accepted
during cleanup; Windows also accepts `ConnectionAborted` (Winsock 10053) in that
same phase. Other platforms still reject `ConnectionAborted`. An earlier reset
or abort, any other error, a task panic or a shutdown timeout still fails the
test. The production SSH lifecycle is unchanged.

Run these receive-side checks on each native runner with:

```bash
cargo test --locked --offline wheel_bytes_reach_the_ssh_terminal
```

They run in the existing CI matrix without real credentials or a remote service.
They do not reproduce native operating-system mouse messages, ConPTY input
translation, or a real Codex/tmux session. Keep native compilation/test results,
SSH receive-side coverage and actual GUI reproduction as separate evidence.

`sftp_drag` traces the internal local/remote pane drag lifecycle, its
local-to-remote upload route, and the native Finder route. Native stages report
only a fixed event stage, the external hover count, and a fixed target outcome
(`remote`, `inactive-tab`, `remote-unavailable`, or `other-region`). The
shared upload route also records whether its live SFTP target resolved or was
rejected; paths, host data, cursor coordinates, file contents, and credentials
are never logged.

`terminal-geometry` reports one changed geometry signature per visible pane:
the terminal UUID, active-tab kind, detached/renderer state, scale factor,
native and logical window size, pane and grid positions/sizes, right/bottom
gaps, cell size, terminal columns/rows, and the unused fractional cell
remainder. The Rust bridge samples both Slint's physical window size and the
Winit inner size, so a mismatch between them is visible in the same record.
Records are coalesced to changes of 0.1 logical pixels and the in-memory
deduplication table is capped at 256 pane entries. The payload never includes
terminal text, host/path values, profile labels, or credentials.

Diagnostic records use fixed `event`, `key`, `route`, `action`, and `outcome`
fields. Special keys have stable names such as `F5` or `ArrowUp`; every
printable, IME, password, or pasted value is recorded only as `Text`, without
its value or length. Paths, profile labels, hosts, clipboard contents, and
credentials are not diagnostic fields. Debug records are written to the
rolling file; the console writer remains capped at INFO.

Latency records use local `input_sequence`, fixed `stage` values, and
microsecond durations. `queue_us` measures the bounded worker queue;
`call_us` measures completion of the russh data call and does not mean the
server received it. `first-output-after-input` has `association=temporal-only`.
The UI fields separate output-to-dispatch, event-loop queue, apply, and total
client output time. They never include terminal content or byte lengths.
Because the rolling writer is non-lossy, compare latency with this debug target
disabled as the baseline and use enabled logs to locate stages, not as the sole
benchmark result.

## Verification boundaries

See [behavior coverage](coverage.md) for repeatable LCOV measurement and the
controller/worker integration fixtures. Device acquisition, permission requests
and credential entries have private injection points; production code keeps
the native adapters. Tests do not access real system credentials or devices.

Automated checks cover profile validation, JSON round-trip, Slint compilation,
save-and-connect routing and authentication storage-selection mapping,
log flush behavior, and a loopback russh server that verifies rejected host-key
probing, trusted password/private-key authentication, an in-memory agent protocol
that performs external signing only after exact host-key matching, PTY shell
input/output, resize, worker disconnect, and worker join. Unit tests also cover ANSI parsing,
bounded scrollback, `alacritty_terminal` cell rendering, terminal control/navigation encoding,
legacy appearance migration into versioned settings, duplicate-profile tab isolation, local key
discovery, encrypted-key passphrases, local PTY lifecycle,
application-cursor arrows, shifted printable-key fallback, raw C0 control-byte
events, Apple modifier normalization, bracketed-paste normalization and size
limits, detached viewport preservation across resize, Telnet negotiation/CRLF/NAWS behavior,
stable Serial USB identity matching, and duplicate direct-connection attempt
isolation. SFTP tests cover remote-path/name validation, fragmented and
oversized packet frames, per-Tab snapshot isolation, browser event recovery,
regular-file metadata/path checks, bounded chunked downloads, truncation,
cancellation, private-cache publication/permissions/cleanup, transfer state,
pending subsystem cancellation, and Tab-shutdown joins. File-icon tests cover
bounded normalized keys, cache identity, LRU capacity, prewarm limits, and
fallback behavior; local-open tests cover snapshot ownership and symlink
replacement rejection.
Manual follow-up is required for the real platform credential backends, which
may trigger an OS authorization prompt, and for window rendering, horizontal tab scrolling,
native title-bar drag hit testing, keyboard/focus input, the visible
group/host-key/authentication flows, concurrent login against real SSH servers,
runtime SSH-agent selection, unlock/confirmation, multiple-identity and failure
behavior on each target platform, real Telnet servers, target-platform Serial discovery/permissions/hot-plug and
device input/output, real SFTP server compatibility, SFTP pane focus/layout,
default-application dispatch, and file-icon appearance/theme changes on macOS,
Windows, and Linux, plus protocol-specific resize behavior and full-screen
terminal programs.
For SSH input latency, compare AxSSH and the system `ssh` client against the
same host and network, preferably with P50/P95 observations. Similar delays in
both clients indicate network/remote-PTY RTT; a larger AxSSH-only delta should
be localized with the latency stages above.

See the [2026-10-03 duplicate-flow cleanup record](duplicate-link-cleanup.md) for
the removed entry points, shared flows and verification limits.
