# Base 计划：终端协议兼容性基线

## 状态与使用边界

- 目标 ID：20261127-terminal-protocol-base
- 阶段：本机与交叉自动化已完成，等待原生人工验收。
- 开工判定：允许开工。2026-11-27 归档后，用户已指示按本计划顺序执行。
- 交付物：兼容矩阵、稳定复现、选定子集的实现与回归、双语说明，以及暂缓项的依据。
- B0–B8 的当前实施状态见 [current.md](../../current.md)。原矩阵保留起始观察，末尾 B7/B8 记录实施后变化。

## 核对口径

- 观察日期：2026-11-27；对象是当时工作区，不是某个发布版本。未来实施前重读源码和锁文件。
- [Cargo.toml](../../../../Cargo.toml)、[Cargo.lock](../../../../Cargo.lock) 锁定 `alacritty_terminal 0.26.0`、`vte 0.15.0`、`unicode-width 0.2.3`；这些是观察事实，不是升级建议。
- 原始 crate 包 SHA-256 与归档时锁文件一致：Alacritty 为 `bda177466b9524d59f1b12f0dd30b68696788e9992a7e959021c4a0ed96fcf59`，VTE 为 `a5924018406ce0063cd67f8e008104968b74b563ee1b85dde3ed1f7cb87d3dbd`。本轮 B2/B4 使用校验后的 VTE 0.15.0 本地补丁；初始矩阵以原始版本为观察对象。
- `CSI` 是控制序列入口；`SGR` 是 `CSI … m` 属性指令。原讨论中的“SGT”按 SGR 理解。DEC VT 是传统终端行为集合，xterm 文档汇总传统指令和扩展；Unicode 的编码、字素、宽度和方向分别核对。
- 区分解析识别、模型状态、UI 呈现、输入编码、同 transport 应答。底层认识某指令不等于应用端到端支持；TERM 或设备属性标识也不证明完整 xterm/VT 型号兼容。
- “有”表示静态代码路径接通；“部分”表示范围/语义有缺口；“无”表示缺失或不生效；“策略禁用”表示有意不开放；“待验证”表示执行证据不足；“保留”不计功能缺失。
- 证据分为代码核对、已有测试但本次未重跑、静态推导待复现、未来验收要求。初始归档未运行专项；后续自动化、本机 PTY 与平台构建结果见 B7/B8。
- 数字指令并非连续分配。资源上限、权限确认和厂商扩展取舍不自动构成协议错误；应将政策边界与语义差异分开。

## 标准与上游来源

| ID | 来源 | 用途 |
| --- | --- | --- |
| S1 | [XTerm Control Sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) | CSI/SGR、ESC、DEC 私有模式、OSC 和查询主表 |
| S2 | [VTE 分派](https://github.com/alacritty/vte/blob/v0.15.0/src/ansi.rs)、[parser](https://github.com/alacritty/vte/blob/v0.15.0/src/lib.rs)、[0.15.0 crate](https://static.crates.io/crates/vte/vte-0.15.0.crate) | 参数解释、默认空 Handler 和 7 位控制边界；固定包是锁定源码依据 |
| S3 | [Alacritty Handler](https://github.com/alacritty/alacritty/blob/v0.17.0/alacritty_terminal/src/term/mod.rs)、[0.26.0 crate](https://static.crates.io/crates/alacritty_terminal/alacritty_terminal-0.26.0.crate) | 模式、属性、字符宽度、应答、剪贴板；固定包是锁定源码依据 |
| S4 | [OSC 8 提案](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) | 链接起止、URI、id 分组及安全策略 |
| S5 | [VS Code Shell Integration](https://code.visualstudio.com/docs/terminal/shell-integration#_supported-escape-sequences) | OSC 133/633 的命令生命周期及目录属性 |
| S6 | [iTerm2 扩展](https://iterm2.com/documentation-escape-codes.html)、[WezTerm OSC 表](https://wezterm.org/escape-sequences.html#operating-system-command-sequences) | OSC 7/9/777/1337 等常见扩展，不作为全部实施承诺 |
| S7 | [同步输出](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md) | 私有模式 2026 呈现事务和能力查询 |
| S8 | [Kitty keyboard](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) | 扩展键盘协议与传统 CSI/SS3 的边界 |
| S9 | [UAX #29](https://www.unicode.org/reports/tr29/)、[UAX #11](https://www.unicode.org/reports/tr11/)、[UAX #9](https://www.unicode.org/reports/tr9/) | 字素、宽度、双向文本；字符宽度属性不能直接替代终端适配 |

## 源码与所有权入口

| ID | 入口 | 阅读目的 |
| --- | --- | --- |
| E1 | [terminal.rs](../../../../src/terminal.rs)、[model.rs](../../../../src/terminal/model.rs) | DTO、持续 parser、模式、队列、查询及额外鼠标/尺寸扫描 |
| E2 | [render.rs](../../../../src/terminal/render.rs)、[selection.rs](../../../../src/terminal/selection.rs) | 样式、颜色、宽/零宽字符、URI、格坐标、选区复制 |
| E3 | [terminal/input.rs](../../../../src/terminal/input.rs)、[app/input.rs](../../../../src/app/input.rs) | 按键、SS3、粘贴、平台映射，不能由 parser 推断输入能力 |
| E4 | [output.rs](../../../../src/app/terminal_bridge/output.rs)、[clipboard.rs](../../../../src/app/terminal_bridge/clipboard.rs)、[tabs/terminal.rs](../../../../src/app/state/tabs/terminal.rs) | 同连接应答、Tab 标题、OSC 52 授权与撤销 |
| E5 | [terminal_render.rs](../../../../src/app/terminal_render.rs)、[typography.rs](../../../../src/app/terminal_render/typography.rs)、[terminal-grid.slint](../../../../ui/components/terminal-grid.slint) | 属性是否绘制、字体角色、Emoji 合并、格宽及 UI 文字布局 |
| E6 | [模型测试](../../../../src/terminal/tests.rs)、[输入测试](../../../../src/terminal/input/tests.rs)、[渲染测试](../../../../src/app/terminal_render/tests.rs)、[排字测试](../../../../src/app/view/tests/terminal_typography.rs) | 已有确定性回归，本轮未重跑 |
| E7 | [Terminal 设置](../../../../src/config/settings/terminal.rs)、[架构](../../../architecture.md)、[中文架构](../../../architecture.zh.md)、[AGENTS.md](../../../../AGENTS.md) | 默认值、模型/应用/UI/worker 所有权、安全约束 |

## CSI 与查询

以下结合 S1–S3、E1–E4；应答共同受有界队列约束。

| ID | 指令或能力 | 状态 | 当前行为或缺口 |
| --- | --- | --- | --- |
| C01 | `CSI A/B/C/D` | 有 | 光标上下左右 |
| C02 | `CSI E/F` | 有 | 前后行并回行首 |
| C03 | `CSI G/H/f/d` | 有 | 列、行、行列定位 |
| C04 | `CSI a/e` | 有 | 相对列、行定位 |
| C05 | `CSI @/P/X` | 有 | 插入、删除、擦除字符 |
| C06 | `CSI L/M` | 有 | 插入、删除行 |
| C07 | `CSI J` 的 `0/1/2/3` | 有 | 清屏区域、历史 |
| C08 | `CSI K` 的 `0/1/2` | 有 | 清行区域 |
| C09 | `CSI S/T` | 有 | 上下滚动 |
| C10 | `CSI I/Z/g` | 有 | 前后制表、清制表位 |
| C11 | `CSI b`，REP | 部分 | 重复前一码点，复杂字素待验证 |
| C12 | `CSI s/u` | 有 | 光标保存/恢复，不代表左右边界支持 |
| C13 | `CSI 4 h/l`，IRM | 有 | 插入模式 |
| C14 | `CSI 20 h/l`，LNM | 部分 | 能记录模式；LF 调用 linefeed，Enter 未按模式切换；语义差异待复现 |
| C15 | `CSI 5n/6n` | 有 | 普通状态/光标查询；原点等边界未完整验证 |
| C16 | `CSI ?6n` | 无 | 私有 DSR 未接通 |
| C17 | 其他 DSR | 无 | 当前主要处理普通 5/6 |
| C18 | DA1/DA2 | 有 | 主属性 `CSI ?6c`；次属性含终端库版本，不是 AxSSH 版本 |
| C19 | DA3 | 无 | 无第三级属性应答 |
| C20 | DECRQM | 部分 | 底层已知模式可报告，应用额外处理的 1015/1016 等未完整纳入 |
| C21 | `CSI 14t/16t/18t` | 有 | 文本区像素、单格像素、行列数；含布局前延后边界 |
| C22 | `CSI 22t/23t` | 有 | 标题栈 |
| C23 | `CSI 13t/15t/19t` | 无 | 不拥有窗口/全屏几何时不应答，不伪造文本区值 |
| C24 | 其余窗口移动/缩放/最大化/最小化 | 无 | 无通用窗口操作处理 |
| C25 | 窗口/图标标题查询、XTVERSION | 无 | 无专用报告 |
| C26 | SGR 栈、调色板栈 | 无 | 无 xterm 对应栈操作 |

## SGR

以下结合 S1–S3、E2/E5；parser 认识但模型忽略的属性仍记为无。

| ID | 参数或能力 | 状态 | 当前行为或缺口 |
| --- | --- | --- | --- |
| G01 | `0` | 有 | 重置 |
| G02 | `1` | 有 | 粗体，传递字重 |
| G03 | `2` | 有 | 弱化，通过显示亮度处理 |
| G04 | `3` | 有 | 斜体 |
| G05 | `4`、`4:1` | 有 | 单下划线 |
| G06 | `4:2` | 有 | 双下划线 |
| G07 | `4:3/4:4/4:5` | 有 | 波浪/点/虚线绘制路径 |
| G08 | `5/6` | 无 | parser 识别慢/快闪，模型不应用；光标闪烁不能代替文字闪烁 |
| G09 | `7` | 有 | 反色，包括空格背景 |
| G10 | `8` | 有 | 隐藏文字和装饰 |
| G11 | `9` | 有 | 删除线 |
| G12 | `10–19` | 无 | 协议字体选择 |
| G13 | `20` | 无 | Fraktur |
| G14 | `21` | 部分，语义不同 | xterm 为双下划线；锁定 parser 生成 CancelBold；当前双线走 4:2 |
| G15 | `22/23/24/27/28/29` | 有 | 取消对应属性 |
| G16 | `25` | 无实际效果 | 文字闪烁未实现 |
| G17 | `30–37/40–47` | 有 | 基础前景/背景 |
| G18 | `90–97/100–107` | 有 | 亮色前景/背景 |
| G19 | `38/48` 索引色、RGB | 有 | 常见分号/冒号形式，不推断支持全部色彩空间 |
| G20 | `39/49` | 有 | 默认色 |
| G21 | `58/59` | 有 | 下划线颜色设置/恢复 |
| G22 | 边框、圈框、上划线 | 无 | 未接通属性 |
| G23 | 上标、下标 | 无 | 未接通属性 |

## DEC VT、ESC 与字符集

逐项能力不构成某一 VT 型号的完整仿真声明。

| ID | 指令或能力 | 状态 | 当前行为或缺口 |
| --- | --- | --- | --- |
| D01 | BS/HT/CR/LF/VT/FF | 有 | 基本控制行为 |
| D02 | BEL | 部分 | 短暂视觉提示 |
| D03 | IND/NEL/RI | 有 | 索引/换行/反向索引 |
| D04 | DECSC/DECRC，ESC 7/8 | 有 | 光标状态保存/恢复 |
| D05 | RIS，ESC c | 有 | 状态重置 |
| D06 | DECALN，ESC #8 | 有 | 对齐测试 |
| D07 | DECSTBM | 有 | 上下滚动区域 |
| D08 | DECOM，?6 | 有 | 原点模式；查询联动边界待验证 |
| D09 | DECAWM，?7 | 有 | 自动换行 |
| D10 | DECTCEM，?25 | 有 | 光标显隐，与同步输出独立 |
| D11 | DECSCUSR | 部分 | 主要 0–6 形状/闪烁，未覆盖 xterm 全部扩展 |
| D12 | DECCOLM，?3 | 部分 | 清屏/重置滚动区，不切换 80/132 列 |
| D13 | DECSCNM，?5 | 无 | 全屏反显 |
| D14 | VT52 模式 | 无 | 无完整 VT52 仿真 |
| D15 | DECSTR | 无 | 无软重置分派 |
| D16 | DECSCA | 无 | 字符保护 |
| D17 | DECSED/DECSEL | 无 | 选择性擦除 |
| D18 | DECLRMM/DECSLRM | 无 | 左右边界 |
| D19 | ESC #3/#4/#5/#6 | 无 | 双高/双宽/行属性 |
| D20 | DEC 矩形操作 | 无 | 无对应编辑分派 |
| D21 | ASCII、DEC 线绘字符集 | 有 | 常用 G0/G1 指定及 SI/SO 调用 |
| D22 | 完整 ISO 2022 字符集 | 部分 | 可指定部分 G0–G3；单次移位/国家/动态字符集等不完整 |
| D23 | DEC 私有模式保存/恢复 | 无 | 无完整 CSI ? … s/r |

## 输入、鼠标、屏幕与字符串协议

| ID | 指令或能力 | 状态 | 当前行为或缺口 |
| --- | --- | --- | --- |
| I01 | 普通方向/导航键 | 有 | 输入编码已接通 |
| I02 | Application Cursor，?1 | 有 | 普通 CSI 与 SS3 切换 |
| I03 | F1–F24 | 有 | 常见 xterm 编码，不推断所有修饰组合 |
| I04 | 修饰导航/功能键 | 有 | 常见 Shift/Alt/Ctrl |
| I05 | Application Keypad，ESC =/> | 部分 | 底层记模式，输入无完整数字小键盘 SS3 编码 |
| I06 | 鼠标 1000/1002/1003 | 有 | 点击/拖动/移动 |
| I07 | 鼠标编码 1005/1006/1015/1016 | 有 | UTF-8/SGR/urxvt/像素坐标；模式查询不足见 C20 |
| I08 | 旧式 ?9 X10 模式 | 无 | 传统 X10 坐标格式不等于支持该开关 |
| I09 | 1001 Highlight Tracking | 无 | 未实现 |
| I10 | Focus Reporting，1004 | 有 | 实际启用后发送 |
| I11 | Alternate Scroll，1007 | 有 | 已接通 |
| I12 | 备用屏 1049 | 有 | 切屏/光标恢复 |
| I13 | 备用屏 47/1047 | 无 | 锁定 parser 无对应模式映射 |
| I14 | 独立保存光标模式 1048 | 无 | 不等于 D04 |
| I15 | Bracketed Paste，2004 | 有 | 完整事务；应用规范化换行/移除 ESC，有大小限制 |
| I16 | Synchronized Output，2026 | 有 | 结束序列或上游约 150 ms 超时后发布完整帧 |
| I17 | Kitty keyboard/扩展 CSI-u | 策略禁用 | 当前配置关闭，不等于普通 CSI u 光标恢复 |
| I18 | modifyOtherKeys | 无 | 未接通实际输入编码 |
| I19 | DCS DECRQSS | 无 | 无状态字符串处理器 |
| I20 | DCS XTGETTCAP | 无 | 无能力查询处理器 |
| I21 | Sixel/ReGIS | 无 | 无图形处理器 |
| I22 | Kitty Graphics/iTerm2 图片 | 无 | 当前不开放 |
| I23 | 7 位 ESC [、ESC ] | 有 | 常见 CSI/OSC 入口 |
| I24 | 8 位 C1，0x9B/0x9D/0x9C | 无完整支持 | 主解析边界为 7 位控制，UTF-8 支持不等于 C1 支持 |

## OSC：xterm 指令逐项基线

按 S1 的 OSC 表列项；保留编号不计功能缺失。

| ID | OSC | 功能 | 状态与当前边界 |
| --- | --- | --- | --- |
| O00 | 0 | 图标名和标题 | 部分：仅运行时 Tab 标题，无独立图标名 |
| O01 | 1 | 图标名 | 无 |
| O02 | 2 | 窗口标题 | 部分：映射为 Tab 标题 |
| O03 | 3 | X 窗口属性 | 无 |
| O04 | 4 | 调色板设/查 | 部分：RGB、索引 0–255；无颜色名及特殊色索引 |
| O05 | 5 | 特殊颜色 | 无 |
| O06 | 6 | 特殊色开关 | 无 |
| O10 | 10 | 文本前景 | 部分：RGB 设/查，无颜色名 |
| O11 | 11 | 文本背景 | 部分：RGB 设/查，无颜色名 |
| O12 | 12 | 文本光标 | 部分：RGB 设/查，无颜色名 |
| O13 | 13 | 指针前景 | 无 |
| O14 | 14 | 指针背景 | 无 |
| O15 | 15 | Tektronix 前景 | 无 |
| O16 | 16 | Tektronix 背景 | 无 |
| O17 | 17 | 选区背景 | 无 |
| O18 | 18 | Tektronix 光标 | 无 |
| O19 | 19 | 选区前景 | 无 |
| O21 | 21 | DECSWT 标题 | 无 |
| O22 | 22 | 鼠标指针形状 | 无实际效果：parser 识别，Handler 未接通修改 |
| O46 | 46 | 日志文件 | 无；应用日志不等于该协议支持 |
| O50 | 50 | xterm 字体设/查 | 无该语义，另接受旧式 CursorShape |
| O51 | 51 | Emacs Shell 保留项 | 保留 |
| O52 | 52 | Clipboard/Selection | 部分，见子项 |
| O60 | 60 | 已允许功能查询 | 无 |
| O61 | 61 | 禁用子项查询 | 无 |
| O62 | 62 | 可用子项查询 | 无 |
| O104 | 104 | 调色板重置 | 有：单个/多索引/无参数全部重置 |
| O105 | 105 | 特殊色重置 | 无 |
| O106 | 106 | 特殊色模式 | 无 |
| O110 | 110 | 前景重置 | 有 |
| O111 | 111 | 背景重置 | 有 |
| O112 | 112 | 文本光标重置 | 有 |
| O113 | 113 | 指针前景重置 | 无 |
| O114 | 114 | 指针背景重置 | 无 |
| O115 | 115 | Tektronix 前景重置 | 无 |
| O116 | 116 | Tektronix 背景重置 | 无 |
| O117 | 117 | 选区背景重置 | 无 |
| O118 | 118 | Tektronix 光标重置 | 无 |
| O119 | 119 | 选区前景重置 | 无 |
| OI | 大写 I | Sun 图标文件 | 无 |
| Ol | 小写 l | Sun 窗口标题 | 无 |
| OL | 大写 L | Sun 图标标签 | 无 |

## OSC 子项、差异与常见扩展

| ID | 子项 | 状态与当前边界 |
| --- | --- | --- |
| OT1 | 标题原文 | 部分：底层 trim 首尾空白，应用截到 512 字节；标题栈及空标题已有代码/测试 |
| OC1 | 颜色格式 | 部分：接受 #…、rgb:…，不接受 red 等颜色名 |
| OC2 | 连续动态颜色 | 部分：从 10/11/12 开始，仅处理到 12，不继续到 13–19 |
| OC3 | 查询与后续设置同批到达 | 静态推导待复现：整批 advance 后 drain，以批结束颜色应答，需区分查询到达时状态 |
| OQ1 | 协议事件队列 | 有界容量 16，满时诊断并丢事件，影响标题/查询/剪贴板，不保证无限应答 |
| OF1 | BEL、7 位 ST 结束 | 底层均支持；项目 OSC 用例主要覆盖 BEL，ST 专项不足 |
| OF2 | 应答终止符 | 底层沿用查询 BEL/ST，项目级往返待验证 |
| OF3 | OSC 跨输入块 | parser 保留状态，专项分片不足；中文 UTF-8 分片测试不能替代 |
| O52a | c 写默认剪贴板 | 有：默认关闭，开启后限 64 KiB UTF-8 文本 |
| O52b | c;? 读默认剪贴板 | 有：开启设置后逐次确认；20 秒超时及连接/权限代次撤销 |
| O52c | p、s | 策略禁用：仅允许 Clipboard，不开放 Selection |
| O52d | q、cut buffers 0–7 | 无 |
| O52e | 多目标如 cp | 部分：只取首字符，不遍历列表 |
| O52f | 空目标 | 语义不同：当前默认 c，xterm 默认 s0 |
| O52g | 空数据 | 有：空 Base64 解码为清空文本 |
| O52h | 无效 Base64 | 语义不同：当前忽略，xterm 定义清空 Selection |
| O52i | 二进制剪贴板 | 无：解码后必须为 UTF-8 文本 |
| O08a | OSC 8 链接起止 | 有：非空 URI 开始、空 URI 结束 |
| O08b | 可打开 scheme | 策略限定 HTTP(S)，用户显式操作；其他 scheme 保留 inert metadata |
| O08c | URI 长度 | 应用快照/命中截到 2 KiB；不证明 parser 原始累积缓冲也有同样上限 |
| O08d | id 分组 | 部分：底层解析 id，应用 DTO/命中按 URI 和当前行连续格处理，跨行同组不完整 |
| X07 | OSC 7 主机/目录 | 无 |
| X09 | OSC 9 通知 | 无 |
| X094 | OSC 9;4 进度 | 无 |
| X50 | OSC 50;CursorShape=N | 有：0/1/2 为块/竖线/下划线，不是 O50 字体功能 |
| X133a | OSC 133;A | 无：提示符开始 |
| X133b | OSC 133;B | 无：提示符结束 |
| X133c | OSC 133;C | 无：执行开始 |
| X133d | OSC 133;D | 无：执行结束/退出码 |
| X633a | OSC 633;A/B/C/D | 四项均无 |
| X633e | OSC 633;E | 无：命令行报告 |
| X633p | OSC 633;P | 无：含 Cwd 的属性报告 |
| X777 | OSC 777;notify | 无 |
| X1337c | OSC 1337;CursorShape | 无，不等于旧式 X50 |
| X1337d | OSC 1337;CurrentDir | 无 |
| X1337h | OSC 1337;RemoteHost | 无 |
| X1337m | OSC 1337;SetMark | 无 |
| X1337v | OSC 1337;SetUserVar | 无 |
| X1337f | OSC 1337;File | 无：文件/内嵌图片 |
| X1337p | OSC 1337;Copy | 无 |
| X1337s | OSC 1337;ReportCellSize | 无，不等于 CSI 尺寸查询 |

## Unicode

依据 S9、E1/E2/E5/E6。字符宽度、字形墨迹、字素簇和终端格坐标不是同一个量；UTF-8 解码成功不能推断完整 Unicode 合规。

| ID | 能力 | 状态 | 当前行为或缺口 |
| --- | --- | --- | --- |
| U01 | UTF-8 解码 | 有 | VTE 处理、跨批保留状态 |
| U02 | UTF-8 分片 | 有 | adjacent_wide_characters_keep_two_cell_columns_across_output_chunks 遍历中文各字节拆点；已有测试，本轮未重跑 |
| U03 | 中文/全角双格 | 有 | 首格/续格、光标、命中、选区复制 |
| U04 | 零宽组合字符 | 有基础支持 | 挂到前一 cell、保留码点，不等于完整字素分段 |
| U05 | 主屏 reflow | 有 | 软/硬换行、宽字符、光标；备用屏不重排 |
| U06 | 字体回退 | 有 | 自带/系统字体，实际覆盖随已安装字体变化 |
| U07 | VS15/VS16 | 部分 | 呈现角色可选，不完整重算序列格宽 |
| U08 | ZWJ Emoji | 部分 | 同样式 run 合并排字，核心按组成码点累计格宽 |
| U09 | 肤色/旗帜/键帽 | 部分 | 字体角色/合并策略，非完整字素宽度模型 |
| U10 | Extended Grapheme Cluster | 未完整实现 | 单码点 width() 加零宽附着，无完整终端字素状态机 |
| U11 | Ambiguous Width | 部分 | 使用库默认宽度，未见终端级可选宽/窄策略 |
| U12 | UAX #9 Bidi | 未完整接入 | 未见行级逻辑/视觉重排及列映射，依赖树含 bidi 不证明终端支持 |
| U13 | 复杂文字连写 | 部分/待验证 | 非 ASCII 通常按 cell 拆 run，需核对阿拉伯文/印度文字跨 cell 连写 |
| U14 | NFC/NFD | 不主动执行 | 保留原文，不自动算缺陷或授权正规化 |
| U15 | GBK/Big5 等编码 | 无 | 未见终端编码切换/转码路径 |
| U16 | Unicode 一致性 | 待验证 | 无本次运行的字素/Bidi/宽度一致性报告，不宣称任意版本全面合规 |

可复核样本：E6 的 mixed_latin_han_and_zwj_emoji_keep_cell_positions_through_reflow 明确期望 `👩‍💻` 为 4 格，证明绘制合并没有改变协议格宽，不证明与所有 TUI 的宽度策略一致。UAX #11 不规定所有 Emoji 一律两格；未来先定义兼容目标和宽度策略，再讨论调整。

## 实施顺序

每阶段按先复现、再确定支持条件、最后验证的顺序推进。矩阵中的历史与厂商扩展由 B6 逐项决定。

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| B0 | completed | base 参考、来源与入口归档 | 文档/tracker/skill/diff 检查 | 文档完成不代表能力补齐 |
| B1 | completed | 最小复现样本与支持范围决定 | 分片/同批时序/逐字节应答，冻结期望和政策 | 本机字节样本完成 |
| B2 | completed | CSI/SGR 子集方案 | G14/G08/C14/C20，解析到呈现/输入/回写 | 不预先决定升级或修改上游 |
| B3 | completed | DEC/备用屏/键鼠范围方案 | I05/I13/I14/D12，模式开关与查询一致 | 不宣称某型号全仿真 |
| B4 | completed | OSC 范围与差异方案 | OC3/O52e/f/h/O08d、BEL/ST、批次/拥塞 | 保留授权/代次/资源上限 |
| B5 | completed | Unicode 字素/格宽/布局策略 | U07–U13，reflow/删除/选区复制/光标/混合脚本 | 视觉由用户验收 |
| B6 | completed | 扩展取舍记录 | Shell integration/DCS/图形/C1 的真实需求及安全边界 | 不因规范收录而默认启用 |
| B7 | blocked | 实施后逐项状态与双语说明 | 定向回归、仓库 Cargo 门禁、原生平台验收 | 自动化已通过，等待原生 GUI 和真实 SSH 验收 |
| B8 | completed | 双格字符重排光标修复 | 10/11 列、复制、后续写入与历史 | 模型专项及全量通过 |

## 后续模型的核验与安全要求

1. 先读本计划、最新锁文件和 E1–E7，逐项重验；兼容声明回到 S1–S9，不能把聊天结论当规范原文。
2. 获准实施后，先说明状态/操作/生命周期归属：terminal core 管解析和格状态；application 管授权、用户意图和连接路由；UI 管呈现；transport 管 I/O。保持既有架构边界。
3. 区分缺项与策略。不得以补全标准为由自动开放远程剪贴板读取、非 HTTP(S) 外部打开、远程文件/日志/窗口控制或图片协议；保留 host-key、短期凭据、撤销和连接代次检查。
4. 字符串、缓冲、队列、高频事件必须有界。DTO 截断不证明 parser 原始缓冲有界，也不能为保证无限应答移除背压。
5. 未来协议用例覆盖 BEL/ST、任意序列/UTF-8 分片、同批交错设/查、取消/未知序列、畸形/超长参数、拥塞；只核对输出文字或镜像实现不足以证明兼容。
6. Unicode 用例保留原文并核对字素、逻辑列、视觉跨度、reflow、光标及复制；选定 UAX 版本和终端适配策略，不用“画面上像一个图形”证明宽度正确。
7. 分别记录单元/集成、真实 SSH/PTY/设备和跨平台结果；GUI 视觉由用户验收，遵守 AGENTS.md，不自行截图作为正确性证据。
8. 本轮不增加或执行上述协议用例。未来命令/依赖/模块边界变化时按仓库规则同步双语说明与 tracker，实际验证后再升级“待验证”状态。

## 初始归档验证

- 已完成：实现/已有测试的只读核对、规范和锁定 crate 阅读。
- 本轮文档验证：见[月度记录](../../changes/2026/11.md)“终端协议 base 计划归档”。
- 未完成：协议补齐、vttest/Unicode 一致性执行、真实 transport/GUI/跨平台验收。
- 最后更新时间：2026-11-27 14:34 +0800。

## B1 范围决定

- 父模型统一使用锁定 VTE 的语义回调和 Alacritty 格状态。通过同步解析回调核对模式与颜色的即时状态。
- B2 改 SGR 21、LNM 和 1015/1016 查询。文字闪烁保留评估。
- B3 优先接通独立光标保存和恢复。上游切屏 API 会重置备用格，47/1047 需要核对独立缓冲语义，不与 1049 简单等同。小键盘和窗口列数先核对现有平台契约。
- B4 改 OSC 颜色应答时序，覆盖 BEL/ST、分片和拥塞。剪贴板与链接遵循现有用户授权和打开规则。
- B5 在格、光标、选择、复制和重排层固定当前列宽策略与差异。B6 明确扩展取舍，B7 记录实际通过的验证。

验证：`cargo test --locked --offline --lib terminal::tests::protocol_base` 在修改运行时代码前复现五项预期失败。

## B5 Unicode 策略

- 以锁定 `unicode-width 0.2.3` 内置的 Unicode 18.0.0 表作为当前宽度数据，以窄宽度解释 East Asian Ambiguous 字符。协议字符格继续累加码点宽度，零宽组合字符保留在所属字符格。没有协议侧正规化。
- 排字层可对同样式字素成分合并呈现。字体、Variation Selector 和字形墨迹不移动协议光标或列范围。若引入 UAX #29 的完整字素分段或 UAX #9 的双向布局，需先定版 UAX 数据和目标终端，与 TUI 共同核对逻辑列、软换行、写入、擦除、选择、复制和编辑。
- 单字节分片后，测试样本宽度为：中＋组合符 2、👩‍💻 4、♥＋VS15/VS16 各 1、👋🏽 4、🇨🇳 2、1️⃣ 1、· 1。阿拉伯文及天城文样本按逻辑顺序复制。专项核对 40→11→40 列的光标和复制，及按格删除组合字符。
- B8 修复前，同一混合样本缩到 10 列时在首行末尾为 👋 插入占位格。文本保持完整，但光标落在末尾 Z 上，恢复 40 列后为第 22 列（0 起算），预期第 23 列。修复与验证见 B8。未运行 UAX 一致性套件。

## B6 扩展取舍

| 范围 | 本轮决定 | 再评估条件 |
| --- | --- | --- |
| SGR 5/6/25 文字闪烁 | 暂缓。保留属性与呈现需求记录 | 有实际 TUI 需求后验证减弱动效、可读性、帧 invalidation 和原生平台表现 |
| DEC 47/1047，80/132 列，application keypad | 暂缓兼容扩展。1048 已支持，1049 继续切屏，物理小键盘走平台逻辑输入，窗口决定实际列数 | 上游提供保留备用屏的切换入口，或有可复现的应用需求和跨平台输入证据 |
| OSC 7/133/633 与厂商目录/命令元数据 | 暂缓。目录、主机和命令报告不作可信输入 | 先明确现有用户操作的交互用途、来源提示及长度上限，再用隔离测试验证 |
| DECRQSS/XTGETTCAP、其他 DSR/DA3 | 暂缓 | 以实际 TUI 的确切查询和常量应答为依据，只公告已接通能力 |
| Sixel、ReGIS、Kitty Graphics、iTerm2 内联图片 | 保持关闭 | 需要明确产品需求、解码/尺寸/内存限额、隔离和 UI 所有权设计 |
| 8 位 C1、Kitty 键盘/CSI-u、modifyOtherKeys | 暂缓 | 先验证 UTF-8、C1 与键盘协商，避免影响普通文字、IME 和平台快捷键 |
| OSC Selection/多目标、自动打开 URI、远程文件/日志/窗口控制 | 保持当前的单默认剪贴板与显式 HTTP(S) 打开策略 | 扩大权限需要直接产品授权及独立安全验收 |

## B7 实施后矩阵变更

| 基线条目 | 当前实现 |
| --- | --- |
| C14 | LNM 开启后 LF/VT/FF 到下一行首列，Return 发送 CRLF。IND 保持当前列。 |
| C20、I07 | DECRQM 读取 1015/1016 的即时开关状态。1016 在 SGR 模式下采用像素。 |
| C21 | 14t/16t 按收到的顺序进入同一个有界查询队列，布局前保留最多 16 个待应答测量请求。18t 按行列数立即应答。 |
| G14 | SGR 21 设置双下划线并保留粗体。24/22 分别清除下划线/粗体和弱化。 |
| I14 | 1048 保存和恢复当前屏的光标，主屏和 1049 备用屏各有自己的保存槽。 |
| OC3、OF1–OF3、OQ1 | OSC 查询按到达时的调色板应答，并沿用 BEL/ST。完整终止后才分派，取消、超长或参数过多的序列被丢弃。 |
| O52e/f/h | 单目标 c 和空目标可用。多目标及畸形负载被忽略。有效空数据清空文本。默认关闭，逐次读取确认。 |
| O08c/d | URI 最长 2 KiB，ID 最长 256 字节，同行连续格按链接身份分组。跨行范围继续列为待评估。 |
| U07–U13 | 保持按码点计列和逻辑顺序复制。已验证样本和上游光标差异见 B5。 |

B6 记录了其余扩展的取舍。与 xterm 对照时，SGR 21、1048、单格像素报告和 OSC 应答终止符可分别在 SGR、DECSET/DECRST、XTWINOPS 和 Operating System Commands 段定位。[XTerm Control Sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

实际验证与人工验收状态见 [current.md](../../current.md)。

### 本轮验证

- 13 项 base 字节回归、macOS 原生 PTY 调色板应答往返、完整 1095 项 Rust 测试通过（506 library、582 binary、7 integration）。
- VTE std/ansi 54 项和无默认特性 34 项通过。已核对 vendored 源码与原始 crate 的差异和许可证。
- macOS ARM64 原生 fmt、locked/offline check、严格 all-target Clippy，及 CI 同款目标 check/Clippy/build 通过。Rust 1.92 locked/offline check 使用本机校验过的匹配 Skia 归档通过。
- Windows x86_64 经 cargo-xwin 交叉 check、严格 all-target Clippy、build 通过，产物确认为 x86-64 PE32+。认证提示中的 profile 快照改用 Box，消除已有的 Windows 大枚举告警。原生 Windows 测试、Windows ARM64、Intel macOS 与 Linux 交由相应 CI。
- Markdown/双语配对、图标生成、翻译、tracker、项目 skill 和 diff 检查通过。真实 SSH 服务器、vttest、UAX 全套一致性及原生 GUI 人工验收待执行。
- 本轮最后更新：2026-11-27 16:13 +0800。

## B8 宽字符重排后的光标

- 修复位于 `vendor/alacritty_terminal/src/grid/resize.rs`。双格字符移到下一行时，前一行末尾的占位只消耗显示列。光标判断是否跨行及其下一行列号使用实际移走的源格数。
- `123456789中Z` 在 40→10→40 列后回到第 12 列（0 起算），继续写入 `!` 得到 `123456789中Z!`。40→11→40、混合 Unicode、恰好填满一行和滚动历史也有回归覆盖。
- 101 项终端模型定向及 1098 项全量 Rust 测试通过。Alacritty 搭配原版 VTE 时，132 项单元测试、45 项录制测试和 1 项 doctest 通过。搭配 AxSSH VTE 时，录制测试 44 项通过，旧 `underline` 录制对 SGR 21 的预期与 B2 的双下划线语义不同。AxSSH 有单独回归覆盖该语义。
- macOS ARM64 与 Windows x86_64 通过 check、严格 Clippy 和链接。Rust 1.92 检查通过。更新于 2026-11-27 17:25 +0800。
