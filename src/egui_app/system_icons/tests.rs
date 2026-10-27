use super::*;

#[path = "../../../build/system_icons.rs"]
mod builder;

#[test]
fn build_catalog_matches_domain_and_contains_visible_premultiplied_pixels() {
    let directory =
        std::env::temp_dir().join(format!("axssh-system-pixels-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    builder::generate(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/os-icons"),
        &directory,
    )
    .unwrap();
    assert_eq!(ASSETS.len(), system_icon_choices().len());
    assert!(ASSETS.len() <= 128);
    assert!(ASSETS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for (id, data) in ASSETS {
        assert_eq!(system_icon_id(id), Some(*id));
        assert_eq!(data.len(), SIDE * SIDE * 4);
        assert!(data.chunks_exact(4).any(|pixel| pixel[3] != 0), "{id}");
        assert!(
            data.chunks_exact(4)
                .all(|pixel| pixel[..3].iter().all(|color| *color <= pixel[3])),
            "{id}"
        );
        assert_eq!(
            std::fs::read(directory.join(format!("system-icon-{id}.rgba"))).unwrap(),
            *data
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn texture_variants_reuse_ids_preserve_alpha_and_clear_on_release() {
    let ctx = egui::Context::default();
    let mut icons = Icons::default();
    let mut native = None;
    let mut mask = None;
    let first = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.visuals_mut().override_text_color = Some(egui::Color32::LIGHT_BLUE);
        let original = icons.image(ui, "Debian", false).unwrap();
        native = Some(
            original
                .load_for_size(&ctx, egui::Vec2::splat(16.0))
                .unwrap()
                .texture_id()
                .unwrap(),
        );
        assert_eq!(original.image_options().tint, egui::Color32::WHITE);
        let mono = icons.image(ui, "debian", true).unwrap();
        mask = Some(
            mono.load_for_size(&ctx, egui::Vec2::splat(16.0))
                .unwrap()
                .texture_id()
                .unwrap(),
        );
        assert_eq!(mono.image_options().tint, egui::Color32::LIGHT_BLUE);
        assert!(icons.image(ui, "/arbitrary/icon.svg", false).is_none());
    });
    let native_pixels = &first
        .textures_delta
        .set
        .get(&native.unwrap())
        .unwrap()
        .last()
        .unwrap()
        .image;
    let mask_pixels = &first
        .textures_delta
        .set
        .get(&mask.unwrap())
        .unwrap()
        .last()
        .unwrap()
        .image;
    let (egui::ImageData::Color(native_pixels), egui::ImageData::Color(mask_pixels)) =
        (native_pixels, mask_pixels);
    for (original, mono) in native_pixels.pixels.iter().zip(&mask_pixels.pixels) {
        assert_eq!(original.a(), mono.a());
        assert_eq!(mono.to_array(), [mono.a(); 4]);
    }
    first.drop_without_applying_deltas();
    let reused = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.visuals_mut().override_text_color = Some(egui::Color32::RED);
        for follow in [false, true] {
            let image = icons.image(ui, "debian", follow).unwrap();
            let id = image
                .load_for_size(&ctx, egui::Vec2::splat(16.0))
                .unwrap()
                .texture_id()
                .unwrap();
            assert_eq!(Some(id), if follow { mask } else { native });
            assert_eq!(
                image.image_options().tint,
                if follow {
                    egui::Color32::RED
                } else {
                    egui::Color32::WHITE
                }
            );
        }
    });
    assert!(reused.textures_delta.set.is_empty());
    reused.drop_without_applying_deltas();
    assert_eq!(icons.textures.len(), 2);
    icons.clear();
    let released = ctx.run_ui(egui::RawInput::default(), |_| {});
    assert!(released.textures_delta.free.contains(&native.unwrap()));
    assert!(released.textures_delta.free.contains(&mask.unwrap()));
    released.drop_without_applying_deltas();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        for (id, _) in ASSETS {
            for follow in [false, true] {
                assert!(icons.image(ui, id, follow).is_some());
            }
        }
    })
    .drop_without_applying_deltas();
    assert_eq!(icons.textures.len(), ASSETS.len() * 2);
    assert!(icons.textures.len() <= 256);
    icons.clear();
    assert!(icons.textures.is_empty());
}

#[test]
fn identities_use_explicit_override_or_matching_discovery_without_guessing_from_titles() {
    let mut profile = SessionProfile::new("Ubuntu 24.04", "example.invalid", "fixture");
    assert_eq!(profile_icon(&profile), "");
    profile.detected_system = Some(Box::new(
        ax_ssh::config::SystemDetection::new(&profile, "Debian GNU/Linux 12", "debian").unwrap(),
    ));
    assert_eq!(profile_icon(&profile), "debian");
    profile.system_icon_override = Some("Mac OS".into());
    assert_eq!(profile_icon(&profile), "macos");
    profile.system_icon_override = None;
    profile.ssh_mut().unwrap().host = "replacement.invalid".into();
    assert_eq!(profile_icon(&profile), "");
    let mut appearance = AppearanceSettings::default();
    assert_eq!(
        local_icon(&appearance),
        ax_ssh::system_icons::local_platform()
    );
    appearance.local_shell_icon_override = Some("Windows".into());
    assert_eq!(local_icon(&appearance), "windows");
    appearance.local_shell_icon_override = Some("../path.svg".into());
    assert_eq!(
        local_icon(&appearance),
        ax_ssh::system_icons::local_platform()
    );
}

#[test]
fn split_tab_icon_follows_the_focused_pane_and_local_override() {
    use ax_ssh::controller::{PaneLayout, PanePlacement, WorkspacePaneView};
    let local = uuid::Uuid::new_v4();
    let remote = uuid::Uuid::new_v4();
    let mut profile = SessionProfile::new("macOS", "example.invalid", "fixture");
    profile.system_icon_override = Some("debian".into());
    let mut tab = WorkspaceTabView {
        id: local,
        title: "Ubuntu is only a title".into(),
        settings: false,
        layout: PaneLayout {
            panes: vec![
                PanePlacement {
                    tab_id: local,
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 1.0,
                    focused: true,
                },
                PanePlacement {
                    tab_id: remote,
                    x: 0.5,
                    y: 0.0,
                    width: 0.5,
                    height: 1.0,
                    focused: false,
                },
            ],
            dividers: vec![],
        },
        panes: vec![
            WorkspacePaneView {
                id: local,
                title: "Debian".into(),
                profile: None,
                local: true,
                sftp: false,
            },
            WorkspacePaneView {
                id: remote,
                title: "macOS".into(),
                profile: Some(profile),
                local: false,
                sftp: false,
            },
        ],
    };
    let appearance = AppearanceSettings {
        local_shell_icon_override: Some("Windows".into()),
        ..Default::default()
    };
    assert_eq!(tab_icon(&tab, &appearance), "windows");
    tab.layout.panes[0].focused = false;
    tab.layout.panes[1].focused = true;
    assert_eq!(tab_icon(&tab, &appearance), "debian");
    tab.panes[1].profile.as_mut().unwrap().system_icon_override = None;
    assert_eq!(tab_icon(&tab, &appearance), "");
}
