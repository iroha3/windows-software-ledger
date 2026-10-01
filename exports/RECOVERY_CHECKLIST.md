# 电脑重装恢复备忘清单 (Recovery Checklist)

> 生成时间: 2026/10/1 17:19:12  
> 统计概览: 软件总数 **236** | 必须恢复 **6** | 建议恢复 **1** | 用到再装 **0** | 待确认 **229** | 待备份资产 **0**

---

## ⚠️ 重装前必须备份的资产清单 (Pre-install Backup Tasks)

*暂无标记为需要打包目录或导出配置的软件。在决策中台中将处置方式标记为「保留/压缩目录」或「导出配置」后将在此列出。*

---

## 一、🔴 必须恢复 (Must Restore)

### 📂 系统工具
- [ ] **ripgrep** (desktop) — 机器: `DESKTOP-HEGVCTR`
- [ ] **x86** (desktop) [官网/下载](https://www.quickhash.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\QuickHash 或安装目录下的 config 文件夹中，建议备份后重新下载以确保版本兼容性。

### 📂 办公与笔记
- [ ] **发送至 OneNote** (desktop) [官网/下载](https://www.microsoft.com/zh-cn/microsoft-365/onenote) — 机器: `DESKTOP-HEGVCTR` *(处置: ☁️ 账号登录同步)*
  - 💡 **备注/配置说明**: OneNote 主要依赖 OneDrive 或 Microsoft 账户进行云同步，本地配置通常存储在 AppData\Roaming\Microsoft\OneNote 目录下。迁移时建议优先恢复云端笔记数据，而非直接复制安装文件。

### 📂 开发工具
- [ ] **AutoHotkey** `2.0.19` (desktop) [官网/下载](https://www.autohotkey.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\AutoHotkey，包含脚本文件 (.ahk) 及注册表设置。
- [ ] **AutoHotkey Dash** (desktop) [官网/下载](https://www.autohotkey.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 配置文件通常位于 C:\Users\[用户名]\AppData\Roaming\AutoHotkey\UX 目录下，包含脚本文件 (.ahk) 及用户设置。由于该版本为独立安装包（.exe），建议重新下载以获取最新组件，但可手动迁移 AppData 中的脚本与配置。
- [ ] **Zed** `1.14.2` (desktop) [官网/下载](https://zed.dev/downloads) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: Zed 的配置文件默认存储在 C:\Users\iroha3\.config\zed 目录下，包含用户偏好设置。由于 Zed 是跨平台 Rust 编写的编辑器，其安装程序会将配置迁移到 Windows 的 AppData/Local/Zed 目录中。建议重新下载时选择'Copy Configurations'选项以保留设置，或手动备份上述配置文件路径。


## 二、🟡 建议恢复 (Should Restore)

### 📂 其他
- [ ] **15minutes** (desktop) [官网/下载](https://www.15minutes.com/) — 机器: `DESKTOP-HEGVCTR` *(处置: 🌐 重新下载安装)*
  - 💡 **备注/配置说明**: 该应用为独立桌面程序，无显著配置文件迁移需求。若需保留设置，建议检查安装目录下的默认文件夹或用户文档中的配置数据。


## 三、🔵 用到再装 (On Demand)

*暂无*

## 四、⚫ 淘汰弃用 (Drop / Deprecate)

*暂无*