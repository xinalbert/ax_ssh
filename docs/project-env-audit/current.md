# 当前项目环境状态

## 项目类型

- 根目录：`<repo-root>`；独立 Rust 2024/Slint 桌面应用，Cargo package 为 `ax_ssh`。
- 当前范围：应用层 SFTP 下载意图的 macOS 条件编译修复；不调整依赖、UI、transport 或 CI。
- 环境记忆已刷新：旧记录的本机 Rust/Python 版本、安装 targets 和任务范围已过期。

## 运行环境

- MSRV 为 Rust 1.92.0，edition 2024；Cargo.lock 是可复现依赖来源。
- 锁定 Slint 1.18.1、Tokio 1.54.1、russh 0.65.0、russh-sftp 3.0.1；vendor 补丁和参考项目隔离规则保持原样。
- `build.rs` 编译 `ui/app.slint`，生成类型只在 `src/app.rs` 声明。
- 本机 macOS ARM64，默认 rustc/Cargo 1.97.1、Python 3.10.20；另安装 Rust 1.92.0/1.99.0 toolchains，本轮不切换工具链。
- 默认 toolchain 安装 `aarch64-apple-darwin`、`x86_64-apple-darwin`；没有 Windows/Linux Rust targets 或原生 runner。

## 测试环境

- Rust 内置测试，SSH/SFTP 使用显式信任的隔离 loopback peer、注入凭据和临时目录；不连接用户服务器或真实 keyring。
- CI 对 Windows x86_64、Linux x86_64/ARM64、macOS ARM64/Intel 执行 target check、严格 all-target Clippy 和 build；仅原生 target 运行测试。
- 本轮 fmt、locked/offline check、严格 all-target Clippy、945 项测试、14 项重连回归及 ARM64 macOS 精确 CI check/Clippy 命令均通过，结果见[实施记录](../project-implementation-tracker/current.md)。macOS 验证不能替代 Windows/Linux CI；GUI 仍由用户验收。

## 关键命令

```bash
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
python3 scripts/check_repository.py
git diff --check
```

- CI 按 target 使用 `cargo check --locked --target <TARGET>` 和 `cargo clippy --all-targets --locked --target <TARGET> -- -D warnings`。
- 更多开发、翻译、图标和发布 helper 命令见[开发说明](../development.zh.md)。

## 外部依赖

- macOS 编译需 Apple SDK；Windows/Linux 验证需相应 SDK、C 工具链和系统库；本轮不安装或变更工具链。
- Linux 使用 Fontconfig、XKB、ALSA 开发包，具体安装命令由 CI 和开发说明维护。
- 现存 RSA RustSec 单项例外属于既有依赖边界，本轮不运行 audit 或变更依赖。
- [历史环境快照](history-before-2026-10-20.md)仅用于追溯；环境变化追加到 [changes.md](changes.md)。

## 证据文件

- `Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`AGENTS.md` 和本机 `rustup show` / `rustc -Vv` / `cargo -V` / `python3 --version`。

## 最后确认时间

- 2026-11-07 07:49 +0800：已完成本机 Rust 门禁；按 Cargo.lock 联网补齐缓存后离线门禁通过，未变更依赖或工具链。Windows/Linux/Intel macOS 留给原生 CI。
