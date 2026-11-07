# 当前项目实施记录

## 当前目标

- 目标 ID：20261107-sftp-intent-platform-cfg
- 目标：修复 Windows/Linux 严格 Clippy 中 macOS 专属 SFTP 下载意图产生的 dead_code 错误。
- 交付物：平台一致的枚举、注册方法和匹配分支，以及本机验证与跨平台验证限制记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/state.rs`、`src/app/state/sftp.rs`、`src/app/sftp_bridge/transfers.rs` 和环境/实施记录。
- 不在本轮范围内：SSH/SFTP transport、主机密钥信任、凭据、UI、依赖版本和 CI matrix。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 对齐 macOS 下载意图与调用方的 cfg | 审查全部构造与匹配位置、rustfmt | 应用层内部变更，不抑制 lint |
| P2 | completed | 编译、严格 Clippy 与现有状态回归 | focused tests、标准 Rust 门禁、可用 macOS target 的 CI 命令 | Windows/Linux 未安装目标及原生工具链 |
| P3 | completed | 完成记录与最终 diff 审查 | 仓库链接、tracker、diff 校验 | 项目地图已覆盖相关模块，无需刷新 |

## 已完成

- 已读取项目地图和 Rust/Slint 规范，确认唯一注册调用在 macOS 原生文件拖出路径。
- 已核对 Cargo/lock/CI 和本机 CLI：Rust/Cargo 1.97.1、Python 3.10.20；默认 toolchain 已安装 ARM64/Intel macOS Rust targets。
- 已完成环境预检并先写计划，原工作区干净；没有联网检索或多 agent；验证时仅按锁文件补齐本机依赖缓存。

## 验证

- 已完成：fmt、默认 locked/offline check、严格 all-target Clippy、完整 422 library + 523 binary 共 945 项测试、14 项 focused 重连回归；ARM64 macOS 精确 CI target check/Clippy 命令；仓库链接/边界、项目 skill、tracker 和 git diff --check 校验。
- 未完成：Windows/Linux/Intel macOS 原生 CI；本机通过不替代对应平台验证。

## 风险与阻塞

- 本机没有 Windows/Linux Rust targets 和对应原生工具链，不能用本机 macOS 通过替代这三个 CI job。
- 前序 SFTP 真实服务器复测与 Finder GUI 验收仍属于前序未验证项，本次 cfg 修复不完成这些验收。

## 下一步

- 重新运行 Windows/Linux 原生 CI，确认严格 all-target Clippy 通过；Intel macOS 仍由对应 CI 编译验证。

## 最后更新时间

- 2026-11-07 07:49 +0800
