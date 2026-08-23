# Third-Party Notices

AxSSH's original software and original application assets are licensed under
the GNU General Public License version 3 only. See `LICENSE`.

The following components and assets remain under their own licenses. Their
license terms are not replaced by the AxSSH license.

## Slint

AxSSH uses Slint 1.17.1 for its user interface. Slint is offered under
`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR
LicenseRef-Slint-Software-3.0`; AxSSH selects the `GPL-3.0-only` option.

Copyright SixtyFPS GmbH and the Slint contributors.

The AxSSH About page also displays Slint's standard `AboutSlint` attribution
component.

AxSSH carries small, source-available local patches for the locked Slint winit
backend and `softbuffer` 0.4.8 under `vendor/i-slint-backend-winit/` and
`vendor/softbuffer/`. The patches preserve their upstream licenses and limit
the behavior change to forwarding multiple damage rectangles and making the
macOS CoreGraphics surface use a persistent tiled framebuffer. They do not
change Slint's UI language or application APIs.

## Bundled Fonts

The font files under `assets/fonts/` are distributed under the SIL Open Font
License 1.1. Their copyright, author, reserved-name, and license notices are in:

- `assets/fonts/LICENSE-MapleMono.txt`
- `assets/fonts/LICENSE-Iosevka.txt`
- `assets/fonts/LICENSE-JetBrainsMono.txt`
- `assets/fonts/AUTHORS-JetBrainsMono.txt`
- `assets/fonts/LICENSE-Monaspace.txt`

## Vendored vt100 Source

The retained source under `vendor/vt100/` is licensed under the MIT License.
Its copyright and license notice are in `vendor/vt100/LICENSE`.

## Platform File Icons

AxSSH obtains file-type icons from operating-system facilities: AppKit and
Uniform Type Identifiers on macOS, Windows Shell and GDI APIs on Windows, and
freedesktop icon themes plus MIME mappings on Linux. Operating-system artwork
is resolved at runtime and is not copied into this repository.

The Rust implementation uses the locked `image`, `freedesktop-icons`,
`mime_guess`, `objc2` family, and `windows-sys` crates on their applicable
targets. They remain under the license terms declared by their respective
copyright holders and recorded in the Cargo dependency metadata.

## Cargo Dependencies

Other Rust dependencies resolved by `Cargo.lock` remain under the license terms
declared by their respective copyright holders. A binary distributor must keep
the corresponding notices and source-availability obligations required by
those dependency licenses.
