# 项目环境变化记录

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
