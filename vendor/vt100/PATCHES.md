# AxSSH Patch Notes

This directory vendors `vt100` 0.16.2 under its upstream MIT license.

`src/grid.rs` differs from upstream only when a grid shrinks in columns. It
uses `Row::truncate` instead of `Row::resize` so a wide character whose second
cell is removed cannot remain as an orphaned first cell. The change protects
both normal and alternate screens because `Screen::set_size` resizes both.

The AxSSH regression test is
`terminal::tests::narrowing_past_a_wide_character_keeps_the_grid_writable`.
Remove this patch and the Cargo `[patch.crates-io]` override when an upstream
release includes the fix.
