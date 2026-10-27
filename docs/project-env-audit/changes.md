# 项目环境变化记录

## 2026-10-08 russh 0.64.1 升级环境验证

- 日期：2026-10-08
- 目的：用户要求将 SSH transport 从 0.63 系列升级到当前 0.64.1，确认 MSRV、host-key 回调和 RSA 兼容边界。
- 改动范围：`Cargo.toml`、`Cargo.lock`、`.github/workflows/{ci,release}.yml`、双语架构、项目地图；不改 host-key trust、凭据生命周期、worker 所有权或 `third_package/axshell` 边界。
- 执行内容：将直接约束改为 `russh 0.64.1`，锁文件同步传递依赖；核对 `PublicKeyOrCertificate` callback、`rsa 0.10.0-rc.18` 固定关系及 Rust 1.89 依赖 MSRV；运行 fmt、locked check、严格 all-target Clippy、完整测试和 RustSec audit。
- 验证结果：Rust 1.97 的 `cargo check --locked`、fmt、严格 Clippy、完整测试（库 303、应用 312、Doc tests 0）和 `git diff --check` 通过；RustSec 仅报告既有允许级别的 `bincode`/`ttf-parser` 警告，并保留项目既有 `RUSTSEC-2023-0071` 忽略项；Windows/Linux 原生目标仍交 CI。
- 风险/待办：`russh 0.64.1` 仍使用 RSA 预发布实现及已记录的时序风险；真实 SSH 服务器、agent、SFTP 和非 macOS runner 互操作需目标环境验收。
- 开工判定：施工与本机自动验证完成。

## 2026-10-03 SFTP 标题栏控件环境验证

- 日期：2026-10-03
- 目的：确认标题栏重排和本地 Hidden 在现有 Rust/Slint 环境中通过构建与回归。
- 改动范围：`ui/sftp-pane.slint`、双语 usage 及项目记录；Cargo、锁文件、CI 和 SSH 安全边界未变。
- 执行内容：Slint 重编译，运行 fmt、locked/offline check、严格 all-target Clippy、完整测试、debug build、Markdown 相对链接、tracker validator 和 diff 检查。
- 验证结果：上述本机门禁全部通过；库 301、应用 309、Doc tests 0，4 条相对 Markdown 链接有效。
- 风险/待办：真实 GUI 视觉/焦点/点击由用户验收；Windows/Linux 原生目标交 CI。
- 开工判定：施工与本机验证完成。

## 2026-10-03 SFTP 标题栏控件施工预检

- 日期：2026-10-03
- 目的：确认双栏控件重排与本地 Hidden 可沿用锁定的 Rust/Slint 构建及测试环境。
- 改动范围：`ui/sftp-pane.slint`、双语 usage 和项目记录；不改 Cargo、锁文件、CI、SSH trust 或 worker。
- 执行内容：读取既有环境记忆，核对 `Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml` 和本机 Rust/Cargo；确认本地与远端 DTO 都有点文件标记。
- 验证结果：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1，Slint 1.18.1；完整门禁待实现后执行。
- 风险/待办：240px 窄栏需保持 Hidden 可用；实际 GUI 由用户验收。
- 开工判定：允许开工。

## 2026-10-02 Rust 1.99 CI 严格 Clippy 环境验证

- 日期：2026-10-02
- 目的：在 CI 失败的稳定工具链及目标上验证修复，并独立确认 Rust 1.92 MSRV。
- 改动范围：`src/app/runtime.rs` 和 `src/app/window_router.rs` 的 warning 兼容实现；依赖、lockfile、CI matrix 与安全边界不变。
- 执行内容：安装 Rust 1.99/1.92 与 Intel macOS target，运行 Rust 1.99 CI 同款 check/严格 Clippy/build、Rust 1.92 locked/offline check、Rust 1.97 fmt/check/严格 Clippy/完整测试及 tracker/链接/diff 门禁。
- 验证结果：上述 Rust 与文档门禁通过；Rust 1.97 完整测试库 299、应用 299、Doc tests 0；用户失败的 Rust 1.99 Intel target 严格 Clippy 命令已通过。
- 风险/待办：本机未验证 Windows/Linux 原生 target，继续由 CI runner 证明；本轮无需 GUI 视觉验收。
- 开工判定：施工与本机验证完成。

## 2026-10-01 Rust 1.99 CI 严格 Clippy 施工预检

- 日期：2026-10-01
- 目的：在修复 CI warning 时保留仓库 Rust 1.92.0 MSRV。
- 改动范围：renderer 窗口计数和多窗口路由两处 Rust 实现；不改依赖、lockfile、CI matrix 或安全边界。
- 执行内容：读取 `Cargo.toml`、CI target 命令与本机 rustc，核对用户 CI 日志及 Rust 官方 `AtomicUsize` API 稳定版本。
- 验证结果：本机 1.97.1，仓库 MSRV 1.92.0；`try_update` 自 1.95 稳定，不适合作为直接替换。两处错误来自 CI 使用较新的 stable Rust 和严格 warning 门禁。
- 风险/待办：需验证 MSRV 可编译、Rust 1.99 严格 Clippy 及原生 target；本机目前只安装默认 stable 工具链。
- 开工判定：允许开工。

## 2026-10-01 SFTP 本地目录监听环境验证

- 日期：2026-10-01
- 目的：确认跨平台目录通知依赖与本地栏自动刷新在锁定 Rust/Slint 环境中可编译、可测试。
- 改动范围：增加 `notify 8.2.0` 与 lockfile 包，修改 application bridge/状态、双语说明和项目地图；不改 SSH trust、凭据、远端 worker 或 CI matrix。
- 执行内容：运行定向测试、fmt、locked/offline check、严格 all-target Clippy、完整测试、两个 macOS target 的 CI 同款 check/Clippy/build、文档相对链接、tracker validator 和差异检查。
- 验证结果：定向 1 项、完整测试库 297/应用 298/Doc tests 0、macOS ARM64/Intel 编译和链接及所有静态/文档门禁通过；本机 Rust/Cargo 1.97.1、项目 MSRV 1.92.0，notify 声明 MSRV 1.77。
- 风险/待办：真实 GUI 和文件系统事件由用户验收；网络挂载可能不投递事件，手动 Refresh 保留；Windows/Linux target 交原生 CI。
- 开工判定：施工与本机验证完成。

## 2026-10-01 SFTP 本地目录监听施工预检

- 日期：2026-10-01
- 目的：确认当前本地目录可使用跨平台事件监听并保留既有 Rust/Slint/Tokio 构建环境。
- 改动范围：Cargo 依赖与 lockfile、application bridge、目录状态、双语文档；不改 SSH trust、凭据或远端 worker。
- 执行内容：读取现有环境记忆并核对 `Cargo.toml`、`Cargo.lock`、`build.rs`、CI 与本机 Rust/Cargo；检索 notify 8.2.0 的官方 API 和 MSRV。
- 验证结果：本机 Rust/Cargo 1.97.1、项目 MSRV 1.92.0；notify 8.2.0 声明 MSRV 1.77，适配当前目标。锁文件更新及平台构建待实施。
- 风险/待办：原生 watcher 对网络挂载目录可能不发事件；Windows/Linux 需原生 CI 和用户验收。
- 开工判定：允许开工。

## 2026-10-01 SFTP 成功事件自动刷新施工预检

- 日期：2026-10-01
- 目的：确认本地下载与远端写操作完成后可沿用现有 SFTP 浏览和测试环境更新对应文件栏。
- 改动范围：application state/bridge、状态测试及双语 SFTP 文档；不改 Cargo、锁文件、CI 或 SSH 安全边界。
- 执行内容：读取现有环境记忆，复核 `Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml` 和本机 Rust/Cargo；沿用 Rust 2024、MSRV 1.92.0、Slint 1.18.1 与 locked/offline 命令。
- 验证结果：上一轮完整门禁通过；本轮定向与完整验证待实施后运行。
- 风险/待办：成功事件与目录请求竞态需状态回归；真实 GUI/SSH 交互及非原生 target 由目标平台和 CI 验收。
- 开工判定：允许开工。

## 2026-10-01 SFTP 成功事件自动刷新环境验证

- 日期：2026-10-01
- 目的：确认成功下载和远端写操作自动刷新在现有 Rust/Slint/Tokio 环境中可编译、可测试。
- 改动范围：application state/bridge、状态回归、双语 SFTP 文档和项目地图；Cargo、锁文件、CI 与 SSH 安全边界未变。
- 执行内容：运行定向测试、fmt、locked/offline check、严格 all-target Clippy、完整 Cargo 测试、Markdown 相对链接、tracker validator 与差异检查。
- 验证结果：定向 2 项、完整测试库 297/应用 297/Doc tests 0 和全部静态/文档门禁通过；本机 Rust/Cargo 1.97.1、MSRV 1.92.0 与 Slint 1.18.1 不变。
- 风险/待办：真实 GUI/SSH 操作由用户在目标平台验收，非原生 target 由原生 CI 验证。
- 开工判定：施工和本机验证完成。

## 2026-10-01 X11 Custom 程序路径输入环境验证

- 日期：2026-10-01
- 目的：确认 X11 Custom 程序路径输入修复能在现有 Rust/Slint 环境中编译并通过回归。
- 改动范围：`ui/settings/x11.slint`、`src/x_server.rs` 和项目记录；不改 Cargo、锁文件、CI、X11 relay 或 SSH 安全边界。
- 执行内容：核对 Rust 2024、MSRV 1.92.0、本机 Rust/Cargo 1.97.1、Slint 1.18.1 与 CI matrix；运行 X11 provider 定向测试、fmt、locked/offline check、严格 all-target Clippy、完整 Cargo 测试、中文目录和 tracker validator。
- 验证结果：定向测试 1 项、完整测试库 297/应用 295/Doc tests 0、Slint 重新编译、中文目录 508 条、fmt/check/Clippy 和 tracker validator 通过。
- 风险/待办：Custom 路径行实际显示、点击与键盘焦点由用户在目标平台确认；非原生目标依照 CI 验证。
- 开工判定：施工及本机静态验证完成。

## 2026-10-01 SFTP 本地导航控件环境验证

- 日期：2026-10-01
- 目的：确认窄本地栏刷新入口与历史回归在当前锁定环境中可编译、可测试。
- 改动范围：`ui/sftp-pane.slint`、`src/app/state/tests.rs` 和双语用法；Cargo、锁文件、CI 与 SSH 安全边界未变。
- 执行内容：运行 fmt、locked/offline check、严格 all-target Clippy、完整测试、中文目录和 `msgfmt --check`、tracker validator、Markdown 相对链接与差异检查。
- 验证结果：定向测试 1 项、完整测试库 297/应用 295/Doc tests 0 和全部静态/文档门禁通过；本机 Rust 1.97.1、MSRV 1.92.0 与 Slint 1.18.1 不变。
- 风险/待办：窄栏图标视觉、键盘焦点和真实本地目录操作由用户在目标平台确认。
- 开工判定：施工及本机静态验证完成。

## 2026-10-01 SFTP 本地导航控件施工预检

- 日期：2026-10-01
- 目的：确认本地栏刷新、前进和后退可沿用现有 Rust/Slint 与测试环境修复。
- 改动范围：`ui/sftp-pane.slint`、本地导航状态回归和双语用法；不改 Cargo、锁文件、CI 或 SSH 安全边界。
- 执行内容：沿用上轮已核对的 Rust 2024、MSRV 1.92.0、本机 Rust 1.97.1、Slint 1.18.1 和 locked/offline 命令；确认本地刷新可见性条件与主/独立窗口回调现状。
- 验证结果：上一轮完整门禁通过；本轮 Slint Cargo check、中文目录 508 条和刷新历史定向测试 1 项通过，完整门禁待执行。
- 风险/待办：窄栏刷新实际视觉和键盘焦点由用户验收，非原生目标以 CI 为准。
- 开工判定：允许开工。

## 2026-10-01 SFTP 双栏上一层导航环境验证

- 日期：2026-10-01
- 目的：确认新增双栏导航在既有 Rust/Slint、翻译和测试环境中正常构建。
- 改动范围：`ui/` SFTP 页面、`src/app/` 状态与 bridge、中文目录及双语文档；不改 Cargo、锁文件、CI 或 SSH 安全边界。
- 执行内容：运行 `cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 `cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`、翻译和文档校验。
- 验证结果：定向父目录测试 2 项、完整测试库 297/应用 294/Doc tests 0、中文目录 508 条、`msgfmt --check`、tracker validator、Markdown 相对链接和 `git diff --check` 通过；本机工具链仍为 Rust 1.97.1，项目 MSRV 1.92.0。
- 风险/待办：GUI 视觉/焦点和真实 SSH 服务端导航需用户在目标平台确认；Windows/Linux 仍由原生 CI 验证。
- 开工判定：施工及本机静态验证完成。

## 2026-09-30 SFTP 双栏上一层导航施工预检

- 日期：2026-09-30
- 目的：核实双栏导航功能在现有项目环境中可实现和验证。
- 改动范围：`ui/` 的 SFTP 页面与 `src/app/` 的状态/bridge；不改 Cargo、锁文件、CI 或 SSH 安全边界。
- 执行内容：核对 `Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、现有环境记忆及本机 Rust 1.97.1；确认 Rust 2024、MSRV 1.92.0、Slint 1.18.1 和 locked/offline 测试命令保持不变。
- 验证结果：既有本地/远端历史测试 2 项和远端路径测试 1 项通过；新功能及完整门禁待执行。
- 风险/待办：GUI 视觉与真实 SSH 服务器导航由用户在目标平台验收；施工后记录完整门禁结果。
- 开工判定：允许开工。

## 2026-09-30 交互终端高延迟输入环境收口

- 日期：2026-09-30
- 变化摘要：复核远端交互终端输入优化所需的 Rust/Cargo、Slint build 入口、CI target 和 locked/offline 测试环境；环境事实未发生版本变化，仅刷新当前态记录。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`src/telnet.rs`、`src/ssh.rs`、`src/ssh/worker/shell.rs`、`docs/project-env-audit/current.md`。
- 更新后的命令或环境：继续使用 Rust 1.97.1 本机工具链、本项目 MSRV 1.92.0、Cargo locked/offline；完成 Telnet 定向测试、fmt、check、严格 Clippy、完整 Cargo 测试和 `git diff --check`。
- 验证结果：定向 Telnet 测试 5 项通过；完整测试库 297、应用 292、Doc tests 0；真实高 RTT Telnet/SSH 和 GUI 输入手感未在本轮执行，目标平台原生 CI 仍为跨平台证据。

## 2026-09-25 子窗口终端列宽同步施工预检

- 日期：2026-09-25
- 变化摘要：确认终端网格、Slint 快照和本地 PTY 共用现有尺寸链路；本轮不改工具链或依赖。
- 受影响文件：`ui/terminal-pane.slint`、双语架构与项目记录。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.18.1 和 Cargo locked/offline 门禁。
- 验证结果：`cargo check --locked --offline` 已通过，完整门禁待执行。
- 风险/待办：截图对应时刻的 PTY 行列数仍需用户提供 `stty size`；最终 GUI 视觉由用户验收。

## 2026-09-25 子窗口终端列宽同步环境验证

- 日期：2026-09-25
- 变化摘要：曾尝试让共享 `TerminalPane` 在模型列数落后于实测网格时再次使用既有延迟 resize；用户发现闪烁后已撤回。
- 受影响文件：`ui/terminal-pane.slint`、双语架构与环境/实施记录。
- 更新后的命令或环境：Cargo 依赖、工具链、CI 矩阵和测试命令不变。
- 验证结果：fmt、locked/offline check、严格 Clippy、完整测试（库 283、应用 277、Doc tests 0）、debug build 与 diff 检查通过；用户视觉验收发现持续闪烁，静态通过不能证明 UI 正确。
- 风险/待办：触发器已撤回；截图时 PTY 列数未取得，原列宽问题仍待定位。

## 2026-09-25 终端闪烁回归撤回

- 日期：2026-09-25
- 变化摘要：撤回快照列数和行数变化驱动的重复 resize 调度；恢复原有仅由几何、字体及布局变更触发的共享尺寸链路。
- 受影响文件：`ui/terminal-pane.slint`、双语架构与环境/实施记录；撤回后代码和双语架构与 HEAD 一致。
- 更新后的命令或环境：无工具链、依赖、锁文件或 CI 变化。
- 验证结果：撤回后的 `cargo build --locked --offline`、fmt、locked/offline check、严格 Clippy、完整测试（库 283、应用 277、Doc tests 0）和 `git diff --check` 通过；用户确认新进程不再闪烁。
- 风险/待办：原列宽异常尚未定位；需在异常同一时刻核对 `stty size` 与窗口网格。今天的默认日志级别没有记录现有 `terminal-geometry` debug 事件。

## 2026-09-24 macOS 标题栏前沿间距施工预检

- 日期：2026-09-24
- 变化摘要：确认标题栏留白调整沿用现有 Slint token、AppKit 命中边界和 Cargo locked/offline 环境；不新增依赖。
- 受影响文件：`ui/theme.slint`、`ui/workspace-shell.slint`、`src/app/macos_window.rs`、双语架构与项目记录。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.18.1 和现有 CI target 矩阵。
- 验证结果：已核对当前 60px 前沿、36px 分栏尾部间距和 AppKit 60px 命中阈值；施工后门禁待执行。
- 风险/待办：标题栏实际视觉和拖动边界需用户在 macOS 新构建中确认。

## 2026-09-24 macOS 标题栏前沿间距环境验证

- 日期：2026-09-24
- 变化摘要：macOS 标题栏前沿从 60px 增至 80px，AppKit 原生拖窗命中同步；36px 分栏尾部间距保留，未新增依赖或改变 CI 矩阵。
- 受影响文件：`ui/theme.slint`、`src/app/macos_window.rs`、双语架构与项目记录。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.18.1、Cargo locked/offline。
- 验证结果：定向命中测试、fmt、check、严格 Clippy、完整测试（库 283、应用 276、Doc tests 0）、debug build、tracker validator 和 diff 检查通过。
- 风险/待办：用户仍需确认实际标题栏视觉和拖动行为。

## 2026-09-24 终端 Tab 方框显示施工预检

- 日期：2026-09-24
- 变化摘要：核对本轮终端显示投影修复的环境；工具链、锁定依赖和 CI 测试矩阵无变化。
- 受影响文件：`src/terminal/render.rs`、`src/terminal/tests.rs`、双语架构与项目记录。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Cargo locked/offline 门禁；本机 rustc/Cargo 1.97.1 和 macOS ARM64/x86_64 target 可用。
- 验证结果：施工前确认 `Cargo.toml`、`Cargo.lock` 和 `.github/workflows/ci.yml`；定向与完整门禁待执行。
- 风险/待办：终端实际字体绘制仍由用户在目标平台视觉验收。

## 2026-09-24 终端 Tab 方框显示环境验证

- 日期：2026-09-24
- 变化摘要：终端显示快照将 Tab 单元格绘制为空格；内容/选区仍保留原始 Tab，依赖、工具链、锁文件和 CI 未变。
- 受影响文件：`src/terminal/render.rs`、`src/terminal/tests.rs`、双语架构与项目记录。
- 更新后的命令或环境：继续使用原有 Cargo locked/offline 和三平台原生 CI target 门禁。
- 验证结果：定向 Tab 回归、fmt、check、严格 Clippy、完整测试（库 283、应用 276、Doc tests 0）、debug build、Markdown 相对链接、tracker validator 和 diff 检查通过。
- 风险/待办：新构建的终端实际字形显示待用户在目标平台验收。

## 2026-09-23 macOS 标题栏拖动区域环境验证

- 日期：2026-09-23
- 变化摘要：主窗口 Slint 标题栏空白区使用现有 Winit window accessor 调用系统拖窗，AppKit 左侧原生命中区限定到标题栏高度；未新增依赖或修改 Cargo.lock。
- 受影响文件：`src/app/{macos_window,window_bridge}.rs`、`ui/{app,workspace-shell}.slint`、`ui/components/workspace-titlebar.slint`、双语架构和项目记录。
- 更新后的命令或环境：本机 rustc/Cargo 1.97.1、MSRV 1.92.0、Slint 1.18.1；首次缺失的锁定依赖通过本机 7897 代理下载后，恢复 locked/offline 验证。
- 验证结果：fmt、locked/offline check、严格 Clippy、macOS 定向测试、完整 Cargo 测试（库 280、应用 268、Doc tests 0）和 native build 通过；ARM64/x86_64 macOS 显式 target check、严格 Clippy 与 build 均通过，x86_64 仅编译链接。Slint 入口重新编译。
- 风险/待办：真实系统拖窗、Tab 重排、按钮点击与侧栏命中需要用户在 macOS 主窗口验收。

## 2026-08-31 RSA 兼容性与 RustSec 风险接受环境验证

- 日期：2026-08-31
- 变化摘要：恢复 `russh 0.63.1` 的 RSA feature，锁文件重新包含 `rsa 0.10.0-rc.18`、`pkcs1` 和 `crypto-primes`；CI 与 Release 的 RustSec audit-check 仅忽略 `RUSTSEC-2023-0071`，以兼容现有 RSA SSH 服务器并明确接受 Marvin timing side-channel 风险。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`.github/workflows/{ci,release}.yml`、`README*.md`、`docs/{usage,usage.zh,architecture,architecture.zh}.md`、`docs/project-{env-audit,implementation-tracker}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Cargo locked/offline 门禁；本地审计使用 `cargo-audit 0.22.2 audit --ignore RUSTSEC-2023-0071`，不改变其他 advisory 的失败策略。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 214、应用 211、Doc tests 0）、`git diff --check` 和带单项 ignore 的离线审计通过；审计仍显示 6 条允许级别告警。
- 风险/待办：RSA 私钥、RSA 主机密钥和 RSA agent identity 重新可用；如果攻击者能观察签名时序，应迁移到 Ed25519/ECDSA。上游修复发布后必须删除该 ignore 并重新执行审计。

## 2026-08-22 原生窗口激活刷新环境验证

- 日期：2026-08-22
- 变化摘要：保留锁定 `i-slint-core 1.17.1` 的 `WindowActiveChanged` hook 作为快速路径，并增加 macOS UI 线程每 100ms 读取 `NSWindow.isKeyWindow()` 的兜底；窗口失焦时将该窗口可见终端切换到 `unfocused_terminal_refresh_fps`，重新激活后恢复 pane focused/unfocused 策略。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`src/app.rs`、`src/app/{macos_window,window_router}.rs`、`src/app/window_router/tests.rs`、双语架构/usage 和项目跟踪。
- 更新后的命令或环境：仍使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo locked/offline 门禁；`raw-window-handle 0.6.2` 已存在，AppKit 读取复用既有 `objc2-app-kit` 依赖。
- 验证结果：窗口路由定向测试 1 项通过；`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 199、应用 192、Doc tests 0）、431 条翻译、46 个仓库 Markdown 相对链接和 `git diff --check` 通过；tracker validator 仍仅报告 5 条既有历史格式问题。未联网、未使用 multi-agent、未自行截图。
- 风险/待办：事件钩子仍依赖 Slint backend，但 macOS 已有 `isKeyWindow()` 轮询兜底；主窗口、detached 窗口及跨应用切换仍需用户在目标平台人工确认低帧率和恢复 focused FPS。

## 2026-08-22 Terminal 光标闪烁设置环境验证

- 日期：2026-08-22
- 变化摘要：新增默认开启的 `terminal_cursor_blink` 外观设置，Settings > Appearance > Rendering 可预览和持久化；关闭时停止 UI 闪烁 Timer 并保持光标显示。
- 受影响文件：`src/config/{settings,tests}.rs`、`src/app/{settings_bridge,view/settings}.rs`、`ui/{app,settings,settings/appearance,workspace-shell,terminal-pane,components/terminal-grid}.slint`、翻译、双语 usage/architecture 和项目跟踪。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo.lock、既有 Cargo locked/offline 门禁和 Python 翻译脚本；未新增依赖或工具链。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 199、应用 191、Doc tests 0）、`cargo test --lib config`（54 项）、`cargo test --bin ax_ssh`（191 项）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py`（431 条）、tracker validator 和 `git diff --check` 已完成；validator 仅报告既有历史条目格式问题。
- 风险/待办：真实光标闪烁/常显、IME、选区和 detached 窗口体验仍需用户在目标平台确认；不自动截图作为 GUI 证据。

## 2026-08-14 日期化 Release 重试与元数据施工预检

- 日期：2026-08-14
- 变化摘要：发现 `2026-08-14` 已重建到当前 `master`，但 `Cargo.toml`、`Cargo.lock` 和 `packaging/macos/Info.plist` 仍携带 `2026.8.13`。Create workflow 拒绝已存在 tag 是设计行为，Release 则缺少用户可见的已有 tag 重试入口。
- 受影响文件：`.github/workflows/{ci,create-dated-release,retry-existing-release,release}.yml`、`Cargo.toml`、`Cargo.lock`、`packaging/macos/Info.plist`、发布文档及 `docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、Python 3、Ruby YAML 解析和 GitHub-hosted Actions；计划将 `actions/github-script` 更新为 v8/Node 24，不新增语言依赖或运行时边界。
- 验证结果：修复前 `python scripts/release_version.py verify --tag 2026-08-14` 明确失败于 `Cargo.toml` 版本不匹配；workflow 静态审阅确认 tag CI 只接受同 tag SHA 的 successful workflow-dispatch run。
- 风险/待办：只能在尚未发布 GitHub Release 前重建日期 tag；已发布 tag 必须保持不变，后续修复需要新的版本标识。远端 CI/Release 需使用明确 tag 输入执行。

## 2026-08-14 本地 PTY shutdown CI 回归施工预检

- 日期：2026-08-14
- 变化摘要：GitHub-hosted macOS 的 `cargo test --locked` 显示本地 PTY 满事件队列时 shutdown 在测试的 5 秒上限内没有完成；开始检查取消、child 终止、reader join 与有界事件反压的竞态。
- 受影响文件：`src/local_shell.rs`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、Tokio、portable-pty 和既有 macOS CI；不新增依赖、工具链、运行时或安全边界。
- 验证结果：已读取 CI 失败日志；本机首次 `local_shell::tests::shutdown_terminates_a_running_shell_with_a_full_event_queue` 定向运行通过，表明需要按间歇性时序竞争重复验证。
- 风险/待办：正常运行必须保留 32 项有界事件队列的反压语义；取消后可放弃未投递事件以避免 reader/worker join 阻塞。修复后需要完整 locked/offline Cargo 门禁和远端 tag CI 复验。

## 2026-08-14 本地 PTY shutdown CI 回归环境验证

- 日期：2026-08-14
- 变化摘要：在 event delivery 前检查取消，使 reader 不会在 owner shutdown 后继续发送；process group 的 SIGKILL 成功时直接结束，不再取得 child-killer 锁并执行额外 SIGHUP 宽限等待。
- 受影响文件：`src/local_shell.rs`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：无。保留 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、Tokio、portable-pty 与既有 macOS CI；不新增依赖或工具链。
- 验证结果：7 项 local shell 定向测试、20 次满队列 shutdown 压力运行、fmt/check/严格 Clippy 和完整 Cargo 测试（库 178、应用 162、Doc tests 0）通过。
- 风险/待办：正常运行仍以有界反压保护输出；只有 owner 已取消时才放弃尚未投递的事件。重建 `2026-08-14` tag 后需由 GitHub-hosted macOS CI 复验。

## 2026-08-14 macOS 多架构 Release 施工预检

- 日期：2026-08-14
- 变化摘要：开始调整 GitHub Release 的 macOS 附件，使 Universal、Apple Silicon 和 Intel bundle 同时发布；保留现有日期 tag、CI 成功和 target 专属 Cargo cache 门禁。
- 受影响文件：`.github/workflows/release.yml`、`docs/development{,.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、现有 `Cargo.lock`、Python 3 标准库和 GitHub-hosted macOS runner；不新增依赖、工具链、运行时或安全边界。
- 验证结果：已复核发布矩阵、`lipo`/codesign bundle 组装路径和同级项目的独立架构+Universal 发布模式；实现后将运行 YAML/Shell/Markdown/tracker/diff 与 `cargo check --locked --offline`。真实 GitHub Actions 附件在下一次日期 tag 验证。
- 风险/待办：GitHub-hosted macOS runner 才能验证 Intel target 链接、ad-hoc codesign、ZIP 展开和 Release 附件列表。

## 2026-08-14 macOS 多架构 Release 环境验证

- 日期：2026-08-14
- 变化摘要：Release 的两个 macOS target 各自产出完整、签名过的 `.app` ZIP，Universal job 解包二者并以 `lipo` 合并可执行文件后重新签名；发布 job 以既有 `release-*` pattern 收集三份 macOS 资产。
- 受影响文件：`.github/workflows/release.yml`、`packaging/macos/build-app.sh`、`docs/development{,.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、现有 `Cargo.lock`、Python 3 标准库、GitHub-hosted macOS runner 和 target 专属 cache；不新增依赖、工具链、运行时或安全边界。
- 验证结果：YAML、Shell、Python 9 项 release 回归、本机 ARM64 `.app` 的 `lipo`/codesign/ZIP round-trip、完整 `cargo fmt/check/clippy/test --locked --offline`（库 177、应用 162、Doc tests 0）和差异检查通过。
- 风险/待办：下一次有效日期 tag 的远端 runner 仍需验证 x86_64 target、双架构 `lipo` 和 GitHub Release 上的三个附件；ad-hoc 签名不等同于 Developer ID 签名或 notarization。

## 2026-08-14 macOS 三份 bundle 本机回环验证

- 日期：2026-08-14
- 变化摘要：使用两个已完成的 target release binary 完整复现 Release 打包路径，分别产出 arm64、x86_64 与 Universal `.app` ZIP；Universal 从两个已签名的原生 bundle 解包、合成可执行文件并重新签名。
- 受影响文件：`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：无。继续使用本机 macOS 的 `lipo`、codesign、ditto 和既有 release binary；不新增依赖、工具链、运行时或安全边界。
- 验证结果：三个 bundle 的架构断言、严格 ad-hoc codesign、ZIP 打包/解包回环、`Info.plist` 校验、图标和许可证存在性检查均通过；Universal bundle 保留 17 个运行时字体文件。
- 风险/待办：GitHub-hosted macOS runner 的 artifact 上传/下载与实际 Release 附件列表仍需在下一个有效日期 tag 验证；ad-hoc 签名不等同于 Developer ID 签名或 notarization。

## 2026-08-14 终端缓冲区 resize 施工预检

- 日期：2026-08-14
- 变化摘要：开始修复普通主屏纵向放大后的顶部空带；变更只涉及锁定 `alacritty_terminal` 之上的 `TerminalModel` 后置补偿与相关回归/文档。
- 受影响文件：`src/terminal.rs`、`docs/{architecture,architecture.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 Cargo locked/offline 门禁；不新增依赖、不修改 `Cargo.lock`、PTY worker、SSH trust 或凭据。
- 验证结果：本机 `cargo fmt` 与 `cargo clippy` 可用，已完成锁定上游与主流终端实现对照；定向和完整 Cargo 门禁待代码完成后执行。

## 2026-08-14 完成终端缓冲区 resize 环境验证

- 日期：2026-08-14
- 变化摘要：`TerminalModel` 删除上游 resize 后的下滚和 cursor-to-bottom 补偿；无 scrollback 时现有内容保持顶部、空行位于底部，有 scrollback 时仍恢复真实历史。
- 受影响文件：`src/terminal.rs`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与 locked/offline Cargo 门禁；不新增依赖、不修改 `Cargo.lock`、PTY worker、SSH trust 或凭据。
- 验证结果：25 项 terminal 定向回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 173、应用 160、Doc tests 0）、tracker、相对 Markdown 链接和差异检查通过；目标平台视觉仍由用户验收。

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

## 2026-08-13 内嵌默认字体基线

- 日期：2026-08-13
- 变化摘要：JetBrains Mono 四个默认字重改由 Rust `include_bytes!` 编译进可执行文件，保证单独运行 Windows EXE 时应用和 Terminal 默认字体可用；Maple Mono NF CN、Iosevka Term 和 Monaspace Neon 继续作为有界运行时资源读取。
- 受影响文件：`src/app/font_bridge.rs`、`assets/fonts/README.md`、`docs/project-env-audit/current.md`、双语架构/开发文档与实施跟踪。
- 更新后的命令或环境：不新增依赖或工具链要求；外部字体文件仍在 Tokio blocking worker 读取，所有字体字节仍只在 Slint UI 线程注册；官方发行包继续携带完整 `assets/fonts/` 和许可证声明。
- 验证结果：无外部资源目录加载 JetBrains Mono 的确定性回归、完整 locked/offline Cargo 门禁（库 169、应用 161、Doc tests 0）、翻译/Markdown/差异检查和 Windows MSVC release 交叉构建通过；Windows 单 EXE 仍需目标机复测。

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

## 2026-08-12 本地 SFTP fingerprint 回归环境修正

- 日期：2026-08-12
- 变化摘要：Linux x86_64/aarch64 和 Windows CI 表明快速同长度写入可保留相同的可查询时间字段；本地文件回归改为断言跨平台可观察的长度变化。
- 受影响文件：`src/app/local_files.rs`、`docs/{architecture.md,architecture.zh.md,development.md,development.zh.md,project-env-audit/{current.md,changes.md},project-implementation-tracker/}`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；未改依赖、锁文件、工具链、CI、SFTP 协议、SSH trust 或凭据。
- 验证结果：`cargo test --bin ax_ssh app::local_files::tests --locked --offline` 通过 7 项应用二进制本地文件回归。完整 Cargo 与文档门禁待当前步骤完成。

## 2026-08-12 本地 SFTP fingerprint 回归门禁完成

- 日期：2026-08-12
- 变化摘要：跨平台确定性的本地打开回归与收紧后的 fingerprint 契约已通过完整本地门禁；环境事实和 CI 矩阵均未改变。
- 受影响文件：`src/app/local_files.rs`、`docs/{architecture.md,architecture.zh.md,development.md,development.zh.md,project-env-audit/{current.md,changes.md},project-implementation-tracker/}`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与 locked/offline Cargo；不新增依赖、不改变锁文件、工具链、CI、SFTP 协议、SSH trust 或凭据。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、`cargo test --locked --offline`（库 150、应用 153、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过；目标 Linux/Windows CI 仍需本次提交触发后确认。
## 2026-08-13 SFTP Transfers 三状态页施工前预检

- 日期：2026-08-13
- 变化摘要：复核 SFTP Downloads 的状态、worker、缓存与 Slint 路由；本机已确认 `cargo fmt` 和 `cargo clippy` 均可执行。
- 受影响文件：`src/{sftp,ssh/worker,app}/`、`ui/`、`docs/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与 `cargo fmt/check/clippy/test --locked --offline`；不变更依赖、锁文件、工具链或 CI。
- 验证结果：已读取 `Cargo.toml`、`Cargo.lock`、`.github/workflows/`、现有环境记录和 SFTP 实现；`rustc 1.97.1`、`cargo 1.97.1`、`rustfmt 1.9.0`、`clippy 0.1.97` 本机可用。完整门禁在功能完成后执行。

## 2026-08-13 SFTP Transfers 三状态页环境更新

- 日期：2026-08-13
- 变化摘要：新增 SFTP worker-lifetime 的暂停/继续、递归发现和到当前 Local files 目录的安全 `.part` 原子下载；应用状态和 Slint 只处理有界传输 DTO/意图，失败与取消由 worker 清理自身任务内容。
- 受影响文件：`src/{app,sftp,ssh/worker}/`、`ui/`、`translations/`、`scripts/` 和双语 SFTP/跟踪文档。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、现有 Tokio/russh 和 `cargo fmt/check/clippy/test --locked --offline`；不新增 crate、锁文件、配置 schema、CI、SSH trust 或凭据契约。
- 验证结果：施工前环境复核已通过；翻译生成、定向/完整 Cargo、tracker、Markdown 和差异门禁将在收口记录。目标平台三页布局、勾选焦点、递归下载、暂停/继续和取消清理由用户验收。

## 2026-08-13 SFTP Transfers 三状态页环境收口

- 日期：2026-08-13
- 变化摘要：完成递归下载、三状态 Transfers 页面和同进程 pause/resume/cancel 的环境验证；递归发现最多扫描 4,096 个条目，本地 `.part` 在 fsync 后无覆盖原子发布，取消和失败仍清理任务创建的数据。
- 受影响文件：`src/{app,sftp,ssh/worker}/`、`ui/`、`translations/`、`scripts/` 和 `docs/`。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio/russh 与 locked/offline Cargo；不新增 crate、不修改锁文件、工具链、CI、配置 schema、SSH trust 或凭据。
- 验证结果：`python3 scripts/build_zh_catalog.py`、翻译 399 条检查、SFTP 37 项（含 Local files loopback 发布）和状态 7 项定向测试、Rustfmt、离线 check、严格 Clippy、完整 Cargo 测试（库 157、应用 153、Doc tests 0）、tracker、Markdown 相对链接及 `git diff --check` 通过。目标平台 GUI 布局、勾选焦点和真实 SFTP 交互仍由用户验收。
## 2026-08-13 SFTP 写能力施工环境记录

- 变化摘要：复用锁定 `russh-sftp 2.3.0` 的 raw `open/read/write/remove/rmdir/rename` API，新增 worker-owned 有界写命令；未新增依赖或改变工具链。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0、`cargo check/test --locked --offline`、fmt/clippy/diff 门禁；文本编辑上限 4 MiB，上传上限 512 MiB，写操作复用独占 worker lifecycle。
- 验证结果：`cargo check --locked --offline` 已通过；完整 fmt、Clippy、test、翻译、tracker 和真实 SFTP/GUI/拖拽仍待收口或目标平台验证。

## 2026-08-13 SSH known_hosts 兼容环境记录

- 变化摘要：新增直接锁定依赖 `hmac 0.13.0`/`sha1 0.11.0`，实现 OpenSSH known_hosts 的有界解析、shared trust、`@revoked` 拒绝和原子追加/替换/移除；不改变 Rust 2024、MSRV、Slint/Tokio/russh 版本。
- 更新后的命令或环境：继续使用 `cargo fmt/check/clippy/test --locked --offline`、翻译、tracker 和 diff 门禁；系统文件路径与权限依赖目标平台。
- 验证结果：known_hosts 定向 4 项、完整 Cargo 测试（库 167、应用 160、Doc tests 0）、locked check、严格 Clippy、翻译、tracker 和 diff 检查通过。真实 SSH 工具互操作、系统权限和 GUI revoked/changed 呈现仍需用户验收。

## 2026-08-14 同日 Release 修订环境验证

- 变化摘要：日期发布支持不可变的 `YYYY-MM-DD[-N]` tag；第二次同日发行 `2026-08-14-1` 使用 Cargo `2026.8.14+1`、Debian `2026.8.14-1`、macOS short `2026.8.14` 与 build `20260814.1`。Create 只创建新 tag，Retry 不具备 tag 写权限。
- 受影响文件：`.github/workflows/{create-dated-release,retry-existing-release,release}.yml`、`scripts/{release_version,generate_release_highlights}.py`、对应 Python 回归、`Cargo.toml`、`Cargo.lock`、`packaging/macos/Info.plist`、发布文档与 `docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、锁定离线 Cargo 和 Python 标准库；GitHub Script 使用 v8/Node 24，远端仍以 tag SHA 的 workflow-dispatch CI 作为 Release 前提。
- 验证结果：Python 12 项、`release_version.py verify --tag 2026-08-14-1`、Ruby YAML、plist、`cargo fmt/check/clippy/test --locked --offline`（库 178、应用 162、Doc tests 0）、tracker、Markdown 相对链接和 `git diff --check` 通过。推送前远端 `master` 无分叉，`2026-08-14-1` 不存在。

## 2026-08-14 日期 Tag Push Release 编排环境验证

- 目的：让直接 push 的有效日期 tag 自动开始发布，而不放宽 Release 对精确 tag SHA CI 的成功要求。
- 改动范围：`.github/workflows/retry-existing-release.yml`、`README.md`、`docs/{architecture,architecture.zh,development,development.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 执行内容：新增日期 tag push 触发器；Retry 统一解析输入 tag 或 push ref，继续校验 annotated tag、已提交版本元数据、精确 tag SHA 的 CI，且仅在 CI 成功后 dispatch Release。Release 不直接监听 tag push，Create workflow 与手动 Retry 均保持可用。
- 验证结果：Ruby YAML 解析、Python 12 项 release/Highlights 回归、版本 verify、`cargo fmt/check/clippy/test --locked --offline`（库 178、应用 162、Doc tests 0）通过；`actionlint` 未在本机安装，GitHub-hosted push event 链路待下一个新日期 tag 验证。
- 风险/待办：已有 `2026-08-14-1` 不会因本次默认分支 workflow 改动而回溯触发新的 push event；需要发布它时仍运行手动 Retry Existing Release。新 tag 应自动进入该编排。

## 2026-08-17 直接日期 Tag 发布环境收口

- 目的：收敛为一个可实际触发的发布入口：发布者推送 annotated 日期 tag 后直接构建并创建 GitHub Release。
- 改动范围：`.github/workflows/{ci,release}.yml`、删除 `.github/workflows/{create-dated-release,retry-existing-release}.yml`、`README.md`、`docs/{architecture,architecture.zh,development,development.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 执行内容：Release 直接监听 `20*-*-*`，在完整 checkout 中检查 tag object 为 annotated，再运行 `scripts/release_version.py verify --tag`；构建、Universal macOS bundle、Highlights 和仅 publish job 的 `contents: write` 保持不变。CI 只由成功默认分支 run 写共享 target cache，Release 只恢复。移除 Create，避免默认 `GITHUB_TOKEN` 推送不能触发 `push` workflow 的隐蔽分支；tag 由发布者在默认分支用版本脚本同步、提交和推送。
- 环境变化：无 Rust crate、锁文件、工具链、构建矩阵、SSH trust 或凭据变化；移除不再需要的 GitHub Script action 和多 workflow dispatch/wait 路由。
- 验证结果：两个 YAML 工作流解析通过；现有 annotated `2026-08-17` 的 tag object/元数据 verify 通过；Python release/Highlights 12 项、413 条翻译、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy 和完整 `cargo test --locked --offline`（库 179、应用 167、Doc tests 0）通过。Markdown、tracker 和 diff 最终检查待本轮收口。
- 风险/待办：已有 `2026-08-17` 来自旧 workflow 快照，不会回溯触发新 Release；使用下一枚同步元数据后的 annotated 日期 tag 验证 GitHub-hosted 五平台构建、附件与 GitHub Release。

## 2026-08-17 直接日期 Tag 发布最终验证

- 目的：记录 direct tag Release 的本地门禁结果，并区分可本地验证的发布契约与 GitHub-hosted 执行。
- 改动范围：`docs/project-{implementation-tracker,env-audit}/`。
- 执行内容：完成 YAML、annotated tag/metadata、Python、Cargo、翻译、Markdown 和差异验证；确认 tracker current 使用标准联网状态字段，新增记录采用完整时间格式。
- 环境变化：无。
- 验证结果：两个 YAML、现有 `2026-08-17` tag/metadata、12 项 Python、413 条翻译、fmt/check/严格 Clippy、完整 Cargo 测试（库 179、应用 167、Doc tests 0）、30 份非参考子模块 Markdown 相对链接和差异检查通过。tracker validator 只保留 39 条既有历史时间字段错误，本轮记录没有新增 validator 报告。
- 风险/待办：下一枚有效 annotated 日期 tag 仍须在 GitHub-hosted Windows、Linux x86_64/aarch64、macOS arm64/x86_64 以及 GitHub Release 权限下执行验证。

## 2026-08-22 内存与线程生命周期优化环境验证

- 日期：2026-08-22
- 变化摘要：应用 runtime 改为显式 2-4 个 Tokio async worker、最多 8 个 blocking worker，blocking 线程空闲 2 秒后允许退出；SFTP 图标预热目标改为 `Weak<AppState>`，扩展图标清理返回释放数量。Fontique/Slint、CoreAnimation 和 allocator 的进程级缓存边界保持不变。
- 受影响文件：`src/app.rs`、`src/app/file_icons.rs`、`src/app/view/sftp.rs`、`docs/{architecture,architecture.zh}.md` 和 `docs/project-{implementation-tracker,env-audit}/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1 和 Cargo.lock；未新增 crate、未联网、未修改 SSH trust/凭据/transport。
- 验证结果：runtime/file-icon 定向测试、`cargo check --locked --offline`、`cargo fmt --all -- --check`、严格 Clippy 和完整 `cargo test --locked --offline`（库 199、应用 193、Doc tests 0）通过；重复 `footprint`/`vmmap -summary` 与目标平台 GUI 资源验收仍需用户执行。

## 2026-08-22 app 聚合模块拆分施工预检

- 目的：为按功能拆分 `src/app.rs` 建立实现前的环境与边界基线。
- 改动范围：`src/app.rs`、`src/app/` 及项目跟踪文档；不改依赖、Cargo.lock、Slint/Cargo 入口、SSH trust、凭据或 transport。
- 执行内容：复核 `AGENTS.md`、AxSSH Rust/Slint skill、`project-map.md`、`Cargo.toml`、`Cargo.lock`、CI workflow；确认 Rust 2024/MSRV 1.92、Slint 1.17.1、Tokio 1、locked/offline Cargo 命令和本机工具链。
- 验证结果：`rustc --version`、`cargo --version`、`cargo fmt --version`、`cargo clippy --version`、`python3 --version` 已确认；代码迁移尚未开始，完整 Cargo 门禁在每批拆分完成后执行。
- 风险/待办：生成 Slint 类型必须留在 app 层；模块拆分不能形成 callback 强引用循环、跨 `.await` 锁或参考工程耦合；需要保留既有 focused tests 并在收口运行完整离线门禁。

## 2026-08-22 app 聚合模块拆分环境验证

- 变化摘要：完成 `src/app.rs` 的功能拆分，新增 `runtime.rs`、`window_bridge.rs` 和 `platform_support.rs`；保留生成 Slint 类型、`run` 和 callback 组装在 app 层。
- 受影响文件：`src/app.rs`、`src/app/{runtime,window_bridge,platform_support}.rs`、双语架构、项目地图和实施跟踪。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1、Cargo.lock 和 locked/offline 门禁；未新增依赖、未联网。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、全量 Cargo 测试（库 199、应用 193、Doc tests 0）、tracker、相对 Markdown 链接和 `git diff --check` 已完成；tracker 仅报告既有历史格式问题。平台窗口视觉与激活行为仍需用户在目标平台验收。
- 环境变化检查：否；只移动私有模块和更新文档，不改变工具链、依赖、构建入口、配置、SSH trust、凭据或 transport。

## 2026-08-23 renderer 无关窗口资源释放施工预检

- 变化摘要：确认 Slint 1.17.1 没有公开的全局 renderer cache flush 或 RSS 归还 API；本轮只补应用拥有的 AppWindow model、文本、图片行、敏感 UI 字段和 detached 强引用的显式释放路径。
- 受影响文件：`src/app.rs`、`src/app/window_bridge.rs`、`docs/{architecture,architecture.zh}.md`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1、Cargo.lock 和 locked/offline 门禁；不新增依赖、不修改 renderer/SSH trust/凭据/transport。
- 验证结果：施工前已核对主窗口、detached 窗口和 Slint models 的所有权；代码和最终门禁尚未完成。目标平台仍需以同 renderer/负载重复 footprint、vmmap 和线程数复核平台缓存边界。

## 2026-08-23 detached model 与字体内存审计

- 变化摘要：detached 初始化只保留 Terminal/SFTP surface 所需的 Slint model，sidebar、连接选择器、Settings、session-editor、shell/X11 和 font-option model 不再为每个 detached 窗口复制；字体审计确认 JetBrains Mono 约 1.1 MiB、Iosevka Term 约 18.8 MiB、Maple Mono NF CN 约 40 MiB、Monaspace Neon 约 1.6 MiB，仍按选中 family 懒加载。
- 受影响文件：`src/app/window_bridge.rs`、`src/app/font_bridge.rs`、`docs/{architecture,architecture.zh}.md`、`docs/project-env-audit/current.md` 和 `docs/project-implementation-tracker/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1 和 locked/offline Cargo 门禁；不新增依赖、不改变 renderer、配置 schema、SSH trust、凭据或 transport。Fontique shared collection 没有可靠动态卸载 API。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 199、应用 193、Doc tests 0）、431 条翻译检查和 `git diff --check` 通过；tracker validator 只报告既有历史格式错误。
- 风险/待办：字体注册后的 Fontique、Slint、CoreAnimation/Metal、平台图标数据库和 allocator 可能保留进程级缓存；目标 macOS 仍需同 renderer/负载重复三轮 `footprint`、`vmmap -summary` 和线程数采样。

## 2026-08-23 Fontique 路径字体源内存优化

- 变化摘要：Maple/Iosevka/Monaspace 的自带字体加载从完整 `fs::read` buffer 改为大小校验后的路径列表，由 Fontique `load_fonts_from_paths` 建立路径 source；JetBrains Mono 继续使用嵌入 bytes。Maple Hani fallback 仍只包含 Maple family。
- 受影响文件：`src/app/font_bridge.rs`、`docs/{architecture,architecture.zh}.md`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Fontique 0.10.0、Tokio 1、Cargo.lock 和 locked/offline 门禁；不新增依赖、不改变 renderer、配置 schema、SSH trust、凭据或 transport。
- 验证结果：已确认锁定 Fontique 源码在路径扫描和按需数据访问阶段使用 mmap/SourceCache；`cargo test --bin ax_ssh app::font_bridge --locked --offline` 通过（8 项），完整 fmt/check/严格 Clippy/Cargo test（库 199、应用 193、Doc tests 0）、431 条翻译、tracker validator 和 `git diff --check` 通过。validator 仍报告旧月度记录的历史格式问题。
- 风险/待办：字体首次渲染仍可能产生 mmap、解析和 glyph cache；Fontique shared collection 没有本轮可靠的动态注销 API。目标 macOS 需相同字体负载重新采样 `MALLOC_LARGE`、`footprint` 和 `vmmap -summary`。

## 2026-08-23 连续窗口 resize 环境验证

- 变化摘要：resize callback 现在只调度当前可见 terminal pane；AppState 先比较规范化模型尺寸，再请求指定 worker 并推进 selection revision；SSH/Telnet watch 和 Local pending latest-value 对同尺寸请求去重。
- 受影响文件：`src/app/terminal_bridge.rs`、`src/app/state/tabs.rs`、`src/terminal.rs`、`src/{local_shell,telnet,ssh/worker}.rs`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1、russh 0.62.2、Cargo.lock 和 locked/offline 门禁；未新增依赖、线程或 renderer surface API。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 201、应用 193、Doc tests 0）、resize focused tests 和 `git diff --check` 通过。
- 人工验收：目标 macOS Software/GPU renderer 的连续拖动闪烁、焦点/IME/选区和真实 SSH/Telnet/Local 行列同步仍需用户确认；不把静态门禁等同于平台 GUI 流畅度证明。

## 2026-08-23 dirty-region backend patch 环境验证

- 变化摘要：新增本地 crates.io patch，保留锁定 Slint 1.17.1 winit software backend 的多矩形 forwarding，并为 macOS softbuffer 0.4.8 增加持久 framebuffer/失效状态；macOS 仍使用单 layer 完整 `CGImage` 提交，不使用 tile/slicing。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`vendor/{i-slint-backend-winit,softbuffer}/`、第三方许可、双语架构/开发说明和项目跟踪。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、softbuffer 0.4.8 和 locked/offline Cargo 门禁；不新增运行时 crate，不改变 SSH trust、凭据或 transport。
- 验证结果：全量 fmt/check/严格 Clippy/test/build、vendor rustfmt、中文 catalog/check 和 `git diff --check` 通过；仓库未提供 `scripts/validate_tracking_docs.py`，该命令未执行。目标平台软件 renderer 的持续输出、resize、隐藏恢复、DPI、闪烁/残影和同负载 sample/A-B 仍待用户验收。
- 环境变化检查：是；仅 macOS CoreGraphics backend 增加持久 framebuffer 与 full-layer present 状态，非 Apple backend dispatch 未改行为。
## 2026-09-06 Settings 控件与终端焦点修复

- 日期：2026-09-06
- 变化摘要：General 三处下拉改用项目主题组件；活动窗口/Terminal Tab 通过有界 revision 和延迟恢复透明 IME 焦点。
- 受影响文件：`ui/settings/general.slint`、`ui/app.slint`、`ui/workspace-shell.slint`、`src/app/window_bridge.rs`、`src/app/window_router.rs`、双语使用说明和 tracker。
- 更新后的命令或环境：工具链、Cargo.lock、Slint build 入口和 locked/offline 门禁保持不变。
- 验证结果：fmt、locked/offline check、严格 Clippy、完整 Cargo test（230 库、218 应用、Doc tests 0）及 diff 检查通过；目标平台 GUI 未自动验收。

# 2026-09-21 键盘输入标准化施工预检

- 日期：2026-09-21
- 变化摘要：本轮进入 Slint/Winit/终端键盘输入标准化施工；继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、`alacritty_terminal` 0.26.0 和 Cargo locked/offline 门禁，不新增依赖、不联网。
- 受影响文件：`src/app/input.rs`、`src/app/terminal_bridge.rs`、`src/terminal/input.rs`、`ui/components/keyboard-input.slint`、`ui/terminal-pane.slint`、双语架构/用法和项目跟踪文档。
- 更新后的命令或环境：先运行 `cargo test --locked --offline terminal::input::tests` 与 `cargo test --locked --offline app::input::tests`；实现后运行 fmt/check/严格 Clippy/全量测试/翻译检查/`git diff --check`。
- 风险/待办：Shift 文本 fallback 需保持布局/IME/AltGr 语义；无稳定 xterm/terminfo 定义的组合继续不发序列；目标平台真实键盘布局、IME、NumLock、焦点和应用快捷键仍需用户验收。

# 2026-09-23 Skia 图形内存修复施工预检

- 日期：2026-09-23
- 变化摘要：Sample 进程主要占用 Metal 图形资源；确认锁定 Skia 1.18.1 的 per-component layer cache 销毁清理缺口，将以精确版本本地 patch 修复。
- 受影响文件：`Cargo.toml`、`Cargo.lock`、`vendor/i-slint-renderer-skia/`、双语架构和项目跟踪。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.18.1 和 locked/offline Cargo 门禁；本地 path patch 不新增网络依赖。
- 风险/待办：1.17.1 安装版与当前 1.18.1 构建不同；需在同一窗口、终端行数和持续输出下比较修复前后 GPU footprint 与纹理数，GUI 视觉由用户验收。

## 2026-09-23 Skia 图形内存修复验证

- 日期：2026-09-23
- 变化摘要：本地 Skia 1.18.1 patch 在动态组件销毁时清除对应 layer cache；锁文件仅切换该 crate 来源，未改变其它依赖版本。
- 受影响文件：`vendor/i-slint-renderer-skia/`、`Cargo.toml`、`Cargo.lock`、双语架构和项目跟踪。
- 更新后的命令或环境：保持既有 locked/offline Cargo 命令，macOS ARM64 原生与 x86_64 交叉 target 均可编译链接；x86_64 测试只在原生 runner 运行。
- 验证结果：ARM64 fmt/check/严格 Clippy/全量 test/build 通过；x86_64 显式 target check/严格 Clippy/build 通过。实际图形内存走势未在新二进制长期负载下验证。
- 风险/待办：用户运行中的 1.17.1 安装版未修改；部署后以相同负载对比 `vmmap -summary` 图形区域及 `heap -s` 纹理数。

## 2026-09-26 窗口保持环境预检与实施

- 项目边界：AxSSH 独立 Rust/Slint 桌面应用的工作区持久化及原生窗口恢复。
- 证据：Cargo.toml、Cargo.lock、build.rs、CI、Slint 1.18.1/Winit 0.30.13 本机源码。
- 环境：本机 macOS ARM64，Rust/Cargo 1.97.1，已安装 ARM64/x86_64 macOS target；保持 Rust 2024、MSRV 1.92.0 与依赖锁定，无依赖/工具链变更。
- 实施：兼容 placement DTO、DPI/屏幕范围恢复、后台串行自动保存与退出 flush；SFTP-only 和非活动分屏恢复回归。
- 验证：首轮完整库 285/应用 288 测试、fmt/check/严格 Clippy 与本机 debug build 通过；最终代码及显式 target 验证仍在执行，结果见 current.md。
- 限制：Windows/Linux 缺少本机 target/SDK，需 CI 原生验证；GUI、真实多屏与最大化由用户验收，不自行截图。

## 2026-09-26 窗口保持最终验证

- 实施结果：主/独立窗口普通几何与最大化恢复、屏幕校验、后台自动保存和多窗口恢复已完成；双语架构/用法与项目地图同步。
- 本机验证：最终 fmt、locked/offline check、严格 all-target Clippy、285 项库测试、288 项应用测试、Doc tests 0 和 debug build 通过。
- 平台验证：ARM64/x86_64 macOS 的显式 target check、严格 Clippy 和 build 全部通过；仅运行原生 ARM64 测试。
- 其他门禁：Markdown、skill、tracker、diff 通过。没有依赖/锁文件/工具链变化。
- 待验收：Windows/Linux 原生 CI；真实 GUI、全屏/最大化动画和多屏恢复由用户验收。

## 2026-10-02 tmux 滚轮输入施工预检

- 项目边界：独立 Rust 2024/Slint 桌面应用的终端 UI、应用 bridge、Alacritty 模型和现有 worker 输入链路；不改 SSH trust、凭据、依赖或 CI。
- 环境记忆状态：核对现有 `current.md`，本轮目标已从 Rust 1.99 CI 修复转到 tmux 备用屏幕滚轮；根目录、关键证据和章节结构仍有效，已刷新当前态。
- 仓库声明：MSRV 1.92.0，Slint 1.18.1，Alacritty Terminal 0.26.0；`Cargo.lock` 与 `.github/workflows/ci.yml` 固定构建和多 target 验证。
- 本机环境：macOS ARM64，Rust/Cargo 1.97.1；本机 tmux 3.6a 可运行，默认 `mouse off`、`alternate-screen on`。
- 可执行命令：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`、`git diff --check`。
- 风险与缺口：GUI 实际滚轮、焦点和远端 tmux 仍需用户视觉验收；非原生平台由 CI target 检查。开工判定：允许开工。

## 2026-10-02 tmux 滚轮输入环境验证

- 项目边界：只增加终端模型/Slint 事件回归与双语说明；生产 UI、模型、worker、SSH trust 和依赖均未改变。
- 本机验证：Rust/Cargo 1.97.1，tmux 3.6a；隔离 PTY 在 `mouse off` 下验证备用屏幕/application-cursor 模式及 `ESC OA` 让 copy cursor 上移一行。
- 门禁：聚焦回归、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 300、应用 300、Doc tests 0）、Markdown 相对链接、tracker validator 与 diff 检查均通过。
- 风险与缺口：真实 GUI 的滚轮和持续刷新光标观感由用户验收；Windows/Linux 原生平台由 CI 验证，未用本机交叉编译结果替代。
- 开工判定：施工与本机验证完成。

## 2026-10-02 主窗口标准终端 pointer 转发环境验证

- 日期：2026-10-02
- 目的：验证主窗口漏转发滚轮事件的最小 Slint 修复，补齐只测独立窗口的覆盖缺口。
- 改动范围：`ui/workspace-shell.slint`、`src/app/view/tests.rs`、双语说明和项目记录；工具链、依赖、lockfile 与 CI 矩阵不变。
- 执行内容：本机 Rust/Cargo 1.97.1、Slint 1.18.1；修复前/后各运行主/独立窗口事件回归，再运行 fmt、locked/offline check、严格 all-target Clippy、完整 test、debug build 与文档门禁。
- 验证结果：主窗口由 0 次 pointer callback 变为正确转发，独立窗口继续通过；库 300、应用 301、Doc tests 0，所有本机门禁和构建通过。
- 风险/待办：真实 GUI 交用户在新构建中验收；Windows/Linux 原生环境由 CI 验证；未自行截图或启动应用。
- 开工判定：施工和本机验证完成。

## 2026-10-02 通用滚轮后 Home 设置预检

- 范围：Rust 配置和 application 输入策略、Slint 设置页/callback、翻译生成源与回归。
- 本机环境：Rust/Cargo 1.97.1，aarch64/x86_64 macOS targets 已安装；项目 MSRV 1.92.0、Rust 2024、Slint 1.18.1。
- 环境变化：无工具链/依赖/CI 变更；使用现有 locked/offline Cargo 与 Python 标准库翻译检查。
- 开工判定：允许开工；先计划后实施，保留上一轮未提交修改。
- 验证安排：先定向输入/旧配置回归，再 fmt/check/严格 Clippy/完整测试/build；GUI 由用户验收，Windows/Linux 原生验证交 CI。

## 2026-10-02 通用滚轮后 Home 设置验证

- 范围：默认关闭设置的持久化/预览/保存与一次 Home 输入策略；未改工具链、依赖、transport 或 SSH 安全契约。
- 验证：3 项定向回归、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 301、应用 303、Doc tests 0）、debug build、510 条翻译、8 条 Markdown 相对链接、skill/tracker validators 与 diff 均通过。
- 环境结果：macOS ARM64 原生 Rust/Cargo 1.97.1 验证完成；Slint 入口已重编译。
- 缺口：GUI/真实备用屏幕交互由用户确认；Windows/Linux 原生 CI 未执行。

## 2026-10-02 滚轮目标行首输入顺序验证

- 目的：每个纵向滚轮步先移动，再请求目标行首。
- 改动范围：application 输入 helper、设置说明/翻译、配置字段注释和双语文档；依赖、toolchain、CI、SSH trust 与凭据不变。
- 执行内容：按完整 `Up/Down + Home` 组合逐步发送并更新回归；重编译 Slint，执行 locked/offline Cargo 门禁和 debug build；通过隔离 tmux socket 检查不同长度行及空行。
- 验证结果：Rust/Cargo 1.97.1 的 3 项定向回归、fmt/check/严格 Clippy、完整测试（库 301、应用 303、Doc tests 0）、build、510 条翻译、8 条相对链接、skill/tracker/diff 通过；tmux 上下六步完成后均为第 0 列，实验 server 已清理。
- 风险/待办：目标程序的自定义按键绑定决定实际 Home 行为；Windows/Linux 原生 CI 和 GUI 由对应平台验收，不自行捕获应用截图。

## 2026-10-02 光标呈现合并施工预检

- 项目边界：terminal model 只读模式查询与 application 快照呈现；不改 Slint 绘制、transport 批次或 SSH 安全边界。
- 环境记忆状态：已核对 Cargo、build/CI 与源码，无依赖或工具链漂移。
- 运行环境：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1，锁定 Slint 1.18.1；已存在 Tokio test-util 虚拟计时支持。
- 测试计划：确定性光标合并/超时回归，fmt、locked/offline check/Clippy/test/build、链接、tracker/skill 与 diff。
- 开工判定：允许开工。GUI 由用户验收；Windows/Linux 原生 CI 本机不可替代。

## 2026-10-03 光标呈现合并验证完成

- 项目边界：terminal model 只读协议可见性及 application 固定 24 ms 呈现合并；parser、响应发送、Slint 闪烁偏好和 SSH 安全边界保持原有所有权。
- 环境记忆状态：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1；依赖、锁文件、Slint/build 入口和 CI 矩阵无变化。
- 验证结果：6 项新定向回归、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 301、应用 309、Doc tests 0）、debug build、4 条相对链接、skill/tracker 与 diff 检查通过。
- 交付状态：代码与回归已提交为 `aebc44e`；配套文档单独提交，不推送远端。
- 风险与缺口：24 ms 只合并短暂 hide/show，持续隐藏仍生效；GUI 由用户验收，Windows/Linux 原生 CI 尚未运行；不自行捕获应用截图。

## 2026-10-03 收起侧栏 Group 样式施工预检

- 项目边界：SidebarRailItem 紧凑文字组项的背景、边框、字体和宽度；无 Rust/SSH/凭据或持久化变更。
- 环境记忆状态：已核对 manifest、锁定 Slint、build.rs 与 CI，环境无漂移；本机 Rust/Cargo 1.97.1，Rust 2024/MSRV 1.92.0。
- 测试计划：Slint 重新编译，fmt、locked/offline check/严格 Clippy/test/build、双语文档链接、skill/tracker/diff。
- 开工判定：允许开工。GUI 以用户截图和确认验收；Windows/Linux 原生 CI 未在本机执行。

## 2026-10-03 收起侧栏 Group 样式验证完成

- 改动范围：SidebarRailItem 紧凑文字样式及双语使用说明/跟踪文档；依赖、工具链、CI、Rust 领域与 SSH 安全边界不变。
- 验证结果：Slint 已重新编译；fmt、locked/offline check、严格 all-target Clippy、完整测试（库 301、应用 309、Doc tests 0）、debug build、4 条 Markdown 相对链接、skill/tracker/diff 通过。
- 交付状态：最新 debug 可执行文件已生成；样式及配套文档独立提交，不推送远端。
- 风险与缺口：GUI 外观和真实焦点交互由用户验收；Windows/Linux 原生 CI 本机未运行。不自行生成或查看应用截图。

## 2026-10-03 重复链路施工预检

- 时间：2026-10-03 12:22 +0800
- 目的：验证三批重复链路清理的运行、测试和打包环境。
- 改动范围：Rust/Slint 应用、历史资源、配置写入和 Linux packaging；保留当前 SSH/凭据契约。
- 执行内容：读取环境与地图，核对 Cargo/build/CI，确认本机 Rust/Cargo 和已安装 targets。
- 验证结果：Rust/Cargo 1.97.1，两个 macOS target；声明 Rust 2024/MSRV 1.92.0、Slint 1.18.1，允许开工。
- 风险/待办：本轮完整门禁待执行；Windows/Linux 原生平台及 GUI/真实 SSH 由 CI/用户验收。

## 2026-10-03 重复链路清理聚焦验证与打包环境

- 任务边界：闲置 UI/worker 链路、共同应用流程、原子写入和 Debian 资产清单；保持 Rust/Slint/SSH 架构与既有未提交 UI 修改。
- 环境核对：Rust/Cargo 1.97.1，Rust 2024/MSRV 1.92.0；两个 macOS target 已安装。Rust 依赖和 lockfile 不变。
- 环境变更：Release Linux 固定安装 cargo-deb 3.7.0 和 dpkg-dev，由 Cargo metadata 提供资产，运行库依赖交 native discovery。上游依据：https://github.com/kornelski/cargo-deb 。
- 已通过：detached 5、probe 2、原子写入 3、发布 helper 14、512 条翻译、22 条相对链接及 skill/tracker/diff；Debian amd64/arm64 fixture 各 30 项资源内容/路径/权限/所有权和控制字段一致。
- 环境排障：默认 Python 3.10 无 tomllib，fixture 改用已有 Python 3.13，未新增 Python 依赖。
- 验证限制：fixture 是资源清单验证，非 Linux ELF、依赖发现或安装验证；Windows/Linux native CI 和用户 GUI/真实连接待验收。最终 native/target 门禁进行中。

## 2026-10-03 重复链路清理环境验证完成

- 项目边界：三批清理及共同流程已完成，Rust 依赖/lockfile、CI target 矩阵、SSH trust 和凭据策略保持原状；已有 UI 修改经基线复核保留。
- 本机门禁：Rust/Cargo 1.97.1 的 fmt、locked/offline check、严格 all-target Clippy、完整测试（库 303、应用 312、Doc tests 0）和 debug build 全部通过。
- 平台门禁：`CARGO_NET_OFFLINE=true` 下执行 CI 同款 `cargo check --locked --target <target>`、`cargo clippy --all-targets --locked --target <target> -- -D warnings`、`cargo build --locked --target <target>`，两个 macOS target 全部通过；不运行非原生 Intel tests。
- 配套门禁：发布 helper 14、512 条翻译、23 条 Markdown 相对链接、Release YAML/18 段 Bash、skill/tracker/diff 检查通过；amd64/arm64 Debian fixture 每包 30 项资源与控制字段一致。
- 环境与验收限制：未提供 Windows/Linux SDK/原生 runner；真实 GUI/拖放/连接和 Linux 真包安装/运行库依赖需 native CI/用户验证。不自行截图，不执行提交、push、tag 或发布。
- 开工判定：代码交付和本机验证完成，限制已记录。

## 2026-10-10 标准终端语义高亮施工预检

- 项目边界：独立 Rust 应用的私有 terminal render mapper，新增 regex 直接依赖及启动预编译；保持 Slint/worker/SSH 和四类颜色设置边界。
- 环境记忆状态：已读取 current，核对 Cargo/build.rs/CI 和工具链；刷新本轮范围，原验证数字仅作历史基线。
- 运行环境：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1；aarch64/x86_64 macOS target 已安装。
- 环境变化：Cargo.lock 已有 regex 1.13.1，仅增加根包依赖边，不升级 regex、Slint、Tokio 或 russh。
- 测试环境：标准等级/HTTP 上下文/样式专项后，执行 fmt、locked/offline check、严格 all-target Clippy、完整 tests、debug build、文档链接/skill/tracker/diff 校验。
- 风险与缺口：Windows/Linux native SDK 不可用，真实终端高亮由用户验收，不自行截图。
- 开工判定：允许开工；计划已先行写入 tracker，无多 agent。

## 2026-10-11 独立缓存与 Skia 本地归档验证

- 变化摘要：现有 cargo run 在 Skia 构建阶段持有 target 锁，独立 APFS clone 缓存避免互相等待；GitHub 连接超时，复用已有 Skia 完整 tar 归档。
- 受影响文件：`ui/settings/terminal.slint`、`scripts/build_zh_catalog.py`、`translations/zh-CN/LC_MESSAGES/ax_ssh.po`、环境与实施记录；不修改 vendor、工具链或 CI。
- 更新后的命令或环境：本轮 Cargo 命令使用临时 CARGO_TARGET_DIR 和 SKIA_BINARIES_URL，后者由 skia-bindings 上游 build support 支持 file URL；仅使用原锁定版本/目标/特性的本机归档。
- 验证结果：tar 完整读取，tag 0.153.3 与 b7f043e0b1e2a850e702/ARM64/ganesh-gl-jpegd-jpege-metal-pdf key 一致；538 条中文目录和 Slint 重编译通过，Rust 门禁继续进行。
- 风险与缺口：不停止其他 cargo run；Windows/Linux 与 GUI 验收仍由 CI/用户完成。


## 2026-10-11 标准终端语义高亮环境验证完成

- 项目边界：Rust 私有 regex 匹配与渲染模块、启动预编译、Settings 双语提示；worker、transport、SSH trust、凭据和配置 schema 保持原有所有权。
- 环境变化：仅将已有 regex 1.13.1 提升为直接依赖，根包 lockfile 依赖同步；MSRV、工具链、Slint/Tokio/russh 和 CI 矩阵未改变。
- 验证结果：macOS ARM64 上 25 项专项、682 项全量测试（315 library + 367 binary）、fmt、locked/offline check、严格 all-target Clippy、debug build 通过；Slint 已重编译，538 条翻译、10 条 Markdown 相对链接、skill/tracker/diff 校验通过。
- 构建环境：独立 APFS clone 缓存与已验证的 Skia 同版本/目标/特性归档；所有本轮 Cargo 命令保持同一临时 CARGO_TARGET_DIR 和 SKIA_BINARIES_URL，未安装或替换外部依赖，未停止其他进程。
- 交付状态：可运行原生程序位于 `target/highlighting-preview/ax_ssh`；未启动 GUI，不截图、不提交或发布。
- 风险与缺口：Windows/Linux 原生 CI 尚未执行；真实终端高亮及自定义配色由用户验收，保留 512 字节 ASCII run 与不跨样式/行识别的界限。
- 开工判定：代码施工与本机自动验证完成。


## 2026-10-11 URL 目标修复施工预检

- 项目边界：私有 terminal_targets 识别与实际选区回归；Slint DTO、worker、SSH trust、凭据和平台 CI 保持原状。
- 环境记忆状态：已读取环境记录、Cargo 和项目地图；本机 Rust/Cargo 1.97.1、Rust 2024/MSRV 1.92.0 与此前一致，原 target 锁已释放，保留独立构建缓存以复用已验证 Skia 归档。
- 环境变化：url 2.5.8 原已存在于 Cargo.lock，计划提升为直接依赖，MSRV 1.63；regex 1.13.1 沿用本轮直接依赖，不升级工具链或 crates。
- 测试环境：先复现用户 `)。HTTP` 原文，再执行 URL/真实选区专项和 fmt/check/严格 Clippy/test/build、双语链接与记录校验。
- 风险与缺口：中文行文标点边界属于终端提取策略，URL parser 不能独自推断；保持原文、1024 字符 URL/2048 字符上下文上限与 OSC 8 语义，GUI 由用户验收。
- 开工判定：允许开工，URL 修复计划已先行记录，无多 agent。


## 2026-10-11 URL 目标修复环境验证完成

- 时间：2026-10-11 08:33 +0800
- 项目边界：应用私有 regex/URL parser 与共享目标区间，保持原有字符/cell/软换行、OSC 8、Slint DTO 及 worker/SSH 契约。
- 环境变化：regex 1.13.1 和 url 2.5.8 仅增加根包直接依赖边；url MSRV 1.63 满足项目 1.92。无 crate、工具链或 CI target 变化。
- 验证结果：macOS ARM64 上 12 项目标/路径与 2 项实际选区专项、689 项完整测试（315 library + 374 binary）、fmt、locked/offline check、严格 all-target Clippy、debug build 通过；538 条翻译、Markdown 相对链接、skill/tracker/diff 校验通过。
- 构建环境：沿用独立缓存和同版本/目标/特性的 Skia 本地归档；没有中断其他 Cargo/应用进程，预览路径已原子更新并与构建结果一致。
- 风险与缺口：用户原文在终端模型中修复前失败/修复后通过；实际 GUI 双击/悬停仍待用户确认，Windows/Linux 原生 CI 未在本机运行。
- 开工判定：施工和本机自动验证完成。

## 2026-10-11

- 为剪贴板和麦克风实施完成开工补充，记录 CPAL 版本/ALSA/TCC 要求及原生验证边界。

## 2026-10-11 终端媒体最终环境验证

- 日期：2026-10-11 10:16 +0800
- 变化摘要：剪贴板图片、CPAL/rubato 多格式麦克风、独立 SSH 音频通道及 whisper.cpp 转写实现完成，保持 Rust 2024/MSRV 1.92、Slint/russh 版本和原生 CI 矩阵。
- 受影响文件：Cargo、Rust/Slint/config、macOS 用途声明、Linux CI/release ALSA 依赖、双语开发/使用/架构与实施记录。
- 更新后的命令或环境：继续使用 locked/offline 本机门禁；可用的两个 macOS target 执行 CI 同款 check/严格 Clippy/build，Skia 使用匹配架构的完整本机归档；不运行非原生 Intel 测试。
- 验证结果：最终 707 项测试、45 种格式、fmt/check/严格 Clippy、两架构构建、翻译/链接/skill/tracker/diff 与最终 ARM64 app 资源/签名通过。预览在 `target/agent-media-preview/AxSSH.app`；Windows/Linux CI、真实剪贴板/音频/远端/CLI 仍待验收。

# 2026-10-11 SFTP 性能施工预检

- 项目边界：独立 Rust/Slint 项目的 SFTP 请求、worker 生命周期与 UI model；SSH trust/凭据/协议边界保持。
- 环境记忆状态：已读取并核对 Cargo、lockfile、build.rs、CI、地图、源码；不升级工具链或依赖。
- 运行环境：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1，Slint 1.18.1、russh 0.64.1、russh-sftp 3.0.1；Cargo 管依赖，build.rs 编译 ui/app.slint。
- 测试环境：模拟 SFTP/无窗口 Slint 回归，locked/offline fmt/check/严格 all-target Clippy/test，两个 macOS target check/Clippy/build，文档/skill/tracker/diff 检查。
- 环境变化检查：否；macOS ARM64/x86_64 target 已安装；Windows/Linux SDK 未安装，交原生 CI。
- 开工判定：允许开工。真实 SSH/GUI 验收由用户完成，不自行截图。

## 2026-10-11 SFTP 性能完整验证

- 变化摘要：目录首批发布、后台有序写队列、16 分块请求窗口、持续本地读取及稳定 UI 源快照/图标缓存已实现；工具链、依赖和 CI 矩阵不变。
- 更新后的命令或环境：保留 locked/offline 门禁；两种 macOS 架构使用 CI 同款 target check/严格 all-target Clippy/build。Intel Skia 下载失败后，复用完整且 tag 0.153.3、x86_64/特性 key 匹配的本机归档，使用上游 `SKIA_BINARIES_URL=file://…` 环境入口完成门禁。
- 验证结果：fmt/check/严格 Clippy、720 项测试（339 library + 381 binary，Doc tests 0）、两架构构建与 Mach-O 检查、Markdown/skill/tracker/diff 通过。模拟服务器等待完整窗口再回应，覆盖逆序、短读、最终复核失败和取消；吞吐改善仍需真实远端测量。
- 验证边界：本机未启动或截图 GUI，不运行非原生 Intel 测试；Windows/Linux 原生 CI、真实 SSH 生命周期和用户视觉验收保留。

## 2026-10-11 SFTP 外部编辑完整环境验证

- 时间：2026-10-11 18:52 +0800
- 变化摘要：完全移除内置编辑器，复用系统关联 opener、私有副本、保存监听和 guarded 原子回传；保持前轮 SFTP 性能实现与 SSH 安全边界。
- 环境变化：无依赖、工具链或 CI 变化；Intel Skia 复用已验证的匹配本机归档。
- 验证结果：最终 737 项测试、fmt/check/严格 all-target Clippy、两个 macOS target 的 CI 参数 check/Clippy/build、两份 Mach-O 架构、559 条翻译、30 条 Markdown 相对链接、skill/tracker/diff 全部通过。
- 风险与缺口：Windows/Linux 原生 SDK 不可用；真实 GUI/系统关联/远端回传待验收，不自行截图、运行非原生 Intel 测试或发布。

## 2026-10-11 外部编辑大小设置施工预检

- 项目边界：独立 Rust/Slint 的 config、设置桥接及 SFTP 外部编辑大小契约。
- 环境记忆：已读取并核对 Cargo/lock/build.rs/CI/源码与本机 Rust 1.97.1；保持 Rust 2024/MSRV 1.92.0、锁定 Slint/russh 与既有 open/notify。
- 环境变化：否；不新增依赖，两个 macOS target 可用，Windows/Linux SDK 缺失。
- 测试环境：旧配置和持久化回归、动态容量/协议测试，完整 locked/offline Cargo 门禁、Slint 重编和文档/翻译/skill/tracker/diff 检查。
- 开工判定：允许开工；计划先行，GUI/真实远端仍由用户验收。

## 2026-10-11 编辑上限设置环境验证完成

- 时间：2026-10-11 19:23 +0800
- 变化摘要：配置 schema v36 新增 remote_edit_max_file_mib，1–64 MiB/default 4，主窗与独立窗口设置/搜索/保存一致，编辑操作按当前值校验。
- 环境变化：无依赖、工具链或 CI 修改。
- 验证结果：24 项专项、743 项完整测试、fmt/check/严格 all-target Clippy、native ARM64 build/Mach-O、562 条翻译、30 条链接和 skill/tracker/diff 通过；程序为 `target/debug/ax_ssh`。
- 验证缺口：本轮未跑 Intel/Windows/Linux 原生 CI，GUI/系统程序/真实远端仍待验收；不自行截图、提交或发行。

## 2026-10-11 标题栏图标预检

- 当前范围：Slint 共用 SVG/Theme 与 macOS 原生按钮外观。
- 环境核对：Cargo/lock/build.rs/CI 与 Rust/Cargo 1.97.1、两个 macOS target 一致；Rust 2024/MSRV 1.92.0、Slint 1.18.1 保持。
- 开工判定：允许开工；无依赖或 CI 变化，Windows/Linux SDK 缺失，GUI 由用户验收。

## 2026-10-11 23:07 +0800 窗口动作图标施工预检

- 目的：统一主/子窗口图标前复核 Slint 与 macOS 原生桥接的环境。
- 改动范围：`ui/`、`src/app/macos_window.rs`、`src/app/window_bridge.rs`、`src/app/view/settings.rs` 与双语文档；无依赖、工具链或 CI 变化。
- 执行内容：读取现有记忆、Cargo/lock/build/CI、工程技能及相关源码，确认 Rust/Cargo 1.97.1 与两个 macOS target 已安装。
- 验证结果：Rust 2024/MSRV 1.92.0、Slint 1.18.1 保持，允许开工；自动门禁待实现后执行。
- 风险/待办：Windows/Linux 缺少 SDK；GUI 视觉和原生交互由用户验收。
- 开工判定：允许开工。

## 2026-10-11 图标与右键重命名 Intel 构建环境复核

- 触发原因：Intel target 的 Skia 预编译归档下载发生 HTTP/2 错误，源码回退无法连接 googlesource；Cargo offline 不覆盖依赖 build script 的下载。
- 执行内容：从本机既有缓存找到完整 gzip/tar，核对 tag 0.153.3 与 b7f043e0b1e2a850e702-x86_64-apple-darwin-ganesh-gl-jpegd-jpege-metal-pdf key；复制到本轮临时缓存，以上游支持的 SKIA_BINARIES_URL=file URL 重新运行 Intel check/严格 all-target Clippy/build。
- 环境变化：仅临时环境变量与构建缓存；未修改 Cargo/lockfile、工具链、依赖或 CI。
- 验证结果：ARM64 默认和显式 target 门禁已通过；Intel 本地归档门禁运行中。
- 风险/待办：不运行非原生 Intel 测试；Windows/Linux 和 GUI/真实远端验收保留。

## 2026-10-12 00:01 +0800 图标与右键重命名环境验证完成

- 改动范围：Slint 共用动作图标、AppKit 原生按钮、SFTP 行菜单/共享名称弹窗、应用层目标重验及双语记录。
- 环境变化：无依赖、工具链或 CI 修改；Intel 验证使用匹配 tag/架构/特性的本机 Skia 归档与临时环境变量，初次下载失败已恢复。
- 验证结果：2 项重命名目标专项、fmt、locked/offline check、严格 all-target Clippy、库/应用完整测试（应用 396 项）、默认 build 和 ARM64/Intel 的 CI 同款 check/严格 Clippy/build 全部通过；三份 Mach-O 架构正确。562 条翻译、10 条相对文档链接与 skill/tracker/diff 通过。
- 验证缺口：不运行非原生 Intel 测试；Windows/Linux 原生平台、GUI 视觉/焦点与真实 SFTP 由 CI/用户验收。
- 交付：新版程序为 `target/debug/ax_ssh`，目标程序为 `target/<macOS-target>/debug/ax_ssh`；不自行截图、不推送或发布。

## 2026-10-12 终端媒体按需显示环境预检

- 项目边界：独立 Rust 2024/Slint 桌面应用；本轮仅终端菜单/状态布局、反馈清理与回归，不改设备、transport、配置、依赖或 CI。
- 环境记忆状态：已读取 current/changes，核对 Cargo/lock/build.rs/CI；工具链、锁定依赖、Slint 入口与前轮一致。
- 运行环境：本机 Rust/Cargo 1.97.1，MSRV 1.92.0、Slint 1.18.1，已安装两个 macOS target。
- 测试环境：复用现有 Slint MinimalSoftwareWindow 数值/事件 harness 和媒体生命周期专项，再运行 fmt、locked/offline check、严格 Clippy、完整 test/build 和文档 validators；不保存或检视应用截图。
- 风险/待办：GUI、真实设备/录音授权/whisper-cli/远端音源及 Windows/Linux 原生验收保留。
- 开工判定：允许开工。

## 2026-10-12 标准录音自动桥接范围复核

- 范围：用户选择 CLI 自行录音并要求标准化；从手动菜单收口改为标准 source-output/non-corked 自动启停，增加 worker/audio owned 配置与设备租约。
- 环境：沿用 Rust 2024/MSRV 1.92.0、Slint 1.18.1、CPAL 0.17.3、Tokio/russh 锁定依赖，manifest/lock/toolchain/CI 不变。
- 证据：6 项标准录音与真实 POSIX helper fixture、2 项主/独立数值布局回归通过；完整 Cargo 与平台门禁进行中。
- 缺口：远端 Linux 需要用户级 PulseAudio/pipewire-pulse 与 pactl，Windows/Linux 原生 SDK 和真实麦克风/GUI 由目标环境验收。

## 2026-10-12 标准录音自动桥接环境验证完成

- 运行环境：Rust 2024/MSRV 1.92.0、锁定 Slint/CPAL/Tokio/russh 和现有 CI 保持；本机 Rust/Cargo 1.97.1 验证完成。
- 验证：fmt、locked/offline check、严格 all-target Clippy、完整 754 项测试、默认 build；两个 macOS target 的 CI 同款 check/严格 Clippy/build 和架构检查通过，Intel 沿用已核对的本地 Skia file URL。未运行非原生 Intel 测试。
- 文档/打包：549 条译文及隔离生成一致、10 条相对链接、plist、skill/tracker/diff 通过；`target/automatic-audio-preview/AxSSH.app` 用途文案/ad-hoc 严格签名通过，默认程序为 `target/debug/ax_ssh`。
- 缺口：未启动 GUI 或真实麦克风、未连接真实远端；Linux 远端需标准 PulseAudio/pipewire-pulse/pactl，Windows/Linux 原生 SDK 与实际设备链路仍由目标环境验收。
- 开工判定：施工与自动门禁完成，用户实机验收保留。

## 2026-10-12 22:15 +0800 覆盖率测量施工预检

- 日期：2026-10-12
- 变化摘要：刷新当前范围到实际控制器/worker 覆盖率；核实两个 macOS target 均已安装，识别缺失的 coverage 测量工具。
- 受影响文件：`docs/project-env-audit/current.md`、`docs/project-implementation-tracker/current.md`。
- 更新后的命令或环境：继续 Rust 2024/MSRV 1.92.0、Slint 1.18.1、russh 0.64.1 和 locked/offline；拟补齐 cargo-llvm-cov/llvm-tools-preview，不变更项目依赖。
- 验证结果：本机 Rust/Cargo 1.97.1、fmt 1.9.0、Clippy 0.1.97、Python 3.10.20 和两个 macOS target 可用；真实设备/系统凭据不访问，覆盖率与全量测试待执行。

## 2026-10-12 23:25 +0800 覆盖率工具与隔离测试环境

- 日期：2026-10-12
- 变化摘要：cargo-llvm-cov 0.9.1 与匹配 LLVM tools 安装完成；Skia 使用本机匹配归档，原始 770 项默认并发基线通过。
- 受影响文件：`docs/project-env-audit/current.md`、`docs/coverage.md`、`docs/coverage.zh.md`、`docs/development.md`、`docs/development.zh.md`。
- 更新后的命令或环境：cargo llvm-cov --locked --offline --workspace --lcov；Slint event-loop proxy 与 logger/panic hook 测试用子进程隔离，profile 由子进程继承；不更改 Rust/MSRV/依赖/CI。
- 验证结果：工具归档已核对工具链 manifest SHA-256；初始 check、基线和设备/凭据/日志回归通过，严格 Clippy 与文档校验通过；最终全量/coverage 和原生 CI 参数验证正在执行。

## 2026-10-12 23:52 +0800 覆盖率最终环境与验证

- 日期：2026-10-12
- 变化摘要：本轮无依赖/MSRV/工具链/CI 变更；本机 coverage 测量与隔离测试工具已配置并验证。
- 受影响文件：`docs/project-env-audit/current.md`、`docs/coverage.md`、`docs/coverage.zh.md`、`docs/project-implementation-tracker/current.md`。
- 更新后的命令或环境：同一 Rust 1.97.1/cargo-llvm-cov 0.9.1/native ARM64；匹配的本机 Skia 归档仅通过临时环境变量使用；保留 locked/offline 和 CI 原有 target 矩阵。
- 验证结果：普通/插桩均默认并发 799 项、fmt/check/严格 all-target Clippy、ARM64 CI 同款 check/Clippy、Markdown/skill/tracker/diff 全通过；canonical src 覆盖率 57.96% -> 65.78%，HTML/LCOV/JSON 已生成。Intel/Windows/Linux 和线上 Codecov 待原生 CI；真实设备、系统凭据和 GUI 未访问。

## 2026-10-13 07:23 +0800 代码职责合规环境预检

- 目的：确认代码错误处理、安全迁移和职责拆分可在现有环境实施。
- 改动范围：`src/`、`ui/` 与配套双语文档和跟踪；不更改 manifest、lockfile、工具链或 CI。
- 执行内容：核对 Rust/Cargo 1.97.1、两个 macOS target、Rust 2024/MSRV 1.92.0、锁定 Slint/russh 和单 UI 构建入口；启动 baseline offline check。
- 验证结果：轻量环境核对完成，允许开工；代码和目标门禁随后执行。
- 风险/待办：系统凭据使用隔离实现；Windows/Linux/GUI 与真实后端人工验收保留。

## 2026-10-13 08:37 +0800 合规施工最终构建与验证

- 日期：2026-10-13
- 变化摘要：工具链、依赖和 CI 未改变；代码职责及安全/错误边界修复完成。
- 受影响文件：本轮 `src/`、`ui/`、双语 architecture/usage 和环境/实施记录。
- 更新后的命令或环境：保留 locked/offline 门禁，两个 macOS target 使用 CI 同款 check/Clippy/build；Skia 外部下载停滞恢复为同版本/架构/特性完整本地归档的临时 file URL，临时 7897 代理可用，无全局修改。
- 验证结果：804 Rust、22 Python、665 × 11 翻译、145 Markdown 相对链接、skill/tracker/diff 及两个 macOS 架构门禁全部通过；Intel 仅编译链接，Windows/Linux/真实系统后端与 GUI 保留原生 CI/人工边界。

## 2026-10-13 23:10 +0800 SFTP 四项视觉施工环境预检

- 日期：2026-10-13
- 变化摘要：范围切换为 SFTP 操作/导航与样式；环境声明保持。
- 受影响文件：`ui/`、`src/app/view/`、翻译和配套记录。
- 更新后的命令或环境：Rust/Cargo 1.97.1、Rust 2024/MSRV 1.92.0、两个 macOS target，Cargo 锁定依赖；不改变 CI/配置 schema。
- 验证结果：预检允许开工；自动门禁待实现，GUI 验收由用户执行，Windows/Linux 交原生 CI。

## 2026-10-14 00:12 +0800 SFTP 四项视觉最终环境验证

- 日期：2026-10-14
- 变化摘要：Slint 页面/DTO 与有界路径展示完成，工具链/依赖/CI 保持。
- 受影响文件：`ui/`、`src/app/view/`、11 个语言目录和双语/实施记录。
- 更新后的命令或环境：锁定 offline Cargo 门禁、ARM64/Intel CI 同款 check/严格 all-target Clippy/build；临时匹配 Skia file URL 与 7897 代理，无全局设置变化。
- 验证结果：60 专项、842 Rust、8 Python、665 × 11 翻译、119 相对链接、九个 SVG 与 skill/tracker/staged diff 通过，两架构 Mach-O 核对；实际 GUI/焦点与 Windows/Linux 原生验证待用户/CI 验收。

## 2026-10-14 07:54 +0800 SFTP 直接路径施工预检

- 日期：2026-10-14
- 变化摘要：当前 UI 范围改为直接路径与中间省略/菜单；环境和工具链保持。
- 受影响文件：`ui/`、应用展示/DTO/回归、双语说明与实施记录。
- 更新后的命令或环境：本机 Rust/Cargo 1.97.1、两 macOS target、锁定 offline Cargo 门禁与 CI 同款目标检查。
- 验证结果：预检允许开工；实际字符串宽度/交互及完整验证待实现后执行，GUI 验收由用户完成。

## 2026-10-14 08:25 +0800 SFTP 直接路径最终环境验证

- 日期：2026-10-14
- 变化摘要：直接路径、中间省略与齿轮菜单替换分段模型；环境保持。
- 受影响文件：`ui/`、`src/app.rs`、`src/app/view/`、双语说明和配套记录。
- 更新后的命令或环境：标准 locked/offline Cargo 门禁和 ARM64/Intel CI 同款目标检查/Clippy/build；临时匹配 Skia file URL 和 7897 代理，无全局修改。
- 验证结果：59 专项/841 Rust、8 Python、665 × 11 翻译、119 相对链接、六个 SVG 和 skill/tracker/diff 全通过；两架构 Mach-O 正确，实际 GUI/键盘与 Windows/Linux 原生验证待用户/CI。

## 2026-10-15 16:39 +0800 Windows 滚轮原生复现环境核对

- 日期：2026-10-15
- 变化摘要：当前排查边界为 Windows/macOS 原生滚轮差异；刷新已过时的默认工具链/targets，并区分原生 CI 和真实 GUI 复现。
- 受影响文件：`src/app/terminal_bridge/{native,input}.rs`、双语开发说明、环境及实施记录。
- 更新后的命令或环境：本机默认 rustc/Cargo 1.99.0，targets 为 macOS ARM64 和 Windows MSVC；没有本轮工具链/依赖安装。VirtualBox 启动目标不存在，未发现 Windows VM/远程桌面执行工具；用户在另一台 Windows 主机运行编译产物，当前无法访问。
- 验证结果：读取 GitHub run `38677130493`/Windows job `116233176549` 的实际日志，`25335e0` 的原生 check/严格 Clippy/build 和 814 项测试通过；现有滚轮组件回归为无窗口注入，不能作为实际故障已经复现或排除的证据。允许补充默认关闭的脱敏诊断，本轮代码门禁及现场对照待完成。

## 2026-10-15 16:44 +0800 滚轮诊断本机验证完成

- 日期：2026-10-15
- 变化摘要：原生滚轮/实际模式/入队结果的元数据诊断与双语复现方法就绪，未更改滚轮策略。
- 受影响文件：`src/app/terminal_bridge/{native,input}.rs`、双语开发说明和环境/实施记录。
- 更新后的命令或环境：既有 Rust 1.99.0/macOS ARM64、locked/offline Cargo 门禁；产物 `target/debug/ax_ssh`，无依赖/工具链/CI 变化。
- 验证结果：相关定向回归、fmt/check/严格 all-target Clippy/870 项测试/build 通过，无 warning；10 个相对链接和本轮 tracker 记录有效，84 项历史问题无新增。故障 Windows 主机、实际 GUI/Codex 和新代码 Windows 原生门禁仍未验证，不以历史 CI 或无窗口回归替代。

## 2026-10-15 17:23 +0800 滚轮到 SSH 接收端测试环境验证

- 日期：2026-10-15
- 变化摘要：新增主/独立窗口 UI 到真实 loopback SSH 服务端字节的回归，增强既有 CI 的可验证边界。
- 受影响文件：`src/app/behavior_tests.rs`、`src/app/behavior_tests/terminal_wheel.rs`、双语开发说明和环境/实施记录。
- 更新后的命令或环境：复用锁定 russh/Tokio/Slint、固定测试 host key/凭据、bounded loopback listener/队列及标准 Cargo 门禁；没有新增软件、运行时依赖或 CI targets。
- 验证结果：两项接收测试及标准 fmt/check/严格 all-target Clippy/872 项测试通过，无 warning；10 个相对链接和本轮 tracker 记录有效，84 项历史问题无新增。新测试在原生 Windows/Linux CI 的执行与实际 OS 鼠标/Codex 复现分别保留，不以本机结果替代。


## 2026-10-20 09:08 +0800 全项目整理环境快照

- 触发原因：用户要求全项目按规范整理，现有 current 混排多年任务和旧版依赖/目标事实。
- 执行内容：用 Cargo/lock/CI、本机 CLI 和最终测试核对当前状态；旧 current 的 105363 字节逐字归档到 `history-before-2026-10-20.md`，新 current 只保留当前环境、命令、验证边界及历史链接。
- 影响文件：`docs/project-env-audit/current.md`、`docs/project-env-audit/history-before-2026-10-20.md`、`docs/project-env-audit/changes.md`。
- 环境事实：Rust 2024/MSRV 1.92.0、锁定 Slint 1.18.1/russh 0.64.1 和升级后的媒体依赖；本机 rustc/Cargo 1.99.0、Python 3.14.6，ARM64 macOS/Windows MSVC Rust targets；本轮未安装工具链/SDK/包或调整依赖。
- 验证：布局移动后的 880 项 Rust 回归已通过；基线 fmt/check/严格 Clippy、665 × 11 翻译、生成图标和本地链接已通过，最终源码及新增校验脚本收尾见实施记录。
- 剩余边界：无 Windows/Linux 原生环境，不执行非原生 Intel 程序；真实设备/keyring/GUI/远端仍由对应平台验收，不启动应用或自主截图。

## 2026-10-22 12:05 +0800 egui 独立前端与最低工具链

- 触发原因：用户授权开始实施 egui 迁移，需要验证新增依赖和调整后的 MSRV。
- 执行内容：加入可选 egui/eframe 0.37.0 + WGPU，Slint/build/softbuffer 改为默认 feature；新增实验 binary；安装 Rust 1.95.0 最低工具链并保持默认 1.99，更新 CI 的五目标双前端和最低工具链检查。
- 影响文件：`Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`src/egui_main.rs`、`src/egui_app/`、`src/controller/`、相关规范、双语文档和环境记录。
- 环境事实：egui-only 图不含 Slint/softbuffer；1.95 两前端 check 通过，默认 909 项测试通过；27 项 Python 回归、675 × 11 翻译和生成资源校验通过。新 egui 完整验证和目标命令收尾见实施记录。
- 剩余边界：本机 ARM64 macOS；其它平台交原生 CI；原生 GUI/IME/字体/性能未验收，实验 WGPU 无等价软件回退，默认 Slint 保留。没有运行 GUI、外部账号或真实 SSH/凭据操作。

## 2026-10-24 E7T 实施前环境复核

- 目的：核实继续 egui 设置迁移的实际环境。
- 改动范围：`docs/project-env-audit/current.md`、egui/controller 设置应用链。
- 执行内容：读取 Cargo/CI 与现有环境记录；确认 rustc/Cargo 1.97.1、Python 3.10.20，以及 macOS ARM64/Intel targets；纠正旧快照中的 1.99、Python 3.14、Windows target 和仅本地原型范围。
- 验证结果：允许开工；离线 check 缺少 eframe，locked 联网恢复进行中；未升级依赖。
- 风险/待办：Rust 构建/测试取决于锁定依赖可用性；Windows/Linux、真实设备、GUI 和性能仍需独立验证。

## 2026-10-24 E7T 缓存恢复与本机门禁完成

- 目的：确认缺包恢复后本轮实现可复现。
- 改动范围：环境快照及终端/SFTP 设置链，未调整依赖版本、Cargo.lock 或工具链。
- 执行内容：Cargo 获取已有锁定依赖后，切回 locked/offline 运行两前端 check/严格 Clippy/test/build、组合 all-features/all-targets check 及静态校验。
- 验证结果：macOS ARM64 默认 930、egui 538 项测试通过；两前端构建和检查、组合检查、79 项 controller 聚焦回归、27 Python 与翻译/文档/skill/tracker/diff 通过。
- 风险/待办：其它平台、显式 target/MSRV 本轮未重跑；新设置 GUI、真实设备与性能仍需独立验收。不以当前宿主结果代替原生 CI。

## 2026-10-24 E7L 语言迁移环境复核

- 目的：继续 egui 语言与翻译工具迁移前确认环境。
- 改动范围：Python 翻译工具、egui 菜单/设置、环境快照。
- 执行内容：核对当前 Cargo/metadata/PO 工具与上轮结果；rustc 1.97.1、Python 3.10.20 保持一致，Cargo 锁定依赖已有缓存，Python 使用标准库及既有 rustfmt。
- 验证结果：允许开工，不增加依赖，不改变 Cargo/MSRV/CI；本轮最终验证另记。
- 风险/待办：原生菜单/GUI 与其它平台验收独立保留；运行时不解析或读取 PO 文件。

## 2026-10-24 E7L 完整门禁完成

- 目的：验证语言/菜单/设置迁移和静态翻译工具。
- 改动范围：egui 界面、Python 翻译工具、共享 PO 与生成表；未增加依赖或升级工具链。
- 执行内容：两前端 locked/offline check/严格 Clippy/test/build、组合 check，及 egui aarch64-apple-darwin 显式 check/Clippy/build；翻译生成/PO/脚本/文档校验。
- 验证结果：默认 930、egui 542 项 Rust tests，31 Python 和 711×11 PO 检查通过；全部构建门禁通过，egui 图仍无 Slint/softbuffer。
- 风险/待办：Windows/Linux/Intel macOS 和 MSRV 未重跑；原生菜单/字体/文案长度及真实平台验收保留，不以无窗口回归替代 GUI 确认。

## 2026-10-24 E7F 字体迁移环境复核

- 目的：确认字体迁移使用现有依赖与资源。
- 改动范围：共用字体资源目录、egui 字体后台任务和设置链。
- 执行内容：核对现有 fontdb 0.24.0、egui/epaint 0.37.0、内置四个字体族及许可证；rustc/Cargo 1.97.1、Python 3.10.20 保持一致。
- 验证结果：允许开工，已有 Cargo 缓存可用；不新增依赖/字体，不调整 MSRV/CI。
- 风险/待办：后台任务、资源上限和真实字体布局回归待本轮验证；原生显示/字体质量由用户验收。

## 2026-10-24 E7F 字体门禁完成

- 目的：确认共用资源目录和 egui 字体选择不改变工具链/依赖/构建隔离。
- 改动范围：共用字体元数据、egui 后台加载、草稿与终端网格；Slint 注册器沿用。
- 执行内容：两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合 check、依赖图及脚本/静态生成/文档检查。
- 验证结果：默认 930、egui 553 项 Rust 测试与 31 Python 通过；711×11 PO/生成表通过，egui 图无 Slint/softbuffer。四家族实际字体、取消/join、过期结果、失败草稿和网格联动有确定性回归；不新增依赖、schema 或字体。
- 风险/待办：本轮未重跑显式 target/MSRV 或其它平台；原生显示/IME/性能和既有按需系统缺字 provider 的首次等待与生命周期仍需 E2V 验证。

## 2026-10-24 E7W 翻译迁移环境复核

- 目的：确认连接/工作区文案使用现有静态目录和生成工具。
- 改动范围：egui 前端固定文案、现有 PO 与生成表；不触及 transport 或持久化 schema。
- 执行内容：读取环境记录、manifest、前端/翻译入口；核对 rustc/Cargo 1.97.1、Python 3.10.20 和 Rust 1.95.0 声明。
- 验证结果：允许开工，依赖/工具链无变更，沿用现有 locked/offline 验证及标准库 Python 工具。
- 风险/待办：文案长度/键盘焦点和原生信任提示仍需用户验收；本轮不验证其它平台或真实远端。

## 2026-10-24 E7W 文案门禁完成

- 目的：验证共享 PO 扩展和 egui 连接/工作区文案保持构建及行为边界。
- 改动范围：前端固定文案、稳定控件 ID、现有 PO/静态表和三项回归；控制层、依赖和 schema 不变。
- 执行内容：两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合 check 和脚本/生成/文档校验。
- 验证结果：默认 930、egui 556 项 Rust 测试通过，17 项相关 Python、741×11 PO/静态生成、Markdown/skill/tracker/fmt/diff 通过。保留原 711 条及元数据，最终 egui 使用 145 条静态源键；默认 Slint 也重新经 Cargo 编译验证。
- 风险/待办：本轮未重跑显式 target/MSRV 或其它平台；原生文案长度/焦点和实际 SSH/设备/性能仍需独立验收，不以无窗口文字/指针回归代替用户视觉确认。

## 2026-10-24 E7S 环境复核

- 目的：确认 SFTP 文案及前端共享提示使用现有依赖和静态翻译链。
- 改动范围：egui 前端及 PO/生成表，不改变 worker、schema 或构建配置。
- 执行内容：核对环境快照、Rust/Slint/egui 规范、SFTP facade 和 UI；rustc/Cargo 1.97.1、Python 3.10.20 保持一致。
- 验证结果：允许开工；沿用 locked/offline 两前端及组合门禁，不增加依赖。
- 风险/待办：共享提示须验证指纹和精确 prompt token，UI 操作须有跨 pass 去重；原生字体/焦点仍需用户验收。

## 2026-10-24 E7S SFTP 门禁完成

- 目的：验证共享 SSH 提示、SFTP UI 意图边界和静态文案保持两前端构建/行为契约。
- 改动范围：前端私有模块提取、提示/秘密清理、按钮每帧准入/错误反馈及 PO/静态表；worker/transport、依赖和 schema 不变。
- 执行内容：两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合 check，及生成/脚本/文档/skill/tracker/fmt/diff 校验。
- 验证结果：默认 930、egui 558 项 Rust 测试通过；17 项相关 Python 和 756×11 PO/静态生成通过，egui 使用 167 条源键。原 741 条及元数据保留，默认 Slint 经 Cargo 重新编译验证。
- 风险/待办：未重跑显式 target/MSRV 或其它平台；原生 SFTP 文案长度/焦点、真实传输/SSH/设备和性能仍需独立验收，无窗口指针/文字回归不替代用户视觉确认。

## 2026-10-24 E7M 环境复核

- 目的：确认终端选择/复制设置沿用当前字段、输入与模型接口。
- 改动范围：egui 交互和配置草稿、controller 复制结果身份；不改持久化 schema 或依赖。
- 执行内容：核对 Cargo edition 2024/MSRV 1.95.0、当前 Rust/Cargo 1.97.1、Python 3.10.20，以及既有 Slint 输入/终端配置与 egui/controller。
- 验证结果：允许开工，缓存可用；继续两前端 locked/offline 验证，不新增库或升级版本。
- 风险/待办：须覆盖真实输入字节、reporting 手势/取消、过期复制与失败保存；原生鼠标/剪贴板和其它平台需独立验收。

## 2026-10-24 E7M 鼠标与复制门禁完成

- 目的：验证保存后的交互配置、鼠标手势所有权及复制/粘贴身份不破坏两前端边界。
- 改动范围：egui 输入/设置/短期粘贴元数据票据、controller 复制结果身份和回归；持久化 schema、依赖、工具链和构建入口不变。
- 执行内容：两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合 check，及脚本/生成/文档/skill/tracker/fmt/diff 校验。
- 验证结果：默认 930、egui 562 项 Rust 测试通过；新增真实 PTY/UI 手势/复制/定向粘贴和 controller 连接/视口复制身份回归通过，保存失败仍使用保存值。17 项相关 Python、756×11 PO/静态生成通过，复用已有条目后 egui 使用 173 个源键；无新原生依赖。
- 风险/待办：未重跑显式 target/MSRV 或其它平台；实际系统鼠标/剪贴板与原生 GUI 仍待独立验收。票据依据固定 eframe 0.37 WGPU 的同步读取/下一输入帧路径；Option/Meta 和远端 OSC 52 权限继续后续切片，OSC 52 仍禁用。

## 2026-10-24 E7P 环境复核

- 目的：核对原生 Option 策略和 OSC 52 的可实现边界。
- 改动范围：egui/controller，macOS egui 直接声明已有 winit 0.30.13，不改 schema 或升级版本。
- 执行内容：读取环境、Cargo/CI、缓存中的 eframe/egui-winit/winit 源码及现有 Slint/model 权限路径；运行 rustc/Cargo/Python/已安装 target 查询。
- 验证结果：允许开工；Rust/Cargo 1.97.1、Python 3.10.20，ARM64/Intel macOS targets 可用；延续两前端 locked/offline 与严格 Clippy，依赖声明后更新 root lock 条目。
- 风险/待办：原生 Option/IME 与真实系统剪贴板需要手动验收；其它平台仍依赖原生 CI。读取仅显式同意后触发，拒绝/超时/重连/关闭权限必须不返回正文。

## 2026-10-25 E7P 原生输入与权限门禁完成

- 目的：确认原生 Option 策略和有界授权读取保持两前端与平台边界。
- 改动范围：macOS egui 可选直接声明现有 winit 0.30.13；复用 arboard/模型/配置，新增可取消并 join 的单个读取 job，不改版本或 schema。
- 执行内容：两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合检查；ARM64/Intel macOS 两前端显式 target check/Clippy；验证依赖图、脚本/目录/生成/Markdown/skill/tracker/fmt/diff。
- 验证结果：默认 930、egui 571 项测试通过；原始空文本/CRLF 协议、单次/拒绝/超时/快速撤权/旧视口、真实 Allow 多 pass 和关闭 join 回归通过。17 项 Python、756×11 PO/生成通过，egui 使用 181 键，独立依赖图无 Slint/softbuffer/Skia。当前 rustup 无 1.95 工具链，未重跑 MSRV。
- 风险/待办：Intel 仅交叉 check/Clippy，未运行 Intel 程序；其它平台完整链接/原生 GUI/权限与真实 Option/dead-key/IME、系统剪贴板仍需独立验收，未自主截图。

## 2026-10-25 E7U 环境复核

- 目的：核对全局上传设置能否沿用现有 schema 和 limiter。
- 改动范围：egui SFTP 控件、workspace 加载/提交配置点与回归，不改 native/transport/持久化 schema。
- 执行内容：读取 Cargo/环境/架构/地图、SFTP 字段、UI 与全局 limiter；轻量确认 rustc/Cargo 1.97.1、Python 3.10.20。
- 验证结果：允许开工；沿用 locked/offline 两前端门禁，无新增依赖或版本升级，既有原生/MSRV 缺口保持。
- 风险/待办：配置必须在原子写成功后应用，测试不得并行改写进程全局状态；真实多连接调度和 GUI 验收独立保留。

## 2026-10-25 E7U 全局上传设置门禁完成

- 目的：验证已保存全局上传上限的应用链与现有 permit 语义。
- 改动范围：SFTP 草稿控件、workspace 加载/提交策略及测试注入，复用既有 limiter；无 native/依赖/构建/CI 变化。
- 执行内容：两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合检查，及聚焦/脚本/译文/文档/skill/tracker/fmt/diff 校验。
- 验证结果：默认 930、egui 573 项完整测试通过；14 项 workspace、permit 和两项设置回归通过。17 项 Python、756×11 PO/静态生成、183 个 egui 源键通过；配置回归注入私有回调，不降低并行测试使用的进程实际上限。
- 风险/待办：本轮未重跑显式 target/MSRV 或其它平台；实际多连接上传调度及 GUI 仍需独立验收，不以无窗口或注入回归替代用户视觉/真实传输确认。

## 2026-10-25 E7O 环境复核

- 目的：确认可共享现有本地文件校验与缓存，并保持实验数据隔离。
- 改动范围：Rust 库/默认适配、实验 companion 与 UI；沿用已有文件身份、缓存限额和 open 依赖。
- 执行内容：读取环境/Cargo/架构/地图、默认校验/快照/原生打开和实验后台编辑生命周期；确认 Rust/Cargo 1.97.1、Python 3.10.20。
- 验证结果：允许开工；无 schema/依赖/工具链升级，继续 locked/offline 两前端门禁，原生/MSRV 缺口保持。
- 风险/待办：链接/替换/目录过期及未完成复制不能启动关联应用；后台任务须保留取消/join，实验缓存不得写入默认缓存；测试使用注入 opener，原生验收独立保留。

## 2026-10-25 E7O 安全打开门禁完成

- 目的：确认共享目录/文件校验、实验独立快照和保存限额在两个前端及可用 macOS target 上保持可编译/可测试。
- 改动范围：共享 Rust 本地文件模块、默认薄适配、实验 companion/控件与回归；复用既有缓存和 open 依赖，无工具链/schema/依赖/CI 变化。
- 执行内容：完成两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合 all-features/all-targets check，及 ARM64/Intel macOS 两前端显式 target check/严格 all-targets Clippy；完成聚焦/脚本/译文/文档/skill/tracker/fmt/diff 校验。
- 验证结果：默认 932、egui 587 项完整测试通过；14 项 local_open、9 项共享校验、全语言 Open 实际点击/重复 pass 和两项设置回归通过。17 项 Python、756×11 PO/生成与 185 个 egui 源键通过，单个后台快照任务取消/清理和关闭 join 经注入 copier/opener 确认，不自主启动外部应用。
- 风险/待办：Intel 只有交叉 check/Clippy；Windows/Linux 原生链接/运行、Intel 链接/运行和 Rust 1.95 尚未验证。非法 UTF-8 文件名测试限 Linux，宿主未运行；真实关联应用、设置 GUI 和其它原生输入/设备验收保留。

## 2026-10-25 E7X 环境预检

- 目的：继续迁移 SSH X11 连接模式与本机 X server 设置。
- 改动范围：实验连接/设置/controller，复用既有 X11Settings、XServerPlan 和 SSH worker；无 schema/依赖/工具链/CI 变更。
- 执行内容：核对 Cargo/锁文件、CI、工具链、地图和既有认证/relay 生命周期；本机 Rust/Cargo 1.97.1、Python 3.10.20，已安装 ARM64/Intel macOS target。
- 验证结果：允许开工；已知安装位置发现仅文件系统/平台只读查询，可在窗口创建前 join；协议回归使用固定信任 loopback，不执行真实 xauth/启动器，不读取真实录音设备。
- 风险/待办：MSRV 1.95 无本机工具链，Windows/Linux 原生门禁和真实 XQuartz/GUI 仍需 CI/手动验收；麦克风/附件留后续，不根据迁移顺带开启录音。

## 2026-10-25 E7X 全部门禁完成

- 目的：验证 X11 连接/设置和共享服务端确认在两前端及可用 macOS target 上的编译、行为与清理边界。
- 改动范围：实验 controller/控件和非秘密设置、共享 SSH setup 与取消；不更改 schema、依赖、工具链或 CI。
- 执行内容：完成两前端 locked/offline check/严格 all-targets Clippy/test、egui build、组合检查、ARM64/Intel macOS 两前端显式 target check/严格 Clippy，以及聚焦/脚本/译文/文档/skill/tracker/fmt/diff 校验。
- 验证结果：默认 934、egui 593 项完整测试通过；24 项 X11、两项设置事务及 28 项 terminal_session 聚焦回归通过，确认请求入队不等于服务端接受、拒绝后 shell 可用、Off 不请求、认证前无请求、等待时断开可 join。17 项 Python、756×11 PO/生成和 207 个 egui 键通过，复用 22 个已有键。发现位置只在启动前后台读取并 join，UI 无文件系统/协议工作。
- 风险/待办：Intel 仅交叉 check/Clippy，无链接/运行；Windows/Linux 原生门禁和 Rust 1.95 未在本机验证。真实 XQuartz/受限授权/X11 应用与 GUI 待独立验收，实验麦克风保持关闭，不以 loopback/无窗口测试替代设备或视觉确认。

## 2026-10-25 G1 / E7A 环境预检

- 目的：将施工前标准检索落入根规则并继续音频格式/权限入口迁移。
- 改动范围：项目规范、实验 UI/controller 与现有 AudioSettings/Pulse worker 接线；不改依赖、schema、工具链或原生采集/权限实现。
- 执行内容：核对 Rust/Cargo 1.97.1、Python 3.10.20、两 macOS target、Cargo/CI 与既有格式/采集租约/需求触发/PCM/关闭边界；已按用户要求检索官方 CPAL 0.17.3/PulseAudio，研究及版本决策落盘。
- 验证结果：允许开工，G1 的 Markdown/skill/tracker/diff 和 Cargo check 通过；实际录音设备测试使用既有注入边界，新增连接测试用固定信任 loopback，测试不触发真实 xauth/provider/麦克风。
- 风险/待办：Rust 1.95 工具链及 Windows/Linux SDK 不在本机；真实 TCC/远端 Pulse/设备/GUI 待原生验收，Apple JS 正文抓取失败不作为已读证据，继续沿用已有权限实现。

## 2026-10-25 E7A 续作验证完成

- 目的：完成音频设置/连接级授权及既有 worker 生命周期接线的环境与验证记录。
- 改动范围：实验 controller/UI、回归和双语文档；依赖/schema/工具链/CI 无本轮新增变化，原生音频实现继续复用。
- 执行内容：先核对 CPAL 0.17.3/PulseAudio/Tokio 1.53.3/egui 0.37 官方正文并落 research，再补全格式点击、授权撤销、保存失败恢复与等待音源准备时断开 join 回归，运行全部门禁。
- 验证结果：默认 934、egui 597 项完整测试、两前端 locked/offline check/严格 all-targets Clippy/test/build、组合检查、双 macOS target 两前端按 CI 原命令 check/严格 Clippy 均通过。17 项 Python、56 Markdown/入口、PO/生成、215 egui 源键、116 图标、fmt/skill/tracker/diff 通过；测试无真实设备访问。
- 风险/待办：Intel 仅交叉 check/Clippy；MSRV 1.95、Windows/Linux 原生链接运行和真实 TCC/设备/远端 Pulse/GUI 保留独立验收。下一附件切片仍先检索官方标准及版本依据，无多 agent/Worker、自主截图、提交/推送/发布。

## 2026-10-25 E7I 图片附件预检与验证

- 目的：接续图片粘贴入口，复用锁定媒体与 SSH 实现，验证原目标插入/取消和文件清理。
- 改动范围：egui 图片事件/后台转换、controller 附件任务/票据/文件所有权、回归及双语说明；无新依赖、schema、工具链、CI 或原生权限代码。
- 执行内容：先研究官方 egui/eframe/ecolor 0.37、Tokio 1.53.3、image 0.25.10 及 SFTP v3 草案正文，再实现单任务、取消保留占位、最终原子确认和退出 join；复用 PrivateMediaFile 与同连接 SFTP。
- 验证结果：默认 934、egui 606 项完整测试，两前端 check/严格 all-targets Clippy/test/build、all-features/all-targets、双 macOS target 两前端 CI 原命令 check/严格 Clippy 通过；7 项附件控制/真实 PTY/固定信任 SSH 回归和 2 项 egui 像素/多 pass/失焦回归通过。17 项 Python、56 Markdown/入口、PO/生成、215 egui 源键、116 图标、skill/tracker/fmt/diff 通过。
- 风险/待办：上游在事件送达前读取/解码原生剪贴板，应用只能事后限额；egui 文本优先，双语说明已记录。Windows/Linux 原生运行、Intel 链接运行、MSRV 1.95、真实系统剪贴板/GUI 仍待 CI/用户验收，断网远端清理尽力而为。无多 agent/Worker、应用截图、提交/推送/发布。

## 2026-10-25 E7H 侧栏遮罩预检与验证

- 目的：继续 E7R，对齐会话列表遮罩与 Workspace 单字符设置。
- 改动范围：共享纯 endpoint 格式化、默认薄适配、egui 控件/显示及回归/双语说明；无依赖/schema/工具链/CI/平台 cfg 变化。
- 执行内容：先读环境及 Cargo/CI，确认 Rust/Cargo 1.97.1、Python 3.10.20 与双 macOS target，官方 egui 0.37/Rust 1.95 tag 正文先于计划及代码；config 原子保存与规范化继续复用。
- 验证结果：默认 935、egui 610 项完整测试，两前端 locked/offline check/严格 all-targets Clippy/test/build 和组合 check 通过；聚焦协议/Unicode/原值保持、全语言 UI/保存失败恢复、17 项 Python、756×11 PO/生成、217 egui 源键、56 Markdown/入口、116 图标、skill/tracker/fmt/diff 通过。
- 风险/待办：遮罩仅列表表示，完整提示/辅助技术与原始搜索/连接保留；GUI/辅助技术交用户验收。本轮未重跑显式 target，MSRV 1.95/其它原生平台仍交 CI；无多 agent/Worker、自主截图、提交/推送/发布。

## 2026-10-25 E7B 文字颜色预检与验证

- 目的：继续 E7R，对齐亮度/粗体亮色控件与实际绘制。
- 改动范围：共享既有 HSL、默认薄适配、egui 控件/颜色顺序/缓存和双语说明，无依赖/schema/工具链/CI/平台 cfg 变化。
- 执行内容：环境确认 Rust/Cargo 1.97.1、Python 3.10.20，先读取 xterm 与锁定 egui 0.37 官方正文再记录计划/实施；既有原子保存与预览所有权保持。
- 验证结果：默认 472 library + 464 binary = 936、egui 547 library + 67 binary = 614 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 和 fmt/diff 通过。四项新增回归覆盖纯 HSL 边界、ANSI/alpha、真实模型静止行缓存/装饰、全语言键盘逐档控件；两项保存事务扩展通过。17 项 Python、56 Markdown/入口、756×11 PO/生成（含 header 为 757）、222 egui 源键、116 图标、skill/tracker 通过。
- 风险/待办：GUI/辅助技术交用户验收；本轮未重跑显式 target，MSRV 1.95 与其它原生平台交 CI。E7R 的语义/自定义主题/其它显示设置继续开放；无多 agent/Worker、自主截图、提交/推送/发布。


## 2026-10-25 20:57 E7R-FPS 施工前预检

- 目的：补 Theme 双前端全量门禁，继续前台/后台输出快照刷新率迁移。
- 改动范围：controller 调度/焦点提示、workspace 保存策略、egui 控件和双语文档；无依赖/schema/工具链/CI/平台 cfg 变更计划。
- 执行内容：核对 Cargo/锁文件/CI、既有环境及本机 Rust/Cargo 1.97.1、Python 3.10.20 与双 macOS target；先查 egui 0.37/Tokio 1.53.3 官方版本源码，后写计划。
- 验证结果：可开工；完整基线和实现后门禁待执行。
- 风险/待办：本机无 Rust 1.95，Windows/Linux SDK 及其它平台运行不具备；原生 GUI/焦点/性能由用户验收，不自主截图，无多 agent/Worker。


## 2026-10-25 21:27 E7R-FPS 最终验证

- 目的：完成前台/后台刷新率迁移并补 Theme 完整自动基线。
- 改动范围：controller 输出 deadline/焦点 watch、workspace 保存应用链、egui 控件/焦点生命周期及双语文档；无依赖/schema/工具链/CI/平台 cfg 变化。
- 执行内容：先环境/官方版本源码核对和计划，再实现并聚焦验证；最终焦点复核补读 Response 契约后修正逐准入帧/隐藏优先提示，协议路径保持。
- 验证结果：默认 480 library + 458 binary = 938、egui 562 library + 71 binary = 633 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 及 fmt/diff 通过；17 项 Python、56 Markdown/入口、756×11 PO/生成（含 header 为 757）、271 egui 源键、116 图标、skill/tracker 校验通过。新增七项回归，覆盖定时/同步输出/损伤保持/实时解析、重复提示和过载/旧代次、固定信任 SSH 协议响应、全语言键盘控件；保存事务扩展通过。
- 风险/待办：无显式 target/MSRV 复验；真实 GUI/焦点/帧率与其它原生平台/Intel 链接、Rust 1.95 仍需 CI/用户验收。无多 agent/Worker、自主截图或提交/推送/发布。


## 2026-10-25 21:52 +0800 E7R-Icons 施工前预检

- 项目边界：独立 Rust 桌面应用，默认 Slint/实验 egui 分进程；现有环境记录已读并刷新本轮范围。
- 运行/测试环境：Cargo.lock、Cargo.toml、build.rs 与五目标 CI 为依据；本机 Rust/Cargo 1.97.1、Python 3.10.20，双 macOS target 已确认；1.95 未安装，无其它平台原生 SDK/运行证据。
- 变更计划：共享系统身份与 egui 图标呈现，增加已锁定 resvg 0.48.1 的可选 build 依赖，不改 schema/工具链/CI；真实 GUI/高 DPI 仍需用户验收。
- 开工判定：允许开工。先记录官方核对/计划，再改代码，最终跑两前端完整门禁、组合检查和文档/生成/skill/tracker 验证。


## 2026-10-25 22:28 +0800 E7R-Icons 最终验证

- 目的：完成本地图标覆盖/主题轮廓及真实图标呈现迁移。
- 改动范围：共享系统身份、Linux 严格来源优先、egui 可选 build 栅格化、UI 有界纹理和保存事务；现有锁定 resvg 增加 build 角色，无新包版本/schema/工具链/CI 或 SSH 权限变化。
- 执行内容：环境及官方依据先于计划/代码，聚焦/组合回归后执行两前端完整门禁与双 macOS target CI check/strict Clippy；更新双语/许可、地图、月度记录和当前态。
- 验证结果：默认 484 library + 455 binary = 939、egui 566 library + 79 binary = 645 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端 CI 原 target check/严格 all-targets Clippy 全部通过；Intel 仅交叉编译，未链接/运行；17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、275 egui 键、116 图标/5 生成文件、skill/tracker/fmt/diff 通过。egui normal/build graph 确认无 Slint/softbuffer；116 图标可见/预乘/目录一致、资源拒绝、透明度/复用/释放、身份/聚焦 pane、全语言控件及实际 Sidebar/Tab 预览/还原通过。新增 9 项测试，原 3 项身份用例随模块迁入库。
- 风险/待办：Linux/Windows 原生 SDK/运行和 MSRV 1.95 未具备；Intel 只检查未链接/运行。原生 GUI/高 DPI/a11y 交用户，无多 agent/Worker、自主截图、提交/推送/发布。


## 2026-10-25 23:28 +0800 E7R-Shells 施工前预检

- 项目边界：独立 Rust 桌面应用；现有环境/current 与 implementation 地图已读取并刷新本轮范围。
- 运行/测试：Rust/Cargo 1.97.1、Python 3.10.20、双 macOS target 确认；MSRV 1.95/其它平台 SDK 与运行环境未具备。Cargo/lock/五目标 CI 仍为依据。
- 范围：仅 egui shell 选择/发现/事务，无依赖/schema/平台 cfg/工具链/CI 变更；组名长度为兼容字段，后续需紧凑侧栏契约。
- 开工判定：允许开工；官方版本源码/计划先于实现，最终执行两前端完整门禁、组合及文档校验。GUI/原生 PATH/辅助技术验收保留，无多 agent/Worker。


## 2026-10-25 23:40 +0800 E7R-Shells 最终验证

- 目的：完成已知 shell 选择/发现/缓存与保存链路，纠正兼容组名字段旧审计。
- 范围：egui 启动发现/候选控件和事务提交副本、四项回归及双语文档；无依赖/schema/平台 cfg/工具链/CI 或 SSH 安全变化。
- 执行内容：先环境/官方 API/计划再实施，发现任务在 GUI 前 await/join，失败不阻止手工选择；UI 读取有界快照，Save 复用 domain/workspace。
- 验证结果：默认 484 library + 455 binary = 939、egui 566 library + 83 binary = 649 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 及 fmt/diff 通过；17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、275 egui 源键、116 图标/5 生成文件、skill/tracker 校验通过。四项新增、14 项设置聚焦通过，覆盖无脏发现、失效/满列表当前值、Context/全语言/Unicode、失败重试和实际新终端消费。
- 风险/待办：本轮无新平台 cfg，未重复显式 target/MSRV；其它原生平台、Intel 链接/运行及 GUI/键盘/PATH 仍需 CI/用户验收。无多 agent/Worker、自主截图或提交发布。


## 2026-10-26 05:34 +0800 E7R-SFTP-Key 开工预检

- 范围：仅实验 UI 菜单/焦点/快捷键，沿用已锁定 egui 0.37/Muda 0.19.3，无新依赖/schema。
- 证据与判定：Cargo/lock、五目标 CI 与源码一致；rustc/Cargo 1.97.1、Python 3.10.20、aarch64/x86_64 macOS target 可用，1.95 未安装。允许开工，先官方检索/计划后代码。
- 验证计划：聚焦焦点/路由/事务测试、两前端完整门禁/组合、双 macOS target check/严格 Clippy，文档/翻译/skill/tracker。原生 GUI/其它平台/MSRV 保留，无多 agent/Worker。


## 2026-10-26 05:42 +0800 E7R-SFTP-Key 最终验证

- 目的：接通 SFTP 菜单/保存快捷键与精确 pane 路径焦点，补全部八键校验。
- 范围：仅 egui 菜单/设置/焦点与回归，无依赖/schema/transport/工具链/CI 变化。
- 执行内容：先环境/官方 API/计划再代码，复用已有 companion，无自动 Connect/认证/List；保存事务与平台别名语义均验证。
- 验证结果：默认 484 library + 455 binary = 939、egui 566 library + 88 binary = 654 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令的 target check/严格 Clippy 全通过。最终平台别名补充后重跑 egui 全量/构建/组合及双 target 门禁通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、275 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 通过。
- 风险/待办：Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/菜单/键盘/焦点验收保留，未自主截图、提交/推送/发布或使用多 agent/Worker。下一项会话级导入/导出。


## 2026-10-26 05:46 +0800 E7R-Transfer 开工预检

- 范围：默认纯规则共享、实验事务与 UI；单 package Rust 2024、MSRV 1.95、现有锁定依赖，无新增依赖/feature/schema/CI。
- 证据：Cargo/lock、源码/五目标 CI 与既有环境记录一致；本机 Rust/Cargo 1.97.1、Python 3.10.20、双 macOS target 可用，1.95 未安装。
- 开工判定：允许，官方检索/计划先于代码；聚焦后运行两前端完整/组合及双 macOS target 门禁。GUI 与其它原生平台/MSRV 仍待补证。


## 2026-10-26 06:16 +0800 E7R-Transfer 最终验证

- 目的：共享既有会话/分组 JSON 格式和脱敏规则，接通实验交换与全部八键。
- 范围：共享纯规则、默认薄适配、controller 后台串行事务和 egui 模态交换；无新依赖/持久 schema/协议格式/transport/工具链/CI。
- 执行内容：先预检/官方版本研究/计划再实现，保留默认行为及纯测试，补输入限额/失败原子性/任务排空和结果归属保护，完成双前端与双 macOS target 检查。
- 验证结果：默认 490 library + 450 binary = 940、egui 575 library + 94 binary = 669 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令的 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、279 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。
- 风险/待办：Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI。本机未安装 1.95；原生 GUI/菜单/焦点/剪贴板验收保留。未自主截图、提交/推送/发布或使用多 agent/Worker。定向分组导入/侧栏快捷复制、SFTP 剩余交互和原生文件选择器继续开放。


## 2026-10-26 06:23 +0800 E6R-Pause 开工预检

- 范围：实验 owner/快照/UI，复用 Tokio 1.53.3/egui 0.37.0 与已有 SFTP worker，无新依赖/feature/schema/工具链/CI。
- 环境：Rust/Cargo 1.97.1、Python 3.10.20、ARM64/Intel macOS target 可用，MSRV 1.95 未安装；Cargo/lock/CI 为依据。
- 开工判定：允许，已完成官方版本研究/计划；聚焦后跑双前端完整/组合，双 macOS target 和文档门禁。原生 GUI/其它平台/Intel 运行及 MSRV 保留，无多 agent/Worker。


## 2026-10-26 06:53 +0800 E6R-Pause 最终验证

- 目的：接通既有单项下载暂停/继续与类型化快照/资格、有界历史和 UI。
- 范围：实验 controller/UI 与回归；没有依赖/schema/协议/工具链/CI 变化。
- 执行内容：先预检/官方版本研究/计划再实现，固定信任 loopback 的 READ 闸门/FIFO barrier 验证实际字节、旧代次、暂停取消/关闭清理；原 native worker 行为保持。
- 验证结果：默认 490 library + 450 binary = 940、egui 580 library + 96 binary = 676 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、281 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。
- 风险/待办：Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行和 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/滚动/真实服务器暂停延迟及取消验收保留。未自主截图、提交/推送/发布或使用多 agent/Worker。下一项优先批量选择/控制，远端修改与原生拖出仍开放。


## 2026-10-26 07:25 +0800 E6R-Batch 开工预检

- 范围：controller 1–64 UUID 批次/有界最新结果，egui pane 局部选择；现有单项控制与任务 owner 复用，无新依赖/schema/协议/CI。
- 环境：Rust/Cargo 1.97.1、Python 3.10.20、双 macOS target 确认，MSRV 1.95 未安装；Cargo/lock 与既有五目标 CI 为依据。
- 开工判定：允许，已先官方版本检索和计划；聚焦后执行双前端完整/组合、双 macOS target 与文档校验。GUI/其它平台/Intel 运行/MSRV 保留，无多 agent/Worker。


## 2026-10-26 19:16 +0800 E6R-Batch 最终验证

- 目的：接通当前 pane 的传输多选、批量 Pause/Resume/Cancel 与逐项入队汇总。
- 范围：实验 controller/UI/回归与双语翻译；没有依赖/schema/worker 协议/工具链/CI 变化。
- 执行内容：先预检/官方版本检索/plan，再实现并验证现有有界 worker；READ 闸门/FIFO barrier 使用 owner 现态保留提前回执，不依赖 sleep。
- 验证结果：默认 490 library + 450 binary = 940、egui 583 library + 98 binary = 681 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、760×11 PO/生成（含 header 为 761）、290 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。
- 风险/待办：Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/滚动/真实服务器批次行为验收保留。无新依赖、持久 schema、worker 协议、工具链或 CI 变化；未自主截图、提交/推送/发布或使用多 agent/Worker。下一项优先远端重命名/删除，远端文件多选与原生拖出仍开放。


## 2026-10-26 20:37 +0800 E6R-Mutate 开工预检

- 项目边界：独立 Rust 桌面应用的实验 egui/controller 单项远端修改，不更改默认前端或原生写协议。
- 环境记忆状态：范围已刷新，Cargo/lock 与本机命令核对；Rust/Cargo 1.97.1、Python 3.10.20、ARM64/Intel macOS target 可用，MSRV 1.95 未安装。
- 测试环境：现 fixed-trust loopback 与无窗口 egui 夹具，双前端 check/严格 Clippy/test/build、组合和双 macOS target；不读取真实凭据或连接真实服务器。
- 开工判定：允许；已先官方来源研究和落盘 plan，再实施。原生 GUI/其它平台/MSRV/Intel 运行保留，无多 agent/Worker。


## 2026-10-26 21:06 +0800 E6R-Mutate 最终验证

- 目的：接通单项远端 Rename/Delete、明确确认、单槽精确回执和导航安全刷新。
- 范围：实验 controller/UI、固定信任 peer/回归及双语翻译；生产 SSH/SFTP 协议、依赖/schema/工具链/CI 不变。
- 执行内容：先预检/官方检索/落盘 plan，再实现并验证现有写 worker 的类型/目标不存在/非递归删除边界；UI 无 I/O/等待/锁，关闭放弃结果，断开仍取消/join。
- 验证结果：默认 490 library + 450 binary = 940、egui 588 library + 101 binary = 689 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、763×11 PO/生成（含 header 为 764）、297 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。
- 风险/待办：Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/焦点/键盘及真实服务器 rename/delete 验收保留，未自主截图。无新生产 task/channel、依赖、持久 schema、SSH 信任/协议、工具链或 CI 变化；未提交/推送/发布或使用多 agent/Worker。下一项优先远端文件多选/批量传输。


## 2026-10-26 21:54 +0800 E6R-Select 开工预检

- 项目边界：独立 Rust 桌面应用的实验远端选择/批量下载，以及共享发现入口既有 512 槽预算补齐。
- 环境记忆状态：范围已刷新，Cargo/lock 与本机命令核对；Rust/Cargo 1.97.1、Python 3.10.20、ARM64/Intel macOS target 可用，MSRV 1.95 未安装。
- 测试环境：固定信任 loopback、无窗口 egui 与双前端/组合/target Cargo 门禁；不读取真实凭据或连接真实服务器。
- 开工判定：允许；先官方来源和 plan 后施工，GUI/其它平台/MSRV/Intel 运行仍保留，无多 agent/Worker。


## 2026-10-26 22:22 +0800 E6R-Select 最终验证

- 目的：接通远端 path 多选/全选/清空、按钮批量下载与入队摘要，补齐共享发现入口预算。
- 范围：实验 controller/UI、共享既有 512 槽准入和测试 peer；没有依赖/schema/协议/工具链/CI 变化。
- 执行内容：先预检/官方研究/落盘 plan，再实施并验证。保持现根行/发现 Cancel，首子项复用根 UUID、空目录 DiscoveryFailed；UI 无 I/O/等待，目标在 owner 复验后交既有 worker。
- 验证结果：默认 490 library + 450 binary = 940、egui 593 library + 103 binary = 696 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、764×11 PO/生成（含 header 为 765）、299 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。
- 风险/待办：Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/选择/真实服务器下载验收保留，未自主截图。无新依赖、持久 schema、wire 协议、队列、task 类型或工具链/CI 变化；SSH 信任保持默认拒绝。未提交/推送/发布或使用多 agent/Worker。下一项优先本地多选/批量上传。


## 2026-10-26 22:29 +0800 E6R-UploadSelect 开工预检

- 项目边界：实验本地多选/批量上传，复用现 controller 单个 blocking 准备与 SSH worker。
- 环境记忆状态：范围已刷新，Cargo/lock/现命令核对；Rust/Cargo 1.97.1、Python 3.10.20、双 macOS target 可用，MSRV 1.95 未安装。
- 测试环境：固定信任 loopback/私有临时目录与无窗口 egui；双前端完整/组合和 target 检查，UI 不自主截图。
- 开工判定：允许，先官方研究/plan 后施工；原生 GUI/其它平台/MSRV/Intel 运行保留，无多 agent/Worker。
