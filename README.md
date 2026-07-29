[中文说明](README.zh.md)

# AxSSH

AxSSH is a cross-platform SSH workspace built with Rust, Slint, Tokio, and
russh. The current session workflow verifies a server's SHA-256 host-key
fingerprint, accepts a transient password, and keeps the authenticated
connection in a cancellable worker. Terminal emulation and SFTP remain staged
for later iterations.

## Quick start

```bash
cargo run
```

AxSSH writes daily logs to the `logs` subdirectory of its platform-local
application data directory and retains at most 15 files. Set `RUST_LOG` to
override the default `ax_ssh=info,russh=warn` filter.

For an offline check when the Cargo cache is already populated:

```bash
cargo check --offline
cargo test --offline
```

## Documentation

- [Documentation index](docs/README.md)
- [Architecture](docs/architecture.md)
- [Development](docs/development.md)
- [Implementation tracker](docs/project-implementation-tracker/current.md)

## Repository boundary

`third_package/axshell` is a reference-only submodule. It is not included in
the Cargo workspace, is not imported by `src/`, and must not become a runtime
or build dependency. AxSSH uses Slint for UI and russh for SSH transport.
