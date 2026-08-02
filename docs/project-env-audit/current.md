# 项目环境当前态

## 项目类型

- 独立 Rust 2024 桌面应用，根 Cargo 包为 `ax_ssh`。
- 项目边界：`<repo-root>`；`third_package/axshell` 仅是参考资料，不进入构建图。

## 运行环境

- Rust edition：2024。
- MSRV：Rust 1.92.0。
- 本机工具链：`rustc 1.96.1`、`cargo 1.96.1`。
- UI/运行时：Slint 1.17.1、Tokio 1、russh 0.62.2、russh-sftp 2.3.0、libmudtelnet-rs 2.0.10、tokio-serial 5.5.0；Tokio 启用 `process` 供有界 `xauth` 子进程调用，`rand 0.10` 是运行时依赖，用于生成单次 X11 fake cookie；macOS 已锁定的 `objc2-app-kit 0.3.2` 启用 `NSWorkspace` 系统应用发现；Slint 已启用 `unstable-fontique-010` 运行时字体注册，并直接依赖锁定的 `fontdb 0.23.0` 扫描系统等宽字体。
- 依赖管理：Cargo，锁文件为 `Cargo.lock`。

## 测试环境

- 单元与集成测试：Cargo 原生测试，包括 config、terminal、应用状态、本地 PTY、loopback SSH/Telnet、SFTP packet/path/state 边界和 Serial descriptor 匹配。
- CI：GitHub Actions 在 Ubuntu、macOS、Windows 上运行 format、check 和 test。
- 本机 `cargo fmt` 与 `cargo clippy` 组件未安装；可直接调用可用的 `rustfmt`，Clippy 需在 CI 或安装组件后验证。

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
- Serial 实机测试依赖目标平台驱动、设备权限和硬件；自动测试不打开真实串口。
- `tokio-serial 5.5.0` 声明 MSRV 1.71；`libmudtelnet-rs 2.0.10` 声明 MSRV 1.66，均低于项目 MSRV 1.92.0。
- `libmudtelnet-rs 2.0.10` 的跨调用不完整 IAC/协商帧和转义 IAC 存在已确认边界；本项目 64 KiB 有界分帧适配由逐字节与 Telnet loopback 回归覆盖。
- `russh-sftp 2.3.0` 未声明 MSRV，且 raw client 内部使用 unbounded packet sender；项目以 MSRV/CI、单浏览 session、串行请求、256 KiB 入站 frame、250 条分页和 2,000 条/2 MiB 目录预算约束其使用。
- 真实 SFTP 服务兼容性与 GUI 文件面板需要目标环境手工验证。
- X11 forwarding 依赖目标平台可用的本机 X server。AxSSH 可从 Settings 选择并有界启动 XQuartz、MacXServer、VcXsrv、Xming 或自定义程序；macOS 通过 bundle identifier、Windows 通过 `PATH` 后接 Program Files 自动发现已知 provider，路径输入只供 Custom 使用。安全默认仍要求 local-only `DISPLAY` 和可查询精确 `MIT-MAGIC-COOKIE-1` 的 `xauth`。MacXServer 和自动启动的 VcXsrv/Xming 只有在显式 no-auth 兼容下使用 loopback/`-ac`。真实 XQuartz/MacXServer、X.Org/Xwayland、VcXsrv/Xming 行为需目标平台手工验证，AxSSH 不安装软件或修改远端 `sshd_config`。
- 自带 TTF 作为 `assets/fonts/` 运行时资源保留在发行包，不经 Slint import 嵌入可执行文件。系统字体扫描依赖 `fontdb` 的预定义目录，必须在 Tokio blocking worker 中执行；各平台真实可见字体和打包后 Resources 路径须手工验收。
- 当前完整 locked/offline 测试门禁通过：库测试 105 项、应用测试 68 项和 Doc tests 均无失败。

## 证据文件

- `Cargo.toml`
- `Cargo.lock`
- `.github/workflows/ci.yml`
- `docs/development.md`
- `AGENTS.md`

## 最后确认时间

- 2026-08-02 20:22 CST
