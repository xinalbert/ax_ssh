# 当前项目实施记录

## 当前目标

- 目标 ID：20261009-workspace-navigation-release-info
- 目标：按示例整理连接侧栏和标签显示，接入文件夹图标、可用搜索和 SSH 远端系统 release 信息，并为探测失败提供无干扰回退；Local Shell 判断本机系统，Linux 识别具体发行版，并统一维护系统名称/别名/图标路径；增加手动图标覆盖、逐服务器识别缓存与 Group 图标尺寸规范。
- 交付物：Slint 导航/标签更新、受限 SSH release 探测、Iconify 文件夹资源与许可记录、统一图标目录和生成器、双语名称索引/行为说明和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/`、`build.rs`、`src/config.rs`、`src/config/`、`src/app.rs`、`src/app/`、`src/ssh.rs`、`src/ssh/`、`assets/os-icons/`、`scripts/`、翻译目录与生成映射、双语架构/用法文档、tracker 和用户授权的本地提交整理。
- 不在本轮范围内：依赖升级、SSH 信任/凭据策略、推送或发布。
- 所有权：config 负责主题着色、手动图标标识和有界目标绑定识别缓存；Slint 负责展示和搜索意图；AppState 负责过滤模型、图标优先级、逐 profile 探测租约和后台串行保存；SSH worker 在已认证连接上使用独立 exec channel 读取短版本标签。
- 安全边界：不改变 host-key trust 或凭据流程；探测命令不进入交互终端，限时限量，失败保留已连接状态。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| OE1 | completed | 系统图标补漏、来源分类和文档收口 | 结构/来源/哈希/渲染及文档链接 | 328 个资源保持既有审计结果 |
| UI1 | completed | 侧栏搜索与 Iconify 文件夹图标 | Slint 编译和筛选映射测试 | 搜索由 AppState 过滤模型 |
| UI2 | completed | SSH/SFTP 成功后读取并显示 release | 解析边界测试、超时/输出上限检查 | 探测失败静默回退，不影响 shell |
| UI3 | completed | 标签系统图标、双语架构与来源文档 | cargo check、clippy、tests、链接和 tracker | UI 视觉由用户截图验收 |
| UI4 | completed | 收起侧栏只显示文件夹/系统/服务器图标，展开服务器行同步显示系统图标 | 图标映射与 release 更新回归、Slint 编译 | 名称保留在 tooltip/辅助技术标签；刷新保持 Group 展开状态 |
| UI5 | completed | 双语行为文档、最终差异和完整验证 | 仓库规定的完整检查 | 本机自动验证通过；历史 tracker 格式问题单独记录 |
| UI6 | completed | 系统图标默认彩色，随主题着色改为默认关闭的持久化配置 | 默认/旧配置/显式开启/规范化与保存回归，Slint 编译 | 配置值在应用层映射，文件夹等 UI 符号继续使用主题色 |
| UI7 | pending | 用户验收原生界面和真实 SSH release | 深浅主题、图标 tooltip、焦点与连接后的系统图标更新 | 按仓库要求由用户验收，不截图应用 |
| UI8 | completed | Local Shell 侧栏/标签按本机系统显示图标，并修正 release 图标刷新比较 | 本地/远端来源映射、标签模型更新回归、完整 Rust/Slint 门禁 | 默认彩色与既有主题着色配置共用；无额外探测或 schema 变更 |
| UI9 | completed | Linux 按发行版选择已有图标，Tux 仅用于通用/未知 Linux | release 与本机 os-release 来源映射、未知/派生发行版回退、共用图标资源和完整门禁 | 本机读取在后台执行，Slint 只接收标识，复用彩色配置 |
| UI10 | completed | 统一系统名称、规范标识、别名与图标路径目录，生成 Rust/Slint 映射 | 单一目录校验、生成结果一致性、双语使用说明 | 仅使用仓库已收录资源，运行时不读取目录或执行生成脚本 |
| UI11 | completed | 持久化手动图标与服务器识别缓存，统一显示和探测优先级 | 配置兼容、缓存命中跳过、目标更改/重新识别、并发保存回归 | config 保存小型标识/有界 release，SSH 信任不变 |
| UI12 | completed | 服务器与 Local Shell 图标选择及重新识别入口 | 选择/自动恢复、主/独立窗口更新、Settings 保存 | 共用已有目录和图标组件 |
| UI13 | completed | Group 层级统一图标尺寸和占位 | Slint 编译、尺寸/缩进静态检查、用户视觉验收 | 层级由缩进/箭头表达 |
| UI14 | completed | 双语文档、目录/翻译同步和完整门禁 | fmt/check/严格 Clippy/完整测试/文档与 tracker | GUI 验收仍由用户负责 |
| UI15 | completed | 按目录资源、功能实现、双语文档分段创建本地提交 | 逐段暂存差异复核、生成/翻译检查、提交后状态确认 | 用户追加授权本地提交；不推送或发布 |

## 已完成

- schema v35 保存逐服务器手动图标、绑定 host/port/username/主机指纹的识别缓存，以及 Local Shell 手动选择；已有成功缓存或手动指定跳过图标探测，失败不缓存。服务器编辑器共用可搜索内置选择器，保存后的重新识别在下次连接执行；Local Shell 独立立即保存。
- Group 文件夹、服务器和 Local Shell 统一为 20px 图标占位框，保留缩进/箭头层级与 16px 标签图标；自动缓存保存等待 profile/凭据事务并保留未保存 Settings 预览。
- Local Shell 入口及主/独立窗口标签按本机系统选择图标；Linux 本机后台有界读取 os-release，未知发行版保留 Tux；标签 release/platform 更新会通知既有 model 行。
- 116 项统一 catalog 包含规范标识、显示名称、别名和资源路径，生成 Rust 识别表、共用 Slint Image 与双语名称索引；113 个 Linux 条目优先使用已有专用资源，原图 SHA-256 不变。
- 收起侧栏固定显示文件夹/服务器/系统图标；展开服务器行显示系统图标，release 更新保留同身份 Group 的展开状态。
- `settings.appearance.system_icons_follow_theme` 默认 false；侧栏和标签栏默认使用彩色系统资源，配置开启才跟随主题；旧配置、规范化和 Settings 保存保留值。
- 侧栏搜索、受限一次性 release 探测、双语行为文档和中文翻译同步完成；目录资源提交 `bbc4721`、功能提交 `a412f49` 已创建，双语文档与验证记录随文档提交收口；未推送或发布。
- 收录 328 个图形文件（312 SVG、16 PNG），覆盖 178 个系统/平台条目；原有 135 个 SVG 保持不变。
- 完成来源/作者/许可状态/修订/包完整性/SHA-256 清单、集合许可证和品牌声明、离线预览、双语目录及检索覆盖/缺口记录。
- Flatcar 官方 `icon` 目录下的彩色、白色、黑色三种文件都含完整名称，均逐项核对并作为 `wordmark` 例外记录；Windows Server 补入 Wikimedia Commons 的 Microsoft 名称标志，未裁字。
- Amazon Linux 和 RancherOS 未确认专用系统图形；Synology DSM 仅收录明确标记的厂商品牌。身份不匹配候选及社区/旧版状态均列入检索记录。

## 验证

- 已完成：本机 macOS fmt、locked/offline Cargo check、严格 all-target Clippy 与完整测试（311 library + 331 binary，共 642 项）；aarch64-apple-darwin 显式 target check/严格 all-target Clippy 已通过，显式 target 链接 build 已通过；Slint 已重新编译；514 条中文翻译、646 条相对文档链接、116 项 catalog/5 个生成文件一致性、116 项原图 SHA-256、项目 skill validator 和 `git diff --check` 通过；当前 tracker/地图及本轮新增历史无错误，全局保留 55 项既有历史格式问题。
- 未完成：原生 GUI/键盘/配色和真实 SSH release 用户验收；Linux 本机实际文件读取及其他 CI 原生目标未在本机运行。

## 风险与阻塞

- 原生配色变体覆盖不相同，未找到的 light/dark 变体没有自行配色；完整名称、字母徽记、品牌代理和未确认专用图形均在目录/清单中注明。
- tracker 全局校验仍报告 55 项既有历史记录格式问题；本轮新增记录字段完整，当前计划和项目地图通过。

## 下一步

- 用户验收 UI7：Local Shell 与远端发行版图标、配色、tooltip/键盘焦点及真实 release 更新；Linux 上确认本机 os-release 识别和 Tux 回退。

## 最后更新时间

- 2026-10-09 18:14 +0800：按用户追加授权分段创建本地提交，UI7 用户界面/真实 SSH 验收保持待完成。
- 2026-10-09 18:04 +0800：手动图标、服务器缓存、统一尺寸与自动门禁完成，等待 UI7 用户界面/真实 SSH 验收。
- 2026-10-09 17:14 +0800
