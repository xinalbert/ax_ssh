# 当前项目实施记录

## 当前目标

- 目标：主窗口及独立工作区保持普通尺寸、位置和最大化状态，防抖自动保存并可靠恢复。
- 交付物：向后兼容窗口快照、屏幕范围/DPI 恢复、后台串行自动保存、多窗口恢复回归及双语契约。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config/workspace.rs`、`src/app.rs`、`src/app/window_router.rs`、`src/app/window_bridge.rs`、窗口持久化模块及工作区读写入口。
- 不在本轮范围内：依赖升级、SSH 信任/认证修改、恢复活动进程、自动恢复全屏或最小化、自动截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| WKEEP1 | completed | 窗口 DTO、普通几何缓存、DPI 与屏幕范围恢复 | 配置兼容及几何边界回归 | 最大化独立保存，保留普通尺寸 |
| WKEEP2 | completed | 有界串行自动保存、退出 flush、最近快照优先和 detached 恢复 | 保存顺序、恢复路由和启动优先级回归 | 不在 UI 线程写盘 |
| WKEEP3 | completed | 双语文档、地图、完整门禁及构建 | fmt/check/Clippy/test/build、tracker/Markdown/diff | 实际 GUI 由用户验收 |

## 已完成

- 已读取项目地图、架构及专项 skill；核实当前 schema 无窗口几何，仅退出自动保存。
- 环境预检：本机 macOS ARM64，Rust/Cargo 1.97.1，Slint 1.18.1；保持 MSRV 1.92，不新增依赖。
- 先写计划后施工；此前实施历史保留在月度 changes 中。
- macOS 主窗口在 FullSizeContentView 设置后恢复几何，保持内外框尺寸口径一致；已有最大化窗口加载快照时显式同步原生 zoom。

## 验证

- 已完成：定向回归及最终完整测试（库 285、应用 288、Doc tests 0）；fmt、locked/offline check、严格 all-target Clippy、debug build；macOS ARM64/x86_64 的 CI 同款 target check/Clippy/build 均通过；Markdown 相对链接、skill、tracker 和 diff 校验通过。
- 未完成：Windows/Linux 原生 CI 和真实 GUI/多屏验收；未运行非原生 Intel 测试。

## 风险与阻塞

- Wayland 不提供绝对窗口定位，位置交给 compositor；Windows/Linux 本机缺少 SDK/target，需 CI 原生验证。
- 强制终止可能丢失最后一次防抖尚未写盘的变化；终端只恢复既有有界文本。
- macOS 全屏/最大化动画和显示器热插拔需真实平台验收，不采集应用截图。

## 下一步

- 运行 `cargo run --locked --offline` 使用新构建进行实际窗口验收。
- 用户验收：调整主窗/独立 Terminal/SFTP 窗口位置尺寸，修改分屏比例，等约两秒后退出重开；核对位置、尺寸、最大化及分屏。
- 多屏验收：在副屏保存窗口后拔出副屏，重启确认窗口位于可见屏幕；全屏和最小化退出后应恢复普通窗口或先前最大化状态。

## 最后更新时间

- 2026-09-26
