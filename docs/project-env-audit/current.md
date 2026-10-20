# 当前项目环境状态

## 项目边界

- 独立 Rust 2024/Slint 桌面应用；工作区只包含 `ax_ssh` package。
- 当前范围：2026-10-20 全项目规范整理；测试布局、文档事实和工程校验，不调整功能、依赖、协议或配置。
- Cargo/Slint 编译、Linux/Windows/macOS CI 与项目自有源码是依据；`vendor/` 保留锁定补丁，参考子模块不进入构建。

## 仓库运行要求

- MSRV：Rust 1.92.0；edition 2024。Cargo 锁文件是可复现依赖来源。
- UI 入口：`build.rs` 只编译 `ui/app.slint`；生成类型只在 `src/app.rs` 声明。
- Slint 及本地补丁 1.18.1；Tokio 1.53.3、russh 0.64.1、russh-sftp 3.0.1。
- arboard 3.7.0、rubato 5.0.1、base64 0.23.1、serde_json 1.0.154、uuid 1.28.0；CPAL 固定 0.17.3，保留 macOS 11 部署范围。
- Linux 构建需要 pkg-config、Fontconfig/XKB 与 ALSA 开发包，具体命令见[开发说明](../development.zh.md)。
- Python 工程工具要求 3.10+、标准库和 rustfmt，不依赖本机个人技能路径或第三方 Python 包。

## 本机可用环境

- 宿主 macOS ARM64；本机默认 rustc 1.99.0、Cargo 1.99.0、Python 3.14.6。
- 已安装 Rust target：`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`。
- Windows target 不代表存在 Windows SDK/执行环境；本轮不运行 Windows/Linux 或 Intel macOS 程序，不安装 SDK 或更换工具链。
- Cargo 缓存可支持本轮 locked/offline 构建；本轮无需联网或真实远端服务。

## 测试与 CI

- 默认验证：fmt、locked/offline check、严格 all-target Clippy、locked/offline test、`git diff --check`。
- CI 仍对 Windows x86_64、Linux x86_64/ARM64、macOS ARM64/Intel 分别 check、严格 Clippy 和 build；只在原生 runner 上执行 Rust tests。
- 终端/SSH/SFTP 测试使用隔离临时文件、固定信任的 loopback peer、注入设备或无窗口 Slint，不访问真实 keyring/麦克风/远端，不以自主截图验收 GUI。
- 本轮布局调整后的 399 library + 481 binary 共 880 项 Rust 测试、26 项 Python 回归、fmt/check/严格 Clippy、翻译/图标、53 份 Markdown/源入口及完整 tracker 检查通过；详见[当前实施记录](../project-implementation-tracker/current.md)。
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

- `russh` 的 RSA 兼容 feature 仍受 CI 中已有 RustSec 例外约束；本轮未升级或扩大例外，不把布局整理描述成依赖安全问题已解决。
- 先前实际 Windows 滚轮问题、XQuartz 启动及真实平台功能的验收边界保留，不通过历史通过数字推断当前主机已复现。
- 整理前 current 中混排的环境/验证资料已逐字保存在[历史环境快照](history-before-2026-10-20.md)；仅供追溯，不代表本机当前已安装目标或本轮验证结论。
- 环境变更按时间追加到 [changes.md](changes.md)；不再把旧任务的“本轮”“当前”叠加到此快照。

## 最后确认

- 2026-10-20 09:37 +0800：本轮整理与本机自动门禁完成，按用户后续授权分组本地提交；提交阶段沿用已有验证结果。工具链、依赖及目标矩阵保持，未推送或改动安装版，其它平台和真实 GUI/设备验收保留。
