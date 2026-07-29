[中文说明](README.zh.md)

# AxSSH

AxSSH is a cross-platform SSH workspace built with Rust, Slint, Tokio, and
russh. Saved sessions can be organized into collapsible groups. The activity
bar group icons open the session sidebar, and clicking the active group again
closes it. The Local Shell icon only opens a new local terminal tab and never
changes the sidebar. Every local or SSH terminal tab has a unique runtime ID,
worker, and bounded terminal model, so opening the same server or local shell
repeatedly does not share output or process state.

The SSH workflow verifies a server's SHA-256 host-key fingerprint, accepts a
transient password, and can optionally remember it in the platform credential
store. Sessions may instead use a private key discovered from the user's
`.ssh` directory or a manually entered path; encrypted keys request a transient
passphrase. The authenticated worker opens a PTY shell and uses the same
terminal surface as local tabs.

The terminal is rendered as a font-linked cell grid with bounded scrollback,
ANSI colors, cell-coordinate selection, and a block cursor. Enter, Backspace,
Tab, Escape, arrows, Home/End, Insert/Delete, Page keys, Ctrl control bytes, and
xterm-style modified navigation are encoded for the active PTY. Unmodified
arrows follow the terminal's normal or application-cursor mode, so shell
history and full-screen programs receive the expected CSI or SS3 sequence.
Terminals, Settings, and the new-session editor share one top tab bar.
Overflowing tabs scroll horizontally with a touchpad or mouse wheel, while
mouse-drag scrolling is disabled. On macOS, the empty zero-tab strip and a
dedicated trailing title-bar space move the window; tabs, the activity bar,
the session sidebar, and terminal content remain interaction-only regions.

Terminal, shortcut, local-shell, and workspace settings are managed in a
Settings tab and persisted in the versioned `sessions.json`; discovered shell
names are cached and only newly available names are added later. JetBrains Mono
is bundled under the SIL Open Font License. SFTP and full mouse-oriented
terminal protocol support remain staged.

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

Terminal selection uses normal pointer interaction. macOS keeps `Cmd+C`/`Cmd+V`
for clipboard actions; other platforms use `Ctrl+Shift+C`/`Ctrl+Shift+V`.
Plain `Ctrl+C` remains the terminal interrupt byte and other Ctrl combinations
remain available to shells and tools such as tmux. Workspace commands use the
platform modifier, including `Cmd+S` on macOS or `Ctrl+S` elsewhere to toggle
the sidebar. AxSSH restores physical Control/Command semantics after Slint's
Apple-platform modifier mapping, so macOS `Ctrl+B` reaches tmux while `Cmd+B`
remains a UI shortcut candidate. Terminal Ctrl input takes priority over a
conflicting workspace shortcut. The configurable right-click quick action
copies an active selection or pastes when none exists.

The visible terminal is a rendered grid rather than a text editor. A fully
transparent input-method proxy follows the terminal cursor so Chinese IME
preedit and candidate UI remain native; only committed text crosses into the
bounded PTY input path.

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
