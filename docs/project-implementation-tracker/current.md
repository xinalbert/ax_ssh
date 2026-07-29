# 当前项目实施记录

## 当前目标

- 目标 ID：20260729-ssh-login-log-lifecycle
- 目标：修复当前没有可用登录路径的问题，并建立进程级日志初始化、滚动、保留、刷新和退出生命周期。
- 交付物：主机密钥确认、临时密码认证、持续连接/断开 worker、全局文件日志守卫、配套测试和同步后的双语文档。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`src/`、`ui/`、双语架构/开发文档和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：终端模拟、SFTP、私钥认证、无确认接受未知主机密钥、持久化密码、复制或引用 `third_package/axshell` 源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 登录阻断根因和本地 API/依赖基线 | 源码路径、锁文件和本机缓存核对 | russh 0.62.2、Slint 1.17.1、tracing-appender 0.2.5 API 已确认 |
| P2 | completed | 主机指纹确认与临时密码 UI 流程 | Slint/Rust 类型检查和策略单元测试 | 未知/变更主机密钥均在认证前拒绝 |
| P3 | completed | 有界命令通道与可取消 SSH worker | loopback 登录、断开和 join 测试 | worker 独占 russh handle |
| P4 | completed | 进程级滚动文件日志生命周期 | 私有日志目录和退出 flush 测试 | 密码和终端内容不得进入日志 |
| P5 | completed | 双语文档、实施记录与最终回归 | 全部可用仓库门禁和 tracker validator | 主窗口及真实 SSH 登录已有运行证据 |

## 已完成

- 已确认连接按钮当前固定向密码认证传入 `None`，没有认证成功路径。
- 已确认新建 profile 的主机指纹为 `None`，安全 handler 会在认证前拒绝所有未知主机。
- 已确认即使认证成功，当前 callback 也会立即丢弃 `SshConnection`，没有持续连接所有者或显式关闭路径。
- 已确认 `main` 仅安装控制台 tracing subscriber，没有文件日志、滚动保留或退出 flush guard。
- 已确认 `tracing-appender 0.2.5` 已在本机 Cargo 缓存，可保持 locked/offline 验证。
- 已确定 app 只持有 SSH worker 控制器；有界事件 receiver 不进入 Slint，russh handle 始终由 `src/ssh.rs` 的 worker 独占。
- 已确定未知主机先执行拒绝式指纹探测，变更主机在认证前返回独立事件；二者都必须经 UI 明确确认后才能更新 profile。
- 已实现主机指纹确认、临时密码弹窗和 profile 原子更新；密码不进入 `AppState`、配置或 tracing 字段。
- 已实现有界命令/事件 channel；取消可中断探测、连接和认证，认证后的 russh handle 由 worker 持有到显式断开或窗口关闭。
- 已根据真实运行日志加入 20 秒 keepalive、三次未响应上限和 90 秒 inactivity 边界，避免健康空闲连接在原 60 秒超时后自动关闭。
- 已实现 `LoggingGuard`：每日 UTC 滚动、最多 15 个文件、1024 行有界无损队列、stderr 镜像和退出 flush/join。
- 已增加 loopback russh 回归，真实执行未知密钥拒绝/探测、精确指纹信任、密码认证、worker 断开与 join。

## 验证

- 已完成：`rustfmt --edition 2024 --check`；`cargo check --locked --offline`；`cargo test --locked --offline`（9 passed）；Slint 重新编译；Cargo metadata/tree 边界审阅；18 个 Markdown 文件相对链接检查；tracking validator；`git diff --check` 和未跟踪文本空白/冲突标记扫描；macOS 900x560 主窗口渲染截图；运行日志记录真实局域网 SSH 密码认证成功、worker connected 和远端关闭；日志文件实际落盘。
- 未完成：本机没有 `cargo-clippy`/`clippy-driver`，无法运行 Clippy；主机密钥和密码弹窗的键盘/焦点仍需完整手工走查；Windows/Linux GUI 和真实 SSH 平台验收留给对应环境。

## 风险与阻塞

- 未知主机必须先展示 SHA-256 指纹并由用户明确确认，不能为了恢复登录而改成全量接受。
- 密码必须只作为短生命周期 worker 输入；任何错误、日志、配置和 tracker 都不得包含密码。
- 当前产品尚无终端模拟；本轮只保证认证后的连接由 worker 持有并可显式断开，不把原始终端输出推向 UI。
- 本机 Cargo 未安装 `cargo-fmt`；需用可用的 `rustfmt` 替代格式门禁并如实记录。Clippy wrapper 待单独确认。
- 本机锁文件与依赖缓存足够，最终构建和测试保持 `--locked --offline`，不需要联网解析依赖。

## 下一步

- 下一阶段接入 shell channel、VT/ANSI 终端模型和有界 scrollback，并复用当前 worker、trust 和日志生命周期。

## 最后更新时间

- 2026-07-29 10:13 +0800
