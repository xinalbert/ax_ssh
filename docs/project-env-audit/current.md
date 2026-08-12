# 项目环境当前态

## 项目类型

- 独立 Rust 2024 桌面应用，根 Cargo 包为 `ax_ssh`。
- 项目边界：`<repo-root>`；`third_package/axshell` 仅是参考资料，不进入构建图。

## 运行环境

- Rust edition：2024。
- MSRV：Rust 1.92.0。
- 本机工具链：`rustc 1.97.1`、`cargo 1.97.1`、`rustfmt 1.9.0`、`clippy 0.1.97`、Python 3.14.3。
- UI/运行时：Slint 1.17.1、Tokio 1、russh 0.62.2、russh-sftp 2.3.0、libmudtelnet-rs 2.0.10、tokio-serial 5.5.0；Slint 1.17.1 的 `ComponentHandle::show/hide`、`Window::on_close_requested`、多个 `AppWindow` 实例和共享 `slint::run_event_loop()` 已在本轮本地 crate 源码核对并由 `cargo check` 编译；russh 已锁定版本内建 Unix/macOS `SSH_AUTH_SOCK` 和 Windows named-pipe agent client 及外部 signer API，不需要新增依赖；Tokio 启用 `process` 供有界 `xauth` 子进程调用，`rand 0.10` 是运行时依赖，用于生成单次 X11 fake cookie；macOS 已锁定的 `objc2-app-kit 0.3.2` 启用 `NSWorkspace`、`NSCell` 和 `NSColor`，以配置 detached 标题栏的 image-only 返回图标及与 Terminal 连续的 sRGB 背景；Slint 已启用 `unstable-fontique-010` 运行时字体注册，并直接依赖锁定的 `fontdb 0.23.0` 扫描系统等宽字体。
- 依赖管理：Cargo，锁文件为 `Cargo.lock`。

## 测试环境

- 单元与集成测试：Cargo 原生测试，包括 config、terminal、应用状态、本地 PTY、loopback SSH/Telnet、内存 SSH agent protocol + 外部 signer、SFTP packet/path/state 边界和 Serial descriptor 匹配。
- CI：GitHub Actions 在 Ubuntu、macOS、Windows 上运行 format、check、test、发布元数据和 Git-backed Release Highlights 测试；tag 发布 workflow 在 GitHub-hosted Windows、Ubuntu x86_64/ARM64 与 macOS runner 上构建发行包。
- 本机已安装 `cargo fmt` 与 `cargo clippy` 子命令；CI 仍是目标平台构建与 GitHub Release 的执行位置。
- Terminal IME 焦点策略：复用 `TerminalPane` 组件的 terminal identity/focused/connected/visible/divider-release 转换同步聚焦既有透明 `TextInput` proxy；仅新建 pane 在首次布局后执行一次可见、focused、connected 重验，避免重建实例在未布局时取得无效原生焦点。

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
- Linux CI 安装 `pkg-config`、`libfontconfig1-dev` 和 `libxkbcommon-dev`。
- 系统凭据集成测试可能触发平台授权，默认 ignored。
- SSH agent 认证在连接时读取当前运行环境：Unix/macOS 使用 `SSH_AUTH_SOCK`，Windows 使用该变量或 OpenSSH 默认 named pipe；AxSSH 最多尝试 5 个 identity，并对 agent 连接、列举、协商、签名和认证应用 30 秒总上限。自动测试使用内存 agent，不访问系统 agent；真实解锁/确认、多 identity 和失败行为需在目标平台手工验证。
- Serial 实机测试依赖目标平台驱动、设备权限和硬件；自动测试不打开真实串口。
- `tokio-serial 5.5.0` 声明 MSRV 1.71；`libmudtelnet-rs 2.0.10` 声明 MSRV 1.66，均低于项目 MSRV 1.92.0。
- `libmudtelnet-rs 2.0.10` 的跨调用不完整 IAC/协商帧和转义 IAC 存在已确认边界；本项目 64 KiB 有界分帧适配由逐字节与 Telnet loopback 回归覆盖。
- `russh-sftp 2.3.0` 未声明 MSRV，且 raw client 内部使用 unbounded packet sender；项目以 MSRV/CI、单浏览 session、串行请求、256 KiB 入站 frame、250 条分页和 2,000 条/2 MiB 目录预算约束其使用。
- 真实 SFTP 服务兼容性与 GUI 文件面板需要目标环境手工验证。
- X11 forwarding 依赖目标平台可用的本机 X server。普通 SSH shell 创建只发送 forwarding request，不读取本机 `DISPLAY`、不运行 `xauth`、不探测端点且不启动 provider；远端实际打开 X11 channel 后才进行本机准备。AxSSH 从 Settings 显示 macOS bundle identifier 或 Windows `PATH`/Program Files 检测到的只读已知位置，且仅在 Custom 时接受用户提供的 executable 路径。安全默认仍要求 local-only `DISPLAY` 和可查询精确 `MIT-MAGIC-COOKIE-1` 的 `xauth`。MacXServer 和自动启动的 VcXsrv/Xming 只有在显式 no-auth 兼容下使用 loopback/`-ac`。真实 XQuartz/MacXServer、X.Org/Xwayland、VcXsrv/Xming 行为需目标平台手工验证，AxSSH 不安装软件或修改远端 `sshd_config`。
- 自带 TTF 作为 `assets/fonts/` 运行时资源保留在发行包，不经 Slint import 嵌入可执行文件。系统字体扫描依赖 `fontdb` 的预定义目录，必须在 Tokio blocking worker 中执行；各平台真实可见字体和打包后 Resources 路径须手工验收。
- 当前发布基线为 Cargo `2026.8.12`、公开 tag `2026-08-12`。`scripts/release_version.py` 与 `scripts/generate_release_highlights.py` 只依赖 Python 标准库；后者在已检出的 tag 历史上生成分类 Markdown，定向测试用临时 Git 仓库覆盖 tag range、去重、跟踪提交排除和失败路径。日期 workflow 使用 `Asia/Shanghai` 生成并验证 Cargo/lockfile/macOS plist 版本。日期 workflow 显式 dispatch tag CI；CI 在成功 default-branch 或已验证日期-tag run 后按 Rust target 保存共享 Cargo cache，Release 只读取对应 cache 并重新构建 `--release --locked` binary。真实 GitHub-hosted ARM、Windows 和 macOS build，以及 GitHub Release 仍需远端仓库权限和网络执行。

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
- 2026-08-12 Terminal 语义高亮可见性修复继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和既有终端 ANSI 色表；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。仅私有渲染映射对有界可见默认 ASCII run 着色，真实 `TerminalModel` snapshot 回归覆盖到 Slint render run 的五类语义色；直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 147、应用 145、Doc tests 0）通过。Cargo fmt/Clippy 子命令仍缺失，GUI 颜色与 ANSI/真彩色保留由用户验收。
- 2026-08-12 日期化发布链路使用 Rust 2024、MSRV 1.92.0、现有锁文件和 Python 3 标准库；新增 GitHub Actions target 专属 Cargo cache 复用、`YYYY-MM-DD` 到包版本映射和跨平台打包，不新增 Rust crate。`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）、Python release-version 3 项单元测试、plist/shell/YAML 和差异检查通过。严格 Clippy 被用户工作区的 `src/ssh/x11.rs` 两项和既有测试模块排列两项 lint 阻断；真实 GitHub-hosted 构建、发布和 macOS Gatekeeper 仍待远端执行。
- 2026-08-12 macOS detached 标题栏连续性修复继续使用锁定的 `objc2-app-kit 0.3.2`，仅额外启用其已锁定的 `NSColor` feature；没有新增 crate 或 lockfile 解析变化。AppKit 仅在 UI 主线程接收 Slint `Color` 值，使用透明标题栏、sRGB window background 和系统重叠窗口 symbol；Settings 仅沿 live weak UI route 同步既有外观配置，绝不携带 worker、terminal buffer、SSH trust 或凭据。`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 149、Doc tests 0）和 `git diff --check` 通过；严格 Clippy 和 tracker validator 分别被范围外既有 lint 与历史时间字段阻断。目标 macOS 标题栏材质、图标、Tooltip、Return 和主题预览同步仍待用户验收。

- `Cargo.toml`
- `Cargo.lock`
- `.github/workflows/ci.yml`
- `docs/development.md`
- `AGENTS.md`

## 本轮施工预检

- 项目边界：独立 Rust 桌面应用；本轮范围为 `src/config/`、`src/app/`、`ui/` 中的 Terminal 语义高亮配置链路。
- 环境记忆状态：已读取并与 `Cargo.toml`、`Cargo.lock`、`.github/workflows/ci.yml`、`AGENTS.md` 及当前源码路由复核。
- 运行环境：Rust 2024、MSRV 1.92.0、Slint 1.17.1；Cargo 及锁文件是唯一包管理与可复现构建来源。
- 测试环境：`cargo check --locked --offline`、`cargo test --locked --offline`、`cargo fmt --all -- --check`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`git diff --check`。
- 环境变化检查：否；本轮不变更依赖、工具链、CI、Cargo 文件、SSH trust 或凭据边界。
- 开工判定：允许开工；颜色仅作为受限持久化值经 AppWindow properties 传到 UI 独立 renderer。

## 本轮实施结果

- 目的：让 Terminal 的五类语义高亮色可由 Settings 配置，同时在所有主题背景上保持可读。
- 改动范围：`src/config/` 中的 schema v20 颜色值，`src/app/` 的 Settings/renderer bridge，以及 `ui/` 的 Terminal Settings 与 callback 中转。
- 执行内容：添加五项可选 `#RRGGBB` 颜色覆盖；空值或无效值跟随主题 ANSI 默认色；每项输出色仍按真实背景修正到至少 4.5:1。没有新增依赖、工具链、CI、worker、SSH trust 或凭据改动。
- 验证结果：`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）、直接 `rustfmt --edition 2024 --check` 和 `git diff --check` 通过。`cargo fmt` 与 `cargo clippy` 子命令未安装，无法在本机执行。
- 风险/待办：Settings 输入、主题切换和实际终端颜色层次需用户在目标图形平台确认；显式 ANSI/256/真彩、反色、dim、非默认背景和非 ASCII 文本仍由远端程序控制。

## 2026-08-12 Terminal Tab 焦点与窗口框线环境门禁

- 日期：2026-08-12
- 变化摘要：`TerminalPane` 使用 Slint-local 的 1ms focus pending timer，在 Tab/pane identity、visible、connected 或 focused 更新后等待当前 UI 更新完成，再确认可见、focused、connected 后聚焦透明 IME proxy；移除 pane 自身边框，`AppWindow` 在客户区绘制唯一的 `Theme.frame-border` 框线。
- 受影响文件：`ui/{app,terminal-pane}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；未新增 crate、配置 schema、工具链、CI、Rust callback、worker、SSH trust 或凭据边界。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）和 `git diff --check` 通过。严格 Clippy 被范围外 `src/ssh/x11.rs` 的两项 byte-slice lint 与 `src/app/{terminal_bridge,view/terminal}.rs` 的两项测试模块顺序 lint 阻断；目标平台 Tab 首次输入、分屏点击焦点和主/独立窗口框线留用户验收。

## 最后确认时间

- 2026-08-12 09:00 CST
- 2026-08-12 21:13 +0800：复核 release helper 扩展仍只使用 Python 标准库和 Git；CI 额外运行 Git-backed Highlights 回归，Rust 依赖、工具链、构建矩阵、日期 tag、CI 门禁和发行包内容未改变。
- 2026-08-12 22:20 +0800：本地 SFTP 只读打开的目录快照/打开重验从平台文件 identity 强化为 identity 加长度、修改时间和创建时间指纹；未改 Rust 依赖、工具链、CI、SSH trust 或凭据，目标是拒绝快速 ID 复用和原地修改。

## 2026-08-12 界面语言环境记录

- 日期：2026-08-12
- 变化摘要：新增直接依赖 `sys-locale 0.3.2`，并使用锁定 Slint 1.17.1 的 bundled translations 接口内嵌 `zh-CN` PO 目录；配置 schema 升至 v21。
- 受影响文件：`Cargo.{toml,lock}`、`build.rs`、`src/config/`、`src/app/`、`ui/`、`translations/`、翻译检查脚本和双语文档。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0 和 locked/offline Cargo 门禁；`sys-locale` 已存在本地锁定依赖缓存，目录额外使用 Python 3 检查，并在可用时执行 `msgfmt --check --check-format`。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 150、应用 152、Doc tests 0）、386 条目录覆盖/占位符检查、`msgfmt`、Python 编译、46 项 Markdown 相对链接和差异检查通过。新增请求代次回归证明迟到语言保存不能覆盖最新选择；tracker validator 只剩 16 条旧月度记录时间字段错误。目标平台 locale 检测、即时切换和多窗口视觉由用户验收。
