# 当前项目实施记录

## 当前目标

- 目标：补齐 SSH known_hosts 兼容能力，同时保留未知或变更 host key 默认拒绝的安全边界。
- 交付物：读取系统 OpenSSH known_hosts（含端口、别名、hashed host、multiple keys 和 @revoked）；将有效匹配纳入 SSH trust 判定；撤销记录不可被普通确认绕过；提供安全的显式记录管理/原子更新能力；双语文档和确定性回归测试。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/ssh.rs`、`src/ssh/known_hosts.rs`、连接请求与 host-key 状态、双语架构/使用文档与实施记录。
- 不在本轮范围内：自动接受未知或变更 host key、明文凭据持久化、完整 SSH certificate/CA 管理、跨进程连接恢复、SFTP 传输协议、依赖或工具链升级，以及参考工程源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TR1 | completed | 有界 download state、选择和暂停/恢复/取消状态转换 | `app::state` 定向回归 | 状态属于 AppState；UI 只接收有界行 DTO。 |
| TR2 | completed | SFTP worker 的递归展开、暂停/恢复/取消与断点缓存协议 | `sftp::transfer`、`ssh::worker::sftp` 回归 | russh session 留在 worker；取消只清理该任务创建的数据。 |
| TR3 | completed | 三个 Transfers 页面、行勾选和批量操作回调 | `cargo check --locked --offline` | Slint 不执行文件系统或网络操作。 |
| TR4 | completed | 双语说明、跟踪资料和完整仓库门禁 | fmt/check/clippy/test/tracker/diff | GUI 交互仍需用户在目标平台验收。 |
| WR1 | completed | worker-owned SFTP 写命令、上传与有界远端文件操作 | focused worker/transfer tests | 上传使用私有远端临时文件和 rename；写操作串行复用独占 stream。 |
| WR2 | completed | 应用状态、在线编辑/Save As、删除/重命名和冲突检测 | app state/bridge tests | 编辑内容有界，保存前重验远端大小 fingerprint。 |
| WR3 | completed | UI 操作入口、拖拽意图、默认关闭的监控/自动上传说明 | Cargo check + user GUI acceptance | 当前 Slint 提供路径意图；自动上传/监控保持关闭。 |
| WR4 | completed | 远端变更监控、显式自动上传、编辑同步提示和拖拽 intent | focused state/worker tests + full gate | 自动上传默认关闭；真实 SFTP 与 GUI 仍需用户验收。 |
| WR5 | completed | 双语契约、回归测试和完整门禁 | fmt/check/clippy/test/tracker/diff | 已恢复下载后打开，保存成功同步 fingerprint，Save As 区分新路径，旧监控按代次退出，拖入路径完成 URI/长度/控制字符校验。 |
| RE1 | completed | 有界重连策略、退避与代次取消状态 | state/connection 定向测试 | 默认最多 5 次，1/2/4/8/16 秒退避，上限 30 秒；主动断开、关闭 Tab 和新 attempt 使旧任务失效。 |
| RE2 | completed | SSH/Telnet/Serial worker 重建与安全认证边界 | connection monitor/authentication 定向测试 | SSH 只复用已验证 host key 和可用私钥/Agent/系统凭据；无密码材料转人工提示。 |
| RE3 | completed | UI 状态提示、终端内容保留与恢复动作 | cargo check + Slint mapping tests | 断线保留 Tab/scrollback，状态文本提供倒计时、重连中、成功及最终失败提示。 |
| RE4 | completed | 双语契约、回归测试和完整仓库门禁 | fmt/check/clippy/test/translations/tracker/diff | 真实网络与目标平台 GUI 断线场景留人工验收。 |
| WS1 | completed | 版本化 workspace snapshot DTO、私有原子存储与校验 | focused config/pane tests | 不保存 worker、连接句柄、凭据或 host-key 临时状态。 |
| WS2 | completed | AppState/PaneTree/WindowRouter 导出与恢复 | state/router tests | 限制 pane 数、路径/终端文本长度和总快照大小。 |
| WS3 | completed | 启动恢复 Tab/连接与退出原子保存 | cargo check + lifecycle tests | 恢复连接重新经过正常认证和 host-key 流程。 |
| WS4 | completed | 双语契约、项目地图和完整门禁 | fmt/check/clippy/test/tracker/diff | GUI 和真实网络行为由用户在目标平台验收。 |
| MP1 | completed | TerminalModel mouse modes、事件 DTO 和 SGR/UTF-8/legacy 编码 | focused terminal tests | 只发送有界坐标/按钮/修饰键；模式由解析后的终端状态决定。 |
| MP2 | completed | Slint pointer routing、selection/scroll fallback 与 worker send | cargo check + UI mapping tests | reporting 开启时 pointer 不被本地选择吞掉；关闭时保持既有行为。 |
| MP3 | completed | 双语契约、tracker/project-map 与完整门禁 | fmt/check/clippy/test/translations/tracker/diff | TUI 真实交互仍需用户在目标平台验收。 |
| KH1 | completed | OpenSSH known_hosts 解析、主机匹配、hashed host 与 @revoked 判定 | `ssh::known_hosts` 定向回归 | malformed/读取失败只收窄信任，不放宽。 |
| KH2 | completed | 将共享 known_hosts 快照接入 probe、连接和 host-key 错误状态 | SSH trust/connection tests | profile 指纹与系统记录冲突时拒绝。 |
| KH3 | completed | 撤销/替换记录的显式、原子、保留其他行的管理能力 | known_hosts file update tests | 不提供普通确认绕过 revoked。 |
| KH4 | completed | 双语契约、项目地图和完整仓库门禁 | fmt/check/clippy/test/translations/tracker/diff | 真实系统文件权限与 GUI 需用户验收。 |

## 已完成

- 已完成施工前环境预检：项目保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与锁定离线 Cargo 门禁；本机 `cargo fmt` 和 `cargo clippy` 可用。
- 已确认项目地图已覆盖 SFTP transfer、worker、application bridge 和 Slint 组合路径；本轮只需在收口时更新其传输语义摘要。
- 已确认既有能力仅为 private-cache 的单文件 download-to-open，取消会删除未发布的部分文件；它不提供暂停、续传或目录递归。
- 已完成 TR1-TR3：AppState 以有界活动/失败/成功传输行和勾选状态表达操作权限；worker 递归展开远端目录，并在存活期间暂停/从当前 offset 继续；Slint 只呈现三页与批量意图。
- 已完成本地目标与清理收紧：下载保留当前 Local files 目录中的相对树，以安全 `.part` 文件 fsync 后无覆盖原子发布；递归扫描限制为 4,096 个条目；失败、取消及发布后观察到的取消都会由 worker 清理其任务创建的内容。
- 已完成 worker 写命令、上传临时发布、删除、重命名、有界 UTF-8 编辑/Save As、远端大小冲突拒绝、Local files 单文件上传和 UI 操作入口；中英文使用/架构契约已同步。
- WR4 已完成有界远端 fingerprint 监控、默认关闭的自动上传开关、编辑同步提示和基于现有选中项的拖拽 intent；WR5 已修复新增路径与既有下载后打开能力之间的回归边界，不在 UI 线程执行文件 I/O。

## 验证

- 已完成：workspace DTO/私有原子存储、Tab/终端文本/SFTP 路径恢复、PaneTree 校验与恢复、启动重建 worker、退出原子保存；`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 159、应用 160、Doc tests 0）、严格 Clippy、翻译检查、tracker validator 和 `git diff --check`。
- 未完成：目标平台 TUI 鼠标点击、滚轮、拖动和 focus 行为手工验收；不能在当前自动化环境替代用户视觉验收。

## 风险与阻塞

- 自动重连只在当前进程内有效，最多 5 次；跨进程重连、密码明文缓存和未知/变更 host key 自动接受均明确不支持。
- SFTP v3 没有持久化任务恢复协议；本轮的断点继续限定为同一运行期的暂停后继续，partial 文件只属于仍存活的 worker-owned 传输。
- 递归目录下载必须限制深度、文件数、路径文本和总字节，且跳过符号链接；超限或不可读项将以有界失败行呈现。

## 下一步

- 用户在目标平台确认系统 known_hosts 路径、权限及 revoked/changed 对话框行为。

## 最后更新时间

- 2026-08-13：完成 MP1-MP3 全屏终端 mouse reporting 实施；不联网、不使用多 agent。工作区恢复、SFTP WR1-WR5 和 RE1-RE4 保持完成。真实 TUI 交互仍需用户验收。
- 2026-08-13：启动 KH1-KH4 known_hosts 兼容实施；复用锁定的 `russh`/`ssh-key`，不联网、不使用多 agent。
- 2026-08-13：完成 KH1-KH4；系统 known_hosts 读取、共享追加、撤销拒绝、changed/unknown 确认、changed 替换和原子移除能力接入并通过定向/全量测试。补充非默认端口不误匹配 plain host、`@cert-authority` 不作普通 host key 信任以及保留无关/撤销行的回归。
