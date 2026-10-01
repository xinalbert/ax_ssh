# 当前项目实施记录

## 当前目标

- 目标 ID：20261001-sftp-local-external-refresh
- 目标：本地栏显示的目录被 Finder、资源管理器或其他本机程序修改时，自动更新 AxSSH 的本地文件列表。
- 交付物：当前本地目录的跨平台事件监听、合并刷新、Tab 生命周期清理、回归、双语说明和验证。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.{toml,lock}`、`src/app/{local_directory_watch.rs,sftp_bridge.rs,state.rs,state/sftp.rs,state/tests.rs}`、`src/app.rs` 和 SFTP 双语用法/架构及项目记录。
- 不在本轮范围内：远端目录轮询、文件自动上传或同步、配置 schema、SSH trust/凭据策略和 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SFTPWATCH1 | completed | 核对现有本地目录读取、Tab 生命周期与跨平台监听 API | 源码、环境和官方 notify 文档 | 沿用当前 SFTP 本地快照所有权 |
| SFTPWATCH2 | completed | 为每个已加载的 SFTP Tab 监听当前本地目录并合并变化事件 | 状态回归、Cargo check | 有界通道、后台读取、关闭时退出 |
| SFTPWATCH3 | completed | 同步双语说明、项目地图并完成门禁 | fmt/check/Clippy/test、文档/tracker/diff | GUI 与不同文件系统由用户验收 |

## 已完成

- 已核对本地栏只在导航、手动刷新和 AxSSH 下载完成后读取目录；其他本机程序的变化没有监听入口。
- 沿用 Rust 2024、MSRV 1.92.0、本机 Rust/Cargo 1.97.1、Slint 1.18.1 和现有 locked/offline 门禁；新增锁定的 `notify 8.2.0`，其声明 MSRV 1.77。
- 已查阅 notify 8.2.0 官方文档：跨平台推荐 watcher 支持非递归目录监听，macOS 默认使用 FSEvents；网络文件系统可能不发事件。
- 本轮在 application bridge 维护监听任务和有界事件通道；目录快照仍由 `src/app/local_files.rs` 后台读取，Slint 不持有监听器。
- 已锁定 `notify 8.2.0` 并实现每 Tab 当前目录非递归监听、导航重绑、单槽事件合并、忙碌时补刷及关闭退出；双语说明和项目地图已更新。
- 已复核多窗口刷新走 `WindowRouter` 的全窗口快照发布；监听任务仅持有弱 UI 引用。旧 X11 未提交改动保留原状。

## 验证

- 已完成：定向测试 1 项、完整测试库 297/应用 298/Doc tests 0；fmt、locked/offline check、严格 all-target Clippy、两种 macOS target 的 CI 同款 check/Clippy/build、文档相对链接、tracker validator 和 `git diff --check` 均通过。
- 未完成：用户在实际 GUI 下验证本地目录变化、目录切换、Tab 关闭及不同文件系统的事件投递；Windows/Linux target 在本机未安装，交由原生 CI。

## 风险与阻塞

- 原生文件系统事件可能合并或漏报；网络挂载目录可能不发事件，手动 Refresh 仍须可用。
- 监听任务随目录变化重新注册，并在 Tab 关闭后的短间隔内释放；运行时结束由 Tokio 取消任务。事件信号为单槽有界通道。

## 下一步

- 用户在最新构建中打开 SFTP Tab，用 Finder/资源管理器新增、重命名和删除当前本地目录中的文件，观察本地栏自动更新；网络挂载未收到事件时可使用 Refresh。

## 最后更新时间

- 2026-10-01 12:19 +0800
