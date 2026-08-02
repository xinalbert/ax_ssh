# 项目环境变化记录

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
