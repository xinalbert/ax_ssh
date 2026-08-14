[简体中文](README.zh.md)

# AxSSH

AxSSH is a cross-platform desktop terminal workspace built with Rust, Slint,
and Tokio. It combines saved SSH, Telnet, and serial sessions with independent
local and remote terminal tabs and persistent workspace settings.

Current functionality includes SSH password, private-key, and runtime-agent
authentication with explicit host-key confirmation, plaintext Telnet, manually initiated serial
connections with automatic port discovery, bounded terminal scrollback, ANSI
rendering, selection, clipboard shortcuts, native input-method support, and
bounded remote SFTP directory browsing in dedicated dual-pane SFTP tabs. The
local pane reads only bounded directory metadata and can open regular files with
the platform default application. Remote regular files can be downloaded into a
private cache and opened from the bounded transfer queue. SFTP upload, delete,
edit, reconnect, workspace restoration, and full
terminal mouse reporting are not implemented yet. A connected SSH Terminal and
its SFTP companion can move together into a separate native window and return
without reconnecting.

## Quick start

Install Rust `1.92.0` or newer and use a desktop environment supported by
Slint's winit backend, then run:

```bash
cargo run --locked
```

The first connection to a host is rejected until you verify and explicitly
confirm its SHA-256 host-key fingerprint. See the [usage guide](docs/usage.md)
for session setup, terminal controls, settings, and data-storage behavior.

## Releases

GitHub Releases provide Windows x86_64, Linux x86_64/aarch64, and macOS Apple
Silicon, Intel, and universal application bundles. Run **Create Dated Release**
from the default branch only to create a new Shanghai-date tag such as
`2026-08-12`; use revision `1` for a second same-day release such as
`2026-08-12-1`. It synchronizes Cargo and macOS metadata first. Pushing an
annotated `YYYY-MM-DD[-N]` tag directly also starts the same CI-to-Release
chain. To retry CI or packaging for an existing valid tag, run **Retry Existing
Release** with its exact value. Both paths validate the tag and metadata,
require successful CI for the exact tag SHA, and cannot create, replace, or
move tags. Each published Release groups high-signal commits into a short
Highlights section with an explicit full-changelog link, then retains GitHub's
generated release notes for the complete change list.

## Documentation

- [Usage guide](docs/usage.md)
- [Architecture](docs/architecture.md)
- [Development and verification](docs/development.md)
- [Documentation index](docs/README.md)
- [Implementation tracker](docs/project-implementation-tracker/current.md)

## Security and repository boundary

Unknown and changed SSH host keys are denied by default. Remembered passwords are
stored by macOS Keychain, Windows Credential Manager, or Unix Secret Service;
passwords, private-key passphrases, private-key contents, SSH-agent socket paths
and identities, terminal output, and live worker state are never written to the
session JSON. Telnet is unencrypted.
Serial discovery lists device metadata only; a device is opened only after an
explicit connect action.

`third_package/axshell` is reference material only. It is not a Cargo workspace
member, source import, runtime dependency, build input, or documentation
dependency. AxSSH uses Slint for its UI, russh and russh-sftp for SSH/SFTP,
libmudtelnet-rs for Telnet protocol events, and tokio-serial for serial
transport.

## License

AxSSH's original software and original application assets are licensed under
the [GNU General Public License version 3 only](LICENSE). Checked-in third-party
source and bundled fonts retain their own licenses; see
[Third-Party Notices](THIRD_PARTY_NOTICES.md).

The application's About page displays the standard Slint attribution component
alongside the AxSSH license identifier.
