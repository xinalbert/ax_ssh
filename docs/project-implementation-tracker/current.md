# 当前项目实施记录

## 当前目标

- 目标 ID：20260730-configurable-themes
- 目标：将应用主题做成可持久化设置，支持预设、完整自定义调色板、固定明暗模式和实时跟随系统。
- 交付物：版本化主题设置与迁移、Rust-Slint 映射、Appearance 设置页、双语架构说明和回归验证。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config.rs`、`src/app/{settings_bridge,terminal_render,view}.rs`、`ui/{app,settings,theme}.slint`、`ui/settings/appearance.slint`、`docs/architecture*.md`、`docs/project-implementation-tracker/`。
- 不在本轮范围内：SSH 信任/认证、凭据内容、Session Profile schema、Tab 生命周期、PTY/local shell/SSH worker 传输、窗口拖拽与静态几何 token。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 主题领域契约、系统配色能力和迁移策略 | 现有配置/UI/锁定 Slint 后端审查 | 旧设置固定迁移至现有深色预设；不新增依赖 |
| P2 | completed | 版本化主题设置、调色板解析、终端/UI 映射 | 20 个配置单测、4 个终端渲染单测、Slint/Cargo 联合编译 | 配置只保存领域色值，不引用 Slint 类型 |
| P3 | completed | Appearance 主题模式、预设和自定义颜色编辑 | Slint/Cargo 联合编译 | Settings 草稿在 Save 前不持久化，Appearance 页面可独立滚动 |
| P4 | completed | 双语文档、tracker 和完整离线回归 | locked/offline Cargo、tracker、Markdown 与 diff 检查 | 已记录可用与不可用的本机验证 |

## 已完成

- 已确定预设包含 AxSSH 深色（现有默认）、浅色和 Solarized 深色；自定义主题覆盖完整语义 UI 调色板及终端默认色。
- 已实现 schema 版本 9：旧终端配色迁移到对应固定主题；主题模式与 13 个规范化颜色字段持久化在私有 `sessions.json`，不含密码或其它 secret。
- 已实现跟随系统：Slint `Palette.color-scheme` 实时解析深浅色，未知系统配色安全回退到深色；刷新只重新渲染当前终端快照，不触发 PTY resize 或 worker 命令。
- 已实现 Appearance 的 Follow system/Dark/Light/Solarized Dark/Custom 选择器、13 个自定义色输入和滚动布局；主题草稿仍只在 Save 时跨越 UI 边界。
- 已核对 `third_package/axshell` 的主题产品模型，仅作为行为参考，未导入或复制其源码、资源和依赖。

## 验证

- 已完成：根指令、AxSSH Rust/Slint skill、项目地图、双语架构、项目环境 quick scan，锁定 Slint 1.17.1/winit 的 `Palette.color-scheme` 系统配色更新路径审查，`cargo check --locked --offline`，完整 `cargo test --locked --offline`（库 52 passed、1 ignored；应用 22 passed），直接 `rustfmt --edition 2024 --check`，tracker validator、Markdown 相对链接和 `git diff --check`。
- 未完成：`cargo fmt --all -- --check` 与 `cargo clippy --all-targets --locked --offline -- -D warnings` 因本机没有对应 Cargo 组件无法执行；目标平台的主题视觉验收仍需用户完成。

## 风险与阻塞

- Slint/winit 后端若无法报告系统配色，将按深色回退；实际 macOS/Windows/Linux 系统切换反馈需用户在目标平台验收。
- 主题切换必须重新渲染活动终端，保持活动终端 Tab 与其背景视觉连接，不得改变 ANSI 16/256 色、PTY resize 或 worker 队列。

## 下一步

- 用户在目标平台验收固定主题、自定义主题、运行中系统主题切换和窄窗口 Appearance 滚动；若有视觉反馈，仅调整 Slint 呈现层。

## 最后更新时间

- 2026-07-30 18:50 +0800
