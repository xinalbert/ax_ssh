# Third-Party Notices

AxSSH's original software and original application assets are licensed under
the GNU General Public License version 3 only. See `LICENSE`.

The following components and assets remain under their own licenses. Their
license terms are not replaced by the AxSSH license.

## Slint

AxSSH uses Slint 1.18.1 for its user interface. Slint is offered under
`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR
LicenseRef-Slint-Software-3.0`; AxSSH selects the `GPL-3.0-only` option.

Copyright SixtyFPS GmbH and the Slint contributors.

The AxSSH About page also displays Slint's standard `AboutSlint` attribution
component.

AxSSH carries source-available local patches for the locked Slint winit and
Skia renderer crates and `softbuffer` 0.4.8 under
`vendor/i-slint-backend-winit/`, `vendor/i-slint-renderer-skia/`, and
`vendor/softbuffer/`. The patches preserve their upstream licenses and limit
the behavior changes to forwarding multiple damage rectangles, making the
macOS CoreGraphics surface use a persistent tiled framebuffer, and releasing
per-component Skia layer images when a component is destroyed. They do not
change Slint's UI language or application APIs.

## Bundled Fonts

The font files under `assets/fonts/` are distributed under the SIL Open Font
License 1.1. Their copyright, author, reserved-name, and license notices are in:

- `assets/fonts/LICENSE-MapleMono.txt`
- `assets/fonts/LICENSE-Iosevka.txt`
- `assets/fonts/LICENSE-JetBrainsMono.txt`
- `assets/fonts/AUTHORS-JetBrainsMono.txt`
- `assets/fonts/LICENSE-Monaspace.txt`

## Platform File Icons

AxSSH obtains file-type icons from operating-system facilities: AppKit and
Uniform Type Identifiers on macOS, Windows Shell and GDI APIs on Windows, and
freedesktop icon themes plus MIME mappings on Linux. Operating-system artwork
is resolved at runtime and is not copied into this repository.

The Rust implementation uses the locked `image`, `freedesktop-icons`,
`mime_guess`, `objc2` family, and `windows-sys` crates on their applicable
targets. They remain under the license terms declared by their respective
copyright holders and recorded in the Cargo dependency metadata.

## Operating-System And Distribution Icon Assets

The SVG/PNG files under `assets/os-icons/` were obtained through Iconify,
publisher websites and official repositories, and community icon projects.
The collection authors and licenses are:

- selfh.st/icons by selfh.st: CC BY 4.0.
- theSVG by thesvg.org: MIT.
- SVG Logos by Gil Barbara: CC0 1.0 Universal.
- Skill Icons by tandpfun: MIT.
- Papirus icon theme by the Papirus Development Team: GNU GPL version 3.
- Font Logos by Lukas W. and contributors: The Unlicense.

CNCF artwork follows the Linux Foundation trademark usage guidelines, preserved
in `assets/os-icons/licenses/NOTICE-cncf-artwork.txt`. Publisher originals have
individual branding terms and no common asset copyright license. Their source
pages and policy links are recorded in
`assets/os-icons/licenses/NOTICE-official-branding.txt` and per-asset metadata.
The Windows Server reference copy is separately attributed in
`assets/os-icons/licenses/NOTICE-wikimedia-commons.txt`; its Commons page
records the 2017 Microsoft artwork as public domain, while Microsoft
trademarks remain with Microsoft Corporation.
The connection-group folder UI icon is the Iconify `material-symbols:folder-outline-rounded`
asset from Google's Material Design Icons repository and is separately
attributed under `assets/os-icons/licenses/NOTICE-iconify-material-symbols.txt`.
An unspecified asset license is explicitly recorded rather than replaced by
MIT, CC0, or a software repository's code license.

Original publisher/community files are unmodified; Iconify bodies only receive
an SVG wrapper. Collection attribution,
original license notices, per-icon source links, source revisions/package
integrity, and hashes are preserved in the
[resource catalog](assets/os-icons/README.md),
[Chinese catalog](assets/os-icons/README.zh.md), and
[manifest](assets/os-icons/manifest.json). The
[search coverage](assets/os-icons/search-coverage.json) distinguishes dedicated
system marks from generic vendor/platform branding and unconfirmed results.
Preserve the corresponding notices
and attribution when distributing these assets. Brand trademarks remain with
their respective owners.

## Cargo Dependencies

Other Rust dependencies resolved by `Cargo.lock` remain under the license terms
declared by their respective copyright holders. A binary distributor must keep
the corresponding notices and source-availability obligations required by
those dependency licenses.
