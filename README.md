[简体中文](README.zh.md)

# AxSSH

AxSSH is a cross-platform desktop SSH workspace built with Rust, Slint, Tokio,
and russh. It combines saved sessions, independent local and remote
terminal tabs, and persistent workspace settings in one native application.

Current functionality includes password and private-key authentication,
explicit SHA-256 host-key confirmation, optional password storage in the
platform credential store, bounded terminal scrollback, ANSI rendering, text
selection, clipboard shortcuts, and native input-method support. SFTP, SSH
agent integration, reconnect, workspace restoration, and full terminal mouse
reporting are not implemented yet.

## Quick start

Install Rust `1.92.0` or newer and use a desktop environment supported by
Slint's winit backend, then run:

```bash
cargo run --locked
```

The first connection to a host is rejected until you verify and explicitly
confirm its SHA-256 host-key fingerprint. See the [usage guide](docs/usage.md)
for session setup, terminal controls, settings, and data-storage behavior.

## Documentation

- [Usage guide](docs/usage.md)
- [Architecture](docs/architecture.md)
- [Development and verification](docs/development.md)
- [Documentation index](docs/README.md)
- [Implementation tracker](docs/project-implementation-tracker/current.md)

## Security and repository boundary

Unknown and changed host keys are denied by default. Remembered passwords are
stored by macOS Keychain, Windows Credential Manager, or Unix Secret Service;
passwords, private-key passphrases, private-key contents, terminal output, and
live worker state are never written to the session JSON.

`third_package/axshell` is reference material only. It is not a Cargo workspace
member, source import, runtime dependency, build input, or documentation
dependency. AxSSH uses Slint for its UI and russh for SSH transport.
