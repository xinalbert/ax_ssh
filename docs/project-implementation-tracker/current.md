# 当前项目实施记录

## 当前目标

- 目标 ID：20261016-x11-authentication-isolation
- 目标：把远端 X11 setup 的逐连接 cookie 校验移到本机 X server 准备之前，避免无效请求触发本机探测或自动启动。
- 交付物：SSH transport 内的有界认证门禁、确定性回归、双语架构/使用说明、研究/实施记录和两笔本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/ssh/x11.rs`、必要的 SSH 回归、双语架构/使用说明和实施记录。
- 不在本轮范围内：服务器配置修改、用户 profile/default 变更、禁用系统 launchd、UI 修改、依赖升级、推送或安装版替换。
- 所有权：SSH transport 拥有 channel、cookie、setup 校验和本机准备顺序；现有 XServerPlan 仍负责平台发现/启动，UI 不接触认证数据。
- 安全：先确认 SSH channel 以允许发送 X11 setup，再在长度/超时上限内核验当前连接的 cookie；通过前不触碰本机 DISPLAY/xauth/socket/provider。无效请求只结束自己的 relay，不中断 SSH，不放宽主机密钥或本机授权。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| XI1 | completed | 当前链路、现场日志和标准边界 | RFC/OpenSSH/sshd_config/XQuartz/launchd 与本机源码 | 错误 cookie 日志不能识别远端 UID/PID；OpenSSH 也先连接本机 display |
| XI2 | completed | 认证先于本机副作用的最小实现 | 有界 setup、零化 cookie、取消/失败和 untrusted 到期边界审阅 | 保留 untrusted 在 SSH 请求前生成受限授权的必要准备 |
| XI3 | completed | 认证隔离及真实 relay 回归 | 错误/跨连接 cookie、畸形/截断/超时、分片/双字节序及数据完整性 | 注入准备步骤和 loopback 假本机 peer；13 项聚焦测试通过 |
| XI4 | completed | 仓库 Cargo 门禁 | focused/fmt/check/严格 all-target Clippy/test/diff | 399 library + 481 binary 共 880 项通过；本机 ARM64 macOS |
| XI5 | completed | 双语说明、地图和跟踪收尾 | 12 条相对链接、skill/current/map/新增记录及完整 diff 审阅 | 完整 tracker 的 84 项历史错误无新增；原生/真实 X11 待验收 |
| XC1 | completed | X11 修复、回归和双语说明提交 | 显式暂存并审阅完整 staged diff | 5a9c6d2；沿用上一轮已通过的源码，不重复测试或编译 |
| XC2 | completed | 研究/实施记录提交及收尾 | current/map/新增记录、相对链接、diff 和最终状态 | 本记录随第二笔提交归档；仅本地提交，不推送 |

## 已完成

- 干净基线为 `b0fe55c`；前轮六项依赖升级已单独提交，不混入本次。
- 基线 trusted 请求到达后先准备/探测本机 server，再校验 setup；连接 launchd DISPLAY socket 本身也可能启动 XQuartz。现已将认证前移到所有逐通道本机准备之前。
- RFC 要求 fake cookie 校验/替换，未要求本机连接与认证的具体先后；OpenSSH 同样先连接 display。本轮收紧为 AxSSH 的防副作用隔离，不误称 OpenSSH 已采用该顺序。
- 每个 trusted 连接已有随机 cookie，队列和活动 relay 各限八个；可沿用这些边界完成最小顺序调整。
- 新增五项认证隔离/relay 回归，使用内存分片流和随机 loopback 端口的假本机 peer；不访问真实 DISPLAY、xauth 或启动图形程序。
- 配套双语架构/使用说明和地图已更新，明确错误 cookie 不能触发本机准备，以及正确 cookie、untrusted 预授权和服务器 UID 隔离的边界。
- 用户授权提交后，修复/回归/双语说明已提交为 `5a9c6d2`；剩余四份研究/实施记录独立归档，源码未再修改，本轮未重复测试或编译。

## 验证

- 已完成：13 项 X11 聚焦测试（新增五项）和最终源码的 880 项全量测试（399 library + 481 binary）通过；fmt、locked/offline check、严格 all-target Clippy 和 diff 检查通过。错误/跨连接 cookie、畸形/截断/超时/取消、大小端逐字节分片及假本机 TCP peer 的授权替换/no-auth/后续数据/双向 EOF 均已验证。八份 Markdown 的 12 条相对链接和项目 skill/current/map/新增记录通过；完整 tracker 的 84 项历史错误集未变化。
- 未完成：真实 XQuartz/远端图形应用手工验收及 Windows/Linux 原生 CI；不将本机假 peer 结果扩大为原生图形启动验收。

## 风险与阻塞

- 客户端只能确认持有此连接 cookie，无法识别远端真实 UID/PID；持有正确 cookie 的后台进程仍可触发启动。
- `X11UseLocalhost yes` 只限制服务器 loopback，并不独立实现同机按用户隔离；服务器权限/namespace 改造不属于本轮客户端改动。
- Untrusted 生成受限授权本身需要本机 server，仍可能在建立 shell 前启动；不以隔离名义降级至 trusted/no-auth。
- 本机仅可原生执行 ARM64 macOS，真实 XQuartz/Windows/Linux GUI 生命周期需外部验收；不读取自建应用截图作为证据。

## 下一步

- 本记录独立提交后核对两笔提交及最终工作区；真实 X11 和 Windows/Linux 验收边界保留。本轮不推送或替换安装版。

## 最后更新时间

- 2026-10-16 15:15 +0800
