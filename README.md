[简体中文](README.zh.md)

# AxSSH

[![CI](https://img.shields.io/github/actions/workflow/status/xinalbert/ax_ssh/ci.yml?branch=master&label=CI)](https://github.com/xinalbert/ax_ssh/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/xinalbert/ax_ssh)](https://github.com/xinalbert/ax_ssh/releases/latest)
[![License](https://img.shields.io/github/license/xinalbert/ax_ssh)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.92.0%2B-dea584?logo=rust)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-1f6feb)](https://github.com/xinalbert/ax_ssh/releases/latest)

AxSSH is a cross-platform desktop terminal workspace built with Rust, Slint,
and Tokio. It brings saved SSH, Telnet, Serial, and local-shell sessions
together with independent Terminal and SFTP Tabs in one native application.

## Highlights

- Connect over SSH with a password, private key, or runtime agent, with explicit
  SHA-256 host-key confirmation before a host is trusted.
- Work in local or remote terminals with bounded scrollback, ANSI rendering,
  selection, clipboard, and native input-method support. Windows physical numeric
  keypads also reach terminal applications, including DEC application-keypad mode.
- Browse local and remote files in a dedicated SFTP Tab, with bounded directory
  listing, recursive downloads, uploads, rename, delete, and remote text editing.
- Use split panes and detached native workspace windows. Settings exposes both
  configurable shortcuts and fixed platform shortcuts; non-secret fields support
  native editing and paste, while secret fields remain non-copyable.

## Quick Start

Install Rust `1.92.0` or newer and use a desktop environment supported by
Slint's winit backend, then run:

```bash
cargo run --locked
```

Unknown and changed SSH host keys are rejected until you verify and explicitly
confirm their SHA-256 fingerprint. Session setup, terminal controls, SFTP, and
settings are covered by the [usage guide](docs/usage.md).

For development commands, renderer options, packaging, and verification gates,
see the [development guide](docs/development.md).

## Releases

GitHub Releases provide Windows x86_64, Linux x86_64/aarch64, and macOS Apple
Silicon, Intel, and universal application bundles. Release tags use
`YYYY-MM-DD` or `YYYY-MM-DD-N`; the annotated tag starts the release workflow
after version metadata is synchronized on the default branch. See the
[release guide](docs/development.md#github-releases) for the required commands.

## Documentation

- [Usage guide](docs/usage.md)
- [Development and verification](docs/development.md)
- [Architecture](docs/architecture.md)
- [Documentation index](docs/README.md)

## Support and Security

Report bugs and feature requests through the [issue tracker](https://github.com/xinalbert/ax_ssh/issues/new)
or the **Report a bug** action in the application's About page. Passwords are
stored only when requested, using the encrypted application vault by default or
the explicitly selected system credential store. Plaintext passwords,
private-key passphrases, terminal output, and live worker state are not written
to session JSON. Telnet is unencrypted, and unknown or changed SSH host keys
remain blocked until explicitly confirmed.

## License

AxSSH's original software and original application assets are licensed under
the [GNU General Public License version 3 only](LICENSE). Checked-in third-party
source and bundled fonts retain their own licenses; see
[Third-Party Notices](THIRD_PARTY_NOTICES.md).

The application's About page displays the standard Slint attribution component
alongside the AxSSH license identifier.
