# AxSSH Rust Layout

## Toolchain Baseline

- Use Rust edition 2024.
- Preserve `rust-version = "1.92.0"` until dependency and CI evidence supports
  a deliberate MSRV change. A newer local stable compiler does not raise MSRV.
- Treat `Cargo.toml` as direct dependency intent and `Cargo.lock` as the
  reproducible application resolution. Keep CI and validation locked.
- Add dependencies only for a demonstrated project need. Keep
  `third_package/axshell` outside every Cargo target and dependency table.

## Package And Target Layout

```text
Cargo.toml           package, dependencies, profiles, MSRV
build.rs             Slint compile entry only
src/main.rs          thin process/binary entry
src/lib.rs           reusable and testable crate boundary
src/app.rs           private binary-side Slint bridge
src/config.rs        session domain and persistence
src/ssh.rs           russh transport and trust boundary
ui/app.slint         single Slint build entry
tests/               cross-module public behavior when needed
```

Keep unit tests beside private implementation. Add `tests/*.rs` only when a
test intentionally exercises the library's public boundary. Add examples or
benches only when they are maintained as real Cargo targets.

## Modern Module Files

- Declare a top-level module with `mod foo;` or `pub mod foo;` in its parent and
  place its body in `foo.rs`.
- Place children of `foo.rs` in `foo/bar.rs`, declared once from `foo.rs`.
- Do not create `foo/mod.rs`. Rust still supports it as an older layout, but
  mixing styles makes navigation ambiguous.
- Treat `mod` as a module-tree declaration, not a textual include.
- Use `crate::` paths for cross-root imports and `super::` only when the local
  parent relationship makes ownership clearer.

## API And Implementation Rules

- Keep items private by default. Use `pub(crate)` for internal cross-module
  contracts and `pub` only for intentional library consumers.
- Keep re-exports explicit in `src/lib.rs`; do not use wildcard public exports.
- Prefer domain types over strings or framework types at module boundaries.
- Keep `main` responsible for initialization and error reporting, not features.
- Return recoverable errors and add context at I/O, parsing, network, and task
  boundaries. Reserve panics for violated internal invariants and tests.
- Keep platform-specific code behind narrow `cfg` modules/functions. Document
  each `unsafe` block with the invariant that makes it safe.
- Use `tracing` fields for operational context and never log credentials,
  secret material, raw private keys, or unrestricted terminal contents.

## Async And Concurrency

- Run russh and network I/O on the Tokio runtime; keep the Slint thread free of
  blocking work.
- Give each spawned task a clear owner, cancellation path, and join/drop policy.
- Prefer message passing for worker state. Bound channels and streamed payloads.
- Do not hold `Mutex`/`RwLock` guards across `.await`.
- Do not wrap values in `Arc<Mutex<_>>` without shared mutable ownership that
  cannot be represented by a task owner and messages.
- Make timeouts and shutdown behavior explicit at network boundaries.

## Change Checks

- Format with rustfmt and lint changed targets with Clippy at `-D warnings`.
- Test validation, state transitions, security policy, cancellation, and error
  paths as well as success paths.
- Use deterministic fixtures; do not weaken host-key checks or embed secrets.
- Review Cargo metadata after target, feature, dependency, or workspace changes.

## Official Sources

- Rust modules and current file layout:
  <https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html>
- Cargo target layout:
  <https://doc.rust-lang.org/cargo/reference/cargo-targets.html>
- Cargo edition and MSRV manifest fields:
  <https://doc.rust-lang.org/cargo/reference/manifest.html#the-edition-field>
  and <https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field>
- Rust API guidelines: <https://rust-lang.github.io/api-guidelines/>
- Clippy lint documentation: <https://doc.rust-lang.org/clippy/>
