# 当前项目实施记录

## 当前目标

- 目标 ID：20261010-themed-menus
- 目标：跨平台应用内菜单统一消费 AxSSH Theme，修复 Windows 原生浅色弹出菜单与应用主题脱节。
- 交付物：共享主题化菜单、顶部/右键/编辑菜单接线、最近工作区子菜单、键盘/焦点契约、双语文档、验证记录和分组本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：Slint 菜单组件、现有 UI 手势和动作接线、双语文档与 tracker、分组本地提交。
- 不在本轮范围内：SSH/凭据/worker、持久化 schema、依赖升级、用户 LICENSE、推送或发行标签。
- 所有权：UI 拥有菜单呈现、鼠标/键盘和短暂选择；应用层保留既有 action callback。
- 安全边界：秘密输入菜单仅提供 Paste，不新增 copy/cut/select-all；不跨 UI 传递秘密或 transport。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TM1 | completed | 共享 Theme 菜单与有界子菜单/键盘控制 | Slint 重编译和静态动作检查 | macOS 屏幕系统菜单按推荐保留；应用内菜单跨平台统一 |
| TM2 | completed | 顶部、Session/SFTP/Terminal 和编辑菜单迁移 | 保留选择、拖动和秘密输入边界 | 三平台共用实现 |
| TM3 | completed | 双语文档、地图和完整门禁 | fmt/check/Clippy/tests/links/validators/diff | 仅宣称本机可验证目标 |
| TM4 | pending | 用户原生 GUI 验收 | Windows/Linux/macOS 菜单配色、键盘、子菜单和焦点 | 不捕获应用截图 |
| TM5 | completed | 分离实现与双语文档/记录的本地提交 | 暂存差异、已有门禁、提交后状态 | 实现 ddc1a71；文档记录随本行所属提交收口，保留 LICENSE |
| RT6 | pending | 用户原生验收延迟提示和连接生命周期 | 前后台、分屏/独立窗口、重连、真实 SSH/SFTP | GUI 验收属于用户 |
| IP5 | pending | 前轮服务器/Local Shell 选择器验收 | 宽窄窗口、常用排序/搜索、自动/手动、焦点 | 715c5dc/69b091f 已提交，657 项测试通过 |
| DI5 | pending | 前轮标题栏与 `+` 列表原生验收 | 分屏/返回/恢复、配色与列表键盘 | b6cbad7/f92c2c5 已提交 |
| PW5 | pending | 前轮逐服务器凭据清除原生验收 | 状态刷新、确认/取消、真实系统后端 | 自动验证完成 |
| UI7 | pending | 前轮导航系统图标原生验收 | 发行版/手动指定、配色、tooltip/焦点 | 自动验证完成 |

## 已完成

- 共享 Theme PopupWindow 菜单支持分隔/禁用、十行滚动、循环键盘导航和最近工作区子菜单。
- 已迁移 Windows/Linux 顶部及三平台 Terminal/Session/SFTP/单行/秘密/多行编辑菜单；保留 macOS 屏幕系统菜单和 native accelerators。
- 原 TouchArea owner 转发右键，终端菜单打开后不再由 pointer-down 夺回输入焦点；父子菜单共用一个 popup 生命周期，鼠标可返回父菜单。
- 12 条新增会话菜单中文翻译、双语用法/架构与项目地图已同步。
- 无窗口交互回归覆盖 hover 子菜单、父菜单鼠标可达、Left 返回及工作区路径/命令同名；修正零尺寸锚点居中和紧凑侧栏坐标。

- 用户授权分组本地提交；菜单实现、测试与翻译为 `ddc1a71`，双语文档和本轮记录作为独立提交。

## 验证

- 已完成：3 项键盘扫描和 2 项无窗口菜单交互专项；完整 674 项测试（315 library + 359 binary）、fmt、locked/offline check/严格 all-target Clippy/tests、Slint 重编译；aarch64-apple-darwin 显式 check/严格 Clippy/build、538 条中文翻译、64 条 Markdown 相对链接、项目 skill validator 和 diff 通过。tracker 仍为 55 项既有历史问题，本轮 current/map 零错误且无新增。
- 未完成：TM4 与前轮用户原生验收；Windows/Linux 原生构建、真实显示和键盘/焦点体验未在本机验证。

## 风险与阻塞

- tracker 保留 55 项既有历史格式问题，本轮不得新增。
- Windows/Linux 原生菜单与键盘体验需用户在目标平台验收；本机只具备 macOS 原生 SDK。

## 下一步

- TM4 由用户在 Windows/Linux/macOS 验证菜单配色、子菜单、滚动、键盘、文本选择与焦点；保留 macOS 屏幕系统菜单及前轮原生验收。

## 最后更新时间

- 2026-10-10 18:13 +0800：TM1/TM2/TM3/TM5 完成，保留 TM4 用户原生验收；分组本地提交，不推送或操作发行标签。
