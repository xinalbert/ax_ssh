# 当前项目实施记录

## 当前目标

- 目标 ID：20260930-sftp-reconnect-liveness
- 目标：修复独立 SFTP 连接空闲断线未被发现、浏览请求遭遇传输错误后不重连，以及重连后远端目录丢失的问题。
- 交付物：worker/浏览器断线处理、就绪事件时序、运行时目录恢复、确定性回归及双语架构说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/{ssh,sftp}.rs`、`src/ssh/worker/sftp.rs`、`src/app/{connection,connection_monitor,state}.rs` 及对应状态文件、测试和双语架构文档。
- 不在本轮范围内：`third_package/axshell`、SSH 主机密钥与凭据策略、配置 schema、传输任务自动续传、已安装应用替换和 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SFTPRE1 | completed | SSH/SFTP 空闲断线发现与可恢复故障分类 | 模拟静默断线和请求故障测试 | 权限/路径错误不应触发重连 |
| SFTPRE2 | completed | 首次目录就绪才报告 connected，重连保留远端目录 | worker/state 定向回归 | 仅保存有界运行时路径，不自动重放传输 |
| SFTPRE3 | completed | 双语架构说明与完整门禁 | fmt/check/Clippy/test/diff/tracker | 真正网络与 GUI 生命周期由用户验收 |

## 已完成

- 只读诊断确认：russh 已设置 20 秒 keepalive/三次未响应及 90 秒 inactivity；SFTP-only worker 未观察已关闭的 SSH handle，浏览器在无 UI 命令时不观察 channel；请求失败仅显示错误后继续等待。
- 原有重连最多五次并重新认证、核验 host key；原先断线清理会丢失当前远端路径和队列，本轮只恢复路径，不允许未经核验自动重放传输。
- 已实现 SFTP-only worker 每秒观察 SSH handle 关闭；浏览器遇协议/网络故障终止，目录状态错误继续留在当前连接。
- 已将 connected 上报移至首次目录页之后；Tab 内路径在运行时重连保留，避免重复清理前丢失路径；同步中英文架构与使用说明。
- 复核补充：浏览器也通过受观察的 SFTP 字节流接收子通道 EOF/错误通知，不依赖下一次 UI 命令；SSH 认证成功事件与目录就绪事件分离，保持原有凭据保存入口。

## 验证

- 已完成：改动前 `cargo test --locked --offline reconnect`（5 项）及 `cargo test --locked --offline sftp`（90 项）通过，工作树干净。
- 已完成：修复后 `cargo test --locked --offline sftp`（库 55、应用 39 项）以及模拟 SFTP 通道关闭回归通过。
- 已完成：最终版 `cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 297、应用 292 项）及 `git diff --check` 通过；静默 EOF 定向测试通过。tracker validator 仅报既有 8/9 月历史与 research 的 54 项格式错误，本轮条目无新增错误。
- 未完成：真实 SFTP 服务器断网/子通道关闭和 GUI 行为验收（需用户在目标平台操作）。

## 风险与阻塞

- SFTP 子系统关闭可能先于 SSH transport 关闭；浏览器需只将协议/网络故障视为重连原因，不能把目录无权限误判为断线。
- 重连不得跳过 host-key 校验或重复上传；活动传输依原有取消和清理路径结束。
- tracker 全局 validator 受既有 54 项历史格式错误阻断；本轮新记录与当前状态未出现在错误列表。

## 下一步

- 用户在新构建上确认 SFTP 长时间空闲后的断线提示、自动重连、远端目录恢复及未完成传输状态；本轮不替换已安装应用。

## 最后更新时间

- 2026-09-30 18:59 +0800
