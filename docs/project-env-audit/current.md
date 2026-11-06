# 当前项目环境状态

## 项目边界

- 独立 Rust 2024/Slint 桌面应用；工作区只包含 `ax_ssh` package。
- 当前范围：2026-11-03 Cargo.lock 依赖刷新与 RustSec 修复验证；不调整功能、SSH 协议或持久化配置。
- Cargo/Slint 编译、Linux/Windows/macOS CI 与项目自有源码是依据；`vendor/` 保留锁定补丁，参考子模块不进入构建。

## 仓库运行要求

- MSRV：Rust 1.92.0；edition 2024。Cargo 锁文件是可复现依赖来源。
- UI 入口：`build.rs` 只编译 `ui/app.slint`；生成类型只在 `src/app.rs` 声明。
- Slint 及本地补丁 1.18.1；Tokio 1.54.1、russh 0.65.0、russh-sftp 3.0.1、UUID 1.29.0、pulldown-cmark 0.13.6、notify 8.2.1。
- 本次通过 Cargo 解析更新 92 个兼容 Rust 1.92 的锁定依赖，将 `notify` 从 8.2.0 升到 8.2.1，并将 `russh` 从 0.64.1 升到 0.65.0；CPAL 仍固定 0.17.3 以保留 macOS 11 部署范围，Slint 与本地补丁未变。
- Linux 构建需要 pkg-config、Fontconfig/XKB 与 ALSA 开发包，具体命令见[开发说明](../development.zh.md)。
- Python 工程工具要求 3.10+、标准库和 rustfmt，不依赖本机个人技能路径或第三方 Python 包。

## 本机可用环境

- 宿主 macOS ARM64；本机默认 rustc 1.99.0、Cargo 1.99.0、Python 3.14.6。
- 已安装 Rust target：`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`；只有 ARM64 macOS 可在本机原生运行与链接。
- Windows target 不代表存在 Windows SDK/执行环境；本轮交叉 check 在 `aws-lc-sys` C 编译阶段因缺少 MSVC target C 工具链失败，Windows 原生验证交 CI runner；本机没有 Linux 或 Intel macOS runner。
- 更新后 Cargo 缓存支持 locked/offline 稳定版构建和 Rust 1.92 check；首次 MSRV 构建曾尝试下载 Skia 资源并遇到外网超时，之后复用本机稳定版生成的 Skia 产物完成了 MSRV 离线检查。

## 测试与 CI

- 默认验证：fmt、locked/offline check、严格 all-target Clippy、locked/offline test、`git diff --check`。
- CI 仍对 Windows x86_64、Linux x86_64/ARM64、macOS ARM64/Intel 分别 check、严格 Clippy 和 build；只在原生 runner 上执行 Rust tests。
- 终端/SSH/SFTP 测试使用隔离临时文件、固定信任的 loopback peer、注入设备或无窗口 Slint，不访问真实 keyring/麦克风/远端，不以自主截图验收 GUI。
- 本次依赖更新后的 fmt、locked/offline check、严格 all-target Clippy、Rust 1.92 ARM64 macOS check/build、920 项 Rust 测试、SSH 19 项 focused tests、RustSec（忽略既有 RSA 例外）、仓库入口、tracker 和 diff 校验已通过，详见[当前实施记录](../project-implementation-tracker/current.md)。Windows MSVC 交叉 check 无适配 C toolchain，未替代原生 CI。
- CI、实际 OS 鼠标/剪贴板/设备、真实 XQuartz/远端会话是不同验证面；本机自动结果不替代其它平台或真实 GUI 验收。

## 常用命令

```bash
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
python3 scripts/check_repository.py
python3 scripts/check_translations.py
python3 scripts/build_translations.py --check
python3 scripts/generate_system_icons.py --check
python3 -m unittest scripts/test_repository.py scripts/test_translations.py scripts/test_release_version.py scripts/test_generate_release_highlights.py -v
git diff --check
```

## 风险与历史证据

- `pulldown-cmark` 已从 0.13.5 更新到修复版 0.13.6，`RUSTSEC-2026-0433` 已消除。`russh 0.65.0` 仍精确固定受 `RUSTSEC-2023-0071` 影响的 `rsa 0.10.0-rc.18`，RustSec 未列出修复版本，继续使用现有单项 CI 例外；`cargo audit --ignore RUSTSEC-2023-0071` 通过，发现 5 项未维护依赖警告。
- 先前实际 Windows 滚轮问题、XQuartz 启动及真实平台功能的验收边界保留，不通过历史通过数字推断当前主机已复现。
- 整理前 current 中混排的环境/验证资料已逐字保存在[历史环境快照](history-before-2026-10-20.md)；仅供追溯，不代表本机当前已安装目标或本轮验证结论。
- 环境变更按时间追加到 [changes.md](changes.md)；不再把旧任务的“本轮”“当前”叠加到此快照。

## 最后确认

- 2026-11-03 13:33 +0800：`notify` 与 `russh` 依赖更新及本机可执行验证完成；工作区未提交或推送。Windows/Linux/Intel macOS 仍交原生 CI 验证。
