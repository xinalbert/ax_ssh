# 当前项目实施记录

## 当前目标

- 目标 ID：20261002-wheel-home-option
- 目标：让行首动作作用于每次纵向滚轮移动后的目标行：开启选项时，每一步发送 `Up/Down + Home`。
- 交付物：逐步移动后 Home 的有界输入策略、回归、双语设置与说明；明确远端按键绑定决定最终光标位置。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config/settings.rs`、`src/app/settings_bridge.rs`、`src/app/view/settings.rs`、`src/app/terminal_bridge.rs`、设置 Slint callback 链、翻译及相关测试/文档。
- 不在本轮范围内：依赖升级、SSH trust、凭据、发布 tag、参考项目代码、远端程序名称识别。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| WH1 | completed | 核对环境、现有输入边界并落盘计划 | manifest/build/CI/项目地图与技能规范 | 沿用上轮标准 Home 研究；用户已选默认关闭选项 |
| WH2 | completed | 设置持久化、UI/翻译与通用输入策略 | 旧配置默认值、round trip、事件次数/模式与 reporting 回归 | 保留上一轮未提交修复 |
| WH3 | completed | 完整门禁与双语使用/架构记录 | fmt/check/Clippy/test/build、翻译、链接、tracker/skill、diff | 上一版语义已完成 |
| WH4 | completed | 上版逐步 Home 语义及验证 | 定向回归、fmt/check/Clippy/test/build、翻译、链接、tracker/skill、diff | 输入顺序已由 WH5 替代 |
| WH5 | completed | 将行首动作改到每个滚轮步的目标行，同步双语文案 | 定向输入回归及翻译校验 | 保留开关；无旧输入顺序兼容分支 |
| WH6 | completed | 完整验证与构建交付 | fmt/check/Clippy/test/build、链接、tracker/skill、diff | 本机通过；GUI 和远端按键绑定由用户验收 |

## 已完成

- 核对现有工作树、项目地图、Rust/Slint 技能与架构；上一轮主窗口 callback 修复保留。
- 环境预检：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1；Slint 1.18.1、锁文件和 CI 矩阵不变。
- 配置拥有布尔偏好，application bridge 读取当前设置并复用终端模式/按键编码；不增加 model 状态或终端公开 API，不改变 SSH 安全边界。
- 已接通设置默认值/持久化/预览/保存、中英文 UI 与搜索；本轮已将逐步 Home 调整到方向键之后，同步配置注释、双语说明和项目地图。
- tmux 隔离 socket 实验覆盖不同长度行和空行，从 14,4 开始，上移三次、下移三次，每步完成后列均为 0；不修改用户 tmux 配置，实验 server 已清理。
- 已同步双语使用/架构说明与项目地图；不增加 model 状态或终端公开 API。

## 验证

- 已完成：环境预检和先计划后施工；Slint 已重编译，3 项定向回归、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 301、应用 303、Doc tests 0）、debug build、510 条翻译、8 条 Markdown 相对链接、skill/tracker 与 diff 检查通过；tmux 坐标实验通过。
- 未完成：GUI 实际行为与 Windows/Linux 原生 CI；本机未运行跨平台 GUI，不自行捕获应用截图。

## 风险与阻塞

- Home 是发给程序的输入，远端自定义按键绑定可能改变效果，不能承诺所有程序都落到第一个字符。保留默认关闭选项和鼠标上报优先规则，不修改绘制层光标坐标。
- Windows/Linux SDK 与原生运行环境本机不可用；GUI 视觉验收依赖用户确认，不自行截图。

## 下一步

- 用户重启最新 debug 构建，在“设置 > 终端 > 鼠标”开启“滚轮时保持光标在行首”，验收目标程序的滚轮行为；远端自定义按键绑定可能影响 Home 的效果。

## 最后更新时间

- 2026-10-02 +0800
