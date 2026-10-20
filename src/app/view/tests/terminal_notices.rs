use super::*;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::ComplexText;
use i_slint_core::window::WindowInner;
use std::ops::ControlFlow;

fn has_text(ui: &AppWindow, expected: &str) -> bool {
    ItemRc::new_root(WindowInner::from_pub(ui.window()).component())
        .visit_descendants(|item| {
            if item.is_visible()
                && let Some(text) = item.downcast::<ComplexText>()
                && ComplexText::FIELD_OFFSETS
                    .text()
                    .apply_pin(text.as_pin_ref())
                    .get()
                    .as_str()
                    == expected
            {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        })
        .is_some()
}

#[test]
fn notice_translations_follow_language_switch_in_main_and_detached_windows() {
    for detached in [false, true] {
        std::thread::spawn(move || {
            let harness = LayoutTestHarness::new();
            let ui = &harness.ui;
            ui.set_detached_window(detached);
            let tab = Uuid::new_v4();
            let show = |notice| {
                apply_active_snapshot(
                    ui,
                    ActiveTabSnapshot {
                        id: Some(tab),
                        kind: "terminal",
                        notice,
                        ..Default::default()
                    },
                    Some(tab),
                )
            };
            show(TerminalNoticeSnapshot::osc52_clipboard_read());
            ui.show().unwrap();
            harness.window.set_size(slint::PhysicalSize::new(960, 800));
            for (locale, title, allow, deny, message) in [
                (
                    "en",
                    "Remote clipboard request",
                    "Allow",
                    "Deny",
                    "A remote application wants to read your default clipboard.",
                ),
                (
                    "zh-CN",
                    "远程剪贴板请求",
                    "允许",
                    "拒绝",
                    "远程应用请求读取默认剪贴板。",
                ),
                (
                    "ja",
                    "リモートからのクリップボード要求",
                    "許可",
                    "拒否",
                    "リモートアプリケーションが既定のクリップボードの読み取りを要求しています。",
                ),
            ] {
                slint::select_bundled_translation(locale).unwrap();
                harness.settle();
                for text in [title, allow, deny, message] {
                    assert!(has_text(ui, text), "missing {locale} notice text: {text}");
                }
                assert_eq!(
                    ui.get_active_tab_notice_primary_action(),
                    "allow-osc52-clipboard-read"
                );
                assert_eq!(
                    ui.get_active_tab_notice_secondary_action(),
                    "deny-osc52-clipboard-read"
                );
            }
            slint::select_bundled_translation("zh-CN").unwrap();
            for (notice, title, action) in [
                (
                    TerminalNoticeSnapshot::failed("Connection failed: fixture.invalid", false),
                    "连接失败",
                    "重试",
                ),
                (
                    TerminalNoticeSnapshot::ended("fixture.invalid", true),
                    "会话已结束",
                    "重新启动",
                ),
                (
                    TerminalNoticeSnapshot::reconnecting("fixture.invalid"),
                    "连接已断开",
                    "关闭",
                ),
            ] {
                show(notice);
                harness.settle();
                for text in [title, action, "关闭", "fixture.invalid"] {
                    assert!(has_text(ui, text), "missing translated notice: {text}");
                }
            }
            slint::select_bundled_translation("en").unwrap();
            ui.hide().unwrap();
        })
        .join()
        .expect("isolated notice translation regression");
    }
}
