# 持续终端重绘光标：来源记录

访问日期：2026-11-02。只采用官方规范、作者文档和原项目源码支撑实现判断。

| Source | Source type | Query / path | Access date | Status | Included / excluded reason | Evidence use |
| --- | --- | --- | --- | --- | --- | --- |
| [st config.def.h](https://git.suckless.org/st/file/config.def.h.html) | 官方源码 | minlatency / maxlatency | 2026-11-02 | included | 当前默认 2ms/33ms，同时说明过早判空会闪烁 | 静默等待与硬期限的权衡；不照搬默认参数 |
| [st x.c](https://git.suckless.org/st/file/x.c.html) | 官方源码 | run / maxlatency | 2026-11-02 | included | 展示在上下限之间尝试等待空闲、耗尽预算仍绘制的实际循环 | 有上限的合并，不无限等待输出停止 |
| [kitty.conf](https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.input_delay) | 作者官方文档 | input_delay / repaint_delay | 2026-11-02 | included | 明确说明过快绘制会显示部分更新；小输入有延迟豁免 | 延迟和闪烁存在权衡；不采用延迟 parser 的实现方式 |
| [Contour synchronized output](https://contour-terminal.org/vt-extensions/synchronized-output/) | 原协议实现文档 | semantics / CSI ?2026h/l | 2026-11-02 | included | 同步期间继续解析而显示上一状态，结束后发布最新状态 | 保留 AxSSH 现有显式同步语义和超时 |
| [alacritty_terminal 0.26.0 event_loop.rs](https://docs.rs/crate/alacritty_terminal/0.26.0/source/src/event_loop.rs) | 锁定版本原项目源码 | MAX_LOCKED_READ | 2026-11-02 | navigation-only | 本轮未增加上游 read batching，已有 AxSSH 64 事件预算继续保留 | 无新增定量判断 |
| [foot 官方仓库](https://codeberg.org/dnkl/foot) | 官方源码入口 | delayed-render-lower / delayed-render-upper | 2026-11-02 | excluded | Web 检索受 robots 限制，未读取原文 | 不据此做事实或参数判断 |
| [TerminalBench](https://github.com/simota/TerminalBench) | benchmark 项目 | terminal cursor flicker benchmark synchronized output latency | 2026-11-02 | excluded | 搜索结果说明相关指标主要衡量 PTY 完成，无法直接确认屏幕半帧/光标跳动；本轮不作跨终端性能排序 | 证据缺口：未检索到与本需求直接对应的统一公开 benchmark |
