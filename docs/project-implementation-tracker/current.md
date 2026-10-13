# 当前项目实施记录

## 当前目标

- 目标 ID：20261013-sftp-file-promise
- 目标：修复 macOS SFTP 拖到 Finder 时的鼠标事件错位、失败回调遗漏和连接结束后的 promise 生命周期。
- 交付物：原生鼠标按下快照、按连接代次清理的文件承诺、SFTP 可见错误、行为回归、双语架构说明和本地 Git 提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/macos_file_drag.rs`、`src/app/terminal_bridge/native.rs`、`src/app/sftp_bridge/`、`src/app/connection_monitor.rs`、相关测试、`docs/architecture*.md` 和实施记录。
- 不在本轮范围内：目录/链接 Finder 拖出、文件覆盖策略、文件系统发布方式、依赖/工具链升级、真实远端访问、应用安装、远端推送或发布。
- 所有权：AppKit 主线程拥有原生事件和 promise；应用层按 Tab/连接代次路由意图及取消；原有 SFTP worker 独占网络和本地 writer。SSH 信任和凭据契约不变。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| D1 | completed | 捕获并校验真实左键按下事件 | 错窗口、松开、事件消费回归 | 不再在延后 moved 回调中读取 currentEvent |
| D2 | completed | promise 回调和连接代次收尾 | 失败、重复请求、断线、迟到结果回归 | 原生对象不跨 Tokio 边界 |
| D3 | completed | SFTP 错误反馈和双语文档 | 无窗口 callback/状态测试、链接检查 | 不改 GUI 布局 |
| D4 | completed | 完整本机门禁和最终 diff | fmt/check/Clippy/test、ARM64 CI 命令、tracker | Finder 人工验证由用户完成 |
| D5 | completed | 源码/回归与文档两组本地提交 | staged diff/check、提交边界和最终工作区核对 | 用户已授权提交，不推送远端 |

## 已完成

- 用户明确同意修复；工作区干净。确认运行中的 2026.10.10 原生桥接与当前实现一致。
- 前轮检查的 7 个现有原生/本地写入/loopback SFTP 测试通过；这些测试尚未覆盖拖拽启动及断线 promise。
- 本机 Rust 1.99.0、原生 ARM64 target 可用；保存 Rust 2024/MSRV 1.92 和所有 CI target。
- 完成主线程事件捕获/一次性消费、64 项上限、按 Tab/attempt 清理及 Finder 回调所有权；11 个原生行为测试通过。
- 启动/入队错误写入 SFTP 状态，旧 attempt 的请求/错误拒绝；双语架构、地图和来源记录已更新。
- 最终 12 个 file-promise 回归通过；完整 391 library + 442 binary（833）测试通过，原生 ARM64 可执行程序已生成。
- 源码与回归提交为 `cdd6255e6800`；双语架构、地图、来源和本记录随后续文档提交交付。提交仅含本轮明确路径，target/cache/日志未进入 index。

## 验证

- 已完成：fmt/check/严格 all-target Clippy、完整 833 Rust 测试、ARM64 target check/严格 Clippy/build、diff 检查；12 个 file-promise 回归及生成 callback 到 SFTP 状态验证。6 个 Markdown 相对链接、skill validator 通过；current/map/新增 tracker 记录无新错误，原有历史格式错误保留。
- 未完成：Finder 实际拖放、窗口焦点/关闭等人工交互；Windows/Linux 原生 runner 验证（由 CI 负责）。

## 风险与阻塞

- 不捕获或检查应用截图；Finder 实际交互由用户确认。
- 本机没有 Windows MSVC/Linux 原生 SDK；这些平台保持 CI 验证，不声称跨平台链接通过。
- 既有 tracker 历史曾有日期格式校验问题；验证时区分基线与本轮错误。
- 技能校验的 PyYAML 只安装在 ignored `target/skill-validation-env`，未改变全局 Python 环境或项目依赖。

## 下一步

- 用户退出旧版并运行 `./target/aarch64-apple-darwin/debug/ax_ssh` 验证 Finder。应用安装、远端推送和发布不在本轮范围内。

## 最后更新时间

- 2026-10-13 17:09 +0800
