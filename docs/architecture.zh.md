[English](architecture.md)

# AxSSH 架构说明

## 边界

AxSSH 是一个独立的 Rust 二进制项目。`third_package/axshell` 仅用于参考，
故意排除在构建图之外；可以参考它的产品行为和评审问题，但不得导入其中的
源码、类型或依赖。

当前实现将 UI、应用、持久化、传输和进程服务拆成独立所有权边界：

```text
Slint UI（.slint）
       │ 生成的 callback / property
       ▼
应用控制器（src/app.rs）
       │ 领域值 + UI event loop 调度
       ├──────────────► 配置存储（src/config.rs）
       │                 JSON schema + 原子替换
       └──────────────► SSH 边界（src/ssh.rs）
                         Tokio task + russh handle/channel

进程启动（src/main.rs）
       └──────────────► 日志生命周期（src/logging.rs）
                         滚动 writer + flush guard
```

## 模块职责

| 区域 | 负责 | 不得负责 |
| --- | --- | --- |
| `ui/` | 布局、视觉状态、用户手势、生成的 callback 契约 | 文件系统、Tokio task、russh handle |
| `src/app.rs` | Slint 初始化、领域值到行模型的转换、callback 接线、event loop 更新 | SSH 协议细节或 JSON schema 细节 |
| `src/config.rs` | `SessionProfile`、校验、JSON 持久化、原子替换 | Slint 类型、网络连接、明文密码存储 |
| `src/ssh.rs` | russh handler、主机密钥决策、认证、shell channel 边界 | 窗口更新、持久化会话修改、UI 格式化 |
| `src/logging.rs` | 全局 tracing subscriber、日志目录、按日滚动、保留和 flush guard | 凭据、功能状态、UI 或 SSH handle |
| `src/main.rs` | 进程启动和日志 guard 生命周期 | 功能逻辑 |

## 事件流

1. Slint callback 只产生会话 ID、草稿字段、信任决策或一次性临时密码等小值。
2. 应用控制器校验并转换领域值。未知主机会启动可取消探测：记录 SHA-256 指纹，
   但传输仍保持拒绝。
3. 用户明确确认后，控制器才原子持久化精确指纹并打开密码弹窗。密码直接移动到
   一次 worker 命令，不进入应用状态或配置。
4. 短暂配置读写可以同步执行；SSH 连接、认证、健康检查和断开都放到 Tokio。
5. worker 通过有界自有值返回结果；UI 更新统一使用
   `slint::invoke_from_event_loop`，并使用 `Weak<AppWindow>`，避免退出时保活窗口。

## SSH 安全契约

`russh::client::Handler::check_server_key` 是信任边界。未知和不匹配的主机密钥都在
认证前拒绝。首次拒绝握手可以把 SHA-256 指纹交给确认 UI，但只有用户明确决定后，
该精确指纹才进入 profile；密钥变化需要再次明确确认。密码只作为 callback 的临时
输入，不进入 `SessionStore`；私钥加载和系统钥匙串接入留作后续工作。

认证后连接遵循以下生命周期：

- 一个 worker 在完整生命周期内独占 russh handle；
- 当前有界命令 channel 传递断开/取消意图；
- 有界 worker 事件只报告 connected、disconnected、host-key rejection 或截断后的错误；
- 取消既能中断连接/认证，也能断开已建立会话；
- 20 秒 keepalive 和三次未响应上限保持健康空闲会话，同时保留 90 秒 inactivity 边界；
- 窗口退出先请求断开，在超时边界内等待 worker join，最后再关闭 Tokio。

## 日志生命周期

`src/main.rs` 在创建 UI 前建立唯一的 `LoggingGuard`，并保持到 Slint 与 Tokio 生命周期
结束之后。`src/logging.rs` 通过有界无损队列写入按 UTC 日期滚动的文件，最多保留
15 个，同时把 `INFO` 及以上事件镜像到 stderr。guard 释放时先写退出事件，再排空
队列、刷新当前文件并 join writer 线程。运行字段可以包含 session ID、host、port 和
主机指纹；禁止记录凭据和终端内容。

## 分阶段范围

当前应用可校验并持久化 profile、确认逐 profile 主机指纹、使用临时密码认证，
并持续持有一个连接直到断开。以下内容仍作为独立步骤：

- 私钥加载和系统凭据集成；
- 共享的 OpenSSH 兼容 known_hosts 存储和主机密钥撤销；
- VT/ANSI 终端模型和有界 scrollback；
- 与认证策略共享的独立 SFTP worker；
- shell channel 命令、resize、重连和多会话生命周期测试。
