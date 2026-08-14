[简体中文](development.zh.md) · [Documentation index](README.md)

# Development

## Requirements

- Rust `1.92.0` or newer
- Cargo
- A desktop backend supported by Slint's winit backend
- Target-platform serial drivers and device permissions for real Serial tests

The implicit root Cargo workspace contains only the `ax_ssh` package.
`third_package/axshell` is a reference submodule and is not a workspace member
or build dependency.

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
- Keep SFTP on a child subsystem channel of the authenticated SSH worker. Do
  not expose the russh handle or `RawSftpSession` to application state or Slint.
  SFTP-only Tabs must not allocate a PTY or interactive shell, while retaining
  the same host-key and credential gates as terminal Tabs. Preserve the inbound
  packet, path/name, page, directory-budget, request, and shutdown limits. The
  read-only download-to-open path must retain its 512 MiB file cap, 64 KiB
  request chunks, bounded writer/event queues, per-operation and overall
  timeouts, per-Tab concurrency cap, cancellation, private-cache publication,
  and owned joins. Future upload/delete/rename/edit work requires separate
  confirmation, conflict, and mutation tests.
- The SFTP local-file pane is a read-only application-bridge snapshot. Directory
  reads run in a bounded blocking task and return only name, path, type, size,
  and modification metadata; Slint must not access the filesystem. Reject stale
  results by Tab and request identity, and preserve entry, name, and path limits
  before data reaches the UI. A local open intent must match the current active
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
  in `src/app/file_icons.rs` blocking work. Slint may consume only bounded owned
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
- `vendor/vt100` is the minimal local patch for locked `vt100 0.16.2` wide-cell
  shrinking. Keep its MIT files, change only the documented resize path with a
  regression test, and remove the patch when an upstream release contains it.
- Keep Slint key values out of `src/terminal/input.rs`; map them in `src/app.rs`
  and test normal/application-cursor byte sequences without constructing a
  window. Platform-specific printable-key fallbacks belong in the Slint bridge.
- Keep unassigned Ctrl combinations available to the focused PTY on every
  platform, including `Ctrl+C` and tmux prefixes. Terminal clipboard defaults
  use `Cmd` on macOS and `Ctrl+Shift` elsewhere. Global UI commands use native
  menu accelerators, which Slint handles before terminal input; do not assign a
  UI command to a terminal-control chord required by the user's workflow. Slint 1.17
  swaps Command/Control modifier fields on Apple platforms. While handling a
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
  as the always-available application and Terminal default. The other bundled
  families remain runtime resources, so release packages must retain the font
  directory beside the executable or in the platform resource path resolved by
  `src/app/font_bridge.rs`. Read external files on a Tokio blocking task and
  register all font bytes only on the Slint UI thread; never load static
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
Keep `assets/fonts/` beside `AxSSH.exe` so Maple Mono NF CN, Iosevka Term, and
Monaspace Neon remain selectable. JetBrains Mono is embedded and remains
available when testing the executable alone. The cross-compiled binary still
needs manual validation on Windows for ConPTY, native window behavior,
credentials, and real SSH connections.

On Windows, a normal Cargo build embeds the executable resource through
`build.rs`. On Linux, `cargo deb` uses `[package.metadata.deb]` to install the
desktop entry, executable, hicolor icon sizes, `LICENSE`, and
`THIRD_PARTY_NOTICES.md`. The macOS bundle keeps both notice files in
`Contents/Resources`. A Windows distribution must ship them beside the
executable or in its installer documentation. Platform shell caches may need
to be refreshed before a replaced icon appears.

## GitHub releases

The repository uses date-based releases. The first public tag for a date uses
`YYYY-MM-DD`; a later release that day uses a positive revision suffix such as
`YYYY-MM-DD-1`. The first form maps to Cargo/Debian `YYYY.M.D` and macOS build
`YYYYMMDD`; the revision example maps to Cargo `YYYY.M.D+1`, Debian
`YYYY.M.D-1`, and macOS build `YYYYMMDD.1`, while its macOS short version stays
`YYYY.M.D`. Run **Create Dated Release** on the default branch with revision
`0` for the first release or `1` for the second. It derives the date in
`Asia/Shanghai`, updates `Cargo.toml`, `Cargo.lock`, and
`packaging/macos/Info.plist`, commits those files, creates the annotated tag,
and enters the CI-to-Release chain. Pushing a valid annotated
`YYYY-MM-DD[-N]` tag directly enters that same chain. The release workflow
starts only after CI for the exact tag SHA succeeds and its cache-save step has
completed. It publishes these assets:

- Windows x86_64 ZIP with the executable, bundled fonts, and license notices
- Linux x86_64 and aarch64 TAR.GZ archives plus matching `.deb` packages
- macOS Apple Silicon (`macos-aarch64`), Intel (`macos-x86_64`), and universal
  `.app` ZIPs; every bundle contains the same icon, runtime fonts, and license
  notices, while the universal executable is assembled from the two native
  binaries

CI writes the shared Cargo cache only after successful default-branch or date-tag
runs; failed jobs cannot save it. Release jobs independently require a successful
CI run for the selected tag, restore that cache, but never save into it. The cache key
includes the target triple, Rust version, and `Cargo.lock` fingerprint, so a
changed lockfile or different architecture cannot reuse an incompatible cache.
Releases always compile a fresh `--release --locked` binary and never publish
CI's check or debug artifacts.

The release workflow verifies that the selected tag names an annotated release
tag, that its own tagged CI run succeeded, and that the Cargo package, lockfile,
and macOS bundle metadata agree before compiling. **Create Dated Release** is
only for a new date/revision tag. To retry CI or packaging for an existing tag,
run **Retry Existing Release** from the default branch with its exact
`YYYY-MM-DD[-N]` value. Direct tag push and manual retry both check the
annotated tag and its metadata, dispatch CI for that exact tag SHA, then
dispatch Release only after CI succeeds; neither path can create, replace, or
move a tag. A failed packaging-only run with an already successful matching CI
may also be retried directly through **Release**. The local
`packaging/macos/build-app.sh` script consumes the checked-in version and does
not mutate release metadata.

Before creating the GitHub Release, `scripts/generate_release_highlights.py`
reads the checked-out tag history and writes a short, categorized **Highlights**
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

Enable redacted keyboard/UI diagnostics and SSH latency stages for one run with:

```bash
RUST_LOG='ax_ssh=info,ax_ssh::diagnostics=debug,ax_ssh::latency=debug,russh=warn' cargo run --locked
```

`terminal-input` reports total UI-to-worker time plus `state_lock_us` and
`worker_request_us`. Multi-window `workspace-refresh` reports
`coalesced_refreshes`, `views_built_us`, `ui_queue_us`, `ui_apply_us`, and the
optional output-to-UI time. These fields contain no key text, terminal content,
host, path, profile label, or credential.

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

Automated checks cover profile validation, JSON round-trip, Slint compilation,
save-and-connect routing and authentication storage-selection mapping,
log flush behavior, and a loopback russh server that verifies rejected host-key
probing, trusted password/private-key authentication, an in-memory agent protocol
that performs external signing only after exact host-key matching, PTY shell
input/output, resize, worker disconnect, and worker join. Unit tests also cover ANSI parsing,
bounded scrollback, terminal control/navigation encoding, legacy appearance
migration into versioned settings, duplicate-profile tab isolation, local key
discovery, encrypted-key passphrases, local PTY lifecycle, vt100 cell rendering,
application-cursor arrows, shifted printable-key fallback, raw C0 control-byte
events, Apple modifier normalization, Telnet negotiation/CRLF/NAWS behavior,
stable Serial USB identity matching, and duplicate direct-connection attempt
isolation. SFTP tests cover remote-path/name validation, fragmented and
oversized packet frames, per-Tab snapshot isolation, browser event recovery,
regular-file metadata/path checks, bounded chunked downloads, truncation,
cancellation, private-cache publication/permissions/cleanup, transfer state,
pending subsystem cancellation, and Tab-shutdown joins. File-icon tests cover
bounded normalized keys, cache identity, LRU capacity, prewarm limits, and
fallback behavior; local-open tests cover snapshot ownership and symlink
replacement rejection.
The ignored `platform_credential_store_round_trips_and_deletes` test performs a
real platform credential write/read/delete and may trigger an OS authorization
prompt. Run it deliberately on each supported credential backend. Manual
follow-up is also required for window rendering, horizontal tab scrolling,
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
