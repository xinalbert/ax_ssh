# 当前项目实施记录

## 当前目标

- 目标 ID：20261113-sftp-windows-paths
- 目标：Windows 远端地址使用原生盘符格式显示，并接受从系统复制的路径。
- 交付物：远端路径展示、导航输入转换、主/独立窗口回归及双语说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：SFTP 路径展示、应用导航状态、共享 Slint 路径栏与两处复制、架构/tracker；按用户授权归档下载冲突与 Windows 路径两项本地提交。
- 不在本轮范围内：SSH 信任/凭据/配置 schema、框架升级、推送/tag/发布、GUI 启动或自制截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 定位 Windows 远端路径显示与粘贴链路 | 远端原样显示，盘符输入被当成相对路径 | 本地已有扩展路径展示 |
| P2 | completed | 显示/复制及导航输入转换 | 盘符、空格、中文、引号和 POSIX 隔离回归 | 保留 canonical 路径与文件身份 |
| P3 | completed | 完整门禁与双语记录 | Rust、Slint 重编译、Markdown/tracker/diff | 原生 Windows 交互由用户确认 |
| P4 | completed | 按功能拆分并创建本地提交 | 暂存 diff、提交范围和最终工作区检查 | 用户授权提交，保留原有历史 |

## 已完成

- 已读取路径展示、共享路径栏、两处复制及应用/transport 导航链路。
- 截图格式为 Windows SFTP 的 `/G:/...`；远端显示未转换，粘贴 `G:\...` 会按相对路径解析。
- 下载冲突实现与双语说明提交为 `bb18608`；Windows 远端路径展示/粘贴与双语说明提交为 `5d07e61`，共用文件按 hunk 拆分。
- 远端地址展示/复制/编辑共用 Windows 盘符格式；应用只在 current/home 具有盘符约定时转换粘贴值，支持成对引号、空格/中文和跨盘符。
- 主/独立窗口 headless 回归验证真实剪贴板粘贴、两处复制及编辑/刷新，应用导航回归验证 canonical 历史、前进和父目录；没有启动真实 GUI。
- 完整跟踪记录独立归档；仅创建本地提交，没有推送、创建 tag 或改写已有历史。

## 验证

- 已完成：10 项 windows_ 定向回归及全量 1026 项 Rust 测试（471 library + 555 binary）；fmt、Slint 重编译、locked/offline check、严格 all-target Clippy、精确 aarch64-apple-darwin CI check/Clippy、本机 debug build、56 份 Markdown/源入口、tracker 和 diff 检查通过。提交阶段未修改源码，重新复核 11 目录各 697 条翻译/生成一致性、文档与暂存范围。
- 未完成：用户真实 Windows SFTP 显示/粘贴交互；其它平台原生 CI 未在本机运行。`target/debug/ax_ssh` 已构建，未启动 GUI。

## 风险与阻塞

- 仅识别明确的 Windows 盘符远端路径；POSIX 名称中的反斜杠和引号不得改写。
- UI 可读字符串不得替代目录历史、刷新、条目及传输中的原始路径；不改变 SSH 信任或凭据契约。

## 下一步

- 用户用新构建确认重复下载冲突选择和 Windows 远端路径的原生显示、复制/粘贴与导航；本地提交已归档，未推送/tag/发布。

## 最后更新时间

- 2026-11-13 16:22 +0800
