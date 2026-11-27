# 当前项目环境状态

## 项目类型

- 根目录：`<repo-root>`；独立 Rust 2024/Slint 桌面应用，Cargo package 为 `ax_ssh`。
- 当前范围：终端协议 base。终端网格和解析器使用锁定版本的本地补丁。

## 运行环境

- MSRV 为 Rust 1.92.0，edition 2024；Cargo.lock 是可复现依赖来源。
- 锁定 Slint 1.18.1、Tokio 1.54.1、russh 0.65.4、RSA 0.10.0、russh-sftp 3.0.1、notify 8.2.2、rubato 5.1.0、rand 0.10.4 和 uuid 1.29.3。CPAL 0.17.3 支持 macOS 11。
- Alacritty 0.26.0 和 VTE 0.15.0 经 `[patch.crates-io]` 解析到 `vendor/alacritty_terminal/` 与 `vendor/vte/`，Unicode 宽度库锁定 0.2.3。
- `build.rs` 编译 `ui/app.slint`，生成类型只在 `src/app.rs` 声明。
- 本机 macOS ARM64，默认 rustc/Cargo 1.99.0、Python 3.14.6、cargo-audit 0.22.2；另有 Rust 1.92.0，可用显式 `+1.92.0` 检查声明的 MSRV。
- 默认 toolchain 安装 `aarch64-apple-darwin`、`x86_64-pc-windows-msvc`；本机没有 Windows/Linux 原生 runner，未安装 Intel macOS/Linux Rust target。
- Windows x86_64 可使用本机 `cargo xwin`、SDK/CRT 缓存、LLVM 及 NASM 交叉验证。首次 Windows 检查按锁文件补取了本地缺失的 wgpu 依赖。

## 测试环境

- Rust 内置测试，SSH/SFTP 使用显式信任的隔离 loopback peer、注入凭据和临时目录；不连接用户服务器或真实 keyring。
- CI 对 Windows x86_64/ARM64、Linux x86_64/ARM64、macOS ARM64/Intel 执行 target check、严格 all-target Clippy 和 build；仅原生 target 运行测试。
- 终端协议 base 的本机与交叉验证详见[实施记录](../project-implementation-tracker/current.md)。macOS ARM64 原生 check、严格 Clippy 和链接通过；Windows x86_64 经 cargo-xwin 完成交叉检查与链接。

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
- RustSec 公告库快照 `8b75c83c3c9142dd18e93d75b8d895a13d285861` 的无豁免审计未发现漏洞，报告 7 条停止维护警告。新增警告为直接依赖 chrono 0.4.45。
- Skia 二进制归档下载失败后，本机验证临时使用 `SKIA_BINARIES_URL` 指向已校验的 Skia 0.153.3 ARM64 归档。
- [历史环境快照](history-before-2026-10-20.md)仅用于追溯；环境变化追加到 [changes.md](changes.md)。

## 证据文件

- `Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`AGENTS.md` 和本机 `rustup show` / `rustc -Vv` / `cargo -V` / `python3 --version`。

## 最后确认时间

- 2026-11-27 17:25 +0800：确认 Alacritty 0.26.0 本地补丁，Rust 1.92 与 macOS ARM64、Windows x86_64 构建通过。其它平台交由原生 CI。
