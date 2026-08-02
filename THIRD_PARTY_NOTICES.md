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

## Cargo Dependencies

Other Rust dependencies resolved by `Cargo.lock` remain under the license terms
declared by their respective copyright holders. A binary distributor must keep
the corresponding notices and source-availability obligations required by
those dependency licenses.
