use super::*;
use ax_ssh::{
    config::{ConfigStore, SessionStore, WORKSPACE_SNAPSHOT_VERSION, WorkspaceSnapshot},
    controller::{Workspace, WorkspaceAction},
};
use std::{path::PathBuf, time::Duration};

async fn fixture(response: &str) -> (Workspace, TerminalClient, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!("axssh-osc52-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("fixture.sh");
    std::fs::write(
        &path,
        format!(
            r#"#!/bin/sh
stty -echo -icanon min 1
printf '\033[?2004h\033]52;c;SGVsbG8=\007\033]52;c;?\007'
hex=$(dd bs=1 count={} 2>/dev/null | od -An -v -tx1 | tr -d ' \n')
printf '\r\nOSC_RESPONSE:%s\r\n' "$hex"
IFS= read -r finish
"#,
            response.len()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let store = ConfigStore::new(directory.join("sessions.json"));
    let mut sessions = SessionStore::default();
    sessions.settings.terminal.osc52_clipboard = true;
    sessions.settings.terminal.local_shell = path.to_string_lossy().into_owned();
    store.save(&sessions).unwrap();
    store
        .save_workspace(&WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            ..Default::default()
        })
        .unwrap();
    let terminal = Workspace::spawn(&tokio::runtime::Handle::current(), store, Arc::new(|| {}));
    let workspace = terminal.client();
    wait(|| workspace.view().unwrap().ready).await;
    workspace.action(WorkspaceAction::NewLocal).unwrap();
    wait(|| !workspace.view().unwrap().tabs.is_empty()).await;
    let id = workspace.view().unwrap().tabs[0].panes[0].id;
    let client = workspace.terminal(id).unwrap().unwrap();
    wait(|| client.clipboard_read().unwrap().is_some()).await;
    (terminal, client, directory)
}

async fn wait(mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

fn frame(ctx: &egui::Context, service: &Client, owner: egui::Id, visible: bool) -> Result<()> {
    let mut result = Ok(());
    ctx.run_ui(
        egui::RawInput {
            focused: true,
            ..Default::default()
        },
        |ui| {
            ui.memory_mut(|memory| memory.request_focus(owner));
            result = service.guard(ui.ctx(), |_| visible);
        },
    )
    .drop_without_applying_deltas();
    result
}

#[tokio::test]
async fn explicitly_approved_reads_preserve_empty_text_crlf_and_send_once_as_raw_protocol() {
    use base64::Engine as _;
    for text in ["a\r\nb", ""] {
        let encoded = base64::engine::general_purpose::STANDARD.encode(text);
        let response = format!("\x1b]52;c;{encoded}\x07");
        let (terminal, client, path) = fixture(&response).await;
        let ctx = egui::Context::default();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = calls.clone();
        let (service, reader) = Service::with_reader(
            &tokio::runtime::Handle::current(),
            &ctx,
            Arc::new(move || {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(text.into())
            }),
        );
        reader.install(&ctx);
        let owner = egui::Id::unique("approved-terminal");
        let token = client.clipboard_read().unwrap().unwrap();
        // Merely displaying a remote request must never access native clipboard data.
        frame(&ctx, &reader, owner, true).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        request(&ctx, owner, &client, &client.view().unwrap(), token).unwrap();
        assert!(request(&ctx, owner, &client, &client.view().unwrap(), token).is_err());
        wait(|| reader.completed.lock().unwrap().is_some()).await;
        frame(&ctx, &reader, owner, true).unwrap();
        frame(&ctx, &reader, owner, true).unwrap();
        let hex: String = response.bytes().map(|byte| format!("{byte:02x}")).collect();
        wait(|| {
            client
                .view()
                .unwrap()
                .terminal
                .lines
                .iter()
                .flat_map(|line| &line.runs)
                .map(|run| run.text.as_str())
                .collect::<String>()
                .contains(&format!("OSC_RESPONSE:{hex}"))
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(client.clipboard_read().unwrap().is_none());
        assert!(request(&ctx, owner, &client, &client.view().unwrap(), token).is_err());
        service.shutdown().await.unwrap();
        terminal.shutdown().await.unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[tokio::test]
async fn cancelled_native_reads_discard_late_contents_and_shutdown_joins_the_blocking_job() {
    let (terminal, client, path) = fixture("no response").await;
    let ctx = egui::Context::default();
    let (entered, started) = tokio::sync::oneshot::channel();
    let entered = Mutex::new(Some(entered));
    let (release, blocked) = std::sync::mpsc::sync_channel(1);
    let blocked = Mutex::new(blocked);
    let (service, reader) = Service::with_reader(
        &tokio::runtime::Handle::current(),
        &ctx,
        Arc::new(move || {
            entered.lock().unwrap().take().unwrap().send(()).unwrap();
            blocked.lock().unwrap().recv().unwrap();
            Ok("private late contents".into())
        }),
    );
    reader.install(&ctx);
    let owner = egui::Id::unique("cancelled-terminal");
    request(
        &ctx,
        owner,
        &client,
        &client.view().unwrap(),
        client.clipboard_read().unwrap().unwrap(),
    )
    .unwrap();
    started.await.unwrap();
    frame(&ctx, &reader, owner, false).unwrap();
    frame(&ctx, &reader, owner, false).unwrap();
    wait(|| client.clipboard_read().unwrap().is_none()).await;
    let shutdown = tokio::spawn(service.shutdown());
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    release.send(()).unwrap();
    shutdown.await.unwrap().unwrap();
    assert!(reader.completed.lock().unwrap().is_none());
    terminal.shutdown().await.unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn oversize_or_failed_native_reads_deny_without_returning_contents() {
    for oversize in [false, true] {
        let (terminal, client, path) = fixture("no response").await;
        let ctx = egui::Context::default();
        let (service, reader) = Service::with_reader(
            &tokio::runtime::Handle::current(),
            &ctx,
            Arc::new(move || {
                if oversize {
                    Ok("x".repeat(ax_ssh::terminal::MAX_OSC52_CLIPBOARD_BYTES + 1))
                } else {
                    anyhow::bail!("fixture clipboard unavailable")
                }
            }),
        );
        reader.install(&ctx);
        let owner = egui::Id::unique("failed-terminal");
        request(
            &ctx,
            owner,
            &client,
            &client.view().unwrap(),
            client.clipboard_read().unwrap().unwrap(),
        )
        .unwrap();
        wait(|| reader.completed.lock().unwrap().is_some()).await;
        frame(&ctx, &reader, owner, true).unwrap();
        assert!(frame(&ctx, &reader, owner, true).is_err());
        wait(|| client.clipboard_read().unwrap().is_none()).await;
        assert!(ctx.data(|data| data.get_temp::<Pending>(key())).is_none());
        service.shutdown().await.unwrap();
        terminal.shutdown().await.unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[tokio::test]
async fn allow_button_admits_only_one_read_across_repeated_ui_passes() {
    use crate::egui_app::terminal::TerminalPane;
    let (terminal, client, path) = fixture("\x1b]52;c;\x07").await;
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let (service, reader) = Service::with_reader(
        &tokio::runtime::Handle::current(),
        &ctx,
        Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }),
    );
    reader.install(&ctx);
    let mut pane = TerminalPane::default();
    let mut draw = |events, repeat| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                reader.guard(ui.ctx(), |_| true).unwrap();
                let view = client.view().unwrap();
                pane.remote_clipboard(ui, &client, &view, true).unwrap();
                pane.show(ui, &client, &view).unwrap();
                if repeat && ui.ctx().current_pass_index() == 0 {
                    ui.ctx().request_discard("clipboard pass fixture");
                }
            },
        )
    };
    draw(vec![], false).drop_without_applying_deltas();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let output = draw(vec![], false);
    fn find(shape: &egui::epaint::Shape) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == "Allow" => {
                Some(text.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(find),
            _ => None,
        }
    }
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape))
        .unwrap();
    output.drop_without_applying_deltas();
    for pressed in [true, false] {
        draw(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
        )
        .drop_without_applying_deltas();
    }
    assert!(
        ctx.data(|data| data.get_temp::<Pending>(key())).is_some(),
        "Allow did not retain a pending read"
    );
    wait(|| reader.completed.lock().unwrap().is_some()).await;
    draw(vec![], true).drop_without_applying_deltas();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    wait(|| client.clipboard_read().unwrap().is_none()).await;
    service.shutdown().await.unwrap();
    terminal.shutdown().await.unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
