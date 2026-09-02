use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=AXSSH_BUILD_REVISION");
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=translations");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon.svg");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon_all_formats/terminal_icon_256.png");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon_all_formats/terminal_icon.ico");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon_all_formats/terminal_icon.icns");
    println!("cargo:rerun-if-changed=packaging/windows/axssh.rc");

    let revision = std::env::var("AXSSH_BUILD_REVISION")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "._+-".contains(character)
                })
        })
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=AXSSH_BUILD_REVISION={revision}");

    let resource_file = if matches!(env::var("TARGET"), Ok(target) if target.contains("windows")) {
        let icon_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/ion/terminal_icon_all_formats/terminal_icon.ico");
        assert!(
            icon_path.is_file(),
            "Windows application icon is missing: {}",
            icon_path.display()
        );
        let icon_path = icon_path.to_string_lossy().replace('\\', "\\\\");
        let resource_file =
            PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is missing")).join("axssh.rc");
        fs::write(&resource_file, format!("1 ICON \"{icon_path}\"\n")).unwrap_or_else(|error| {
            panic!(
                "failed to write generated Windows resource file {}: {error}",
                resource_file.display()
            )
        });
        resource_file
    } else {
        PathBuf::from("packaging/windows/axssh.rc")
    };

    embed_resource::compile(resource_file, embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed the Windows application icon");

    let slint_config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", slint_config)
        .expect("failed to compile Slint UI");
}
