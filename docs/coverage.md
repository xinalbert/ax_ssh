[简体中文](coverage.zh.md) · [Documentation index](README.md)

# Behavior coverage

Coverage is generated with the same LCOV command as CI. Compare reports from
the same platform, Rust toolchain and test options. macOS results include
different native code from the Linux Codecov job and cannot be substituted for
its result. Business source is retained in the report, including startup and
native adapters that require manual verification.

```bash
cargo install cargo-llvm-cov --locked
rustup component add llvm-tools-preview
cargo llvm-cov --locked --offline --workspace --lcov --output-path target/coverage/after.lcov
cargo llvm-cov report --html --output-dir target/coverage
```

Use the installed toolchain's LLVM tools. A cold Skia build can download native
binaries independently of Cargo's offline flag; when necessary, use a complete
local archive matching the locked Skia version, architecture and feature key.
See the [official cargo-llvm-cov documentation](https://github.com/taiki-e/cargo-llvm-cov)
for report formats and profile cleaning. Do not merge stale baseline profiles
into the final run.

## Test boundaries

| Tests | Actual implementation exercised | Isolated boundary / assertions |
| --- | --- | --- |
| `src/app/behavior_tests.rs` | Generated Slint callbacks, application controller, models, and a live Telnet worker | Headless event-loop proxy in child processes; loopback TCP; exact terminal key/paste/focus bytes, resize, modal/identity guards, disconnect and retained tab |
| `src/app/connection_monitor/behavior_tests.rs` | SSH event monitor and AppState transitions | Bounded event streams; output parsing, latency, security retry, SFTP transitions and stale-attempt rejection |
| `src/app/connection/direct/behavior_tests.rs` | Shared Telnet/Serial event monitor | Transport event adapters; output, errors, reconnect scheduling, EOF and replacement attempts |
| `src/ssh/worker/behavior_tests.rs` | Actual SSH handler and SFTP worker over russh channels | Seeded public keys and explicit trust snapshots; in-memory protocol peer; download/upload bytes, conflict decisions, guarded edits, cancellation cleanup and subsystem failure |
| `src/serial/worker_tests.rs` | Actual bounded serial worker | Injected port acquisition and duplex/error streams; paste integrity, timestamps, EOF, pressure and stalled-write shutdown |
| `src/audio/worker_tests.rs` | Actual capture controller, blocking worker and PCM converter | Injected permission/device source; readiness, errors, overflow, cancellation and device lease held until close |
| `src/credentials/behavior_tests.rs` | CredentialStore operations and encrypted vault | Per-instance memory keyring and temporary directories; namespaces, error context, backup/restore, ciphertext binding and malformed records; no real system credentials |
| `src/logging/behavior_tests.rs` | Global logging initialization, flush guard and panic hook | Child process and temporary private log directory; durable report and escaped/bounded payload |

Injected boundaries remain private. Production constructors retain native
device acquisition and system keyring behavior. SSH tests preserve
`check_server_key`; unknown, mismatched, changed and revoked keys are rejected.
Passwords used in new tests remain in memory or encrypted vault records.

## Measurement

The 2026-10-12 macOS ARM64 comparison uses Rust 1.97.1 and cargo-llvm-cov 0.9.1.
The baseline passed 770 tests; the final normal and instrumented runs passed
799 tests with default concurrency, including 29 new behavior tests.

| Scope | Before | After |
| --- | --- | --- |
| Canonical `src/` total | 57.96% (26,723/46,107) | 65.78% (30,381/46,185) |
| `src/app/` | 49.05% | 57.04% |
| `src/app.rs` | 20.27% | 45.16% |
| `src/ssh/` | 49.98% | 64.78% |
| `src/ssh.rs` | 67.36% | 74.35% |
| `src/serial.rs` | 32.09% | 73.61% |
| `src/audio.rs` | 42.42% | 57.11% |
| `src/credentials.rs` | 73.63% | 88.03% |
| `src/logging.rs` | 59.52% | 91.55% |

Normalize LLVM source aliases before grouping: `src/config/../../build/languages.rs`
belongs to `build/`, outside the `src/` subtotal. Comparing identical runtime
lines against the original Git revision, excluding inline test modules, finds
3,580 newly covered existing lines. This conservative count excludes moved or
new implementation lines and all new fixture files.

Reports are saved under ignored `target/coverage/`: `baseline.lcov`, `after.lcov`,
`comparison.json`, `after-summary.json` and `html/index.html`. The final validation is
recorded in the [implementation tracker](project-implementation-tracker/current.md).
New fixtures add executable lines, so assess newly covered existing source
alongside percentage changes. Remaining native GUI, hardware, system-store and
real-server paths retain their manual verification requirements.
