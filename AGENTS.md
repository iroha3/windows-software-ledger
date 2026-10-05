# AGENTS.md

给 AI 编码助手（以及新加入的人）的项目速览。**先读这里，再动手。**

## 这是什么

**软件备份台账** — Windows 原生的「软件家底清点 / 重装迁移」工具。本地优先、单 exe 便携运行、数据可携带。

技术栈：**Tauri v2（Rust 后端）+ 无框架静态前端（原生 HTML/CSS/JS）**。没有 `package.json`，没有 npm 依赖，没有前端构建步骤——`client/` 就是最终产物，release 时由 Tauri 内嵌进 exe。

## 目录结构

```
client/                 前端（直接就是产物，无打包）
  index.html  app.js        主页台账：表格 + 侧边抽屉 + 批量条 + 设置弹窗
  card_review.html  card_review.js   卡片速审页：逐条翻卡标注
  browsers.html  browsers.js         浏览器扩展页（复刻主页外壳，扫描字段只读）
  dev_env.html  dev_env.js           开发环境只读证据页
  vault.js                  配置归档「保管箱」组件（主页/卡片/浏览器共用）
  confirm_delete.js         删除确认弹窗
  tauri-shim.js             ★把 fetch('/api/...') 透明转发到 Rust command
  style.css  browser-icons/
src-tauri/src/
  lib.rs                    Tauri Builder + command 注册表（新增命令要在这里登记）
  commands.rs               所有 #[tauri::command]：CRUD / 扫描 / LLM / 归档 / 导出
  store.rs                  数据落盘：data/ 路径、读写 JSON（纯读，不做兼容）
  ingest.rs                 扫描候选合并进 software.json（去重 / 墓碑 / 便携判定）
  exporter.rs               生成《重装恢复清单》《精选资产库》Markdown
  main.rs
scripts/                  独立辅助脚本，不参与应用运行时（见下）
src-tauri/Cargo.toml      ★版本号唯一来源
src-tauri/tauri.conf.json 不写 version（回退用 Cargo 版本）；frontendDist=../client
BUILD.md                  构建 / 数据目录 / 采集脚本的详细说明（本文件不重复）
SPEC.md                   产品范围与数据模型
```

## 命令

> ⚠️ **在 Git Bash / MSYS 下不要直接 `cargo`**：`/usr/bin/link` 会抢占 MSVC 的 `link.exe`，报 `link: extra operand`。一律用包装脚本。

```bash
./scripts/cargo-msvc.bat check            # 类型检查（最快的验证手段）
./scripts/cargo-msvc.bat test             # 单元测试
./scripts/cargo-msvc.bat build --release  # 产物：src-tauri/target/release/windows-software-ledger.exe
./scripts/dev.bat                         # 开发：MSVC 环境 + tauri dev（改 client/ 热刷新，不重编 Rust）
```

- `scripts/cargo-msvc.bat` 会自动定位 VS 2022/2019 的 `vcvars64.bat` 再调 `cargo %*`。
- 纯 JS 语法检查：`node --check client/app.js`（无需依赖）。
- LLM 探测脚本：`bun scripts/test_llm.js "软件名" "路径"`（默认走 DeepSeek 非思考模式）。
- 图标重建：`python scripts/generate_icon.py`（仅改图标时需要，改后记得 `cargo-msvc.bat clean -p windows-software-ledger` 再构建）。

## 架构关键点

**前端 → 后端**：前端统一写 `fetch('/api/xxx', {method, body})`。`client/tauri-shim.js` 用一张 `switch` 路由表把它们映射到 `invoke('<command>', {...})` 并伪造一个 `Response`。因此：

- **新增一个后端命令时**，必须同时在 `lib.rs` 的 `generate_handler![...]` 和 `tauri-shim.js` 的 `route()` 里登记，否则前端调不到。
- shim 只在 Tauri 环境生效；纯浏览器/Bun 下 `window.__TAURI__` 不存在，它自动让路给原生 `fetch`。

**数据根目录**：永远是 **exe 所在目录** 下的 `data/`（`store.rs::app_root`）。没有环境变量覆盖、不向上查找、不依赖标记文件。`cargo run` 调试时落在 `src-tauri/target/debug/data/`。`data/` 已 gitignore。

**持久化**：全部是 `data/` 下的美化 JSON，直接读写，无数据库。主要文件：`software.json`、`config.json`、`ignored.json`（删除墓碑）、`extensions.json`（扩展标注）、`icons/`、`vault/`、`evidence/`。

**版本号**：只在 `src-tauri/Cargo.toml` 的 `[package] version`。`.github/workflows/release.yml` 在推送到 `master` 时读它——若对应 tag 不存在就自动构建并发 Release，已存在则整条跳过。**要发版必须升版本号**，否则 CI 不会动。

**扫描脚本**：`scripts/collect.ps1` 通过 `include_str!` 编译进 exe，运行时释放到临时目录执行；release 不依赖仓库里的 `scripts/`。改采集逻辑改这个 `.ps1`。

## 数据模型要点（详见 SPEC.md）

- `restore_intent`：`must` / `on_demand` / `drop` / `unreviewed`
- `type`：`desktop` / `portable` / `cli` / `runtime`
- `backup_strategy`：`copy_dir` / `redownload` / `sync_account` / `none`
- **处置方式是推导出来的，不是让 LLM 猜的**：默认 `none`；评 `must` 时按形态给默认值——绿色/便携 → `copy_dir`，否则 → `redownload`。规则实现在 `commands.rs::derive_strategy` + 前端 `deriveStrategy()`，入口有单条（表格/抽屉/卡片）与批量。LLM 只负责 `category` / `type` / `restore_intent` / `download_url` / `config_notes`，**不输出 `backup_strategy`**。
- 便携判定：`type == "portable"` 或任一机器分布 `form == "portable"`。
- **扫描认回匹配键 = 机器 + 安装路径（bin path）**（`ingest.rs::find_known`）：**同名不算同一实体**，路径不变就不换 uuid；仅当候选本身没任何路径时才退回「同机器 + 同名」。删除墓碑同理按路径精确命中（`tombstone_match`）。
- **不做旧版本兼容**：这一系列都是破坏性更新。`store::read_software` 是**纯读**——不补 uuid、不归并字段、不读 evidence。**uuid 是所有内部绑定的硬前提**（匹配 / 删除 / 合并一律只认 uuid，不再回退 SW-ID）。

## 约定与地雷

- **UI / 注释一律中文**，与现有风格保持一致；代码简洁，不引入依赖。
- **前端改动 release 不会自动生效**：要么 `scripts/dev.bat`，要么重新 `build`。改完 JS 至少 `node --check` 一下。
- **不要往主 `tauri.conf.json` 写 `devUrl`**。dev 专属配置在 `src-tauri/tauri.dev.conf.json`，由 `tauri dev --config` 合并。原因见 BUILD.md。
- **LLM 端点**：任意 OpenAI 兼容的 `/chat/completions`。DeepSeek 默认开思考模式，代码检测到 `api.deepseek.com` 时显式下发 `thinking:{type:"disabled"}`；这个参数**只对 DeepSeek 发**，否则 OpenAI/本地端点会因未知字段报 400。LLM 配置可全部留空（表示不启用 AI），不应阻塞保存。
- **安全红线**：采集与扩展读取**只读元数据**，绝不碰 `~/.ssh`、凭据、`.npmrc` token、浏览器 Cookie/密码、扩展 `storage.local`。保管箱（`data/vault/`）只能用户手动拖入，**绝不自动采集**。
- 改动数据结构（`software.json` 字段）时**直接改，不写兼容层**；旧 `data/` 不保证可用。
- 提交前跑 `./scripts/cargo-msvc.bat check`（本机缺 MSVC 时此项会失败，属环境问题，非代码问题）。

## 快速定位

| 想改什么 | 去哪 |
|---|---|
| 主页表格 / 抽屉 / 批量操作 | `client/app.js` + `client/index.html` |
| 卡片速审交互 | `client/card_review.js` + `card_review.html` |
| 浏览器扩展页 | `client/browsers.js` + `browsers.html` |
| 新增/修改后端接口 | `src-tauri/src/commands.rs` + `lib.rs` + `client/tauri-shim.js` |
| 扫描/去重/导入规则 | `src-tauri/src/ingest.rs`、`scripts/collect.ps1` |
| 导出文档格式 | `src-tauri/src/exporter.rs` |
| 数据路径 / 数据落盘 | `src-tauri/src/store.rs` |
| LLM 提示词 | `src-tauri/src/commands.rs`（`llm_analyze`）、`scripts/test_llm.js` |
