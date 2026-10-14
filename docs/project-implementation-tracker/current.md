# 当前项目实施记录

## 当前目标

- 目标 ID：20261014-sftp-full-path
- 目标：根据用户反馈将 SFTP 分段导航改为直接路径显示，路径占据行内最大空间，超宽时从中间省略并优先保留末尾，以设置齿轮承载路径操作。
- 交付物：单行首尾保留的路径展示、完整原文编辑和路径操作菜单；删除不再使用的分段模型；定向回归、双语说明和验证/提交/push 记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/sftp/path-bar.slint`、`ui/components/` 路径展示、`ui/icons/`、相关应用展示映射/DTO/回归、双语 usage/architecture、环境和实施记录。
- 不在本轮范围内：上传/下载样式、传输区行为、SSH transport/trust/认证、文件系统解析/校验、配置 schema、工具链/依赖/CI 和 tag/Release。
- 所有权：UI 根据实际字体/宽度计算展示与编辑/菜单状态；应用展示层仅提供有界纯文本裁剪辅助，原始路径始终用于已有目录/复制 callback；移除分段导航 DTO。
- 安全：省略后的路径不得成为文件系统或 SSH 意图目标；保持 owned 值、显示计算有界且无 I/O；无新增秘密、协议句柄或持久化。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用
- 交付：沿用用户提交/push 授权；已证实直连 HTTP/2 故障时使用命令级 7897 代理。GUI 视觉/键盘验收由用户确认。

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| F1 | completed | 环境/地图/所有权预检与落盘计划 | manifest/lock/build/CI、工具链、相关源码核对 | Rust 2024/MSRV 1.92.0，本机 1.97.1，两种 macOS target |
| F2 | completed | 路径最大占宽、首尾保留中间省略与齿轮菜单 | 实际字体宽度、完整路径编辑/导航/复制、最窄/Unicode 长路径回归 | 尾部优先；沿用既有路径意图 |
| F3 | completed | 删除分段 DTO/映射及同步双语 docs/地图/翻译 | Cargo Slint 重编、翻译/链接/skill/tracker/diff | 不保留无调用的旧解析器 |
| F4 | completed | 标准 Rust 门禁和可用平台构建 | fmt/check/严格 all-target Clippy/test 与 macOS target check/Clippy/build | 不执行非原生 Intel 测试；Windows/Linux 交 CI |
| F5 | completed | 分项提交与普通 push | index 审查、远端 SHA 与干净工作区 | 不改写历史或全局代理 |
| F6 | pending | 用户视觉与键盘验收 | 用户运行新版并提供截图/确认 | 不捕获或检查代理自身应用截图 |

## 已完成

- 起始工作区干净，前轮三个提交已保存并推送。
- 已读取环境/地图/AGENTS.md、Rust/Slint 技能与引用、相关路径组件/DTO/回归；核对既有目录和菜单意图。
- 用户希望直接看到路径，两端特别是尾部尽量保留；路径行只保留主文本和设置齿轮，编辑和复制使用原文。
- 直接路径/齿轮菜单已实现，UI 根据真实字体宽度有界适配；应用只提供保留首尾的无状态文本 helper。旧分段 DTO、映射/解析器及无调用的导航 SVG 已移除。
- 双语说明和地图已同步；已有 665 条文本覆盖新菜单，不需新增翻译。未联网检索、未使用多 agent 或 Worker 记录。

## 验证

- 已完成：预检、Slint/全新回调编译与 59 项 SFTP 专项通过，包括真实字体适配/扩宽、原文编辑/复制与 Unicode 有界展示；8 Python、665 × 11 翻译、119 相对链接、六个 SFTP SVG、skill/tracker/diff 通过。测试路径加长以避免 Linux 无 CJK 字体时不触发省略；最终版本与二分 upper-mid 调整纳入完整门禁。
- 已完成：冻结源码的 59 项专项、391 library + 450 binary（841）完整测试、fmt、locked/offline check 与严格 all-target Clippy；ARM64 target check 通过。
- 已完成：ARM64/Intel CI 同款 check、严格 all-target Clippy/build 与 Mach-O 架构核对；整个门禁期间源码指纹未变。
- 已完成：实现提交 `cbd3a2c` 和文档提交 `bfc629b` 已通过命令级 7897 代理/HTTP1.1 普通 push；独立 ls-remote 与本地 HEAD 均为 `bfc629b9c740bd6136fae03075dd6312f1540b0d`，核对时工作区干净。本条作为后续交付记录提交并再次推送/核对。
- 未完成：用户视觉/键盘验收；Windows/Linux 原生验证交 CI，Intel 不执行测试。

## 风险与阻塞

- 中间省略必须按实际文本宽度并保持 UTF-8 边界，不能裁坏 Unicode 或误用缩略显示作为目标。
- 宽度/字体改变后需更新显示，避免 UI layout 反馈环；计算与文本预算有界。
- Windows/Linux SDK 本机不可用，原生 CI 保留；GUI 视觉验收属于用户。

## 下一步

- 用户运行 `target/aarch64-apple-darwin/debug/ax_ssh`，检查完整/超长路径、栏宽/字体变化、齿轮导航/复制与点击编辑/Enter/Escape，并提供截图确认。F6 在用户验收前保持 pending。

## 最后更新时间

- 2026-10-14 08:27 +0800
