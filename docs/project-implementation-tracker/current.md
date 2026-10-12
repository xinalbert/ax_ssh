# 当前项目实施记录

## 当前目标

- 目标 ID：20261012-behavior-coverage
- 目标：针对用户提供的 Codecov 低覆盖报告，补齐实际应用控制器和 transport worker 的行为测试，测量同平台前后覆盖率。
- 交付物：可重复的覆盖率基线与最终报告；控制器事件、worker I/O/取消/错误行为回归；必要的私有边界注入；双语开发说明和实施记录；分项提交并推送 origin/master。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/`、`src/ssh/` 及覆盖报告选定的 `src/{serial,audio,credentials,logging}.rs` 薄弱路径、相关测试与开发文档；本机 coverage 工具和报告输出位于 `target/`。
- 不在本轮范围内：UI 视觉改版、依赖/工具链升级、真实设备和系统凭据访问、真实远端会话、release/tag、无关历史文档整修。
- 所有权：AppState/控制器拥有状态和事件消费；transport worker 独占连接、I/O 和取消；仅文件、设备、凭据等边界允许注入，不复制控制器实现。
- 安全：未知/不匹配/revoked 主机密钥继续拒绝；凭据测试使用隔离实现和临时目录，不访问真实 keyring，不持久化明文秘密；有界通道和 shutdown/join 契约保持。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| COV1 | completed | 环境预检、工具及 target/coverage/baseline.lcov | 原生基线默认并发 770 项通过；locked/offline check 通过 | cargo-llvm-cov 0.9.1；LLVM 归档与已安装工具链 manifest SHA-256 一致；使用匹配 Skia 本机归档 |
| COV2 | completed | 实际控制器、monitor 与 loopback worker 回归 | SFTP/Handler/monitor 与全部 callback 专项通过，含精确 socket 字节与正常关闭 | 最后一项修正 fixture 的原生修饰键快照注入，保持真实输入归一化和物理键盘隔离 |
| COV3 | completed | 音频/串口实际 worker、隔离凭据与日志回归 | 默认并发库测试中全部边界回归通过；无真实设备/keyring | 私有设备/权限/entry factory；日志及 Slint process-global 状态在子进程隔离 |
| COV4 | completed | 最终 LCOV/JSON/HTML、源码行对照与完整门禁 | 799 默认并发普通/插桩 test、fmt/check/严格 Clippy、原生 ARM64 CI 同款 check/Clippy、文档/skill/tracker/diff 全通过 | canonical src 总覆盖 57.96% -> 65.78%，新增命中 3,580 行既有业务代码；线上 Linux 仍待 CI |
| COV5 | completed | 控制器/SSH、隔离边界测试、双语文档与记录分项提交 | 代码 3c5586e/cea1a99、双语说明 b5d1fc1；审计/实施记录随本页提交 | 每组显式暂存并审查 index；生成报告和缓存不入 Git |
| COV6 | completed | origin/master 推送与远端 HEAD 核对 | 前三项提交已推送，代理 ls-remote 与本地 b5d1fc1 完整 SHA 一致 | 记录提交随后同步并核对；直连核对出现 HTTP2 错误时使用命令级 7897 代理 |

## 已完成

- 已读取三项指定技能、地图、当前环境和架构；工作区起始干净。
- 核对 Rust 2024/MSRV 1.92.0、Slint 1.18.1、russh 0.64.1、Cargo.lock、build.rs 与 CI；本机 Rust/Cargo 1.97.1、Python 3.10.20，两个 macOS target 可用。
- 用户明确本轮目标为处理 Codecov 低覆盖；前轮多语言交付记录保留在月度历史，母语/GUI 验收仍由用户进行。
- 原生基线默认并发 770 项通过，报告 `target/coverage/baseline.lcov`；cargo-llvm-cov 0.9.1/LLVM tools 已补齐，Skia 使用已核验版本/架构/特性的本机归档。
- 新增控制器 callback、SSH/直连 monitor、实际 SFTP worker 与 Handler、音频/串口 worker、内存凭据和日志生命周期回归；注入保持私有，public API 和依赖未改变。
- 刷新地图，新增双语 `docs/coverage*.md`、同步开发/文档索引；删除开发文档中已不存在的真实 keyring ignored test 说明。
- 最终新增 29 项行为测试；386 library + 413 binary（799）在普通和 LLVM 插桩模式都默认并发通过；完整 callback 到实际 worker 验证精确按键/粘贴/焦点/Control-C 字节、resize、显式断开与 Tab 保留。
- canonical src 覆盖率 26,723/46,107（57.96%）提升到 30,381/46,185（65.78%）；归一化 LLVM 路径别名不混入 build registry，未调整报告的业务源码排除规则。新命中 3,580 行与原始 Git 完全相同的 runtime 代码，排除 inline tests 与所有新 fixture。
- 报告位于 `target/coverage/{baseline.lcov,after.lcov,comparison.json,after-summary.json}` 和 `target/coverage/html/index.html`；双语 coverage 页面保留测量口径、逐模块对比和可复现命令。
- 按用户授权将控制器/SSH、隔离边界测试与双语说明分项提交为 3c5586e、cea1a99、b5d1fc1；已推送 origin/master 并核对远端完整 SHA。环境审计与实施记录随包含本页的提交同步；未纳入生成报告或缓存，保持追加历史。

## 验证

- 已完成：原生 770 基线及最终普通/插桩 799 默认并发全量、专项、fmt、locked/offline check、严格 all-target Clippy；CI 同款 ARM64 target check/严格 all-target Clippy；LCOV/JSON/HTML、源码对照、相对链接、skill/tracker/diff。
- 未完成：Windows/Linux/Intel 目标本轮验证与线上 Codecov 上传；真实 GUI、系统 keyring、麦克风/串口和真实服务器人工验收。本轮未启动/截图 GUI，不发布。

## 风险与阻塞

- 用户表格来自 Linux Codecov，本机是 macOS；平台专属代码与分母不同，必须分别标记，不能将本机数字当作 Codecov 已更新。
- 测量工具已齐备，原生基线和本轮库全量验证未复现旧音频 helper 超时；不以串行运行代替默认并发。
- Windows/Linux 原生 SDK 不可用；GUI 与真实 SSH/设备手动验收不由本轮自动测试代替。
- 仍有较大 UI/native 路径未覆盖，尤其启动与原生窗口/设备适配；本轮提升测试范围而非宣称全部路径覆盖。Intel target 虽已安装，本轮只执行 native ARM64 验证。
- tracker 全局校验当前通过；保留历史，不改写旧记录。

## 下一步

- 由原生 CI 重测 Linux Codecov；后续可继续对 SFTP/terminal bridge 剩余交互及平台适配补测，保留实际 controller/worker 和隔离边界约束。

## 最后更新时间

- 2026-10-13 00:07 +0800
