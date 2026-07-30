# 环境审计变更

## 2026-07-29 初始化独立 Slint/russh 项目环境

- 触发原因：用户要求参考 `third_package/axshell` 的评估结论建立新框架，并明确 UI 使用 Slint、SSH 使用 russh，参考代码不作引用。
- 执行内容：确认外层仓库无既有 Cargo 项目；核对本机 Rust/Cargo、Slint/russh 缓存和参考子模块边界；建立 Rust 2024 清单、锁文件、Slint build 和三平台 CI 验证入口。
- 影响文件：`Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`docs/project-env-audit/`。
- 验证结果：本机 `cargo check --offline` 通过；在线 `cargo search` 因 crates.io DNS 解析失败不可用。
- 对 plan 的更新：后续开发以根 Cargo 项目为唯一实施边界，参考子模块保持 build graph 外部。

## 2026-07-29 刷新 registry 与系统凭据环境

- 触发原因：会话密码记忆引入跨平台 keyring，原记录的 crates.io DNS 失败事实也已过时。
- 执行内容：确认根目录仍是单一 Rust package；核对 Rust/Cargo 版本、CI、Cargo metadata 和 keyring feature tree；重新访问 crates.io，并运行真实 macOS Keychain 写入、读取和删除测试。
- 影响文件：`Cargo.toml`、`Cargo.lock`、`src/credentials.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`。
- 验证结果：`cargo search keyring --limit 1` 返回 `4.1.5`；locked/offline Cargo metadata 仅含一个 workspace member；macOS 平台凭据 round-trip 测试通过且测试条目已删除。本机仍未安装 `cargo-fmt`/`cargo-clippy`。
- 对 plan 的更新：registry 不再是当前阻塞；Linux Secret Service 和 Windows Credential Manager 保留为对应平台验收项。

## 2026-07-29 对齐环境记忆契约

- 日期：2026-07-29 18:49 +0800
- 变化摘要：环境事实未变化；把 `current.md` 从压缩式预检摘要整理为项目类型、运行环境、测试环境、关键命令、外部依赖和证据文件的当前契约结构。
- 受影响文件：`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`。
- 更新后的命令或环境：继续使用 Rust 2024、锁定依赖和 locked/offline Cargo 门禁；UI-only 修复不需要网络或外部 SSH 服务。
- 验证结果：本机 `rustc 1.96.1`、`cargo 1.96.1` 与 locked/offline Cargo metadata 通过；仓库仍只有一个 workspace member。

## 2026-07-29 暴露 macOS 原生菜单直接依赖

- 日期：2026-07-29 23:44 +0800
- 变化摘要：为复用标准 macOS 应用菜单，将锁文件中已有的 objc2、objc2-app-kit 和 objc2-foundation 版本声明为 macOS target 的直接依赖，并启用 NSApplication/NSMenu/NSMenuItem 所需 feature。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`src/app/macos_window.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`。
- 更新后的命令或环境：继续使用 locked/offline Cargo 门禁；macOS 原生菜单编译不需要新增系统包或网络服务。
- 验证结果：`cargo check --locked --offline` 通过，最新二进制成功进入 Slint event loop，AppKit 菜单桥接没有运行时错误。

## 2026-07-30 复核 macOS 菜单恢复环境

- 日期：2026-07-30 07:18 +0800
- 变化摘要：最终改为消除 macOS 菜单对活动 Tab 动态状态的依赖，避免 Slint/Muda 重建后 Settings/About 丢失；运行环境、依赖版本和测试命令没有变化。
- 受影响文件：`src/app.rs`、`docs/architecture.md`、`docs/architecture.zh.md`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`。
- 更新后的命令或环境：继续使用 Rust 2024、锁定依赖和 locked/offline Cargo 门禁；AppKit 菜单安装仍只在 macOS UI 主线程执行。
- 验证结果：`cargo check --locked --offline` 通过；未新增依赖、系统服务或网络要求。
