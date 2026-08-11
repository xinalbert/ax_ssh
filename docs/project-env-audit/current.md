# 项目环境当前态

## 项目类型

- 独立 Rust 2024 桌面应用，根 Cargo 包为 `ax_ssh`。
- 项目边界：`<repo-root>`；`third_package/axshell` 仅是参考资料，不进入构建图。

## 运行环境

- Rust edition：2024。
- MSRV：Rust 1.92.0。
- 本机工具链：`rustc 1.96.1`、`cargo 1.96.1`。
- UI/运行时：Slint 1.17.1、Tokio 1、russh 0.62.2、russh-sftp 2.3.0、libmudtelnet-rs 2.0.10、tokio-serial 5.5.0；Slint 1.17.1 的 `ComponentHandle::show/hide`、`Window::on_close_requested`、多个 `AppWindow` 实例和共享 `slint::run_event_loop()` 已在本轮本地 crate 源码核对并由 `cargo check` 编译；russh 已锁定版本内建 Unix/macOS `SSH_AUTH_SOCK` 和 Windows named-pipe agent client 及外部 signer API，不需要新增依赖；Tokio 启用 `process` 供有界 `xauth` 子进程调用，`rand 0.10` 是运行时依赖，用于生成单次 X11 fake cookie；macOS 已锁定的 `objc2-app-kit 0.3.2` 启用 `NSWorkspace` 系统应用发现，并启用 `NSCell` 以配置 detached 标题栏的 image-only 返回图标；Slint 已启用 `unstable-fontique-010` 运行时字体注册，并直接依赖锁定的 `fontdb 0.23.0` 扫描系统等宽字体。
- 依赖管理：Cargo，锁文件为 `Cargo.lock`。

## 测试环境

- 单元与集成测试：Cargo 原生测试，包括 config、terminal、应用状态、本地 PTY、loopback SSH/Telnet、内存 SSH agent protocol + 外部 signer、SFTP packet/path/state 边界和 Serial descriptor 匹配。
- CI：GitHub Actions 在 Ubuntu、macOS、Windows 上运行 format、check 和 test。
- 本机未安装 `cargo fmt` 与 `cargo clippy` 子命令；仓库仍声明并由 CI 执行格式、检查和测试门禁。

## 关键命令

```bash
cargo run --locked
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

## 外部依赖

- Slint 桌面后端需要目标平台图形环境。
- Linux CI 安装 `libfontconfig1-dev` 和 `libxkbcommon-dev`。
- 系统凭据集成测试可能触发平台授权，默认 ignored。
- SSH agent 认证在连接时读取当前运行环境：Unix/macOS 使用 `SSH_AUTH_SOCK`，Windows 使用该变量或 OpenSSH 默认 named pipe；AxSSH 最多尝试 5 个 identity，并对 agent 连接、列举、协商、签名和认证应用 30 秒总上限。自动测试使用内存 agent，不访问系统 agent；真实解锁/确认、多 identity 和失败行为需在目标平台手工验证。
- Serial 实机测试依赖目标平台驱动、设备权限和硬件；自动测试不打开真实串口。
- `tokio-serial 5.5.0` 声明 MSRV 1.71；`libmudtelnet-rs 2.0.10` 声明 MSRV 1.66，均低于项目 MSRV 1.92.0。
- `libmudtelnet-rs 2.0.10` 的跨调用不完整 IAC/协商帧和转义 IAC 存在已确认边界；本项目 64 KiB 有界分帧适配由逐字节与 Telnet loopback 回归覆盖。
- `russh-sftp 2.3.0` 未声明 MSRV，且 raw client 内部使用 unbounded packet sender；项目以 MSRV/CI、单浏览 session、串行请求、256 KiB 入站 frame、250 条分页和 2,000 条/2 MiB 目录预算约束其使用。
- 真实 SFTP 服务兼容性与 GUI 文件面板需要目标环境手工验证。
- X11 forwarding 依赖目标平台可用的本机 X server。普通 SSH shell 创建只发送 forwarding request，不读取本机 `DISPLAY`、不运行 `xauth`、不探测端点且不启动 provider；远端实际打开 X11 channel 后才进行本机准备。AxSSH 从 Settings 显示 macOS bundle identifier 或 Windows `PATH`/Program Files 检测到的只读已知位置，且仅在 Custom 时接受用户提供的 executable 路径。安全默认仍要求 local-only `DISPLAY` 和可查询精确 `MIT-MAGIC-COOKIE-1` 的 `xauth`。MacXServer 和自动启动的 VcXsrv/Xming 只有在显式 no-auth 兼容下使用 loopback/`-ac`。真实 XQuartz/MacXServer、X.Org/Xwayland、VcXsrv/Xming 行为需目标平台手工验证，AxSSH 不安装软件或修改远端 `sshd_config`。
- 自带 TTF 作为 `assets/fonts/` 运行时资源保留在发行包，不经 Slint import 嵌入可执行文件。系统字体扫描依赖 `fontdb` 的预定义目录，必须在 Tokio blocking worker 中执行；各平台真实可见字体和打包后 Resources 路径须手工验收。
- 最近一次完整 locked/offline 测试门禁通过：库测试 147 项、应用测试 141 项和 Doc tests 0 项均无失败；SFTP 当前路径复制按钮及 Terminal target 实时下划线均已通过完整 Slint 编译图。本轮没有新增 crate、版本升级、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 契约变化。Cargo fmt/Clippy 子命令仍缺失，直接 `rustfmt` 是本机格式回退；目标平台 GUI、剪贴板和真实连接行为仍需人工确认。

## 证据文件

- 2026-08-10 macOS detached 返回控件已从 58px 文字按钮改为 28px image-only 系统图标，带模板 fallback、Tooltip 与无障碍描述；完整 `cargo test --locked --offline` 通过（库 141、应用 121、Doc tests 0），`Cargo.lock` 未改变。`cargo fmt`/`cargo clippy` 子命令本机未安装，真实标题栏显示和操作需用户验收。
- 2026-08-10 一个可见 Terminal Tab 管理多 pane 的路由已通过 `cargo check --locked --offline` 重新编译完整 Slint 图和完整 `cargo test --locked --offline`（库 141、应用 121、Doc tests 0）；没有 Cargo 依赖、锁文件、工具链或 CI 契约变化。`cargo fmt`/`cargo clippy` 子命令本机未安装；目标平台 Tab/pane 可见性、焦点、点击/快捷键、detached Return 和实际 SSH/Telnet/Serial 生命周期仍需用户验收。
- 2026-08-10 Terminal pane divider 的 0.1-0.9 有界比例、嵌套 identity 和跨 Tab/detach/return 生命周期已通过定向测试及完整 `cargo test --locked --offline`（库 141、应用 124、Doc tests 0）；主窗口和 detached 窗口共用同一 Rust-owned `PaneTree` 与 Slint divider。没有环境、依赖或安全边界变化；真实拖动、键盘/无障碍操作和 PTY resize 需用户验收。
- 2026-08-10 Terminal Edit Copy/Paste/Select All 复用现有 Slint 1.17.1 `MenuItem.shortcut`、配置快捷键 parser 和主/独立窗口共用的 `TerminalPaneGroup`；没有新增 crate、配置字段、工具链、CI 或 transport 要求。直接 Rustfmt、`cargo check --locked --offline`、3 项定向测试、完整 `cargo test --locked --offline`（库 141、应用 125、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 已通过；目标平台原生菜单/焦点验收仍待用户完成。
- 2026-08-11 Terminal divider drag/focus 修复继续使用锁定 Slint 1.17.1 的稳定 `ModelRc::set_row_data` 和本地有界 revision；不新增依赖、不变更配置 schema、工具链、CI、worker 或 SSH 安全边界。`cargo check --locked --offline`、2 项 model identity 定向测试和完整 `cargo test --locked --offline`（库 141、应用 127、Doc tests 0）通过；直接 Rustfmt、tracker/Markdown/diff 门禁与目标平台 GUI 验收待本轮收口。
- 2026-08-11 detached Terminal 分屏入口复用锁定 Slint 1.17.1 的组件导出和既有 `pane-command` callback；没有新增 crate、配置字段、工具链、CI、worker 或 SSH 安全边界。`cargo check --locked --offline`、完整 `cargo test --locked --offline`、46 个 Markdown 相对链接和 `git diff --check` 通过；`cargo fmt`/`cargo clippy` 子命令仍未安装，真实 detached 工具栏可见性、点击、焦点和 PTY resize 需目标平台验收。
- 2026-08-11 工作区窗口与分屏 glyph 统一只使用锁定 Slint 1.17.1 的本地 Rectangle/color API；没有新增 crate、配置、工具链、CI、worker 或 SSH 边界变化。`cargo check --locked --offline`、完整 `cargo test --locked --offline`、46 个 Markdown 相对链接和 `git diff --check` 通过；`cargo fmt`/`cargo clippy` 子命令仍未安装，目标平台图标锐度、方向可辨性和原生 Return 协调性待用户验收。
- 2026-08-11 detached Terminal 分屏入口改为 macOS 原生标题栏 image-only 按钮；继续使用锁定 `objc2-app-kit 0.3.2`、Slint 1.17.1、Rust 2024 和 MSRV 1.92.0。未新增 crate、配置 schema、工具链、CI、worker、SSH trust 或凭据边界。直接 `rustfmt`、`cargo check --locked --offline` 和完整 `cargo test --locked --offline`（库 145、应用 134、Doc tests 0）通过；`cargo fmt`/`cargo clippy` 子命令缺失，目标 macOS 标题栏和 PTY resize 待用户验收。
- 2026-08-11 Terminal URL/路径目标打开继续使用锁定的 Rust 2024、MSRV 1.92.0、Slint 1.17.1、已有 `open` crate 与 Cargo locked/offline 门禁；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。直接 `rustfmt`、4 项 parser、宽字符 cell 坐标和 runtime SFTP companion 定向测试、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 146、应用 139、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 子命令仍缺失；目标平台 `Cmd/Ctrl` hover/click、实际默认 opener 和 SFTP 认证由用户验收。
- 2026-08-11 SFTP 当前路径复制按钮继续使用锁定的 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和既有系统剪贴板 callback；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。复制只转发已发布的有界路径，不读取文件系统或接触 SSH/SFTP worker。直接 `rustfmt`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 146、应用 139、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 子命令仍缺失；目标平台标题栏布局、Tooltip、焦点和剪贴板结果由用户验收。
- 2026-08-12 Terminal target hover 下划线继续使用锁定的 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和已有 URL/SFTP 依赖；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。短暂 DTO 只转发 active、row 和半开 cell 区间，完整行、目标文本、worker 和 transport 不跨 UI 边界。直接 Rustfmt、`cargo check --locked --offline`、6 项 parser/修饰键定向测试、宽字符 cell span 测试、完整 `cargo test --locked --offline`（库 147、应用 141、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 已通过；`cargo fmt`/`cargo clippy` 子命令仍缺失，目标平台 Cmd/Ctrl hover/click 由用户验收。
- 2026-08-12 Terminal 语义高亮继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和既有终端 ANSI 色表；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。仅私有渲染映射对有界可见默认 ASCII run 着色，结果不跨 Slint/worker 边界；直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 147、应用 144、Doc tests 0）、tracker、Markdown 和 diff 门禁均已通过。

- `Cargo.toml`
- `Cargo.lock`
- `.github/workflows/ci.yml`
- `docs/development.md`
- `AGENTS.md`

## 最后确认时间

- 2026-08-12 01:25 CST
