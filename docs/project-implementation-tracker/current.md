# 当前项目实施记录

## 当前目标

- 目标 ID：20261014-text-editing-shortcuts
- 目标：统一输入框编辑快捷键，在 macOS 支持 Control 光标/删除动作，其他平台保留对应标准行为。
- 交付物：共享输入内核和集中策略、普通/多行/密码输入接入、平台与 Unicode/安全回归、双语文档及验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/` 文本输入、`ui/sftp/path-bar.slint`、`ui/app.slint`、`src/app.rs` 和编辑适配/回归、双语文档与实施记录，以及用户授权的分项本地 Git 提交。
- 不在本轮范围内：终端按键编码、SSH/凭据/worker、持久化 schema、工具链/依赖/CI、push 和 tag/Release。
- 所有权：Slint TextInput 独占文本、光标、选区、滚动、IME 和撤销；共享 UI 内核只发按键意图；应用适配将已匹配动作定向交给当前窗口的已聚焦 TextInput。
- 安全：密码仍禁止复制/剪切/全选和鼠标选择泄漏；不传字段正文、不记录文本或秘密、不持久化输入状态；不介入终端 IME 输入。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| E1 | completed | 地图/环境/锁定 API 与平台预检、计划记录 | manifest/lock/build/CI、Slint 1.18.1 源码与官方文档 | Rust 2024/MSRV 1.92.0；本机 1.99.0/ARM64；Windows target 无本机 SDK |
| E2 | completed | 共享输入内核和平台/秘密/只读/IME 策略 | 编译与真实输入事件/Unicode 回归 | 复用底层标准编辑事件，避免自建光标状态 |
| E3 | completed | 普通/多行/密码输入接入及双语/地图/研究同步 | 地址栏、密码与多行回归、链接/翻译/tracker 检查 | 主/独立窗口共用注册；地址编辑使用平台主修饰键 |
| E4 | completed | 标准门禁与本机 CI 同款检查/构建 | fmt/check/严格 all-target Clippy/test、ARM64 target check/Clippy/build、diff | Windows/Linux 原生验证交 CI |
| E5 | pending | 用户原生键盘/焦点/IME 验收 | 用户运行构建并确认 | 不捕获或检查代理自己的 GUI 截图 |
| E6 | completed | 用户授权的分项本地提交 | 各组 index 审阅、提交后状态与最终 diff | 三项代码提交完成；配套双语文档和记录随本提交交付 |

## 已完成

- 前轮 SFTP 两侧导航按钮已实现，保留工作区现有改动，包括用户的目录菜单修复。
- 完成快速环境核查及 Slint 锁定 API 研究；公开选区 API 不提供完整移动命令，使用集中版本适配复用 TextInput 标准事件。
- 用户明确要求各平台对应行为；非 macOS 事件沿用上游标准实现，不将 Control-A 覆盖为行首。
- 共享 EditingTextInput、集中 EditPolicy/动作和焦点适配已接入普通/多行/密码；最多八层解析原生 popup adapter，不重分发到菜单或终端。
- macOS A/E、B/F、H/D、P/N 复用原生编辑；地址编辑入口只用平台主修饰键；密码本地保护在未注册 callback 时也有效。
- 双语 usage/architecture、地图和研究已同步；采用完整跟踪，未使用多 agent/Worker。
- 本机 ARM64 可执行文件已更新至 `target/aarch64-apple-darwin/debug/ax_ssh`；没有启动 GUI 或连接真实 SSH。
- 按用户后续授权创建本地提交：`8544db8` 修复目录行菜单身份，`de1be55` 添加两侧地址栏导航，`7c836dc` 统一平台输入快捷键；双语文档和验证记录独立提交。

## 验证

- 已完成：8 项编辑专项、最终源码 fmt/locked-offline check/diff、665 × 11 翻译、8 份改动文档的 12 条相对链接和技能校验通过；current/map 和新增历史/研究记录校验通过。技能校验使用自动清理的临时 Python 环境，不修改项目依赖。
- 已完成：标准严格 all-target Clippy、391 library + 458 binary（849）完整测试，以及 ARM64 CI 同款 check/严格 all-target Clippy/build 全通过；Mach-O arm64 架构核对通过。
- 已完成：三项代码 index 分组审阅及提交后状态核对；提交期间仅更新交付记录，受测源码保持不变。
- 未完成：用户原生键盘/焦点/IME/popup 验收与 Windows/Linux 原生 CI；全量 tracker 校验存在此前历史格式问题。

## 风险与阻塞

- 编辑适配涉及锁定 i-slint-core 1.18.1 的焦点/item 事件接口，集中在一个模块；升级 Slint 时须核对接口并执行回归。
- 必须保留 grapheme/UTF-8、反向选区、撤销、IME 和密码剪贴板边界，不在应用维护第二份文本/光标。
- 当前安装 ARM64 macOS 和 Windows target；Windows SDK/linker、Linux SDK 本机不可用，原生 CI 保留。
- 安装的 tracker validator 对旧历史要求更严格；本月历史 47 项、research 14 项错误与 HEAD 基线一致，本轮新增 0 项。不编造旧时间或修改无关历史。

## 下一步

- 用户运行 `target/aarch64-apple-darwin/debug/ax_ssh`，验证地址栏 Control-A/E、中文/emoji、密码移动与粘贴、输入法和 popup 搜索框；E5 保持待确认。

## 最后更新时间

- 2026-10-14 10:36 +0800
