vte
===

[![Build Status](https://travis-ci.org/alacritty/vte.svg?branch=master)](https://travis-ci.org/alacritty/vte)
[![Crates.io Version](https://img.shields.io/crates/v/vte.svg)](https://crates.io/crates/vte/)

Parser for implementing virtual terminal emulators in Rust.

The parser is implemented according to [Paul Williams' ANSI parser state
machine]. The state machine doesn't assign meaning to the parsed data and is
thus not itself sufficient for writing a terminal emulator. Instead, it is
expected that an implementation of the `Perform` trait which does something
useful with the parsed data. The `Parser` handles the book keeping, and the
`Perform` gets to simply handle actions.

See the [docs] for more info.

[Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser
[docs]: https://docs.rs/crate/vte/

## AxSSH patch

The source is based on the crates.io VTE 0.15.0 package with SHA-256
`a5924018406ce0063cd67f8e008104968b74b563ee1b85dde3ed1f7cb87d3dbd`.

- Route LF, VT, and FF through the newline-mode callback and preserve IND.
- Map SGR 21 to double underline and expose `CSI 16 t` to the handler.
- Limit OSC payload and parameter counts, require complete BEL or ST
  termination, and discard canceled or overlong sequences.
- Pass the full OSC 52 target to the handler and keep raw OSC data out of
  diagnostics. Reject malformed UTF-8 in OSC 8 URIs.

The AxSSH handler owns palette lookup, clipboard target policy, hyperlink
limits, and delivery of protocol responses.
