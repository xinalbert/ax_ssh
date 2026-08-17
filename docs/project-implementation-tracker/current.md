# 当前项目实施记录

## 当前目标

- 目标 ID：20260817-terminal-selection-focus-clear
- 目标：终端本地选区在失去终端逻辑或原生输入焦点时立即清除，避免点击窗口其他控件后留下陈旧高亮。
- 交付物：Slint-local 选区失焦收口、保留右键 Copy 行为的双语契约、聚焦检查与完整离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/terminal-pane.slint`、终端选区/焦点行为、`docs/{architecture,architecture.zh,usage,usage.zh}.md` 与 `docs/project-implementation-tracker/`。
- 不在本轮范围内：终端内容解析、PTY/worker、SSH trust、凭据、配置 schema、依赖、锁文件、参考工程与截图自动化。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SS1 | completed | 终端局部选区在逻辑/输入失焦时清除 | Slint 编译、静态状态审阅 | 右键菜单 Copy 继续保留当前选区。 |
| SS2 | completed | 双语选区焦点契约和项目地图 | `cargo check`、Markdown 相对链接 | 选区仍不进入 Rust、日志或 transport。 |
| SS3 | completed | 完整离线质量门禁与跟踪验证 | fmt/check/Clippy/test/tracker/diff | 目标平台焦点、右键 Copy 与分屏待用户验收。 |

## 已完成

- 已审计选区只由 `TerminalPane` 的 anchor/focus 坐标在 Slint 本地持有，普通左键、滚轮、输入和粘贴已有局部清除。
- 已确认现有窗口/分屏控件会转移透明 `TextInput` 或 pane 的逻辑焦点；无需新增 Rust callback、worker、配置或依赖。
- 已在 `focused` 与透明 IME `has-focus` 的失焦分支调用既有 `clear-selection()`，覆盖分屏切换、divider、Tab、侧栏与其他可聚焦窗口控件。
- 已同步双语架构/使用说明和项目地图，明确右键 ContextMenuArea 的 Copy 继续使用当前局部选区。
- 已完成隔离 locked/offline 全量测试：库 179 项、应用 167 项和 Doc tests 均通过；终端底部锚定的几何回归也保持通过。

## 验证

- 已完成：锁定 Cargo metadata、当前 Slint 焦点/选区与 workspace 控件静态审阅、`cargo fmt --all -- --check`、Slint/Rust 联合 `cargo check --locked --offline`、严格 Clippy、隔离 locked/offline 全量 Cargo 测试、7 份本轮文档的相对 Markdown 链接和 `git diff --check`；环境事实无变化。
- 未完成：目标平台手工交互验收。

## 风险与阻塞

- 右键 ContextMenuArea 必须在 Copy action 前保留选区；本轮只响应逻辑 pane 切换和透明输入失焦，不能把选区提升到 Rust 以规避 UI 状态。
- 目标平台仍需确认分屏切换、divider、侧栏/Tab/Settings 点击、右键 Copy 和 IME 焦点；tracker validator 仍只报告 37 条既有历史时间字段问题，本轮未引入新增报告。

## 下一步

- 由用户验证选择、切换焦点与右键 Copy。

## 最后更新时间

- 2026-08-17 18:06 +0800
