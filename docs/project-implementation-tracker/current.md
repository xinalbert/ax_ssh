# 当前项目实施记录

## 当前目标

- 目标 ID：20261116-compatible-dependency-upgrade
- 目标：更新已核对的五项直接依赖及兼容间接依赖，保持 Rust 1.92 和现有平台基线。
- 交付物：Cargo 约束/锁文件、准确的安全和版本说明、完整验证及实施记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：russh、notify、rand、uuid、rubato 及兼容传递依赖；双语开发/架构、CI 注释、环境和实施记录；必要时修复依赖升级导致的兼容问题。
- 不在本轮范围内：CPAL/macOS 下限升级、替换上游固定 RSA/PKCS#1、Slint/vendor/工具链迁移、凭据或持久化契约改动、GUI 启动/截图、推送或发布。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| D1 | completed | 版本、安全与平台约束核对 | 54 项直接依赖 registry、上游 release/manifest、RustSec、工具链 | 五项候选 MSRV 均低于 1.92；保留 CPAL 与上游 RSA pin |
| D2 | completed | 五项约束、兼容锁文件及准确双语说明 | Cargo update、锁文件差异、定向信任/音频/监听回归 | 39 项更新；三组 23 项定向回归通过，未改应用源码 |
| D3 | completed | 完整门禁、原生构建和记录收口 | fmt/check/Clippy/test、原生 CI 命令、MSRV check、audit、文档/tracker/diff | 1030 项测试、原生 check/Clippy/build 和 Rust 1.92 check 通过 |

## 已完成

- 已升级五项依赖：russh 0.65.1、notify 8.2.2、rand 0.10.4、uuid 1.29.2、rubato 5.1.0，并同步双语架构/开发说明及 CI 注释。
- russh 修复 host-key 验证回调时序等问题，notify 修复 Windows watcher 关闭死锁；notify MSRV 1.88，仍符合项目 1.92。
- RSA 稳定版 0.10.0 已修复公告，但 russh 0.65.1 仍精确锁定 rc.18；CPAL 0.18.2 要求 macOS 14.2，与当前 11.0 冲突。
- 已刷新本机 stable Rust/Cargo 1.99.0、Python 3.14.6、cargo-audit 0.22.2 和安装 targets 等过期环境事实；另用 Rust 1.92.0 完成 MSRV 编译验证。
- 实际更新 39 项包（5 直接 + 34 间接），移除 `untrusted 0.7.1` 和 `generic-array 0.14.9`；Slint/vendor、CPAL、工具链及 RSA/PKCS#1 pin 保持。
- 主机密钥 6 项、音频 12 项、目录监听 5 项定向回归全部通过；无需修改应用 Rust/Slint 源码。
- Rust 1.92 检查初次遇 Skia GitHub 归档 HTTP/2 错误及 googlesource 回退下载失败；核对本机归档的版本 0.153.3、ARM64/功能 key 和 gzip 完整性后，以临时 `SKIA_BINARIES_URL=file://...` 重试通过，不改项目配置或工具链。

## 验证

- 已完成：版本/安全核对、锁文件差异审查、23 项定向回归、fmt、locked/offline check、严格 all-target Clippy、升级后完整 1030 项测试（471 library + 559 binary，Doc tests 0），以及 macOS aarch64 的 CI 同款 check/Clippy/build、ARM64 Mach-O 确认和 Rust 1.92 locked/offline check。
- 已完成：升级后原始 audit 仍为 RSA 单项漏洞及 6 项停止维护警告，按已有唯一例外审计通过；128 项图标/5 文件生成、56 份 Markdown/源边界、项目 skill、tracker 和 diff 校验通过。
- 未完成：Windows/Linux/Intel macOS 原生 CI、真实音频/目录监听及 GUI 人工验收；不以本机编译回归替代这些结果。

## 风险与阻塞

- RSA rc.18 仍命中 RUSTSEC-2023-0071；CI 的唯一显式例外保留，不宣称本次消除此漏洞。
- 本机是 macOS aarch64；Windows target 已安装，但无对应原生 runner，Linux/Intel macOS 编译及 watcher/渲染/音频实机结果另验。
- Slint 和三个 vendor 补丁保持原版本；传递图形依赖变化已通过编译及已有回归，GUI 视觉由用户验收。

## 下一步

- 由其它平台原生 CI 继续验证；真实音频、目录监听和 GUI 由用户在目标平台验收。
- 等 russh 采用已修复的 RSA 稳定版后，再评估移除唯一审计例外。已按用户要求清理编译产物；原生程序需通过 `cargo build --locked --target aarch64-apple-darwin` 重新生成。

## 最后更新时间

- 2026-11-16 14:21 +0800
