# ParallelsClipboardFix

English | [中文](#中文)

`ParallelsClipboardFix` is a small Windows utility that repairs image clipboard data for Parallels Desktop shared clipboard sync.

It was built for a specific issue: images copied from Windows WeChat inside Parallels Desktop can sometimes be read by Windows applications, but cannot be pasted on macOS through the shared clipboard. `ParallelsClipboardFix` listens for clipboard changes, reads a valid image from the Windows clipboard, rewrites it as standard clipboard image data, and lets Parallels synchronize the repaired clipboard contents.

## Platform

`ParallelsClipboardFix` is Windows-only.

Recommended:

- Windows 10 or Windows 11
- Parallels Desktop shared clipboard enabled
- Windows WeChat running inside the virtual machine

## Build

Install Rust from [rustup.rs](https://rustup.rs), then build from the repository root:

```powershell
cargo build --release
```

The executable will be created at:

```powershell
target\release\ParallelsClipboardFix.exe
```

## Run

```powershell
target\release\ParallelsClipboardFix.exe
```

The app runs in the system tray. 

## Startup

Startup is implemented with a shortcut in the current user's Startup folder:

```powershell
%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\ParallelsClipboardFix.lnk
```

## Configuration

On first run, the app creates a configuration file under the user config directory.

Default options:

```toml
enabled = true
start_with_windows = false
retry_count = 5
retry_delay_ms = 80
write_png = true
write_dib = true
log_level = "info"
```

## License

- MIT License

---

# 中文

[English](#parallelsclipboardfix) | 中文

`ParallelsClipboardFix` 是一个 Windows 小工具，用来修复 Parallels Desktop 共享剪贴板中的图片同步问题。

它解决的是一个具体场景：在 Parallels Desktop 的 Windows 虚拟机里，从 Windows 微信复制图片后，Windows 程序可以读取剪贴板图片，但 macOS 通过共享剪贴板经常无法粘贴。`ParallelsClipboardFix` 会监听 Windows 剪贴板变化，读取可用图片，然后把它重新写成标准的剪贴板图片格式，让 Parallels 能正确同步到 macOS。

## 平台

 Windows。

推荐环境：

- Windows 10 或 Windows 11
- 已启用 Parallels Desktop 共享剪贴板
- Windows 微信运行在虚拟机中

## 编译

先从 [rustup.rs](https://rustup.rs) 安装 Rust，然后在项目根目录执行：

```powershell
cargo build --release
```

生成的可执行文件位置：

```powershell
target\release\ParallelsClipboardFix.exe
```

## 运行

```powershell
target\release\ParallelsClipboardFix.exe
```

程序运行后会出现在系统托盘。

## 开机自启动

开机自启动通过当前用户的 Startup 文件夹快捷方式实现：

```powershell
%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\ParallelsClipboardFix.lnk
```

## 配置

首次运行时，程序会在用户配置目录下创建配置文件。

默认配置：

```toml
enabled = true
start_with_windows = false
retry_count = 5
retry_delay_ms = 80
write_png = true
write_dib = true
log_level = "info"
```

## 许可证

- MIT License
