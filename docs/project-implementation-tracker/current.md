# 当前项目实施记录

## 当前目标

- 目标 ID：20261014-workspace-connecting-layout-focus
- 目标：修复 Workspace 连接等待期的布局/焦点问题，并统一侧栏两种形态的 Group 图标、展开箭头和紧凑缩进。
- 交付物：连接前尺寸更新与焦点承接、共用 Group glyph、紧凑侧栏缩进、回归与双语记录；按用户追加授权分项提交并推送 origin/master。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/terminal-pane.slint`、`ui/components/session-navigation.slint`、`ui/components/sidebar-controls.slint`、`ui/theme.slint`、`src/app/view/tests.rs`、`src/app/view/tests/terminal_connection.rs`、`src/app/state/tests.rs`、双语架构/使用说明和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：transport/信任/认证/凭据、配置 schema、同服务器多连接能力、依赖/工具链、发布 tag 或改写历史。
- 所有权：Slint 负责 pane 几何、等待焦点及侧栏 glyph/布局，Theme 管理统一尺寸；现有 UUID 定向 resize 入口更新模型和 worker，只有 connected 的 IME 输入进入 transport。
- 安全：不缓冲连接前的键入，不影响模态安全对话框，不新增网络请求或凭据持久化。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| C1 | completed | 用户截图与尺寸/焦点根因核查 | 恢复模型、Slint resize/focus 条件、Slint/WAI/RFC 文档 | 用户确认连接成功后自动恢复；10×3 初始模型被 connected 条件卡住 |
| C2 | completed | 连接前几何与等待焦点修复、定向回归 | 实际指针/Enter、主/独立窗口、模型重排 | 保留多连接意图与安全输入边界 |
| C3 | completed | 完整仓库验证和说明 | fmt/check/严格 Clippy/test/build、翻译、相对链接、tracker/diff | 本机 ARM64 macOS 调试程序已重建；不捕获应用截图 |
| C4 | pending | 用户 GUI/真实连接确认 | 恢复、连接等待、Enter、模态框和重连 | 视觉验收由用户完成 |
| S1 | completed | 共用 Group glyph 与紧凑缩进 | 静态布局差异、Slint 重编译 | 按收起栏：文件夹右侧 7px 箭头，展开向下/折叠向右；缩进 42px 改为 20px |
| S2 | completed | 侧栏改动完整验证和双语说明 | fmt/check/严格 Clippy/test/build、翻译、链接、tracker/diff | 857 项 Rust 测试通过，本机调试程序已重建 |
| S3 | pending | 用户侧栏视觉验收 | 用户确认或截图 | 位置、大小和留白；不捕获代理应用截图 |
| G1 | completed | 分项本地提交 | 显式暂存、逐组审阅 staged diff、提交范围 | 代码分别保存为 59f6e3b、bb9a00a；双语文档独立归档 |
| G2 | in_progress | 推送并确认 origin/master | 远端 master SHA、祖先关系、正常 push、独立 ls-remote | 直连失败后使用本机 7897 代理；不改全局配置 |

## 已完成

- 初始工作区干净，前轮 Tab hover 已分项本地提交。
- Workspace 历史以 10×3 重建；TerminalPane 的尺寸上报和焦点交接均要求 connected，等待期间侧栏仍接收 Enter 并执行创建新 Tab。
- 用户确认连接成功后窄列自动恢复，与 connected 门槛一致。
- 核对 Slint 焦点、WAI 聚焦项激活与 RFC 4254 的 PTY/window-change；问题是本地几何/焦点条件，不据截图推断 SSH 故障。
- TerminalPane 在连接前上报实测尺寸；等待 FocusScope 承接焦点和 Enter/Space，不启用 IME 或发送输入；connected 后转到原生 IME，阻塞提示保持优先。
- 3 项定向回归通过：主/独立窗口在连接前 resize；展开/收起侧栏真实点击后连续 Enter 不重复建连，等待输入不补发，主机密钥确认焦点不被抢占，连接后的 Enter 正常，主动点击可打开第二个连接；恢复模型无 worker 时可按实测尺寸重排。
- 双语架构/使用说明、项目地图与研究记录已同步；无 transport、schema、凭据、依赖或工具链改动。
- fmt、locked/offline check、严格 all-target Clippy、391 library + 466 binary（857）完整测试与本机 debug build 全通过，定向回归覆盖的源码保持一致。
- 用户追加截图要求侧栏箭头统一并减少展开态左侧留白；确认展开栏使用 14px 左置上下箭头和 42px 子项缩进，收起栏使用 7px 右置向右/向下箭头。
- 两种侧栏现共用 `SidebarGroupGlyph` 的 20px 文件夹、3px 间隔和 7px 箭头，展开向下/折叠向右；收起栏的原图标/箭头位置保持，展开栏移除前置箭头槽，子项缩进由 Theme 派生为 20px，名称与组名对齐。
- 修改后的 Slint 已重编译，既有主/独立窗口几何及展开/收起侧栏真实点击/Enter/焦点回归通过；双语架构/使用说明和地图已同步，665 × 11 翻译/生成、8 项翻译脚本、12 个相对链接及新 tracker 记录通过。
- 侧栏改动再次通过 locked/offline check、严格 all-target Clippy、391 library + 466 binary（857）完整测试和本机 debug build；前轮连接等待期修复同时通过回归。
- 用户追加授权分项提交与推送；代码已正常提交为 `59f6e3b`（连接前布局/焦点）和 `bb9a00a`（侧栏图标/缩进），配套双语说明与跟踪记录独立归档。
- 直连 origin 查询发生 curl 28 低速超时；本机 7897 代理可用，命令级 HTTP 代理读取远端 master 为 `ca729ae65a9728eee1e36d5b1f8f340310c0ecd9`，祖先关系核查通过。

## 验证

- 已完成：源码/用户症状/标准核对，Slint 实际重编译和 3 项定向回归，fmt/check/严格 Clippy/857 项 Rust 测试/build/diff、665 × 11 翻译/生成、8 项翻译脚本测试、12 个相对链接；current/map/新记录单独有效，完整 tracker 对照 HEAD 仍为 84 项旧错误且无新增；最终 diff 保留既有地图历史。
- 已完成：侧栏改动的 fmt/check/严格 Clippy/857 项测试/build/diff、翻译/生成和双语文档检查；完整 tracker 的 84 项旧格式问题对照 HEAD 无新增。
- 未完成：C4/S3 用户 GUI 验收；其他平台原生 CI 未在本机执行。

## 风险与阻塞

- 等待终端必须接收焦点但不启用 IME/网络输入；安全对话框仍优先持有焦点。
- 完整 tracker 已有 84 项旧格式问题；本轮记录单独验证并与 HEAD 比较错误集。

## 下一步

- 完成 G2 正常推送与独立远端 SHA 核对，并归档推送结果；C4/S3 仍由用户验收，不据推送推定 GUI 已通过。

## 最后更新时间

- 2026-10-14 18:23 +0800
