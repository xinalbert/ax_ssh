# 当前项目实施记录

## 当前目标

- 目标 ID：20261011-sftp-edit-limit-setting
- 目标：将远程外部编辑的文件大小上限纳入 Settings > SFTP，并统一下载/本地副本/保存回传检查。
- 交付物：持久化配置、设置控件与实时映射、按操作传递的大小上限、旧配置兼容、回归与双语文档。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：config/settings、Settings Slint/bridge/view、外部编辑应用和 SFTP 字节操作、测试、翻译与双语文档；复核既有 SFTP 性能与外部编辑改动并创建本地提交。
- 不在本轮范围内：依赖/工具链升级、流式超大文件编辑、内置编辑器恢复、推送/发布；既有性能和外部编辑实现保留。
- 所有权：配置层持久化并规范化 1–64 MiB（默认 4）；应用层为操作取得当前设置并限制本地读取；SFTP 层在请求边界再次校验；Slint 只传递数值意图。
- 安全边界：文件缓冲、监听和队列保持有界；拒绝链接、原子回传与 SSH host-key/短期凭据规则保持。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| EL1 | completed | schema 默认/规范化及 Settings 持久化链 | 旧配置默认、保存重读、上下界与 Slint 重编 | 默认 4 MiB，1–64 MiB |
| EL2 | completed | 下载/副本/读取/回传共用当前配置 | 超过旧 4 MiB、设置降低/提高、协议入口与字节边界 | 已排队操作保留取得的限制 |
| EL3 | completed | 双语文档/翻译/地图与最终门禁 | Cargo fmt/check/strict Clippy/test/build、文档 validators | 不联网、不使用多 agent |
| GC1 | completed | 按性能优化、系统程序编辑与配置、文档记录整理本地提交 | 每次检查暂存差异，完成后核对提交与工作区 | 代码提交 d9cbc4a / 76eef8f；本记录随文档提交保存，不推送 |
| EL4 | pending | 用户设置/关联程序/真实远端 GUI 验收 | 新控件、保存后生效与外部编辑回传 | 前轮真实平台验收保留 |

## 已完成

- 已读取工程技能、环境记忆、地图、Cargo/CI、架构契约和相关源码；现有 737 项测试和两种 macOS 构建为历史基线。
- 环境预检完成：Rust 2024/MSRV 1.92.0、本机 Rust/Cargo 1.97.1、Slint 1.18.1，两个 macOS target 可用；无依赖/工具链/CI 变化。
- 先计划后施工；无新增 Worker 记录。
- 本地提交已分离性能优化（d9cbc4a）与系统程序编辑/大小配置（76eef8f）；双语文档和验证记录随文档提交保存。拆分后的性能 index 快照单独通过 fmt 和 all-target Cargo check。

## 验证

- 已完成：环境与源码预检，Slint 重编及 24 项专项（含 6 项新增），最终 743 项全量测试（349 library + 394 binary），fmt/locked-offline check/严格 all-target Clippy/native ARM64 build，562 条翻译、30 条相对链接、skill/tracker/diff 检查。
- 未完成：GUI/真实远端、Windows/Linux 原生验证及本轮非原生 Intel 构建；前轮两种 macOS 的验证仅作历史记录。

## 风险与阻塞

- 当前编辑字节会整文件驻留内存，配置范围限制为 1–64 MiB，不提供无限值。
- 保存期间设置可改变；下一个检查采用当前设置，已排队网络操作保持自己的数值，超限副本保留。
- 本机缺少 Windows/Linux SDK；真实 GUI/关联程序/SSH 验收由相应平台完成。

## 下一步

- 使用 `target/debug/ax_ssh` 验收 Settings > SFTP 的远程编辑大小设置与真实回传；当前施工和自动验证完成，GC1 本地提交交付完成；EL4 用户 GUI/真实平台验收保留。

## 最后更新时间

- 2026-10-11 19:34 +0800
