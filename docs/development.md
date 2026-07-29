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

`cargo check` / `cargo test` without `--offline` may need registry access. This
environment currently cannot resolve crates.io, so a cold dependency cache is a
known external prerequisite rather than a code failure.

## Change rules

- Keep Slint generated types in `src/app.rs`; domain and transport modules must
  stay UI-free.
- Do not store passwords in JSON. A credential provider should return an
  ephemeral secret to the SSH worker.
- Do not accept an unknown SSH host key for convenience or tests. Tests should
  inject a deterministic trust policy.
- Keep the process-owned logging guard alive through application shutdown so
  its bounded non-blocking queue is flushed. Never log credentials or terminal
  contents.
- Keep payloads crossing the UI boundary bounded and owned; never expose a
  russh channel or Tokio receiver to Slint.
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
probing, trusted password authentication, worker disconnect, and worker join.
Manual follow-up is still required for window rendering, keyboard/focus input,
the visible host-key/password dialogs, and login against a real SSH server.
