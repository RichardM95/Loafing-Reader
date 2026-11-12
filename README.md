# Loafing Reader

Windows 本地 TXT／EPUB 悬浮阅读器，使用 Rust 编写，面向 Windows 7 SP1 至 Windows 11，提供 32 位和 64 位免安装包。Win7 SP1 已由用户实机验证通过。

**制作者：扯蛋虾米 · 版本号：0.1**

[GitHub 仓库](https://github.com/RichardM95/Loafing-Reader) · [Releases](https://github.com/RichardM95/Loafing-Reader/releases)

## 下载与使用

1. 下载 [64 位版](https://github.com/RichardM95/Loafing-Reader/releases/download/v0.1.0/Loafing-Reader-win64.zip) 或 [32 位版](https://github.com/RichardM95/Loafing-Reader/releases/download/v0.1.0/Loafing-Reader-win32.zip)。Win11 使用 64 位版，其他版本请按系统位数选择。
2. 解压后运行 `LoafingReader.exe`，在“阅读”页点击“打开小说”，或从“最近阅读”继续。
3. 关闭设置窗口只会隐藏它；从托盘“系统设置”重新打开，从托盘“退出”完整关闭程序。

更新时先通过托盘退出旧版，再解压替换文件。设置、阅读进度和导入字体会保留。

## 功能与设置

- **悬浮阅读**：无边框置顶，文字与背景分别调节透明度；根据窗口高度自动显示一行或多行，保留原文换行。
- **目录与搜索**：自动识别 TXT 章节，读取 EPUB 书内目录，支持章节跳转和正文搜索。
- **字体与外观**：选择系统字体或导入 TTF／OTF／TTC，调整字号、字重、斜体、字距、行距与颜色。
- **阅读进度**：在“系统设置 → 阅读”中切换全书／章节进度，百分比保留两位小数，切换后立即保存；可在“外观”中关闭行末进度显示。
- **多屏定位**：选择显示器、调整位置与尺寸，支持居中和窗口找回；“重置位置”保留窗口尺寸。
- **翻屏操作**：鼠标左键下一页、右键上一页，支持自动翻页和全局快捷键。

TXT 自动识别编码，支持 UTF-8、UTF-16、GBK／GB18030、Big5。乱码或目录识别不合适时，可在阅读页“编码与章节规则”中调整。

EPUB 2／3 按书内顺序提取正文，使用软件的字体与排版设置。不支持加密、图片型书籍，也不保留原书图片与复杂版式。

外观修改先显示在预览区。外观、阅读操作、窗口位置和快捷键修改后，点击“应用更改”才会生效并保存；“还原更改”恢复上次已应用的值。切换分类或隐藏设置窗会保留草稿，退出程序会丢弃未应用内容。

## 默认快捷键

| 操作 | 快捷键 |
| --- | --- |
| 上一页 | `Ctrl+Alt+,` |
| 下一页 | `Ctrl+Alt+.` |
| 老板键 | `Ctrl+Alt+M` |
| 自动翻页 | `Ctrl+Alt+P` |
| 找回窗口 | `Ctrl+Alt+R` |

在“快捷键”页点击对应区域，按下组合键并松开，再应用更改。若提示冲突，请更换组合键；原有效配置会保留。

老板键会隐藏阅读窗和设置窗，并暂停自动翻页。阅读窗移到屏幕外时，可从托盘选择“找回窗口”，将它移到鼠标所在屏幕居中。

## 数据与备份

设置、阅读进度和导入字体保存在 `%APPDATA%\Thief-Reader`，备份时复制整个目录即可。目录保留旧名以兼容已有数据；也可通过 `THIEF_READER_DATA` 环境变量指定其他保存位置。

## 从源码构建

<details>
<summary>构建环境与命令</summary>

需要 Windows x64、PowerShell 7、Rust **1.99.1**（含 `rust-src`、`rustfmt`、`clippy`）、Visual Studio 2019 的 **v142 14.29.30133** x86/x64 工具链及 Windows SDK **10.0.19041.0**。首次构建需要联网下载依赖。

```powershell
rustup toolchain install 1.99.1 --profile minimal --component rust-src,rustfmt,clippy
rustup override set 1.99.1
.\build.ps1 test
.\build.ps1 clippy
.\build.ps1 release
.\build.ps1 package
```

免安装包和源码 ZIP 输出到 `dist/`。单独构建一个架构：`.\compat-build.ps1 -Task build -Architecture x64`（或 `x86`）。

</details>

## 许可与参考

- 项目许可：[LICENSE](LICENSE)
- 第三方许可：[Slint](licenses/Slint-License.md) · [EPUB 解析依赖](licenses/EPUB-Dependencies.txt)
- 界面与数据处理参考：[Thief 4.0.0](https://github.com/cteamx/Thief/tree/4.0.0)

如果喜欢这款软件，欢迎前往 GitHub 点一个 **Star** 支持作者。
