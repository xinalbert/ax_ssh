# 当前项目实施记录

## 当前目标

- 目标 ID：20261016-terminal-selection-owner
- 目标：修复本地选择优先模式中双击既转发给远端、又建立本地选区导致的重复高亮。
- 交付物：统一左键手势归属、移除释放补发点击、双语说明、编译和静态检查、用户双击验收及分组本地提交；按用户要求不运行测试。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/terminal-pane.slint`、双语使用/架构说明及 `docs/project-implementation-tracker/`。
- 不在本轮范围内：词边界算法、远端 Codex/tmux 实现、滚轮行为、配置 schema/用户设置、SSH trust/认证/凭据、依赖、安装版替换、推送和发布。
- 所有权：TerminalPane 在按下时确定本地或远端 owner；释放与后续双/三击沿用该归属。TerminalGrid 仍只识别手势，TerminalModel 仍计算边界并提取文字。
- 安全：不新增日志、队列、远端命令或剪贴板读取，不修改 SSH 安全与连接输入门槛。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| S1 | completed | 事件链与高亮来源核对 | 用户截图、保存的单个偏好值、释放补发与双击回调源码 | 本地优先已开启；本地 overlay 与远端背景可叠加 |
| S2 | completed | 本地/远端手势独占 | 删除 local-click-candidate；按下时记录归属，转发前清除本地选区 | 主/独立窗口共用 TerminalPane；不升级依赖 |
| S3 | completed | 双语文档与静态检查 | Slint 重编译、fmt/check/严格 Clippy、链接/tracker/diff | 不运行测试或截取应用图像；原生操作由用户验收 |
| S4 | completed | 用户双击问题验收 | 用户明确回复“修复了” | 只确认本次问题；未扩大为全部修饰键、窗口和平台矩阵验收 |
| C1 | completed | 修复与双语行为说明提交 | 显式暂存、完整 staged diff 审阅 | `129c026`；源码保持不变，不再运行测试或编译 |
| C2 | completed | 实施记录与用户确认归档 | 记录用户验收、链接/tracker/diff 和提交边界检查 | 本记录独立提交后核对最终工作区 |

## 已完成

- 当前基线为 `612042c`，开工工作区干净；上一轮 SSH 接收测试收尾修复已提交。
- 前轮只读定位已确认：本地优先模式的每次 stationary release 会补发远端 press/release，随后 double-clicked 又请求本地语义范围；截图中 Codex 复制提示与深浅背景符合两层叠加。
- 用户已授权修改；选择本地优先模式下普通左键单/双/三击及拖选由本地独占，Alt/Option 转发给启用 mouse reporting 的远端，默认标准模式保持既有规则。
- 已移除 local-click-candidate 及释放补发，last-pointer-forwarded 在按下时记录 owner，双/三击沿用；远端按下先清理本地选区。修改只在共用 TerminalPane，双语使用/架构与地图已同步。
- 本机 macOS ARM64 的 fmt、locked/offline check 和严格 all-target Clippy 通过，Slint 已重新编译且无 warning；没有运行测试、启动应用/真实 SSH 或修改用户配置。

- 用户确认“修复了”并授权创建提交；索引为空，八份改动均属于本次修复及配套说明/记录，构建产物和缓存已忽略。

- 修复及四份双语说明已提交为 `129c026`，逐项审阅暂存差异；提交后仅剩三份实施记录，本轮未重跑测试或编译。

## 验证

- 已完成：S1/S2 源码与手势归属审阅；S3 fmt、locked/offline check、严格 all-target Clippy、Slint 重编译、链接/tracker/diff 检查；S4 用户确认双击问题已修复。
- 未完成：完整修饰键/窗口/平台矩阵未单独确认，其它平台编译交原生 CI；遵用户要求未运行测试，不能沿用前轮测试数作为本次验证。

## 风险与阻塞

- 本地优先开启时，普通 stationary click 将不再转发；需要点击远端 TUI 控件时使用 Alt/Option。
- 既有远端已绘制的选区不能由本地透明 overlay 擅自擦除，本次修复阻止同一次新手势重复处理。
- 前轮 Windows 滚轮现场根因、新代码 Windows 原生门禁及其它用户验收仍待确认；完整 tracker 有 84 项历史格式问题，本轮单独验证 current/map/新增记录并比较错误集。

## 下一步

- 本记录独立提交后核对最终工作区和两笔提交清单；完整窗口/修饰键/平台矩阵继续按需验收，本轮不推送或改写历史。

## 最后更新时间

- 2026-10-16 10:46 +0800
