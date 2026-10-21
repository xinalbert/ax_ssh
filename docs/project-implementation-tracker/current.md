# 当前项目实施记录

## 当前目标

- 目标 ID：20261021-terminal-cursor-ime
- 目标：修复反色空格模拟光标消失、光标样式更新遗漏和隐藏远端光标时本地拼音预编辑不可见。
- 交付物：终端模型/应用桥接/Slint 修复、确定性回归、双语架构和完整验证记录；用户验收后分组本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/terminal/render.rs`、终端渲染回归、`src/app/view/terminal.rs`、终端 Slint 组件及无窗口回归、双语架构和实施记录。
- 不在本轮范围内：依赖/工具链升级、协议显隐语义变更、transport/SSH/凭据、vendor、发布、推送或安装版替换。
- 所有权：终端模型保留有可见样式的空格；应用桥接发布完整光标状态；Slint 独占本地 IME/preedit 和绘制。
- 安全：保持主机密钥默认拒绝与短期凭据边界；预编辑不发送到 worker，不记录输入内容，不捕获应用截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 反色空格保留及光标更新修复 | ANSI 到实际 snapshot/render 的回归；光标独立更新与模型 identity 回归 | 不按 Claude/Codex 程序名特判 |
| P2 | completed | 隐藏远端光标时仍显示本地拼音预编辑 | 主/独立窗口的真实 composition 事件、可见项和提交路径回归 | 不改变远端 DECTCEM 状态 |
| P3 | completed | 双语架构、项目地图和本机自动验证 | fmt/check/严格 Clippy/test/build、链接/skill/tracker/diff | GUI 视觉及原生输入法由用户验收 |
| C1 | completed | 反色空格修复与 ANSI 到绘制回归提交 | 显式暂存、完整 staged diff/check | `42492f1` |
| C2 | completed | 光标状态更新和本地预编辑修复提交 | 显式暂存、完整 staged diff/check | `04d09d8`；保留焦点及模型 identity |
| C3 | completed | 双语架构、地图和验收记录提交 | 链接/skill/tracker/diff 与最终干净工作区 | 本记录随第三笔归档；提交后核对工作区 |

## 已完成

- 已读取项目技能、地图、架构、CI 与相关生产/测试源码；修改前工作区干净。
- 用户从只读排查切换为明确修复授权；已检查两张用户提供的截图，候选词面板存在而 Claude 输入区缺少本地预编辑文本。
- 已确认三个源码缺陷：空白过滤遗漏 inverse；光标 shape/blinking/color 更新未被外层比较及 UI 订阅完整消费；preedit 可见性错误依赖远端 cursor_visible。
- 环境快扫：Rust 1.99.0，MSRV/依赖声明保持；已装 ARM64 macOS 和 Windows MSVC target，本机验证使用 ARM64 macOS。
- 三处修复已实现：保留反色空格，所有光标字段订阅保留单行模型，preedit 独立于远端显隐；双语架构及项目地图已同步。
- 用户已确认“修复了”，本轮视觉/原生输入法修复验收通过，并授权分组本地提交。
- 实现与回归分为两笔提交 `42492f1`、`04d09d8`；双语架构和实施记录随第三笔归档。七份 src/ui 文件指纹 `be6815fc7189ab5c3ec19469bef8eeebfd9c1b9d3665559d80f6b6b2365b192c` 提交前后一致。

## 验证

- 已完成：修改前三项新增回归均复现缺陷；修复后 14 项光标专项及完整 399 library + 492 binary 共 891 项 Rust 测试通过。fmt、locked/offline check、严格 all-target Clippy、locked/offline build、53 份 Markdown/入口、skill/tracker/diff 检查通过。Slint 已重新编译，产物为 `target/debug/ax_ssh`。
- 未完成：Windows/Linux/Intel macOS 原生 CI。

## 风险与阻塞

- 用户已确认本轮修复有效；本机自动验证和用户验收不替代其它平台原生 CI。
- 无用户实际终端字节捕获，不把源码触发条件等同于已确认 Claude 每个版本的具体画法。

## 下一步

- 本记录随第三笔归档后核对三笔提交和最终干净工作区；其它目标由现有原生 CI 验证。本轮只做本地提交。

## 最后更新时间

- 2026-10-21 16:03 +0800
