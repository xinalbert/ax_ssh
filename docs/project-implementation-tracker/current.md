# 当前项目实施记录

## 当前目标

- 目标 ID：20260812-terminal-semantic-color-settings
- 目标：让 Terminal 的链接、信息、成功、警告和错误语义高亮色可在 Settings 中配置，并在所有终端主题上保持可读。
- 交付物：持久化的五项语义颜色、Settings > Terminal 编辑与即时预览、渲染期最小对比度保护、回归测试和双语契约更新。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config/{settings,session}.rs`、`src/app/{settings_bridge,terminal_render}.rs`、`src/app/view/{settings,terminal}.rs`、`ui/{app,settings,settings/terminal}.slint`，以及相关测试、双语文档和项目/环境记录。
- 不在本轮范围内：终端 target 打开语义、指针/下划线交互、终端缓冲/回滚、SSH/SFTP worker、host-key trust、凭据、依赖/工具链、构建文件和 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TSC1 | completed | 语义高亮色持久化模型、兼容默认值与输入规范化 | config 单元测试、配置反序列化回归 | 配置层只保存受限的 CSS hex 字符串，不引用 Slint。 |
| TSC2 | completed | Settings > Terminal 五项色值编辑、保存和即时预览映射 | Cargo 编译 Slint 图、bridge/view 测试 | UI 只管理草稿和用户意图；Rust 负责验证与保存。 |
| TSC3 | completed | 用户颜色注入渲染器、对比度保护、双语文档与完整门禁 | renderer tests、Cargo/test/tracker/diff | 保留显式 ANSI/真彩色 cell，不改 SSH/worker。 |

## 已完成

- 已复核现有语义高亮仅使用终端 ANSI 色表，且有界地覆盖默认前景/背景的 ASCII run。
- 已确认由 `AppearanceSettings` 持有全局五项颜色可同时适用于内置和自定义主题，避免将十项值耦合进 Light/Dark 应用主题 palette。
- 已完成环境预检：锁定 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 Cargo locked/offline 门禁保持不变；无需联网或新依赖。
- 已新增 schema v20 的五项 `TerminalSemanticColors`，旧配置缺失字段时保留主题默认 ANSI 色；有效输入规范为大写 `#RRGGBB`，非法值和空值回退为主题默认色。
- 已在 Settings > Terminal 增加 Link and path、Success、Information、Warning、Error 五项即时预览输入；空字段显示 Theme default，并保留可访问标签。
- 已让 `TerminalRenderSettings` 接收小型 RGB override DTO；每个类别仍按真实 Terminal 背景校正至至少 4.5:1，明确 ANSI/256/真彩、非默认背景、反色、dim 和非 ASCII run 维持原样。
- 已完成完整离线测试、Tracker/Markdown 相对链接及差异门禁；本机缺少 `cargo fmt` 与 `cargo clippy` 子命令，已以直接 Rustfmt 复核格式。

## 验证

- 已完成：项目地图、AxSSH Rust/Slint 规范、环境记忆、现有设置/渲染/迁移链路审阅；`cargo check --locked --offline`、`cargo test --locked --offline`（库 148、应用 147、Doc tests 0）、直接 Rustfmt、Tracker、Markdown 相对链接与差异检查。
- 未完成：无；`cargo fmt --all -- --check` 和 `cargo clippy --all-targets --locked --offline -- -D warnings` 均因本机缺少子命令无法运行，已记录为环境限制。

## 风险与阻塞

- 用户输入必须限制为不透明 `#RRGGBB`，无效值回退为主题 ANSI 默认色，避免让 Slint 或渲染器处理任意 CSS 颜色。
- 语义颜色仍须依据真实终端背景强制最低 4.5:1 对比度；显式 ANSI/256/真彩色、反色、dim 和背景色仍保持远端程序的样式。
- 本机可能缺少 `cargo fmt` 与 `cargo clippy` 子命令；若缺失将明确记录并由具备组件的 CI/环境补充。

## 下一步

- 等待目标平台用户确认 Settings 输入、主题切换和实际终端颜色层次。

## 最后更新时间

- 2026-08-12 09:10 CST
