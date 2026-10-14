# 当前项目实施记录

## 当前目标

- 目标 ID：20261014-shared-text-overflow
- 目标：修复新建服务器按钮在空间足够时仍省略的问题，并统一单行展示文字的溢出悬停提示。
- 交付物：按实际标签字体/内边距测量的共享按钮、共用省略标签接入、无截图布局/事件回归、双语说明与验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/elided-controls.slint`、共享控件和页面的单行展示标签、`ui/theme.slint`、`src/app/view/tests/`、双语文档和实施记录，以及用户授权的分项本地 Git 提交。
- 不在本轮范围内：编辑文字的光标/IME 策略、终端字符网格、SSH/凭据/worker、持久化 schema、工具链/依赖/CI、push 和 tag/Release。
- 所有权：Slint 共享控件独占自然宽度、截断检测与有界全文 Tooltip；标准 Button 保持点击、焦点、键盘、enabled 与无障碍语义。
- 安全：悬停仅展示非秘密标签或静态占位说明，绝不传入密码/口令正文；无新增 I/O 或跨 Rust/worker 契约。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| O1 | completed | 共享控件/锁定 Slint/布局预检 | 用户截图、manifest/build、组件源码、地图 | 初始工作区干净；标准 probe 和覆盖标签宽度不一致 |
| O2 | completed | 按实际标签测量按钮并统一省略显示 | Cargo Slint 重编和真实页面数字布局/hover/click | 复用既有 ElidedLabel/ElidedButton，不创建第二套提示 |
| O3 | completed | 完整门禁、双语文档和交付记录 | fmt/check/严格 all-target Clippy/test、翻译/链接/tracker/diff、本机构建 | Windows/Linux 原生与视觉验收保留 |
| O4 | pending | 用户 GUI 悬停/字体/布局确认 | 用户运行构建并提供确认 | 不捕获或检查代理自己的应用截图 |
| O5 | completed | 用户授权的分项本地提交 | index 分组审阅、源码指纹与最终工作区 | 两项代码提交完成，双语文档与记录随本提交交付 |

## 已完成

- 用户提供截图：新建服务器的 Cancel/Save & connect/Save session 在宽页面仍被截断。
- 仓库已有共享 ElidedLabel/ElidedButton；按钮 preferred-width 只采用标准 Button 的 min-width，但覆盖标签使用 Theme 字体和更大内边距。
- 部分共享表单标题/字段、下拉、设置与 SFTP 列仍直接使用 Text overflow，已接入统一全文悬停能力。
- 使用环境快速扫描和完整实施跟踪；Slint 1.18.1、Rust 2024/MSRV 1.92.0 保持；未联网研究或使用 agent/Worker。
- 前轮键盘/IME 与 Windows/Linux 原生验收仍未收到确认，本轮不将其标记完成。
- 共享按钮按覆盖标签自然宽度和统一内边距预留；28 处遗漏展示文字接入 ElidedLabel，原生编辑/按钮 callback 保持。ARM64 可执行文件已更新，未启动真实 GUI。
- 密码占位回归发现保护 TouchArea 遮挡 hover，已将标签置于重叠指针处理层之上；仅空字段说明可提示，输入正文后提示关闭。
- 下拉/菜单/文件行同样调整标签层序；菜单行改为自然 preferred-width 和可缩小 min-width，避免超出有界菜单后静默裁切。
- 根据用户后续授权，本地分项提交 `bf2c2be` 修复共享按钮宽度、`9ad1b1c` 统一截断提示和回归；双语说明与实施记录独立提交。

## 验证

- 已完成：4 项真实会话页面专项全部通过，覆盖英/德/中文、两种字体/窗口宽度、溢出 hover/resize、点击/焦点、密码占位和下拉/菜单指针层；Slint 已重编。
- 已完成：fmt、locked/offline check、严格 all-target Clippy、391 library + 462 binary（853）完整测试；ARM64 CI 同款 check/严格 all-target Clippy/build 和 Mach-O arm64 核对通过。
- 已完成：665 × 11 翻译、12 相对 Markdown 链接、skill 校验、current/map/新增日志和 diff 检查；技能验证使用自动清理的临时 Python 环境，不修改项目依赖。源码指纹 `4761e43261748fd6be4c894c2ac74420eb307c4bbb01e4301edb95a3286675b7` 在门禁及构建后保持。
- 已完成：两项代码 index 审阅及提交后状态核对，源码仍与受测/构建指纹一致；剩余改动仅为配套文档。
- 未完成：用户 GUI 显示/hover/字体/焦点验收和 Windows/Linux 原生 CI；完整 tracker 存在 84 项旧历史格式错误，本轮新增 0 项。

## 风险与阻塞

- 共享按钮修复须兼容主动设置固定宽度的调用方：空间不足仍允许省略和悬停；不把自然宽度设为全局最小宽度。
- 保持 Text 字体、对齐、行高和无障碍名称，Tooltip 不得抢占按钮/输入/菜单事件。
- 不给密码正文添加提示；终端内容和已有专用富详情 Tooltip 不改动。
- 本机仅验证可用 macOS 原生工具链；Windows/Linux 原生验证交 CI。tracker 旧历史格式问题仍保留。

## 下一步

- 用户运行 `target/aarch64-apple-darwin/debug/ax_ssh`，确认新建/编辑服务器按钮完整显示，缩窄、下拉/菜单与密码占位的悬停效果；O4 保持 pending。

## 最后更新时间

- 2026-10-14 11:13 +0800
