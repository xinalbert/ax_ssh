# 当前项目实施记录

## 当前目标

- 目标 ID：20261020-notices-persistence-shutdown
- 目标：修复固定连接/授权文案的翻译边界、会话管理持锁写盘和关闭 Tab 后的任务收尾。
- 交付物：实现、确定性回归、双语架构/翻译说明和验证记录；按用户后续授权分组本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：应用状态/会话管理/Tab 生命周期、Slint 提示映射、十一种翻译目录和配套文档。
- 不在本轮范围内：其它审查项、协议或配置 schema 变化、依赖升级、发布、推送、vendor 和参考项目。
- 所有权：ConfigStore 保留原子持久化；应用层串行事务与 worker 收尾；Slint 负责固定文案翻译。
- 安全：保留主机密钥默认拒绝、凭据零化、导入脱敏与 OSC 52 一次性授权；关闭任务有明确 owner 和退出等待。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 固定连接/剪贴板授权文案进入 Slint 翻译及十一种目录 | 语言切换/生成组件回归、翻译校验与 Cargo 编译 | 动态技术错误原文保留 |
| P2 | completed | 会话复制/导入/分组变更在短锁之外后台持久化 | 慢写盘下状态可访问、失败不提交、串行事务/预览保留 | 继续使用全局持久化 gate |
| P3 | completed | Tab 关闭先发取消/断开、统一持有清理任务并在退出等待 | 关闭后立即退出、任务失败/超时及实际 worker 回归 | 不在 UI 线程等待网络 |
| P4 | completed | 双语说明、地图及全套自动验证 | fmt/check/严格 Clippy/test、翻译/仓库/skill/tracker/diff | 本机 ARM64 macOS，GUI 由用户验收 |
| C1 | completed | 提示翻译及配套回归/双语说明提交 | 显式文件/代码块暂存、完整 staged diff/check | `5a33866`；其它功能块继续留在工作区 |
| C2 | completed | 会话后台持久化及配套回归/双语说明提交 | 显式暂存、完整 staged diff/check | `a37ec32`；owned gate 和事务成功合并 |
| C3 | completed | worker 统一关闭及配套回归/双语说明提交 | 显式暂存、完整 staged diff/check | `f54d814`；原子转交、退出等待及测试导入归属 |
| C4 | completed | 地图/当前状态/月度历史独立归档 | tracker/链接/skill/diff 与最终干净工作区 | 本记录随第四笔归档；提交后核对干净工作区 |

## 已完成

- 已读取 AGENTS、技能、地图、现有 tracker 和关键实现；基线工作区干净。
- 环境快扫：Rust/Cargo 1.99.0，MSRV 仍为 1.92.0；已装 ARM64 macOS 和 Windows MSVC Rust target，本机不具备 Windows/Linux 原生执行环境。
- 三项变更由用户明确授权；单 agent 实施，不创建 Worker 记录。
- 固定提示改用类型化 kind，Slint 负责十项新增固定文案；十一种目录已补齐。主/独立窗口英文、中文、日文切换回归通过，授权 action ID 与技术详情保持。
- 会话管理四类变更接入后台事务，blocking 操作持有 owned gate，锁外读写后仅合并 profiles/groups；四项慢盘/失败/取消/导入脱敏回归通过。
- worker 清理 registry 覆盖 Tab 关闭、工作区替换、本地 shell 收尾与进程退出；三项回归通过，包括实际 loopback Telnet worker 断开、立即退出等待、错误后的总超时取消/join。probe 在 Tab 删除前取消。
- 用户授权后分三笔本地提交：提示翻译 `5a33866`、会话后台持久化 `a37ec32`、worker 统一收尾 `f54d814`；双语架构随相应功能提交，当前/map/月度历史另行归档。源码指纹 `c9112e9fe17a6522524d01e7b743b62a2df1eb6192ee4183b3bb172a255dca19` 在提交前后一致。
- 双语架构/翻译说明和地图已同步；675 × 11 翻译、生成表、53 份 Markdown/入口、26 项 Python、skill/tracker/diff 校验通过。skill validator 使用缓存中的隔离 PyYAML 环境，不修改项目依赖。

## 验证

- 已完成：最终源码 fmt、locked/offline check、严格 all-target Clippy、399 library + 489 binary 共 888 项 Rust 测试通过；含 8 项新增回归与补强后的本地 shell 原子转交回归。675 × 11 翻译与生成表、26 项 Python、116 项图标、53 份 Markdown/入口、skill/tracker/diff 检查通过，四个新增源码文件空白复核通过。
- 未完成：Windows/Linux/Intel macOS 原生 CI、用户视觉/键盘焦点及真实 SSH 生命周期验收；本机 ARM64 macOS 的自动结果不扩大到其它平台。

## 风险与阻塞

- 会话复制/导入/分组后台事务保留写盘期间的 Settings 预览，失败不发布候选状态，调用者取消时 owned gate 继续保护在途写入。
- worker 收尾不持同步状态锁跨 await；Tab/child pane/本地 shell 的移出与登记已在同一次锁内，退出统一等待。
- GUI 视觉/焦点与真实 SSH 生命周期需用户在目标平台验收；不自主截图。

## 下一步

- 本记录随第四笔提交归档后核对四笔提交与最终干净工作区。后续由用户验收 GUI/真实 SSH，既有原生 CI 验证其它目标；本轮仅本地提交。

## 最后更新时间

- 2026-10-20 16:58 +0800
