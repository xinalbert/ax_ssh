# AxSSH Slint Guidelines

## Version Rule

Use the Slint version resolved by `Cargo.lock` as the executable API baseline;
it is currently `1.17.1`. Consult `latest` official guidance for current style,
but do not write APIs from a newer release until Cargo dependencies, MSRV, CI,
and migration notes are updated together.

## Files And Exports

- Compile only `ui/app.slint` in `build.rs` with `slint_build::compile`.
- Split cohesive reusable views into sibling `.slint` files and import them by
  relative path from the entry graph. Avoid a file per trivial element.
- Export only the top-level component, DTO structs/enums, globals, and reusable
  components that must cross a file or Rust boundary.
- Keep visual tokens in a theme global. Do not use globals as a mutable domain
  store, worker registry, or substitute for explicit component properties.
- Keep `slint::include_modules!()` in `src/app.rs`; domain modules must not name
  generated Slint component, model, string, or image types.

## Declarative Contracts

- Use properties for data and declarative visual state. Use callbacks for user
  intent and commands that Rust handles.
- Choose `in`, `out`, or `in-out` according to actual ownership. Avoid `in-out`
  when one side is authoritative.
- Compute derived presentation with bindings rather than mirrored mutable state.
- Keep callback payloads small and typed. Convert UI DTOs to domain values in
  `src/app.rs`, validate there or in the domain owner, and return concise state.
- Keep filesystem, SSH, Tokio, terminal parsing, and sleeps out of `.slint`.

## Rust Integration And Threads

- Create Slint components and run their event loop on the main/UI thread.
- Capture `Weak<AppWindow>` in callbacks and worker completions. Capturing a
  strong component handle in its own callback can create a reference cycle.
- Run russh/Tokio work on the project's multi-thread runtime. Send only owned,
  bounded, `Send + 'static` results back through
  `slint::invoke_from_event_loop`, then upgrade the weak handle inside it.
- Use `slint::spawn_local` only for non-`Send`, UI-thread-local futures. Do not
  place Tokio I/O futures there without an explicit, documented compatibility
  adapter and a reason to depart from the project runtime boundary.
- Treat window closure as normal: a failed weak upgrade means the UI is gone.
  Do not keep it alive from a background task.

## Models And High-Frequency Data

- Represent repeated UI data as a Slint model (`ModelRc` with a suitable model
  implementation), not as hand-created parallel child state.
- Use `ListView` or another virtualized/bounded view for growing collections.
- Give repeated rows stable heights and text overflow behavior so model changes
  do not move surrounding layout.
- Mutate model rows or batch snapshot replacement according to update rate.
  Avoid rebuilding a large model for every terminal chunk or status tick.
- Bound terminal scrollback and aggregate worker output before crossing the UI
  event loop. Keep terminal emulation in a dedicated Rust domain, not `Text`.

## Layout, Input, And Accessibility

- Prefer layouts and responsive constraints over absolute positioning. Define
  min/max/preferred dimensions where window or tool geometry needs stability.
- Prefer Slint standard widgets for buttons, edits, views, and other controls.
- Make custom interactive elements keyboard reachable and visibly focused.
  Supply the appropriate `accessible-role`, `accessible-label`, value, and
  enabled/checked state where native widget semantics are not available.
- Keep text readable at supported window sizes; wrap or elide intentionally.
  Verify dialogs and repeated content at the declared minimum window size.
- Avoid animation or visual bindings that trigger filesystem, network, model
  rebuild, or other expensive Rust callbacks on every frame.

## Build And Verification

- Let `slint_build::compile` report every imported `.slint` dependency to Cargo;
  keep `build.rs` deterministic and free of network/runtime discovery.
- A successful syntax edit alone is insufficient. Run Cargo check so generated
  Rust types and callback/property signatures are type-checked together.
- Test Rust-side mapping, state, and worker behavior without a real window when
  possible. Manually verify rendering, focus order, keyboard input, resizing,
  accessibility, and high-frequency updates for affected UI flows.

## Official Sources

- Slint file imports and exports:
  <https://docs.slint.dev/latest/docs/slint/guide/language/coding/file/>
- Properties and callbacks:
  <https://docs.slint.dev/latest/docs/slint/guide/language/coding/properties/>
  and <https://docs.slint.dev/latest/docs/slint/guide/language/coding/functions-and-callbacks/>
- Globals and models:
  <https://docs.slint.dev/latest/docs/slint/guide/language/coding/globals/>
  and <https://docs.slint.dev/latest/docs/slint/guide/language/coding/repetition-and-data-models/#models>
- Accessibility properties:
  <https://docs.slint.dev/latest/docs/slint/reference/common/#accessibility-properties>
- Rust event-loop dispatch and local futures:
  <https://docs.slint.dev/latest/docs/rust/slint/fn.invoke_from_event_loop.html>
  and <https://docs.slint.dev/latest/docs/rust/slint/fn.spawn_local.html>
- Slint build API:
  <https://docs.slint.dev/latest/docs/rust/slint_build/fn.compile.html>
