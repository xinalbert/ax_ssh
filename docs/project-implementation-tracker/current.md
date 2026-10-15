# 当前项目实施记录

## 当前目标

- 目标 ID：20261015-terminal-undo-guard
- 目标：修复终端按 Command+Z 时透明输入代理执行本地撤销引发的退出，并按结果创建本地提交。
- 交付物：终端撤销/重做 fallback 隔离、主/独立窗口真实按键回归、普通字段编辑兼容性、双语说明、本机构建和本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/terminal-pane.slint`、`src/app/view/tests.rs`、`src/app/view/tests/text_editing.rs`、`src/app/view/tests/terminal_input.rs`、双语架构/使用说明及 `docs/project-implementation-tracker/`。
- 不在本轮范围内：SSH trust/认证/凭据、传输协议、配置 schema、依赖/工具链、通用输入框编辑实现、全局快捷键注册、安装程序替换、推送和发布。
- 所有权：Slint 终端输入代理负责阻止未处理的本地撤销/重做；应用层和终端编码器继续负责平台按键意图及远端字节。没有新增跨模块接口或编辑状态。
- 安全：不记录终端或剪贴板内容，不增加队列/缓冲，不修改安全提示、连接前输入门槛或 host-key 验证。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| U1 | completed | 崩溃证据、独立复现及输入所有权核对 | 14:56 报告、用户 Command+Z 操作、锁定 Slint 空文本撤销越界复现 | 安装版 UUID 匹配；不将当前源码复现等同于安装版完整符号栈 |
| U2 | completed | 终端 fallback 修复与定向回归 | 主/独立窗口实际普通/IME 提交、两个 fallback、Control 字节、配置 Paste 及普通字段撤销/重做通过 | 修改前两项实际终端回归均越界失败，修复后通过 |
| U3 | completed | 完整门禁、双语记录与本机构建 | 870 项测试、fmt/check/严格 Clippy/build、翻译/链接/tracker/diff 通过 | 本机 macOS ARM64；其它平台交原生 CI |
| U4 | pending | 用户原生验收 | 新构建内普通/IME 输入、Command+Z/Shift+Z、Control+Z 和独立窗口 | 不截取代理应用图像；不主动连接真实 SSH |
| C1 | completed | 审阅并提交修复与配套双语说明 | 显式暂存、逐项审阅 staged diff 和提交后状态 | `5e0255d`；沿用已通过的 870 项测试、严格 Clippy/build，不改源码 |
| C2 | completed | 整理本轮归档记录 | 链接/tracker/diff、提交边界和记录审阅 | 本记录单独提交后独立核对最终工作区，U4 保持 pending |

## 已完成

- 本轮开工时前轮五项本地提交已完成、工作区干净；终端帧合并、Windows 地址、独立窗口快捷键及侧栏的用户原生验收继续 pending。
- 找到 2026-10-15 14:56:21 的主线程 SIGABRT 报告，二进制 UUID 与安装版一致；用户确认退出前按 Command+Z。
- 使用锁定 Slint 1.18.1 在独立无原生窗口的临时程序中复现：输入后在 edited 清空，再按撤销导致空字符串索引越界；正常字段同样操作通过。临时诊断程序已清理。
- TerminalPane 的配置编辑快捷键和终端字节回调未处理时会回退到透明 TextInput；该代理只用于输入/IME，发送后清空，不应执行本地撤销/重做。
- 已读取项目技能、地图、实现与验证入口；不升级 Slint 或改变普通文本 wrapper 的原生编辑契约。
- 主/独立窗口的实际 TerminalPane 回归在改动前均复现空字符串索引越界；修复后两项通过，两个普通字段 Unicode 撤销/重做回归也通过。
- 在配置编辑动作和可编码终端键优先之后，两个 fallback 消费未处理的主修饰键 Z/Y。物理 Control+Z/Y 分别保持 0x1a/0x19；自定义主修饰键 Z 为 Paste 仍只执行一次。
- 回归夹具补齐正式 Winit hook 提供的事件级修饰键快照，避免无原生窗口的测试读取宿主 AppKit 当前按键状态；不修改运行时归一化实现。
- 双语架构/使用说明和地图已同步；665 × 11 翻译/生成与 8 项脚本测试、12 个相对链接及 current/map/本轮新增记录通过，完整 tracker 84 项旧问题无新增。
- 完整 fmt、locked/offline check、严格 all-target Clippy、392 library + 478 binary（870）测试和本机 debug build 通过，无 warning；新程序为 `target/debug/ax_ssh`，未启动原生应用或真实 SSH。
- 用户追加授权创建提交；初始 index 为空，11 份已跟踪文件和 1 份新增测试属于同一修复，构建产物与缓存已忽略；按功能及项目记录分为两笔提交。
- 修复、实际组件回归和双语说明已提交为 `5e0255d`；提交后仅剩四份项目记录，归档内容已整理，最终工作区核对在本记录提交后独立执行。

## 验证

- 已完成：两项实际终端回归修复前失败/修复后通过、两项普通输入 Undo/Redo 回归；Slint 实际重编译，fmt/check/严格 Clippy/870 项测试/build/diff；665 × 11 翻译/生成、8 项脚本回归、12 个相对链接及 current/map/本轮新增记录验证。完整 tracker 84 项历史问题无新增。
- 未完成：U4 用户原生 GUI/IME/真实 SSH 验收；其它平台的原生 CI；前轮待验收事项保留。

## 风险与阻塞

- 安装版去除部分符号且专用 panic 文件为空；根因以同触发条件的确定性复现及输入链路交叉确认，不声称恢复了原始第一层 panic 文本。
- macOS 的 Slint control 表示 Command，物理 Control 仍需正常发送终端控制字节；Windows/Linux Ctrl+Z/Y 同样不得被一概吞掉。
- 完整 tracker 已有 84 项历史格式问题；本轮 current/map/新增记录单独验证并比较错误集。

## 下一步

- 本记录独立提交后核对最终工作区和两笔提交清单；用户使用新构建复验主/独立窗口按键与 IME，现有安装版更新后才包含修复。

## 最后更新时间

- 2026-10-15 16:22 +0800
