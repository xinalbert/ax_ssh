# AxSSH egui transition

- Read `docs/egui-migration.md` and the active tracker before extending the
  experiment. Slint remains the default and the egui binary runs independently.
- Keep `eframe`/`egui` types in `src/egui_app.rs` and children. The library
  controller holds PTY/model ownership and exposes bounded intents and owned
  snapshots; no UI dependency or worker receiver crosses that boundary.
- Never parse terminal output, run filesystem/network work, join tasks or wait
  on Tokio from an egui drawing callback. Load font bytes before the event loop
  on blocking workers; draw visible rows and cache by revision and geometry.
- Process every PTY byte and protocol reply in order. Only parsed presentation
  snapshots may replace older snapshots. Compare row revisions after skipped
  frames; the final dirty_rows alone is insufficient.
- Use stable widget IDs and apply input once per egui frame, skipping sizing
  passes. Keep IME preedit local; send committed text once. Physical controls
  and Text events must not duplicate output.
- Reuse terminal model encoding, cell dimensions, selection and viewport rules.
  No raw terminal contents, clipboard text, titles or credentials in logs.
- Own cancellation and join beyond native window closure. Return from the
  event loop, cancel and await cleanup, then stop the runtime. Never use
  process::exit to bypass cleanup.
- Verify default, egui-only and combined builds; preserve the five CI targets.
  WGPU has no equivalent of Slint's CPU/damage path yet, so default Slint is the
  supported fallback. User confirmation is required for visual acceptance;
  agents must not capture/inspect their own application screenshots.

- Workspace state and writes belong to `controller/workspace.rs`; persistence
  schemas/atomic files remain in config. Use the isolated experimental store.
  Saved edits carry a catalog revision and exact request receipt; publish after
  writes succeed and keep drafts on errors. Bound active plus closing terminals.
- Reuse `controller/panes.rs` in both frontends. Scope widget/password IDs by pane
  UUID, route keys to one focused pane and disable terminal input behind editors.
  Validate import before closing existing routes, retain shutdown ownership and
  restore idle terminals without implicit process/network/device/auth actions.
