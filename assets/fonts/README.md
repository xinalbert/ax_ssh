# Bundled Application And Terminal Fonts

AxSSH distributes these independently licensed fonts with the application.
They are not Slint imports. The four JetBrains Mono faces are compiled into the
executable as the always-available application and Terminal default. The other
families are read from this directory on a blocking worker. All font bytes are
registered through the application `FontRegistry` on the Slint UI thread. The
selected Terminal family remains primary; Maple Mono NF CN is the one `Hani`
fallback when that family lacks a glyph. No renderer-side substitution path is
used. Both Settings font lists present these bundled families before discovered
system monospace fonts.

| Family | Files | License |
| --- | --- | --- |
| Maple Mono NF CN | Regular, Bold | `LICENSE-MapleMono.txt` |
| Iosevka Term | Regular, Bold, Italic, Bold Italic | `LICENSE-Iosevka.txt` |
| JetBrains Mono | Regular, Bold, Italic, Bold Italic | `LICENSE-JetBrainsMono.txt`, `AUTHORS-JetBrainsMono.txt` |
| Monaspace Neon Var | Variable | `LICENSE-Monaspace.txt` |

Release packages must retain `assets/fonts/` next to the executable, or under
the platform resources path resolved by `src/app/font_bridge.rs`. Maple Mono NF
CN is a required runtime resource for deterministic Han rendering; Iosevka Term
and Monaspace Neon remain optional primary families, and all license notices
must stay with the package. The files were imported into this project as static
resources; AxSSH never loads them from
`third_package/axshell` at build time or runtime.
