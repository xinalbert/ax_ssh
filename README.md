[简体中文](README.zh.md)

# AxSSH

[![CI](https://img.shields.io/github/actions/workflow/status/xinalbert/ax_ssh/ci.yml?branch=master&label=CI)](https://github.com/xinalbert/ax_ssh/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/xinalbert/ax_ssh/branch/master/graph/badge.svg)](https://app.codecov.io/gh/xinalbert/ax_ssh)
[![Latest release](https://img.shields.io/github/v/release/xinalbert/ax_ssh)](https://github.com/xinalbert/ax_ssh/releases/latest)
[![License](https://img.shields.io/github/license/xinalbert/ax_ssh)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.92.0%2B-dea584?logo=rust)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-1f6feb)](https://github.com/xinalbert/ax_ssh/releases/latest)
[![English](https://img.shields.io/badge/English-Click-1f6feb)](docs/usage.md)
[![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-%E7%82%B9%E5%87%BB-dea584)](docs/usage.zh.md)
[![繁體中文](https://img.shields.io/badge/%E7%B9%81%E9%AB%94%E4%B8%AD%E6%96%87-%E9%BB%9E%E6%93%8A-dea584)](docs/usage.zh-TW.md)
[![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-%E3%82%AF%E3%83%AA%E3%83%83%E3%82%AF-blue)](docs/usage.ja.md)
[![한국어](https://img.shields.io/badge/%ED%95%9C%EA%B5%AD%EC%96%B4-%EC%97%B4%EA%B8%B0-blue)](docs/usage.ko.md)
[![Deutsch](https://img.shields.io/badge/Deutsch-Klicken-blue)](docs/usage.de.md)
[![Español](https://img.shields.io/badge/Espa%C3%B1ol-Abrir-blue)](docs/usage.es.md)
[![Français](https://img.shields.io/badge/Fran%C3%A7ais-Ouvrir-blue)](docs/usage.fr.md)
[![Português (Brasil)](https://img.shields.io/badge/Portugu%C3%AAs%20%28Brasil%29-Abrir-blue)](docs/usage.pt-BR.md)
[![Русский](https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-%D0%9E%D1%82%D0%BA%D1%80%D1%8B%D1%82%D1%8C-blue)](docs/usage.ru.md)
[![Українська](https://img.shields.io/badge/%D0%A3%D0%BA%D1%80%D0%B0%D1%97%D0%BD%D1%81%D1%8C%D0%BA%D0%B0-%D0%92%D1%96%D0%B4%D0%BA%D1%80%D0%B8%D1%82%D0%B8-blue)](docs/usage.uk.md)
[![English (United Kingdom)](https://img.shields.io/badge/English%20%28United%20Kingdom%29-Click-1f6feb)](docs/usage.en-GB.md)

Choose the interface language in **Settings > General > Language**.
See the [translation workflow](translations/README.md) for supported locales and maintenance.

AxSSH is a cross-platform desktop terminal workspace built with Rust, Slint,
and Tokio. It brings saved SSH, Telnet, Serial, and local-shell sessions
together with independent Terminal and SFTP Tabs in one native application.

## Highlights

- Connect over SSH with a password, private key, or runtime agent, with explicit
  SHA-256 host-key confirmation before a host is trusted.
- Work in local or remote terminals with bounded scrollback, ANSI rendering,
  selection, clipboard, native input-method support, and one normalized keyboard
  boundary. Physical numeric keypads also reach terminal applications, including
  DEC application-keypad mode, on supported desktop platforms.
- Browse local and remote files in a dedicated SFTP Tab, with bounded directory
  listing, recursive downloads, uploads, rename, delete, and remote editing
  through system-associated applications.
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
