# 当前项目实施记录

## 当前目标

- 目标 ID：20260731-terminal-keyboard-input-routing
- 目标：按 Codex 会话 `019fb6bf-aa58-7bd3-abac-91183ca45218` 修正终端按键捕获、IME 分流、Function 键和 macOS Option Meta 策略。
- 交付物：终端按键编码与映射测试、持久化 `Option acts as Meta` 设置、Slint 输入路由、双语说明和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/terminal/input.rs`、`src/app/{input,terminal_bridge,settings_bridge,view}.rs`、`src/config.rs`、`ui/{app,workspace-shell,terminal-pane,settings}.slint`、`ui/settings/terminal.slint`、`docs/{architecture,usage}*.md` 与 `docs/project-implementation-tracker/`。
- 不在本轮范围内：SSH 信任策略、凭据记录格式或秘密生命周期、Tokio/worker 生命周期、依赖升级、参考子模块耦合和 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| K1 | completed | 键盘捕获审计与按键/文本分流契约 | Slint/Rust 边界检查 | 特殊键走 callback；可打印与 IME 提交文本走 `TextInput.edited` |
| K2 | completed | application-cursor Home/End、F1-F12 映射和 xterm 编码 | focused encoder/mapping tests | 不改变 SSH 或 worker 输入队列边界 |
| K3 | completed | macOS Option-as-Meta 持久化、DTO、Settings 页面和输入路由 | Cargo 联合编译、config tests | 默认保留 Option 字符、死键和 IME 文本提交 |
| K4 | completed | 双语文档、项目地图与完整门禁 | Rust/Cargo/Markdown/tracker/diff | GUI 平台行为由用户手动验收 |

## 已完成

- 已确认原有 `key-pressed` 将 Shift 或 macOS Option 的可打印输入过早直送终端；本轮改为只捕获特殊/控制键。
- 已确认根 `AppWindow` 仅转发 Rust-facing property/callback，`WorkspaceShell`、`TerminalPane` 和 `SettingsPane` 已有可扩展 DTO 边界。
- 已确认 macOS Control/Command 还原仍在 Rust `normalize_slint_modifiers`，不改变该既有平台兼容契约。
- 已确认无需联网或多 agent；不创建 Worker 记录。
- 已完成 application-cursor Home/End、F1-F12 领域映射和 xterm 编码；测试覆盖未修饰与 Ctrl+F5 序列。
- 已完成 schema v13 `TerminalSettings::option_as_meta`、Settings > Terminal 开关、Rust/Slint DTO 与输入 bridge；旧文件缺失字段时保持关闭。
- 已将可打印/Shift/IME committed text 与特殊/控制按键分成 `TextInput.edited` 和 `key-pressed` 两条路径；Windows/Linux Ctrl+Alt 可打印文本保留给 AltGr。

## 验证

- 已完成：按键编码与 Slint mapping focused tests（终端 8、应用 4、配置 1）；`cargo check --locked --offline`；完整 `cargo test --locked --offline`（库 63、应用 33、Doc tests 通过）；直接 `rustfmt --edition 2024 --check`；Markdown 相对链接、tracker validator 和 `git diff --check`。
- 未完成：`cargo fmt --all -- --check` 与 `cargo clippy --all-targets --locked --offline -- -D warnings`，本机未安装相应 Cargo 子命令；未进行 GUI 截图或平台键盘自动化验收。

## 风险与阻塞

- SSH host-key 拒绝、短期秘密、Tokio worker 和有界输入队列均不在本轮改变。
- Slint 不提供跨平台自动化 IME/死键验收；完成联合编译后，仍需用户在目标平台确认 Option、IME、AltGr 和全屏终端行为。

## 下一步

- 等待用户在目标平台手动验收 macOS Option/死键/中文或日文 IME、Option-as-Meta、Cmd/Ctrl、Windows/Linux AltGr，以及 `vim`、`less`、`tmux` 中的 Home/End 与 F1-F12。

## 最后更新时间

- 2026-07-31 16:01 +0800
