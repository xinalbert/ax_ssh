# 持续重绘光标的呈现策略调研

## 需求定义

- 输入：持续 TUI 输出，移动光标、绘制内容和复位可能落在独立的 transport 批次中。
- 输出：有界的完整可见快照与真实协议光标；内容、颜色、显示字符和光标位置一起发布。
- 约束：保留单一 cursor_state；parser、输入、协议应答即时；不根据程序名/屏幕内容猜测输入位置；不让持续输出无限推迟呈现；不新增无界队列、框架或依赖。
- 指标：确定性时序下中间位置是否被发布、首个 pending 输出到准入的延迟上界、脏行是否丢失、慢 UI 是否仍消费过时已准入帧。

## 检索范围与纳入标准

只纳入原项目官方源码、作者文档和协议说明。访问日期与处理状态见 [source-tracking.md](source-tracking.md)。

## 候选方法概览

| 方法 | 证据 | 对 AxSSH 的适用性 |
| --- | --- | --- |
| 显式同步输出 | [Contour](https://contour-terminal.org/vt-extensions/synchronized-output/) 说明同步期间继续解析并保留上次画面，结束后取得最新 buffer | 已有能力，必须保留；不能覆盖未发送 2026 的 TUI |
| 固定帧期限 | 仓库 src/app/terminal_presentation.rs | 修改前首批约 16.7ms，持续阶段可能到数据就发布；无法合并跨过旧期限的光标复位 |
| 有上限的静默合并 | [st config](https://git.suckless.org/st/file/config.def.h.html) 和 [事件循环](https://git.suckless.org/st/file/x.c.html) | 采用方法思路，以原有帧预算为参数依据，独立实现；不复制上游源码 |
| 延迟处理程序输出 | [kitty 配置](https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.input_delay) 描述 input_delay 的速度/半帧权衡 | 仅作机制证据；AxSSH 的 parser 和协议应答保持即时 |
| 更新尚未显示的已准入快照 | 仓库 state/terminal.rs 的单槽 pending 和 merge_terminal_snapshots | 不增加队列；每次新准入将最新状态与旧 damage 合并，后续未准入输出继续隔离 |

## Benchmark / 公开评测对比

未检索到与该需求直接对应的统一公开 benchmark。本轮使用官方实现作为 proxy evidence，并以相同 AxSSH 代码上的确定性回归比较修改前后行为；不报告终端排行榜、主观评分或真实屏幕性能数据。

同一批新增回归先在未修改生产实现上运行，再在修复后运行：

| 确定性场景 | 固定期限旧实现 | 修复后 |
| --- | --- | --- |
| 持续阶段上一帧期限已过，8ms 后收到光标复位 | 失败：首段立即就绪 | 通过：复位一起发布 |
| 首段后 12ms 仍有内容，复位跨过原 16.7ms 期限 | 失败：软期限提前就绪 | 通过：延后合并且保持硬期限 |
| 慢 UI：第二次准入后又有未准入片段 | 失败：显示旧光标 (0,5) | 通过：显示最新已准入光标 (23,2)，旧 damage 保留 |

以上是确定性 parser/application 状态回归，不是屏幕帧率或用户视觉测量。原实现日志保存在忽略的 target/cursor-redraw-investigation/red-tests.log，首轮修复后 7 项 redraw 相关测试通过（包含三项新回归）；随后 16 项调度、103 项状态专项及 404 library + 516 binary 共 920 项完整测试通过，fmt/check/严格 Clippy 无 warning。

## 推荐结论

工程选择：每个聚焦 pending 批次等待最多一个 nominal 60Hz 帧间隔的静默，静默长度随更高配置 FPS 缩短；从本批首个 pending 起最多合并两倍该长度。实际准入仍不能早于既有配置/持续输出帧率期限。默认 60 FPS 下分别约 16.7ms 和 33.3ms。这里是 AxSSH 的工程参数，不是 st/kitty 默认值，也不是公开 benchmark 得出的最优值。

持续无静默输出到达硬期限仍发布；只有显式 2026 才额外持有同步帧。未聚焦上限和 hidden 策略保持既有契约。慢 UI 下更新同一个 pending 快照并合并 damage；不把尚未准入的 fragment 混入画面。

## 不确定项与不可比项

有限合并窗口不能识别任意长网络间隔后的最终输入位置，超过预算的非同步 TUI 仍可能显示中间状态。持续高频输出可能更接近约 30 FPS（默认合并预算），既有 FPS 是上限而非保证；GPU 持续输出自适应和软件配置上限继续生效。实际程序、传输方式和 GUI 视觉仍待用户验收；不截图，不将协议回归等同于视觉证明。

## 参考来源

完整来源、访问日期与排除原因见 [source-tracking.md](source-tracking.md)。
