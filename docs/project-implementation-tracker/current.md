# 当前项目实施记录

## 当前目标

- 目标 ID：20260729-tabbed-workspace-parameterized-settings
- 目标：把终端、设置和新建会话统一为顶部 Tab 工作区，并让同一服务器 profile 的多个终端实例通过唯一 Tab ID 独立运行。
- 交付物：Tab 领域模型、多 worker 事件路由、参数化 JSON 设置、Settings/New Session Tab、终端 Tab 生命周期测试、双语架构与完整验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config.rs`、`src/app.rs`、`src/app/`、`src/ssh/worker.rs`、`ui/`、双语 README/架构/开发文档和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：SFTP、SSH agent 转发、持久化终端输出或凭据、无确认接受未知主机密钥、完整全屏终端属性引擎、复制或依赖 `third_package/axshell` 源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 唯一 Tab ID、工作区 Tab 类型和参数化 JSON 设置 schema | 配置迁移与 Tab 领域单测 | 设置继续使用现有私有原子 JSON 存储 |
| P2 | completed | 按 `tab_id + attempt_id` 隔离的多 SSH worker、终端缓冲和关闭生命周期 | 同 profile 多实例、迟到事件、关闭和 shutdown 测试 | worker/receiver 不暴露给 Slint |
| P3 | completed | 顶部 Tab bar、终端 Tab、Settings Tab 和 New Session Tab | Slint/Cargo 联合编译和最小/常用窗口走查 | 安全确认和一次性 secret 仍为短期提示层 |
| P4 | completed | 设置保存、活动 Tab 快照、输入/resize/断开接线 | focused 应用状态与配置测试 | 非活动 Tab 不重建整个 Slint 输出模型 |
| P5 | completed | 双语架构、开发说明和项目地图更新 | Markdown 相对链接与边界扫描 | 明确 JSON schema 和多实例所有权 |
| P6 | completed | 最终格式、Cargo、tracker、差异和 GUI 回归 | 仓库要求的完整验证命令 | 记录本机缺失工具或平台验收限制 |

## 已完成

- 已确认参考布局的顶部 Tab、左侧导航和右侧内容区结构；仅参考行为与布局，不复制参考项目源码。
- 已确认现有单活动连接限制位于 `AppState.active_session`、全局终端输出和设置弹窗，需要跨应用状态与 UI 契约重构。
- 已实现 Settings/New Session 单例 Tab 和每次创建新 UUID 的终端 Tab；同 profile 使用独立标题序号、worker、attempt 和终端模型。
- 已把字体、scrollback、默认 PTY 尺寸、侧栏宽度和 Tab 宽度写入版本化 `settings` JSON，并兼容迁移旧版顶层 `appearance`。
- 已按 `tab_id + attempt_id` 路由连接、认证重试、输入、resize、输出和关闭；非活动终端输出保留在 Rust 状态。
- 已拆分 `ui/terminal-pane.slint`、`ui/settings.slint` 和 `ui/session-editor.slint`，并加入可横向滚动的顶部 Tab 条。

## 验证

- 已完成：项目 skill/reference 与参考图检查；直接 `rustfmt --edition 2024 --check`；`cargo check --locked --offline`；完整测试（库 29 passed、1 ignored，应用 7 passed）；Slint 多文件与生成 Rust 接口联合编译；默认窗口实际启动和窗口级截图；Markdown 相对链接、Cargo metadata、tracking validator、参考耦合/无界 channel 扫描和 `git diff --check`。
- 未完成：本机未安装 `cargo-fmt` 和 `cargo-clippy`；系统未授予辅助功能权限，无法自动点击 Settings/New Session、缩放到最小窗口或走查真实同服务器多连接，需手工验收。

## 风险与阻塞

- 每个终端 Tab 必须独占 worker、attempt ID 和有界终端模型；同 profile ID 不能作为运行实例键。
- 关闭 Tab 必须先让事件路由失效，再请求对应 worker shutdown，防止迟到事件污染其他 Tab。
- 设置只持久化非敏感参数；密码、passphrase、私钥内容、终端输出和运行时 handle 不得进入 JSON。
- 未知或变化主机密钥继续默认拒绝，并要求明确确认。
- 当前顶部 Tab 区支持 Flickable 横向滚动，但鼠标/触控板滚动手感仍需在目标平台手工确认。

## 下一步

- 手工走查 Settings/New Session Tab、横向 Tab 滚动、窗口最小尺寸和两个真实同 profile 终端；后续可单独增加重连与工作区恢复。

## 最后更新时间

- 2026-07-29 13:05 +0800
