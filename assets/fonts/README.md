# Bundled Application And Terminal Fonts

AxSSH distributes these independently licensed fonts beside the application.
They are runtime resources, not Slint imports: the independently selected
application and Terminal families are read from this directory on a blocking
worker and registered on the Slint UI thread. Both Settings font lists present
these bundled families before discovered system monospace fonts.

| Family | Files | License |
| --- | --- | --- |
| Maple Mono NF CN | Regular, Bold | `LICENSE-MapleMono.txt` |
| Iosevka Term | Regular, Bold, Italic, Bold Italic | `LICENSE-Iosevka.txt` |
| JetBrains Mono | Regular, Bold, Italic, Bold Italic | `LICENSE-JetBrainsMono.txt`, `AUTHORS-JetBrainsMono.txt` |
| Monaspace Neon Var | Variable | `LICENSE-Monaspace.txt` |

Release packages must retain `assets/fonts/` next to the executable, or under
the platform resources path resolved by `src/app/font_bridge.rs`. The files
were imported into this project as static resources; AxSSH never loads them from
`third_package/axshell` at build time or runtime.
