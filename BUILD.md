# 构建指南

本项目为 Windows 原生的 Tauri v2 应用（Rust 后端 + 静态前端）。

## 环境要求

| 依赖 | 说明 |
|---|---|
| Windows 10 / 11 | 目标平台 |
| [Rust](https://rustup.rs/) | stable 工具链，MSVC ABI（`x86_64-pc-windows-msvc`） |
| Visual Studio 2022 生成工具 | 安装时勾选「使用 C++ 的桌面开发」，提供 MSVC 链接器 |
| WebView2 Runtime | Windows 11 自带；Windows 10 若缺失需手动安装 |
| Node.js / Bun（可选） | 仅用于辅助脚本，应用本身不依赖 |

## 编译

在项目根目录执行：

```bat
scripts\cargo-msvc.bat build --release
```

产物：

```
src-tauri\target\release\windows-software-ledger.exe
```

`scripts\cargo-msvc.bat` 是一个包装脚本，用于在 Git Bash / MSYS 等环境下正确加载 MSVC 环境（否则 `/usr/bin/link` 会抢占 MSVC 的 `link.exe`，报 `link: extra operand`）。

若在「x64 Native Tools Command Prompt for VS」中构建，可直接调用 cargo：

```bat
cargo build --release --manifest-path src-tauri\Cargo.toml
```

### 开发运行

**方式一：热重载（推荐，改前端不用重编）**

```bat
scripts\dev.bat
```

`dev.bat` 先加载 MSVC 环境，再跑 `tauri dev`：

- `scripts/dev-server.mjs`（Bun）把 `client/` 跑在 `http://127.0.0.1:1420`；dev 专属配置在 `src-tauri/tauri.dev.conf.json`（用 `tauri dev --config` 合并），主配置 `tauri.conf.json` 保持干净；
- 改动 `client/` 下任何文件 → 窗口**自动刷新**，**不触发 Rust 重编**；
- 只有改 Rust 代码时才增量重编并重启。

首次会通过 `bunx @tauri-apps/cli` 拉取 Tauri CLI（无需 `cargo install`）。

**方式二：自包含构建（前端改动需重新编译）**

```bat
scripts\cargo-msvc.bat build --release
```

产物在 `src-tauri\target\release\windows-software-ledger.exe`，`client/` 内嵌其中。

> 注意：不要把 `devUrl` 写进主 `tauri.conf.json`。本地是用 `cargo` 直接构建、没有启用 `custom-protocol` feature，此时 `dev == true`，主配置里的 `devUrl` 会让 release exe 也去加载 `localhost:1420`。所以 dev 用 `--config` 合并到临时配置里。

### 测试

```bat
scripts\cargo-msvc.bat test
```

## 数据目录约定

数据根目录规则只有一条：**永远是可执行文件所在目录**。

- 即 `<exe目录>\data\`，没有环境变量覆盖、不向上查找、不依赖任何标记文件。
- 便携发布 = 一个 exe + 旁边的 `data\` 文件夹，`data\` 在首次需要写入时自动创建。
- `cargo run` 调试时 exe 位于 `src-tauri\target\debug\`，数据即落在 `src-tauri\target\debug\data\`。

> **旧数据迁移**：v1.x 的 `data\`（无 uuid）不与当前版本兼容。用 `python scripts\migrate_to_v2.py <旧 data 目录>` 一次性转换（原地迁移会先备份为 `<目录>.v1bak`，也可 `--out` 输出到新目录）：补 uuid、`should`→`on_demand`、`copy_config`→`copy_dir`、`vault\<主机名>\<kind>\<id>\`→`vault\<kind>\<uuid>\`、扩展标注重挂到 `(browser_id, profile, ext_id)`。

## 采集脚本

`scripts\collect.ps1` 通过 Rust 的 `include_str!` **编译进 exe**：扫描时脚本释放到系统临时目录执行，采集结果写入 `data\evidence\<主机名>\` 并长期保留（`screenshots\` 供手动存放参考截图，重扫不会清除）。扫描分两步：`scan_preview` 解析候选，`scan_commit` 只把勾选项写入 `data\software.json`。

因此：

- 运行时不依赖外部 `scripts\` 目录；
- 证据保留在 `data\evidence\`，可随便携目录一起拷走；
- 临时脚本文件带 UTF-8 BOM 写出，兼容 PowerShell 5.1（存在 `pwsh` 时优先使用）。

### 软件图标（`data\icons\` + 证据里的 `app-icons\`）

扫描第 6 步用 PowerShell 的 `System.Drawing.Icon.ExtractAssociatedIcon` 从可执行文件抽取 32×32 图标（只读 exe 资源，不碰任何敏感数据）：

- **来源**：注册表项的 `DisplayIcon`（会剥掉 `,0` 索引）、快捷方式的 `target_path`、绿色软件的 `main_exe`；按 exe 路径去重，产出 `data\evidence\<主机名>\app-icons\<hash>.png`，并把文件名写回 `registry-apps.json` / `shortcuts.json` / `portable-apps.json` 的 `icon_file` 字段。
- **导入**：`scan_commit` 把图标复制到 `data\icons\<uuid>.png`，并在条目的 `icon_file` 字段记下文件名；重扫时「已知」的已有条目走 `known_icon_refreshes` 补齐（缺则补、不覆盖）。
- **展示**：`get_software` 读条目的 `icon_file` 字段指向的文件，编码成 data URI 注入返回值的 `icon` 字段（**不写回 `software.json`**），主表格 / 抽屉 / 卡片速审在名称前显示 20px 缩略图，取不到则回退通用方盒图标。删除软件与合并条目时按 `icon_file` 级联删图标，不留孤儿。
- **命名**：`icon_file` = `<uuid>.png`（见 `store::icon_file_name`），绑定内部 uuid，与软件名 / 扫描序号（`SW-ID`）无关；一条记录一个图标，删除 / 合并跟着记录走。
- **局限**：`ExtractAssociatedIcon` 固定 32×32；UWP/Store 应用与部分注册表项没有可用 exe，只能回退占位。

### 开发环境清单（`dev-env.json` + `dev-env/`）

扫描第 8 步额外产出两样东西（都在 `data\evidence\<主机名>\` 下）：

- `dev-env/` 目录：包列表 / 配置原文，UTF-8 **无 BOM**，供 pip / npm / cargo 直接读取。
- `dev-env.json`：7 个 provider 的摘要（Python / Rust / VS Code 扩展 / Git / Node / Go / .NET）。

包多时不再逐行罗列命令，而是写文件 + 短命令。命令里的 `{{EVIDENCE}}` 占位符由前端替换为 `data/evidence/<设备目录>`，例如：

```json
{
  "schema_version": 2,
  "machine_id": "...",
  "files_dir": "dev-env",
  "providers": [
    {
      "id": "python",
      "label": "Python",
      "available": true,
      "summary": "204 个 pip 包",
      "items": [ { "name": "numpy", "version": "2.1.0" } ],
      "files": [ { "name": "python-requirements.txt", "count": 204 } ],
      "restore_commands": [ "pip install -r \"{{EVIDENCE}}/dev-env/python-requirements.txt\"" ]
    }
  ]
}
```

采集的清单文件：`python-requirements.txt` / `python-pipx.txt` / `rust-toolchains.txt` / `rust-components.txt` / `rust-crates.txt` / `vscode-extensions.txt` / `node-globals.txt` / `dotnet-tools.txt`。

**源 / 镜像配置**也会另存一份（同样脱敏）：`pip-config.txt`（`pip config list`）、`cargo-config.toml`（`~/.cargo/config.toml`）、`git-config.txt`（`~/.gitconfig`），npm registry 与 Go `GOPROXY` 作为 item 展示。

前端「开发环境」页（`client/dev_env.html`）通过 `get_dev_env` 命令聚合所有 `dev-env.json`（并带上证据目录名 `dir`），按设备折叠展示文件清单与命令块，命令可一键复制。它只是只读证据，**不进入 `software.json`，也不参与导入流程**。

**安全红线**：provider 只调白名单只读命令，**绝不读取环境变量块、`~/.ssh`、`.git-credentials`、`.npmrc` token、`.aws/`、`.env` 或任何凭据**；Git 只取白名单键（用户名/邮箱/编辑器/别名等），仓库 remote 不在采集范围。另存的配置文件会先经 `Protect-Secret` 脱敏：`scheme://user:pass@` 与含 `token/password/secret/_auth/api[-_]key` 的取值一律掩码为 `***`。

**扩展（欢迎 PR）**：新增一个工具链只需在 `collect.ps1` 第 8 步加一个 provider 对象（`id` / `label` / `items` / `files` / `restore_commands`），前端无需改动。欢迎补充 PowerShell 模块、WSL、JetBrains 插件等。

### 浏览器扩展（`browser-extensions.json`）

扫描第 9 步采集**已安装浏览器**的扩展只读元数据，产出 `data\evidence\<主机名>\browser-extensions.json`：

- Chromium 系（Edge / Chrome / Brave / Vivaldi / Chromium / Helium / Opera / Opera GX）用**数据驱动的候选根目录**，存在才扫，不在就跳过；逐个 profile 读 `Extensions\<扩展ID>\<版本>\manifest.json`，`__MSG_xxx__` 名称会从 `_locales` 解析。
- Firefox 读 `%APPDATA%\Mozilla\Firefox\Profiles\<profile>\extensions.json`，只取 `location == "app-profile"` 且 `type == "extension"` 的项，名称优先 `defaultLocale.name`，并带上 `active` 与 AMO `sourceURI`。
- 结构：`{ schema_version, machine_id, collected_at, browsers: [ { id, label, profiles: [ { profile, extensions: [ { id, name, version, enabled, type, store_url } ] } ] } ] }`。

Rust 侧 `get_browser_extensions` 聚合各机器的该文件（仿 `get_dev_env`），并合并用户层标注，前端 `client\browsers.html` **复刻主页台账外壳**（统计横条 / 设备选项卡 / 筛选 / 批量条 / 侧边抽屉 / 设置弹窗），可按设备 / 浏览器 / 意愿 / 进度过滤与搜索。表格列为：勾选 / 精选 / 扩展名称（下方标签显示浏览器与配置）/ 版本 / 状态 / 保留意愿 / 准备进度 / **商店链接** / 设备 / 备注 / 附件。

**商店链接**：扫描时自动录入（Firefox 取 AMO `sourceURI`，Chromium 有可靠商店归属才填），列为可编辑文本框，值按内部 uuid 存 `data\extensions.json`，右边按钮一键在浏览器打开。

**Firefox 配置名**：证据里的 profile 形如 `xc8zepzv.default-release`（前缀是随机串），展示时由 `cleanProfile()` 剥掉前缀，只显示 `default-release`。

**浏览器品牌图标**：`client/browser-icons/` 内置 Chrome / Chromium / Edge / Firefox / Brave / Vivaldi / Opera / Opera GX 的 SVG（取自 `alrra/browser-logos`，纯静态资源），表格“扩展名称”下方的浏览器标签里按 `browserId` 显示，未收录的（如 Helium）回退通用图标。“所在设备”列与主页一致，用 `badge-machine` 胶囊。

**用户层（扫描字段只读，用户层可增）**：把主页那套交互原样搬来，扫描字段（名称 / 版本 / 扩展 ID / 状态 / 来源 / 设备 / 配置）一律**只读**，用户可增：

- **保留意愿**（必须 / 按需 / 淘汰 / 待确认，快捷键 1~4）、**准备进度**（待办 / 就绪）、**精选星标**：与软件台账同一套值域与 UI；
- **备注**：内联可编辑，按内部 uuid 存 `data\extensions.json`（重扫不丢）；
- **附件归档**：每条扩展可手动放入文件（拖入或选择），存 `data\vault\ext\<扩展uuid>\`，行尾回形针按钮带数量角标；
- 用户层字段按内部 uuid 存入 `data\extensions.json`，并内嵌匹配键 `(browser_id, profile, ext_id)`；重扫时按该组合认回 uuid（同一扩展在不同浏览器 / 配置各自独立标注）；
- 浏览器页另有**整份浏览器配置归档**（`data\vault\browser\<浏览器uuid>\`）。

**安全红线**：只读 `manifest.json` / `extensions.json` 元数据，**绝不读取扩展的 `storage.local`（LevelDB）、`Preferences` 敏感键、Cookie 或密码**。本页同样**不进入 `software.json`，不参与导入**。

### 耗时统计（`timings.json`）

`collect.ps1 -Timing` 会在同一证据目录额外写 `timings.json`（`schema_version` / `machine_id` / `collected_at` / `total_seconds` / `laps`），记录各步骤与第 8 步各 provider 的耗时，方便定位扫描慢在哪。平时扫描不加该开关，不产生额外文件。

实测（264 包 / 63 扩展的机器）总耗时约 **7s**，大头是注册表枚举与开发环境采集。为了让开发环境这一段不拖后腿，已用几个快速路径替代重命令（均有回退）：

- Python 包列表：`importlib.metadata`（~0.6s）代替 `pip list`（~2.2s）；`pip config list`（~1.1s）只在确有 pip 配置文件 / `PIP_*` 环境变量时才调。
- Node 全局包：读 `npm root -g` 下的 `package.json`（~0.8s）代替 `npm ls -g`（~1.7s）；npm registry 直接读 `.npmrc` / 环境变量，不再起 npm 进程。
- Go：一次 `go env GOPATH GOPROXY` 代替两次调用。

### 配置归档（`data\vault\<kind>\<uuid>\`）

与扫描解耦的**手动保管箱**，用来把个性化配置文件随台账一起带走：

- 路径按**内部 uuid** 分层：`data\vault\soft\<软件uuid>\`（软件卡片）、`data\vault\browser\<浏览器uuid>\`（浏览器整份配置）与 `data\vault\ext\<扩展uuid>\`（扩展附件），不再分主机名。
- 只能由用户**手动拖入或点「选择文件」**添加（`vault_add`），保存用 `vault_export`（另存为），删除用 `vault_delete`。**绝不自动采集**。
- 点「无配置」只是收起面板，**归档文件一律不动**；删除软件时把 `soft\<uuid>\` 整体**软删除**到 `data\trash\<时间戳>-<随机>\`（见 `store::trash_dir`），不物理销毁。
- Windows 路径不能含 `:`，所以 kind 落成目录名（`soft` / `browser` / `ext`），而不是 `soft:SW-001`。

## 图标

图标源文件为 `src-tauri\icons\icon.svg`，构建所用的多分辨率 `icon.ico`（16/24/32/48/64/128/256，PNG 压缩）已生成并提交。

如需重新生成（仅在修改图标时需要，依赖 `pip install resvg-py pillow`）：

```bat
python scripts\generate_icon.py
```

脚本会自检每一帧非空且居中，避免再次出现 128 帧透明、256 帧内容偏移导致的 exe 桌面图标错位。

> 修改图标后需先清理再构建，否则增量编译会沿用过期的图标资源：
>
> ```bat
> scripts\cargo-msvc.bat clean -p windows-software-ledger
> scripts\cargo-msvc.bat build
> ```

## 版本号与自动发布

**版本号唯一来源：`src-tauri\Cargo.toml` 中 `[package]` 的 `version`。**

`src-tauri\tauri.conf.json` 刻意不写 `version`，Tauri 会自动回退使用 Cargo 的版本号（旧版 Bun 中台所需的 `package.json` 只存在于 `bun` 分支）。

改好版本号并推送到 `master` 后，GitHub Actions（`.github\workflows\release.yml`）会自动：

1. 读取该版本号；
2. 若 `v<version>` tag 尚不存在，在 `windows-latest` 上构建 `windows-software-ledger.exe`；
3. 打包为 `windows-software-ledger-v<version>-windows-x64.zip`（内含 exe 与 LICENSE）；
4. 创建同名 tag 与 Release 并上传该 zip。

若 tag 已存在则整条流程跳过，不会重复发版。

## 安装包（可选）

仓库未内置 Tauri CLI。如需生成 NSIS 安装包：

```bash
bun add -D @tauri-apps/cli
bunx tauri build
```

## 旧版 Bun 中台

历史版本是一个 Bun HTTP 服务 + 浏览器前端，完整保留在 `bun` 分支。
