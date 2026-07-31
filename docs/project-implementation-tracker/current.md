# 当前项目实施记录

## 当前目标

- 目标 ID：20260731-split-theme-mode-palette
- 目标：把显示策略 `System / Light / Dark` 与配色方案 `AxSSH / Solarized / Custom` 拆成独立设置，并让每个配色方案分别解析 Light/Dark 语义色。
- 交付物：schema v11 兼容迁移、双 Custom palette、对比度保护、独立模式/配色控件、统一 Theme token 解析、双语架构记录和完整回归验证。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config.rs`、`src/app.rs`、`src/app/{settings_bridge,view}.rs`、`ui/{app,theme,settings}.slint`、`ui/settings/appearance.slint`、`docs/architecture*.md`、`docs/usage*.md`、`docs/project-implementation-tracker/`。
- 不在本轮范围内：原生 `ContextMenuArea` 的平台色值、SSH/凭据/PTY/worker 生命周期、Rust/Slint/依赖版本升级、参考子模块初始化或耦合。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| T1 | completed | 环境预检、现有主题数据链审计、schema/迁移/对比度方案 | 环境记忆与 Cargo 最小复核；Theme/Settings 全链搜索；既有研究记录 | AxShell 子模块未初始化，不初始化、不依赖、不复制 |
| T2 | completed | schema v11、独立 `ThemeMode`/palette、双 Custom palette 和兼容迁移 | focused `config` 单元测试 | 旧 Solarized/Custom 外观方向需保留 |
| T3 | completed | Slint 双模式 resolver、独立设置控件、双 palette 编辑和 Rust bridge | `cargo check --locked --offline`；主题契约搜索 | 所有组件继续只消费 `Theme` 语义 token |
| T4 | completed | 双语架构/使用说明、project-map 刷新和完整门禁 | Cargo/格式/测试/tracker/Markdown/diff 检查 | GUI 视觉结果由用户截图验收 |

## 已完成

- 已确认现有 `ThemeMode` 把显示策略、预设色板和 Custom 混在一个枚举内，Custom 只有一套 13 色，无法在 System 模式下分别适配 Light/Dark。
- 已复核环境记忆、`Cargo.toml` 和本机工具链；Rust 2024、MSRV 1.92.0、Slint 1.17.1 与 locked/offline 构建事实未漂移，环境审计文件无需更新。
- 已确认本轮不改变异步或安全边界；持久化归 `src/config.rs`，生成类型和 callback 映射归 `src/app.rs`，视觉解析归 `ui/theme.slint`。
- 已采用现有 AxShell/网络研究结论：显示策略与 palette 独立；普通文字至少 4.5:1，关键边界/焦点至少 3:1。
- 已升级 schema v11：`ThemeMode` 只保留 System/Light/Dark，新增 AxSSH/Solarized/Custom palette 类型，Custom 分别持久化 Light/Dark 两套 13 色。
- 已实现 v8/v10 迁移：旧 Solarized Dark 变为 Dark + Solarized；旧 Custom 按背景亮度迁移到对应侧，另一侧使用安全 AxSSH 默认。
- 已在配置规范化中保护表面方向、正文/次要文字、边框、焦点/强调、成功/危险和终端文字/选区对比度；不安全值回落到相同明暗侧的安全 token。
- 已新增共享 `ThemePaletteEditor`，Appearance 分别显示 Display mode 与 Color palette，Custom 才展开 Light/Dark 两套编辑器。
- 已让 `Theme` 同时接收选中 palette 的 Light/Dark 两侧，并显式提供 divider/frame/control/focus/hover/selected 状态 token；共享下拉及主要框线组件已切换到对应语义。
- 已同步中英文架构/使用说明并刷新 project-map 的 schema v11、主题 bridge、状态 token 和 `ThemePaletteEditor` 路由。
- 已修正旧 Custom 迁移的明暗方向：浅背景迁移为 Light + Custom，深背景迁移为 Dark + Custom，从而保持升级前的实际外观。

## 验证

- 已完成：环境/架构/主题路径静态审计；AxShell 子模块边界确认；既有 W3C/Slint/AxShell 研究记录复核；focused config tests（25 passed）；`cargo check --locked --offline`；完整 `cargo test --locked --offline`（库 57 passed、1 ignored，应用 28 passed，Doc tests 通过）；直接 rustfmt、tracker validator、Markdown 相对链接、旧主题残留搜索和 `git diff --check`。
- 未完成：本机未安装 Cargo `fmt`/`clippy` 子命令；按仓库约束未自行截图，目标平台 GUI 视觉验收由用户完成。

## 风险与阻塞

- schema 必须兼容 v10 的 `solarized-dark` 和 `custom` 合并模式，不能让升级后意外切换明暗方向。
- Custom palette 既要允许个性化，也必须避免文字、边框、焦点、状态色和终端选区失去必要对比度；不合格项应回落到相同明暗方向的安全 token。
- System 模式的实际 Light/Dark 由 Slint 运行时决定；两套自定义 palette 都必须在启动时送入 `Theme`，不能等系统切换后再做阻塞读取。
- 当前工作树已有用户的主题与会话管理改动；本轮在现状上增量迁移，不覆盖无关修改。
- 无阻塞。

## 下一步

- 由用户在实际窗口中检查 AxSSH/Solarized/Custom 与 System/Light/Dark 的组合、Custom 双侧编辑，以及边框/分隔线/焦点/下拉/终端选区的最终视觉。

## 最后更新时间

- 2026-07-31 07:57 +0800
