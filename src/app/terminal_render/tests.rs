use std::sync::Arc;

use super::*;

#[test]
fn terminal_symbols_choose_text_fonts_but_explicit_emoji_keeps_its_presentation() {
    for (text, role) in [
        ("⏺", FontRole::Symbol),
        ("⏺\u{fe0e}", FontRole::Symbol),
        ("🖥\u{fe0e}", FontRole::Symbol),
        ("⏺\u{fe0f}", FontRole::Emoji),
        ("✈\u{fe0f}", FontRole::Emoji),
        ("😀", FontRole::Emoji),
        ("⌚", FontRole::Emoji),
        ("✅", FontRole::Emoji),
        ("⚡", FontRole::Emoji),
        ("✅\u{fe0e}", FontRole::Symbol),
        ("1\u{fe0f}\u{20e3}", FontRole::Emoji),
        ("👩\u{200d}💻", FontRole::Emoji),
        ("👩🏽\u{200d}💻", FontRole::Emoji),
        ("👍🏽", FontRole::Emoji),
        ("🇨🇳", FontRole::Emoji),
        ("e\u{301}", FontRole::Primary),
        ("中\u{301}", FontRole::Primary),
    ] {
        let mut terminal = ax_ssh::terminal::TerminalModel::new(80, 3, 10);
        terminal.process(text.as_bytes());
        let source = terminal.snapshot();
        let column = source.cursor_column;
        let rendered = render_terminal(source, settings());
        let run = &rendered.lines[0].runs[0];
        assert_eq!(run.font_role, role, "{text:?}");
        assert_eq!(
            run.text, text,
            "presentation must keep each composition together"
        );
        assert!(
            terminal.contents().starts_with(text),
            "rendering must not rewrite source Unicode"
        );
        assert_eq!(terminal.snapshot().cursor_column, column);
        if text.chars().count() > 1 {
            assert!(!run.fixed_spacing);
        }
    }
}

#[test]
fn compatible_symbol_runs_batch_without_crossing_style_or_combining_boundaries() {
    let mut terminal = ax_ssh::terminal::TerminalModel::new(80, 3, 10);
    terminal.process("←↑→\x1b[31m↓\x1b[0me\u{301}中⏺\u{fe0f}".as_bytes());
    let before = terminal.snapshot();
    let expected_column = before.cursor_column;
    let rendered = render_terminal(before, settings());
    let runs = &rendered.lines[0].runs;
    assert_eq!(runs[0].text, "←↑→");
    assert_eq!((runs[0].column, runs[0].cells), (0, 3));
    assert!(runs[0].fixed_spacing);
    assert_eq!(runs[1].text, "↓");
    assert_eq!(runs[1].column, 3);
    assert_eq!(runs[2].text, "e\u{301}");
    assert!(!runs[2].fixed_spacing);
    assert_eq!(runs[3].text, "中");
    assert_eq!(runs[3].cells, 2);
    assert_eq!(runs[4].font_role, FontRole::Emoji);
    assert_eq!(terminal.snapshot().cursor_column, expected_column);
}

#[test]
fn text_symbol_fallback_keeps_program_colors_and_original_characters() {
    let mut terminal = ax_ssh::terminal::TerminalModel::new(80, 3, 10);
    let text = "↳⎿↳✳✻·✻✽✻✶·";
    terminal.process(format!("{text}\x1b[38;2;225;40;60m⏺\x1b[38;2;30;210;80m⏺\x1b[0m").as_bytes());
    let before = terminal.snapshot();
    let cursor = before.cursor_column;
    let copied = terminal.selection_text(0, 0, 0, cursor);
    let rendered = render_terminal(before, settings());
    let runs = &rendered.lines[0].runs;
    let circles = runs
        .iter()
        .filter(|run| run.text == "⏺")
        .collect::<Vec<_>>();
    assert_eq!(circles.len(), 2);
    assert_eq!(circles[0].foreground, RgbColor::new(225, 40, 60));
    assert_eq!(circles[1].foreground, RgbColor::new(30, 210, 80));
    assert!(circles.iter().all(|run| run.font_role == FontRole::Symbol));
    assert!(runs.iter().all(|run| run.geometry.is_empty()));
    let joined = runs.iter().map(|run| run.text.as_str()).collect::<String>();
    assert_eq!(joined.trim_end(), format!("{text}⏺⏺"));
    assert_eq!(terminal.snapshot().cursor_column, cursor);
    assert_eq!(terminal.selection_text(0, 0, 0, cursor), copied);
}

#[test]
fn terminal_geometry_preserves_text_colors_and_protocol_cell_positions() {
    let mut terminal = ax_ssh::terminal::TerminalModel::new(80, 3, 10);
    let text = "┌─┬┐\r\n│▁█▉░▒▓▖\u{e0b0}\u{e0b1}中";
    terminal.process(text.as_bytes());
    let copy = terminal.selection_text(0, 0, 1, 12);
    let before = terminal.snapshot();
    let position = (before.cursor_row, before.cursor_column);
    let rendered = render_terminal(before, settings());
    assert!(!rendered.lines[0].runs[0].geometry.light.is_empty());
    assert_eq!(rendered.lines[0].runs[0].text, "┌─┬┐");
    assert!(
        rendered.lines[1]
            .runs
            .iter()
            .any(|run| run.text.contains('\u{e0b0}') && !run.geometry.fill.is_empty())
    );
    let han = rendered.lines[1]
        .runs
        .iter()
        .find(|run| run.text == "中")
        .unwrap();
    assert_eq!((han.column, han.cells), (10, 2));
    assert!(han.geometry.is_empty());
    assert_eq!(terminal.selection_text(0, 0, 1, 12), copy);
    let after = terminal.snapshot();
    assert_eq!((after.cursor_row, after.cursor_column), position);

    terminal.process("\r\x1b[2K\x1b[31;7m█\x1b[0m".as_bytes());
    let rendered = render_terminal(terminal.snapshot(), settings());
    let run = &rendered.lines[1].runs[0];
    assert!(!run.geometry.fill.is_empty());
    assert_eq!(run.foreground, settings().default_background);
    assert_ne!(run.background, settings().default_background);
}

#[test]
fn geometry_uses_exact_eighths_and_keeps_unimplemented_glyphs_on_the_font_path() {
    let mut paths = geometry::GlyphPaths::default();
    assert!(paths.append('▁', 0));
    assert!(paths.fill.contains("M 0 112 h 128 v 16"));
    let mut paths = geometry::GlyphPaths::default();
    assert!(paths.append('▉', 1));
    assert!(paths.fill.contains("M 128 0 h 112 v 128"));
    let mut paths = geometry::GlyphPaths::default();
    assert!(paths.append('┌', 0));
    assert!(paths.light.contains("L 128 64"));
    assert!(paths.light.contains("L 64 128"));
    assert!(!paths.append('╔', 1));
    assert!(!paths.append('═', 1));
    assert!(!paths.append('中', 1));
    assert!(!paths.append('😀', 1));
}

fn snapshot(style: TerminalStyle) -> TerminalSnapshot {
    TerminalSnapshot {
        lines: vec![Arc::new(TerminalStyledLine {
            revision: 1,
            runs: vec![TerminalStyledRun {
                text: "x".into(),
                column: 0,
                cells: 1,
                style,
                hyperlink: None,
            }],
        })],
        dirty_rows: vec![0],
        full_refresh: true,
        max_columns: 1,
        cursor_row: 0,
        cursor_column: 1,
        cursor_cells: 1,
        cursor_visible: true,
        cursor_shape: ax_ssh::terminal::TerminalCursorShape::Block,
        cursor_blinking: false,
        foreground_color: TerminalColor::Default,
        background_color: TerminalColor::Default,
        cursor_color: TerminalColor::Default,
        cursor_text: " ".into(),
        display_offset: 0,
        viewport_mode: ax_ssh::terminal::TerminalViewportMode::Follow,
        mouse_reporting: Default::default(),
        mouse_button_reporting_active: false,
        mouse_wheel_reporting_active: false,
        bell_revision: 0,
    }
}

fn settings() -> TerminalRenderSettings {
    TerminalRenderSettings {
        color_scheme: TerminalColorScheme::Dark,
        default_foreground: RgbColor::new(204, 204, 204),
        default_background: RgbColor::new(30, 30, 30),
        selection_background: RgbColor::new(38, 79, 120),
        text_brightness: 1.0,
        bright_bold_text: true,
        semantic_highlighting: false,
        semantic_colors: SemanticColorOverrides::default(),
    }
}

fn semantic_settings() -> TerminalRenderSettings {
    TerminalRenderSettings {
        semantic_highlighting: true,
        ..settings()
    }
}

#[test]
fn inverse_space_cursors_survive_parsing_and_rendering_with_the_native_cursor_hidden() {
    for (prefix, column) in [("", 0), ("dfa1", 4), ("中文", 4)] {
        let mut terminal = ax_ssh::terminal::TerminalModel::new(20, 3, 10);
        terminal.process(format!("\x1b[?25l{prefix}\x1b[7m \x1b[27m").as_bytes());
        let source = terminal.snapshot();
        assert!(!source.cursor_visible);
        let rendered = render_terminal(source, settings());
        assert_eq!(
            rendered.lines[0].backgrounds,
            vec![RenderedTerminalBackgroundRun {
                column,
                cells: 1,
                background: settings().default_foreground,
            }],
            "the inverse space after {prefix:?} must remain a visible block"
        );

        terminal.process(b"\r\x1b[2K\x1b[7mX\x1b[27m");
        let occupied = render_terminal(terminal.snapshot(), settings());
        assert_eq!(occupied.lines[0].backgrounds[0].column, 0);
        terminal.process(b"\r\x1b[2K\x1b[7m \x1b[27m");
        let blank = terminal.snapshot();
        assert!(blank.dirty_rows.contains(&0));
        assert_eq!(
            render_terminal(blank, settings()).lines[0].backgrounds,
            occupied.lines[0].backgrounds,
            "moving from an inverse letter to an inverse space keeps its background"
        );

        terminal.process(b"\r\x1b[2K");
        let cleared = render_terminal(terminal.snapshot(), settings());
        assert!(cleared.lines[0].backgrounds.is_empty());
        assert!(
            cleared.lines[0].runs.is_empty(),
            "plain blank rows stay compact"
        );
    }
}

#[test]
fn terminal_local_default_background_overrides_the_theme_surface() {
    let mut source = snapshot(TerminalStyle::default());
    source.background_color = TerminalColor::Rgb {
        red: 0x65,
        green: 0x43,
        blue: 0x21,
    };

    let rendered = render_terminal(source, settings());
    assert_eq!(rendered.background, RgbColor::new(0x65, 0x43, 0x21));
}

#[test]
fn sgr_underline_color_overrides_the_text_foreground_for_decorations() {
    let rendered = render_terminal(
        snapshot(TerminalStyle {
            underline: true,
            underline_style: TerminalUnderlineStyle::Curly,
            underline_color: Some(TerminalColor::Rgb {
                red: 1,
                green: 2,
                blue: 3,
            }),
            ..TerminalStyle::default()
        }),
        settings(),
    );

    assert_eq!(
        rendered.lines[0].decorations[0].foreground,
        RgbColor::new(1, 2, 3)
    );
}

fn snapshot_line(runs: Vec<TerminalStyledRun>) -> TerminalSnapshot {
    TerminalSnapshot {
        lines: vec![Arc::new(TerminalStyledLine { revision: 1, runs })],
        dirty_rows: vec![0],
        full_refresh: true,
        max_columns: 128,
        cursor_row: 0,
        cursor_column: 0,
        cursor_cells: 1,
        cursor_visible: false,
        cursor_shape: ax_ssh::terminal::TerminalCursorShape::Block,
        cursor_blinking: false,
        foreground_color: TerminalColor::Default,
        background_color: TerminalColor::Default,
        cursor_color: TerminalColor::Default,
        cursor_text: String::new(),
        display_offset: 0,
        viewport_mode: ax_ssh::terminal::TerminalViewportMode::Follow,
        mouse_reporting: Default::default(),
        mouse_button_reporting_active: false,
        mouse_wheel_reporting_active: false,
        bell_revision: 0,
    }
}

#[test]
fn non_ascii_runs_preserve_terminal_cell_spans() {
    let rendered = render_terminal(
        snapshot_line(vec![
            TerminalStyledRun {
                text: "中".into(),
                column: 0,
                cells: 2,
                style: TerminalStyle::default(),
                hyperlink: None,
            },
            TerminalStyledRun {
                text: "┌".into(),
                column: 2,
                cells: 1,
                style: TerminalStyle::default(),
                hyperlink: None,
            },
            plain_run("A", 3),
        ]),
        settings(),
    );
    let runs = &rendered.lines[0].runs;

    assert_eq!(
        runs.iter()
            .map(|run| (run.text.as_str(), run.column, run.cells))
            .collect::<Vec<_>>(),
        vec![("中", 0, 2), ("┌", 2, 1), ("A", 3, 1)],
    );
}

#[test]
fn compact_spans_omit_default_backgrounds_and_merge_adjacent_decorations() {
    let default_background = settings().default_background;
    let accent_background = RgbColor::new(80, 30, 20);
    let foreground = settings().default_foreground;
    let run = |column, background, underline, strikethrough| RenderedTerminalRun {
        text: "x".into(),
        hyperlink: None,
        column,
        cells: 1,
        foreground,
        background,
        decoration_foreground: foreground,
        bold: false,
        italic: false,
        underline,
        underline_style: if underline {
            TerminalUnderlineStyle::Single
        } else {
            TerminalUnderlineStyle::None
        },
        strikethrough,
        font_role: FontRole::Primary,
        fixed_spacing: false,
        geometry: Default::default(),
    };
    let runs = [
        run(0, default_background, false, false),
        run(1, accent_background, true, false),
        run(2, accent_background, true, false),
        run(4, accent_background, false, true),
    ];

    assert_eq!(
        merged_background_runs(&runs, default_background),
        vec![
            RenderedTerminalBackgroundRun {
                column: 1,
                cells: 2,
                background: accent_background,
            },
            RenderedTerminalBackgroundRun {
                column: 4,
                cells: 1,
                background: accent_background,
            },
        ]
    );
    assert_eq!(
        compact_decoration_runs(&runs),
        vec![
            RenderedTerminalDecorationRun {
                column: 1,
                cells: 2,
                foreground,
                underline_style: TerminalUnderlineStyle::Single,
                strikethrough: false,
            },
            RenderedTerminalDecorationRun {
                column: 4,
                cells: 1,
                foreground,
                underline_style: TerminalUnderlineStyle::None,
                strikethrough: true,
            },
        ]
    );
}

fn plain_run(text: &str, column: usize) -> TerminalStyledRun {
    TerminalStyledRun {
        text: text.to_owned(),
        column,
        cells: text.len(),
        style: TerminalStyle::default(),
        hyperlink: None,
    }
}

#[test]
fn semantic_highlights_cover_standard_severities_and_contextual_http() {
    let text = "INFO HTTP/1.1 200 OK https://example.test HTTP/2 302 http_status=404 WARN HTTP/3 503 ERROR /srv/log";
    let rendered = render_terminal(snapshot_line(vec![plain_run(text, 0)]), semantic_settings());
    let runs = &rendered.lines[0].runs;
    let run_for = |text: &str| {
        runs.iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("missing semantic run {text:?}"))
    };
    let run_containing = |text: &str| {
        runs.iter()
            .find(|run| run.text.contains(text))
            .unwrap_or_else(|| panic!("missing semantic text {text:?}"))
    };
    let palette = SemanticPalette::for_terminal(
        &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
        SemanticColorOverrides::default(),
    );

    assert_eq!(
        run_containing("https://example.test").foreground,
        settings().default_foreground
    );
    assert_eq!(
        run_containing("/srv/log").foreground,
        settings().default_foreground
    );
    assert_eq!(run_for("INFO").foreground, palette.info);
    assert_eq!(run_for("200").foreground, palette.success);
    assert_eq!(
        run_containing("OK").foreground,
        settings().default_foreground
    );
    assert_eq!(run_for("302").foreground, palette.info);
    assert_eq!(run_for("404").foreground, palette.warning);
    assert_eq!(run_for("WARN").foreground, palette.warning);
    assert_eq!(run_for("503").foreground, palette.error);
    assert_eq!(run_for("ERROR").foreground, palette.error);
}

#[test]
fn semantic_highlights_apply_to_real_default_terminal_cells() {
    let mut terminal = ax_ssh::terminal::TerminalModel::new(128, 3, 10);
    terminal.process(b"INFO HTTP/1.1 200 OK ERROR https://example.test /srv/log");
    let snapshot = terminal.snapshot();
    let source_runs = &snapshot.lines[0].runs;
    assert_eq!(source_runs.len(), 1);
    assert_eq!(source_runs[0].style, TerminalStyle::default());

    let rendered = render_terminal(snapshot, semantic_settings());
    let runs = &rendered.lines[0].runs;
    let run_for = |text: &str| {
        runs.iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("missing rendered run {text:?}"))
    };
    let run_containing = |text: &str| {
        runs.iter()
            .find(|run| run.text.contains(text))
            .unwrap_or_else(|| panic!("missing rendered text {text:?}"))
    };
    let palette = SemanticPalette::for_terminal(
        &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
        SemanticColorOverrides::default(),
    );

    assert_eq!(run_for("INFO").foreground, palette.info);
    assert_eq!(run_for("200").foreground, palette.success);
    assert_eq!(
        run_containing("OK").foreground,
        settings().default_foreground
    );
    assert_eq!(run_for("ERROR").foreground, palette.error);
    assert_eq!(
        run_containing("https://example.test").foreground,
        settings().default_foreground
    );
    assert_eq!(
        run_containing("/srv/log").foreground,
        settings().default_foreground
    );
}

#[test]
fn semantic_highlights_are_boundary_aware_and_preserve_ansi_runs() {
    let text = "terror error 2000 200 HTTP/1.1 200";
    let ansi_style = TerminalStyle {
        foreground: TerminalColor::Indexed(4),
        ..TerminalStyle::default()
    };
    let rendered = render_terminal(
        snapshot_line(vec![
            plain_run(text, 0),
            TerminalStyledRun {
                text: " ERROR".to_owned(),
                column: text.len(),
                cells: 6,
                style: ansi_style,
                hyperlink: None,
            },
        ]),
        semantic_settings(),
    );
    let runs = &rendered.lines[0].runs;
    let terror = runs
        .iter()
        .find(|run| run.text.starts_with("terror"))
        .unwrap();
    let error = runs.iter().find(|run| run.text == "error").unwrap();
    let number = runs
        .iter()
        .find(|run| run.text.contains("2000 200"))
        .unwrap();
    let status = runs.iter().find(|run| run.text == "200").unwrap();
    let ansi = runs.iter().find(|run| run.text == " ERROR").unwrap();

    assert_eq!(terror.foreground, settings().default_foreground);
    assert_eq!(
        error.foreground,
        SemanticPalette::for_terminal(
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            SemanticColorOverrides::default(),
        )
        .error
    );
    assert_eq!(number.foreground, settings().default_foreground);
    assert_eq!(
        status.foreground,
        SemanticPalette::for_terminal(
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            SemanticColorOverrides::default(),
        )
        .success
    );
    assert_eq!(
        ansi.foreground,
        resolve_style_colors(
            ansi_style,
            &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            &settings(),
        )
        .0
    );
}

#[test]
fn semantic_highlighting_is_disabled_by_default() {
    let rendered = render_terminal(
        snapshot_line(vec![plain_run("INFO ERROR https://example.test", 0)]),
        settings(),
    );

    assert_eq!(rendered.lines[0].runs.len(), 1);
    assert_eq!(
        rendered.lines[0].runs[0].foreground,
        settings().default_foreground
    );
}

#[test]
fn semantic_highlighting_preserves_explicit_and_special_terminal_styles() {
    for style in [
        TerminalStyle {
            foreground: TerminalColor::Indexed(1),
            ..TerminalStyle::default()
        },
        TerminalStyle {
            foreground: TerminalColor::Indexed(196),
            ..TerminalStyle::default()
        },
        TerminalStyle {
            foreground: TerminalColor::Rgb {
                red: 12,
                green: 34,
                blue: 56,
            },
            ..TerminalStyle::default()
        },
        TerminalStyle {
            background: TerminalColor::Indexed(4),
            ..TerminalStyle::default()
        },
        TerminalStyle {
            inverse: true,
            ..TerminalStyle::default()
        },
        TerminalStyle {
            dim: true,
            ..TerminalStyle::default()
        },
        TerminalStyle {
            hidden: true,
            ..TerminalStyle::default()
        },
    ] {
        let source = TerminalStyledRun {
            style,
            ..plain_run("ERROR HTTP/1.1 200", 7)
        };
        let baseline = render_terminal(snapshot_line(vec![source.clone()]), settings());
        let rendered = render_terminal(snapshot_line(vec![source]), semantic_settings());
        assert_eq!(rendered.lines[0].runs.len(), 1, "{style:?}");
        assert_eq!(
            rendered.lines[0].runs[0].foreground, baseline.lines[0].runs[0].foreground,
            "{style:?}"
        );
        assert_eq!(
            rendered.lines[0].runs[0].background, baseline.lines[0].runs[0].background,
            "{style:?}"
        );
    }
}

#[test]
fn semantic_highlighting_keeps_bounded_ascii_cell_geometry() {
    let text = "prefix ERROR HTTP/1.1 200 suffix";
    let rendered = render_terminal(snapshot_line(vec![plain_run(text, 7)]), semantic_settings());
    let runs = &rendered.lines[0].runs;
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        text
    );
    assert_eq!(runs.iter().map(|run| run.cells).sum::<usize>(), text.len());
    let mut column = 7;
    for run in runs {
        assert_eq!(run.column, column);
        column += run.cells;
    }

    let within_limit = format!("{} ERROR", "x".repeat(MAX_SEMANTIC_HIGHLIGHT_BYTES - 6));
    assert!(
        !semantic_highlight_spans(&within_limit, within_limit.len(), TerminalStyle::default())
            .is_empty()
    );
    let beyond_limit = format!("{within_limit} ");
    assert!(
        semantic_highlight_spans(&beyond_limit, beyond_limit.len(), TerminalStyle::default())
            .is_empty()
    );
    assert!(semantic_highlight_spans("中文 ERROR", 9, TerminalStyle::default()).is_empty());
    assert!(semantic_highlight_spans("ERROR", 4, TerminalStyle::default()).is_empty());
}

#[test]
fn configured_semantic_colors_are_brightened_after_selection() {
    let overrides = SemanticColorOverrides {
        success: Some(RgbColor::new(37, 211, 139)),
        info: Some(RgbColor::new(116, 177, 255)),
        warning: Some(RgbColor::new(255, 210, 77)),
        error: Some(RgbColor::new(255, 114, 114)),
    };
    let rendered = render_terminal(
        snapshot_line(vec![plain_run(
            "INFO HTTP/1.1 200 OK https://example.test HTTP/2 302 http_status=404 WARN HTTP/3 503 ERROR /srv/log",
            0,
        )]),
        TerminalRenderSettings {
            text_brightness: 1.20,
            semantic_highlighting: true,
            semantic_colors: overrides,
            ..settings()
        },
    );
    let runs = &rendered.lines[0].runs;
    let run_for = |text: &str| {
        runs.iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("missing semantic run {text:?}"))
    };
    let run_containing = |text: &str| {
        runs.iter()
            .find(|run| run.text.contains(text))
            .unwrap_or_else(|| panic!("missing semantic text {text:?}"))
    };

    assert_eq!(
        run_containing("https://example.test").foreground,
        adjust_text_foreground(settings().default_foreground, 1.20, false)
    );
    assert_eq!(
        run_for("INFO").foreground,
        adjust_text_foreground(overrides.info.unwrap(), 1.20, false)
    );
    assert_eq!(
        run_for("200").foreground,
        adjust_text_foreground(overrides.success.unwrap(), 1.20, false)
    );
    assert_eq!(
        run_for("302").foreground,
        adjust_text_foreground(overrides.info.unwrap(), 1.20, false)
    );
    assert_eq!(
        run_for("WARN").foreground,
        adjust_text_foreground(overrides.warning.unwrap(), 1.20, false)
    );
    assert_eq!(
        run_for("ERROR").foreground,
        adjust_text_foreground(overrides.error.unwrap(), 1.20, false)
    );
}

#[test]
fn bold_standard_colors_use_bright_palette_when_enabled() {
    let rendered = render_terminal(
        snapshot(TerminalStyle {
            foreground: TerminalColor::Indexed(1),
            bold: true,
            ..TerminalStyle::default()
        }),
        settings(),
    );

    assert_eq!(
        rendered.lines[0].runs[0].foreground,
        RgbColor::new(255, 92, 87)
    );
}

#[test]
fn resolves_256_color_cube_and_inverse_background() {
    let rendered = render_terminal(
        snapshot(TerminalStyle {
            foreground: TerminalColor::Indexed(208),
            background: TerminalColor::Indexed(21),
            inverse: true,
            ..TerminalStyle::default()
        }),
        settings(),
    );

    assert_eq!(
        rendered.lines[0].runs[0].foreground,
        RgbColor::new(0, 0, 255)
    );
    assert_eq!(
        rendered.lines[0].runs[0].background,
        RgbColor::new(255, 135, 0)
    );
}

#[test]
fn theme_defaults_override_only_terminal_defaults() {
    let mut themed = settings();
    themed.default_foreground = RgbColor::new(170, 187, 204);
    themed.default_background = RgbColor::new(16, 32, 48);
    themed.selection_background = RgbColor::new(48, 64, 80);
    let default_rendered = render_terminal(snapshot(TerminalStyle::default()), themed);
    let indexed_rendered = render_terminal(
        snapshot(TerminalStyle {
            foreground: TerminalColor::Indexed(1),
            ..TerminalStyle::default()
        }),
        themed,
    );

    assert_eq!(default_rendered.foreground, RgbColor::new(170, 187, 204));
    assert_eq!(default_rendered.background, RgbColor::new(16, 32, 48));
    assert_eq!(
        default_rendered.selection_background,
        RgbColor::new(48, 64, 80)
    );
    assert_eq!(
        indexed_rendered.lines[0].runs[0].foreground,
        RgbColor::new(255, 92, 87)
    );
}

#[test]
fn fixed_dark_theme_schemes_use_axshell_ansi() {
    for scheme in [
        TerminalColorScheme::Dark,
        TerminalColorScheme::SolarizedDark,
        TerminalColorScheme::ArcticDark,
        TerminalColorScheme::TokyoDark,
        TerminalColorScheme::EmberDark,
        TerminalColorScheme::ForestDark,
    ] {
        assert_eq!(TerminalPalette::for_scheme(scheme).ansi, AXSHELL_DARK_ANSI);
    }
}

#[test]
fn neutral_brightness_preserves_all_foreground_sources_exactly() {
    for (color, expected) in [
        (TerminalColor::Default, RgbColor::new(204, 204, 204)),
        (TerminalColor::Indexed(1), RgbColor::new(255, 92, 87)),
        (TerminalColor::Indexed(208), RgbColor::new(255, 135, 0)),
        (
            TerminalColor::Rgb {
                red: 100,
                green: 120,
                blue: 140,
            },
            RgbColor::new(100, 120, 140),
        ),
    ] {
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: color,
                ..TerminalStyle::default()
            }),
            settings(),
        );
        assert_eq!(rendered.lines[0].runs[0].foreground, expected);
    }
}

#[test]
fn brightness_adjusts_all_foreground_sources_but_not_surfaces_or_cursor() {
    let background = RgbColor::new(220, 230, 240);
    for color in [
        TerminalColor::Default,
        TerminalColor::Indexed(1),
        TerminalColor::Indexed(208),
        TerminalColor::Rgb {
            red: 100,
            green: 120,
            blue: 140,
        },
    ] {
        let rendered = render_terminal(
            snapshot(TerminalStyle {
                foreground: color,
                background: TerminalColor::Rgb {
                    red: background.red,
                    green: background.green,
                    blue: background.blue,
                },
                ..TerminalStyle::default()
            }),
            TerminalRenderSettings {
                text_brightness: 0.60,
                ..settings()
            },
        );
        assert_ne!(
            rendered.lines[0].runs[0].foreground,
            resolve_color(
                color,
                settings().default_foreground,
                &TerminalPalette::for_scheme(TerminalColorScheme::Dark),
            )
        );
        assert_eq!(rendered.lines[0].runs[0].background, background);
        assert_eq!(rendered.background, settings().default_background);
        assert_eq!(
            rendered.selection_background,
            settings().selection_background
        );
        assert_eq!(rendered.foreground, settings().default_foreground);
    }
}

#[test]
fn inverse_is_resolved_before_visible_foreground_brightness() {
    let rendered = render_terminal(
        snapshot(TerminalStyle {
            foreground: TerminalColor::Indexed(208),
            background: TerminalColor::Indexed(21),
            inverse: true,
            ..TerminalStyle::default()
        }),
        TerminalRenderSettings {
            text_brightness: 0.60,
            ..settings()
        },
    );
    let run = &rendered.lines[0].runs[0];

    assert_eq!(
        run.foreground,
        adjust_text_foreground(RgbColor::new(0, 0, 255), 0.60, false)
    );
    assert_eq!(run.background, RgbColor::new(255, 135, 0));
}

#[test]
fn dim_combines_with_brightness_in_the_final_foreground_adjustment() {
    let source = RgbColor::new(100, 120, 140);
    let rendered = render_terminal(
        snapshot(TerminalStyle {
            foreground: TerminalColor::Rgb {
                red: source.red,
                green: source.green,
                blue: source.blue,
            },
            dim: true,
            ..TerminalStyle::default()
        }),
        TerminalRenderSettings {
            text_brightness: 1.20,
            ..settings()
        },
    );

    assert_eq!(
        rendered.lines[0].runs[0].foreground,
        adjust_text_foreground(source, 1.20, true)
    );
    assert_ne!(
        rendered.lines[0].runs[0].foreground,
        adjust_text_foreground(source, 1.20, false)
    );
}
