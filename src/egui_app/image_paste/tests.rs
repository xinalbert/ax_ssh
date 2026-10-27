use super::*;

#[test]
fn image_encoder_unmultiplies_alpha_and_rejects_oversized_or_malformed_pixels() {
    let color = egui::Color32::from_rgba_unmultiplied(120, 60, 30, 128);
    let image = egui::ColorImage::filled([2, 1], color);
    let decoded = image::load_from_memory(&encode(&image).unwrap())
        .unwrap()
        .into_rgba8();
    assert_eq!(&decoded.as_raw()[..4], &color.to_srgba_unmultiplied());
    assert!(encode(&egui::ColorImage::filled([0, 1], color)).is_err());
    let mut malformed = image;
    malformed.size = [4097, 1];
    assert!(encode(&malformed).is_err());
    malformed.size = [1, 1];
    assert!(encode(&malformed).is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn native_image_event_inserts_once_and_lost_focus_discards_completion() {
    use crate::egui_app::terminal::TerminalPane;
    use ax_ssh::controller::TerminalSession;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;
    for cancel in [false, true] {
        let path = std::env::temp_dir().join(format!("axssh-image-ui-{}.sh", Uuid::new_v4()));
        std::fs::write(&path, "#!/bin/sh\nstty -echo\nprintf 'IMAGE_READY\\n'\nIFS= read -r first\nprintf 'IMAGE_PATH:%s\\n' \"$first\"\nIFS= read -r second\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let terminal = TerminalSession::spawn(
            &tokio::runtime::Handle::current(),
            path.to_string_lossy().into_owned(),
            Arc::new(|| {}),
        );
        let client = terminal.client();
        wait(|| text(&client).contains("IMAGE_READY")).await;
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
        let (service, images) = Service::spawn(&tokio::runtime::Handle::current(), &ctx);
        images.install(&ctx);
        let mut pane = TerminalPane::default();
        let mut draw = |events, visible| {
            let view = client.view().unwrap();
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
                    images
                        .guard(ui.ctx(), |owner| visible && pane.owns_widget(owner))
                        .unwrap();
                    pane.show(ui, &client, &view).unwrap();
                    // Force a repeated pass to exercise once-per-frame image admission.
                    if ui.ctx().current_pass_index() == 0 {
                        ui.ctx().request_discard("image paste regression");
                    }
                },
            )
            .drop_without_applying_deltas();
        };
        draw(vec![], true);
        wait(|| client.view().unwrap().selection_revision != 0).await;
        draw(vec![], true);
        draw(
            vec![egui::Event::PasteImage(Arc::new(egui::ColorImage::filled(
                [1, 1],
                egui::Color32::RED,
            )))],
            true,
        );
        let token = ctx
            .data(|data| data.get_temp::<Pending>(key()))
            .unwrap()
            .token;
        if cancel {
            draw(vec![], false);
            wait(|| !images.busy.load(Ordering::Acquire)).await;
            assert!(ctx.data(|data| data.get_temp::<Pending>(key())).is_none());
            assert!(client.image_ready(token).unwrap().is_none());
            client.paste("ONLY_USER_TEXT".into()).unwrap();
        } else {
            tokio::time::timeout(Duration::from_secs(5), async {
                while client.image_ready(token).unwrap() != Some(true) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "prepare timeout: status={} revision={} ticket={:?} pending={:?}",
                    client.view().unwrap().status,
                    client.view().unwrap().selection_revision,
                    client.image_ready(token).unwrap(),
                    ctx.data(|data| data.get_temp::<Pending>(key()))
                        .map(|pending| (pending.current(), pending.revision))
                )
            });
            draw(vec![], true);
            // A new interaction after confirm is queued still revokes the same live ticket.
            wait(|| client.image_ready(token).unwrap().is_none()).await;
            draw(vec![], true);
        }
        client
            .key(ax_ssh::terminal::TerminalKey::Return, Default::default())
            .unwrap();
        wait(|| text(&client).contains("IMAGE_PATH:")).await;
        let received = text(&client);
        if cancel {
            assert!(received.contains("IMAGE_PATH:ONLY_USER_TEXT"));
        } else {
            assert_eq!(received.matches("input.png").count(), 1);
            assert!(received.contains("axssh-media-"));
        }
        service.shutdown().await.unwrap();
        terminal.shutdown().await.unwrap();
        std::fs::remove_file(path).unwrap();
    }

    async fn wait(mut predicate: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
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
}
