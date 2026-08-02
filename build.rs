fn main() {
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon.svg");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon_all_formats/terminal_icon_256.png");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon_all_formats/terminal_icon.ico");
    println!("cargo:rerun-if-changed=assets/ion/terminal_icon_all_formats/terminal_icon.icns");
    println!("cargo:rerun-if-changed=packaging/windows/axssh.rc");

    embed_resource::compile("packaging/windows/axssh.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed the Windows application icon");

    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");
}
