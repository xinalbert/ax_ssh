# 当前项目实施记录

## 当前目标

- 目标 ID：20260810-detached-return-icon
- 目标：将 macOS detached 子窗口标题栏的文字 Return 按钮改为紧凑的系统返回图标。
- 交付物：带 Tooltip 和无障碍描述的 AppKit image-only 返回按钮、双语文档与项目记录、自动化验证和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/macos_window.rs`、`docs/{architecture,architecture.zh,usage,usage.zh,development,development.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：Slint 客户区布局、WindowRouter/WorkspaceTransfer、SSH/SFTP worker、跨平台标题栏、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| RICON1 | completed | 使用 SF Symbol 返回箭头替换文字标题，并提供 AppKit 模板图标回退 | macOS Cargo check、静态 API 复核 | 图标按钮保持同一行、固定尺寸、原 callback 和弱引用生命周期。 |
| RICON2 | completed | 同步双语架构/使用/开发说明、环境记录和项目地图 | tracker/Markdown validator | 文档明确图标的 Tooltip 与返回语义。 |
| RICON3 | completed | 执行完整门禁、审阅并创建独立提交 | fmt/check/clippy/test/diff/staged review | 标题栏实际图标、hover 和点击由用户在目标 macOS 验收。 |

## 已完成

- 已重新读取工程规则、AxSSH Rust/Slint skill、项目地图、环境记录和原生标题栏实现。
- 环境预检确认 Rust 2024、MSRV 1.92.0、macOS 11.0、锁定 `objc2-app-kit 0.3.2` 与 locked/offline Cargo 合同未漂移。
- 已从本地锁定 crate 源码确认 `NSImage::imageWithSystemSymbolName_accessibilityDescription`、`NSCellImagePosition::ImageOnly` 和 `NSImageNameGoBackTemplate` 可用，无需联网、新资源或依赖版本变更。
- 已将 58px 文字按钮改为 28px image-only 系统返回图标；SF Symbol 缺失时回退 AppKit 模板图标，Tooltip、图标无障碍描述和原 `returnWorkspace:` action 均保留。
- 已只为锁定的 `objc2-app-kit 0.3.2` 增加 `NSCell` feature，`Cargo.lock`、crate 版本和运行时依赖集合不变；`cargo check --locked --offline` 通过。
- 已同步双语架构、使用、开发说明、项目地图与环境记录。
- 已完成直接 Rustfmt、locked/offline Cargo check 与全量测试、tracker validator、44 个 Markdown 相对链接和差异检查，并确认没有参考项目耦合、秘密、无界 buffer/queue、UI 线程阻塞或 SSH trust 变化。

## 验证

- 已完成：项目边界与 AppKit API 复核、图标实现、双语文档、直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 121、Doc tests 0）、tracker validator、44 个 Markdown 相对链接、`git diff --check` 和提交范围审阅。
- 未完成：`cargo fmt --all -- --check` 与 `cargo clippy --all-targets --locked --offline -- -D warnings` 因本机缺少对应 Cargo 子命令无法执行；目标 macOS 人工验收待用户完成。

## 风险与阻塞

- SF Symbol 名称在目标系统异常缺失时会回退到 AppKit 自带模板返回图标；自动化已覆盖编译边界，剩余风险仅是目标平台实际渲染与操作。
- GUI 视觉验收必须由用户提供；本轮未自行截图，也未改变返回路由、worker 或 SSH 安全策略。

## 下一步

- 等待用户在目标 macOS 确认返回图标尺寸、Tooltip、无障碍读取和点击返回行为。

## 最后更新时间

- 2026-08-10 20:04 CST
