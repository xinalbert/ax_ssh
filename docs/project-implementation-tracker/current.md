# 当前项目实施记录

## 当前目标

- 目标 ID：20261104-sftp-reconnect-recovery
- 目标：参考公共 SFTP 客户端的重试语义，修复 SSH/SFTP transport 断连时传输意图被静默丢弃的问题，并补齐 macOS 目录原生 file promise 的安全递归落地。
- 交付物：应用层传输意图保留、SFTP worker 重连重新提交、旧 attempt 事件隔离、文件/目录原生 promise、空目录与类型变化回归测试、双语架构说明和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/sftp/directory.slint`、`src/app/macos_file_drag.rs`、`src/app/sftp_bridge/drag.rs`、`src/app/connection_monitor.rs`、`src/sftp/transfer/{discovery,local,tests}.rs`、SFTP 根 DTO、相关 worker 回归和双语架构/追踪文档。
- 不在本轮范围内：跨平台系统文件管理器的原生 promise、字节级断点续传、修改 SSH 主机密钥信任或凭据存储边界。

## 当前状态

- 阶段：已完成本机实现与验证
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| S1 | completed | 检索公共实现并确认断连丢失根因 | OpenSSH/WinSCP/lftp 文档、worker/UI 日志与代码追踪 | 公共实现保留可重试意图和临时发布；当前 worker 退出时丢弃 queued/pending 请求 |
| S2 | completed | 建立应用层 SFTP 传输意图和断连保留状态 | 状态单测、事件 DTO 编译检查 | 覆盖根下载、目录子项、上传和冲突等待 |
| S3 | completed | 重连成功后重新提交并隔离旧 attempt 事件 | connection monitor/worker 行为测试 | 新 attempt 只接受新 worker 的事件 |
| S4 | completed | 完成完整门禁和双语文档 | fmt/check/Clippy/test/tracker/diff | 已补充 subsystem channel 重开边界与 loopback 回归；本轮门禁已完成 |
| S5 | completed | 分离 SFTP browser channel 恢复与 SSH reconnect | loopback worker 14 项、完整 Rust 门禁、双语说明与 tracker | 原 transport 重开、有界退避、请求级错误、取消与传输连续性均通过 |
| S6 | completed | 依据真实日志修复通道拒绝触发的循环重连 | 受限服务器/monitor 回归、完整 Rust 门禁与新版构建 | 13:51 日志证实客户端主动退役；共享预算、局部重试、取消和终态隔离均已实现并通过回归 |
| S7 | completed | macOS 目录原生 promise 与安全递归落地 | transfer focused tests、完整 Rust 门禁 | `public.directory`、空/嵌套空目录、本地目标安全创建及目录类型变化已覆盖；Finder GUI 仍需人工验收 |

## 已完成

- 已确认仓库原始状态干净，当前稳定版为 Rust/Cargo 1.99.0，另已安装 Rust 1.92.0。
- `cargo update` 已更新 92 个符合 Rust 1.92 的锁定依赖，包含 `pulldown-cmark 0.13.6` 安全修复、Tokio/UUID 更新，并移除两个已 yanked 版本。
- 稳定版 fmt/check/严格 Clippy/完整测试、Rust 1.92 ARM64 macOS offline check、`cargo audit --ignore RUSTSEC-2023-0071` 与仓库入口链接校验已通过。
- `notify 8.2.1` 声明 MSRV 1.85，修复空路径 `unwatch` 崩溃，并将 macOS backend 从 `fsevent-sys` 迁移至 objc2。
- `russh 0.65.0` 声明 MSRV 1.89，修复 SSH 客户端 EXT_INFO 无界增长和零最大包导致 channel 写入停滞；loopback 测试服务器已适配新公钥回调。
- 剩余候选审查确认 CPAL 0.18.2 要求 macOS 14.2，故保留 0.17.3；russh 精确固定的 RSA/PKCS#1 版本也不能单独更新。
- 稳定版 fmt/check/严格 Clippy/927 项测试、Rust 1.92 ARM64 macOS check、ARM64 macOS build、RustSec 和 SSH 19 项 focused tests 均通过。
- SFTP 恢复实现已保留根下载、目录根、显式下载、上传和等待冲突确认的应用层意图；连接断开后未完成行显示等待重连，新 SFTP worker 连接成功后重新提交。目录根重新发现，子项 ID 由根 ID 和远端路径稳定导出；旧 attempt 的迟到事件继续由既有 attempt 路由拒绝。
- SFTP browser channel 与 SSH transport 分层处理：transport 存活时优先在原连接重开 subsystem，最多连续两次、退避 1/2 秒；目录页成功并连续存活 30 秒后才恢复额度。失败或持续不稳定才交给整条 SSH 重连；独立传输继续，恢复期浏览请求失败不升级为连接失败。
- 诊断日志已证实新通道被 AdministrativelyProhibited 拒绝后，app 错误地立即退役整个连接并 1 秒后重连。SSH owner 现共享六个后台 SFTP 许可，限制一个 discovery 与最多四个 file transfer，browser 保留通道；channel open 最多重试两次（250/500 ms），文件 stream 最多重试两次（1/2 秒），用户取消可中断。活连接的局部最终失败明确上报且不影响兄弟任务；app 等待 session 终态才重连。
- local writer 的失败隔离已与用户取消分开，允许清理本次残缺文件后重试；上传每次用新 execution ID，无法确认最后 rename 发布结果时不自动重放。新增限制服务器会话数、530 文件加 12 roots、流中断、拒绝耗尽、退避取消和 monitor 隔离回归。

## 验证

- 已完成：前序 MSRV/build/audit；本轮 worker loopback 19 项、完整 416 library + 523 binary 共 939 项通过；fmt、locked/offline check、严格 all-target Clippy、locked/offline debug build 和 Markdown/项目 skill/tracker/diff 校验通过。批量回归在八会话限制下以并发偏好 16 完成 530 文件加 12 roots，且只认证一次；单文件故障隔离、传输重试、取消和发布不确定保护通过。
- 未完成：用户真实服务器用新版复测；Windows/Linux/Intel macOS 原生 CI 尚需对应 runner 验证，本机 Windows MSVC 交叉 check 缺少目标 C 工具链。

## 风险与阻塞

- `russh 0.65.0` 仍精确依赖 `rsa 0.10.0-rc.18` 和 `pkcs1 0.8.0-rc.4`；本轮保留 RSA 兼容 feature 和现存审计例外。
- `cpal 0.18.2` 的 macOS 14.2 下限不满足项目 macOS 11 部署目标；`russh 0.65.0` 仍精确固定易受 `RUSTSEC-2023-0071` 影响的 RSA 版本，升级 russh 不会消除该风险。
- 依赖更新需要原生 CI 覆盖 Windows/Linux/macOS；本机只提供 macOS ARM64 原生目标，其它平台不能由本机验证替代。
- Windows MSVC 交叉 check 在 `aws-lc-sys` 的 C 编译阶段失败，因本机缺少适配目标的 MSVC C 工具链；此结果不代表原生 Windows CI 结果。
- 13:51 日志已证实 AdministrativelyProhibited 后客户端主动重连；服务端具体限制（如 MaxSessions）仍需 sshd 日志确认，不能从 reason code 证明实际配置。

## 下一步

- S7：用户退出旧进程后运行新构建 `target/debug/ax_ssh`，用同一批文件复测；核对 channel refusal 的局部重试、最终文件结果及是否还有 session 终态。服务端具体拒绝策略须结合 sshd 日志；其他平台由原生 CI 验证。

## 最后更新时间

- 2026-11-05 14:52 +0800
