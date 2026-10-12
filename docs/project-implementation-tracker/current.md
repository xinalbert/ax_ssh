# 当前项目实施记录

## 当前目标

- 目标 ID：20261012-ru-uk-british-english
- 目标：沿用目录注册表，补入俄罗斯的俄语、乌克兰语及英国的英式英语地区版本。
- 交付物：ru/uk/en-GB 三套完整 UI/search catalog、locale 元数据与回归、英式拼写/英美匹配、12 语言 README badge/对应指南、双语说明与可运行程序；全部多语言改动分项提交并推送到 origin/master。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`translations/{ru,uk,en-GB}/`、生成的 `src/app/settings_bridge/settings_search_catalog.rs`、`src/config/{language/tests,tests}.rs`、`src/app/settings_bridge.rs` 的测试、双语使用/开发/架构与翻译说明、`docs/usage.*.md` 简明指南、两份 README badge、文档索引和地图/实施记录；按用户追加授权一并提交前三轮改动并推送。
- 不在本轮范围内：运行时 Gettext/外部语言包/下载、终端输出/日志/技术错误翻译、SSH trust/凭据/transport、Rust/Slint 依赖升级、release/tag 发布、历史重写。
- 所有权：build 发现并校验目录元数据；config 保存稳定 locale 和系统匹配策略；UI bridge 映射静态名称/临时索引并沿用保存后切换；Slint 使用 `@tr` 和数据模型；PO/JSON 由维护脚本生成静态搜索数组。SSH/凭据边界不变。
- 运行边界：不新增 IO、解析器、翻译 worker 或轮询；语言数据进入只读编译表，搜索按当前语言/固定行索引访问。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| RUK1 | completed | ru/uk/en-GB 各 665 键与静态搜索表 | 11×665 catalog/生成/占位符/复数/数字/空白、3 msgfmt 通过；英式 37 contextual keys 改写 | uk 为乌克兰语，英国使用 en-GB；不增加运行时任务 |
| RUK2 | completed | locale/搜索回归、双语说明、12 badge 与对应指南 | 19 个不同专项通过；12 语言 headless；两份 README 各 12 个 badge 对应注册表与页面，149 相对链接有效 | 保留英语/简体/日语指南，补齐其它 9 个简明指南及双语索引 |
| RUK3 | completed | 全量验证与可运行 binary | fmt/check/严格 Clippy/build、串行 770 Rust、22 Python/翻译/链接/skill/current-map/diff 通过 | 标准默认并发 363/367 library，4 项既有音频 fixture 超时；全局 tracker 旧历史字段债务保留 |
| RUK4 | pending | 用户语言/GUI 验收 | 西里尔字形、俄乌用词/长文案、英式拼写、主/独立窗口及键盘 | 不自行截图或宣称母语审核 |
| RUK5 | completed | 实现、双语文档及记录分项提交 | 实现 e7276c2、文档 434fe0b；记录随包含本页的提交 | 每组显式暂存并复核 index，无构建产物/缓存；保持追加历史 |
| RUK6 | completed | origin/master 推送与远端 SHA 核对 | 实现及文档已推送，ls-remote 与 434fe0b 完整 SHA 一致 | 直连成功，无需代理；包含本记录的提交随后同步并核对 |

## 已完成

- ru/uk/en-GB 三目录各补齐 547 UI + 118 search keys；俄乌文案逐项整理，英式英语在 37 个 contextual keys 使用 colour/licence/behaviour/maximise/minimise 等拼写。现为源英文加 11 PO，共 12 语言/地区版本。
- ru-RU/uk-UA 匹配各自语言；en-GB/en-UK 使用英国版本，en-US/en-AU/en-CA 和语言 en 保持源英文；uk 为乌克兰语，不与英国 alias 混淆。所有 metadata 驱动，无新增生产 Rust 分支。
- 新增英式英语地区回退及英美拼写搜索测试；已有 locale/全部 search 行/所有注册语言 headless/model/旧配置/保存事务测试覆盖新目录并通过。
- 两份 README 各 12 语言 badge，与所有 native_name/guide 逐一对应；保留原英语/简体/日语指南，补齐 de/es/fr/ko/pt-BR/zh-TW/ru/uk/en-GB 九份简明指南，实际菜单词从 PO 对齐；双语目录同步，149 相对链接有效。
- 双语架构/使用/翻译维护说明同步，README 移除重复语言枚举而以 badge/链接为入口；完整跟踪，无联网研究或多 agent/Worker。
- schema v36、Rust 2024/MSRV 1.92/Slint 1.18.1 保持；运行时不读 PO/元数据，不增加后台任务；SSH/凭据/音频/终端和 lockfile 无 diff。
- 按用户授权将全部多语言实现提交为 e7276c2，README badge/指南及双语说明提交为 434fe0b；已直连推送 origin/master 并核对远端完整 SHA。记录随包含本页的提交同步，无 tag/Release 或历史重写。

## 验证

- 已完成：Slint 重编、fmt/locked-offline check/严格 all-target Clippy/build、串行 367 library + 403 binary（770）、19 个不同专项、22 Python、11×665 catalog/生成/参数/复数/数字/空白、3 msgfmt、README 各 12 badge 与注册表/页面一致、149 相对链接、skill/current-map/diff；全局 tracker 无新增错误；程序 `target/debug/ax_ssh`。
- 未完成：用户母语/GUI/长文案/西里尔字形与键盘、Windows/Linux 原生 CI、本机未安装 Intel target；标准并发 4 项音频 fixture 超时与全局 tracker 历史时间字段债务。

## 风险与阻塞

- 全部新增语言/简明指南的母语用词、西里尔/Hangul/繁体字形、长文案/窄窗口与键盘仍须用户验收，不将结构校验或 headless 选取当视觉/母语审核。
- binary 静态数据随语言增加，不承诺零体积/零 RSS 增量；未增加运行时 IO 或常驻任务。
- 标准默认并发测试 363/367 library 通过，3 automatic audio helper 在 automatic/tests.rs:202、1 microphone helper 在 media/tests.rs:201 超时；音频源码无 diff。完整串行 770 项通过，不降低 CI 门禁或宣称修复无关时序风险。
- tracker 全局校验已有历史时间字段错误；本轮新增记录遵守完整字段，不虚构既有时间。

## 下一步

- 从 `target/debug/ax_ssh` 在 Settings > General > Language 验收 12 语言/地区版本；两份 README badge 链接对应指南。其它目标平台 CI 保留。

## 最后更新时间

- 2026-10-12 18:35 +0800
