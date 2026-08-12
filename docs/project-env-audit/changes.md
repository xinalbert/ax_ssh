# 项目环境变化记录

## 2026-08-12 macOS detached 标题栏连续性环境记录

- 日期：2026-08-12
- 变化摘要：detached 原生标题栏改为透明并采用已解析的 Terminal theme background；Return 使用系统 `rectangle.on.rectangle` 和 AppKit 多文档 template fallback。Settings preview/save/字体预览会沿既有 weak UI route 同步外观到 live detached UI，并只刷新对应原生标题栏背景。
- 受影响文件：`Cargo.toml`、`src/app.rs`、`src/app/{macos_window,window_router,settings_bridge}.rs`、`src/app/view{,.rs/settings.rs}`、`docs/{architecture,architecture.zh,usage,usage.zh,project-env-audit,project-implementation-tracker}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、`objc2-app-kit 0.3.2` 与 locked/offline Cargo 门禁；仅启用锁定 crate 的 `NSColor` feature，不新增 crate、版本、lockfile 解析、配置 schema、worker、SSH trust、凭据或 CI 契约。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo test --locked --offline`（库 148、应用 149、Doc tests 0）、本轮 Markdown 相对链接审阅和 `git diff --check` 通过。严格 Clippy 仅被范围外既有 4 项 lint 阻断，tracker validator 仅被既有月度历史的 14 条时间字段阻断；用户目标 macOS 视觉验收仍待完成。

## 2026-08-12 完成 Terminal 语义高亮色设置环境门禁

- 日期：2026-08-12
- 变化摘要：新增 schema v20 的五项可选 Terminal 语义色配置，并经既有 Settings 即时预览和 UI 独立 renderer 传递；空值/无效值保留主题 ANSI 默认色，最终色仍按真实终端背景执行 4.5:1 对比度保护。
- 受影响文件：`src/config{,.rs/{settings,tests}}`、`src/app{,.rs/{settings_bridge,terminal_render,view/{settings,terminal}}.rs`、`ui/{app,settings,settings/terminal,workspace-shell}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh,project-env-audit,project-implementation-tracker}/`。
- 更新后的命令或环境：继续使用锁定 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 Cargo locked/offline 门禁；没有新增 crate、`Cargo.lock`、工具链、CI、worker、SSH trust 或凭据契约变化。
- 验证结果：`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）、直接 `rustfmt --edition 2024 --check` 和 `git diff --check` 通过；`cargo fmt` 与 `cargo clippy` 因本机缺少子命令无法执行，GUI 视觉与交互留目标平台用户验收。

## 2026-08-10 复核单 Tab pane group 实施环境

- 日期：2026-08-10
- 变化摘要：本轮只调整现有 Rust/Slint 窗口、Tab 和 pane 路由，不新增 crate，不修改 `Cargo.toml`、`Cargo.lock`、Rust edition、MSRV 或 CI 命令。
- 受影响文件：预计为 `src/app.rs`、`src/app/{panes,state,view,workspace}.rs`、`ui/{app,workspace-shell}.slint`、相关测试和文档。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与 locked/offline Cargo 门禁；本机 `cargo fmt`/`cargo clippy` 子命令仍不可用。
- 验证结果：已从 `Cargo.toml`、`Cargo.lock`、CI、当前环境记忆和已通过的上一轮 257 项测试确认可以开工；本轮实现后的 Cargo/Slint 门禁待执行。

## 2026-08-10 完成单 Tab pane group 环境门禁

- 日期：2026-08-10
- 变化摘要：一个可见 Terminal Tab 现在管理其内部的独立 pane sessions；只调整现有 Rust/Slint 路由和文档，没有新增依赖、修改工具链、MSRV、Cargo lock 或 CI 命令。
- 受影响文件：`src/app.rs`、`src/app/{panes,state,view,workspace}.rs`、`ui/{app,workspace-shell}.slint`、相关测试和双语/跟踪文档。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；本机使用直接 `rustfmt` 回退，Cargo fmt/Clippy 由安装组件的 CI 执行。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 121、Doc tests 0）、tracker validator、44 个 Markdown 相对链接和 `git diff --check` 通过；`cargo fmt`/`cargo clippy` 因子命令未安装无法执行，目标平台 GUI 和真实连接生命周期仍需用户验收。

## 2026-08-10 记录终端 pane 可见分屏控件环境事实

- 日期：2026-08-10
- 变化摘要：在主窗口 `WorkspaceTitlebar` 内增加一组可聚焦的纵向/横向分屏图标和 Tooltip；控件复用活动 pane UUID 的 callback，没有新增 crate、修改 `Cargo.toml`/`Cargo.lock`、调整 Rust edition/MSRV 或 CI 命令。
- 受影响文件：`ui/{components/workspace-titlebar,workspace-shell,theme}.slint`、`docs/{usage,usage.zh,architecture,architecture.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与 locked/offline Cargo 门禁；本机 `cargo fmt`/`cargo clippy` 子命令仍不可用。
- 验证结果：`cargo check --locked --offline` 已重新编译完整 Slint 图，完整 `cargo test --locked --offline`（库 141、应用 116、Doc tests 0）、tracker validator、Markdown 相对链接检查和 `git diff --check` 均通过；`cargo fmt`/`cargo clippy` 因本机未安装对应子命令无法执行。目标平台 GUI/真实连接验收仍待用户完成。

## 2026-08-03 刷新最终提交门禁环境

- 日期：2026-08-03
- 变化摘要：本机 Cargo fmt/clippy 组件当前可用；五项跨模块结果完成按功能边界提交后，重新确认完整 Slint/Rust 构建、测试和 lint 状态。
- 受影响文件：`docs/project-env-audit/{current,changes}.md`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：关键命令仍为 locked/offline fmt、check、clippy、test 和差异检查；严格 Clippy 的既有基线需单独记录，不作为本轮功能回归。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo test --locked --offline`（库 109、应用 75、Doc tests 0）通过；严格全目标 Clippy 命中既有基线，允许已记录的八类 lint 后通过。

## 2026-08-03 记录 X11 延迟准备与位置快照环境

- 日期：2026-08-03
- 变化摘要：普通 SSH shell 创建不再准备或启动本机 X server；只有远端实际打开 X11 channel 时才解析 `DISPLAY`、查询 `xauth`、探测端点和按需启动 provider。Settings 以 Tokio blocking task 显示 macOS/Windows 已知 provider 的只读检测位置，Custom 保持用户显式路径。
- 受影响文件：`src/{x_server,ssh/{x11,worker,tests}}.rs`、`src/app/view.rs`、`ui/settings/`、`docs/project-env-audit/{current,changes}.md`。
- 更新后的命令或环境：Rust 2024、MSRV 1.92.0、现有 locked/offline 门禁及外部 X server 前置条件不变；首次图形 channel 的启动/认证/relay 继续受既有超时和队列上限约束。
- 验证结果：`cargo check --locked --offline`、X11/位置 focused tests、完整 `cargo test --locked --offline`（库 109、应用 68、Doc tests 0）、直接 Rustfmt、tracker validator、Markdown 相对链接和 `git diff --check` 通过；本机未安装 Cargo fmt/clippy 组件。真实三平台 provider 检测、GUI 布局与远端图形应用行为仍需目标平台手工验收。

## 2026-07-31 初始化施工前环境记忆

- 日期：2026-07-31
- 变化摘要：初始化当前项目的环境记忆；未改变仓库工具链或测试命令。
- 受影响文件：`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：Rust 2024、MSRV 1.92.0、Cargo locked/offline 验证；CI 覆盖 Ubuntu、macOS、Windows。
- 验证结果：本机 `rustc 1.96.1` 和 `cargo 1.96.1` 可用；`cargo fmt` 与 `cargo clippy` 组件未安装。

## 2026-07-31 记录 Telnet 与 Serial 依赖环境

- 日期：2026-07-31
- 变化摘要：锁定 `nectar 0.4.0`、`tokio-serial 5.5.0` 及 codec/异步 I/O 直接依赖，并把 loopback Telnet 与 Serial descriptor 回归纳入测试环境。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0 与既有 locked/offline 命令；运行时新增 Telnet codec 和跨平台 Serial transport。
- 验证结果：`cargo check --offline` 与完整 `cargo test --offline` 已通过；库 68 passed、应用 37 passed、Doc tests 通过。真实 Serial 驱动/权限/设备和三平台 CI 仍需外部验证；本机仍缺 Cargo fmt/clippy 子命令。

## 2026-07-31 替换 Telnet 协议依赖

- 日期：2026-07-31
- 变化摘要：以 `libmudtelnet-rs 2.0.10` 替换 `nectar 0.4.0` 和其 codec 直接依赖；新增 64 KiB 有界流分帧适配及逐字节/loopback 回归。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`src/telnet.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0 与既有 locked/offline 命令；Telnet parser 声明 MSRV 1.66，Serial transport 声明 MSRV 1.71。
- 验证结果：focused Telnet/schema 3 passed；`cargo check --locked --offline` 和完整 `cargo test --locked --offline` 通过（库 69 passed、应用 37 passed、Doc tests 通过）。本机仍缺 Cargo fmt/clippy 子命令；目标平台 CI、真实 Telnet 和实机 Serial 待外部验证。

## 2026-08-01 记录 SFTP 浏览依赖与测试面

- 日期：2026-08-01
- 变化摘要：新增 `russh-sftp 2.3.0` 与直接 `chrono 0.4`，并把 SFTP packet/path/state 边界纳入 Cargo 测试环境。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`src/sftp.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0 与 locked/offline 命令；SFTP v3 浏览复用已认证 SSH transport，并由 256 KiB frame、250 条分页和 2,000 条/2 MiB 目录预算约束。
- 验证结果：`cargo check --locked --offline`、SFTP focused tests（库 7 passed、应用 2 passed）和完整 `cargo test --locked --offline`（库 76 passed、应用 39 passed、Doc tests 通过）均通过；`russh-sftp` 未声明 MSRV，需继续由项目 CI 约束。三平台 CI、真实 SFTP 服务和 GUI 面板仍待外部验证；本机仍缺 Cargo fmt/clippy 子命令。

## 2026-08-01 记录运行时字体资源环境

- 日期：2026-08-01
- 变化摘要：确认本轮字体将作为 AxSSH 自有 `assets/fonts/` 文件在运行时读取，而非由 Slint 字体 import 编译进二进制；系统等宽字体发现使用已锁定的 `fontdb 0.23.0`。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`assets/fonts/`、`src/app/`、`ui/`、`docs/project-env-audit/current.md`。
- 更新后的命令或环境：Slint 1.17.1 增加 `unstable-fontique-010` feature，运行时注册由 UI 线程执行；系统扫描在 Tokio `spawn_blocking` 中执行，继续使用 locked/offline 验证命令。
- 验证结果：已核实 Slint 1.17.1 和本机缓存 `fontdb 0.23.0` API；资源导入、代码变更及 Cargo 门禁尚未执行。真实系统字体范围与发行包 Resources 路径仍需目标平台确认。

## 2026-08-01 完成运行时字体资源环境验证

- 日期：2026-08-01
- 变化摘要：完成 AxSSH 自有运行时字体资源、Slint fontique 注册和 `fontdb` 系统等宽字体发现，并更新锁文件。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`assets/fonts/`、`src/app/`、`ui/`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0 与 locked/offline 命令；字体文件在 Tokio blocking task 读取并在 Slint UI 线程注册，发行包必须携带 `assets/fonts/`。
- 验证结果：字体测试 4 项、应用目标测试 50 项、locked/offline check/build、直接 Rustfmt、tracker validator 和差异门禁通过；完整库测试 85 项中 3 个既有终端 resize/reflow 用例失败。本机仍缺 Cargo fmt/clippy，真实系统字体和打包资源路径待目标平台手工验收。

## 2026-08-02 记录 X11 forwarding 运行环境

- 日期：2026-08-02
- 变化摘要：将已锁定的 `rand 0.10` 从测试依赖提升为运行时依赖，并为 Tokio 启用 `process`，用于生成单次 X11 fake cookie 和执行带超时/输出上限的精确 `xauth` 查询。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`src/ssh/x11.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0 和 locked/offline 命令；运行 X11 forwarding 还要求已启动的本机 X server、local-only `DISPLAY` 以及该 display 的 `MIT-MAGIC-COOKIE-1`。
- 验证结果：`cargo check --locked --offline`、X11/config/worker 定向测试和应用目标测试（64 passed）通过；完整库测试为 92 passed、3 个既有 terminal resize/reflow 测试失败，本机仍缺 Cargo fmt/clippy 子命令。macOS、Linux、Windows 的真实 X server 行为仍需目标平台手工验收。

## 2026-08-02 增加跨平台 X server 选择与启动环境

- 日期：2026-08-02
- 变化摘要：新增 Settings > X11 与平台 X server 层，可选择 Auto/System/XQuartz/MacXServer/VcXsrv/Xming/Custom，并在 SSH 连接 worker 中有界探测或启动本机服务；没有新增依赖或工具链要求。
- 受影响文件：`src/x_server.rs`、`src/config/`、`src/ssh/`、`src/app/`、`ui/`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0 和 locked/offline 命令；macOS 标准路径为 XQuartz/MacXServer，Windows 从 Program Files 发现 VcXsrv/Xming，Linux 使用系统 `DISPLAY` 或显式 Custom。默认 cookie 模式不变，no-auth 启动必须显式选择并限制到 loopback。
- 验证结果：完整 Slint 图通过 `cargo check --locked --offline`；配置、provider、X11 relay 和 Settings 定向测试通过。三平台真实 X server 启动、GUI 设置布局和远端 `sshd` 互操作仍需目标平台手工验收。

## 2026-08-02 完成跨平台 X11 环境门禁

- 日期：2026-08-02
- 变化摘要：完成 provider 跨平台归一化、具体本机准备错误上报、测试隔离和最终离线门禁；未增加外部依赖。
- 受影响文件：`src/x_server.rs`、`src/ssh/{x11,worker,tests}.rs`、`src/app/view.rs`、X11 双语文档和项目记录。
- 更新后的命令或环境：桌面应用使用持久化 `X11Settings` 才能自动启动外部 X server；通用 worker 入口只探测既有 server，不擅自启动 GUI。Cargo fmt/clippy 子命令仍未安装。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、应用目标 68 项、X11/config/provider/SSH 定向测试、tracker validator、Markdown 相对链接、参考项目耦合扫描和 `git diff --check` 通过。完整库测试为 97 passed、3 failed，均是既有 Terminal resize/reflow 用例；真实三平台 X server 和远端 `sshd` 仍需手工验收。

## 2026-08-02 恢复 Terminal 完整测试门禁

- 日期：2026-08-02
- 变化摘要：确认 3 个 Terminal resize/reflow 失败来自测试夹具和过时断言，而非废弃生产路径；保留用例并校正到锁定的 `alacritty_terminal 0.26.0` 语义。
- 受影响文件：`src/terminal.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`、`docs/project-implementation-tracker/current.md`、`docs/project-implementation-tracker/changes/2026/08.md`
- 更新后的命令或环境：依赖、工具链和测试命令不变；继续使用 Rust 2024、MSRV 1.92.0 与 locked/offline Cargo 门禁。
- 验证结果：Terminal 定向测试 18 项、完整库测试 100 项、应用测试 68 项和 Doc tests 全部通过；`cargo check --locked --offline`、直接 Rustfmt、tracker validator 与 `git diff --check` 通过。本机仍缺 Cargo fmt/clippy 子命令。

## 2026-08-02 增加 X server 系统应用发现

- 日期：2026-08-02
- 变化摘要：在已锁定的 macOS `objc2-app-kit 0.3.2` 上启用 `NSWorkspace` feature，以 bundle identifier 发现 XQuartz/MacXServer；Windows 运行时增加进程 `PATH` 搜索后再检查 Program Files，没有新增 crate 或工具链版本。
- 受影响文件：`Cargo.toml`、`src/x_server.rs`、`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、现有 `Cargo.lock` 和 locked/offline 门禁；已知 provider 不再使用持久化路径，Custom 仍以无 shell 进程启动并校验文件类型/Unix executable 权限。
- 验证结果：X11 provider/path 定向测试 8 项、`cargo check --locked --offline` 和完整测试通过（库 105、应用 68、Doc tests）；本机应用数据库可按 bundle identifier 返回 XQuartz 与 MacXServer。真实 Windows 发现仍需 Windows CI 或目标机确认；Cargo fmt/clippy 子命令本机未安装。

## 2026-08-04 校正本机 Cargo 组件记录

- 日期：2026-08-04
- 变化摘要：最小环境复核确认当前本机没有 `cargo fmt` 和 `cargo clippy` 子命令，修正此前把它们记录为可用的过期结论；仓库工具链、依赖和 CI 命令未改变。
- 受影响文件：`docs/project-env-audit/current.md`、`docs/project-env-audit/changes.md`
- 更新后的命令或环境：Rust 2024、MSRV 1.92.0、`rustc/cargo 1.96.1` 和 locked/offline check/test 不变；fmt/clippy 继续由具备相应组件的 CI 执行。
- 验证结果：`cargo fmt --version` 与 `cargo clippy --version` 均返回 Cargo 子命令不存在；未安装新工具。

## 2026-08-04 完成五项审查修复环境门禁

- 日期：2026-08-04
- 变化摘要：SSH 认证前命令隔离、私有原子临时文件、profile mutation 协调、owned PTY shutdown 和配置输入上限均已纳入自动化回归；未新增依赖或改变工具链。
- 受影响文件：`src/{app,config,ssh,local_shell}.rs`、对应子模块/UI/测试，以及双语架构与项目追踪记录。
- 更新后的命令或环境：Rust 2024、MSRV 1.92.0、Cargo locked/offline 和既有三平台 CI 不变；本机继续使用直接 `rustfmt`，Cargo fmt/clippy 组件仍未安装。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 117、应用 84、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 通过；真实三平台 GUI、SSH/SFTP、凭据后端与本地 PTY 关闭留给目标环境验收。

## 2026-08-08 完成按需加载与资源回收环境门禁

- 日期：2026-08-08
- 变化摘要：启动扫描改为 Settings/Private key/Serial/Terminal/SFTP 操作按需触发，并为页面、模式和图标缓存增加释放边界；未新增依赖或改变工具链、MSRV、Cargo lock 与 CI 命令。
- 受影响文件：`src/app.rs`、`src/app/`、`src/sftp/transfer.rs`、`ui/{app,workspace-shell,session-editor}.slint`、`docs/`。
- 更新后的命令或环境：Rust 2024、MSRV 1.92.0、Cargo locked/offline 和三平台 CI 保持不变；本机仍使用直接 `rustfmt` 替代缺失的 Cargo fmt，Clippy 继续由安装组件的 CI 执行。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 137、应用 104、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 通过；目标平台 GUI、冷启动和 RSS 趋势留给用户人工验收。

## 2026-08-09 增加运行时 SSH agent 认证环境

- 日期：2026-08-09
- 变化摘要：基于已锁定的 russh 0.62.2 agent client 和外部 signer API 增加按连接 SSH agent 认证；未新增依赖，未改变 Rust edition、MSRV、Cargo lock 或 CI 命令。
- 受影响文件：`src/{config,ssh}.rs`、`src/app/`、`ui/session-editor.slint`、双语入口/usage/architecture/development、环境记录与项目实施记录。
- 更新后的命令或环境：Unix/macOS 运行时需可用的 `SSH_AUTH_SOCK`；Windows 使用该变量或 OpenSSH 默认 named pipe。agent 操作由 SSH worker 独占，最多尝试 5 个 identity，并受 30 秒总认证上限约束；profile 不保存 socket、identity 或秘密。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 106、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 通过；Cargo fmt/Clippy 因本机未安装对应子命令无法执行。内存 agent signer + loopback russh 已验证精确主机密钥后的真实外部签名；真实目标平台 agent 解锁/确认、多 identity 和失败恢复仍需用户手工验收，Windows 分支未在本机编译。
## 2026-08-09 增加 Slint 多窗口工作区环境事实

- 变化摘要：核对锁定的 Slint 1.17.1 本地 API，确认多个 `AppWindow` 可共享同一 UI event loop；`ComponentHandle::show/hide` 用于 detached 生命周期，`Window::on_close_requested` 可在隐藏前合并 workspace route。未新增依赖、工具链或 Cargo.lock 变化。
- 受影响文件：`src/app.rs`、`src/app/state.rs`、`src/app/view.rs`、`src/app/workspace.rs`、`src/app/terminal_bridge.rs`、`src/app/sftp_bridge.rs`、`ui/app.slint`、双语文档和实施记录。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、locked/offline check/test；`cargo fmt` 与 `cargo clippy` 子命令仍由 CI/已安装组件提供，本机无法执行。
- 验证结果：`cargo check --locked --offline` 和两项 workspace transfer focused tests 通过；真实 macOS/Windows/Linux 原生窗口、关闭/合并、焦点、拖动和不重连行为需目标平台手工验收。

## 2026-08-09 记录终端窗格拆分环境基线

- 日期：2026-08-09
- 变化摘要：主窗口和 detached Terminal 现在使用同一窗口级窗格布局与 UUID 定向终端 callback；没有新增 crate、修改 `Cargo.toml`/`Cargo.lock`、调整 Rust edition/MSRV 或 CI 命令。
- 受影响文件：`src/app.rs`、`src/app/`、`ui/{app,workspace-shell,terminal-pane}.slint`、`docs/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0 和 locked/offline Cargo 门禁；本机缺少 Cargo fmt/Clippy 子命令时，使用直接 `rustfmt --edition 2024` 检查已改 Rust 文件。
- 验证结果：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 116、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 通过。`cargo fmt` 与 `cargo clippy` 因本机没有对应子命令无法执行；目标平台 GUI/真实 SSH、Telnet、Serial 生命周期验收仍待用户完成。

## 2026-08-10 启用 macOS image-only 返回图标 API

- 日期：2026-08-10
- 变化摘要：在已锁定的 `objc2-app-kit 0.3.2` 上启用 `NSCell` feature，使 detached 标题栏按钮可使用 `NSCellImagePosition::ImageOnly`；没有新增 crate、升级版本或修改 `Cargo.lock`。
- 受影响文件：`Cargo.toml`、`src/app/macos_window.rs`、双语多窗口说明和项目记录。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、macOS 11.0 与 locked/offline 门禁；按钮优先使用 SF Symbol `arrow.uturn.backward`，并回退到 AppKit `NSImageNameGoBackTemplate`。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 121、Doc tests 0）、tracker validator、44 个 Markdown 相对链接和 `git diff --check` 通过；`cargo fmt` 与 `cargo clippy` 因本机没有对应子命令无法执行。真实标题栏图标、Tooltip、无障碍读取与点击行为需目标 macOS 用户验收。

## 2026-08-10 记录可调 Terminal pane divider 环境

- 日期：2026-08-10
- 变化摘要：Terminal `PaneTree` 增加当前运行期的有界 split ratio/divider 快照，Slint 增加主/独立窗口共用的拖拽、键盘和无障碍 divider；未新增依赖，未修改 `Cargo.toml`、`Cargo.lock`、Rust edition、MSRV 或 CI 命令。
- 受影响文件：`src/app.rs`、`src/app/{panes,terminal_bridge,view}.rs`、`ui/{app,workspace-shell,theme}.slint`、双语多 pane 说明和项目记录。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；本机仍以直接 Rustfmt 代替缺失的 Cargo fmt，Clippy 由安装组件的 CI 执行。
- 验证结果：6 项 pane 定向测试、比例跨 Tab/detach/return 路由测试、直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 124、Doc tests 0）、tracker validator、44 个 Markdown 相对链接和 `git diff --check` 通过。`cargo fmt` 与 `cargo clippy` 因本机没有对应子命令无法执行；真实 divider 对比度、宽高拖动、键盘焦点、无障碍读取和 PTY resize 需目标平台人工验收。

## 2026-08-10 记录跨平台 Terminal Edit 菜单环境

- 日期：2026-08-10
- 变化摘要：主窗口 Edit 增加只作用于 focused Terminal pane 的 Copy/Paste/Select All，detached Terminal 增加相同直接键盘路径；复用锁定的 Slint 1.17.1、现有 shortcut parser 和 clipboard callback，没有新增运行时依赖或外部服务。
- 受影响文件：`src/app/{input,view,diagnostics}.rs`、`ui/{app,workspace-shell,terminal-pane}.slint`、双语架构/使用说明和项目记录。
- 更新后的命令或环境：Rust 2024、MSRV 1.92.0、Cargo locked/offline 与三平台 CI 命令不变；Select All 固定为 macOS `Cmd+A`、Windows/Linux `Ctrl+Shift+A`，不增加配置 schema。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、3 项定向测试、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 通过；本机仍缺 Cargo fmt/Clippy 子命令，完整测试和目标平台原生菜单/焦点行为待最终门禁。

## 2026-08-10 完成跨平台 Terminal Edit 菜单环境门禁

- 日期：2026-08-10
- 变化摘要：完成 Terminal-only Edit menu、focused pane 本地命令路由和 detached 直接快捷键的完整 locked/offline 验证；环境事实与预检一致。
- 受影响文件：`src/app/{input,view,diagnostics}.rs`、`ui/{app,workspace-shell,terminal-pane}.slint`、双语架构/使用说明和项目记录。
- 更新后的命令或环境：依赖、锁文件、Rust edition/MSRV、配置 schema 和 CI 命令均不变；本机继续用直接 Rustfmt 覆盖格式检查，Cargo fmt/Clippy 由安装相应组件的环境执行。
- 验证结果：直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 125、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 通过。`cargo fmt` 与 `cargo clippy` 因本机没有对应子命令无法执行；三平台原生菜单显示、focused pane 定向和 detached 快捷键需用户人工验收。

## 2026-08-11 记录 Terminal divider drag/focus 修复环境

- 日期：2026-08-11
- 变化摘要：divider 几何改为对 UUID、divider identity/方向和 model 行数均匹配的 Slint `ModelRc` 原地更新，保持拖动中的 repeater 实例；鼠标 release/cancel 后以本地有界 revision 恢复 focused、connected terminal 的 IME 输入焦点。未新增依赖或外部服务，也未修改 Rust edition、MSRV、`Cargo.toml`、`Cargo.lock`、配置 schema、CI、worker 或 SSH 安全边界。
- 受影响文件：`src/app.rs`、`src/app/{terminal_bridge,view}.rs`、`ui/{workspace-shell,terminal-pane}.slint`、双语 architecture/usage、项目跟踪与环境记录。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；本机仍以直接 Rustfmt 代替缺失的 Cargo fmt，Clippy 由安装组件的 CI 执行。
- 验证结果：`cargo check --locked --offline`、2 项 `app::view::tests::terminal_pane_layout` 定向测试及完整 `cargo test --locked --offline` 通过（库 141、应用 127、Doc tests 0）；直接 Rustfmt、tracker/Markdown/diff 门禁和目标平台连续拖动、焦点恢复、键盘/无障碍 divider、PTY resize 仍待本轮收口/用户验收。

## 2026-08-11 完成 Terminal divider drag/focus 环境门禁

- 日期：2026-08-11
- 变化摘要：已完成原地 Slint model 更新与鼠标 drag 后 IME focus 恢复的完整本机门禁；环境、依赖、配置 schema、工具链和 CI 契约均无变化。
- 受影响文件：`src/app.rs`、`src/app/{terminal_bridge,view}.rs`、`ui/{workspace-shell,terminal-pane}.slint`、双语 architecture/usage、项目跟踪与环境记录。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；本机直接 Rustfmt 已通过，Cargo fmt/Clippy 仍由安装组件的 CI/目标环境执行。
- 验证结果：`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 127、Doc tests 0）、`rustfmt --edition 2024 --check`、tracker validator、44 个 Markdown 相对链接目标和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 子命令未安装；真实主/独立窗口连续拖动、焦点恢复、键盘/无障碍 divider 和 PTY resize 仍需用户验收。

## 2026-08-11 记录独立 Terminal 分屏入口环境

- 日期：2026-08-11
- 变化摘要：独立 Terminal 的紧凑分屏工具栏复用锁定 Slint 1.17.1 的可访问图标组件和既有 `pane-command` callback；未新增 crate、版本升级、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变更。
- 受影响文件：`ui/{components/workspace-titlebar,workspace-shell}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 Cargo locked/offline 门禁；工具栏只发出既有 pane intent，不持有 `PaneTree`、Tokio worker、SSH transport 或凭据。
- 验证结果：`cargo check --locked --offline`、`cargo test --locked --offline`、46 个 Markdown 相对链接目标与 `git diff --check` 通过。`cargo fmt` 与 `cargo clippy` 子命令未安装，需 CI 补充；真实 detached 窗口控件可见性、点击、焦点和 PTY resize 仍需用户验收。

## 2026-08-11 记录工作区窗口与分屏 glyph 环境

- 日期：2026-08-11
- 变化摘要：Tab Move/Return 和 Terminal split 图标统一为锁定 Slint 1.17.1 的 Rectangle/color glyph；没有新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置、工具链或 CI 变化。
- 受影响文件：`ui/{components/workspace-titlebar,theme}.slint`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁。绘图仅在 UI 线程本地发生，既有 callback、`WindowRouter`、Tokio worker、SSH transport 与凭据边界不变。
- 验证结果：`cargo check --locked --offline`、`cargo test --locked --offline`、46 个 Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 子命令未安装；目标平台图标锐度、方向可辨性、hover/focus 与原生 Return 协调性仍需用户验收。

## 2026-08-11 记录 detached 标题栏分屏入口环境

- 日期：2026-08-11
- 变化摘要：detached Terminal 删除客户区分屏条，改在 macOS 原生标题栏的 Return 左侧使用 image-only 分屏按钮；返回按钮继续使用系统符号与 AppKit 模板回退。未新增 crate、版本升级、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变更。
- 受影响文件：`src/{app,app/macos_window}.rs`、`ui/workspace-shell.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、`objc2-app-kit 0.3.2` 和 locked/offline Cargo 门禁；原生 button 只经 weak `AppWindow` 发出已有 pane intent，不持有 `PaneTree`、Tokio worker、SSH transport 或凭据。
- 验证结果：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 145、应用 134、Doc tests 0）和 `git diff --check` 通过。`cargo fmt` 与 `cargo clippy` 子命令未安装，需 CI 补充；目标 macOS 标题栏图标、Tooltip、点击、焦点和 PTY resize 仍需用户验收。

## 2026-08-11 记录 Terminal URL 与路径目标环境

- 目的：为可见 Terminal 内容增加受平台主修饰键保护的 URL/远端路径打开，并确保 SFTP 路由不改变已声明的安全与运行环境。
- 改动范围：`src/terminal.rs`、`src/app/{terminal_targets,terminal_bridge,sftp_bridge,connection}.rs`、`src/app/state/`、`ui/{app,workspace-shell,terminal-pane,components/terminal-grid}.slint`、双语契约和项目跟踪记录。
- 执行内容：复用锁定的 Slint 1.17.1 pointer modifier callback、已有 `open` crate、现有 SSH/SFTP companion UUID 路由和 SFTP-only worker；不新增依赖或外部服务。可见行仅做有界解析，宽字符 cell 坐标在终端模型内映射；URL 不被 AxSSH 请求，新的 SFTP Tab 仍是独立 SSH transport 并走正常 host-key/认证。
- 验证结果：直接 `rustfmt --edition 2024 --check`、4 项 `terminal_targets` parser 测试、`visible_row_target_text_preserves_cell_columns_after_wide_characters` 和 `targeted_sftp_companion_path_stays_on_the_runtime_tab` 通过。完整 locked/offline check/test、Markdown/tracker/diff 门禁待本轮收口；本机 `cargo fmt`/`cargo clippy` 子命令未安装。
- 风险/待办：目标 macOS/Windows/Linux 的 `Cmd/Ctrl` hover/click、默认 URL opener、主机密钥确认、认证和远端路径实际目录需用户验证；连接中的 companion 至多保留一个运行时路径，绝不持久化。

## 2026-08-11 完成 Terminal URL 与路径目标环境门禁

- 目的：确认 URL/远端路径打开不改变已声明的依赖、构建或 SSH 安全环境。
- 改动范围：`src/terminal.rs`、`src/app/`、`ui/`、双语契约和项目/环境记录。
- 执行内容：执行直接 Rustfmt、locked/offline Cargo check/test、tracking validator、Markdown 相对链接与差异检查；保留本机缺失 Cargo fmt/Clippy 的事实，不安装或升级组件。
- 验证结果：`cargo check --locked --offline` 通过；完整 `cargo test --locked --offline` 通过（库 146、应用 139、Doc tests 0）；tracker validator、Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`、`cargo clippy` 因本机无子命令无法执行；直接 `rustfmt --edition 2024 --check` 通过。
- 风险/待办：CI 或具备组件的环境补充 Cargo fmt/Clippy。真实 `Cmd/Ctrl` hover/click、默认 URL opener、host-key 确认、认证和远端目录需用户在目标平台验收。

## 2026-08-11 记录 SFTP 当前路径复制按钮环境

- 目的：为 SFTP 双栏目录标题提供路径复制，而不扩大 UI、worker 或 SSH transport 边界。
- 改动范围：`ui/{components/sftp-controls,sftp-pane,workspace-shell}.slint`、双语 SFTP 契约和项目/环境记录。
- 执行内容：复用锁定 Slint 1.17.1 和已有系统剪贴板 callback；Remote/Local 按钮只转发当前有界路径，主窗口与 detached SFTP 共用接线。未新增依赖或外部服务，也未变更 Rust edition、MSRV、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI。
- 验证结果：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 146、应用 139、Doc tests 0）、tracker validator、Markdown 相对链接与 `git diff --check` 通过。本机 `cargo fmt`/`cargo clippy` 子命令未安装；未自行截图，目标平台布局、Tooltip、焦点和剪贴板结果留用户验收。
- 风险/待办：CI 或具备组件的环境补充 Cargo fmt/Clippy；不对无图形环境中的系统剪贴板行为作推断。

## 2026-08-12 记录 Terminal target hover 下划线环境

- 目的：确认实时 URL/路径下划线只扩展现有 Rust/Slint UI 合同，不改变运行环境、依赖或 SSH/SFTP 安全边界。
- 改动范围：`src/{terminal.rs,app/{terminal_targets,terminal_bridge}.rs}`、`ui/{app,workspace-shell,terminal-pane,components/terminal-grid}.slint` 和双语契约。
- 执行内容：复用锁定 Slint 1.17.1、已有 `open` crate 与 SFTP companion 路由；新增的 `TerminalTargetHighlight` 只携带 active、row、半开 cell 区间。修正 Slint Apple modifier 映射，使 macOS `Cmd` 与其它平台 `Ctrl` 均使用 `control` 字段；不新增 crate、外部服务、配置字段或 CI 命令。
- 验证结果：`cargo check --locked --offline` 和 parser 定向测试已通过；完整 Cargo test、直接 Rustfmt、tracker/Markdown/diff 门禁待执行。本机 `cargo fmt`/`cargo clippy` 子命令未安装，目标平台 GUI hover/click 待用户确认。

## 2026-08-12 完成 Terminal target hover 下划线环境门禁

- 变化摘要：确认实时完整目标下划线不改变 Rust 2024、MSRV、Cargo 依赖、Slint 版本、配置 schema、CI 命令、SSH trust、凭据或 worker 所有权。
- 受影响文件：`src/terminal.rs`、`src/app/{terminal_targets,terminal_bridge}.rs`、`ui/{app,workspace-shell,terminal-pane,components/terminal-grid}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；本机以直接 Rustfmt 覆盖缺失的 Cargo fmt，Clippy 留给安装组件的 CI 或目标环境。
- 验证结果：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、6 项 parser/修饰键定向测试、1 项宽字符 cell span 测试、完整 `cargo test --locked --offline`（库 147、应用 141、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 因子命令未安装无法执行；目标平台 Cmd/Ctrl hover/click、下划线呈现和实际 opener/SFTP 认证由用户验收。

## 2026-08-12 记录 Terminal 语义高亮环境

- 日期：2026-08-12
- 变化摘要：在已有 UI 独立终端渲染层增加有界语义色，不改变依赖、锁文件、Rust edition、MSRV、Slint 版本、配置 schema、工具链或 CI 命令。
- 受影响文件：`src/app/terminal_render.rs`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；render mapper 只读取已有有界 snapshot，默认 ASCII run 才拆分为 URL/路径、HTTP 和状态词色，不访问 Slint、worker、SSH/SFTP transport 或持久化。
- 验证结果：直接 `rustfmt --edition 2024` 与 8 项 `terminal_render` 定向测试通过；完整 `cargo check --locked --offline`、`cargo test --locked --offline`、tracker、Markdown 和 diff 门禁待本轮收口。`cargo fmt`/`cargo clippy` 子命令仍未安装。

## 2026-08-12 完成 Terminal 语义高亮环境门禁

- 日期：2026-08-12
- 变化摘要：完成可见 Terminal 默认 ASCII run 语义色的离线编译和回归门禁；环境、依赖、锁文件、配置 schema、工具链与 CI 契约仍无变化。
- 受影响文件：`src/app/terminal_render.rs`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 命令；本机以直接 Rustfmt 覆盖缺失的 Cargo fmt，Clippy 由具备组件的 CI 或目标环境执行。
- 验证结果：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 147、应用 144、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过；`cargo fmt`/`cargo clippy` 子命令未安装，GUI 颜色/交互需用户验收。

## 2026-08-12 Terminal 语义高亮可见性修复环境门禁

- 日期：2026-08-12
- 变化摘要：以真实 `TerminalModel` snapshot 验证默认 cell 至 Slint render run 的语义色传递，并将有界默认 ASCII run 扩展为链接、成功、信息、警告和错误五类；不改变显式 ANSI/真彩色、非默认背景、inverse、dim 或非 ASCII 输出。
- 受影响文件：`src/app/terminal_render.rs`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；没有新增依赖、工具链、配置 schema、worker、SSH trust 或凭据合同。
- 验证结果：直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 147、应用 145、Doc tests 0）通过。`cargo fmt`/`cargo clippy` 因子命令未安装无法执行；目标平台主题下的颜色层次和 ANSI/真彩色保留由用户验收。

## 2026-08-12 Terminal Tab 焦点与窗口框线环境记录

- 日期：2026-08-12
- 变化摘要：在 Slint UI 层将 Terminal pane 的 focused 边框移除，改由 `AppWindow` 绘制唯一客户区框线；Tab/pane identity、visible、connected 或 focused 变化后的透明 IME proxy 聚焦延迟到下一次 UI tick，并再次验证状态。
- 受影响文件：`ui/{app,terminal-pane}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；不新增依赖、配置 schema、工具链、CI、Rust callback、worker、SSH trust 或凭据边界。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）和 `git diff --check` 通过。严格 Clippy 仍被范围外既有 4 项 lint 阻断，目标平台 Tab 切换首次输入和主/独立窗口框线由用户验收。

## 2026-08-12 日期化 GitHub 发布环境记录

- 日期：2026-08-12
- 变化摘要：新增按上海日期同步的 GitHub 多平台发布工作流。CI 成功后使用 `Swatinem/rust-cache` 为每个 Rust target 保存 cache；Release 在入口再次验证相同 tag 的成功 CI，只恢复同 target cache 并始终重新构建 `--release --locked`，不会发布 CI check/debug 产物或中间 macOS 架构二进制。
- 受影响文件：`.github/workflows/{ci,create-dated-release,release}.yml`、`scripts/{release_version,test_release_version}.py`、`Cargo.{toml,lock}`、`packaging/macos/{Info.plist,build-app.sh}`、发布文档和项目/环境记录。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、现有 `Cargo.lock` 和 Python 3 标准库。远端 CI 使用 Windows x86_64、Linux x86_64/aarch64 与 macOS arm64/x86_64 runner；Linux 额外安装 `pkg-config`、fontconfig 和 xkbcommon 开发包。
- 验证结果：Python release-version 3 项回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）、macOS plist/shell 和 YAML 静态检查通过。严格 Clippy 因当前工作区现有的 `src/ssh/x11.rs` 两项 byte-slice lint，以及 `src/app/{terminal_bridge,view/terminal}.rs` 两项测试模块顺序 lint 失败；真实 GitHub Actions、cache hit、GitHub Release 与 macOS Gatekeeper 验证待远端执行。

## 2026-08-12 GitHub Release Highlights 施工前预检

- 目的：在为日期化 GitHub Release 增加自定义 Highlights 正文前，确认项目环境、发布边界和验证命令。
- 改动范围：`.github/workflows/release.yml`、`scripts/`、`README{,.zh}.md`、`docs/development{,.zh}.md` 与发布相关跟踪记录。
- 执行内容：复核 `Cargo.toml`、`Cargo.lock`、发布版本脚本、三个 GitHub Actions workflow 和现有环境当前态；确认项目继续使用 Rust 2024、Cargo、锁定依赖、Python 标准库脚本和 `unittest`，本轮不变更工具链、依赖、CI 构建矩阵、tag/CI 门禁或发行包内容。
- 验证结果：预检通过；现有 release job 已使用 `generate_release_notes: true`，但尚未生成或传入自定义 `body_path`。后续将验证 Python 定向测试、YAML/Shell、Markdown、tracker、`cargo check --locked --offline` 和 `git diff --check`；真实 GitHub Release 页面拼接需在远端 tag 发布时确认。
- 风险/待办：提交主题关键词归类是启发式的，未归类或文档类提交仍由 GitHub 自动 release notes 保留完整记录。

## 2026-08-12 GitHub Release Highlights 环境更新

- 目的：记录自定义 Release Highlights 对 CI 与测试环境的实际影响。
- 改动范围：`.github/workflows/{ci,release}.yml`、`scripts/{generate_release_highlights,test_generate_release_highlights}.py` 与发布文档/跟踪记录。
- 执行内容：CI 的 Python 测试步骤扩展为 release-version 和 Git-backed Highlights 两组 `unittest`；新增脚本只用 Python 标准库、`Path` 和已检出的 Git 历史生成 Markdown，不改 Rust 依赖、工具链、Cargo 锁文件、构建矩阵、日期 tag、CI 成功门禁、cache 策略或发行包内容。
- 验证结果：Python 9 项定向回归与编译、YAML/Shell、11 个相关 Markdown 链接、tracker、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 150、应用 152、Doc tests 0）和差异检查通过；远端 GitHub Release 正文拼接、三平台构建和资产发布仍需下一次日期 tag 验证。
- 风险/待办：分类依赖提交主题关键词；未分类提交仍由 GitHub 自动 release notes 保留，workflow 只在已检出的严格日期 tag 上执行。
## 2026-08-12 分屏即时 IME 焦点环境记录

- 日期：2026-08-12
- 变化摘要：将 `TerminalPane` 的通用 1ms IME 聚焦计时器收窄为仅在组件 `init` 时执行的一次首次布局重试。复用组件的 terminal identity、`focused`、连接、可见性和 divider release revision 直接聚焦已存在的透明 `TextInput` proxy，因此不会在 Tab、`Alt+H/J/K/L` 或鼠标切换分屏后人为等待下一轮 UI tick。
- 受影响文件：`ui/terminal-pane.slint`、`docs/{architecture,architecture.zh,usage,usage.zh,project-implementation-tracker/{current,project-map,changes/2026/08}}.md`、`docs/project-env-audit/{current,changes}.md`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；没有新增依赖、配置 schema、Rust callback、worker、SSH trust 或凭据边界。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 149、Doc tests 0）、Markdown 相对链接和 `git diff --check` 通过。严格 Clippy 被范围外既有四项 lint 阻断；无窗口自动化测试不能直接断言原生 IME，主/独立窗口分屏切换、新建 pane、Tab 切换和 divider release 由目标平台验收。
## 2026-08-12 界面语言与内嵌目录环境变化

- 日期：2026-08-12
- 变化摘要：新增 `sys-locale 0.3.2` 直接依赖和 Slint bundled `zh-CN` 翻译目录，配置 schema 从 v20 升至 v21；Rust edition、MSRV、Slint/Tokio/russh 版本和 CI 命令不变。
- 受影响文件：`Cargo.{toml,lock}`、`build.rs`、`src/config/`、`src/app/`、`ui/`、`translations/`、`scripts/{build_zh_catalog,check_translations}.py` 及相关文档。
- 更新后的命令或环境：locked/offline Cargo 可从现有缓存解析全部依赖；翻译目录使用 Python 3 生成/覆盖/占位符检查，并可用 GNU gettext `msgfmt` 验证 PO 格式。
- 验证结果：定向配置/Settings/迟到语言请求测试、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 150、应用 152、Doc tests 0）、386 条翻译覆盖、占位符、`msgfmt`、Python 编译、46 项 Markdown 相对链接和差异空白检查通过；tracker validator 只剩 16 条旧月度记录时间字段错误。
- 风险/待办：系统 locale 与 GUI 即时切换由 Windows/macOS/Linux 目标平台验收；运行时技术错误详情保持原文，不通过字符串替换伪本地化。
## 2026-08-12 本地 SFTP 文件指纹环境更新

- 目的：记录本地只读打开的身份验证从单一平台 identity 强化为 identity 加元数据指纹后的环境影响。
- 改动范围：`src/app/local_files.rs`、`docs/{architecture,architecture.zh,development,development.zh}.md` 与环境/实施跟踪记录。
- 执行内容：目录读取与打开后 handle 重验继续完全在 blocking worker 中进行；指纹包含平台文件 identity、长度、修改时间与创建时间。未新增 crate、修改 `Cargo.toml`/`Cargo.lock`、工具链、CI、SFTP 协议、SSH trust 或凭据。
- 验证结果：`app::local_files` 7 项定向测试、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 150、应用 153、Doc tests 0）、tracker、Markdown 链接和差异检查通过。
- 风险/待办：外部文件系统可在验证后的 handle 复制期间继续修改内容，但路径替换不能重定向 opener；复制保持从已验证 handle 读取，Windows/Linux 真实文件系统替换场景仍待 CI/平台验证。
