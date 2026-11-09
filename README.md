# Loafing Reader

**制作者：扯蛋虾米**

项目仓库：[RichardM95/Thief-Reader](https://github.com/RichardM95/Thief-Reader)。如果喜欢这款软件，欢迎前往 GitHub 点一个 Star，支持作者持续改进。

Windows 10/11 x64 本地 TXT 悬浮阅读器，使用 Rust 编写。阅读窗采用 Win32 分层窗口与 DirectWrite，系统设置采用 Slint 1.18.1 Fluent、winit 和 Skia，跟随系统明暗主题。

## 开始阅读

1. 解压免安装包，运行 `LoafingReader.exe`。更新时，先在托盘菜单中选择“退出”关闭旧版，再替换程序。
2. 在“阅读”页点击“打开 TXT”，或通过“最近阅读”继续已有小说。
3. 使用目录、正文搜索或翻页操作定位内容。关闭设置窗口只会隐藏它，仍可通过托盘“系统设置”重新打开。

阅读窗保留无边框、置顶、独立文字与背景透明度，并根据窗口高度自动显示一行或多行。新配置默认使用 856 × 47 个逻辑像素、14 像素字号。

## 功能与设置

| 页面 | 功能 |
| --- | --- |
| 阅读 | 打开 TXT、最近阅读、章节目录、正文搜索；高级选项提供编码选择和章节正则预览／应用 |
| 外观 | 搜索系统字体，导入 TTF／OTF／TTC，设置字号、字重、斜体、字距、行距、颜色、不透明度和页码显示 |
| 阅读操作 | 鼠标左键下一页／右键上一页、自动翻页间隔、未打开小说时显示的居中伪装文字 |
| 窗口位置 | 显示器示意图、目标屏幕、屏内逻辑坐标、窗口尺寸、居中与仅重置位置 |
| 快捷键 | 直接按键录制，以独立键帽显示；检查重复分配和系统占用 |
| 关于 | 软件名称、制作者、版本号 0.1、GitHub 与 Star 支持入口；第三方组件署名收纳在独立许可弹窗 |

TXT 支持 UTF-8、UTF-16、GBK／GB18030、Big5；自动识别不合适时可手动选择编码。章节按原始段落识别，阅读时保留原文换行，短段落右侧允许留白；自定义规则可以先预览再应用。阅读位置按正文偏移保存，重新分页后仍保留位置。

不再提供“多行阅读”开关，窗口能放几行就显示几行。分页与绘制使用相同的字体度量、行距和可用宽高，页码仅预留最后一行的实际空间。

外观修改先更新设置页预览，点击“应用更改”后才影响阅读窗并保存。各设置分类分别保留草稿：切换分类或隐藏窗口不会丢失草稿，“还原更改”恢复该分类已应用的值，退出程序不保存未应用内容。

托盘菜单为“系统设置”“显示 / 隐藏”“自动翻页”“找回窗口”“退出”。阅读窗在显示、重绘、拖动结束和 DPI 变化后校验置顶顺序；可见时还会定期检查重叠遮挡，恢复层级时不抢输入焦点，隐藏时不会被自动唤出。**找回窗口会立即将阅读窗移到鼠标所在屏幕居中**；窗口位置页的居中、坐标和尺寸则先进入草稿，应用后生效。“重置位置”仅调整位置，保留尺寸。

### 快捷键录制

点击对应操作的键帽区域，直接按下组合键并松开，再点击“应用更改”。也可用 Tab 聚焦后，通过回车或空格开始录制；Esc、失焦、切换页面或最小化会取消录制。

录制期间暂停本软件的全局快捷键，结束后恢复已应用的配置。新组合键在应用前不会正式注册；重复分配会在对应操作旁提示，系统注册失败时恢复原配置。

| 操作 | 默认快捷键 |
| --- | --- |
| 上一页 | `Ctrl+Alt+,` |
| 下一页 | `Ctrl+Alt+.` |
| 老板键 | `Ctrl+Alt+M` |
| 自动翻页 | `Ctrl+Alt+P` |
| 找回窗口 | `Ctrl+Alt+R` |

老板键隐藏阅读窗与设置窗，并暂停自动翻页。

## 配置与兼容性

配置、各书阅读进度和导入字体保存在 `%APPDATA%/Thief-Reader`。可设置 `THIEF_READER_DATA` 环境变量指定独立目录，便于测试时隔离个人数据。

应用已更名为 Loafing Reader，内部 Cargo 包／构建文件名、配置目录和单实例标识继续保留原名，以兼容已有 Rust 版本的设置。新版不提供 Electron 旧版配置导入。

## 构建与发布

需要 Windows x64、Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。构建脚本通过 Windows SDK 的 `rc.exe` 嵌入图标与产品信息，也可用 `RC` 环境变量指定编译器路径。

在项目根目录运行 PowerShell：

```powershell
.\build.ps1 test
.\build.ps1 build
.\build.ps1 release
.\build.ps1 package
```

| 输出 | 路径 |
| --- | --- |
| Cargo Release 程序 | `target/release/thief-reader.exe` |
| 免安装目录 | `dist/Loafing-Reader/`，入口为 `LoafingReader.exe` |
| 免安装 ZIP | `dist/Loafing-Reader-win64.zip` |

`build.ps1 package` 构建 Release、复制说明和许可证，并在检测到 Visual Studio 重分发目录时打包对应的 x64 VC 运行库。依赖版本由 `Cargo.lock` 固定，首次构建需要下载依赖；`.cargo/config.toml` 配置了 Cargo 镜像。开发机的 `.tools/` 本地工具链不会随源码或程序分发。

图标的 SVG 源稿、PNG 和多尺寸 ICO 位于 `assets/`，均需保留在源码中。仅在重新生成图标时需要 Python 与 Pillow：

```powershell
python tools/generate_icon.py
```

正常构建不需要 Python，程序运行也不依赖外部图标文件。

## 验证

在已配置 Rust 工具链的终端运行：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked --example ui_snapshot
```

依赖已缓存时可添加 `--offline`。桌面 UI 测试和快照使用 Slint 无窗口后端，虚拟输入只在测试进程内传递，不创建生产阅读窗、不注册实际全局快捷键、不发送系统鼠标或键盘输入。置顶测试仅创建桌面外的 2×2 非激活窗口，验证遮挡恢复、焦点和隐藏状态。

快照输出到 `dist/screenshots/`，覆盖六个分类、明暗主题、650 × 520／1000 × 720 逻辑尺寸及 100%／150%／200% 缩放，共 72 张；部分快捷键页覆盖录制状态。`cargo run --locked --example reader_snapshot` 另外生成 `dist/reader-previews/` 下的一行、两行、段落及不同间距排版示例。只检查快捷键页时可运行：

```powershell
cargo run --locked --example ui_snapshot -- --hotkeys-only
```

最近一次实现验证为 34 项测试通过。实测记录与未完成的桌面交互验收见 [验证记录](docs/verification.md)，实现说明见 [实现文档](docs/implementation.md)。

## CodeGraph 代码索引

CodeGraph 是可选的开发工具，不是应用运行依赖。已安装 CLI 并将其加入 PATH 后，在项目根目录运行：

```powershell
codegraph init --yes
codegraph status
codegraph explore "Controller begin_recording record_released Recorder hotkeys"
```

索引建立后，代码变更可使用 `codegraph sync` 同步，需要完整重建时使用 `codegraph index`。通过 MCP 调用 `codegraph_explore` 时，将 `projectPath` 设为当前项目根目录的绝对路径。

当前 CodeGraph 索引覆盖 Rust 与 Python。Slint 布局、PowerShell 脚本和 Markdown 文档需直接读取对应文件。

`.codegraph/` 属于本机生成数据，已通过 `.gitignore` 排除；源码、`Cargo.lock`、图标及 `.cargo/config.toml` 继续版本管理。已有索引时，理解或定位代码优先使用 CodeGraph，再按需补充文本搜索。

## 许可与参考

项目许可证见 [LICENSE](LICENSE)。Slint 署名可从“关于”页的“第三方许可”入口打开，相关许可随发布包附带，详见 [Slint 许可说明](docs/Slint-License.md)。界面与数据处理参考 [Thief 4.0.0](https://github.com/cteamx/Thief/tree/4.0.0)，本项目使用 Rust 实现。
