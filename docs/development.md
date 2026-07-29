[中文说明](development.zh.md)

# Development

## Requirements

- Rust `1.92.0` or newer (the local verification environment uses `1.96.1`)
- Cargo
- A desktop backend supported by Slint's winit backend

The implicit root Cargo workspace contains only the `ax_ssh` package.
`third_package/axshell` is a reference submodule and is not a workspace member
or build dependency.

## Commands

```bash
cargo run
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

`cargo check` / `cargo test` without `--offline` may need registry access. The
local environment could reach crates.io when `keyring 4.1.5` was resolved on
2026-07-29; offline commands still require a populated Cargo cache.

## Change rules

- Keep Slint generated types in `src/app.rs`; domain and transport modules must
  stay UI-free.
- Do not store passwords in JSON. `src/credentials.rs` may load or save one
  profile-scoped password through the platform credential store, and must pass
  only an ephemeral secret to the SSH worker.
- Private-key profiles may persist a filesystem path only. Load key contents
  and passphrases off the UI thread, and never log or persist either value.
- Do not accept an unknown SSH host key for convenience or tests. Tests should
  inject a deterministic trust policy.
- Keep the process-owned logging guard alive through application shutdown so
  its bounded non-blocking queue is flushed. Never log credentials or terminal
  contents.
- Keep payloads crossing the UI boundary bounded and owned; never expose a
  russh channel or Tokio receiver to Slint.
- Use the terminal tab UUID, not the saved profile UUID, as the runtime
  instance key. Route input, resize, output, retry, close, and late events by
  `tab_id + attempt_id`.
- Keep terminal input, output batches, event queues, and scrollback bounded.
- Keep Slint key values out of `src/terminal/input.rs`; map them in `src/app.rs`
  and test normal/application-cursor byte sequences without constructing a
  window. Platform-specific printable-key fallbacks belong in the Slint bridge.
- Keep Ctrl combinations available to the focused PTY on every platform,
  including `Ctrl+C` and tmux prefixes. Terminal clipboard defaults use `Cmd`
  on macOS and `Ctrl+Shift` elsewhere. Global UI commands use `Cmd` on macOS
  and `Ctrl` elsewhere, and must not shadow terminal control bytes. Slint 1.17
  swaps Command/Control modifier fields on Apple platforms; normalize them in
  `src/app.rs` before shortcut matching or terminal encoding.
- Keep the visible terminal as a rendered grid. The hidden Slint `TextInput`
  is an IME proxy only: position it at the terminal cursor, leave unmodified
  preedit keys to the input method, and send committed text exactly once.
- Keep local PTY child, reader, writer, cancellation, and join ownership inside
  `src/local_shell.rs`; do not move blocking PTY operations onto the UI thread.
- Bundled fonts must live under `assets/fonts/` with their independent license
  and notices. Never load static resources from `third_package/axshell` at
  build time or runtime.
- Update both language pages when changing user-facing documentation.

## Runtime logs

`src/main.rs` initializes one global tracing subscriber through
`src/logging.rs`. The file writer rotates daily in UTC, keeps at most 15 files,
and flushes when the process-owned guard is dropped. Logs live in the `logs`
subdirectory of the platform-local AxSSH application data directory. The
default filter is `ax_ssh=info,russh=warn`; `RUST_LOG` overrides it.

## Verification boundaries

Automated checks cover profile validation, JSON round-trip, Slint compilation,
log flush behavior, and a loopback russh server that verifies rejected host-key
probing, trusted password/private-key authentication, PTY shell input/output,
resize, worker disconnect, and worker join. Unit tests also cover ANSI parsing,
bounded scrollback, terminal control/navigation encoding, legacy appearance
migration into versioned settings, duplicate-profile tab isolation, local key
discovery, encrypted-key passphrases, local PTY lifecycle, vt100 cell rendering,
application-cursor arrows, shifted printable-key fallback, raw C0 control-byte
events, and Apple modifier normalization.
The ignored `platform_credential_store_round_trips_and_deletes` test performs a
real platform credential write/read/delete and may trigger an OS authorization
prompt. It passed against macOS Keychain; Unix Secret Service and Windows
Credential Manager still require platform-specific verification. Manual
follow-up is also required for window rendering, horizontal tab scrolling,
keyboard/focus input, the visible group/host-key/authentication flows,
concurrent login against real SSH servers, and full-screen terminal programs.
