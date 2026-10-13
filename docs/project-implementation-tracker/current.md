# 当前项目实施记录

## 当前目标

- 目标 ID：20261013-code-ownership-compliance
- 目标：按 AGENTS.md 和架构契约修复错误处理、平台 cfg 与秘密持久化问题，并按实际职责拆分审查指出的代码文件。
- 交付物：安全凭据迁移及错误路径回归；SFTP/终端应用桥接、配置和状态子模块；拆分后的 Slint SFTP 组件；双语契约、项目地图和完整验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/credentials.rs`、`src/config/`、`src/sftp/transfer.rs`、`src/app.rs` 与相关 `src/app/` 子模块、`ui/sftp-pane.slint`、配套测试和双语文档。
- 不在本轮范围内：依赖或工具链升级、CI 矩阵修改、视觉改版、真实凭据/设备/远端访问、tag/Release 发布、无关历史文档整理。
- 所有权：config 独占 schema/私有文件写入；credentials 独占秘密后端与迁移；SFTP worker 独占传输；应用子模块编排 owned DTO 和回调；Slint 仅组合视觉组件。
- 安全：自动解锁密钥不再新写明文文件；旧文件仅在系统凭据保存成功后清理，失败保持恢复能力。主机信任、短期认证输入、有界队列和 worker 取消/关闭保持。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 远端交付：用户授权 origin/master 推送与远端核对；网络错误使用命令级 7897 代理
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| A1 | completed | 环境预检、审查证据和施工计划 | manifest/lock/build/CI、工具链及地图核对 | Rust 2024/MSRV 1.92.0；本机 1.97.1，两个 macOS target |
| A2 | completed | 秘密后端迁移、权限/fsync/上传 close 错误传播及平台 cfg 修复 | 隔离 keyring、持久化与 SFTP 错误回归 | 不访问真实用户凭据；旧文件迁移失败不删除 |
| A3 | completed | SFTP upload/download/discovery 与测试现代子模块 | 实际 loopback SFTP、取消与传输专项 | 保持现有 crate API 与 worker 所有权 |
| A4 | completed | 应用 SFTP/terminal 功能子模块与 workspace 启动恢复归位 | 实际 callback/monitor、状态和本地文件专项 | 保留弱 UI 引用及 event-loop dispatch |
| A5 | completed | 分类 settings、Tab 状态子模块与 SFTP UI 组件 | 配置迁移、Tab/workspace 与 Slint 重编测试 | 无 schema 或布局行为变化 |
| A6 | completed | 双语契约、地图、环境/实施记录与完整门禁 | fmt/check/严格 Clippy/test、可用 macOS CI target check/Clippy/build、链接/skill/tracker/diff | GUI 与真实系统后端由用户验收，Windows/Linux 交原生 CI |
| A7 | in_progress | 按结果分项提交，显式暂存和 index 审查 | staged diff/check 与每次提交后的 status | 不改写历史，不包含缓存/报告 |
| A8 | pending | 推送 origin/master 并核对远端 HEAD | push 与 ls-remote SHA 一致、最终工作区状态 | 直连失败用本机 7897 命令级代理 |

## 已完成

- 已读取 AGENTS.md、Rust/Slint 技能及引用、架构模块职责和项目地图；用户授权处理代码审查结果，工作区起始干净。
- 已核对本机 Rust/Cargo 1.97.1、ARM64/Intel macOS target 与既有锁定环境；不新增依赖或修改 CI。
- 已在代码施工前落盘本计划；前轮覆盖率交付保留在月度历史。
- 自动解锁密钥改用系统凭据后端，保留只读旧文件迁移和失败恢复；新增最小的受保护 entry provider crate 边界，让 binary 应用测试使用独立内存凭据而不接触真实 keyring。
- 权限和目录 fsync 错误传播；临时文件权限在原子替换前确认；上传成功包含 close 响应且取消不能返回 Published；非 macOS 无用字段不再用 allow(dead_code) 掩盖。
- SFTP 与终端 bridge 已按实际功能拆成私有子模块；本地上传扫描归入 local_files；启动恢复归入 workspace；settings 分类、Tab 快照/刷新/终端状态和 SFTP UI 组件完成迁移和完整验证。

## 验证

- 已完成：最终源码 fmt、locked/offline check、严格 all-target Clippy、完整默认并发 test 全部通过；391 library + 413 binary（804），Doc tests 0。
- macOS 平台：ARM64/Intel 均通过 CI 同款 target check、严格 all-target Clippy 与 build；Mach-O 架构分别为 arm64/x86_64。不在 ARM64 主机运行 Intel 测试。
- 翻译/脚本/文档：665 条消息 × 11 个翻译目录、translation build check、22 项 Python 回归、145 条 Markdown 相对链接、skill/tracker 与 diff 检查通过。
- 结构复核：Rust 无 mod.rs；SFTP/terminal 回调注册与 Slint struct/component/callback 契约保留；生成 Slint 类型仅在 app.rs。新增 entry provider 返回自有值/通用错误，平台错误保留在 credentials 内部。
- 已处理的验证问题：写入 fixture 使用独立目录，已有 0700 不重复 chmod；Skia 下载停滞后核验同版本/架构/特性完整归档，用临时 file URL 和用户指定 7897 代理完成目标构建，不改变依赖或 CI。
- 未完成：分项提交与推送/远端核对；自动验证边界：未访问真实系统凭据、设备或 SSH/GUI；Windows/Linux SDK 不可用，保留原生 CI；GUI 验收由用户完成，不自行截图。

## 文件职责整理

AGENTS.md 没有统一行数上限；本轮依据职责混合、错误处理和平台边界判断，不把长文件本身当作违规。

| 原入口 | 原行数 | 当前行数 | 功能归位 |
| --- | --- | --- | --- |
| `src/app.rs` | 955 | 850 | checkpoint 恢复归入 workspace |
| `src/app/sftp_bridge.rs` | 3126 | 195 | 导航、文件、拖放、传输；本地发现归入 local_files |
| `src/app/terminal_bridge.rs` | 2673 | 89 | 输入、原生事件、几何、输出、剪贴板、targets、panes、本地 shell |
| `src/sftp/transfer.rs` | 3658 | 892 | 控制、上传、下载、发现、隔离 peer 测试 |
| `src/config/settings.rs` | 1454 | 155 | appearance/audio/sftp/shortcuts/terminal/workspace/x11 |
| `src/app/state/tabs.rs` | 1537 | 633 | refresh/snapshots/terminal；编辑器状态归入 editor |
| `ui/sftp-pane.slint` | 1927 | 541 | types/directory/transfers，单 app.slint 构建入口保持 |

## 风险与阻塞

- 系统凭据后端可能失败或触发系统授权；旧解锁文件必须在可恢复迁移成功后才能清理，错误不含秘密。
- 模块移动需保持私有辅助函数、测试可见性和平台 cfg，不能用 allow(dead_code) 掩盖跨平台问题。
- Windows/Linux SDK 不可用；Intel macOS 仅编译链接，不在 ARM64 主机运行测试。

## 下一步

- 显式暂存并复核各组提交，通过本机 7897 命令级代理推送 origin/master，核对远端 SHA 和最终工作区。

## 最后更新时间

- 2026-10-13 08:37 +0800
