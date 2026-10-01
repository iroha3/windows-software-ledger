# SPEC: windows-software-ledger

## 1. 背景与核心目标

### 1.1 背景
用户面临 3 台 Windows 电脑（均为配置完整的开发机）的重装、备份与软件迁移需求。
- 软件形态杂乱：安装版（Registry / Winget / Scoop）、便携版（绿色解压包）、独立命令行工具（CLI / 单可执行文件）。
- 关键资产关联：各种软件的配置位置（AppData、本地目录）、扩展（VS Code / 浏览器等）需要人工关注。
- 核心诉求：提供一个**轻量、高效的表格化决策与状态管理中台**，协助完成清点、去重、决策打标，并最终输出一份清晰的重装恢复清单与精选软件资产库。

### 1.2 明确“做”与“不做”
- **不做**：
  - 自动备份执行脚本（不搞复杂的 `robocopy` 自动备份，便携软件手动拷/压 zip，个性化设置手动导出更为可靠）。
  - 内置复杂视觉模型（VLM/OCR 截图识别不需要集成进软件；需要看图的零星场景直接手动贴给大模型即可）。
  - 过度复杂的快捷键体系（重点是直观、低负担的表格数据操作与状态打标）。
  - 任何商业 SaaS、多租户、外置数据库服务。
- **做**：
  - **轻量采集**：通过 PowerShell 脚本快速抓取各机器的安装列表、便携目录第一层、快捷方式与环境 PATH。
  - **表格化决策中台**：提供一个可视化的本地 Web 表格界面（支持 3 台机器筛选、搜索、分类过滤）。
  - **人机协同整理**：支持直观地删除垃圾条目、合并重复项、补充官网/下载网址与配置备注。
  - **清晰的双列决策**：
    - `恢复意愿`（必须 / 建议 / 用到再装 / 淘汰）
    - `处置方式`（保留目录 / 手动导配置 / 重新下载 / 账号同步 / 无需处理）
  - **高价值文档导出**：
    1. `RECOVERY_CHECKLIST.md`：按优先级分组的重装恢复检查清单（含下载链接、配置导出位置与验证备忘）。
    2. `AWESOME_LIST.md`：个人精选工作流软件清单。

---

## 2. 核心工作流 (Workflow)

```
[阶段 1: 证据收集 (Collect)]
  ├─ 在每台机器上运行采集（脚本已内嵌进 exe，释放到临时目录执行）
  │    ├─ 已安装应用 (Registry / Winget / Scoop)
  │    ├─ 扫描便携/绿色软件目录
  │    └─ 快捷方式与环境 PATH
  └─ 采集结果写入系统临时目录，入库后立即删除，不保留 evidence/
        │
        ▼
[阶段 2: 数据入库 (Ingest)]
  ├─ 解析临时目录中的结构化数据
  └─ 归并去重后写入主数据 data/software.json
        │
        ▼
[阶段 3: 表格化决策与整理 (Table Hub)]
  ├─ 多机视图切换 (PC-A / PC-B / PC-C / 全局)
  ├─ 人工去重与清理 (合并同名项、删除卸载残留)
  ├─ 完善事实信息 (形态确认、补充下载链接/官网、记录配置导出路径)
  └─ 状态打标 (恢复意愿列 + 处置方式列 + Awesome 标记)
        │
        ▼
[阶段 4: 交付物导出 (Export)]
  ├─ 一键导出 RECOVERY_CHECKLIST.md (重装恢复执行备忘单)
  └─ 一键导出 AWESOME_LIST.md (精选软件库)
```

---

## 3. 数据模型设计 (Data Schema)

直接使用版本控制友好、可读可写的纯文本文件 `data/software.json`。

### 关键字段定义

```json
[
  {
    "id": "SW-001",
    "name": "Visual Studio Code",
    "category": "开发工具",
    "type": "desktop",
    "machines": [
      {
        "machine_id": "PC-Desktop",
        "form": "installed",
        "path": "C:\\Program Files\\Microsoft VS Code",
        "version": "1.93.0"
      },
      {
        "machine_id": "PC-Laptop",
        "form": "installed",
        "path": "C:\\Program Files\\Microsoft VS Code",
        "version": "1.92.1"
      }
    ],
    "restore_intent": "must",
    "backup_strategy": "sync_account",
    "download_url": "https://code.visualstudio.com/",
    "config_notes": "通过 GitHub 账号同步配置与插件，本地无需单独打包",
    "is_awesome": true,
    "awesome_role": "主力代码编辑与跨平台脚本编写",
    "status": "reviewed"
  }
]
```

#### 字段枚举说明
- **`restore_intent` (恢复意愿)**:
  - `must` (必须恢复)
  - `should` (建议恢复)
  - `on_demand` (用到再装)
  - `drop` (淘汰弃用)
  - `unreviewed` (待定)
- **`backup_strategy` (处置方式)**:
  - `copy_dir` (保留/压缩整个便携目录)
  - `copy_config` (手动导出/备份配置文件)
  - `redownload` (官网或包管理重新下载)
  - `sync_account` (依赖云端/账号登录同步)
  - `none` (无需任何操作)

---

## 4. 技术栈选型与系统架构

运行环境为 Windows 10 / 11（依赖 WebView2 Runtime，Win11 自带）；采集脚本优先使用 PowerShell 7，兼容 PowerShell 5.1。

### 4.1 技术选型
- **桌面应用框架**：Tauri v2（原生 Rust，无本地 HTTP 服务、不占用端口；前端资源内嵌进 exe）。
- **采集脚本 (`scripts/collect.ps1`)**：原生 PowerShell，读取注册表、快捷方式与目录结构；通过 Rust 的 `include_str!` 编译进 exe，扫描时释放到系统临时目录执行（优先 `pwsh`，兼容 PowerShell 5.1）。
- **后端核心 (Rust，`src-tauri/src/`)**：
  - 扫描：释放内嵌脚本 → 采集 → 解析入库 → 删除临时目录（证据用完即弃，不落 `evidence/`）。
  - 读写 `data/software.json` 与 `data/config.json`。
  - 通过 Tauri command 暴露配置、状态、增删改、批量、合并、LLM 分析、扫描、导出等接口。
  - 生成最终 Markdown 交付文档（直接回传前端，不落盘）。
  - 数据根目录固定为 exe 所在目录下的 `data/`（便携 = exe + `data/` 文件夹）。
- **前端决策中台 (`client/`)**：
  - 纯静态 HTML / CSS / JS，无需 Vite 等构建工具，资源直接内嵌进 exe。
  - 通过 `tauri-shim.js` 把 `fetch('/api/*')` 映射到 `invoke()`；在纯浏览器环境下自动退化为原 Bun 服务模式。
  - 核心功能：
    - 多机聚合展示与标签过滤。
    - 单元格即时编辑（名称、官网、备注等）。
    - 快速切换意愿/处置下拉状态。
    - 一键合并重复条目、一键删除无效条目。
    - 一键导出 Markdown 恢复手册。

> 历史 Bun 中台版本（Node.js 本地服务 + 浏览器前端）完整保留在 `bun` 分支。

---

## 5. 实施里程碑 (Milestones)

- [ ] **M1: 采集器与多机样本**
  - 编写 `scripts/collect.ps1`。
  - 在当前电脑运行，验证注册表卸载项、便携目录扫描及快捷方式解析。
- [ ] **M2: 数据解析与存储**
  - 编写入库逻辑，将采集到的证据清洗转换为初始 `data/software.json`。
- [ ] **M3: Web 决策中台构建**
  - 搭建表格界面，实现搜索、多机过滤、行编辑、合并/删除、状态打标与持久化。
- [ ] **M4: 恢复清单与 Awesome List 导出**
  - 实现一键导出 `RECOVERY_CHECKLIST.md` 与 `AWESOME_LIST.md`。
