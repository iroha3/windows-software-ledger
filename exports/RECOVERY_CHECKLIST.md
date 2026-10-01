# 电脑重装恢复备忘清单 (Recovery Checklist)

> 生成时间: 2026/10/1 19:25:54  
> 统计概览: 软件总数 **236** | 必须恢复 **7** | 建议恢复 **0** | 用到再装 **0** | 待确认 **229** | 待备份资产 **0**

---

## ⚠️ 重装前必须备份的资产清单 (Pre-install Backup Tasks)

*暂无标记为需要打包目录或导出配置的软件。在软件备份台账中将处置方式标记为「保留/压缩目录」或「导出配置」后将在此列出。*

---

## 一、🔴 必须恢复 (Must Restore)

### 📂 开发工具
- [x] **ripgrep** (cli) [官网/下载](https://github.com/BurntSushi/ripgrep/releases) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)* *(已就绪)*
  - 💡 **备注/配置说明**: ripgrep 为命令行工具，无图形界面配置文件。主要依赖系统环境变量（PATH）及用户级缓存目录（通常在 %APPDATA%\ripgrep 或 %LOCALAPPDATA%\ripgrep）。若需保留搜索历史或自定义配置，建议检查上述 AppData 路径下的 JSON 文件；否则可直接重新下载最新版本。
- [ ] **AutoHotkey** `2.0.19` (desktop) [官网/下载](https://www.autohotkey.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\AutoHotkey，包含脚本文件 (.ahk) 及注册表设置。
- [ ] **AutoHotkey Dash** (desktop) [官网/下载](https://www.autohotkey.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\AutoHotkey\UX 目录下，包含脚本文件 (.ahk) 及用户设置。由于该版本为独立安装包（.exe），建议重新下载以获取最新组件，但可手动迁移 AppData 中的脚本与配置。
- [ ] **Zed** `1.14.2` (desktop) [官网/下载](https://zed.dev/downloads) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: Zed 的配置文件默认存储在 C:\Users\iroha3\.config\zed 目录下，包含用户偏好设置。由于 Zed 是跨平台 Rust 编写的编辑器，其安装程序会将配置迁移到 Windows 的 AppData/Local/Zed 目录中。建议重新下载时选择'Copy Configurations'选项以保留设置，或手动备份上述配置文件路径。

### 📂 系统工具
- [ ] **7-Zip** `24.07` (desktop) [官网/下载](https://www.7-zip.org/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
- [ ] **x86** (desktop) [官网/下载](https://www.quickhash.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\QuickHash 或安装目录下的 config 文件夹中，建议备份后重新下载以确保版本兼容性。

### 📂 其他
- [ ] **15minutes** (desktop) [官网/下载](https://www.15minutes.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 该应用为桌面端软件，配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\15minutes。由于该软件主要作为计时器工具运行且无显著的云端同步机制，建议直接重新下载安装以获取最新功能，原路径下的配置可手动迁移至新安装目录的 AppData 中。


## 二、🟡 建议恢复 (Should Restore)

*暂无建议恢复的软件*

## 三、🔵 用到再装 (On Demand)

*暂无*

## 四、⚫ 淘汰弃用 (Drop / Deprecate)

*暂无*