---
name: ax-ssh-rust-slint
description: Implement and review AxSSH changes using the repository's Rust 2024 module layout, Slint UI architecture, Tokio task boundary, and russh security contract. Use for work on Cargo/build files, Rust modules under src/, Slint files under ui/, UI-to-worker data flow, SSH sessions, architecture docs, or reviews of those changes.
---

# AxSSH Rust And Slint

Apply the repository's executable architecture rules without coupling the
project to its reference checkout.

## Workflow

1. Read the root `AGENTS.md`, `Cargo.toml`, the relevant source files, and
   `docs/project-implementation-tracker/project-map.md`.
2. Read [rust-layout.md](references/rust-layout.md) completely for Cargo, Rust,
   module, API, error, concurrency, or test changes.
3. Read [slint-guidelines.md](references/slint-guidelines.md) completely for
   `.slint`, `build.rs`, `src/app.rs`, UI models, callbacks, or event-loop work.
4. Read both references for changes that cross the UI/application/worker
   boundary. Read `docs/architecture*.md` before changing ownership or flow.
5. Define the layer that owns the new state, operation, and lifetime. Keep the
   smallest public contract that allows the adjacent layer to use it.
6. Implement a focused change and preserve the deny-by-default SSH trust
   boundary. Never import, depend on, link to, generate from, or copy source
   from `third_package/axshell`.
7. Review data crossing layers for ownership, size bounds, thread affinity,
   cancellation, secret lifetime, and error propagation.
8. Update paired documentation and full tracker records only when the change
   affects their declared scope.

## Architecture Decisions

- Keep process setup in `src/main.rs` and reusable domain/transport code behind
  `src/lib.rs`.
- Keep generated Slint types and callback wiring in `src/app.rs`.
- Keep filesystem/schema behavior in `src/config.rs` or modern child modules.
- Keep russh types and protocol state in `src/ssh.rs` or modern child modules.
- Pass owned DTOs and commands between layers. Keep live handles, receivers,
  runtime-specific errors, and secrets inside their owning layer.
- Prefer extending these boundaries over adding a new abstraction. Add one only
  when it removes real duplication or establishes a required ownership point.

## Security Gates

- Reject unknown and mismatched host keys until an explicit known-hosts and
  user-confirmation flow is implemented and tested.
- Keep credentials ephemeral and redact them from logs, errors, fixtures, and
  persisted configuration.
- Bound terminal output, queues, retries, timeouts, and reconnect behavior.
- Make disconnect and task cancellation observable and testable.

## Verification

Run focused checks first. For Rust or Slint changes, finish with:

```bash
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

For agent/document-only changes, validate relative Markdown links, run the
skill and tracker validators, and run `git diff --check`. Also run Cargo check
when the documented build entry, module path, dependency, or command changed.
Record any GUI, platform, network, or cold-cache verification that could not be
performed; do not replace it with an assumption.
