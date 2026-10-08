# 当前项目实施记录

## 当前目标

- 目标 ID：20261003-duplicate-link-cleanup
- 目标：完成依赖升级后的 SSH transport 验证，并保持现有功能和安全边界。
- 交付物：russh 0.64.1 与锁文件/CI/双语架构说明同步，保留既有 SFTP、设置和打包行为。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/ssh/`、`Cargo.toml`、`Cargo.lock`、`.github/workflows/{ci,release}.yml`、双语架构、项目地图和环境验证记录。
- 所有权：Slint 管组件与草稿；AppState/应用 bridge 管路由与共同编排；SSH worker 管 transport；config 管私有存储；Cargo metadata 管 Linux 安装清单。
- 不在本轮范围内：Slint/Skia vendor patch、发布、修改现有 SSH trust 和凭据策略、参考子模块源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| DL1 | completed | 环境审计、调用链核对、工作区基线和计划 | Cargo/build/CI/源码核对 | 保留现有标题栏/Hidden 改动 |
| DL2 | completed | SFTP 单实例、闲置回调/资源/旧侧栏清理 | 数值布局回归与 Cargo check/test | 公共接口变动在记录中逐项列出 |
| DL3 | completed | 设置参数/字体、目录历史、Telnet/Serial 共同流程 | 设置/导航/attempt 聚焦测试与完整门禁 | 沿用现有有界队列 |
| DL4 | completed | SSH probe、旧 SFTP/刷新路径、原子写入与打包清单收敛 | trust/生命周期/权限/打包回归 | 保留平台与安全行为差异 |
| DL5 | completed | 双语文档、项目地图、完整验证和总结 | fmt/check/Clippy/test/build、macOS targets、链接/skill/tracker/diff | GUI/真实 SSH 由用户验收 |
| RSH1 | completed | russh 0.64.1 与锁文件/CI/架构说明同步 | MSRV check、fmt/check/Clippy/test、RustSec、diff | host-key callback 与 RSA 兼容边界保持不变 |

## 已完成

- 逐项交付清单：见[重复链路清理记录](../duplicate-link-cleanup.zh.md)。

- 用户已明确授权按审查方案实施并做好记录；此前只读限制已被本轮授权更新。
- 核对 Rust 2024/MSRV 1.92.0、Slint 1.18.1、Tokio/russh 锁定依赖；本机 Rust/Cargo 1.97.1，安装两个 macOS target。
- 保存施工前 diff 和既有 SFTP/UI/文档基线；地图覆盖本轮范围，结构变动时同步刷新。
- 已确认两套设置提交各 86 参数，其中 84 相同；旧 SFTP load/close 无 UI 触发，单上传/OpenSftp 无仓库调用。

- DL2 已删除 vt100 副本、重复图标、无 UI 触发的 load/close 回调及辅助函数、旧侧栏布局分支和闲置单上传方法。
- SFTP 数值回归在旧代码复现侧栏 220 -> 360 使 detached local drop width 476 -> 406；主布局现增加 detached 创建闸门。
- 设置预览/保存共用 84 个草稿参数；目录历史和 Telnet/Serial 监控共同实现已落地，字体加载/注册共用帮助函数。
- `russh` 从 0.63.3 升级到 0.64.1；新版仍使用 `PublicKeyOrCertificate` host-key callback，RSA 仍固定 `rsa 0.10.0-rc.18`，未改变证书 deny-by-default、known_hosts 或 worker 所有权。

## 验证

- 聚焦验证：detached 5 条、SSH probe 2 条、原子写入 3 条、发布脚本 14 条通过；512 条翻译、23 条 Markdown 相对链接、skill/tracker/diff 通过。
- 打包验证：cargo-deb 3.7.0 在 amd64/arm64 fixture 中各生成 30 项资源，逐一校验内容、路径、mode、root 所有权及 Package/Version/Architecture；不模拟 Linux 二进制或运行库依赖。

- 已完成：russh 0.64.1 的 Rust 1.92 MSRV check、fmt/check/严格 all-target Clippy/完整 locked-offline test（库 303、应用 312、Doc tests 0）/debug build 全部通过；已有项目的聚焦回归、Debian payload、YAML/18 段 Bash、文档/skill/tracker/diff 通过。
- 未完成：GUI 视觉/原生拖放/真实连接、Windows/Linux 原生 runner，以及 Linux 真包安装和运行库依赖的验收；这些本机限制已记录。

## 风险与阻塞

- 新建/恢复探测的过期隔离、取消、deny-by-default 和 user/private 父目录权限差异已保留并回归。
- GUI 视觉、拖放、键盘及真实 SSH/Serial 生命周期由用户验收；Windows/Linux SDK/runner 本机不可用。
- 已用施工前基线确认既有 SFTP 标题栏/Hidden 及双语 usage 修改完整保留；本轮未发布。

## 下一步

- 用户重启最新构建验收独立 SFTP 拖放、布局、设置和真实连接；Windows/Linux 原生验证继续使用既有 CI 矩阵。

## 最后更新时间

- 2026-10-08 11:27 +0800
