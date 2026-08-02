# Ion Terminal Icon Assets

The canonical source is `terminal_icon.svg`, copied from the user-provided Ion
asset. Every raster and container format in
`terminal_icon_all_formats/` was generated from that SVG; no icon file or
source code was copied from the reference project.

## Formats

- `terminal_icon.svg`: canonical vector source (also retained at the `assets/ion/` root).
- `terminal_icon.png`: 1024 x 1024 RGBA PNG.
- `terminal_icon_<size>.png`: RGBA PNGs at 16, 24, 32, 48, 64, 128, 256,
  512, and 1024 pixels.
- `terminal_icon.ico`: Windows icon containing 16, 24, 32, 48, 64, 128, and
  256 pixel images.
- `terminal_icon.icns`: macOS icon containing the standard Retina icon sizes
  through 1024 pixels.

## Application integration

- `ui/app.slint` embeds the 256px PNG as the Slint/winit window icon on all
  supported platforms.
- Windows builds compile `packaging/windows/axssh.rc` through
  `embed-resource`, embedding the multi-size ICO in the executable for the
  shell, taskbar, and executable-file views.
- macOS startup installs the 256px PNG as the running application's Dock icon.
  `packaging/macos/build-app.sh` creates an `AxSSH.app` bundle whose
  `Info.plist` selects the ICNS resource for Finder, Launchpad, and the Dock.
- Linux packages use `packaging/linux/axssh.desktop` and install the generated
  PNG sizes under the matching hicolor application-icon directories through
  the `cargo-deb` metadata in `Cargo.toml`.

When the canonical SVG changes, regenerate every derived image and container
together. The build and packaging paths must not read icon assets from
`third_package/axshell`.
