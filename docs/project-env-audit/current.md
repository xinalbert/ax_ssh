# 当前项目环境状态

## 项目类型

- 根目录：`<repo-root>`；独立 Rust 2024/Slint 桌面应用，Cargo package 为 `ax_ssh`。
- 当前范围：五项直接依赖和兼容传递依赖升级；保持现有 Rust/平台/Slint/vendor 基线与 SSH 信任契约。
- 环境记忆已刷新：旧记录的本机 Rust/Python 版本、安装 targets 和任务范围已过期。

## 运行环境

- MSRV 为 Rust 1.92.0，edition 2024；Cargo.lock 是可复现依赖来源。
- 锁定 Slint 1.18.1、Tokio 1.54.1、russh 0.65.1、russh-sftp 3.0.1、notify 8.2.2、rubato 5.1.0、rand 0.10.4、uuid 1.29.2；CPAL 0.17.3 保留 macOS 11 支持。
- `build.rs` 编译 `ui/app.slint`，生成类型只在 `src/app.rs` 声明。
- 本机 macOS ARM64，默认 rustc/Cargo 1.99.0、Python 3.14.6、cargo-audit 0.22.2；另有 Rust 1.92.0，本轮用显式 +1.92.0 命令检查 MSRV，不变更默认 toolchain。
- 默认 toolchain 安装 `aarch64-apple-darwin`、`x86_64-pc-windows-msvc`；本机没有 Windows/Linux 原生 runner，未安装 Intel macOS/Linux Rust target。

## 测试环境

- Rust 内置测试，SSH/SFTP 使用显式信任的隔离 loopback peer、注入凭据和临时目录；不连接用户服务器或真实 keyring。
- CI 对 Windows x86_64/ARM64、Linux x86_64/ARM64、macOS ARM64/Intel 执行 target check、严格 all-target Clippy 和 build；仅原生 target 运行测试。
- 升级后 23 项定向回归及完整 1030 项测试（471 library + 559 binary）、fmt/check/严格 all-target Clippy、macOS aarch64 CI 同款 check/Clippy/build 和 Rust 1.92 check 均通过，结果见[实施记录](../project-implementation-tracker/current.md)。随后按用户要求执行 `cargo clean`，清理 25.2 GiB 产物，已验证的程序需要重新构建；macOS 验证不能替代其它平台 CI 或真实设备/GUI 验收。

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
- 升级后安全审计仍报告 RSA 单项漏洞及 6 项停止维护警告；按已有唯一 RustSec 例外审计通过。RSA 0.10.0 已有修复，但 russh 0.65.1 精确锁定 rc.18；其他漏洞仍阻止 CI/Release。
- Rust 1.92 check 的 Skia 归档下载初次失败，源码回退也无法连接 googlesource；重试使用核对过版本 0.153.3、ARM64/功能 key 和 gzip 完整性的本机归档，仅临时设置上游支持的 `SKIA_BINARIES_URL=file://...`，未变更依赖/工具链/CI 配置。
- [历史环境快照](history-before-2026-10-20.md)仅用于追溯；环境变化追加到 [changes.md](changes.md)。

## 证据文件

- `Cargo.toml`、`Cargo.lock`、`build.rs`、`.github/workflows/ci.yml`、`AGENTS.md` 和本机 `rustup show` / `rustc -Vv` / `cargo -V` / `python3 --version`。

## 最后确认时间

- 2026-11-16 14:21 +0800：依赖升级和本机验证完成后已清理编译产物；核对清理前后 Git 差异完全相同，提交前重新验证文档与暂存范围。其它平台原生验证与真实设备/GUI 验收保留。
