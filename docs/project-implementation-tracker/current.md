# 当前项目实施记录

## 当前目标

- 目标 ID：20260818-terminal-events-and-tab-recovery
- 目标：终端遵循通用 xterm mouse reporting 事件仲裁，将连接失败/断开恢复提示绑定到对应 Terminal Tab/pane，稳定原生菜单/Tab 菜单状态链路，并校准 CJK/盒线字符格渲染，避免软件名特判、跨 Tab 状态串扰和 fallback 字形错位。
- 交付物：Slint pointer owner 调整、tab-local notice/Retry/Close 链路、稳定 menu-state DTO、CJK/盒线 cell metric 与分段绘制、双语架构契约、项目地图和完整离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/terminal.rs`、`src/app/{terminal_render.rs,terminal_bridge.rs,state/{tabs,terminal}.rs,view/{settings,terminal,workspace}.rs}`、`ui/{app,terminal-pane,components/terminal-grid}.slint`、终端 mouse reporting/Tab notice/menu-state/cell rendering 路由、`docs/{architecture,architecture.zh,usage,usage.zh}.md` 和 tracker。
- 不在本轮范围内：终端 parser/ANSI mode 解析、SSH trust、凭据、配置 schema、依赖、锁文件、参考工程代码复制或软件名特判。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| ME1 | completed | mouse reporting 下默认本地左键拖选，Alt/Option 才转发 click/drag/motion | Slint 编译、静态事件分流审阅 | 不识别 tmux/vim/htop 等软件名。 |
| ME2 | completed | mouse reporting 下保留终端上下文菜单 Copy/Paste/Select All 路径 | Slint 编译、使用说明审阅 | right-click copy/paste 设置仍优先。 |
| ME3 | completed | 双语使用/架构契约、项目地图和变更记录 | Cargo 全量门禁、tracker/diff 检查 | 真实 GUI 交互仍由用户验收。 |
| TN1 | completed | 失败/断线/重连耗尽在对应 Terminal pane 内显示 notice | 状态回归、Slint DTO 编译 | host-key/认证安全 prompt 继续使用原有阻塞弹窗。 |
| TN2 | completed | Retry/Restart 与 Close 通过 pane UUID 路由并重验窗口归属 | bridge 审阅、Tab/Pane 测试 | root Tab 关闭整组，child pane 只关闭自身。 |
| TN3 | completed | Tab-local snapshot 隔离、双语契约与全量门禁 | Cargo/Clippy/Test/diff | 真实 GUI 仍由用户验收。 |
| MN1 | completed | 菜单 enabled 状态改为 Rust 发布的稳定布尔值 | Cargo check、架构审阅 | terminal 输出/notice/status 刷新不再重绑已打开原生菜单。 |
| CR1 | completed | CJK/盒线统一 cell metric 与非 ASCII 分段渲染 | 终端/渲染定向回归、Cargo 全量门禁 | 保持 ASCII run 批量绘制和既有 PTY 列语义。 |

## 已完成

- 已确认 AxSSH 没有 tmux 特判；触发条件是终端私有 mouse mode，而不是本地或远程 worker 差异。
- 已将 `TerminalPane` 的 button/motion 转发条件收窄为 `mouse_reporting && Alt/Option && !Shift`，普通左键拖动默认进入 Slint-local 选区。
- 已保留 mouse reporting 下的滚轮转发行为；`Shift` + 滚轮继续走本地 scrollback。
- 已让 `TerminalGrid` 的上下文菜单不再仅因 mouse reporting 开启而禁用；菜单仍受 right-click copy/paste 模式控制。
- 已同步中英文 usage、architecture 和项目地图，明确采用 xterm.js `mouseEventsRequireAlt` 风格的通用仲裁，而非软件名识别。
- 已将失败、非主动断开、重连倒计时和重试耗尽转换为对应 Tab/pane 的非阻塞 notice；主动 Disconnect 不显示失败提示。
- 已让 Retry/Restart 复用既有连接或本地 shell worker 路径，Close 复用 Tab/child-pane 释放路径，并在 bridge 侧校验当前窗口 route。
- 已补充两个 Tab 切换时 notice 不串扰的状态回归；修正 Slint 1.17 不支持的 `accessible-role: status` 为受支持的 `text`。
- 已将菜单栏的 Terminal Edit、前后 Tab、Move/Close enabled 判断改为 Rust 发布的 `menu-terminal-active`、`menu-has-multiple-tabs`、`menu-has-active-tab`，不再直接依赖高频 `workspace-tabs` model 或 `active-tab-kind` 绑定。
- 已让 `TerminalPane` 共同测量 Latin、Han fallback 与 box-drawing glyph 的单 cell advance，并以最大值作为 grid cell；Rust 将非 ASCII cell 与 ASCII run 分开发布，Slint 在固定的一格或两格 span 中居中绘制这些字形。

## 验证

- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 180 项、应用 173 项、Doc tests 0）、宽字符/非 ASCII 分段/渲染对齐定向回归、413 条翻译校验、7 份 Markdown 相对链接和 `git diff --check`；tracker validator 仅报告既有历史时间字段问题，本轮新记录未新增错误。
- 未完成：目标平台本地/SSH/tmux 的中英文混排、盒线、缩放、选区和 PTY 列数人工验收。

## 风险与阻塞

- `Alt/Option` + 右键在不同平台/窗口系统上可能同时涉及原生 context-menu 触发与 TUI mouse event，需要目标平台实测确认。
- 本轮未改变底层 `TerminalModel` mouse mode 解析或编码；如果 TUI 对某些 mouse mode 有特殊期望，后续应在通用 xterm mouse event 层修正，而不是加入软件名特判。
- 极少数系统主字体可能比 Maple fallback 或盒线字体更窄；cell metric 取三者最大值可避免跨 cell 绘制，但需要目标平台确认额外字距是否可接受。

## 下一步

- 完成全量离线门禁后，由用户在本地/SSH/tmux/VSCode 中验证中文与 ASCII 混排、表格盒线、缩放/分屏后的对齐、选区和 PTY 换行列数，并回归鼠标、Tab、菜单和断开提示。

## 最后更新时间

- 2026-08-18 11:51 +0800
