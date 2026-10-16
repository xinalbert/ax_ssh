# 当前项目实施记录

## 当前目标

- 目标 ID：20261016-windows-wheel-peer-close
- 目标：修复 Windows CI 滚轮接收测试在全部断言通过后，因主动关闭返回 ConnectionAborted 而失败的问题。
- 交付物：Windows 收尾错误分类、提前断线/平台边界回归、双语开发说明、静态检查及分组本地提交；沿用用户不运行测试的要求。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/behavior_tests/terminal_wheel.rs`、双语开发说明与 `docs/project-implementation-tracker/`。
- 不在本轮范围内：生产 SSH shutdown、鼠标/滚轮/选区实现、信任/认证/凭据、配置/依赖/工具链、CI 矩阵、安装版替换、推送和发布。
- 所有权：测试夹具在所有接收/视口断言通过后标记主动关闭，服务端结束时采样；错误放行仅属于该夹具，不进入生产 transport。
- 安全：保留真实 host-key 校验、固定测试凭据、有界队列和超时；提前断线、其它 IO/协议错误、任务 panic 仍失败。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | Windows 失败阶段与错误码定位 | 用户原生 CI 日志、当前 helper 与 Microsoft Winsock 定义 | 365 library 通过，binary 453/454 通过，失败点为 finish 结果 |
| P2 | completed | Windows 主动收尾错误补齐 | 仅 Windows 且已开始收尾时接受 ConnectionAborted | 其它平台及收尾前仍拒绝，保留 ConnectionReset 既有处理 |
| P3 | completed | 双语说明与静态门禁 | fmt、locked/offline check、严格 all-target Clippy、链接/tracker/diff | 不运行测试，不声称 Windows 分支已经本机验证 |
| P4 | pending | Windows 原生 CI 验证 | 同名接收矩阵及错误边界用例原生运行 | 本机 macOS 无 Windows 执行环境；不得替代现场 Codex 复现 |
| C1 | completed | 夹具修复与双语说明提交 | 显式暂存、完整 staged diff 审阅 | `09c8dad`；源码保持前轮静态检查版本，不运行测试或重编译 |
| C2 | completed | 本轮研究/实施记录归档 | 链接/tracker/diff 和提交边界检查 | P4 保持 pending，记录独立提交后核对最终工作区 |

## 已完成

- 本轮基线 `651d56a`，工作区干净；双击重复选区修复已获用户确认并提交。
- 用户提供原生 Windows MSVC CI 日志：全部滚轮字节/视口断言走完，主动关闭后服务端返回 IO 10053 / ConnectionAborted，finish 的 expect 报错；主窗口接收矩阵通过。
- 既有 helper 只接受显式关闭后的 ConnectionReset，未覆盖 Windows 此次返回的 ConnectionAborted；Microsoft 区分 10053 与 10054，不能把所有此类错误一概视为成功。

- 夹具现仅在 Windows 且已开始主动收尾时接受 ConnectionAborted，既有跨平台 ConnectionReset 处理保留；提前 abort/reset 和其它错误仍失败。边界用例补齐 Windows raw 10053 与非 Windows 拒绝断言，但按用户要求没有执行。
- fmt、locked/offline check 和严格 all-target Clippy 已通过，无 warning；双语开发说明、地图及研究/实施记录已同步，生产源码未修改。

- 用户授权“做些提交吧”；七份工作区改动均属该夹具修复与配套记录，索引为空，缓存和构建产物已忽略。按修复/双语说明与研究/实施记录分两笔本地提交。

- 夹具修复与双语开发说明已提交为 `09c8dad`，提交前完整审阅暂存差异；随后仅剩四份研究/实施记录，本轮未重跑测试或编译。

## 验证

- 已完成：P1/P2 证据与错误分类审阅；P3 fmt、locked/offline check、严格 all-target Clippy 和链接/current/map/新增记录/diff 静态检查。
- 未完成：P4 Windows 原生 CI 及所有新回归的实际执行；按用户要求不运行测试，不沿用前轮通过数，不以 macOS 编译替代 Windows 结果。

## 风险与阻塞

- 本机仍为 macOS；Windows SDK/执行环境不可用，不能声称已在 Windows 重跑。
- 用户日志没有提供 revision/run URL，不推断它对应本机 HEAD；失败 helper 和错误与当前源码相符。
- 当前问题属于测试夹具的收尾判据，不能作为 Windows 实际鼠标/Codex 问题已经修复的证据。
- 完整 tracker 有 84 项历史格式问题，本轮 current/map/新增记录单独验证并比较错误集；其它前轮待验收事项保留。

## 下一步

- 本记录独立提交后核对最终工作区和两笔提交清单；P4 仍由后续 Windows 原生 CI 验证，本轮不推送、不运行测试或重编译。

## 最后更新时间

- 2026-10-16 10:53 +0800
