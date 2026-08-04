# 当前项目实施记录

## 当前目标

- 目标 ID：20260805-TABCYCLE01
- 目标：为工作区 Tab 增加符合平台习惯的前后循环快捷键。
- 交付物：macOS `Cmd+Shift+[` / `Cmd+Shift+]`、Windows/Linux `Ctrl+Shift+[` / `Ctrl+Shift+]`，状态层循环逻辑、回归测试和同步的双语文档。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/app.slint`、`src/app/{input,state,workspace,diagnostics}.rs`、相关测试、双语架构/使用文档和项目跟踪文档。
- 不在本轮范围内：持久化快捷键设置、Tab 拖动/重排、SSH transport、认证/主机密钥策略、`third_package/axshell` 耦合和 GUI 截图验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| STATE1 | completed | 按现有工作区顺序前后循环活动 Tab | AppState focused tests | 0/1 个 Tab 不变，首尾 wrap。 |
| UI2 | completed | Window 菜单及跨平台原生 accelerator | Slint/Rust 联合编译、input tests | 复用现有菜单快捷键解析，不扩展设置 schema。 |
| DOC3 | completed | 双语架构、使用说明和项目地图同步 | Markdown relative-link checks | 明确平台按键和循环语义。 |
| VERIFY4 | completed | 完整 Rust 与差异门禁 | fmt/check/clippy/test/diff 可用项 | GUI 键盘行为由用户在目标平台验收。 |

## 已完成

- 已复核 Rust 2024、MSRV 1.92.0、Slint 1.17.1、现有菜单快捷键解析和 Tab 状态所有权。
- 已确认快捷键解析可表达 bracket 组合，Tab 顺序由 `AppState.tabs` 唯一持有，窗口菜单通过 `slint::Keys` 接收平台 accelerator。
- 已确认本机 Cargo `fmt` / `clippy` 子命令缺失；施工前 locked/offline 基线为库 117、应用 84、Doc tests 0。
- `AppState::cycle_tab` 现在按内存 Tab 顺序前后循环并在首尾 wrap；0/1 个 Tab 不改变活动状态。
- Window 菜单新增 Previous/Next Tab，按平台注入固定 bracket accelerator，只在多于一个 Tab 且共享菜单快捷键闸门开放时启用。
- 状态循环、四组平台 bracket 解析和 diagnostics 固定 action 的 focused tests 均实际命中并通过，完整 Slint 图已重新编译。
- 双语架构/使用说明和项目地图已同步；tracker validator、46 个 Markdown 相对链接与 `git diff --check` 通过。
- 最终复核未引入参考项目耦合、秘密字段、网络/worker handle、无界队列、UI 线程阻塞、`unsafe` 或持久化 API 扩张。

## 验证

- 已完成：相关 Rust 文件直接 `rustfmt --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 117、应用 85、Doc tests 0）、tracker validator、46 个 Markdown 相对链接检查和 `git diff --check`；focused 状态/input/diagnostics 回归均实际命中并通过。
- 未完成：无。Cargo fmt/clippy 因本机子命令缺失未执行；直接 Rustfmt 已覆盖全部本轮 Rust 文件，Clippy 保留给具备组件的三平台 CI。

## 风险与阻塞

- Slint/winit 对 native menu bracket accelerator 的实际派发仍需 macOS、Windows 和 Linux 目标平台验收。
- 工作树包含上一轮五项审查修复的未提交改动；本轮只追加相关差异，不回退或覆盖已有修改。
- 本轮不改变 host-key 默认拒绝、秘密短期生命周期、bounded channel 或 UI/Tokio/russh 所有权边界。

## 下一步

- 用户在 macOS、Windows 和 Linux 目标平台验收原生/客户端菜单 accelerator、焦点和安全提示禁用行为。

## 最后更新时间

- 2026-08-05 07:36 +0800
