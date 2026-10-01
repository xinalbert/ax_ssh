# 当前项目实施记录

## 当前目标

- 目标 ID：20261001-rust-199-clippy-compat
- 目标：修复 stable Rust 1.99 的 macOS Intel CI 严格 Clippy 失败，同时保持 Rust 1.92.0 MSRV。
- 交付物：两处最小 Rust 修复、目标编译和严格 Clippy 验证、实施与环境记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/runtime.rs`、`src/app/window_router.rs`、`docs/project-implementation-tracker/`、`docs/project-env-audit/`。
- 不在本轮范围内：CI target 矩阵、依赖版本、窗口计数语义、多窗口路由结果、SSH trust/凭据、Slint UI 和 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| CI1991 | completed | 核对 CI 报错、MSRV 和 Rust 官方 API 稳定版本 | 用户 CI 日志、Cargo/CI、Rust 文档 | `try_update` 从 1.95 才可用 |
| CI1992 | completed | 保留原子计数语义并去掉闭包多余借用 | 定向源码检查、Cargo check | 不用弃用抑制或升级 MSRV |
| CI1993 | completed | 在可用工具链和 macOS target 上完成门禁并提交 | fmt/check/Clippy/test/build、tracker/diff | Rust 1.99 Intel 与 Rust 1.92 MSRV 均已验证 |

## 已完成

- CI 使用 stable 工具链并在两个 macOS target 上执行 `cargo clippy --all-targets --locked --target ... -- -D warnings`；用户提供的 Intel 日志包含两处 warning 升级为错误。
- 本机默认 Rust/Cargo 1.97.1，仓库 MSRV 1.92.0；Rust 官方 API 文档说明 `fetch_update` 自 1.99 弃用、`try_update` 自 1.95 稳定，因此不能直接按 CI 提示替换。
- `src/app/runtime.rs` 属于 renderer 窗口计数与诊断；`src/app/window_router.rs` 属于多窗口路由。仅调整计数的原子实现和闭包传参，不改变跨模块契约或安全边界。
- 已用 `compare_exchange_weak` 循环保留零值不递减和原有内存序；`workspace_tab_for` 在第一处 `.map()` 直接按值传入。Rustfmt 检查通过。
- 已安装 Rust 1.99 和两个 macOS target；原始失败命令在 Rust 1.99 的 `x86_64-apple-darwin` 上通过，同 target 的 check 与 build 也通过。本机 Rust 1.97 完整测试库/应用各 299 项通过。
- Rust 1.92 的 `cargo check --locked --offline` 已通过；源码修复已提交为 `02eae17`。

## 验证

- 已完成：项目/环境预检、错误定位和 Rust 官方 API 核对。
- 已完成：Rust 1.97 的 fmt/check/严格 Clippy/完整测试、Rust 1.99 的 fmt 和 Intel macOS target 的 check/严格 Clippy/build、tracker、相对链接和 diff 检查。
- 已完成：Rust 1.92 的 MSRV check；源码提交 `02eae17`。本记录和环境记录通过校验后单独提交。
- 未完成：Windows/Linux 原生 CI target 与 GitHub 新一轮工作流结果。

## 风险与阻塞

- Rust 1.99 与 1.92 工具链已安装；本机缺 Windows/Linux 原生 SDK，平台验证依赖 CI。
- 原子递减在计数为零时必须保持零，且保留原有 `AcqRel` 成功、`Acquire` 失败内存序。

## 下一步

- 重新运行 CI，确认 Windows/Linux 原生 target 的严格 Clippy 与构建结果。

## 最后更新时间

- 2026-10-02 00:58 +0800
