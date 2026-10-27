# 当前项目环境状态

## 项目边界

- 独立 Rust 2024 桌面应用，默认 Slint 和实验 egui 分别运行；工作区只包含 `ax_ssh` package。
- 当前范围：E6R-UploadSelect 本地多选/批量上传开工，Cargo/lock、Rust/Cargo 1.97.1、Python 3.10.20 和双 macOS target 已核对，无依赖/schema/CI 变化。
- Cargo/Slint 编译、Linux/Windows/macOS CI 与项目自有源码是依据；`vendor/` 保留锁定补丁，参考子模块不进入构建。

## 仓库运行要求

- MSRV：Rust 1.95.0；edition 2024。Cargo 锁文件是可复现依赖来源。
- UI 入口：默认 slint-ui 的 `build.rs` 只编译 `ui/app.slint`，生成类型只在 `src/app.rs`；egui-ui 为 `src/egui_main.rs`，独立图不含 Slint/softbuffer。
- egui/eframe 0.37.0 + WGPU，系统字体/clipboard/AccessKit；实验会话、设置、工作区和日志使用独立目录，不修改默认应用的数据。
- Slint 及本地补丁 1.18.1；Tokio 1.53.3、russh 0.64.1、russh-sftp 3.0.1。egui 可选 build 依赖 resvg 0.48.1 生成 48px 预乘系统图标，不在 UI 解 SVG；其 normal/build 图无 Slint/softbuffer。
- arboard 3.7.0、rubato 5.0.1、base64 0.23.1、serde_json 1.0.154、uuid 1.28.0；CPAL 固定 0.17.3，保留 macOS 11 部署范围。
- Linux 构建需要 pkg-config、Fontconfig/XKB 与 ALSA 开发包，具体命令见[开发说明](../development.zh.md)。
- Python 工程工具要求 3.10+、标准库和 rustfmt；翻译源键合并 Slint/egui/搜索并生成静态 Rust 表，不增加运行时依赖。

## 本机可用环境

- 本轮宿主 macOS ARM64；rustc/Cargo 1.97.1、Python 3.10.20（实际命令输出）。
- 本轮已安装 Rust target：`aarch64-apple-darwin`、`x86_64-apple-darwin`。
- 本轮 rustup 列表无 Rust 1.95.0，未重跑 MSRV；两前端旧版通过只作历史记录。
- Windows target 不代表存在 Windows SDK/执行环境；本轮不运行 Windows/Linux 或 Intel macOS 程序，不安装 SDK。
- 当前锁定依赖缓存可用；E6R-Select 默认 490 library + 450 binary = 940、egui 593 library + 103 binary = 696 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、764×11 PO/生成（含 header 为 765）、299 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/选择/真实服务器下载验收保留，未自主截图。无新依赖、持久 schema、wire 协议、队列、task 类型或工具链/CI 变化；SSH 信任保持默认拒绝。未提交/推送/发布或使用多 agent/Worker。

## 测试与 CI

- 默认验证：fmt、locked/offline check、严格 all-target Clippy、locked/offline test、`git diff --check`。
- CI 仍对 Windows x86_64、Linux x86_64/ARM64、macOS ARM64/Intel 分别 check、严格 Clippy 和 build；只在原生 runner 上执行 Rust tests。
- 终端/SSH/SFTP 测试使用隔离临时文件、固定信任的 loopback peer、注入设备或无窗口 Slint，不访问真实 keyring/麦克风/远端，不以自主截图验收 GUI。
- 历史测试数量不代表本轮验证；egui 独立检查、完整测试/构建及原生目标命令的结果见[当前实施记录](../project-implementation-tracker/current.md)。
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

- 2026-10-26 06:53 +0800：E6R-Pause 默认 490 library + 450 binary = 940、egui 580 library + 96 binary = 676 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、281 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。
- 运行/测试：Rust/Cargo 1.97.1、Python 3.10.20 与双 macOS target 确认；MSRV 1.95 未安装，Cargo/lock/五目标 CI 仍为依据。
- 范围：实验传输状态/资格/有界历史、Pause/Resume/Cancel 和 UI，复用现有默认拒绝信任/凭据/worker 控制，暂停为连接内 chunk 调度，无上传暂停/持久断点。
- 开工判定：允许开工；已先预检/官方版本查询/计划，再实施/完整验证。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行和 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/滚动/真实服务器暂停延迟及取消验收保留。未自主截图、提交/推送/发布或使用多 agent/Worker。
