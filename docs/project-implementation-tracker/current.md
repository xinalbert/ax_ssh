# 当前项目实施记录

## 当前目标

- 目标 ID：20261022-egui-migration
- 目标：按独立分支计划完整用 egui 替代 Slint，保留既有 Rust 领域、transport 和安全行为；过渡期默认 Slint，E9 移除旧 UI/依赖/专用构建路径。
- 交付物：`codex/egui-ui-migration`、双语计划、egui-ui binary、UI 无关终端/连接 controller、Painter/IME 与临时 Local/SSH/Telnet/Serial、信任/认证及重连、会话/分组、Tab/八 pane 与独立工作区恢复、构建/行为测试及阶段追踪。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：E6R-SelectedDrag 选中多项 egui 内部拖放；E6R-UploadConflict 已收口，E7R 剩余界面翻译、设置与媒体入口仍由总项跟踪。
- 不在本轮范围内：E2V/E4V/E5V 原生验收、E6 完整拖放/交互对照、E7 完整功能及 E8 多窗口、E9、正式切换、安装/推送/发布、真实凭据存储迁移、参考工程写入、自主应用截图验收。
- 后续实施范围：E3R 后续共享应用状态及工作区/传输/设置/平台迁移；E2 完整排字、原生输入、GUI 和性能验收作为 E2V 保留。
- 所有权与安全：前端只消费快照并发出意图；控制层保留解析、业务、持久化、取消和清理，SSH 继续默认拒绝，凭据短期化、队列/缓存有界。`third_package/axshell` 保持只读参考边界。

## 当前状态

- 阶段：实施中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| E0 | completed | 分支、双语计划、架构决策方向、FigCraft 定位及追踪 | Markdown/入口、skill、tracker、基线 Cargo check、diff 已通过 | 本轮交付；基线 master `15404e1` |
| E1 | completed | 版本/MSRV、renderer 策略、feature、实验入口和隔离目录 | 最低工具链两前端、组合/egui-only 图、strict 与本机链接通过 | 0.37 + Rust 1.95；其它平台交既有原生 CI |
| E2I | completed | 真实本地 PTY 单 pane 原型实现 | 默认 909、egui 406+15 测试、本机严格门禁、组合/MSRV 和构建通过 | E2 分为已交付实现和独立待验收项 |
| E2V | pending | E2 原生输入/完整字体/性能验收 | 用户视觉确认、原生 IME/keypad/AltGr、可访问性和性能数据 | 不以用户要求下一项代替完整验收 |
| E3 | completed | 独立终端所需的最小共享 controller、状态/呈现/cleanup | 前三切片通过两前端完整回归、本机构建及边界校验 | 最小基础已具备；其余状态提取随 E4/E5/E7 对应功能推进 |
| E3R | pending | 其余工作区/安全提示/连接/设置编排提取 | 随对应功能保留原有回归，无双份业务状态 | 终端/连接与 pane 树已共用；实验 workspace owner 已落地，其余业务编排随 E6/E7 |
| E4I | completed | 临时 SSH/Telnet/Serial 与信任/认证/有界重连实现 | 固定 loopback peer/凭据、真实字节/resize/取消/替换和完整回归通过 | 复用既有 worker；仅本次信任，未知密钥认证前拒绝；连接尝试不保存凭据或仅本次信任 |
| E4V | pending | E4 原生 GUI、SSH-agent/服务器/串口验收 | 真实服务器/agent/加密私钥与串口设备、目标平台 GUI/权限/断开 | 不以 loopback 或编译代替原生验收 |
| E5I | completed | 会话/分组与编辑、Tab/八 pane、独立工作区保存/打开/恢复 | 9 项 workspace、共享 pane、真实 PTY 焦点/禁用输入、多 pass、两前端完整门禁通过 | UUID/版本/请求隔离，恢复 idle，32 活动/关闭终端和 64 命令/回执上限 |
| E5V | pending | E5 原生布局/键盘/恢复与完整功能对照 | 用户视觉、原生拖动/焦点/文件路径、真实多连接关闭恢复 | 不保存输出历史/临时目标/草稿；原生文件选择器/最近菜单、凭据存储和完整旧格式对照待后续界面 |
| E6 | completed | SFTP 核心：companion、目录/传输/图标、外部编辑回传和拖入 | 字节/冲突/迟到回执、任务清理及双前端门禁 | 本项只代表核心实现；剩余交互另列 E6R |
| E6R-Pause | completed | 下载单项 Pause/Resume、类型化资格/等待与有界 history | 默认 940/egui 676、双前端完整/组合/双 macOS target | 五项 controller/两项 UI 新回归；批量由 E6R-Batch 接通，原生验收保留 |
| E6R-Batch | completed | 传输多选/全选/清空、批量 Pause/Resume/Cancel、部分入队汇总 | 默认 940/egui 681、双前端完整/组合/双 macOS target | 新增三项 controller/两项 UI 回归；原生验收与远端文件选择另列 |
| E6R-Mutate | completed | 单项远端 Rename/Delete、冻结目标确认、单槽精确回执与导航安全刷新 | 默认 940/egui 689、双前端完整/组合/双 macOS target | 五项 controller/三项 UI 新回归；真实 rename/冲突/空及非空目录/delete，原生验收保留 |
| E6R-Select | completed | 远端 path 多选/全选/清空、批量下载与根项入队摘要、发现入口预算 | 默认 940/egui 696、双前端完整/组合/双 macOS target | 五项 controller/两项 UI 新回归；递归字节/空目录失败/冲突/取消，原生验收保留 |
| E6R-UploadConflict | completed | egui Keep both 与上传批次应用的冲突 Modal | 默认 940/egui 599+106、双前端完整/组合与双 macOS target | 复用既有 KeepBoth 与 batch policy；原生 GUI/真实服务器验收保留 |
| E6R-SelectedDrag | completed | Local/Remote files 已选多项的 egui 内部拖放 | 版本/代次/目录快照复验、单次入队与边界回归；双前端及可用 macOS target 门禁 | 复用现有选择与 upload/download preparation；系统文件管理器原生拖出和完整平台验收仍独立保留 |
| E6R | pending | 原生拖出、批量修改、完整 SFTP 功能对照 | 真实平台拖放及旧功能对照 | 本地多选上传与选中拖放已完成；不把核心完成视为全部验收 |
| E7 | completed | 首批设置：主题/字号/行高/部分快捷键、preview/save/close | 事务/草稿和现有门禁 | 原 completed 限定为首切片；完整设置另列 E7T/E7R |
| E7T | completed | 终端行为与 SFTP 设置界面及 controller/worker 应用链 | 79 项 controller 回归、两前端完整门禁、组合检查及文档校验通过 | 持久值经 owner，预览不改变连接安全权限；GUI 验收保留 |
| E7L | completed | PO/Rust 提取与静态目录、语言选择、菜单/设置切换 | 711×11 文案、31 Python、36 egui 聚焦回归、两前端完整及 macOS ARM64 target 门禁通过 | 语言成功保存后应用；其余文案归 E7R，原生 GUI 验收保留 |
| E7F | completed | 共用字体目录、应用/终端字体选择及后台应用链 | 四内置字体/实测网格/字重、预览/回退/旧结果拒绝/关闭 join、两前端门禁通过 | 显式选择加载有界，GUI 和既有系统缺字 provider 验收独立保留 |
| E7W | completed | 连接/信任/认证与会话/分组/Tab/pane/工作区文件文案 | 741×11 目录、逐语言信任/折叠/草稿数据回归及两前端门禁通过 | 固定文案由 UI 翻译，错误/用户数据不翻译；原生验收保留 |
| E7S | completed | SFTP 文案、共享信任提示与 UI 操作边界 | 756×11 目录、167 egui 键、指纹/秘密/数据保持和重复 pass 回归、两前端门禁通过 | 只改 UI 意图/显示；worker 信任、文件及冲突规则不变，原生验收保留 |
| E7M | completed | 选择优先/自动复制/右键操作设置与实际行为 | 鼠标字节/拖选/Shift/Alt/取消、复制身份/迟到结果、保存失败/预览及两前端门禁通过 | UI 拥有手势，controller 拥有模型复制；原生鼠标/剪贴板验收保留 |
| E7P | completed | Option/Meta 原生适配与 OSC 52 写入/逐次读取确认 | 精确输入/IME、默认关闭/拒绝/一次性/超时/撤销、两前端及双 macOS target check/Clippy 通过 | 保存值驱动，后台读取保留 join；真实 Option/IME/系统剪贴板验收保留 |
| E7U | completed | 全局上传并发设置及启动/保存应用链 | 加载成功/失败、保存成功/失败、旧 revision、独立限流器及两前端门禁通过 | 复用已有 process-wide limiter；媒体留给 E7R，X11 入口见 E7X |
| E7O | completed | 本地文件快照打开、限额设置和独立缓存 | 原句柄/替换/链接/限额、旧目录/取消/关闭 join、保存失败、两前端及双 macOS target 门禁通过 | 已验证句柄复制到实验私有缓存；关联应用/GUI 原生验收保留 |
| E7X | completed | SSH Off/-X/-Y 选择、X server 设置与已保存配置应用链 | 默认 Off/编辑保存/失败草稿、固定信任 loopback 请求/拒绝后 shell 保持、等待取消、两前端及双 macOS target 门禁通过 | 复用 worker，修正共享 X11 服务端确认；麦克风与原生 XQuartz/GUI 验收保留 |
| G1 | completed | 根 AGENTS.md 强制施工前标准检索及项目 skill 对齐 | 官方来源/版本/决策落 research、Markdown/skill/tracker/diff 和基线 Cargo check 通过 | 用户明确要求，每个实施切片先检索官方/版本化依据再写代码 |
| E7A | completed | 音频格式设置与连接级默认关闭的麦克风转发 | 全格式/保存失败、信任/认证前无准备、格式捕获/重连/等待断开、两前端及双 macOS target 门禁通过 | 默认关闭/临时 opt-in；复用 CPAL/Pulse worker，真实设备/GUI、附件与手动转写保留 |
| E7I | completed | 剪贴板图片附件与后台任务/文件生命周期 | 7 项附件/PTY/SSH、2 项 egui 回归及两前端/双 macOS target 门禁通过 | 依据 research E7I；后台转换、controller 准备、原目标原子确认，无自动回车；原生剪贴板/GUI 保留 |
| E7H | completed | 会话侧栏遮罩与单字符设置/预览 | 协议/Unicode/原值不变、全语言/保存失败恢复、默认 935/egui 610 测试及两前端完整门禁 | 先研究后施工；复用默认规则，完整悬停详情保留，GUI 验收独立 |
| E7B | completed | 文字亮度/粗体亮色控件及真实 Painter 消费 | 默认 936/egui 614 测试、两前端完整门禁/组合及颜色/缓存/全语言/事务回归通过 | 先研究后施工；共享既有 HSL，GUI 单独验收 |
| E7Q | completed | 共享语义识别、controller 有界注解及 egui 控件/Painter | 规则/资格/行身份缓存、真实模型几何/静止预览、全语言/保存事务及两前端门禁 | research E7Q；绘制无正则，GUI 单独验收 |
| E7R-FPS | completed | 前台/后台刷新率控件、焦点提示及输出 deadline | 默认 938/egui 633 测试、两前端完整门禁与组合检查 | 默认 60/4、1–120 FPS；仅保存后应用，协议不等帧，原生帧率待验收 |
| E7R-Icons | completed | 共享身份/构建像素、真实 Sidebar/Tab 图标、本地覆盖与主题轮廓 | 默认 939/egui 645、完整门禁及双 macOS target check/strict Clippy | 预览/还原与失败保存、8KiB/资源边界、纹理释放；远端新探测/会话 picker 与原生验收保留 |
| E7R-Shells | completed | 后台发现、只读候选、自定义输入与 Save 缓存 | 14 项设置聚焦、默认 939/egui 649 与两前端完整门禁 | 当前优先、32 项/256 字符、失败/重试/新终端 resolver；原生 PATH 体验保留 |
| E7R-SFTP-Key | completed | SFTP 菜单/保存键/UUID 单次路径焦点、六项编辑和八字段校验 | 默认 939/egui 654、两前端完整/组合及双 macOS target 门禁 | 同名 research；不自动连接，原生验收/导入导出保留 |
| E7R-Transfer | completed | 共享 JSON/脱敏规则、后台导入导出及模态入口/八键编辑 | 默认 940/egui 669、双前端完整/组合与双 macOS target 门禁 | 自动合并/显式粘贴；定向导入、原生验收保留 |
| E7R | in_progress | 其余界面翻译、剩余设置及媒体入口 | 全部 locale、权限/取消和功能对照 | 剩余迁移统一使用本总行；上一切片 E6R-UploadConflict 已完成；后续切片继续执行标准检索 → controller/UI 回归 → 双前端/组合/target 门禁 |
| E8 | completed | 菜单与 Tab 首切片 | 既有 action/Tab 回归及 macOS 编译记录 | 原生 GUI 验收保留，剩余平台工作另列 E8R |
| E8R | pending | 多窗口、DPI、可访问性和原生适配收口 | 五目标 check/Clippy/build、原生 GUI 及资源生命周期 | 不以首批菜单代替完整平台迁移 |
| E9 | pending | 功能/性能报告、默认切换与旧依赖清理 | renderer 支持策略、全部回归、用户验收、配置兼容、发布/许可 | E9 前默认 Slint；无预设性能收益 |

## 已完成

## E6R-UploadConflict 已完成

- controller 保留 `resolve_conflict` 旧调用，新增带 `apply_to_batch` 的最小决策 API；命令只转发该布尔值，worker 继续拥有 KeepBoth 编号命名、批次策略和 pending 冲突生命周期。
- egui 传输区使用稳定 UUID `Modal`，遮罩/Esc 按 Skip 收口；复选框只持有 pane 临时状态，提供 Skip、Keep both、Overwrite 和 Apply this choice to this upload batch，换代次/断开/回执完成会清理状态。
- 新增 Modal/批次回归，并修正旧 SFTP 聚焦测试不再假设冲突行有直接 Overwrite/Skip 按钮；没有新增依赖、持久化 schema、wire 协议、后台任务或 SSH 信任路径。


- E6R-Pause 已完成：类型化阶段/下载能力、连接代次/UUID/资格复验、成功入队后等待回执、进度不覆盖等待阶段、终态吸收迟到事件、仅已接纳新请求可回收终态历史。egui 以 160px 有界区域提供每项 Pause/Resume/Cancel，标准禁用/UUID/每帧准入保持。五项新增 controller 与两项 UI 回归、完整双前端/组合/双 macOS target 门禁通过；实际固定信任下载覆盖非重复字节/旧代次/暂停取消关闭清理，11 locale 控件与历史布局均通过。上传不可暂停，不保存断点，原生验收保留。

- E7R-Transfer 已完成：共享 version1 JSON 格式、256 KiB/128 profile/空组/新 UUID/冲突命名与脱敏，默认 callback 保留适配；实验 owner 在 awaited blocking task 做导入/导出/原子写入，导出只读且不递增 revision。模态显式粘贴确认、保存会话/分组选择与 Copy、八键编辑已接通；关闭/旧 revision/失焦的导出只消费丢弃，导入失败保留配置和草稿，既有 pane/设置不替换。五项原纯回归迁入库，四项新增库/六项新增 binary 与两项设置事务扩展，以及完整门禁通过；定向分组导入和原生验收保留。

- E7R-SFTP-Key 已完成：View > SFTP 与保存 Ctrl+M 路由到当前保存 SSH pane 的 Remote path；UUID 请求单次复验，终端同帧不抢焦点。六项可编辑键、八字段别名/顺序/保留键校验及 Windows/Linux Cmd/Meta→Ctrl 归一化已接通；预览不改路由，保存失败保留草稿，重试成功生效。五项新增和两项事务扩展、全量/组合及双 macOS target 门禁通过；不自动连接/认证/List，无 worker/schema/依赖改变，原生验收保留。

- E7R-Shells 已接通启动 blocking 发现/GUI 前 join、Context 局部只读候选、下拉与 256 字符自定义输入。候选当前优先、最多 32 项，失效/自定义值不被覆盖；发现/打开设置不制造脏缓存。Save 在提交副本合并，失败保留原草稿/保存值；后续新终端读取提交值，现有 transport 不重启。四项新增回归及完整门禁通过；默认侧栏组名长度只是兼容字段，旧审计表述已纠正，紧凑导航需要独立契约。

- E7R-Icons 已实现共享平台身份/默认薄适配、egui 构建期 116 图标与 48px 像素、原色/mask 有界纹理、Sidebar/聚焦 pane Tab 消费，以及本地覆盖/跟随主题草稿。预览/还原与失败保存不修改持久值，清缓存/销毁释放纹理。Linux os-release 仅缺失时回退、来源限额及 SVG/PNG/路径拒绝均有回归；新增 9 项用例，原有 3 项身份测试随模块迁入共享库。全量/组合与双 macOS target 门禁通过，原生视觉和远端新探测/会话图标 picker 保留。

- E7R-FPS 已实现 1–120 FPS 控件（默认 60/4）、启动/恢复及成功保存应用链、绝对脏输出 deadline 与独立焦点 watch；空闲不周期唤醒，错过期限不补帧。逐准入 UI 帧提示与隐藏优先清除覆盖连接代次重置/鼠标释放失败；DEC1004 仍由有界命令和代次校验保护。五项调度/焦点、一项固定信任 SSH 及一项全语言 UI 新增回归，加上保存失败/成功/规范化事务与同步输出/复制实时解析扩展均通过。Theme 完整自动门禁已补齐；原生视觉/焦点/性能验收保留。

- E7Q 已完成共享规则/资格、controller 当前可见行身份缓存及 egui 开关/四颜色/Painter；默认适配沿用原 split 与装饰。默认 480 library + 458 binary = 938、egui 556 library + 69 binary = 625 项完整测试通过；两前端 check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过。真实 PTY 几何/显式风格/装饰与预览还原、全语言稳定 ID、保存成功/失败恢复和身份缓存回归通过。

- E7B 已完成亮度/粗体亮色控件、共享 HSL 与真实 Painter 消费；反显后调整文字/装饰、隐藏最后、alpha/背景/选区/光标保留，静止行按设置重建。四项新增回归、两项扩展保存事务及完整门禁通过，复用五条已有 PO 键。

- E7H 已将默认前端纯 endpoint 遮罩共享到 controller，egui 增加侧栏地址行和 Workspace 单字符设置，预览对副本规范化、关闭预览/丢弃恢复保存值；原始搜索/profile/连接和完整悬停/辅助技术详情保留。两项共享回归、全语言 Unicode 输入/侧栏、两项保存事务回归及完整门禁通过；中英文 IPv4 示例已按既有第二段遮罩纠正，未改变规则。

- E7A 已完成音频采样率/声道设置、成功保存后的格式 watch 与连接级默认关闭的转发选择。旧 connect API 默认关闭，授权不保存到 profile/工作区；重试沿用，主动断开/替换/切换协议清除。全部格式的全语言点击、保存失败及成功重试、固定信任 loopback 的认证前无准备/新 worker 格式捕获/准备失败保留 shell/重连/等待断开 join 通过；真实设备与 GUI 单独保留。

- E7X 已接通 Off/-X/-Y、保存后的本机 X server 设置与只读启动位置快照，麦克风保持关闭；共享 setup 已修正为读取服务端回执后才启用 X11，并保留有界早到输出，等待期间断开可取消。24 项 X11、两项设置事务与 28 项终端 controller 聚焦回归、两前端完整门禁和双 macOS target check/Clippy 均通过；复用 22 个已有文案，egui 使用 207 键。

- E7O 已提取共享目录/文件校验并保留默认薄适配；实验本地文件 Open 和快照大小控件已接通，独立缓存使用已验证句柄并在复制前后复核身份。单个 blocking job 保留撤销/30 分钟期限/退出 join，取消或 opener 失败会清理未使用副本。五项后台打开、9 项共享校验及全语言 Open 多 pass 回归、两前端完整门禁和双 macOS target check/Clippy 均通过；控件复用已有两条文案，egui 使用 185 键。

- E7U 已接通 1–32 的全局上传上限控件和 workspace 加载/保存应用链，复用进程级 limiter；已有 permit 保留，无需重连。新增两项回归确认加载成功/失败、失败保存、旧 revision、无关 catalog 和规范化边界；使用私有回调避免测试全局污染，两前端完整门禁通过。新增控件复用已有两条文案，egui 使用 183 键。

- E7P 已实现保存后的 macOS Option/Meta 原生焦点策略和 OSC 52 权限。默认关闭，开启后读写均限制 64 KiB；读取每次明确确认、UUID 一次性/20 秒超时，快速关闭再开启也撤销旧授权。后台单个读取 job/有界队列和结果槽保留空文本/CRLF，失焦/导航/几何/权限变化丢弃结果并在退出 join。Allow 捕获视口 revision，controller 在展示发布前拒绝旧响应，未授权提示不因自身布局调整被取消。新增 9 项回归及两前端完整门禁通过，egui 使用 181 个已有 PO 源键。

- E7M 已接通三个保存后生效的鼠标/复制开关和独立手势状态；本地拖选、Alt/Shift、延迟单击、取消与自动复制/右键处理已由真实 PTY 验证。复制结果携带连接/视口身份并在视口命令后撤销，应用粘贴在任何控件处理前做目标/焦点/连接/视口与多 pass 检查。复用已有译文，egui 源键增至 173；两前端完整门禁通过，原生验收保留。

- E7S 已接通共用终端/SFTP 信任提示及 SFTP 固定文案，显示原始目标和指纹、保留 confirmable/token；连接/提示代次变化清除旧凭据，密码控件作用域分离。SFTP 按钮每帧准入/跳过 sizing，拒绝发送显示错误，传输使用 UUID。每目录新增 15 条并保留原 741 条/元数据；756×11 PO、167 egui 键、48 项 egui 聚焦回归及两前端完整门禁通过。

- E7W 已接通连接/信任/认证、会话/分组、Tab/pane 与工作区文件静态文案；复用已有 PO 并新增 30 条/目录，原 711 条及元数据保留，741×11 消息和生成表通过校验，egui 使用 145 条键。新增三项回归覆盖逐语言确认/阻止、指纹/秘密显示、保持折叠/草稿以及与界面键同名的用户数据/路径/配置原样保留；两前端本机门禁通过。

- E7F 已实现共用字体目录及 egui 独立应用/终端选择、后台有界加载、最新请求拒绝旧结果、缺失/非等宽回退、静态/可变样式和退出取消/join。四内置字体、实测网格/字重、选择失效且保留 IME、草稿保存失败与预览回退、过期合并/退出 join 均通过回归。Monaspace 元数据不能作为非等宽结论，使用实测 Latin 字宽判断。

- E7L 已接通成功保存后的语言选择、Context 本地静态译文、应用菜单/设置/关闭确认与 Settings 标题；原生菜单原位更新文本，保留 ID/快捷键/系统角色。两前端和搜索源键合并，原 675 条译文不变，新增 36 条到各 PO 711 条，egui 首批使用 84 条；其余文案仍属 E7R。

- E7T 已实现终端历史/默认尺寸/备用屏滚轮策略、SFTP 每连接并发/文件大小/带宽/编辑上限/名称过滤，以及复制粘贴快捷键。workspace 写盘成功后通过 watch 更新现有 worker；预览不改行为。连接策略下次连接捕获且 profile override 优先，旧上传准备撤销并保留 join；未迁移设置仍归 E7R。

- 从干净的 `master`（`15404e1aab2963ce452c3da1f634cc6260b1f819`）创建并切换到 `codex/egui-ui-migration`。
- 读取项目规范、双语架构入口、模块及工具链事实；确认现有 47 份 Slint、16,353 行，UI 之外的库领域未直接使用 Slint 类型。
- 初始评估把应用状态细化为局部提取：媒体/外部编辑仍留默认应用，几何为路径字符串；Linux icon decoder 的 Slint 依赖已在 E6 图标切片移除。
- 核对上游 egui 0.37 的 Rust 1.95、系统字体能力与 eframe 后端；0.35 声明 Rust 1.92，后续需验证依赖闭包。
- 按用户补充，只读检查 FigCraft 固定版本 `a86897d` 的 App、主题/控件、窗口、render worker、canvas 和翻译；在双语计划中明确借鉴范围、有界适配和独立清理。
- 写入 D1 方向、E0–E9 依赖顺序、feature/build/翻译校验迁移、快照丢帧正确性、重复 pass 操作去重及阶段验收；旧发布目标保留在本月追加历史。
- 加入默认 slint-ui/可选 egui-ui、独立 ax_ssh_egui binary 和 cfg-gated Slint build；包 MSRV/五平台 CI/README 同步为 1.95，egui/eframe 固定 0.37，默认 Slint 1.18.1 未升级。
- 实验 controller 持有 PTY/parser task、64 命令有界队列、完整可见快照、16ms 展示合并及显式取消/join；不接触保存的会话或凭据，日志目录独立。UI 绘制可见行并按 revision 缓存，基础输入/IME/选择/颜色/滚动/鼠标与尺寸链路完成。
- 真实 `/bin/sh` 输入/resize/清理、过载/失败、跳过快照行 revision、egui 无窗口文字与预编辑/提交、几何/颜色回归已补充。参考 FigCraft 只用于模式核对，无依赖/资产复制。
- 已启动 `target/debug/ax_ssh_egui`；日志未报告错误，进程与其子 `/bin/zsh` 持续存活。未截图或判断视觉正确，窗口留给用户体验。
- 根据用户截图继续 E2：实现 SGR 像素坐标、轮滚批量上限、motion 丢弃保留可靠队列、按住鼠标释放兜底、resize 去重、双击词选/三击逻辑行选和 TextInput 辅助名称。普通/IME 文本同帧或相邻帧时以 IME commit 去重；窗口字体尺寸变动和选择结果均使用精确请求身份处理。截图中连续提示符未作为错误结论，必须由输入字节回归判断。
- 对照 egui-winit、winit 与 Alacritty 的 IME 路径：preedit 只作本地候选显示，commit 是唯一 PTY 写入；egui 输出 Terminal purpose 和光标矩形，由 eframe/egui-winit 启用平台 IME 并更新候选窗锚点。新增真实 PTY 回归覆盖同帧 `Text`、`Ime::Commit` 与 Enter，确认中文只到达 shell 一次。
- 按用户后续截图继续 E2：ASCII/框线按模型列独立布局，覆盖四种 DPI/字形样式；每帧向 egui 重新取得 galley，避免其字形图集回收后缓存旧 UV。滚轮统一经 model 的协议编码，横向轮与 alternate-scroll 共用有界鼠标事务。
- E3 首切片已接通两套前端：公共 controller 不再仅由 egui feature 编译；共用有界鼠标编码、`TerminalOutput`、`TerminalFrames` 和 `WorkerShutdowns`。Slint 保留标题、剪贴板连接/权限代次和调度策略，egui 继续禁用标题/OSC 52 平台动作。清理任务在移除 Tab 时登记，退出统一排空；同步输出与待消费脏行合并规则沿用既有实现。
- 通用滚轮/Home 与 cleanup 回归迁至 controller；修复提取后的常量/测试导入，并将 egui 像素鼠标测试改为单次普通按下、拒绝重复普通按下，符合既有输入入口契约。共享层 15 项聚焦回归和两前端完整门禁通过。
- E3 第二切片已接线：`TerminalViewport` 统一拥有视口事务、非负选区代次与帧准入；两前端的 resize/scroll/return-to-bottom 共用状态规则，`TerminalFrames` 收为内部实现。普通输出及单纯展示失效保留选区身份，字符尺寸/历史位置改变才推进代次并丢弃旧帧。清除 Slint/egui 中重复的代次和失效状态。
- 新增四项共享视口回归，以及真实 PTY 过期 Copy/selection 和 resize 拒绝重试回归。后者定位并修复 Local PTY/controller 在接纳失败前缓存 resize 的问题；失败不提前改变几何或选区身份，重复请求继续返回真实结果。共享层 21 项聚焦回归、默认 411+506=917 项和 egui 420+15=435 项完整回归及两前端构建通过。
- E3 第三切片提取 `ConnectionIdentity` 与 `ReconnectState`：协议/profile/attempt 身份及五次重连预算/代次由 controller 持有，应用层消费接口，清除重复字段。精确 Tab 查找、主机信任/认证、安全提示、worker 所有权、SSH attempt 变化时的延迟重置和媒体/剪贴板撤销仍由应用层保持。
- 新增 7 项纯连接状态回归，覆盖协议隔离、同 profile 重复连接与 SFTP companion 独立、重连在途/worker gate、上限、代次取消及回绕；99 项状态和 19 项连接相关回归通过。默认 418+506=924 项、egui 427+15=442 项完整回归、严格 Clippy/组合检查及两前端可执行文件构建通过。E4 的后续实现见下一条。

- E4I 已落地：`TerminalSession` 复用现有 Local/SSH/Telnet/Serial worker，egui 提供临时连接草稿、密码/私钥/SSH agent、信任/认证提示、断开和手动重试。秘密使用最多 16 KiB 的 Zeroizing 负载，不进入快照/日志/持久化，密码编辑 undo 清理；确认未知密钥须重新输入凭据。系统 known_hosts 仅读取，仅本次信任不写入；变更/撤销及 probe 后换钥持续阻断。临时 SSH 禁用 X11/麦克风初始化，默认 Slint 行为保留。
- 可选自动重连复用五次预算，控制器持有 1/2/4/8/16 秒期限；SSH 密码/口令重新索取，agent/直连可无秘密重试。替换先丢弃旧 receiver 并 join worker，重置完整帧及选区，连接代次拒绝旧输入/复制/提示。八项真实远端链路回归覆盖认证前拒绝、精确 prompt、认证重试、换钥、取消 probe、Telnet/Serial 失败、重连和期限撤销；另补视口重置、草稿验证/秘密失败清除/undo 回归。

- E5I 已完成：两前端共用从 `app/panes.rs` 提取的 PaneTree；实验 workspace owner 统一持有 Tab 顺序、逐 pane 终端/关闭清理、目录版本与精确请求回执、独立 ConfigStore 和两秒变化 checkpoint。保存会话/分组复用 domain 校验及原子 blocking 写入，写入成功后发布，失败/旧编辑保留草稿；资源上限包含关闭中的任务，未消费回执有界。UI 增加搜索/分组/编辑/复制/删除、Tab 排序、分屏/分隔拖动/方向焦点和路径导入/导出。
- E5 恢复持久 UUID/根 Tab 标题、顺序、比例和活动焦点；全部终端恢复 idle，无自动 process/network/device/认证动作。导入先校验再取消/join 旧工作区，损坏启动文件保留。快照不保存终端输出、凭据、临时目标、草稿或仅本次信任；源配置/凭据与默认应用隔离。pane 控件/密码 ID 独立，禁用编辑器不把文字发给终端，Tab 切换撤销旧焦点和远端按住状态。增加 9 项 workspace、共享树非法恢复和 3 项 egui 回归；6 项原有 pane 回归从 binary 移入共享 library。

- E6 egui 切片已扩展：远程 pane 创建独立 `SftpSession` companion，使用既有 `SshSessionHandle::spawn_sftp`，不共享终端 russh handle；连接先执行 host-key probe，未知 key 只能在明确 prompt 下本次确认，changed/revoked 仍拒绝，认证重试只接收新的 zeroizing secret。SFTP 远端目录页按当前 request ID 接纳，旧页丢弃；本地目录读取在 companion 的 blocking worker 中执行，仅发布有界 `LocalEntryView`，egui 可浏览子目录并选择正规文件进入上传表单。下载、上传、进度、取消和冲突决策事件有界并由 companion 维护快照。上传源在入队前复验正规文件与大小上限，worker 继续负责冲突探测和原子发布。关闭 pane/tab 时终端和 companion 分别 cancel/join，workspace 退出统一 drain cleanup。

- E6 图标切片：将原平台 provider/cache 和九项回归移入 `src/file_icons.rs`，默认应用保留 re-export adapter；两前端使用同一 bounded key/fallback/platform-theme 缓存。Linux SVG 改用固定 resvg 0.48.1、禁用默认 features 与外部/内嵌 image resolver，UTF-8/2 MiB/512px 源限制，输出 24×24 straight RGBA。companion 给 remote/local 条目发布类型 key 和最多 64 个共享图标，持有/join 单个 blocking 预热任务，完成后刷新当前目录。egui 每 pane 最多 64 个纹理，复用未变像素、隐藏/关闭/最小化释放；未解析类型保留通用 fallback。补缓存 key 复验、SVG 资源拒绝/alpha、纹理复用/上限、真实 local facade 和 probe 取消安全/关闭回归。host-key receiver 在其它事件唤醒间保留，probe task 取消/join，秘密在关闭时清除。

- E6 编辑切片：共享 `remote_edit` 状态机/私有副本与 `file_identity`，默认适配保留动态设置、watcher 和既有行为；实验 companion 增加独立 cache、最多八编辑/128 副本、单 blocking job，500ms 稳定文件观察、自动/手动回传、停止与重开。任务带 generation/edit UUID，旧副本完成只清理不启动应用，停止/断开撤销 launch permit，退出 join 本地工作；已发布副本保留。正文不进快照/日志/持久工作区，SFTP 原有 size/mtime/permission 与 atomic overwrite 冲突保护继续生效。egui 控件按 frame 去重并跳过 sizing，使用默认 4 MiB，E7 再迁移设置。13 项既有编辑回归迁共享层，新增真实 loopback、opener 注入、冲突、停止迟到下载、待处理副本清理及 UI 去重测试。

- E6 拖放切片：本地递归上传发现提取到共享 `local_upload`，默认应用只保留薄适配；egui 增加同 pane 双向栏内复制和系统文件/目录拖入远端区域。载荷携带 pane/连接代次/目录 revision，裁切区域和当前原生指针共同选目标，无坐标拒绝。上传准备单 blocking task，扫描前后复验目标连接/request/目录及内部来源快照；可取消，退出 join，准备结果不因导航/断开而发布。根/实验文件批次最多 64，保留共享 scan/depth/path 上限、默认过滤和 worker 大小/identity 检查，正文不在 UI 读取。补多 pass/禁用/区域外/跨 pane 回归、真实递归上传与拖回本地、导航取消丢弃及关闭 join。修复传输 progress 状态和冲突 marker 清理，保持历史有界。

## 验证

- E6R-UploadSelect：6 项 controller/2 项 UI 聚焦与 egui strict Clippy 通过；真实混选递归/字节/过滤/一个 batch UUID、冲突明确选择、取消/导航、65 文件整批拒绝、文件替换/目录变类型、旧版本与 11 locale/多 pass/busy 资格覆盖。765×11 PO/300 egui 键已生成，egui 599 library + 105 binary = 704 全量及 check/严格 Clippy/build 通过，默认 check/严格 Clippy/test/build、组合及双 macOS target 已通过；E6R-UploadConflict 随后完成。

- 已完成：E6R-Select 默认 490 library + 450 binary = 940、egui 593 library + 103 binary = 696 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、764×11 PO/生成（含 header 为 765）、299 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。 新增五项 controller/两项 UI 回归，已有共享发现预算测试补 512 满槽/释放边界；覆盖非法/重复/超量/文本限额、Full/Closed/部分失败继续、旧代次与源/目标，真实混合递归/过滤/空目录失败、冲突不覆盖且其它任务完成、取消清理，以及 11 locale/路径与类型/64 上限/多 pass/摘要不改选择。根行/发现阶段 Cancel 保持原实现，入口只补既有总预算检查。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/选择/真实服务器下载验收保留，未自主截图。无新依赖、持久 schema、wire 协议、队列、task 类型或工具链/CI 变化；SSH 信任保持默认拒绝。未提交/推送/发布或使用多 agent/Worker。

- 已完成：E6R-Mutate 默认 490 library + 450 binary = 940、egui 588 library + 101 binary = 689 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、763×11 PO/生成（含 header 为 764）、297 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。 五项 controller/三项 UI 新回归通过：非法名称/路径、真实 Full/Closed、不消费不释槽、旧代次/目录/条目变化，真实冲突重试/精确字节/文件删除/空目录删除/非空保留、关闭/断开/未知回执与导航不回跳，11 locale/真实行入口/多 pass/sizing/禁用/草稿保持及 workspace blocked。路径使用稳定 ID/标准截断，错误区最高 120px。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/焦点/键盘及真实服务器 rename/delete 验收保留，未自主截图。无新生产 task/channel、依赖、持久 schema、SSH 信任/协议、工具链或 CI 变化；未提交/推送/发布或使用多 agent/Worker。

- 已完成：E6R-Batch 默认 490 library + 450 binary = 940、egui 583 library + 98 binary = 681 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、760×11 PO/生成（含 header 为 761）、290 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。 新增三项 controller/两项 UI 回归；覆盖输入限额/重复、逐项失败继续/重试、旧代次与真实双下载 Pause/Resume/Cancel/精确字节/临时清理，11 locale 真实选择、重复 pass 和生命周期隔离通过。汇总仅表示入队，无可靠完成回执队列。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/滚动/真实服务器批次行为验收保留。无新依赖、持久 schema、worker 协议、工具链或 CI 变化；未自主截图、提交/推送/发布或使用多 agent/Worker。

- 已完成：E6R-Pause 默认 490 library + 450 binary = 940、egui 580 library + 96 binary = 676 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、281 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。五项 controller、五项 SFTP UI 聚焦（其中新增两项）和 egui strict Clippy 先行通过；真实 pinned peer 使用 READ 闸门与 SystemRelease FIFO barrier，无 sleep 猜测。等待阶段、Full/Closed 类失败不提交状态、未知上传能力、终态不复活、64 history 回收、旧 generation、实际字节/临时文件清理及全语言/多 pass/禁用/布局边界均覆盖。未新增 PO，静态目录已生成；无新依赖/持久 schema/共享 worker 协议/工具链/CI 变更。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行和 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/键盘/滚动/真实服务器暂停延迟及取消验收保留。未自主截图、提交/推送/发布或使用多 agent/Worker。

- E6R-Pause 宿主门禁：默认 490 library + 450 binary = 940、egui 580 library + 96 binary = 676；两前端 locked/offline check/严格 all-targets Clippy/test/build、组合 all-features/all-targets check 已通过，目标检查记录另行收口。

- 已完成：E7R-Transfer 默认 490 library + 450 binary = 940、egui 575 library + 94 binary = 669 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令的 target check/严格 Clippy 全通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、279 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 校验通过。五项原默认纯回归迁入 library，新增一项共享恶意协议/限额、三项 egui-only controller 和六项 binary 回归；默认持续覆盖原适配器持久化/取消。字节/字符边界、保存失败重试、空组/旧 revision/活动 pane 身份、64 请求预算与关闭排空、回执归属/关闭/失焦/多 pass 单次复制及 11 locale 稳定 JSON/快捷键输入均通过。沿用已有 PO，生成静态表已更新；无新依赖、持久化 schema/格式/transport/工具链/CI 变化。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI。本机未安装 1.95；原生 GUI/菜单/焦点/剪贴板验收保留。未自主截图、提交/推送/发布或使用多 agent/Worker。

- 已完成：E7R-SFTP-Key 默认 484 library + 455 binary = 939、egui 566 library + 88 binary = 654 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端按 CI 原命令的 target check/严格 Clippy 全通过。最终平台别名补充后重跑 egui 全量/构建/组合及双 target 门禁通过。17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、275 egui 源键、116 图标/5 生成文件、skill/tracker/fmt/diff 通过。五项新增覆盖全语言真实输入/稳定焦点/限额、全部八键别名/顺序/保留键与跨平台模式、enabled/repeat 路由、实际 workspace 多 pass/路径文本/无连接/混合 pane/迟到请求、sizing/禁用清除；两项设置保存/预览/失败/重试回归扩展通过。未改 PO/生成表、依赖/schema/transport/工具链/CI。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 原生链接/运行与 Rust 1.95 仍需 CI，本机未安装 1.95。原生 GUI/菜单/键盘/焦点验收保留，未自主截图、提交/推送/发布或使用多 agent/Worker。

- 已完成：E7R-Shells 默认 484 library + 455 binary = 939、egui 566 library + 83 binary = 649 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 及 fmt/diff 通过；17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、275 egui 源键、116 图标/5 生成文件、skill/tracker 校验通过。四项新增、14 项设置聚焦通过，包含 11 locale 选择/Unicode/稳定 ID、满列表当前值、Context 隔离、无脏/失败保存/重试及新终端实际 resolver 消费。未改 PO/生成表，无依赖/schema/平台 cfg/工具链/CI 变更；本轮未重复显式 target/MSRV 或原生 GUI 验收。

- 已完成：E7R-Icons 默认 484 library + 455 binary = 939、egui 566 library + 79 binary = 645 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 通过；ARM64/Intel macOS 两前端 CI 原 target check/严格 all-targets Clippy 全部通过；Intel 仅交叉编译，未链接/运行；17 项 Python、56 Markdown/入口、758×11 PO/生成（含 header 为 759）、275 egui 键、116 图标/5 生成文件、skill/tracker/fmt/diff 通过。9 library/6 binary 图标、10 设置、6 既有 workspace 与新增 Sidebar/Tab 组合聚焦回归通过；句柄限额读取追加后重跑图标通过。egui normal/build graph 无 Slint/softbuffer，resvg 可选 build 依赖沿用锁定版本，无新包版本/schema/工具链/CI 改动。

- 已完成：E7R-FPS 默认 480 library + 458 binary = 938、egui 562 library + 71 binary = 633 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 及 fmt/diff 通过。43 项初始 controller 聚焦、修正后 8 项 refresh 聚焦与全语言控件、9 项设置回归通过；17 项 Python、56 Markdown/入口、756×11 PO/生成（含 header 为 757）、271 egui 源键、116 图标、skill/tracker 校验通过。无平台 cfg/依赖/schema/工具链/CI 变更，未重跑显式 target/MSRV 或真实 GUI。
- 已完成：Rust 修改前 Theme 完整基线默认 938、egui 626，双前端完整门禁和组合检查通过；本轮 FPS 最终门禁同时覆盖 Theme。

- 已完成：E7B 默认 472 library + 464 binary = 936、egui 547 library + 67 binary = 614 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check 和 fmt/diff 通过。四项新增回归覆盖纯 HSL 边界、ANSI/alpha、真实模型静止行缓存/装饰、全语言键盘逐档控件；两项保存事务扩展通过。17 项 Python、56 Markdown/入口、756×11 PO/生成（含 header 为 757）、222 egui 源键、116 图标、skill/tracker 通过。无平台专用修改，本轮未重跑显式 target/MSRV 或实际 GUI；历史平台结果不作为本轮验证。

- 已完成：E7H 默认 471 library + 464 binary = 935、egui 546 library + 64 binary = 610 项完整测试通过；两前端 locked/offline check、严格 all-targets Clippy、test/build、组合 all-features/all-targets check、fmt/diff 全通过。两项共享、全语言 Unicode 控件/侧栏及两项设置事务聚焦回归通过；17 项 Python、756×11 PO/生成（含 header 为 757）、217 egui 源键、56 Markdown/入口、116 图标、skill/tracker 通过。无平台专用改动，本轮未重跑显式 target 或 MSRV；历史双 macOS target 通过不作为本轮结果。

- 已完成：E7A 的 egui-only 537 library + 60 binary = 597 项、默认 469 library + 465 binary = 934 项完整测试通过。两前端 locked/offline check、严格 all-targets Clippy/test/build、组合 all-features/all-targets check，以及 ARM64/Intel macOS 两前端按 CI 原命令的显式 target check/严格 Clippy 均通过。音源准备断开 join、格式捕获/重连/兼容 API 默认关闭、全部语言九档采样率/五档声道/协议切换及两项设置事务回归通过。标准来源已在本轮修改前补入 research；17 项 Python、56 Markdown/入口、756×11 PO（校验输出含 header 为 757 条）/生成、215 个 egui 源键、116 图标、fmt/skill/tracker/diff 通过。

- 已完成：E7X 本轮默认 469 library + 465 binary = 934 项、egui-only 535 library + 58 binary = 593 项完整测试通过，两前端 locked/offline check、严格 all-targets Clippy、egui build 和组合 all-features/all-targets check 通过；两前端在 aarch64-apple-darwin/x86_64-apple-darwin 的显式 target check/严格 all-targets Clippy 全通过。本轮无 Cargo/依赖/feature/schema/CI 变更，共享 SSH 修正同时覆盖两前端。
- 已完成：24 项 X11、两项设置事务与 28 项终端 controller 聚焦回归通过。新增两项共享 setup 回归覆盖明确回执、早到 stdout/stderr、消息/字节上限和关闭；两项真实 loopback 覆盖拒绝后 shell 保持/Off 重连和等待确认时断开 join，两项全语言真实 UI 操作覆盖模式选择/路径保持/兼容草稿。既有信任前无请求、保存成功/失败和 clipboard 合并撤权回归已扩展。17 项 Python、756×11 PO/生成、207 个 egui 源键、fmt/56 Markdown/入口/skill/tracker/diff 通过，复用 22 个已有键。
- 未完成：真实 XQuartz/授权/X11 应用及设置 GUI、其它原生功能和 E2V/E4V/E5V、E3R/E6R/E7R/E8R/E9 保留。Intel 仅交叉 check/Clippy，未链接/运行；Windows/Linux 完整链接/运行及 Rust 1.95 仍需 CI，本机无 1.95 工具链；未自主截图、启动真实 X server/xauth/关联应用、提交/推送/发布。

- E7R-Theme 已完成实现，完整双前端门禁已在本轮补齐；原始聚焦证据：egui Appearance 加入 Custom 选择器、Light/Dark 各 13 个有界 hex 字段、稳定角色 ID 和无障碍名称；Workspace 预览先用 `ThemeSettings::normalized` 修复非法/低对比草稿，再通过现有主题映射应用。保存仍由既有 catalog revision/request 事务完成，失败保留草稿和旧值。新增全语言 26 ID/输入回归及非法 hex/低对比保存回归；默认 config 聚焦回归、egui binary 全量 70 项（含设置回归）、egui-only strict Clippy/check 和翻译/diff 校验通过。

- E7 设置首切片与 E8 菜单/Tab 首切片已实现：AppSettings 由 Workspace owner 以有界 CatalogAction 保存；设置单例 Tab 可预览、保存、还原和关闭确认，草稿不进入快照；Tab 支持拖动排序、中键关闭、稳定 ID 和多 pass 去重。
- 菜单按 Slint File/Edit/View/Pane/Window/Help action 表统一；macOS 接 AppKit、Windows 接 Win32 muda 0.19.3，Linux/X11/Wayland 在现有 winit 窗口中用 egui MenuBar；GTK 未引入。


- E7I 已接通图片事件、单后台反预乘/PNG、每终端准备/ready/确认/取消及 Local/SSH 文件生命周期。Local PTY、同连接固定信任 SSH/SFTP 成功/写失败、无自动回车、权限/清理、旧连接/视口/限额及 egui 多 pass/失焦回归已通过；全部必需本机门禁通过；9 项新增回归，默认 934、egui 544+62=606 项完整测试通过，两前端 check/严格 Clippy/test/build、all-features/all-targets 和双 macOS target CI 原命令 check/严格 Clippy 通过。17 项 Python、56 Markdown/入口、756×11 PO/生成、215 egui 源键、116 图标、skill/tracker/fmt/diff 通过。

## 风险与阻塞

- egui WGPU 暂无等价 CPU/damage fallback，保持默认 Slint 作为当前受支持替代；迁移最终切换前需完整 renderer 支持决策。
- 本轮测试和 egui 可执行文件构建只在宿主 macOS ARM64 完成；双 macOS target check/Clippy 已通过，Intel 仍只有交叉编译检查。Windows/Linux/Intel 的完整链接/原生 GUI 最终依赖原生 CI 和手动验收，不能以本机通过推定。
- 翻译已支持 Slint/egui 源键合并，但尚有 Slint-only 文案；E9 前须继续迁移源键和入口/边界检查，不得直接删除门禁。
- 终端字体/IME、多窗口平台集成、软件渲染和性能为主要不确定性，E2 优先验证；用户负责视觉验收。
- E7F 上限和 join 覆盖显式字体选择加载；eframe 既有按需系统字形 provider 对额外文字/emoji 的首次缺字可能阻塞，完整 provider 生命周期与覆盖仍属 E2V，未在本轮更换或关闭。

## 剩余迁移核对（2026-10-26 22:22 +0800）

- 已实现：音频格式/连接级麦克风（E7A）、图片附件（E7I）、侧栏遮罩（E7H）、文字亮度/粗体亮色（E7B）、语义高亮（E7Q）、自定义 Light/Dark 调色板、前台/后台刷新率（E7R-FPS）、系统图标/本地覆盖（E7R-Icons）、shell 选择/发现缓存（E7R-Shells）、SFTP 菜单/快捷键（E7R-SFTP-Key）、会话/分组 JSON 交换（E7R-Transfer）、单项下载暂停/继续（E6R-Pause）、传输批量选择/控制（E6R-Batch）、单项远端重命名/删除（E6R-Mutate）及远端多选/批量下载（E6R-Select）。不得把这些入口再列为未迁移；本轮完整自动门禁已通过，原生验收保留。
- 设置/快捷键：全部八项持久化快捷键均已接通编辑与菜单路由；SFTP 只定位当前保存 SSH pane 的 companion，会话导入走显式粘贴/自动合并，导出提供目标选择/Copy。原“已知 shell 管理”已按真实发现/选择缓存完成；两前端均无独立候选增删页。collapsed_group_label_chars 当前仅兼容传递，无 UI 消费，应在紧凑侧栏迁移时决定行为，不直接增加无效控件。
- 会话/凭据/工作区：远端新系统探测/会话级图标 picker、记住凭据、系统 keyring/加密保险库管理与迁移仍未接入，controller 仍拒绝凭据后端 profile 的相关修改；定向分组导入、侧栏复制快捷操作、完整高级参数对照、原生文件选择器及最近工作区菜单仍保留；核心 JSON 自动合并/导出已实现。现有 `.axssh` 路径式打开/保存已实现。
- SFTP（E6R）：单项/批量下载暂停/继续、任务取消与入队汇总、单项远端重命名/删除、远端多选及按钮批量下载已实现；删除明确确认且非递归，上传无暂停/跨连接断点。本地多选/批量上传与 egui 选中多项拖放已复用既有批量意图；批量修改、原生拖出和完整 UI/command 对照仍未补齐。
- 终端交互：URL/路径点击打开与 OSC 窗口标题等仍需对照接线；语义识别/着色已实现，不能据此推定交互已完成。
- 平台/渲染（E8R）：多窗口/分离 Tab、跨窗口资源和焦点生命周期、完整 DPI/辅助技术收口，以及 egui 的 CPU/软件降级与性能策略仍开放。Slint 专用 drawing/cache 参数须作 egui 适配决策，不直接照搬为失效控件。
- 已实现待验收：真实 IME/Option/AltGr/字体/鼠标/剪贴板/拖放、SSH agent/加密私钥/串口、X11、麦克风、辅助技术及真实多连接生命周期；其它原生平台、Intel 链接/运行和 MSRV 证据仍按 CI/记录边界保留。
- 最终切换（E9）：功能完整对照、性能报告、旧配置/凭据兼容、默认入口切换、Slint 依赖/构建路径清理与发布许可收口均未完成；当前默认仍为 Slint。

## 下一步

- 已完成：E6R-UploadSelect 的有界路径/目录快照/后台准备和 Local files 选择链已完成；E6R-SelectedDrag 让已选行拖放复用 `upload_selected`/`download_selected`，未选行保留单项路径。后续优先批量远端修改和原生拖出，再做完整 SFTP 对照与平台验收。
- 已完成：E6R-SelectedDrag 聚焦回归覆盖有界 payload 的已选/未选路径分流、pane/generation 拒绝和既有 native drop 多 pass 行为；egui 599 library + 107 binary 全量测试、默认 490 library + 450 binary、两前端严格 Clippy/check/build、组合 all-features check，以及翻译/仓库/Markdown/tracker/diff/fmt 门禁通过。该切片无平台 cfg，未新增依赖、协议、schema、队列或任务；原生拖出、真实平台/服务器与 Windows/Linux/Rust 1.95 仍由独立验收/CI 负责。
- 所有权：UI 仅持有界选择与意图，controller 保存私有快照和单个扫描任务；不交出锁/receiver/句柄/指纹，不在 UI 扫描或 await。不新增后台任务类型、队列、协议、依赖或持久 schema，仍经既有 SSH trust 和 native upload identity 复验。
- 边界：沿用最多 4096 扫描项/512 文件/256 目录/16 层/512 KiB 发现文本与单文件限制，实验端最终 <=64 文件；一个错误拒绝准备批次，上传入队才建 history。源目录观测不是原子快照；冲突仍按现有逐项确认，不自动覆盖。Keep both/应用冲突到批次和选中多项拖放已接通；批量修改/原生拖出仍后续。
- 默认仍 Slint；GUI/真实服务器、其它平台/MSRV/Intel 运行独立验收。E7R 为唯一 in_progress 总行，无多 agent/Worker。

## 最后更新时间

- 2026-10-27 12:00 +0800
