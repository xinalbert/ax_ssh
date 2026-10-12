[English](README.md)

# 界面翻译

AxSSH 使用英文源文案，并内嵌十一个翻译目录：

| Locale | 语言本名 |
| --- | --- |
| `en` | English（源语言） |
| `de` | Deutsch |
| `en-GB` | English (United Kingdom) |
| `es` | Español |
| `fr` | Français |
| `ja` | 日本語 |
| `ko` | 한국어 |
| `pt-BR` | Português (Brasil) |
| `ru` | Русский |
| `uk` | Українська |
| `zh-CN` | 简体中文 |
| `zh-TW` | 繁體中文 |

Slint 在构建期将译文编译为静态表；
运行中的应用不会扫描这个目录或解析 PO 文件。

直接编辑 `LOCALE/LC_MESSAGES/ax_ssh.po`。没有 context 的条目对应 Slint `@tr` 文案。
`msgctxt "settings-search"` 条目对应 `settings-search.json` 中的固定设置搜索项，生成静态
Rust 数组，但保持英文路由 ID 不变。

修改 UI 文案、翻译或搜索项后，在仓库根目录运行：

```bash
python3 scripts/build_translations.py
python3 scripts/check_translations.py
python3 scripts/build_translations.py --check
python3 -m unittest scripts/test_translations.py -v
```

命令需要 Python 3.10+ 和 `rustfmt`。生成器保留已有译文和译者注释、移除过期项，并为新增项
建立空译文。补全后重新运行；搜索译文缺失或标记为 fuzzy 时，生成器会拒绝生成搜索表。
`--check` 不写文件。旧命令 `build_zh_catalog.py` 会委托给相同流程处理 `zh-CN`。

新增语言时，建立 `LOCALE/LC_MESSAGES/ax_ssh.po`，填写与目录一致的 `Language` 和适当的
`Plural-Forms` 头部，并加入 `LOCALE/language.json`。例如现有法语目录：

```json
{
  "native_name": "Français",
  "aliases": ["fr-FR"],
  "language_default": true
}
```

补全 UI 与搜索译文，运行上述命令及 Cargo 验证即可。`build.rs` 自动发现翻译目录，在 Cargo
输出目录生成静态注册表；Settings 使用同一清单，无需逐语言修改 Rust 或 Slint。英文源语言只需
`en/language.json`，不需要 PO。选项顺序为跟随系统、英文、按 locale 排序的目录；索引只在本次
构建内使用，不保存到配置。

元数据采用规范的 `language[-Script][-REGION]` 标识。`native_name` 必填，其余字段可选。
`aliases` 保存同一语言的系统 locale 别名；`setting_aliases` 兼容历史配置值；`script` 指定
Hans/Hant 等书写系统，也可从带 script 的 locale/别名推导；`language_default` 为每个语言
指定至多一个回退目录，只有语言部分的目录也可作为隐式回退；`region_scripts` 按地区推导书写
系统，即使对应译文尚未内嵌。中文策略见 `zh-CN/language.json`。构建会拒绝缺少配对元数据/PO、
无效名称/标识、重复别名/默认值、未知字段和冲突的书写系统提示。

配置保存 `system` 或规范 locale（`en`、`zh-CN`、`ja`），兼容读取旧的 `english`、
`simplified-chinese`、`japanese`。系统匹配顺序为精确 locale/别名、明确或按地区推导的书写系统、
地区、语言回退、英文；不会回退到同一语言的不兼容书写系统。`zh-TW`、`zh-HK`、`zh-MO`、
`zh-Hant` 使用繁体中文，只有 `zh` 时仍使用简体中文。法语、西语、德语和韩语的地区变体
回退到对应语言目录。葡萄牙语使用巴西目录；在加入独立葡萄牙目录前，`pt-PT` 系统也回退到它。
俄罗斯的 `ru-RU` 和乌克兰的 `uk-UA` 匹配各自语言目录。英国使用 `en-GB`（兼容 `en-UK`
别名），采用 colour、licence、behaviour 等英式显示拼写；`en-US`、`en-AU` 等其它英语地区
仍使用源英文。语言代码 `uk` 表示乌克兰语，英国地区代码在 `en-GB` 中为 `GB`。

校验器遍历各目录，检查缺失、过期、空译文、fuzzy、重复键、locale 头部、复数
形式和格式参数。编号占位符可重排，但身份和出现次数必须一致。CI 和发布预检验证目录及生成表。

运行时技术错误、日志、用户值和远端终端内容不进入此目录。语言切换保留先保存再应用的事务，
并在 UI 线程同步主窗口和独立窗口。新增译文仍需用户验收用语、长度、字体及键盘/焦点行为。
