# 环境审计变更

## 2026-07-29 初始化独立 Slint/russh 项目环境

- 触发原因：用户要求参考 `third_package/axshell` 的评估结论建立新框架，并明确 UI 使用 Slint、SSH 使用 russh，参考代码不作引用。
- 执行内容：确认外层仓库无既有 Cargo 项目；核对本机 Rust/Cargo、Slint/russh 缓存和参考子模块边界；建立 Rust 2024 清单、锁文件、Slint build 和三平台 CI 验证入口。
- 影响文件：`Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`docs/project-env-audit/`。
- 验证结果：本机 `cargo check --offline` 通过；在线 `cargo search` 因 crates.io DNS 解析失败不可用。
- 对 plan 的更新：后续开发以根 Cargo 项目为唯一实施边界，参考子模块保持 build graph 外部。
