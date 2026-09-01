use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const SETTINGS_SEARCH_CATALOG: [(&str, &str, &str); 46] = [
    (
        "General",
        "Language",
        "Language used by the AxSSH interface",
    ),
    (
        "General",
        "Local shell",
        "Executable used for new local terminal tabs",
    ),
    (
        "General",
        "Remembered password storage",
        "Used for remembered passwords and new passwords entered in the session editor",
    ),
    (
        "General",
        "Default terminal size",
        "10-300 columns, 3-100 rows",
    ),
    ("Appearance", "Font family", "Application interface font"),
    (
        "Appearance",
        "Renderer",
        "Changes take effect after restarting AxSSH",
    ),
    (
        "Appearance",
        "Software presentation",
        "macOS CPU presentation path; changes take effect after restarting AxSSH",
    ),
    (
        "Appearance",
        "Compact terminal rendering",
        "Reduce terminal items and omit default background rectangles",
    ),
    (
        "Appearance",
        "Cache unchanged terminal rows",
        "Reuse GPU row layers; may increase graphics memory",
    ),
    (
        "Appearance",
        "Blink terminal cursor",
        "Show a blinking cursor while the terminal is focused",
    ),
    (
        "Appearance",
        "Focused refresh rate",
        "Terminal updates while the pane is focused",
    ),
    (
        "Appearance",
        "Unfocused refresh rate",
        "Visible terminal updates while another pane is focused",
    ),
    (
        "Appearance",
        "Display mode",
        "Choose system, light, or dark appearance",
    ),
    (
        "Appearance",
        "Color palette",
        "Apply one color family to both display modes",
    ),
    (
        "Appearance",
        "Custom light palette",
        "Configure custom light application and terminal colors",
    ),
    (
        "Appearance",
        "Custom dark palette",
        "Configure custom dark application and terminal colors",
    ),
    ("Terminal", "Font family", "Terminal character-cell font"),
    ("Terminal", "Font size", "9-32 px"),
    ("Terminal", "Line height", "100-200 percent"),
    (
        "Terminal",
        "Text brightness",
        "60-120 percent; 100 percent preserves terminal colors",
    ),
    (
        "Terminal",
        "Bright colors for bold text",
        "Use the bright ANSI palette for bold text",
    ),
    (
        "Terminal",
        "Semantic highlighting",
        "Optional link, path, status, and log-level colors",
    ),
    (
        "Terminal",
        "Scrollback",
        "Number of terminal history lines retained in memory",
    ),
    (
        "Terminal",
        "Right-click copy or paste",
        "Choose copy or paste behavior for terminal right click",
    ),
    (
        "Terminal",
        "Copy selection on select",
        "Copy completed terminal selections immediately",
    ),
    (
        "Terminal",
        "Local selection priority",
        "On: Alt/Option sends mouse gestures; off: standard xterm with Shift selection",
    ),
    (
        "Terminal",
        "Option acts as Meta",
        "Use Option as terminal Meta on macOS",
    ),
    (
        "X11",
        "Provider",
        "X server used by forwarded graphical applications",
    ),
    (
        "X11",
        "Detected locations",
        "Known local X server installations",
    ),
    (
        "X11",
        "Custom executable",
        "Launch a custom X server executable",
    ),
    (
        "X11",
        "Start for first X11 application",
        "Launch the selected X server only when needed",
    ),
    (
        "X11",
        "Allow local connections without X authority",
        "Loopback-only compatibility mode",
    ),
    (
        "Workspace",
        "Session sidebar width",
        "180-420 px when sessions are available",
    ),
    ("Workspace", "Workspace tab width", "120-260 px"),
    (
        "Workspace",
        "Session mask character",
        "Masks usernames and IPv4 addresses in the session sidebar",
    ),
    (
        "Workspace",
        "Collapsed group label",
        "Choose 1-4 characters or show the full group name",
    ),
    ("Shortcuts", "Open Settings", "Application command shortcut"),
    ("Shortcuts", "New Server", "Workspace command shortcut"),
    (
        "Shortcuts",
        "Import from Clipboard",
        "Session transfer command shortcut",
    ),
    (
        "Shortcuts",
        "Export Selected",
        "Session transfer command shortcut",
    ),
    (
        "Shortcuts",
        "Toggle Session Sidebar",
        "Workspace command shortcut",
    ),
    (
        "Shortcuts",
        "Switch SSH/SFTP Tab",
        "Workspace command shortcut",
    ),
    (
        "Shortcuts",
        "Copy Terminal Selection",
        "Terminal command shortcut",
    ),
    (
        "Shortcuts",
        "Paste Into Terminal",
        "Terminal command shortcut",
    ),
    (
        "About",
        "Application",
        "AxSSH version, license, interface, transport, and runtime details",
    ),
    (
        "About",
        "Support",
        "Bug reports, runtime logs, and diagnostics",
    ),
];

fn settings_search_matches(query: &str, section: &str, title: &str, description: &str) -> bool {
    let query = query.trim();
    let query = query.to_lowercase();
    !query.is_empty()
        && [section, title, description]
            .into_iter()
            .any(|value| value.to_lowercase().contains(&query))
}

fn settings_search_results(query: &str, language: &str) -> Vec<SettingsSearchEntry> {
    let chinese = UiLanguage::from_setting(language)
        .resolved_locale(sys_locale::get_locale().as_deref())
        == "zh-CN";
    SETTINGS_SEARCH_CATALOG
        .iter()
        .filter(|(_, title, _)| cfg!(target_os = "macos") || *title != "Software presentation")
        .filter_map(|(section, title, description)| {
            let (display_section, display_title, display_description) =
                localized_settings_search_entry(section, title, description, chinese);
            settings_search_matches(query, section, title, description)
                .then_some(())
                .or_else(|| {
                    settings_search_matches(
                        query,
                        display_section,
                        display_title,
                        display_description,
                    )
                    .then_some(())
                })?;
            Some(SettingsSearchEntry {
                section: (*section).into(),
                title: display_title.into(),
                description: display_description.into(),
            })
        })
        .collect()
}

fn localized_settings_search_entry<'a>(
    section: &'a str,
    title: &'a str,
    description: &'a str,
    chinese: bool,
) -> (&'a str, &'a str, &'a str) {
    if !chinese {
        return (section, title, description);
    }
    SETTINGS_SEARCH_CATALOG_ZH_CN
        .iter()
        .find(|(source_title, source_description, _, _)| {
            source_title == &title && source_description == &description
        })
        .map(|(_, _, translated_title, translated_description)| {
            (
                localized_settings_section(section),
                *translated_title,
                *translated_description,
            )
        })
        .unwrap_or((localized_settings_section(section), title, description))
}

fn localized_settings_section(section: &str) -> &str {
    match section {
        "General" => "通用",
        "Appearance" => "外观",
        "Terminal" => "终端",
        "Workspace" => "工作区",
        "Shortcuts" => "快捷键",
        "About" => "关于",
        _ => section,
    }
}

const SETTINGS_SEARCH_CATALOG_ZH_CN: [(&str, &str, &str, &str); 46] = [
    (
        "Language",
        "Language used by the AxSSH interface",
        "语言",
        "AxSSH 界面使用的语言",
    ),
    (
        "Local shell",
        "Executable used for new local terminal tabs",
        "本地 Shell",
        "新建本地终端标签页使用的可执行程序",
    ),
    (
        "Remembered password storage",
        "Used for remembered passwords and new passwords entered in the session editor",
        "已记住密码的存储方式",
        "保存已记住密码所用的安全存储",
    ),
    (
        "Default terminal size",
        "10-300 columns, 3-100 rows",
        "默认终端大小",
        "10-300 列，3-100 行",
    ),
    (
        "Font family",
        "Application interface font",
        "字体系列",
        "应用界面字体",
    ),
    (
        "Renderer",
        "Changes take effect after restarting AxSSH",
        "渲染器",
        "重启 AxSSH 后生效",
    ),
    (
        "Software presentation",
        "macOS CPU presentation path; changes take effect after restarting AxSSH",
        "软件呈现方式",
        "macOS CPU 呈现路径；重启 AxSSH 后生效",
    ),
    (
        "Display mode",
        "Choose system, light, or dark appearance",
        "显示模式",
        "跟随系统、浅色或深色外观",
    ),
    (
        "Color palette",
        "Apply one color family to both display modes",
        "配色方案",
        "为两种显示模式应用同一配色系列",
    ),
    (
        "Custom light palette",
        "Configure custom light application and terminal colors",
        "自定义浅色配色",
        "配置应用和终端的浅色颜色",
    ),
    (
        "Custom dark palette",
        "Configure custom dark application and terminal colors",
        "自定义深色配色",
        "配置应用和终端的深色颜色",
    ),
    (
        "Font family",
        "Terminal character-cell font",
        "字体系列",
        "终端字符单元格字体",
    ),
    ("Font size", "9-32 px", "字体大小", "9-32 px"),
    ("Line height", "100-200 percent", "行高", "100-200%"),
    (
        "Text brightness",
        "60-120 percent; 100 percent preserves terminal colors",
        "文字亮度",
        "60-120%；100% 保留终端原色",
    ),
    (
        "Bright colors for bold text",
        "Use the bright ANSI palette for bold text",
        "粗体使用亮色",
        "为粗体使用明亮 ANSI 调色板",
    ),
    (
        "Semantic highlighting",
        "Optional link, path, status, and log-level colors",
        "语义高亮",
        "可选的链接、路径、状态和日志级别颜色",
    ),
    (
        "Compact terminal rendering",
        "Reduce terminal items and omit default background rectangles",
        "紧凑终端渲染",
        "减少终端绘制项并省略默认背景矩形",
    ),
    (
        "Cache unchanged terminal rows",
        "Reuse GPU row layers; may increase graphics memory",
        "缓存未变化的终端行",
        "复用 GPU 行图层；可能增加图形内存",
    ),
    (
        "Blink terminal cursor",
        "Show a blinking cursor while the terminal is focused",
        "终端光标闪烁",
        "终端聚焦时显示闪烁光标",
    ),
    (
        "Focused refresh rate",
        "Terminal updates while the pane is focused",
        "聚焦刷新率",
        "终端窗格聚焦时的刷新帧率",
    ),
    (
        "Unfocused refresh rate",
        "Visible terminal updates while another pane is focused",
        "非聚焦刷新率",
        "其他窗格聚焦时可见终端的刷新帧率",
    ),
    (
        "Scrollback",
        "Number of terminal history lines retained in memory",
        "回滚行数",
        "内存中保留的终端历史行数",
    ),
    (
        "Right-click copy or paste",
        "Choose copy or paste behavior for terminal right click",
        "右键复制或粘贴",
        "选择终端右键行为",
    ),
    (
        "Copy selection on select",
        "Copy completed terminal selections immediately",
        "选中后复制",
        "完成终端选择后立即复制",
    ),
    (
        "Local selection priority",
        "On: Alt/Option sends mouse gestures; off: standard xterm with Shift selection",
        "本地选区优先",
        "开启：Alt/Option 转发鼠标手势；关闭：标准 xterm，Shift 本地选择",
    ),
    (
        "Option acts as Meta",
        "Use Option as terminal Meta on macOS",
        "Option 作为 Meta",
        "在 macOS 终端中把 Option 用作 Meta",
    ),
    (
        "Provider",
        "X server used by forwarded graphical applications",
        "提供程序",
        "转发图形应用使用的 X server",
    ),
    (
        "Detected locations",
        "Known local X server installations",
        "检测到的位置",
        "已知本地 X server 安装",
    ),
    (
        "Custom executable",
        "Launch a custom X server executable",
        "自定义可执行文件",
        "启动自定义 X server 程序",
    ),
    (
        "Start for first X11 application",
        "Launch the selected X server only when needed",
        "首个 X11 应用时启动",
        "仅在需要时启动所选 X server",
    ),
    (
        "Allow local connections without X authority",
        "Loopback-only compatibility mode",
        "允许无 X authority 的本地连接",
        "仅限回环地址的兼容模式",
    ),
    (
        "Session sidebar width",
        "180-420 px when sessions are available",
        "会话侧栏宽度",
        "有会话时为 180-420 px",
    ),
    (
        "Workspace tab width",
        "120-260 px",
        "工作区标签宽度",
        "120-260 px",
    ),
    (
        "Session mask character",
        "Masks usernames and IPv4 addresses in the session sidebar",
        "会话掩码字符",
        "遮盖侧栏中的用户名和 IPv4 地址",
    ),
    (
        "Collapsed group label",
        "Choose 1-4 characters or show the full group name",
        "折叠组标签",
        "显示 1-4 个字符或完整组名",
    ),
    (
        "Open Settings",
        "Application command shortcut",
        "打开设置",
        "应用命令快捷键",
    ),
    (
        "New Server",
        "Workspace command shortcut",
        "新建服务器",
        "工作区命令快捷键",
    ),
    (
        "Import from Clipboard",
        "Session transfer command shortcut",
        "从剪贴板导入",
        "会话传输命令快捷键",
    ),
    (
        "Export Selected",
        "Session transfer command shortcut",
        "导出所选项",
        "会话传输命令快捷键",
    ),
    (
        "Toggle Session Sidebar",
        "Workspace command shortcut",
        "切换会话侧栏",
        "工作区命令快捷键",
    ),
    (
        "Switch SSH/SFTP Tab",
        "Workspace command shortcut",
        "切换 SSH/SFTP 标签页",
        "工作区命令快捷键",
    ),
    (
        "Copy Terminal Selection",
        "Terminal command shortcut",
        "复制终端选择",
        "终端命令快捷键",
    ),
    (
        "Paste Into Terminal",
        "Terminal command shortcut",
        "粘贴到终端",
        "终端命令快捷键",
    ),
    (
        "Application",
        "AxSSH version, license, interface, transport, and runtime details",
        "应用",
        "AxSSH 版本、许可证、界面、传输和运行时信息",
    ),
    (
        "Support",
        "Bug reports, runtime logs, and diagnostics",
        "支持",
        "错误报告、运行日志和诊断",
    ),
];

pub(super) fn wire_settings(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
    font_registry: Arc<Mutex<FontRegistry>>,
    window_router: WindowRouter,
    persistence: Arc<PersistenceCoordinator>,
) {
    ui.on_settings_search_results(|query, language| {
        ModelRc::new(VecModel::from(settings_search_results(
            query.as_str(),
            language.as_str(),
        )))
    });

    let ui_for_language = ui.as_weak();
    let state_for_language = state.clone();
    let runtime_for_language = runtime.clone();
    let persistence_for_language = persistence.clone();
    let language_revision = Arc::new(AtomicU64::new(0));
    ui.on_save_ui_language(move |index| {
        log_ui_action("settings.language.save");
        let language = UiLanguage::from_selector_index(index);
        let revision = language_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let language_revision = language_revision.clone();
        let state = state_for_language.clone();
        let ui = ui_for_language.clone();
        let persistence = persistence_for_language.clone();
        runtime_for_language.spawn(async move {
            let _persistence_guard = persistence.gate.lock().await;
            let revision_for_save = language_revision.clone();
            let save_result = tokio::task::spawn_blocking(move || {
                save_ui_language(&state, language, revision, &revision_for_save)
            })
            .await;
            dispatch_ui(&ui, move |ui| {
                if language_revision.load(Ordering::Acquire) != revision {
                    return;
                }
                match save_result {
                    Ok(Ok(true)) => {
                        if let Err(error) = apply_ui_language_to_open_windows(ui, language) {
                            ui.set_status(
                                format!("Cannot apply interface language: {error}").into(),
                            );
                        } else {
                            ui.set_status("".into());
                        }
                    }
                    Ok(Ok(false)) => {}
                    Ok(Err(error)) => {
                        ui.set_status(format!("Cannot save interface language: {error}").into())
                    }
                    Err(error) => {
                        ui.set_status(format!("Interface language task failed: {error}").into())
                    }
                }
            });
        });
    });

    let ui_for_save = ui.as_weak();
    let font_registry_for_save = font_registry;
    let persistence_for_save = persistence;
    ui.on_save_settings(
        move |application_font_family,
              renderer_preference,
              software_presentation,
              terminal_font_family,
              font_size,
              line_height_percent,
              terminal_software_block_rows,
              text_brightness,
              bright_bold_text,
              terminal_semantic_highlighting,
              terminal_compact_rendering,
              terminal_row_render_cache,
              terminal_cursor_blink,
              focused_terminal_refresh_fps,
              unfocused_terminal_refresh_fps,
              terminal_semantic_link_color,
              terminal_semantic_success_color,
              terminal_semantic_info_color,
              terminal_semantic_warning_color,
              terminal_semantic_error_color,
              theme_mode,
              theme_palette,
              theme_light_background,
              theme_light_panel,
              theme_light_panel_alt,
              theme_light_border,
              theme_light_text,
              theme_light_muted,
              theme_light_accent,
              theme_light_success,
              theme_light_danger,
              theme_light_overlay,
              theme_light_terminal_foreground,
              theme_light_terminal_background,
              theme_light_terminal_selection,
              theme_dark_background,
              theme_dark_panel,
              theme_dark_panel_alt,
              theme_dark_border,
              theme_dark_text,
              theme_dark_muted,
              theme_dark_accent,
              theme_dark_success,
              theme_dark_danger,
              theme_dark_overlay,
              theme_dark_terminal_foreground,
              theme_dark_terminal_background,
              theme_dark_terminal_selection,
              right_click_copy_or_paste,
              copy_selection_on_select,
              terminal_mouse_local_selection_priority,
              option_as_meta,
              x11_server_provider,
              x11_server_app_path,
              x11_launch_on_connect,
              x11_allow_no_auth,
              local_shell,
              scrollback_lines,
              default_columns,
              default_rows,
              sidebar_width,
              tab_width,
              session_mask_character,
              collapsed_group_label_chars,
              open_settings_shortcut,
              new_session_shortcut,
              import_sessions_shortcut,
              export_selected_shortcut,
              toggle_sidebar_shortcut,
              copy_selection_shortcut,
              paste_shortcut,
              open_sftp_shortcut,
              credential_storage,
              settings_tab_id,
              close_after_save| {
            let is_preview = !close_after_save && settings_tab_id.is_empty();
            log_ui_action(if is_preview {
                "settings.preview"
            } else {
                "settings.save"
            });
            if !close_after_save && !is_preview {
                set_status(&ui_for_save, "Invalid Settings preview request");
                return;
            }
            let close_tab_id = if close_after_save {
                let Some(id) = parse_uuid(settings_tab_id.as_str(), "settings tab", &ui_for_save)
                else {
                    return;
                };
                Some(id)
            } else {
                None
            };
            let shortcuts = ShortcutSettings {
                open_settings: open_settings_shortcut.as_str().to_owned(),
                new_session: new_session_shortcut.as_str().to_owned(),
                import_sessions: import_sessions_shortcut.as_str().to_owned(),
                export_selected: export_selected_shortcut.as_str().to_owned(),
                toggle_sidebar: toggle_sidebar_shortcut.as_str().to_owned(),
                copy_selection: copy_selection_shortcut.as_str().to_owned(),
                paste: paste_shortcut.as_str().to_owned(),
                open_sftp: open_sftp_shortcut.as_str().to_owned(),
            };
            if let Err(error) = shortcuts.validate() {
                if !is_preview {
                    set_status(&ui_for_save, &format!("Cannot save shortcuts: {error}"));
                }
                return;
            }
            let (known_shells, ui_language) = match state.lock() {
                Ok(app) => (
                    app.sessions.settings.terminal.known_shells.clone(),
                    app.sessions.settings.ui_language,
                ),
                Err(_) => {
                    set_status(&ui_for_save, "Cannot read local shell settings");
                    return;
                }
            };
            let settings = AppSettings::normalized(AppSettingsInput {
                appearance: AppearanceSettingsInput {
                    renderer_preference: renderer_preference.as_str(),
                    software_presentation: software_presentation.as_str(),
                    application_font_family: application_font_family.as_str(),
                    terminal_font_family: terminal_font_family.as_str(),
                    terminal_font_size: font_size,
                    terminal_line_height_percent: line_height_percent,
                    terminal_software_block_rows,
                    color_scheme: terminal_color_scheme_for_theme(
                        theme_mode.as_str(),
                        theme_palette.as_str(),
                    ),
                    text_brightness,
                    semantic_highlighting: terminal_semantic_highlighting,
                    terminal_compact_rendering,
                    terminal_row_render_cache,
                    terminal_cursor_blink,
                    focused_terminal_refresh_fps,
                    unfocused_terminal_refresh_fps,
                    terminal_semantic_colors: TerminalSemanticColorsInput {
                        link: terminal_semantic_link_color.as_str(),
                        success: terminal_semantic_success_color.as_str(),
                        info: terminal_semantic_info_color.as_str(),
                        warning: terminal_semantic_warning_color.as_str(),
                        error: terminal_semantic_error_color.as_str(),
                    },
                    bright_bold_text,
                    right_click_copy_or_paste,
                    copy_selection_on_select,
                    terminal_mouse_local_selection_priority,
                },
                terminal: TerminalSettingsInput {
                    scrollback_lines,
                    default_columns,
                    default_rows,
                    local_shell: local_shell.as_str(),
                    known_shells: &known_shells,
                    option_as_meta,
                },
                workspace: WorkspaceSettingsInput {
                    sidebar_width,
                    tab_width,
                    session_mask_character: session_mask_character.as_str(),
                    collapsed_group_label_chars,
                },
                shortcuts,
                credential_storage: credential_storage.as_str(),
                ui_language: ui_language.as_setting(),
            });
            let mut settings = settings;
            settings.x11 = X11Settings::normalized(
                x11_server_provider.as_str(),
                x11_server_app_path.as_str(),
                x11_launch_on_connect,
                x11_allow_no_auth,
            );
            settings.set_theme(ThemeSettings::normalized(
                theme_mode.as_str(),
                theme_palette.as_str(),
                ThemePalette {
                    background: theme_light_background.to_string(),
                    panel: theme_light_panel.to_string(),
                    panel_alt: theme_light_panel_alt.to_string(),
                    border: theme_light_border.to_string(),
                    text: theme_light_text.to_string(),
                    muted: theme_light_muted.to_string(),
                    accent: theme_light_accent.to_string(),
                    success: theme_light_success.to_string(),
                    danger: theme_light_danger.to_string(),
                    overlay: theme_light_overlay.to_string(),
                    terminal_foreground: theme_light_terminal_foreground.to_string(),
                    terminal_background: theme_light_terminal_background.to_string(),
                    terminal_selection: theme_light_terminal_selection.to_string(),
                },
                ThemePalette {
                    background: theme_dark_background.to_string(),
                    panel: theme_dark_panel.to_string(),
                    panel_alt: theme_dark_panel_alt.to_string(),
                    border: theme_dark_border.to_string(),
                    text: theme_dark_text.to_string(),
                    muted: theme_dark_muted.to_string(),
                    accent: theme_dark_accent.to_string(),
                    success: theme_dark_success.to_string(),
                    danger: theme_dark_danger.to_string(),
                    overlay: theme_dark_overlay.to_string(),
                    terminal_foreground: theme_dark_terminal_foreground.to_string(),
                    terminal_background: theme_dark_terminal_background.to_string(),
                    terminal_selection: theme_dark_terminal_selection.to_string(),
                },
            ));
            if is_preview {
                let families = match apply_preview_settings(&state, settings.clone()) {
                    Ok(families) => families,
                    Err(error) => {
                        set_status(
                            &ui_for_save,
                            &format!("Cannot preview workspace settings: {error}"),
                        );
                        return;
                    }
                };
                if let Some(ui) = ui_for_save.upgrade() {
                    apply_settings_to_open_windows(&ui, &settings);
                }
                apply_terminal_presentation_policy(&window_router, &settings);
                refresh_session_models(&ui_for_save, &state);
                // Tile grouping is part of the terminal snapshot model, so a
                // settings preview must rebuild visible terminal views once.
                refresh_workspace(&ui_for_save, &state);
                load_preview_bundled_fonts(
                    runtime.clone(),
                    state.clone(),
                    ui_for_save.clone(),
                    font_registry_for_save.clone(),
                    families,
                );
                return;
            }
            let state = state.clone();
            let ui = ui_for_save.clone();
            let window_router_for_save = window_router.clone();
            let font_registry = font_registry_for_save.clone();
            let runtime_for_save = runtime.clone();
            let persistence_for_save = persistence_for_save.clone();
            let runtime_for_close = runtime_for_save.clone();
            set_status(&ui_for_save, "Saving workspace settings...");
            runtime.spawn(async move {
                let resources = match font_registry.lock() {
                    Ok(registry) => registry.resources(),
                    Err(_) => {
                        set_status(&ui, "Cannot access font resources");
                        return;
                    }
                };
                let families = vec![
                    settings.appearance.application_font_family.clone(),
                    settings.appearance.terminal_font_family.clone(),
                ];
                let font_load = match tokio::task::spawn_blocking(move || {
                    resources.load_bundled_fonts(&families)
                })
                .await
                {
                    Ok(Ok(fonts)) => fonts,
                    Ok(Err(error)) => {
                        set_status(&ui, &format!("Cannot read font resources: {error}"));
                        return;
                    }
                    Err(error) => {
                        set_status(&ui, &format!("Font loading task failed: {error}"));
                        return;
                    }
                };
                dispatch_ui(&ui, move |ui| {
                    for font in font_load {
                        let registration = font_registry
                            .lock()
                            .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                            .and_then(|mut registry| registry.register_loaded_font(font));
                        if let Err(error) = registration {
                            ui.set_status(format!("Cannot register font: {error}").into());
                            return;
                        }
                    }

                    let state_for_save = state.clone();
                    let state_for_refresh = state.clone();
                    let settings_for_save = settings.clone();
                    let ui_for_result = ui.as_weak();
                    let ui_for_refresh = ui_for_result.clone();
                    let ui_for_close = ui_for_result.clone();
                    let state_for_close = state_for_refresh.clone();
                    runtime_for_save.spawn(async move {
                        let _persistence_guard = persistence_for_save.gate.lock().await;
                        let save_result = tokio::task::spawn_blocking(move || {
                            save_workspace_settings(&state_for_save, settings_for_save)
                        })
                        .await;
                        match save_result {
                            Ok(Ok(saved_settings)) => dispatch_ui(&ui_for_result, move |ui| {
                                apply_settings_to_open_windows(ui, &saved_settings);
                                apply_terminal_presentation_policy(
                                    &window_router_for_save,
                                    &saved_settings,
                                );
                                refresh_session_models(&ui_for_refresh, &state_for_refresh);
                                ui.set_status("".into());
                                if let Some(tab_id) = close_tab_id {
                                    close_workspace_tab(
                                        tab_id,
                                        &state_for_close,
                                        &ui_for_close,
                                        &runtime_for_close,
                                    );
                                } else {
                                    refresh_workspace(&ui_for_refresh, &state_for_refresh);
                                }
                            }),
                            Ok(Err(error)) => set_status(
                                &ui_for_result,
                                &format!("Cannot save workspace settings: {error}"),
                            ),
                            Err(error) => set_status(
                                &ui_for_result,
                                &format!("Workspace settings task failed: {error}"),
                            ),
                        }
                    });
                    ui.set_status("Saving workspace settings...".into());
                });
            });
        },
    );
}

fn save_workspace_settings(
    state: &Arc<Mutex<AppState>>,
    mut settings: AppSettings,
) -> Result<AppSettings> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    settings.ui_language = app.sessions.settings.ui_language;
    let mut candidate = app.sessions.clone();
    candidate.settings = settings.clone();
    app.config.save(&candidate)?;
    app.sessions = candidate;
    app.apply_scrollback_setting();
    Ok(settings)
}

fn save_ui_language(
    state: &Arc<Mutex<AppState>>,
    language: UiLanguage,
    revision: u64,
    latest_revision: &AtomicU64,
) -> Result<bool> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    if latest_revision.load(Ordering::Acquire) != revision {
        return Ok(false);
    }
    let mut candidate = app.config.load()?;
    candidate.settings.ui_language = language;
    app.config.save(&candidate)?;
    app.sessions.settings.ui_language = language;
    Ok(true)
}

fn apply_preview_settings(
    state: &Arc<Mutex<AppState>>,
    mut settings: AppSettings,
) -> Result<Vec<String>> {
    let mut app = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    settings.ui_language = app.sessions.settings.ui_language;
    let font_families = changed_font_families(&app.sessions.settings, &settings);
    app.sessions.settings = settings;
    app.apply_scrollback_setting();
    Ok(font_families)
}

fn changed_font_families(previous: &AppSettings, current: &AppSettings) -> Vec<String> {
    let pairs = [
        (
            &previous.appearance.application_font_family,
            &current.appearance.application_font_family,
        ),
        (
            &previous.appearance.terminal_font_family,
            &current.appearance.terminal_font_family,
        ),
    ];
    let mut changed = Vec::new();
    for (previous, current) in pairs {
        if previous.eq_ignore_ascii_case(current)
            || changed
                .iter()
                .any(|family: &String| family.eq_ignore_ascii_case(current))
        {
            continue;
        }
        changed.push(current.clone());
    }
    changed
}

fn load_preview_bundled_fonts(
    runtime: Handle,
    state: Arc<Mutex<AppState>>,
    ui: slint::Weak<AppWindow>,
    font_registry: Arc<Mutex<FontRegistry>>,
    families: Vec<String>,
) {
    if families.is_empty() {
        return;
    }
    let resources = match font_registry.lock() {
        Ok(registry) => registry.resources(),
        Err(_) => {
            set_status(&ui, "Cannot access font resources");
            return;
        }
    };
    runtime.spawn(async move {
        let font_load = match tokio::task::spawn_blocking(move || {
            resources.load_bundled_fonts(&families)
        })
        .await
        {
            Ok(Ok(fonts)) => fonts,
            Ok(Err(error)) => {
                set_status(&ui, &format!("Cannot read font resources: {error}"));
                return;
            }
            Err(error) => {
                set_status(&ui, &format!("Font loading task failed: {error}"));
                return;
            }
        };
        dispatch_ui(&ui, move |ui| {
            for font in font_load {
                let registration = font_registry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))
                    .and_then(|mut registry| registry.register_loaded_font(font));
                if let Err(error) = registration {
                    ui.set_status(format!("Cannot register font: {error}").into());
                    return;
                }
            }
            let generation = match font_registry.lock() {
                Ok(registry) => registry.generation_as_slint_int(),
                Err(_) => {
                    ui.set_status("Cannot update font state".into());
                    return;
                }
            };
            super::view::set_font_registry_generation(ui, generation);
            let settings = match state.lock() {
                Ok(app) => app.sessions.settings.clone(),
                Err(_) => {
                    ui.set_status("Cannot read workspace settings".into());
                    return;
                }
            };
            apply_settings_to_open_windows(ui, &settings);
            super::view::refresh_workspace(&ui.as_weak(), &state);
        });
    });
}

fn terminal_color_scheme_for_theme(mode: &str, palette: &str) -> &'static str {
    if mode.trim().eq_ignore_ascii_case("light") {
        "light"
    } else if palette.trim().eq_ignore_ascii_case("solarized") {
        "solarized-dark"
    } else if palette.trim().eq_ignore_ascii_case("arctic") {
        "arctic-dark"
    } else if palette.trim().eq_ignore_ascii_case("tokyo") {
        "tokyo-dark"
    } else if palette.trim().eq_ignore_ascii_case("ember") {
        "ember-dark"
    } else if palette.trim().eq_ignore_ascii_case("forest") {
        "forest-dark"
    } else {
        "dark"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_only_loads_new_font_families_once() {
        let previous = AppSettings::default();
        let mut current = previous.clone();
        current.appearance.application_font_family = "Maple Mono NF CN".to_owned();
        current.appearance.terminal_font_family = "Maple Mono NF CN".to_owned();

        assert_eq!(
            changed_font_families(&previous, &current),
            ["Maple Mono NF CN"]
        );
    }

    #[test]
    fn preview_replaces_memory_without_persisting() {
        let path = std::env::temp_dir()
            .join(format!("axssh-settings-preview-{}", Uuid::new_v4()))
            .join("sessions.json");
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(path.clone()),
            SessionStore::default(),
        )));
        let mut settings = AppSettings::default();
        settings.terminal.scrollback_lines = 321;
        settings.ui_language = UiLanguage::SimplifiedChinese;

        let expected_language = state.lock().unwrap().sessions.settings.ui_language;

        let loaded = apply_preview_settings(&state, settings.clone()).unwrap();

        assert!(loaded.is_empty());
        assert_eq!(
            state.lock().unwrap().sessions.settings.ui_language,
            expected_language
        );
        settings.ui_language = expected_language;
        assert_eq!(state.lock().unwrap().sessions.settings, settings);
        assert!(!path.exists());
    }

    #[test]
    fn language_save_persists_only_after_success() {
        let path = std::env::temp_dir()
            .join(format!("axssh-language-save-{}", Uuid::new_v4()))
            .join("sessions.json");
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(path.clone()),
            SessionStore::default(),
        )));

        let revision = AtomicU64::new(1);
        assert!(save_ui_language(&state, UiLanguage::SimplifiedChinese, 1, &revision).unwrap());

        assert_eq!(
            state.lock().unwrap().sessions.settings.ui_language,
            UiLanguage::SimplifiedChinese
        );
        let persisted = ConfigStore::new(path).load().unwrap();
        assert_eq!(
            persisted.settings.ui_language,
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn language_save_keeps_other_preview_settings_unpersisted() {
        let path = std::env::temp_dir()
            .join(format!("axssh-language-preview-{}", Uuid::new_v4()))
            .join("sessions.json");
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(path.clone()),
            SessionStore::default(),
        )));
        let mut preview = AppSettings::default();
        preview.terminal.scrollback_lines = 321;
        apply_preview_settings(&state, preview).expect("preview should update memory");

        let revision = AtomicU64::new(1);
        assert!(save_ui_language(&state, UiLanguage::SimplifiedChinese, 1, &revision).unwrap());

        assert_eq!(
            state
                .lock()
                .unwrap()
                .sessions
                .settings
                .terminal
                .scrollback_lines,
            321
        );
        let persisted = ConfigStore::new(path).load().unwrap();
        assert_eq!(
            persisted.settings.terminal.scrollback_lines,
            AppSettings::default().terminal.scrollback_lines
        );
        assert_eq!(
            persisted.settings.ui_language,
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn stale_language_save_cannot_overwrite_latest_request() {
        let path = std::env::temp_dir()
            .join(format!("axssh-language-revision-{}", Uuid::new_v4()))
            .join("sessions.json");
        let state = Arc::new(Mutex::new(AppState::new(
            ConfigStore::new(path.clone()),
            SessionStore::default(),
        )));
        let latest_revision = AtomicU64::new(2);

        assert!(
            save_ui_language(&state, UiLanguage::SimplifiedChinese, 2, &latest_revision,).unwrap()
        );
        assert!(!save_ui_language(&state, UiLanguage::English, 1, &latest_revision).unwrap());

        assert_eq!(
            state.lock().unwrap().sessions.settings.ui_language,
            UiLanguage::SimplifiedChinese
        );
        let persisted = ConfigStore::new(path).load().unwrap();
        assert_eq!(
            persisted.settings.ui_language,
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn settings_search_matches_titles_descriptions_and_sections_case_insensitively() {
        let title_matches = settings_search_results("FONT", "english");
        assert!(
            title_matches
                .iter()
                .any(|entry| { entry.section == "Appearance" && entry.title == "Font family" })
        );
        assert!(
            title_matches
                .iter()
                .any(|entry| { entry.section == "Terminal" && entry.title == "Font family" })
        );

        let description_matches = settings_search_results("forwarded graphical", "english");
        assert_eq!(description_matches.len(), 1);
        assert_eq!(description_matches[0].section, "X11");
        assert_eq!(description_matches[0].title, "Provider");

        let section_matches = settings_search_results("workspace", "english");
        assert!(section_matches.iter().any(|entry| {
            entry.section == "Workspace" && entry.title == "Session sidebar width"
        }));

        let renderer_matches = settings_search_results("restarting", "english");
        assert_eq!(
            renderer_matches.len(),
            if cfg!(target_os = "macos") { 2 } else { 1 }
        );
        assert!(
            renderer_matches
                .iter()
                .all(|entry| entry.section == "Appearance")
        );
        assert!(
            renderer_matches
                .iter()
                .any(|entry| entry.title == "Renderer")
        );
        assert_eq!(
            renderer_matches
                .iter()
                .any(|entry| entry.title == "Software presentation"),
            cfg!(target_os = "macos")
        );

        let cache_matches = settings_search_results("graphics memory", "english");
        assert_eq!(cache_matches.len(), 1);
        assert_eq!(cache_matches[0].section, "Appearance");
        assert_eq!(cache_matches[0].title, "Cache unchanged terminal rows");

        let blink_matches = settings_search_results("blinking cursor", "english");
        assert_eq!(blink_matches.len(), 1);
        assert_eq!(blink_matches[0].section, "Appearance");
        assert_eq!(blink_matches[0].title, "Blink terminal cursor");
    }

    #[test]
    fn settings_search_ignores_empty_and_unknown_queries() {
        assert!(settings_search_results("   ", "english").is_empty());
        assert!(settings_search_results("not-a-setting", "english").is_empty());
    }

    #[test]
    fn settings_search_matches_chinese_without_changing_route_ids() {
        let matches = settings_search_results("界面字体", "simplified-chinese");

        assert!(matches.iter().any(|entry| {
            entry.section == "Appearance"
                && entry.title == "字体系列"
                && entry.description == "应用界面字体"
        }));

        let renderer_matches = settings_search_results("重启", "simplified-chinese");
        assert_eq!(
            renderer_matches.len(),
            if cfg!(target_os = "macos") { 2 } else { 1 }
        );
        assert!(
            renderer_matches
                .iter()
                .all(|entry| entry.section == "Appearance")
        );
        assert!(renderer_matches.iter().any(|entry| entry.title == "渲染器"));
        assert_eq!(
            renderer_matches
                .iter()
                .any(|entry| entry.title == "软件呈现方式"),
            cfg!(target_os = "macos")
        );

        let compact_matches = settings_search_results("省略默认背景", "simplified-chinese");
        assert_eq!(compact_matches.len(), 1);
        assert_eq!(compact_matches[0].section, "Appearance");
        assert_eq!(compact_matches[0].title, "紧凑终端渲染");

        let blink_matches = settings_search_results("终端光标", "simplified-chinese");
        assert_eq!(blink_matches.len(), 1);
        assert_eq!(blink_matches[0].section, "Appearance");
        assert_eq!(blink_matches[0].title, "终端光标闪烁");
    }
}
