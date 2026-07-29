# 项目施工前预检

- 边界：`<repo-root>`；独立 Rust 桌面项目。`third_package/axshell` 是参考子模块，不属于 Cargo workspace 或构建图。
- 环境记忆：已初始化 `docs/project-env-audit/current.md`；当前环境与框架决策作为后续实施基线。
- 运行环境：Rust 2024，MSRV `1.92.0`（由 Slint `1.17.1` 约束）；本机 `rustc 1.96.1` / `cargo 1.96.1`；Tokio `1`、russh `0.62.2`、keyring `4.1.5`；入口为 `src/main.rs` 和 `ui/app.slint`；证据为 `Cargo.toml`、`Cargo.lock`、`build.rs`。
- 测试环境：Rust 单元测试、Slint build script、直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、`cargo test --locked --offline`、真实平台凭据忽略测试和 `git diff --check`；CI 使用 `.github/workflows/ci.yml` 在 Linux/macOS/Windows 执行 locked format/check/test。本机 Cargo 未安装 `fmt` 和 `clippy` 子命令。
- 变化与风险：2026-07-29 本机已可访问 crates.io，并成功锁定 keyring；macOS Keychain 真实写入/读取/删除已验证。冷缓存仍需要 registry，Linux Secret Service、Windows Credential Manager、真实 Slint GUI 和外部 SSH 交互未在本机自动验证。
- 开工判定：允许开工。当前工具链和本地缓存可完成离线构建，registry 可用于显式依赖解析；UI、配置、系统凭据、异步运行时和 russh 边界已明确，不需要引用参考子模块。

## 最后确认时间

- 2026-07-29 11:00 +0800
