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

    embed_resource::compile("packaging/windows/axssh.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed the Windows application icon");

    let slint_config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", slint_config)
        .expect("failed to compile Slint UI");
}
