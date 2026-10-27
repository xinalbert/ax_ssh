use super::*;
use ax_ssh::controller::TerminalSession;
use std::sync::Arc;

#[tokio::test]
async fn semantic_preview_recolors_published_ranges_without_changing_grid_or_program_styles() {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("axssh-semantic-{}.sh", uuid::Uuid::new_v4()));
    std::fs::write(&path, "#!/bin/sh\nstty -echo\nprintf '\\033[4;9;58;2;90;100;110mINFO WARN ERROR HTTP/1.1 200 bare=404\\033[0m\\r\\n\\033[31mERROR explicit\\033[0m\\r\\n\\033[7mERROR inverse\\033[0m\\r\\n\\033[2mERROR faint\\033[0m\\r\\n\\033[8mERROR hidden\\033[0m\\r\\nREADY'\nIFS= read -r ignored\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let session = TerminalSession::spawn(
        &tokio::runtime::Handle::current(),
        path.to_string_lossy().into_owned(),
        Arc::new(|| {}),
    );
    let client = session.client();
    let view = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let view = client.view().unwrap();
            if view
                .terminal
                .lines
                .iter()
                .flat_map(|line| &line.runs)
                .any(|run| run.text.contains("READY"))
            {
                break view;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::egui_app::fonts::load().unwrap());
    let palette = [Color32::from_gray(200), Color32::from_gray(20)];
    let mut cache = Vec::new();
    let mut draw = |settings: &AppearanceSettings| {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let grid = Grid::new(ui.max_rect(), egui::vec2(10.0, 20.0), 1.0);
            rows(
                ui.painter(),
                &view,
                &grid,
                &mut cache,
                15.0,
                1.0,
                Colors::new(palette, Some(settings)),
            );
        })
        .drop_without_applying_deltas();
        cache
            .iter()
            .flat_map(|row| {
                row.runs
                    .iter()
                    .map(|run| (run.column, run.cells, run.background, run.text.clone()))
            })
            .collect::<Vec<_>>()
    };
    let saved = AppearanceSettings::default();
    let baseline = draw(&saved);
    let mut draft = saved.clone();
    draft.terminal_semantic_highlighting = true;
    draft.terminal_semantic_colors.info = " #112233 ".into();
    draft.terminal_semantic_colors.warning = "#445566".into();
    draft.terminal_semantic_colors.error = "#778899".into();
    draft.terminal_semantic_colors.success = "#AABBCC".into();
    for percent in [100, 60, 120] {
        draft.terminal_text_brightness_percent = percent;
        let highlighted = draw(&draft);
        let settings = Colors::new(palette, Some(&draft));
        assert_eq!(highlighted.len(), baseline.len());
        for (before, after) in baseline.iter().zip(&highlighted) {
            assert_eq!((before.0, before.1, before.2), (after.0, after.1, after.2));
            assert_eq!(before.3.text, after.3.text);
            assert_eq!(before.3.sections.len(), after.3.sections.len());
            for (before, after) in before.3.sections.iter().zip(&after.3.sections) {
                assert_eq!(before.byte_range, after.byte_range);
                assert_eq!(before.leading_space, after.leading_space);
                assert_eq!(before.format.font_id, after.format.font_id);
            }
        }
        let run = highlighted
            .iter()
            .find(|run| run.3.text.contains("INFO WARN"))
            .unwrap();
        for (word, class) in [
            ("INFO", SemanticHighlight::Info),
            ("WARN", SemanticHighlight::Warning),
            ("ERROR", SemanticHighlight::Error),
            ("200", SemanticHighlight::Success),
        ] {
            let start = run.3.text.find(word).unwrap();
            for byte in start..start + word.len() {
                let format = run.3.format_at_byte(egui::text::ByteIndex(byte));
                assert_eq!(format.color, settings.semantic(class), "word={word}");
                assert_eq!(
                    format.underline.color,
                    settings.adjust(Color32::from_rgb(90, 100, 110), false)
                );
                assert_eq!(
                    format.strikethrough.color,
                    settings.adjust(palette[0], false)
                );
            }
        }
        let bare = run.3.text.find("404").unwrap();
        assert_eq!(
            run.3.format_at_byte(egui::text::ByteIndex(bare)).color,
            settings.adjust(palette[0], false)
        );
        let mut off = draft.clone();
        off.terminal_semantic_highlighting = false;
        let plain = draw(&off);
        for (before, after) in plain.iter().zip(&highlighted).filter(|(_, run)| {
            run.3.text.contains("explicit")
                || run.3.text.contains("inverse")
                || run.3.text.contains("faint")
                || run.3.text.contains("hidden")
        }) {
            assert_eq!(before.3, after.3);
        }
    }
    draft.terminal_semantic_colors.error = "#12345680".into();
    assert_eq!(Colors::new(palette, Some(&draft)).semantic_colors[3], None);
    let restored = draw(&saved);
    for (before, after) in baseline.iter().zip(&restored) {
        assert_eq!(before, after);
    }
    let current = client.view().unwrap();
    assert_eq!(current.generation, view.generation);
    assert_eq!(current.selection_revision, view.selection_revision);
    for (before, after) in view.terminal.lines.iter().zip(&current.terminal.lines) {
        assert!(Arc::ptr_eq(before, after));
    }
    session.shutdown().await.unwrap();
    std::fs::remove_file(path).unwrap();
}
