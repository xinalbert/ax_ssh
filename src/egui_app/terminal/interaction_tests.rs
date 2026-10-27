use super::*;
use ax_ssh::config::AppearanceSettings;
use ax_ssh::controller::TerminalSession;
use std::sync::Arc;
use std::time::Duration;

fn script(body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("axssh-mouse-{}.sh", uuid::Uuid::new_v4()));
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn text(client: &TerminalClient) -> String {
    client
        .view()
        .unwrap()
        .terminal
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| run.text.as_str())
        .collect()
}

async fn wait(client: &TerminalClient, expected: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !text(client).contains(expected) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

fn draw(
    ctx: &egui::Context,
    pane: &mut TerminalPane,
    client: &TerminalClient,
    mut events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    let view = client.view().unwrap();
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            focused: true,
            events,
            time: Some(ctx.cumulative_frame_nr() as f64),
            ..Default::default()
        },
        |ui| {
            crate::egui_app::clipboard::guard(ui.ctx(), |owner| pane.owns_widget(owner));
            pane.show(ui, client, &view).unwrap();
        },
    )
}

async fn warm(ctx: &egui::Context, pane: &mut TerminalPane, client: &TerminalClient) {
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    draw(ctx, pane, client, vec![], egui::Modifiers::NONE).drop_without_applying_deltas();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.view().unwrap().terminal.max_columns != pane.size.unwrap().columns() as usize {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    draw(ctx, pane, client, vec![], egui::Modifiers::NONE).drop_without_applying_deltas();
}

fn pointer(
    pos: egui::Pos2,
    button: egui::PointerButton,
    pressed: bool,
    modifiers: egui::Modifiers,
) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        },
    ]
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_option_meta_encodes_original_keys_once_and_never_prefixes_ime_commits() {
    let path = script(
        "stty -echo -icanon min 1\nprintf 'META_READY\\r\\n'\nhex=$(dd bs=1 count=10 2>/dev/null | od -An -v -tx1 | tr -d ' \\n')\nprintf 'META_HEX:%s\\r\\n' \"$hex\"\nIFS= read -r finish",
    );
    let terminal = TerminalSession::spawn(
        &tokio::runtime::Handle::current(),
        path.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = terminal.client();
    wait(&client, "META_READY").await;
    let ctx = egui::Context::default();
    let mut pane = TerminalPane::default();
    warm(&ctx, &mut pane, &client).await;
    crate::egui_app::native_input::emulate(&ctx, false);
    draw(
        &ctx,
        &mut pane,
        &client,
        vec![egui::Event::Text("∫".into())],
        egui::Modifiers::ALT,
    )
    .drop_without_applying_deltas();
    crate::egui_app::native_input::emulate(&ctx, true);
    draw(
        &ctx,
        &mut pane,
        &client,
        vec![
            egui::Event::Key {
                key: egui::Key::B,
                physical_key: Some(egui::Key::B),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            },
            egui::Event::Text("b".into()),
            egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            },
            egui::Event::Ime(egui::ImeEvent::Commit("中".into())),
            egui::Event::Text("中".into()),
        ],
        egui::Modifiers::ALT,
    )
    .drop_without_applying_deltas();
    wait(&client, "META_HEX:e288ab1b621b7fe4b8ad").await;
    terminal.shutdown().await.unwrap();
    std::fs::remove_file(path).unwrap();
}

fn drag(
    ctx: &egui::Context,
    pane: &mut TerminalPane,
    client: &TerminalClient,
    a: egui::Pos2,
    b: egui::Pos2,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    draw(
        ctx,
        pane,
        client,
        pointer(a, egui::PointerButton::Primary, true, modifiers),
        modifiers,
    )
    .drop_without_applying_deltas();
    draw(
        ctx,
        pane,
        client,
        vec![egui::Event::PointerMoved(b)],
        modifiers,
    )
    .drop_without_applying_deltas();
    draw(
        ctx,
        pane,
        client,
        pointer(b, egui::PointerButton::Primary, false, modifiers),
        modifiers,
    )
}

#[tokio::test]
async fn reporting_gestures_keep_local_drags_alt_override_and_cancelled_clicks_distinct() {
    for (priority, modifiers, cancelled) in [
        (true, egui::Modifiers::NONE, true),
        (false, egui::Modifiers::SHIFT, false),
        (true, egui::Modifiers::ALT, false),
    ] {
        let path = script(
            "stty raw -echo\nprintf 'AX_READY\\r\\ncopy-value\\r\\n\\033[?1000h\\033[?1006h'\nbytes=$(dd bs=1 count=18 2>/dev/null | od -An -tx1 | tr -d ' \\n')\nprintf '\\r\\nAX_BYTES:%s\\r\\n' \"$bytes\"\ncat >/dev/null",
        );
        let terminal = TerminalSession::spawn(
            &tokio::runtime::Handle::current(),
            path.to_string_lossy().into_owned(),
            Arc::new(|| {}),
        );
        let client = terminal.client();
        wait(&client, "copy-value").await;
        let ctx = egui::Context::default();
        let mut pane = TerminalPane::default();
        pane.configure_interaction(&AppearanceSettings {
            terminal_mouse_local_selection_priority: priority,
            ..Default::default()
        });
        warm(&ctx, &mut pane, &client).await;
        assert!(
            client
                .view()
                .unwrap()
                .terminal
                .mouse_button_reporting_active
        );
        let grid = pane.last_grid.unwrap();
        let a = grid.span(1, 1, 1).center();
        let b = grid.span(1, 4, 1).center();
        if cancelled {
            draw(
                &ctx,
                &mut pane,
                &client,
                pointer(a, egui::PointerButton::Primary, true, modifiers),
                modifiers,
            )
            .drop_without_applying_deltas();
            draw(
                &ctx,
                &mut pane,
                &client,
                vec![egui::Event::PointerGone],
                modifiers,
            )
            .drop_without_applying_deltas();
            draw(
                &ctx,
                &mut pane,
                &client,
                pointer(a, egui::PointerButton::Primary, false, modifiers),
                modifiers,
            )
            .drop_without_applying_deltas();
        }
        drag(&ctx, &mut pane, &client, a, b, modifiers).drop_without_applying_deltas();
        let local = modifiers.shift || priority && !modifiers.alt;
        if local {
            assert!(pane.selection.is_some());
            draw(
                &ctx,
                &mut pane,
                &client,
                pointer(a, egui::PointerButton::Primary, true, egui::Modifiers::NONE),
                egui::Modifiers::NONE,
            )
            .drop_without_applying_deltas();
            draw(
                &ctx,
                &mut pane,
                &client,
                pointer(
                    a,
                    egui::PointerButton::Primary,
                    false,
                    egui::Modifiers::NONE,
                ),
                egui::Modifiers::NONE,
            )
            .drop_without_applying_deltas();
        } else {
            assert!(pane.selection.is_none());
        }
        let end = if local { "32" } else { "35" };
        let code = if modifiers.alt { "38" } else { "30" };
        wait(
            &client,
            &format!("AX_BYTES:1b5b3c{code}3b323b324d1b5b3c{code}3b{end}3b326d"),
        )
        .await;
        terminal.shutdown().await.unwrap();
        std::fs::remove_file(path).unwrap();
    }
}

async fn copied(ctx: &egui::Context, pane: &mut TerminalPane, client: &TerminalClient) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let output = draw(ctx, pane, client, vec![], egui::Modifiers::NONE);
            let copied = output.platform_output.commands.iter().find_map(|command| {
                if let egui::OutputCommand::CopyText(text) = command {
                    Some(text.clone())
                } else {
                    None
                }
            });
            output.drop_without_applying_deltas();
            if let Some(text) = copied {
                break text;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn completed_selection_copies_and_right_click_paste_reaches_the_same_pty_once() {
    let path = script(
        "stty -echo\nprintf 'AX_READY\\ncopy-value\\n'\nwhile IFS= read -r line; do printf 'AX_PASTE:%s\\n' \"$line\"; done",
    );
    let terminal = TerminalSession::spawn(
        &tokio::runtime::Handle::current(),
        path.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = terminal.client();
    wait(&client, "copy-value").await;
    let ctx = egui::Context::default();
    let mut pane = TerminalPane::default();
    let mut appearance = AppearanceSettings {
        copy_selection_on_select: true,
        right_click_copy_or_paste: true,
        ..Default::default()
    };
    pane.configure_interaction(&appearance);
    warm(&ctx, &mut pane, &client).await;
    let grid = pane.last_grid.unwrap();
    let (a, b) = (grid.span(1, 0, 1).center(), grid.span(1, 3, 1).center());
    drag(&ctx, &mut pane, &client, a, b, egui::Modifiers::NONE).drop_without_applying_deltas();
    assert_eq!(copied(&ctx, &mut pane, &client).await, "copy");
    let output = draw(
        &ctx,
        &mut pane,
        &client,
        pointer(
            a,
            egui::PointerButton::Secondary,
            true,
            egui::Modifiers::NONE,
        ),
        egui::Modifiers::NONE,
    );
    assert!(output.viewport_output.values().any(|output| {
        output
            .commands
            .iter()
            .any(|cmd| matches!(cmd, egui::ViewportCommand::RequestPaste))
    }));
    output.drop_without_applying_deltas();
    assert!(pane.selection.is_none());
    let mut events = pointer(
        a,
        egui::PointerButton::Secondary,
        false,
        egui::Modifiers::NONE,
    );
    events.push(egui::Event::Paste("fixture\n".into()));
    draw(&ctx, &mut pane, &client, events, egui::Modifiers::NONE).drop_without_applying_deltas();
    wait(&client, "AX_PASTE:fixture").await;
    assert_eq!(text(&client).matches("AX_PASTE:fixture").count(), 1);
    appearance.copy_selection_on_select = false;
    pane.configure_interaction(&appearance);
    drag(&ctx, &mut pane, &client, a, b, egui::Modifiers::NONE).drop_without_applying_deltas();
    let output = draw(
        &ctx,
        &mut pane,
        &client,
        pointer(
            a,
            egui::PointerButton::Secondary,
            true,
            egui::Modifiers::NONE,
        ),
        egui::Modifiers::NONE,
    );
    assert!(!output.viewport_output.values().any(|output| {
        output
            .commands
            .iter()
            .any(|cmd| matches!(cmd, egui::ViewportCommand::RequestPaste))
    }));
    output.drop_without_applying_deltas();
    assert!(pane.selection.is_none());
    assert_eq!(copied(&ctx, &mut pane, &client).await, "copy");
    terminal.shutdown().await.unwrap();
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn owned_paste_is_dropped_before_an_editor_can_receive_it_across_passes() {
    let path = script("stty -echo\nprintf 'AX_READY\\n'\ncat >/dev/null");
    let terminal = TerminalSession::spawn(
        &tokio::runtime::Handle::current(),
        path.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = terminal.client();
    wait(&client, "AX_READY").await;
    let ctx = egui::Context::default();
    let mut pane = TerminalPane::default();
    warm(&ctx, &mut pane, &client).await;
    pane.request_paste(&ctx).unwrap();
    let id = egui::Id::unique("new-editor");
    ctx.memory_mut(|memory| memory.request_focus(id));
    let mut field = String::new();
    let mut passes = 0;
    let output = ctx.run_ui(
        egui::RawInput {
            focused: true,
            events: vec![egui::Event::Paste("fixture-terminal-only".into())],
            ..Default::default()
        },
        |ui| {
            crate::egui_app::clipboard::guard(ui.ctx(), |_| false);
            ui.add(egui::TextEdit::singleline(&mut field).id(id));
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("verify owned paste rejection");
            }
        },
    );
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    assert!(field.is_empty());
    let output = ctx.run_ui(
        egui::RawInput {
            focused: true,
            events: vec![egui::Event::Paste("normal-editor".into())],
            ..Default::default()
        },
        |ui| {
            crate::egui_app::clipboard::guard(ui.ctx(), |_| false);
            ui.add(egui::TextEdit::singleline(&mut field).id(id));
        },
    );
    output.drop_without_applying_deltas();
    assert_eq!(field, "normal-editor");
    pane.request_paste(&ctx).unwrap();
    let revision = client.view().unwrap().selection_revision;
    client.resize(TerminalSize::model(70, 20), 10, 20).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.view().unwrap().selection_revision == revision {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let output = ctx.run_ui(
        egui::RawInput {
            focused: true,
            events: vec![egui::Event::Paste("old-viewport".into())],
            ..Default::default()
        },
        |ui| {
            crate::egui_app::clipboard::guard(ui.ctx(), |owner| pane.owns_widget(owner));
            assert!(ui.input(|input| {
                input
                    .events
                    .iter()
                    .all(|event| !matches!(event, egui::Event::Paste(_)))
            }));
        },
    );
    output.drop_without_applying_deltas();
    terminal.shutdown().await.unwrap();
    std::fs::remove_file(path).unwrap();
}
