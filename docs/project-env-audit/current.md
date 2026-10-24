# 当前项目环境状态

## 项目边界

- 独立 Rust 2024 桌面应用，默认 Slint 和实验 egui 分别运行；工作区只包含 `ax_ssh` package。
- 当前范围：2026-10-22 E1/E2 egui 独立构建与本地终端，不调整 SSH/凭据/持久化 schema。
- Cargo/Slint 编译、Linux/Windows/macOS CI 与项目自有源码是依据；`vendor/` 保留锁定补丁，参考子模块不进入构建。

## 仓库运行要求

- MSRV：Rust 1.95.0；edition 2024。Cargo 锁文件是可复现依赖来源。
- UI 入口：默认 slint-ui 的 `build.rs` 只编译 `ui/app.slint`，生成类型只在 `src/app.rs`；egui-ui 为 `src/egui_main.rs`，独立图不含 Slint/softbuffer。
- egui/eframe 0.37.0 + WGPU，系统字体/clipboard/AccessKit；实验程序不读写已有 session/config/workspace，日志独立。
- Slint 及本地补丁 1.18.1；Tokio 1.53.3、russh 0.64.1、russh-sftp 3.0.1。
- arboard 3.7.0、rubato 5.0.1、base64 0.23.1、serde_json 1.0.154、uuid 1.28.0；CPAL 固定 0.17.3，保留 macOS 11 部署范围。
- Linux 构建需要 pkg-config、Fontconfig/XKB 与 ALSA 开发包，具体命令见[开发说明](../development.zh.md)。
- Python 工程工具要求 3.10+、标准库和 rustfmt，不依赖本机个人技能路径或第三方 Python 包。

## 本机可用环境

- 宿主 macOS ARM64；本机默认 rustc 1.99.0、Cargo 1.99.0、Python 3.14.6。
- 已安装 Rust target：`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`。
- 已增加 Rust 1.95.0 最低工具链；本机默认工具链仍 1.99。1.95 的 egui-only/default check 已通过。
- Windows target 不代表存在 Windows SDK/执行环境；本轮不运行 Windows/Linux 或 Intel macOS 程序，不安装 SDK。
- 联网获取并锁定 egui 依赖后使用 locked/offline 验证；真实 PTY 测试用 `/bin/sh`，无真实远端服务。

## 测试与 CI

- 默认验证：fmt、locked/offline check、严格 all-target Clippy、locked/offline test、`git diff --check`。
- CI 仍对 Windows x86_64、Linux x86_64/ARM64、macOS ARM64/Intel 分别 check、严格 Clippy 和 build；只在原生 runner 上执行 Rust tests。
- 终端/SSH/SFTP 测试使用隔离临时文件、固定信任的 loopback peer、注入设备或无窗口 Slint，不访问真实 keyring/麦克风/远端，不以自主截图验收 GUI。
- 当前默认 399 library + 510 binary 共 909 项 Rust 测试通过；egui 独立检查、完整测试/构建及原生目标命令的最终结果见[当前实施记录](../project-implementation-tracker/current.md)。
- CI 已扩展默认/egui 两套 check/严格 all-targets Clippy/build、原生 test 和组合 check；MSRV 两个前端均检查。
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

egui-only 在 Cargo 命令后加 `--no-default-features --features egui-ui`；启动加
`--bin ax_ssh_egui`，组合检查为 `cargo check --locked --offline --all-features --all-targets`。

## 风险与历史证据

- `russh` 的 RSA 兼容 feature 仍受 CI 中已有 RustSec 例外约束；本轮未升级或扩大例外，不把布局整理描述成依赖安全问题已解决。
- 先前实际 Windows 滚轮问题、XQuartz 启动及真实平台功能的验收边界保留，不通过历史通过数字推断当前主机已复现。
- 整理前 current 中混排的环境/验证资料已逐字保存在[历史环境快照](history-before-2026-10-20.md)；仅供追溯，不代表本机当前已安装目标或本轮验证结论。
- 环境变更按时间追加到 [changes.md](changes.md)；不再把旧任务的“本轮”“当前”叠加到此快照。

## 最后确认

- 2026-10-22 12:05 +0800：Rust 1.95、egui-only/default 最低工具链 check 已验证；默认 909 回归通过，egui/组合和本机 target 收尾见实施记录。其它平台和真实 GUI/设备/性能仍需原生验收，未推送或改动安装版。
