# 当前项目实施记录

## 当前目标

- 目标 ID：20261010-ssh-tab-latency
- 目标：使用已认证 SSH 连接的往返延迟更新 Tab 状态点颜色和悬停提示。
- 交付物：有界单请求采样、前后台节奏、临时延迟状态、平滑与颜色缓冲、双语提示/说明、确定性测试和本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：SSH/SFTP worker、应用临时状态和窗口路由、Slint Tab 状态点、双语文档与 tracker、本地提交。
- 不在本轮范围内：ICMP、额外 TCP 连接、远端命令、配置 schema、用户 LICENSE、推送或发行标签。
- 所有权：SSH worker 拥有认证连接和探测生命周期；应用层拥有临时样本、连接代次校验和刷新合并；Slint 拥有颜色、可访问名称和悬停文本。
- 安全边界：不改变主机信任、认证、既有保活或断线判定；最多一个未完成探测，超时后保留请求等待迟到响应或关闭。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| RT1 | completed | 核对 SSH 探测协议和现有连接/路由边界 | russh 0.64.1、RFC4254、worker/attempt guard | 使用 want-reply 的 keepalive 全局请求；成功或拒绝均是有效响应 |
| RT2 | completed | 单请求 SSH/SFTP 采样和前后台节奏 | 确定性时钟、超时/迟到/关闭、协议响应测试 | 前台 10s、后台 30s、超时 5s、过期 90s |
| RT3 | completed | 临时状态、代次校验、Tab 刷新和展示 | 平滑、阈值、重连、刷新合并、Slint 编译 | Local/Telnet/Serial 保留普通连接状态 |
| RT4 | completed | 全部自动门禁与本机链接构建 | focused tests、fmt/check/Clippy/tests/diff | 不捕获应用截图 |
| RT5 | completed | 双语说明、地图、研究/历史与本地提交 | translations/links/skill/tracker validators、index diff | 保留 LICENSE，不推送或改 tag |
| RT6 | pending | 用户原生验收延迟提示和连接生命周期 | 前后台、分屏/独立窗口、重连、真实 SSH/SFTP | GUI 验收属于用户 |
| IP5 | pending | 前轮服务器/Local Shell 选择器验收 | 宽窄窗口、常用排序/搜索、自动/手动、焦点 | 715c5dc/69b091f 已提交，657 项测试通过 |
| DI5 | pending | 前轮标题栏与 `+` 列表原生验收 | 分屏/返回/恢复、配色与列表键盘 | b6cbad7/f92c2c5 已提交 |
| PW5 | pending | 前轮逐服务器凭据清除原生验收 | 状态刷新、确认/取消、真实系统后端 | 自动验证完成 |
| UI7 | pending | 前轮导航系统图标原生验收 | 发行版/手动指定、配色、tooltip/焦点 | 自动验证完成 |

## 已完成

- 用户已授权按照评估方案实施；明确采样归属和安全边界。
- 核对固定 russh 的全局请求响应：拒绝也表示收到响应，断线不能当作有效往返；不使用忽略 oneshot 关闭结果的 send_ping 包装。

- 已实现 Terminal/SFTP 单请求采样、10/30 秒节奏、5 秒超时和 90 秒过期；迟到回复恢复，worker 释放取消 pending future。
- 已实现临时样本、20% EWMA、连续两次新档位切色、连接代次校验和重连清空；Tab-only 刷新共用有界门控，独立/分屏路由和 SFTP 焦点均覆盖。
- 已编译 Slint 颜色/悬停/可访问文本及拖动预览数据，同步双语用法和架构说明。

- 更新静默 shell 回归：只允许正常遥测事件，仍拒绝任何终端失败/断线或事件 channel 关闭；完整门禁复验通过。
- 已同步地图、协议研究和月度历史，复核本地提交内容；用户 LICENSE 修改保留，不推送或修改 tag。

## 验证

- 已完成：11 项延迟专项和现有可信 SSH/静默 shell 专项；完整 669 项测试（315 library + 354 binary）、fmt/check/严格 all-target Clippy、Slint 重编译、aarch64-apple-darwin 显式 check/严格 Clippy/build、diff；526 条中文翻译、116 项图标/5 个生成文件、696 条相对链接和 skill validator 通过。tracker 与同一历史基线比较仍为 55 项、无新增错误。
- 未完成：RT6 及前轮 IP5/DI5/PW5/UI7 原生验收。仅运行可用的 aarch64 macOS 原生环境，其他 CI 平台未在本机验证。

## 风险与阻塞

- 全局请求按 FIFO 匹配，无响应时不得取消后追加新请求。
- tracker 保留 55 项既有历史格式问题，本轮不得新增。

## 下一步

- RT6 由用户验证状态点配色/tooltip、前后台和真实服务器的延迟/重连体验；前轮原生验收继续保留。

## 最后更新时间

- 2026-10-10 15:04 +0800：实现、完整门禁、本机构建、双语文档和本地提交内容收口，保留用户原生验收。
