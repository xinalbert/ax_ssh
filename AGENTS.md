# AxSSH Repository Instructions

## Scope And Sources Of Truth

- These instructions apply to the entire repository.
- For Rust, Slint, application-boundary, or SSH changes, load
  `.agents/skills/ax-ssh-rust-slint/SKILL.md` and the references it selects.
- Treat `Cargo.toml`, `Cargo.lock`, `build.rs`, and the checked-in source as the
  executable truth. Treat `docs/architecture*.md` as the ownership contract.
- Preserve Rust edition 2024 and the declared MSRV. Re-check upstream guidance
  when changing the Rust toolchain, Slint, Tokio, or russh versions.

## Project Boundary

- AxSSH is an independent Rust desktop application: Slint owns UI composition,
  Tokio owns asynchronous task execution, and russh owns SSH transport.
- `third_package/axshell` is reference material only. Never make it a workspace
  member, dependency, path dependency, source import, generated-code input, or
  documentation link target. Do not copy its source into this project.
- Do not introduce another UI or SSH framework without an explicit architecture
  decision and matching documentation update.

## Ownership

| Path | Owns | Must Not Own |
| --- | --- | --- |
| `src/main.rs` | logging and process startup | feature logic |
| `src/app.rs` | generated Slint types, UI mapping, callbacks, UI dispatch | russh protocol details or persistence schema |
| `src/config.rs` | session domain types, validation, private persistence | Slint types, network handles, plaintext credentials |
| `src/ssh.rs` | russh handler, trust policy, authentication, channels | UI state, visual formatting, persistent profile mutation |
| `ui/` | layout, visual state, gestures, UI contracts | filesystem, Tokio, russh, blocking work |

Keep domain and transport code usable without constructing a Slint component.
Pass small, owned domain values across boundaries; never expose russh handles,
Tokio receivers, locks, or unbounded terminal buffers to Slint.

## Rust Layout And Code

- Keep `src/main.rs` thin and put reusable/testable behavior behind `src/lib.rs`.
- Use the modern module layout: `src/foo.rs` and `src/foo/bar.rs`. Do not add
  `mod.rs`, and declare each file module once in the module tree.
- Default to private visibility. Use `pub(crate)` before `pub`; expose a public
  item only when it is part of an intentional crate boundary.
- Keep runtime code free of unchecked `unwrap`, `expect`, and ignored errors.
  Add error context at filesystem, parsing, network, and task boundaries.
- Keep `unsafe` platform code minimal, cfg-scoped, and documented with a
  concrete `SAFETY` invariant.
- Do not hold synchronous locks across `.await`. Give spawned tasks explicit
  ownership, cancellation, and shutdown behavior; use bounded channels for
  streams and terminal data.

## Slint Rules

- Compile the single UI entry `ui/app.slint` from `build.rs`; compose additional
  `.slint` files through explicit relative imports and intentional exports.
- Keep `slint::include_modules!()` and all generated component types in
  `src/app.rs`. Do not leak generated types into config or SSH modules.
- Use properties for declarative data/state and callbacks for user intent.
  Choose the narrowest property direction; do not use globals as general
  mutable application state.
- Create components and run the event loop on the UI thread. Never block that
  thread with network, filesystem, terminal parsing, sleeps, or Tokio waits.
- Capture `Weak<Component>` in callbacks and background work. Return owned,
  `Send + 'static` results with `slint::invoke_from_event_loop`; use
  `slint::spawn_local` only for UI-thread-local futures.
- Use Slint models for repeated data. Keep lists bounded or virtualized, give
  repeated rows stable dimensions, and batch high-frequency worker updates.
- Prefer standard widgets for controls. Preserve keyboard focus, accessible
  names/roles for custom controls, readable text, and responsive min/max sizes.

## SSH Security

- Keep `russh::client::Handler::check_server_key` as a deny-by-default trust
  boundary. Unknown or mismatched host keys remain rejected until an explicit,
  testable confirmation and known-hosts flow exists.
- Never persist plaintext passwords, private-key passphrases, or live secrets.
  Credentials must be short-lived inputs to the SSH worker.
- Keep one clear owner for each russh handle/channel lifetime. Disconnect and
  cancel workers before removing their UI/session state.
- Tests must inject deterministic trust and credentials; they must not disable
  host-key verification.

## Change Workflow

1. Read the relevant source, `docs/project-implementation-tracker/project-map.md`,
   and the project skill reference selected for the change.
2. State the affected ownership boundary and security implications before
   changing cross-module contracts.
3. Keep edits scoped. Update paired English/Chinese docs when architecture,
   commands, user behavior, or public contracts change.
4. Update the implementation tracker for cross-module, long-running, or
   handoff-sensitive work. Do not create Worker records without authorization.
5. Review the final diff for accidental reference-project coupling, leaked
   secrets, unbounded queues/buffers, UI-thread blocking, and expanded APIs.

## Verification

For Rust or Slint changes, run:

```bash
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

- A `.slint` change is not validated until Cargo recompiles `ui/app.slint`.
- Run focused tests first when available, then the repository commands above.
- Documentation/agent-only changes require Markdown relative-link checks,
  `git diff --check`, the relevant skill validator, and the implementation
  tracker validator. Run `cargo check --locked --offline` when instructions,
  dependency facts, module paths, or build commands are changed.
- GUI rendering, keyboard/focus behavior, and real SSH lifecycle changes also
  require manual verification on the affected platform.
- GUI visual acceptance belongs to the user. Agents must not capture or inspect
  their own application screenshots as evidence that a UI change is correct.
  Complete compilation, tests, and static checks, then rely on explicit user
  confirmation and screenshots supplied by the user for visual review and
  further layout iteration.
