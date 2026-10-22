//! Exercise font initialization through real restoration and retry controllers.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

fn restored_ssh(harness: &Harness) -> (Uuid, Uuid) {
    let mut app = harness.state.lock().unwrap();
    let profile = &mut app.sessions.sessions[0];
    // The saved trust snapshot leads only to a password prompt. This test
    // never supplies credentials or opens a network connection.
    profile.ssh_mut().unwrap().host_key_fingerprint = Some("SHA256:font-fixture".into());
    let profile = profile.clone();
    let tab_id = app.open_terminal_tab(&profile);
    let snapshot = app.workspace_snapshot();
    app.restore_workspace_tabs(&snapshot.tabs);
    (tab_id, profile.id)
}

fn wait_for_fonts(harness: &Harness) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        harness.status();
        let fonts = harness.font_registry.lock().unwrap();
        if fonts.is_registered("Maple Mono NF CN")
            && fonts.is_registered("Iosevka Term")
            && harness.ui.get_font_registry_generation() > 0
        {
            return;
        }
        drop(fonts);
        assert!(
            std::time::Instant::now() < deadline,
            "terminal fonts were never initialized"
        );
        std::thread::yield_now();
    }
}

#[test]
fn resuming_an_existing_ssh_terminal_initializes_its_fonts() {
    isolated(
        "terminal_fonts::resuming_an_existing_ssh_terminal_initializes_its_fonts",
        || {
            let harness = Harness::new();
            let (tab_id, profile_id) = restored_ssh(&harness);
            let started = Arc::new(AtomicBool::new(false));
            let context = ConnectionContext::new(
                harness.ui.as_weak(),
                harness.state.clone(),
                harness._runtime.handle().clone(),
                harness.font_registry.clone(),
                started.clone(),
            );
            resume_existing_connection(&context, tab_id, profile_id, ConnectionTarget::Terminal);
            assert!(
                started.load(Ordering::Acquire),
                "resume bypassed terminal font initialization"
            );
            wait_for_fonts(&harness);
            #[cfg(target_os = "macos")]
            assert_ui_font_context_shapes_reported_symbols(&harness);
            let app = harness.state.lock().unwrap();
            let terminal = app.terminal(tab_id).unwrap();
            assert!(terminal.worker.is_none());
            assert!(matches!(
                terminal.ssh_phase(),
                Some(SshConnectionPhase::AwaitingAuthentication { .. })
            ));
        },
    );
}

fn restored_local(harness: &Harness) -> Uuid {
    let mut app = harness.state.lock().unwrap();
    // Fail before spawning a process; restoration still traverses the actual
    // local controller and its asynchronous completion path.
    app.sessions.settings.terminal.local_shell = harness
        .directory
        .join("missing-font-test-shell")
        .to_string_lossy()
        .into_owned();
    let id = app.open_local_shell_tab();
    let snapshot = app.workspace_snapshot();
    app.restore_workspace_tabs(&snapshot.tabs);
    id
}

#[test]
fn restored_local_workspace_initializes_fonts_without_opening_a_new_tab() {
    isolated(
        "terminal_fonts::restored_local_workspace_initializes_fonts_without_opening_a_new_tab",
        || {
            let harness = Harness::new();
            let id = restored_local(&harness);
            let started = Arc::new(AtomicBool::new(false));
            let context = ConnectionContext::new(
                harness.ui.as_weak(),
                harness.state.clone(),
                harness._runtime.handle().clone(),
                harness.font_registry.clone(),
                started.clone(),
            );
            context.resume_restored_terminals();
            assert!(started.load(Ordering::Acquire));
            wait_for_fonts(&harness);
            assert_eq!(harness.state.lock().unwrap().restored_local_tabs(), [id]);
            #[cfg(target_os = "macos")]
            assert_ui_font_context_shapes_reported_symbols(&harness);
        },
    );
}

#[test]
fn retrying_a_restored_local_terminal_initializes_fonts() {
    isolated(
        "terminal_fonts::retrying_a_restored_local_terminal_initializes_fonts",
        || {
            let harness = Harness::new();
            let id = restored_local(&harness);
            {
                let mut app = harness.state.lock().unwrap();
                assert!(harness.router.include_tab(MAIN_WINDOW_ID, id));
                assert!(harness.router.activate_tab(MAIN_WINDOW_ID, id, &mut app));
            }
            assert!(
                harness
                    .ui
                    .invoke_terminal_pane_command(id.to_string().into(), "retry".into())
            );
            wait_for_fonts(&harness);
        },
    );
}

#[test]
fn restoring_only_sftp_does_not_request_terminal_fonts() {
    isolated(
        "terminal_fonts::restoring_only_sftp_does_not_request_terminal_fonts",
        || {
            let harness = Harness::new();
            let (old, profile_id) = restored_ssh(&harness);
            {
                let mut app = harness.state.lock().unwrap();
                assert!(app.close_tab(old).is_some());
                let profile = app
                    .sessions
                    .sessions
                    .iter()
                    .find(|p| p.id == profile_id)
                    .unwrap()
                    .clone();
                app.open_sftp_tab(&profile);
            }
            let started = Arc::new(AtomicBool::new(false));
            let context = ConnectionContext::new(
                harness.ui.as_weak(),
                harness.state.clone(),
                harness._runtime.handle().clone(),
                harness.font_registry.clone(),
                started.clone(),
            );
            context.resume_restored_terminals();
            assert!(!started.load(Ordering::Acquire));
            assert!(
                !harness
                    .font_registry
                    .lock()
                    .unwrap()
                    .is_registered("Iosevka Term")
            );
        },
    );
}

#[cfg(target_os = "macos")]
fn assert_ui_font_context_shapes_reported_symbols(harness: &Harness) {
    use i_slint_core::textlayout::sharedparley::parley;
    use i_slint_core::window::WindowInner;
    let context = WindowInner::from_pub(harness.ui.window()).context().clone();
    let mut fonts = context.font_context().borrow_mut();
    let mut layouts = parley::LayoutContext::<()>::new();
    let text = "↳✳✻·✻✽✻✶·";
    let mut builder = layouts.ranged_builder(&mut fonts.inner, text, 1.0, false);
    builder.push_default(parley::StyleProperty::FontFamily(
        parley::FontFamily::Single(parley::FontFamilyName::named("Iosevka Term")),
    ));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let runs = layout
        .lines()
        .flat_map(|line| line.runs())
        .collect::<Vec<_>>();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| {
        run.clusters()
            .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != 0))
    }));
}
