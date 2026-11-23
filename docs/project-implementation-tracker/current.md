# 当前项目实施记录

## 当前目标

- 目标 ID：20261123-compatible-dependency-update
- 目标：按 Cargo 预演升级 Rust 1.92 兼容依赖，验证 russh 和 RSA 更新后的构建、回归与安全审计。
- 交付物：直接约束、锁文件、CI/Release 审计和双语说明同步，附本机验证结果。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：Cargo 预演中的五项直接依赖和兼容的传递依赖、RustSec 审计配置和相关文档。
- 不在本轮范围内：工具链升级、Slint 本地补丁、CPAL、notify 和 russh-sftp 跨版本迁移，应用 SSH 信任和凭据策略，已交接的 macOS 图标视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| D1 | completed | 确认可升级范围 | crates.io 和 Cargo 1.92 兼容预演 | 预演列出 29 个可更新包 |
| D2 | completed | 更新依赖与审计配置 | 29 项更新，无豁免 RustSec 审计通过 | RSA 0.10.0 |
| D3 | completed | 本机门禁和双语文档 | SSH 定向与全量测试、Clippy、构建、MSRV | 其他平台交原生 CI |

## 已完成

- 核对用户授权和锁定版本。russh 0.65.4 兼容 Rust 1.92，预演将 RSA 更新至 0.10.0。
- 更新五项直接与 24 项传递依赖。移除锁文件中的 generic-array 1.4.5 和 CI/Release 的 RSA 豁免。

## 验证

- 已完成：联网 Cargo 解析、RustSec 无豁免审计、Rust 1.92 ARM64 macOS check；8 项 SSH 主机密钥回归、480 library + 578 binary + 7 integration tests、fmt、locked/offline check、严格 Clippy、稳定构建和 macOS ARM64 target check/Clippy/build。
- 未完成：Windows、Linux 和 Intel macOS 原生 CI。

## 风险与阻塞

- RustSec 当前快照报告 7 项停止维护警告。其他平台原生验证由 CI 执行。

## 下一步

- 完成稳定工具链验证并提交依赖升级。

## 最后更新时间

- 2026-11-23 14:12 +0800
