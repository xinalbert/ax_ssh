# 项目施工前预检

- 边界：`<repo-root>`；独立 Rust 桌面项目。`third_package/axshell` 是参考子模块，不属于 Cargo workspace 或构建图。
- 环境记忆：已初始化 `docs/project-env-audit/current.md`；当前环境与框架决策作为后续实施基线。
- 运行环境：Rust 2024，MSRV `1.92.0`（由 Slint `1.17.1` 约束）；本机 `rustc 1.96.1` / `cargo 1.96.1`；Tokio `1`、russh `0.62.2`；入口为 `src/main.rs` 和 `ui/app.slint`；证据为 `Cargo.toml`、`Cargo.lock`、`build.rs`。
- 测试环境：Rust 单元测试、Slint build script、`cargo fmt --all -- --check`、`cargo check --offline`、`cargo test --offline`、`git diff --check`；CI 使用 `.github/workflows/ci.yml` 在 Linux/macOS/Windows 执行 locked check/test。
- 变化与风险：根仓库由仅包含参考子模块变为独立 Cargo 项目；当前网络无法解析 crates.io，冷缓存环境需要外部 registry；真实 Slint GUI 和 SSH 服务器交互未自动验证。
- 开工判定：允许开工。当前工具链和本地缓存可完成离线构建；UI、配置、异步运行时和 russh 边界已明确，不需要引用参考子模块。

## 最后确认时间

- 2026-07-29 09:12 +0800
