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

```bat
scripts\cargo-msvc.bat run
```

### 测试

```bat
scripts\cargo-msvc.bat test
```

## 数据目录约定

数据根目录规则只有一条：**永远是可执行文件所在目录**。

- 即 `<exe目录>\data\`，没有环境变量覆盖、不向上查找、不依赖任何标记文件。
- 便携发布 = 一个 exe + 旁边的 `data\` 文件夹，`data\` 在首次需要写入时自动创建。
- `cargo run` 调试时 exe 位于 `src-tauri\target\debug\`，数据即落在 `src-tauri\target\debug\data\`。

## 采集脚本

`scripts\collect.ps1` 通过 Rust 的 `include_str!` **编译进 exe**：扫描时释放到系统临时目录执行，采集结果也在同一临时目录内，`ingest` 进 `data\software.json` 后立即删除。

因此：

- 运行时不依赖外部 `scripts\` 目录；
- 不会在数据目录留下 `evidence\`；
- 临时文件带 UTF-8 BOM 写出，兼容 PowerShell 5.1（存在 `pwsh` 时优先使用）。

## 图标

图标源文件为 `src-tauri\icons\icon.svg`，构建所用的多分辨率 `icon.ico`（16/32/48/64/128/256，PNG 压缩）已生成并提交。

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
