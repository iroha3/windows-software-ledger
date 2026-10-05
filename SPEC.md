# SPEC: windows-software-ledger

## 1. 背景与核心目标

### 1.1 背景
用户面临 3 台 Windows 电脑（均为配置完整的开发机）的重装、备份与软件迁移需求。
- 软件形态杂乱：安装版（Registry / Winget / Scoop）、便携版（绿色解压包）、独立命令行工具（CLI / 单可执行文件）。
- 关键资产关联：各种软件的配置位置（AppData、本地目录）、扩展（VS Code / 浏览器等）需要人工关注。
- 核心诉求：提供一个**轻量、高效的表格化决策与状态管理中台**，协助完成清点、去重、决策打标，并最终输出一份清晰的重装恢复清单与精选软件资产库。

### 1.2 明确“做”与“不做”
- **不做**：
  - 自动备份执行脚本（不搞复杂的 `robocopy` 自动备份，便携软件手动拷/压 zip，个性化设置手动导出更为可靠）。**此边界仅针对被清点的软件**；本工具自身的开发 / 构建环境是确定性的，按其依赖清单复现即可，见 [BUILD.md](BUILD.md)。
  - 追求 100% 自动识别：扫描认回已有条目是「尽力而为」——匹配键 = **机器 + 安装路径（bin path）归一化完全一致**；**同名不算同一实体**。仅当候选本身没有任何路径（个别注册表项缺 `InstallLocation`）时，才退回「同机器 + 同名」匹配。路径变了 / 路径为空即视为新候选，需手动合并。
  - 内置复杂视觉模型（VLM/OCR 截图识别不需要集成进软件；需要看图的零星场景直接手动贴给大模型即可）。
  - 过度复杂的快捷键体系（重点是直观、低负担的表格数据操作与状态打标）。
  - 任何商业 SaaS、多租户、外置数据库服务。
  - 扫描或备份环境变量块、SSH key、git 凭据等敏感信息（采集器只跑只读白名单命令）。
- **做**：
  - **轻量采集**：通过 PowerShell 脚本快速抓取各机器的安装列表、便携目录第一层、快捷方式与环境 PATH。
  - **表格化决策中台**：提供一个可视化的本地 Web 表格界面（支持 3 台机器筛选、搜索、分类过滤）。
  - **人机协同整理**：支持直观地删除垃圾条目、合并重复项、补充官网/下载网址与配置备注。
  - **清晰的双列决策**：
    - `恢复意愿`（必须 / 用到再装 / 淘汰 / 待确认）
    - `处置方式`（保留目录 / 重新下载 / 账号同步 / 无需处理）
  - **高价值文档导出**：
    1. `RECOVERY_CHECKLIST.md`：按优先级分组的重装恢复检查清单（含下载链接、配置导出位置与验证备忘）。
    2. `AWESOME_LIST.md`：个人精选工作流软件清单。
    3. `SOFTWARE_LEDGER.xlsx`：整份软件清单的 Excel 表格（单表，表头之上有标题与导出信息：导出时间 / 软件版本 / 条数；列含名称/分类/版本/形态/所在机器/安装路径/意愿/处置/进度/精选/链接/备忘）。由 `src-tauri/src/xlsx.rs` 零依赖（手写 ZIP + OOXML）生成，冻结表头 + 自动筛选 + 加粗表头。
  - **开发环境复现**：扫描时用只读命令白名单采集 Python / Rust / VS Code 扩展 / Git / Node / Go / .NET 的全局包、工具链、源/镜像配置与全局配置；包列表与配置原文落到 `evidence/<设备>/dev-env/`，页面给出引用这些文件的短恢复命令（避免逐包罗列）；作为独立页面按设备折叠展示，不写入软件清单。

---

## 2. 核心工作流 (Workflow)

```
[阶段 1: 证据收集 (Collect)]
  ├─ 机器 A / B / C 运行极简采集脚本 (PowerShell)
  │    ├─ 已安装应用 (Registry / Winget / Scoop)
  │    ├─ 扫描便携/绿色软件目录
  │    └─ 快捷方式与环境 PATH
  └─ 输出各机器证据文件到 evidence/<machine_id>/
        │
        ▼
[阶段 2: 数据入库 (Ingest)]
  ├─ 读取 evidence/ 结构化数据
  └─ 解析并初始化生成主数据 data/software.json
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
  ├─ 一键导出 AWESOME_LIST.md (精选软件库)
  └─ 一键导出 SOFTWARE_LEDGER.xlsx (软件清单表格)
```

---

## 3. 数据模型设计 (Data Schema)

直接使用版本控制友好、可读可写的纯文本文件 `data/software.json`。

### 关键字段定义

```json
[
  {
    "uuid": "0f8fad5b-d9cb-469f-a165-70867728950e",
    "id": "SW-001",
    "name": "Visual Studio Code",
    "category": "开发工具",
    "type": "desktop",
    "machines": [
      {
        "machine_id": "PC-Desktop",
        "form": "installed",
        "install_location": "C:\\Program Files\\Microsoft VS Code",
        "version": "1.93.0"
      },
      {
        "machine_id": "PC-Laptop",
        "form": "installed",
        "install_location": "C:\\Program Files\\Microsoft VS Code",
        "version": "1.92.1"
      }
    ],
    "restore_intent": "must",
    "backup_strategy": "sync_account",
    "download_url": "https://code.visualstudio.com/",
    "config_notes": "通过 GitHub 账号同步配置与插件，本地无需单独打包",
    "is_awesome": true,
    "awesome_role": "主力代码编辑与跨平台脚本编写",
    "prep_status": "ready",
    "has_config": true,
    "icon_file": "visualstudiocode-1a2b3c4d5e6f7788.png"
  }
]
```

#### 字段枚举说明
- **`restore_intent` (恢复意愿)**:
  - `must` (必须恢复)
  - `on_demand` (用到再装)
  - `drop` (淘汰弃用)
  - `unreviewed` (待确认)
- **`backup_strategy` (处置方式)**:
  - `copy_dir` (保留/压缩整个便携目录)
  - `redownload` (官网或包管理重新下载)
  - `sync_account` (依赖云端/账号登录同步)
  - `none` (无需任何操作)
  - 推导规则：默认 `none`；当恢复意愿评为 `must` 时，按形态自动给出默认值——绿色版 → `copy_dir`，安装版 → `redownload`。手动选择的处置方式在下次改动恢复意愿前保留。

#### 证据文件（`data/evidence/<machine_id>/`）

> `machine_id` = 主机名（见上「身份与存储」）。

采集脚本产出的原始证据，可随便携目录拷走，**与 `software.json` 解耦**：

- `registry-apps.json` / `portable-apps.json` / `shortcuts.json` / `winget-apps.json` / `scoop-apps.json` / `cli-tools.json` / `machine-info.json`：软件候选来源。
- `dev-env.json` + `dev-env/`：开发环境声明式清单与包列表 / 配置原文（见 BUILD.md）。
- `browser-extensions.json`：已安装浏览器的扩展只读元数据（见 BUILD.md）。
- `timings.json`：仅在 `-Timing` 时产出，记录各步骤耗时。
- `app-icons/`：从 exe 抽取的 32×32 图标（证据 JSON 的 `icon_file` 字段引用其文件名），扫描时生成，重扫不清除。
- `screenshots/`：供用户手动存放参考截图，重扫不清除。

#### 身份与存储（uuid 为唯一内部标识）

- **`uuid`**：每个实体的**内部唯一标识**（UUID v4）。所有内部绑定（保管箱 / 图标 / 合并 / 删除 / 前端传参）一律以它为准。软件条目在 `software.json` 里持久化；扩展 / 浏览器在用户层文件里持久化。
- **`id`（`SW-xxx`）**：**仅供展示**的顺序号，可被复用，不参与任何存储绑定；搜索同时匹配 `SW-ID` 与名称。
- **不做旧版本兼容**：`read_software` 是**纯读**——不补 uuid、不归并已下线的 `copy_config`、不读 evidence。**uuid 是所有内部绑定的硬前提**，匹配 / 删除 / 合并一律只认 uuid（不再回退 `SW-ID`）。数据结构变更直接破坏，不写兼容层。
- **重扫认回**：扫描只按「匹配键」把已有实体的 uuid 认回来，绝不另铸新号。软件的匹配键 = **机器 + 安装路径（bin path）**（`find_known`，**路径不变就不换 uuid；同名不算同一实体**）；仅当候选无任何路径时，才退回「同机器 + 同名」。扩展 = `(machine_id, browser_id, profile, ext_id)`（扩展跟软件一样按机器分开，同一扩展装在两台机器上各自独立；扩展 ID 只在单个浏览器内唯一，所以机器与浏览器维度都要带）；浏览器 = `(machine_id, browser_id)`。
- **路径（`data/` 不追求人类可读）**：
  - 图标：`data/icons/<uuid>.png`
  - 保管箱：`data/vault/<kind>/<uuid>/`（`kind` ∈ `soft` / `browser` / `ext`），**不再分主机名**
  - 垃圾桶：`data/trash/<stamp>-<rand>/`（内含 `meta.json` 记录原始相对路径，另一项为被删的文件/目录本体）
- **用户层文件**：
  - `data/extensions.json`：键 = 扩展 uuid，值内嵌匹配键 `machine_id` / `browser_id` / `profile` / `ext_id` + 备注等用户字段。
  - `data/browsers.json`：键 = 浏览器 uuid，值内嵌 `machine_id` / `browser_id`。
- **机器身份**（独立于实体 uuid 的第三条轴）：`machines[].machine_id` 直接就是**主机名**（`COMPUTERNAME`），机器名本身就是显示名，**绝不把内部标识当名字展示**（正如软件只秀 `SW-ID`、不秀 uuid）。改名视为换机器，由用户用别名统一显示。
- **evidence 与台账彻底解耦**：`data/evidence/` 纯给人看（及其查看页），**软件台账完全不读它**——随便增删改 evidence（包括 `machine-info.json`）都不影响软件清单 / 合并 / 删除 / 归档。因此 evidence 目录就用**机器名**命名（`data/evidence/<主机名>/`），可读性优先。上一版曾用 MachineGuid 当机器 id 的遗留数据不再自动归位（接受破碎，不写迁移脚本）。

#### 软件图标（`data/icons/<uuid>.png`）

图标与条目**显式绑定**，一条记录一个文件，与显示号 / 软件名 / 机器无关：

- **字段**：每条软件的 `icon_file` 记图标文件名（形如 `<uuid>.png`）。缺省或为空 → 前端回退通用图标。
- **命名**：`store::icon_file_name(uuid)` = `<uuid>.png`。
- **写入**：扫描导入**新条目**时从 `evidence/<主机名>/app-icons/` 复制到 `data/icons/<uuid>.png`，并写入字段；重扫对「已知」条目走 `known_icon_refreshes` **补齐**（仅当条目没有 `icon_file`、或其文件不存在时才补）。
- **读取**：`get_software` 读该文件编码为 data URI 注入返回值的 `icon` 字段（**不写回** `software.json`）。
- **删除**：`delete_software` 直接删 `<uuid>.png`（一条一个，**无共享、无需引用计数**）。
- **合并**：锚点自己有图标则保留；否则从第一个有图标的子项**复制到锚点 uuid 名下**（`<锚点uuid>.png`），再删除被并入项的图标文件。
- **构建兼容**：图标命名**不向后兼容**——新版 `data/` 必须搭配同版本 exe。

#### 合并与删除（整理操作）

- **合并**（`merge_software`）：target = **第一个选中项**（前端 `selectedIds` 插入顺序的 `ids[0]`，注意 `Set` 的插入顺序 = 勾选顺序；「全选」时锚点 = 筛选结果第一条）；其余选中项并入 target 后从台账移除。

  入口：① 主页批量操作（选 ≥2 项，锚点 = 第一个勾选）；② 主页抽屉与 ③ 卡片速审页的「同名条目」区块——列出与当前条目**名称去空白 + 小写后相等**的其它条目，可逐条「并入」或「全部并入」，锚点 = 当前条目（谓词与后端 `find_known` 的「同名」判定一致；两处共用同一套前端逻辑与 `.merge-*` 样式）。合并前需先冲刷防抖自动保存，且待并入项带非默认评档时先弹确认（提示该评档将被丢弃）。

  **统一心智模型：锚点为准；事实与资产在锚点基础上并入或补齐，决策与主观永不并入。**

  锚点的值一律优先；子项只在「锚点为空 / 未标」处补齐，绝不覆盖锚点已经表达的结论。决策字段的默认值（`unreviewed` / `todo` / `none` / `false`）**不是「空」，而是一个已表达的取值**——例如锚点为「未评」时，合并**不会**继承子项的「必须恢复」；锚点为「待办」时不会继承子项的「已就绪」。

  按字段展开：

  | 字段 | 归类 | 合并行为 |
  |---|---|---|
  | `machines` | 事实集合 | **按 `machine_id` 去重**（同一台机器只留一条，缺失字段补齐）；锚点为基、子项并入 |
  | `has_config` | 事实标志 | 取或（并集）；`false` 视作「未标」，任一为 `true` 即 `true` |
  | `version` / `download_url` | 事实 | 锚点非空则保留，空则按勾选顺序取子项第一个非空值 |
  | `icon_file` | 资产 | 同上（细则见上「软件图标」） |
  | `config_notes` | 文本 | 锚点空则用子项；非空则 `锚点; 子项` 追加 |
  | `restore_intent` / `backup_strategy` / `prep_status` | 决策 | 锚点为准，**从不并入**；默认值即结论 |
  | `is_awesome` / `awesome_role` | 主观 | 锚点为准，**从不并入** |
  | `name` / `category` / `type` | 标识 / 规则事实 | 锚点为准 |
  | `id` / `created_at` / `is_new` | 元数据 | 锚点保留（`created_at`、`is_new` 均不刷新） |

  分类依据：**事实/资产**（机器列表、版本、下载地址、图标、配置标志）可安全合并或补齐；**决策/主观**（恢复意愿、处置方式、准备状态、精选）是用户的明确意志，一律以锚点为准。

  因此「把已评的子项并进未评的锚点会丢掉子项评审」是**刻意的取舍而非缺陷**：需要分别保留两台机器的决策时，就不要合并；合并即表示接受「以锚点为准」。

  > 附注：`unreviewed` 在统计口径上等同于「尚未评档」（`restore_intent` 为空串或 `unreviewed` 都计入未评），但在**合并**口径下它是一个确定取值，不触发补齐。
- **删除**（`delete_software`）：按 `uuid` 移除条目并写入 `ignored.json` 墓碑。墓碑**按路径精确命中**（`tombstone_match`：候选带路径时只认路径，仅双方都无路径才退回同名），避免删掉一份同名安装就永久误伤其它同名安装。归档目录 `data/vault/<kind>/<uuid>/` 整个**软删除**到 `data/trash/`（写 `meta.json` 记住原始路径，不 `remove_dir_all`，避免「删一条记录」变成「瞬间抹掉几个 G」）；图标 `<uuid>.png` 体积可忽略，直接删。

#### 配置归档（`data/vault/<kind>/<uuid>/`）

用户**手动**放入的文件保管箱（配置文件、安装包等），键 = 实体 uuid，**不再分主机名**：`vault/soft/<uuid>/`、`vault/browser/<uuid>/`、`vault/ext/<uuid>/`。默认收起，不自动采集。删除条目时整个目录**软删除**到 `data/trash/`（绝不物理销毁）；合并时子项文件搬入锚点目录，同名冲突锚点优先、子项同名者进垃圾桶。浏览器扩展的备注 / 意愿等存 `data/extensions.json`（按扩展 uuid，匹配键内嵌，重扫不丢）。

#### 首次扫描与增量导入

`scan_preview` 只解析候选，`scan_commit` 只写入勾选项。若导入前台账为空（首次全量扫描），导入的条目**不标记 `is_new`**；台账非空时，勾选导入的条目 `is_new = true`。删除的条目写入 `data/ignored.json` 作墓志铭，重扫默认不勾选，手动再勾选可复活。

#### WebDAV 同步（单写者模型）

不追求真正的分布式合并，而是用「同一时刻只有一台机器编辑」把并发消掉。

- **会话锁**：远端 `lock.json` 租约（`{owner, host, acquired_at, expires_at}`，15 分钟）。开软件抢锁并拉取，关软件推送并释放（Rust `on_window_event` 拦截关闭）；心跳每 5 分钟续租。抢不到锁 = 本机只读：顶部横幅提示，且**前后端双层禁用编辑**（前端禁用编辑控件、关闭 WebDAV 时隐藏同步入口；后端 `commands.rs` 对全部写命令直接拒绝），可强制接管。同机旧实例残留的租约视为自家锁，可直接接管。异常退出后租约到期自动可被接管。
- **传输**：远端镜像 `data/`（排除 `trash/`、`.sync/`、`webdav.json`），并维护 `manifest.json`（每个文件的 sha256 / size / mtime）。本地 `.sync/manifest.json` 是上次同步基线，`.sync/index.json` 是指纹缓存（size+mtime 未变则复用 hash，避免重算几个 G）。只用 GET / PUT / DELETE / MKCOL / MOVE，不用 PROPFIND，因此无 XML 依赖。
- **常态 = 镜像**：拉取时按「基线 / 本地 / 远端」三路判断，仅单侧改动取该侧；推送时本地为权威，上传变更并删除远端多余（删除自然传播）。JSON 台账仅当双方相对基线都改才按 uuid 语义合并（首次接入即此情形：双方按 uuid 并集，`machines[]` 按 `machine_id` 并集）。
- **新旧判据**：每条记录 `updated_at`（回退 `created_at`）。冲突时较新者为准，空字段由旧记录补齐。
- **强制覆盖（高级）**：`本地覆盖远端` / `远端覆盖本地`，覆盖前自动快照（远端 → `_backup/<时间戳>/`，本地 → `data/trash/sync-<时间戳>/`）。
- **凭据**：`data/webdav.json` 仅存本地，不参与同步；`llm_api_key` 随 `config.json` 同步。`scan_directories` 按主机名分键。
- **兼容**：远端 `manifest.json` 携带 `schema_version`，不匹配直接拒绝（不做兼容层）。

---

## 4. 技术栈选型与系统架构

运行环境为 Windows 10 / 11（依赖 WebView2 Runtime，Win11 自带）；采集脚本优先使用 PowerShell 7，兼容 PowerShell 5.1。

### 4.1 技术选型
- **桌面应用框架**：Tauri v2（原生 Rust，无本地 HTTP 服务、不占用端口；前端资源内嵌进 exe）。
- **采集脚本 (`scripts/collect.ps1`)**：原生 PowerShell，读取注册表、快捷方式与目录结构；通过 Rust 的 `include_str!` 编译进 exe，扫描时释放到系统临时目录执行（优先 `pwsh`，兼容 PowerShell 5.1）。
- **后端核心 (Rust，`src-tauri/src/`)**：
  - 扫描：释放内嵌脚本 → 采集到 `data/evidence/<machine>/`（长期保留，含 `screenshots/`）→ 解析候选 → 弹窗勾选导入；已存在条目零改动。
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
    - 一键导出 Markdown 恢复手册与 Excel 软件表格（`.xlsx`）。
    - 开发环境复现页（`dev_env.html`）：按设备折叠展示开发环境声明式清单（含落盘文件列表）与引用这些文件的短恢复命令，只读、不改动软件清单。

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
