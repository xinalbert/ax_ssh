# 当前项目实施记录

## 当前目标

- 目标 ID：20260729-terminal-rendering-interaction
- 目标：完成接近 VS Code 习惯的终端渲染、IME 与工作区交互，并加入独立 Local Shell Tab、可配置跨平台 shell 和可滚动分组 Activity Bar。
- 交付物：ANSI 彩色终端、网格选区/光标、透明 IME 代理、跨平台 Ctrl/Cmd 物理键还原、右键/快捷键设置、macOS 统一标题栏、唯一 ID 的 SSH/Local 终端 Tab、有界本地 PTY worker、版本化 shell 设置、Unicode 分组图标和完整验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`src/local_shell.rs`、`src/lib.rs`、`src/terminal.rs`、`src/config.rs`、`src/app.rs`、`src/app/`、`ui/`、双语 README/架构/开发文档和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：SSH 认证或 host-key 策略变更、全屏 TUI 兼容、鼠标上报、OSC 超链接、持久化终端内容、恢复上次工作区或复制 `third_package/axshell` 源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | ANSI 样式 run、标准/亮色/256 色/真彩色解析和亮度映射 | 终端模型 focused tests 与 Slint/Rust 联合编译 | scrollback 与快照保持有界 |
| P2 | completed | 版本化外观/右键/快捷键设置和 Settings 专页 | 配置迁移、clamp、快捷键测试与 GUI 走查 | 只持久化非敏感参数 |
| P3 | completed | macOS 统一标题栏、可折叠导航与动态 Tab 布局 | macOS 实际窗口截图和 Cargo 联合编译 | AppKit 失败时保留标准布局 |
| P4 | completed | 有界 Local Shell PTY worker、显式 SSH/Local backend 和版本化 shell 选择 | worker/config/state focused tests | 子进程、reader、writer 和关闭流程由 worker 独占 |
| P5 | completed | 顶部 Local Shell 入口、Unicode `前2*后2` 分组图标和可滚动 Activity Bar | Slint/Rust 联合编译和 GUI 走查 | 点击分组时展开侧栏和目标组 |
| P6 | completed | 双语产品/架构/开发说明和项目地图更新 | Markdown 相对链接与 tracking validator | 记录 shell、终端输入和生命周期 |
| P7 | completed | 最终格式、Cargo、差异、GUI 回归和清晰提交 | 仓库完整验证命令与 Git diff/stage 检查 | 记录本机缺失工具和手工键盘验收边界 |

## 已完成

- 已用有界 `vt100` 网格取代文本框式终端，完成 ANSI 样式、宽字符、网格选区、整格光标、scrollback、字体测量、行高与 floor-based PTY 尺寸。
- 已完成逐 Tab UUID 的 SSH/Local backend、独立 worker、本地 shell 发现缓存、设置页参数化、顶部 Tab、Activity Bar、分组图标和 macOS 统一标题栏。
- 已完成 application-cursor 方向键、`Shift+-` 后备、终端 Ctrl 优先、平台剪贴板快捷键和可配置右键复制/粘贴。
- 已移除可见终端编辑框；透明 `TextInput` 只作为随网格光标定位的系统 IME 代理，预编辑留给输入法，提交文本只发送一次。
- 已确认 Slint 1.17.1 在 Apple 平台交换 Command/Control 字段，并在 `src/app.rs` 恢复物理修饰键语义；macOS 物理 `Ctrl+B/C` 编码为 `0x02/0x03`，物理 `Cmd+C/V/S` 保留给剪贴板和工作区。
- SSH host-key 拒绝策略、短期凭据、worker 有界队列和日志禁记终端内容的安全边界保持不变。

## 验证

- 已完成：项目 skill/reference、locked Slint/winit Apple 修饰键源码、直接 `rustfmt --check`、`cargo check --locked --offline`、`cargo test --locked --offline`（库 43 passed、1 ignored；应用 15 passed）、Slint 联合编译、窗口截图、依赖/有界 channel 和 `git diff --check`。
- 未完成：本机未安装 `cargo-fmt` 与 `cargo-clippy` 子命令；真实中文输入法候选、物理 `Ctrl+B/C`、`Cmd+C/V/S` 和远端 tmux 仍需以最新二进制做目标平台手工验收。

## 风险与阻塞

- 无实现阻塞。GUI 自动化不能证明物理 macOS 修饰键、系统中文 IME 候选窗或真实 tmux 的端到端行为，需保留手工验收。
- 透明 IME 代理是 Slint 接入系统输入法的必要边界，不能在没有替代平台输入法 API 的情况下删除。
- 后续升级 Slint/winit 时必须重新核对 Apple Command/Control 映射，避免重复交换或回归。

## 下一步

- 使用最新提交后的二进制手工验收中文 IME、`Ctrl+B/C`、`Cmd+C/V/S` 和真实 tmux；鼠标上报、全屏 TUI 完整兼容与工作区恢复作为后续独立目标。

## 最后更新时间

- 2026-07-29 16:31 +0800
