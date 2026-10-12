[English](coverage.md) · [文档导航](README.zh.md)

# 行为覆盖率

覆盖率使用与 CI 相同的 LCOV 命令生成；前后比较必须保持平台、Rust 工具链与测试参数一致。
macOS 与 Linux Codecov job 编译的原生代码不同，本机结果不能代替 Codecov 结果。
报告保留全部业务源码，包括仍需人工验证的启动和原生适配代码。

```bash
cargo install cargo-llvm-cov --locked
rustup component add llvm-tools-preview
cargo llvm-cov --locked --offline --workspace --lcov --output-path target/coverage/after.lcov
cargo llvm-cov report --html --output-dir target/coverage
```

LLVM tools 必须与当前工具链匹配。冷缓存下 Skia build script 仍可能自行下载原生归档，
不受 Cargo offline 参数控制；必要时使用与锁定版本、架构和特性 key 完全匹配的完整本机归档。
报告格式与 profile 清理见 [cargo-llvm-cov 官方说明](https://github.com/taiki-e/cargo-llvm-cov)。
最终测量不能混入旧基线的 profile。

## 测试边界

| 测试 | 驱动的实际实现 | 隔离边界与断言 |
| --- | --- | --- |
| `src/app/behavior_tests.rs` | 生成的 Slint callback、应用控制器、model 和真实 Telnet worker | 子进程中的无窗口 event-loop proxy、loopback TCP；按键/粘贴/焦点字节、resize、模态/身份闸门、断开与 Tab 保留 |
| `src/app/connection_monitor/behavior_tests.rs` | SSH event monitor 与 AppState 状态转换 | 有界事件流；输出解析、延迟、安全重试、SFTP 状态与旧 attempt 拒绝 |
| `src/app/connection/direct/behavior_tests.rs` | Telnet/Serial 共用 event monitor | 实际事件适配；输出、失败、重连调度、EOF 和替换 attempt |
| `src/ssh/worker/behavior_tests.rs` | 实际 SSH Handler 与 russh channel 上的 SFTP worker | 确定性公钥/显式信任快照、内存协议 peer；下载/上传字节、冲突决策、编辑保护、取消清理与子系统失败 |
| `src/serial/worker_tests.rs` | 实际有界串口 worker | 注入端口获取与 duplex/错误 stream；粘贴完整性、接收时间、EOF、背压与阻塞写入关闭 |
| `src/audio/worker_tests.rs` | 实际采集控制器、blocking worker 与 PCM 转换 | 注入权限/设备源；启动结果、错误、溢出、取消和设备关闭前仍持有租约 |
| `src/credentials/behavior_tests.rs` | CredentialStore 和加密 vault | 逐实例内存 keyring 与临时目录；账号隔离、错误上下文、备份/恢复、密文绑定与损坏记录；不访问真实系统凭据 |
| `src/logging/behavior_tests.rs` | 全局日志初始化、flush guard 与 panic hook | 子进程和私有临时目录；持久崩溃报告、转义与长度边界 |

注入接口保持私有，生产构造器仍使用原生设备与系统 keyring。SSH 测试保留
`check_server_key`，未知、不匹配、变化和撤销密钥继续拒绝。新增测试中的密码只保留在内存或密文 vault 记录。

## 测量记录

2026-10-12 macOS ARM64 同平台对比使用 Rust 1.97.1、cargo-llvm-cov 0.9.1。
基线默认并发通过 770 项；最终普通与插桩运行均以默认并发通过 799 项，新增 29 项行为测试。

| 范围 | 修改前 | 修改后 |
| --- | --- | --- |
| 归一化 `src/` 总计 | 57.96%（26,723/46,107） | 65.78%（30,381/46,185） |
| `src/app/` | 49.05% | 57.04% |
| `src/app.rs` | 20.27% | 45.16% |
| `src/ssh/` | 49.98% | 64.78% |
| `src/ssh.rs` | 67.36% | 74.35% |
| `src/serial.rs` | 32.09% | 73.61% |
| `src/audio.rs` | 42.42% | 57.11% |
| `src/credentials.rs` | 73.63% | 88.03% |
| `src/logging.rs` | 59.52% | 91.55% |

分组前先归一化 LLVM 源路径别名：`src/config/../../build/languages.rs` 属于 `build/`，
不进入 `src/` 小计。与原始 Git 版本中的完全相同业务行对照、剔除 inline test module 后，
新增命中 3,580 行既有代码；这个保守计数不包含移动/新增实现行和新增 fixture 文件。

报告位于 ignored `target/coverage/`：`baseline.lcov`、`after.lcov`、`comparison.json`、
`after-summary.json` 和 `html/index.html`；最终验证记录在[实施跟踪](project-implementation-tracker/current.md)。
新增 fixture 也增加可执行行数，因此同时关注比例与新命中的既有源码行。
原生 GUI、硬件、系统凭据后端和真实服务器路径仍按各自人工验证边界验收。
