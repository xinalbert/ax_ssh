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
       │ Tab ID + 领域值 + UI event loop 调度
       ├──────────────► 配置存储（src/config.rs）
       │                 版本化设置/profile JSON + 原子替换
       ├──────────────► 系统凭据（src/credentials.rs）
       │                 阻塞式平台 keyring API
       ├──────────────► 终端模型（src/terminal.rs）
       │                 有界 ANSI 状态 + scrollback
       └──────────────► SSH 边界（src/ssh.rs）
                         Tokio task + russh handle/channel + 私钥加载

进程启动（src/main.rs）
       └──────────────► 日志生命周期（src/logging.rs）
                         滚动 writer + flush guard
```

## 模块职责

| 区域 | 负责 | 不得负责 |
| --- | --- | --- |
| `ui/` | 顶部 Tab、页面布局、视觉状态、用户手势、生成的 callback 契约 | 文件系统、Tokio task、russh handle |
| `src/app.rs` | Slint 初始化、领域值到行模型的转换、callback 接线、event loop 更新 | SSH 协议细节或 JSON schema 细节 |
| `src/app/` | 与 UI 无关的工作区 Tab、逐 Tab 终端/worker 状态、attempt 转换、分组聚合和阻塞式凭据 task 边界 | 生成的 Slint component/model 类型 |
| `src/config.rs` | `SessionProfile`、版本化 `AppSettings`、校验、旧配置迁移、JSON 持久化和原子替换 | Slint 类型、网络连接、明文密码存储 |
| `src/credentials.rs` | 按 profile 访问平台系统凭据库 | UI 状态、明文配置、SSH 传输 handle |
| `src/terminal.rs` 与 `src/terminal/input.rs` | 有界 ANSI 解析、光标状态、文本 scrollback 和终端按键编码 | Slint 类型、网络 handle、凭据 |
| `src/ssh.rs` | russh handler、主机密钥决策、认证、shell channel 边界 | 窗口更新、持久化会话修改、UI 格式化 |
| `src/ssh/private_keys.rs` | 本机 `.ssh` 私钥发现和阻塞式密钥加载 | passphrase 持久化、UI 状态、主机信任决策 |
| `src/ssh/worker.rs` | 有界 shell 输入命令、合并式 resize 状态、批量输出事件、取消和关闭 | UI 状态或 profile 持久化 |
| `src/logging.rs` | 全局 tracing subscriber、日志目录、按日滚动、保留和 flush guard | 凭据、功能状态、UI 或 SSH handle |
| `src/main.rs` | 进程启动和日志 guard 生命周期 | 功能逻辑 |

## 事件流

1. Slint callback 只产生已保存 profile ID、唯一 Tab ID、组名、终端按键/修饰键、
   草稿字段、信任决策或一次性临时密码等小值。
2. 每次打开 profile 都会创建新的终端 Tab UUID，即使另一个 Tab 使用同一 profile。
   应用控制器按 `tab_id + attempt_id` 路由输入、resize、输出、重试和关闭。未知主机
   会启动绑定该 Tab 的可取消探测，但传输仍保持拒绝。
3. 用户明确确认后，控制器才原子持久化精确指纹。密码 profile 通过 Tokio blocking
   边界读取已记住的凭据或打开密码弹窗；私钥 profile 在 UI 线程外加载所选路径，
   只有加密密钥无法空口令打开时才请求一次性 passphrase。
4. 新建 profile 时明确选择保存的密码会随该 profile 操作写入系统凭据库；在认证
   弹窗输入的密码只在 SSH 认证成功后写入。已存凭据缺失或被拒绝时清除非敏感
   标记，并回退到一次手工密码提示。
5. 终端表面把 Slint 特殊键转换成与 UI 无关的终端键值；`src/terminal/input.rs`
   生成控制字节、常规 CSI 和带修饰键的 xterm 序列。选区复制留在 UI，本地粘贴
   内容作为有界 shell 输入发送。
6. 认证后每个终端 Tab 持有一个 worker，该 worker 独占一个 PTY shell 及其 russh
   handle/channel。同 profile 的重复 Tab 使用彼此独立的有界命令队列和单槽尺寸状态。
   关闭 Tab 时先移除事件路由，再异步 shutdown 对应 worker，迟到事件不会更新其他 Tab。
7. 每个终端 Tab 还持有一个有界 `TerminalModel`。非活动 Tab 的输出留在 Rust 状态，
   只有活动 Tab 快照进入 Slint event loop；更新统一使用 `slint::invoke_from_event_loop`
   和 `Weak<AppWindow>`，避免退出时保活窗口。

## SSH 安全契约

`russh::client::Handler::check_server_key` 是信任边界。未知和不匹配的主机密钥都在
认证前拒绝。首次拒绝握手可以把 SHA-256 指纹交给确认 UI，但只有用户明确决定后，
该精确指纹才进入 profile；密钥变化需要再次明确确认。密码只作为 callback 的临时
输入，不进入 `SessionStore`。profile 只包含 `credential_stored` 标记；密码本身以稳定
profile UUID 为键存入平台系统凭据库。私钥 profile 只持久化路径；私钥内容和可选
passphrase 只在一次 blocking 加载/认证任务中短暂存在，不进入配置、tracing 字段或
UI model。

认证后连接遵循以下生命周期：

- 每个终端 Tab 有唯一运行时 UUID，并由一个 worker 在完整生命周期内独占 russh handle；
- 有界命令 channel 传递 shell 输入、断开和取消意图；watched terminal size 合并
  高频 resize 更新；
- 终端输出按批次限制大小，并通过有界事件 channel 反压后进入有界终端模型；
- worker 事件报告 connected、resize、output、disconnected、host-key rejection、
  凭据失败或截断后的错误；
- 取消既能中断连接/认证，也能断开已建立会话；
- 20 秒 keepalive 和三次未响应上限保持健康空闲会话，同时保留 90 秒 inactivity 边界；
- 关闭 Tab 先使 Tab/attempt 路由失效，再请求 worker shutdown；
- 窗口退出对所有剩余 worker 请求断开，在超时边界内逐个等待 join，最后再关闭 Tokio。

## 日志生命周期

`src/main.rs` 在创建 UI 前建立唯一的 `LoggingGuard`，并保持到 Slint 与 Tokio 生命周期
结束之后。`src/logging.rs` 通过有界无损队列写入按 UTC 日期滚动的文件，最多保留
15 个，同时把 `INFO` 及以上事件镜像到 stderr。guard 释放时先写退出事件，再排空
队列、刷新当前文件并 join writer 线程。运行字段可以包含 session ID、host、port 和
主机指纹；禁止记录凭据和终端内容。

## 持久化设置与字体资源

`assets/fonts/JetBrainsMono-Regular.ttf` 是由 Slint 编译器注册的项目自有静态资源，
同目录保留 OFL 许可证和作者声明。构建和运行时都不会从 `third_package/axshell`
加载字体。`SessionStore` 在现有私有 `sessions.json` 中写入版本化 `settings` 对象，
包括经过约束的终端字体/字号、scrollback、默认 PTY 尺寸、侧栏宽度和 Tab 宽度。
旧版顶层 `appearance` 会在反序列化时迁移。密码、passphrase、私钥内容、终端输出、
Tab 运行时 ID 和 worker 永远不会序列化。

## 分阶段范围

当前应用可校验并持久化 profile、确认逐 profile 主机指纹、使用临时密码或本机
私钥认证，并持有多个逐 Tab 隔离的交互式 PTY shell，同一 profile 也可重复打开。
Settings 和新建会话编辑器属于工作区 Tab；只有短期信任和 secret 提示保留为覆盖层。
以下内容仍作为独立步骤：

- 共享的 OpenSSH 兼容 known_hosts 存储和主机密钥撤销；
- 与认证策略共享的独立 SFTP worker；
- SSH agent、重连和工作区恢复；
- 更完整的全屏终端兼容、颜色/属性渲染、应用光标模式和鼠标上报。
