[简体中文](duplicate-link-cleanup.zh.md) · [Architecture](architecture.md)

# Duplicate-flow cleanup, 2026-10-03

Implementation follows the user's authorization after the repository audit.
The initial working-tree diff was saved, including the existing SFTP title-bar
controls and local Hidden changes, which are preserved.

## Phase 1: unused entry points and duplicate instances

| Item | Result | Evidence and active behavior |
| --- | --- | --- |
| `vendor/vt100/` | Removed the historical copy and its notice | No manifest/lock reference; terminal parsing uses `alacritty_terminal` |
| Duplicate icons | Removed formats-directory `terminal_icon.svg` and `terminal_icon.png` | Root SVG is canonical; the PNG matches the retained 1024px image; platform icons remain |
| Remote load/close UI callbacks | Removed callback forwarding, handlers and unused helpers | Edit uses `ReadText`; workspace Tab lifecycle closes workers |
| Single-upload and OpenSftp/CloseSftp wrappers | Removed unused methods and commands | Batch uploads remain; the independent worker owns SFTP startup/shutdown |
| Old SFTP sidebar mode | Removed `standalone` branches and size tokens | Both component call sites use the full workspace layout |
| Hidden SFTP instance in detached windows | Main-layout SFTP is created only in main windows | Prevents hidden geometry from overwriting visible drop regions |

## Phase 2: shared application flows

| Item | Shared implementation | Preserved differences |
| --- | --- | --- |
| Settings preview/save | `SettingsPane.submit-draft` | 84 draft parameters shared; tab ID and close flag still select persistence |
| Bundled-font loading/registration | `load_settings_fonts`, `register_settings_fonts` | Blocking-task IO and UI-thread registration; preview loads changed families |
| Local/remote directory history | `complete_directory_history` | Separate reads, path validation, pagination and request isolation |
| Telnet/Serial monitoring | `spawn_direct_monitor` | Separate workers, event adapters, endpoint status, protocol encoding and attempt checks |

## Phase 3: lifecycle and distribution

| Item | Result | Preserved constraints |
| --- | --- | --- |
| New/restored SSH probes | Shared `spawn_host_key_probe` and result transition | Cancellation and tab/profile/target/phase checks; unknown/changed/revoked trust rules |
| Old shell-owned SFTP browser | Removed creation, event forwarding and shutdown branches | SFTP-only worker owns browser/transfers; terminal shell/X11 remains |
| Single-window refresh without a router | Removed old snapshot/model branches | Startup must install routing before callbacks; bounded multi-window refresh remains |
| Atomic file writes | Shared `write_file_contents_atomically` | Private temporary files, fsync, closed handles, atomic replacement and failure cleanup; separate parent permission rules |
| Linux Debian packaging | Cargo metadata is the single asset list; CI uses cargo-deb 3.7.0 | Fonts/licenses, nine icon sizes, desktop entry and notices; version/architecture checks and native runtime dependency discovery |

Removed Rust handle methods are `request_open_sftp`, `request_close_sftp` and
`request_open_sftp_upload`. The crate declares `publish = false`; all repository
callers were checked. External consumers calling these methods must use the
SFTP-only worker or batch upload entry point.

## Verification and acceptance

- CI-style check, strict all-target Clippy and build passed for both
  `aarch64-apple-darwin` and `x86_64-apple-darwin`, using Rust/Cargo 1.97.1 and
  `CARGO_NET_OFFLINE=true`. Non-native Intel tests were not executed.

- Native locked/offline fmt, check, strict all-target Clippy, tests and debug
  build passed: 303 library and 312 application tests, with no failures. Release
  helpers passed 14 tests; 512 translations and Markdown/skill/tracker checks passed.
- Debian fixtures passed for amd64 and arm64: each verified 30 declared assets
  against exact bytes, paths, modes and root ownership, plus package/version/architecture.

- The original numerical regression reproduced local drop width changing from
  476 to 406 when the hidden main sidebar changed from 220 to 360. The fixed
  detached window ignores sidebar changes and still responds to window resizing
  and return to the main layout.
- Probe regressions cover trusted/unknown/changed/revoked results for Terminal
  and SFTP targets, stale tab/profile/target/phase and cancellation. Existing
  loopback tests continue to cover transport rejection.
- Atomic-write regressions cover distinct parent permissions, destination mode
  0600, and retaining the destination while cleaning temporary files on failure.
- Final results are maintained in the [current tracker](project-implementation-tracker/current.md)
  and [monthly log](project-implementation-tracker/changes/2026/10.md).
- GUI appearance, native drag/drop, focus and real SSH/Telnet/Serial lifecycles
  require user acceptance. Windows/Linux SDKs and native runners are unavailable
  locally; their CI checks remain. Packaging fixtures verify payload and control
  fields, without simulating Linux installation or runtime dependencies.

## Packaging sources

The [upstream cargo-deb contract](https://github.com/kornelski/cargo-deb#packagemetadatadeb-options)
and installed 3.7.0 CLI were checked for asset globs, `target/release/` rewriting
and `--no-build`. Rust dependencies, CI gates and SSH trust policy are preserved.
