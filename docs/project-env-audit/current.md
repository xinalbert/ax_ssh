# 项目施工前预检

## 项目类型

- 边界：`<repo-root>`。
- 类型：独立 Rust 桌面项目；`third_package/axshell` 仅为参考子模块，不属于 Cargo workspace 或构建图。
- 开工判定：允许开工。

## 运行环境

- 语言与版本：Rust 2024，MSRV `1.92.0`；本机 `rustc 1.96.1`、`cargo 1.96.1`。
- 主要依赖：Slint `1.17.1`、Tokio `1`、russh `0.62.2`、keyring `4.1.5`。
- 构建入口：`src/main.rs`、`build.rs` 和 `ui/app.slint`。
- 依赖管理：Cargo，锁文件为 `Cargo.lock`。

## 测试环境

- 默认门禁：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、`cargo test --locked --offline` 和 `git diff --check`。
- CI：`.github/workflows/ci.yml` 在 Linux、macOS 和 Windows 执行 locked format/check/test。
- 本机限制：Cargo 未安装 `fmt` 和 `clippy` 子命令；真实 Slint GUI、Linux Secret Service、Windows Credential Manager 和外部 SSH 交互需要对应平台或手工验证。

## 关键命令

- `cargo check --locked --offline`
- `cargo test --locked --offline`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --locked --offline -- -D warnings`
- `git diff --check`

## 外部依赖

- 冷缓存依赖 crates.io registry；当前本机缓存可完成 locked/offline 构建。
- macOS Keychain 真实写入、读取和删除已验证；其他平台凭据服务尚未在本机验证。
- UI-only 布局修改不依赖网络或外部 SSH 服务器。

## 证据文件

- `Cargo.toml`
- `Cargo.lock`
- `build.rs`
- `.github/workflows/ci.yml`
- `AGENTS.md`

## 最后确认时间

- 2026-07-29 18:49 +0800
