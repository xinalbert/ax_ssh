# 当前项目实施记录

## 当前目标

- 目标 ID：20261012-terminal-media-on-demand
- 目标：移除终端媒体手动入口，按系统录音接口自动接入；本地 CLI 走原生音频，Linux SSH CLI 走 PulseAudio/pipewire-pulse 音源与按需采集。
- 交付物：空闲终端空间恢复、标准录音流驱动的 SSH 音频桥接、共享采集互斥/有界流/清理、标准接口回归、双语文档与验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/terminal-pane.slint`、`ui/settings/terminal.slint`、翻译目录/生成脚本、`packaging/macos/Info.plist` 麦克风用途文案、`src/audio.rs`、`src/ssh.rs`、`src/ssh/worker.rs`、`src/ssh/worker/{shell,media}.rs`、新标准音源 bridge、`src/app/connection/worker_start.rs`、`src/app/connection_monitor.rs`、`src/app/terminal_media.rs`、`src/app/view/tests.rs`、双语使用/架构文档及项目记录。
- 不在本轮范围内：CLI 名称/命令/终端文本适配、云转写、录音驱动替换、SSH trust/凭据/配置 schema、依赖/CI 升级、推送/发布。
- 所有权：Slint 只保留终端网格及分屏关闭布局；SSH worker 发布标准虚拟音源并监听其非暂停录音流，音频领域持有共享设备租约和有界 PCM，worker 所有的辅助通道负责转发/清理；应用层只传 owned 音频配置，不传 Slint/设备/网络 handle。
- 安全边界：只有已验证并认证的 SSH 会话可以准备虚拟音源；仅该音源的非暂停录音流触发采集，空闲不打开麦克风；关闭流/会话时停止并清理；共享设备仅允许一个采集 owner，PCM 有界，录音权限由系统处理。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| MO1 | completed | 标准录音流驱动的音频桥接与手动 UI 撤除 | 空闲无设备访问、non-corked/所属 source 判定、状态分片/队列/取消/清理与布局回归 | 用户选择 CLI 自动调用并要求标准化；已撤下未提交的手动菜单方案 |
| MO2 | completed | 双语文档、地图和自动门禁 | Slint 重编、fmt/check/严格 Clippy/test/build、translation/链接/skill/tracker/diff | 不改依赖、工具链或 CI |
| MO4 | completed | 标准录音功能分项本地提交 | 显式暂存/index 复核、提交后状态与历史检查 | 用户授权；功能、翻译生成脚本、双语文档分别提交，不推送/发布 |
| MO3 | pending | 用户真实 GUI/录音验收 | 主/独立窗口、空闲空间、标准录音流启停、权限与虚拟音源清理 | 不自行截图或验收视觉 |

## 已完成

- 已查看用户截图并读取媒体/UI/终端菜单、所有权文档和项目地图；无条件的 32px 媒体行及任务结束保留状态造成常驻显示。
- 工作区初始干净，HEAD 为前轮文档提交 70736a7；前轮窗口图标与 SFTP 重命名已分别提交，GUI/真实平台验收保留在历史。
- 已核对 Cargo/lock/build.rs/CI、本机 Rust/Cargo 1.97.1、两个 macOS target；Rust 2024/MSRV 1.92.0 和 Slint 1.18.1 保持。
- 已先写实施计划与环境预检，不使用多 agent、不创建 Worker 记录。
- 用户选择 CLI 自动接入并要求避免个别 CLI 适配；已撤下本轮未提交的手动菜单源码，刷新标准音频边界与计划。
- 已核对 PulseAudio 官方客户端录音/暂停语义、pactl 输出和 PipeWire pipe-source 选项；不解析 CLI 名称或终端文本。
- MO1 已完成：按自有音源非暂停录音流启停，设备租约保持至实际 stream 关闭，停止清队列，EOF/TERM 回收 helper；去掉终端常驻媒体行和设置页 whisper 字段，历史配置内部兼容。
- 已添加拒绝辅助 exec 的 SSH fixture 行为；不支持音频仍保持正常 shell，实际录音后失败走原连接代次状态，不显示空闲错误行。
- MO2 自动门禁完成；音频/布局/翻译生成均有回归，权限用途文案与标准录音方式一致。未启动 GUI/真实麦克风、未连接真实远端；不推送或发布。
- MO4 分项本地提交交付：录音桥接与界面 b957d27，翻译生成脚本 ffd6f5a；双语说明、地图、研究和验证/提交记录随包含本记录的文档提交保存。逐次显式暂存并复核 index，未纳入构建物/缓存。

## 验证

- 已完成：环境预检、Slint 重编、7 项标准录音回归（含无读者 FIFO 的 TERM 清理）、主/独立窗口 2 项数值布局回归；fmt/check/严格 Clippy、完整 754 项测试及两个 macOS 架构 CI 同款 check/Clippy/build；549 条翻译及隔离生成一致、相对链接、skill/tracker/diff。默认 debug 和两个 macOS target 程序已构建；ARM64 app 已打包并验证用途文案/签名。
- 未完成：用户 GUI/真实设备/远端与 Windows/Linux 原生验收。

## 风险与阻塞

- 修改头部高度会改变实际 terminal rows；须以既有数值布局回归验证，不把自截图当视觉验收。
- 虚拟音源使用远端系统音频接口，直接读取硬件设备的程序不走该接口；当前远端桥接能力仅 Linux PulseAudio/pipewire-pulse，不宣称跨平台驱动能力。
- 真实录音权限、设备和 Linux 音源链路仍由目标环境验收；本机缺少 Windows/Linux SDK。

## 下一步

- 使用 `target/automatic-audio-preview/AxSSH.app` 验收主/独立窗口、系统录音权限和 Linux 标准音源启停/清理；默认 debug 程序为 `target/debug/ax_ssh`。MO3 保留用户实机验收。

## 最后更新时间

- 2026-10-12 +0800
