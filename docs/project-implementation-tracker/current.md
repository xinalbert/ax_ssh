# 当前项目实施记录

## 当前目标

- 目标 ID：20261029-sftp-complete-transfer
- 目标：移除 SFTP 常规上传、下载和目录浏览中的客户端内容截断限制，确保队列背压不会丢弃后续文件，并呈现逐文件进度与路径。
- 交付物：完整的递归发现、无客户端文件大小/路径名称长度/总目录文本预算限制、队列满等待、FileZilla 风格 Transfers 行、双语说明和回归验证。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/sftp*`、`src/ssh/worker/sftp.rs`、`src/app/{local_files,sftp_bridge,state}.rs`、SFTP Slint 与设置搜索翻译、`Cargo.lock` 的三个 RustSec 修复版本、双语用户/架构文档和实施 tracker。
- 不在本轮范围内：本地打开缓存和远程编辑的独立容量策略、SSH/SFTP 服务端或操作系统自身限制、其它传输协议。
- 所有权与安全：Rust 负责发现、路径校验、worker 排队和 transfer 状态；Slint 仅展示行状态、路径、速度和 ETA。保留路径逃逸/控制字符/symlink/文件类型校验、取消/暂停及协议资源背压；本地打开快照和外部编辑的独立缓存限制不属于普通文件传输。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| S1 | completed | 移除常规单文件上限和超限跳过逻辑 | 配置/设置搜索/UI 不再暴露上限；旧配置字段可忽略 | 常规流式上传和下载支持任意服务端/系统可表示的大小 |
| S2 | completed | 取消目录与路径名称字符预算 | 本地/远端目录不因总文本预算提前截断；长路径名称回归 | 分页只控制每次渲染行数 |
| S3 | completed | 让发现、worker 和 UI 队列等待背压 | 超过 512 项、深目录和队列满场景回归 | 满队列等待；取消和断连仍按现有生命周期清理 |
| S4 | completed | 同步 FileZilla 风格显示与中英文说明 | 翻译生成和双语文档/项目地图校验 | Transfers 展示名称、来源/目标、状态、速度和 ETA |
| S5 | in_progress | 完成整棵工作区门禁并等待目标平台验收 | fmt/check/严格 Clippy/full tests/diff 和专用校验 | 本轮 SFTP 链路修复已通过本机门禁；原生 Windows 和真实服务器/GUI 待验 |
| F1 | completed | 定向更新三个漏洞依赖 | 检查锁文件差异和 RustSec 审计 | crypto-bigint 0.7.10、ctutils 0.4.7、pulldown-cmark 0.13.5；保留现有 RSA 例外 |
| F2 | completed | 缩小上传遍历枚举的目录分支 | 原有递归发现回归及 Windows target Clippy | 已使用 Box<ReadDir>；运行验证归 F3，不抑制 lint |
| F3 | completed | 完成本机门禁和记录验证边界 | focused tests、fmt/check/Clippy/test、仓库与 tracker 校验 | 本次修复隔离副本全门禁及 912 测试通过；Windows 枚举对照通过，全项目因缺 SDK 未验证 |

## 已完成

- 移除 Settings > SFTP 的常规传输单文件大小选项及配置字段，历史 JSON 的旧字段不再影响传输。
- 移除上传/下载与递归发现阶段的客户端文件大小、文件名/路径字符数和目录总文本预算；移除本地目录名称/路径/文本预算，浏览器以 250 行分页完整枚举。
- 移除 Transfers 历史窗口截断；上传/下载提交队列满时等待，发现通过有界通道背压，不拒绝超出窗口的后续文件。
- 保留 unsafe path、控制字符、symlink、文件类型、原子发布与冲突确认等安全校验；保留数据分块、协议 frame、并发和读写窗口作为流程/资源边界。
- 保留仅用于本地打开缓存和外部编辑的独立大小限制，这些不参与常规 SFTP 传输。
- 定向升级三个 RustSec 受影响依赖，crypto-bigint 避开已撤回的 0.7.6–0.7.8，采用兼容且未撤回的 0.7.10；其余为 ctutils 0.4.7 和 pulldown-cmark 0.13.5，MSRV 均不超过 1.92。
- `UploadWalkFrame::ReadDirectory` 保存 `Box<ReadDir>`，目录首次打开时分配，后续遍历帧之间直接移动该 Box，生命周期和遍历顺序保持。

## 验证

- 已完成：本次锁文件在沿用 RSA 例外的 cargo-audit 中为 0 漏洞，剩余 5 条未维护及 1 条 clipboard-win 撤回警告；实际工作区上传两项回归通过。HEAD 加本次锁文件/upload.rs 修复的隔离副本 fmt、locked/offline check、严格 all-targets Clippy、401 library + 511 binary 共 912 测试和 Doc tests 0 全通过，无 warning；Windows 枚举 metadata-only Clippy 原版失败、修复版通过。Markdown/源入口、skill/tracker 及 diff 检查通过。
- 未完成：Windows 全项目检查受本机缺少 SDK（windows.h）限制，仍交原生 CI；真实服务器大目录/大文件传输与目标平台 GUI 视觉验收仍由用户完成。

## 风险与阻塞

- 不设客户端文件大小、文件/目录数量、递归深度、路径/名称字符数和目录文本总量上限；目标服务器与操作系统自身约束仍会使个别条目失败，应用会逐项报告。
- SFTP 并发、分页、分块与协议请求超时仍保留；这些控制吞吐/内存和界面渲染节奏，不作为内容数量或文件大小门槛。
- 本轮目录根聚合和上传背压改动跨越应用/transport 边界，已通过本机严格 Clippy 与完整测试；取消、路径和协议资源安全校验保持不变。
- 主机为 macOS ARM64；Windows Rust target 已安装，但全项目 check 在 aws-lc-sys 因缺 windows.h 失败，未验证原生 Windows 链接/运行。提取实际枚举的 Windows metadata-only Clippy 对照已证实原版失败、装箱版通过。

## 下一步

- 继续执行原生 Windows、真实服务器大目录/大文件传输和目标平台 GUI 验收；若发现视觉或链路差异，再按具体事件序列修正。

## 最后更新时间

- 2026-10-31 00:00 +0800
