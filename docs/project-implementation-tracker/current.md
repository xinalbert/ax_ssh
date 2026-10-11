# 当前项目实施记录

## 当前目标

- 目标 ID：20261011-agent-clipboard-microphone
- 目标：落实剪贴板审计建议，为 SSH 上的 Codex CLI 等终端工具提供图片附件、本机语音转写和 Linux 远端麦克风链路。
- 交付物：可报告错误的剪贴板适配、OSC 52 权限/大小修复、私有附件上传、显式麦克风启停、转写确认入口、跨平台编译配置与双语使用说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：平台剪贴板/音频适配、应用回调及小型状态 DTO、现有 SSH worker 的独立 SFTP/音频通道、终端操作入口、依赖和打包/CI、文档与记录，以及用户要求的本地分组提交。
- 不在本轮范围内：远端安装软件、云端转写/凭据管理、RDP、Codex 内部修改、推送或发行、参考项目代码。
- 所有权：Slint 发用户意图；平台适配采集/读剪贴板；SSH worker 管认证连接与辅助通道；音频回调只写有界队列；转写/PNG 编码在后台。
- 安全边界：OSC 52 仍默认关闭且逐次确认读取；撤销后清除请求。图片只在用户粘贴后传送，文件/目录私有。麦克风只由用户开启，退出/断线取消；不把音频写入终端，不开放远端 TCP 音频端口。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| CM1 | completed | 剪贴板 Result 适配及 OSC 52 撤销/64 KiB 边界 | 回归、各 transport 协议响应 | 持久剪贴板 owner；错误不视为空文本 |
| CM2 | completed | PNG 图片粘贴及私有 SFTP 附件 | 大小/权限/失败/过期结果回归 | 本地 shell 使用本机附件；SSH 使用远端路径 |
| CM3 | completed | CPAL 采集和 Linux PulseAudio/PipeWire 麦克风通道 | 合成 PCM、背压、独立通道与取消测试 | 远端需 pactl/module-pipe-source；其他远端系统明确报错 |
| CM4 | completed | whisper.cpp 本机转写与确认插入 | WAV/命令参数/超时/取消测试 | 引擎和模型由用户配置，不自动下载 |
| CM5 | completed | 双语文档、打包权限、CI 和地图 | 链接、目录/skill/tracker validators | 保持 macOS 11 部署范围，CPAL 固定 0.17.3；Linux CI 补 ALSA 开发包 |
| CM6 | completed | 完整本机 Cargo 门禁及可运行预览 | fmt/check/clippy/test/build/diff | 最终源码 707 项测试、macOS ARM64/x86_64 check/严格 Clippy/build 和预览签名通过 |
| CM7 | pending | 用户真机端到端验收 | 剪贴板图片、系统权限、SSH 音源、CLI 输入 | 不自行捕获 GUI 截图 |
| CM8 | completed | 用户要求的本地分组提交 | 暂存差异、提交边界、文档与工作区检查 | 高亮/URL 为 858b5bf，终端媒体为 36a9f1d；实施记录单独提交 |
| HL4 | pending | 用户终端高亮显示验收 | 本地/SSH、ANSI、调色和真实日志 | 已收到 URL 区域异常截图，尚未验收通过 |
| TM4 | pending | 用户原生 GUI 验收 | Windows/Linux/macOS 菜单配色、键盘、子菜单和焦点 | 不捕获应用截图 |
| RT6 | pending | 用户原生验收延迟提示和连接生命周期 | 前后台、分屏/独立窗口、重连、真实 SSH/SFTP | GUI 验收属于用户 |
| IP5 | pending | 前轮服务器/Local Shell 选择器验收 | 宽窄窗口、常用排序/搜索、自动/手动、焦点 | 715c5dc/69b091f 已提交，657 项测试通过 |
| DI5 | pending | 前轮标题栏与 `+` 列表原生验收 | 分屏/返回/恢复、配色与列表键盘 | b6cbad7/f92c2c5 已提交 |
| PW5 | pending | 前轮逐服务器凭据清除原生验收 | 状态刷新、确认/取消、真实系统后端 | 自动验证完成 |
| UI7 | pending | 前轮导航系统图标原生验收 | 发行版/手动指定、配色、tooltip/焦点 | 自动验证完成 |

## 已完成

- 已审计现有文本剪贴板/OSC 52 和 SFTP/worker，确认图片与音频链路原来未实现。
- 官方来源确认 OSC 52 是文本协议、SSH 无标准麦克风转发；选择独立 SSH session + module-pipe-source，而非音频写入 PTY。
- 环境预检允许开工：Rust 2024/MSRV 1.92、Slint 1.18.1 不变，已有 image PNG 编码可复用。
- 前轮高亮/URL 自动门禁通过，已提交为 858b5bf，用户待验收项保留。
- 已实现图片附件、显式远端麦克风、本机转写确认与可保存的 9 种采样率/5 种声道设置；正常设置保存保留音频配置。
- 终端媒体实现与双语使用/架构/开发文档已提交为 36a9f1d；环境、研究、地图和施工验证记录单独提交。

## 验证

- 已完成：707 项测试（329 library + 378 binary）；45 种采样率/声道合成 PCM、实际 POSIX helper EOF/默认音源条件恢复、模拟 SFTP 权限/分块/失败、OSC 52 撤销代次/64 KiB 与转写 argv/WAV/Unicode/超时/大小检查；560 条中文翻译及文档/skill/tracker 校验。
- 已完成：最终源码的 fmt/check/严格 all-target Clippy；两种 macOS 架构 check/严格 Clippy/build 与含麦克风用途声明的 ARM64 预览 app；字体/许可资源和 ad-hoc 签名验证通过。Intel 只编译链接，不在 ARM64 主机运行测试。
- 收尾修复：转写插入把 CR/LF 转为空格，防止非 bracketed paste 终端提交；实际编码回归通过，预览已更新为最终 ARM64 构建。
- 预览：`target/agent-media-preview/AxSSH.app`，以最终 ARM64 构建打包；未自行启动 GUI 或截图。
- 未完成：Windows/Linux 原生 CI、真实剪贴板/麦克风/远端音频及 CM7 验收。

## 风险与阻塞

- Linux 远端须有用户级 PulseAudio 或 pipewire-pulse 与 pactl；原生 ALSA 硬件直读程序不自动获得该音源。
- Codex CLI /voice 为实验功能，账号/版本可限制可用性；音源链路不承诺启用上游未开放的功能。
- 本地 whisper.cpp 可执行文件和模型不随应用分发；缺失时显示明确配置错误。
- 本机仅 macOS SDK；其他平台需要 CI 原生编译和真机权限验收。

## 下一步

- 用户关闭旧版后运行预览 app，配置转写引擎/模型并按使用说明检查真实剪贴板、权限、声道/采样率与远端音源；保留 CM7 与 Windows/Linux 原生 CI 验收。

## 最后更新时间

- 2026-10-11 10:22 +0800：CM1–CM6 和 CM8 完成，707 项测试与两种 macOS 架构门禁/打包通过，实现已分组本地提交；CM7 与前轮用户验收保留。
