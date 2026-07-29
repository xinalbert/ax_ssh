[中文说明](README.zh.md)

# AxSSH

AxSSH is a cross-platform SSH workspace built with Rust, Slint, Tokio, and
russh. Saved sessions can be organized into collapsible groups. The connection
workflow verifies a server's SHA-256 host-key fingerprint, accepts a transient
password, and can optionally remember it in the platform credential store for
later automatic login. Sessions may instead use a private key discovered from
the user's `.ssh` directory or a manually entered path; encrypted keys request
a transient passphrase. The authenticated worker opens a PTY shell, displays
bounded ANSI terminal output, and accepts direct terminal-surface keyboard
input until disconnect. Enter, Backspace, Tab, Escape, arrows, Home/End,
Insert/Delete, Page keys, Ctrl control bytes, and xterm-style modified
navigation are encoded for the remote PTY. Terminals, Settings, and the new
session editor share one top tab bar. Every terminal tab has a unique runtime
ID and owns an independent worker and bounded terminal model, so the same saved
server can be opened more than once without sharing output or connection state.
Terminal and workspace settings are managed in a Settings tab and persisted in
the versioned `sessions.json`; JetBrains Mono is bundled under the SIL Open Font
License. SFTP and full mouse-oriented terminal protocol support remain staged.

## Quick start

```bash
cargo run
```

AxSSH writes daily logs to the `logs` subdirectory of its platform-local
application data directory and retains at most 15 files. Set `RUST_LOG` to
override the default `ax_ssh=info,russh=warn` filter.

Remembered passwords use macOS Keychain, Windows Credential Manager, or the
Unix Secret Service through the platform backend. Session JSON stores profiles,
non-secret settings, and only a credential-availability marker; it never stores
the password, passphrase, private-key contents, terminal output, or worker state.

Private-key profiles store only the selected filesystem path. Key contents and
passphrases are never persisted or logged.

Terminal selection uses the normal pointer interaction. `Ctrl+Shift+C` copies
the selection; `Cmd+V` on macOS and `Ctrl+Shift+V` on other desktop platforms
paste clipboard text into the remote shell. Plain `Ctrl+C` remains the terminal
interrupt byte.

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
