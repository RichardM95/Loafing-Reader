# Thief Reader

Windows 10/11 x64 本地 TXT 悬浮阅读器，Rust 编写。管理窗口使用 Slint Fluent，阅读窗口使用 DirectWrite 和 Win32 逐像素 Alpha 合成。

## 使用

托盘菜单“系统设置”打开控制窗口。阅读页选择 TXT、搜索正文、预览和应用章节正则。外观页调整字体后点击“应用并预览”。窗口页可选择显示器和屏内坐标，并配置快捷键。老板键隐藏窗口并暂停自动翻页，托盘“窗口找回”将窗口移至鼠标所在屏幕。

托盘菜单提供“系统设置”“显示 / 隐藏”“自动翻页”“窗口找回”和“退出”。

默认快捷键为 Ctrl+Alt+, 上一页、Ctrl+Alt+. 下一页、Ctrl+Alt+M 老板键、Ctrl+Alt+P 自动翻页、Ctrl+Alt+R 窗口找回。

数据保存在 %APPDATA%/Thief-Reader。可使用 THIEF_READER_DATA 环境变量指定独立测试目录。

## 构建

安装 Rust MSVC 工具链和 Visual Studio C++ Build Tools。运行 ./build.ps1 test、./build.ps1 release 或 ./build.ps1 package。依赖版本由 Cargo.lock 固定，项目中的 Cargo 镜像配置可按网络环境调整。

开发机本地工具链位于 .tools/native-toolchain，不随源码或发布包分发。

## 验证记录

文档、配置、像素透明度和 Unicode 连续分页由自动测试验证。
完整 GUI、三屏交互、原版截图对照和整机性能仍以 docs/implementation.md 中的最新实测记录为准。

系统设置的“关于”页提供 Slint 署名，相关许可随发布包附带。

界面与数据格式参考 https://github.com/cteamx/Thief 4.0.0，程序使用独立 Rust 实现。
