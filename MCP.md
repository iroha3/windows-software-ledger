# MCP：让 AI agent 读懂你的软件台账

软件备份台账内置了一个 [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) 服务端，**默认只读**；在「设置 → Agent 集成」里可以开启写入，把台账交给 agent 一起维护。接上它之后，Claude、Cursor 等支持 MCP 的 agent 就能直接查询你这份台账：这台机器装过什么、装在哪、有没有官网链接、有没有归档的安装包或配置文件。

于是你可以对 agent 说一句：

> 帮我把这台电脑该装的软件重新装一遍。

它自己会去查台账、整理出方案交给你确认，再动手——**规划由 agent 来做，台账只负责提供事实。**

## 定位：默认只读的信息源

不开启写入时，这个 MCP 服务端**只做一件事：把 `data/` 里的信息读出来递给 agent**。

- 不修改你的台账（连首次发现的浏览器扩展都不会落盘）；
- 不下载、不执行任何东西；
- 不联网。

它不替 agent 做决定，也不限定用途：重装只是主要场景，整理清单、跨机对比、审计迁移等都可以。具体怎么做由 agent 自行规划，并交给你确认。

在「设置 → Agent 集成 (MCP)」勾选 **允许 AI agent 写入台账** 后，会额外开放一组写入工具，让 agent 帮你更新版本号、补官网链接、调整恢复意愿、增删条目。写入是**风险自担**的可选项，默认关闭。

## 它能给出什么

| 工具 | 作用 |
|---|---|
| `list_machines` | 所有机器、别名、软件数、当前机器；用来发现「别的机器装了、这台没有」 |
| `search_software` | 按关键词 / 机器 / 恢复意愿 / 形态 / 处置方式 / 精选过滤，返回精简条目 |
| `get_software` | 单条完整信息：各机器路径、官网链接、配置备注、**归档文件清单** |
| `list_vault_files` | 某条实体归档目录里的文件（名称 / 大小 / 时间） |
| `list_dev_env` | 各机器的开发环境：每个工具链的全局包、配置文件列表与短恢复命令（路径已展开为绝对路径） |
| `get_dev_env_file` | 读取 `dev-env/` 下某个文件的**原文**（如 `python-requirements.txt`、`vscode-extensions.txt`），跨机器重装时把包列表带过去 |
| `list_browser_extensions` | 各机器浏览器扩展的只读元数据与备注 |
| `get_recovery_doc` | 《重装恢复清单》/《精选资产库》Markdown 原文 |

### 写入工具（可选，默认关闭）

需先在「设置 → Agent 集成 (MCP)」勾选「允许 AI agent 写入台账」。字段更新走白名单，`id` / `uuid` / `machines` 等结构字段不可改。

| 工具 | 作用 |
|---|---|
| `update_software` | 更新单条记录的可写字段（`version` / `download_url` / `restore_intent` / `backup_strategy` / `config_notes` / `is_awesome` 等） |
| `batch_update` | 对多条记录批量写入相同字段 |
| `add_software` | 新增软件入库（挂到当前机器） |
| `delete_software` | 删除记录（写墓碑 + 归档移入 `data/trash/`，可回滚） |
| `merge_software` | 合并重复条目（**不可逆**，合并前建议先 `get_software` 确认） |

另外还暴露了几个只读 Resource（`ledger://software`、`ledger://machines`、`ledger://checklist`、`ledger://awesome`、`ledger://dev-env`）和一个 `reinstall_plan` 提示词模板。

## 怎么配置

> 最省事的方式：打开软件 → 右上角 **设置** → 左栏 **Agent 集成 (MCP)** → 选好客户端后点「复制配置」，粘贴到对应文件即可。下面手动配置的写法与之一致。

服务端就是应用自己的 exe，加一个 `--mcp` 参数即可（客户端会以 stdio 方式拉起它，不需要你先打开应用）。**`command` 请填 exe 的绝对路径，指向你日常使用的那份**——数据目录始终是 exe 同级的 `data/`。

### 通用配置

```json
{
  "mcpServers": {
    "software-ledger": {
      "command": "D:\\Tools\\软件备份台账\\windows-software-ledger.exe",
      "args": ["--mcp"]
    }
  }
}
```

> 路径里的反斜杠要写成 `\\`；不要用 `cmd /c` 包一层。

### Claude Desktop

编辑 `%APPDATA%\Claude\claude_desktop_config.json`，把上面的 `mcpServers` 片段加进去，重启 Claude。

### Claude Code

```bash
claude mcp add software-ledger -- "D:\Tools\软件备份台账\windows-software-ledger.exe" --mcp
```

### Pi

Pi 支持 MCP，用命令添加最省事：

```bash
pi mcp add software-ledger -- "D:\Tools\软件备份台账\windows-software-ledger.exe" --mcp
```

也会直接读配置文件：用户级 `~/.pi/agent/mcp.json`（Windows：`%USERPROFILE%\.pi\agent\mcp.json`）或项目级 `.pi/mcp.json`，格式与上面的通用配置一致（`mcpServers`，项目配置需先授予项目信任）。添加后在会话里用 `/mcp` 查看连接，或 `/reload` 让改动生效。

### Cursor

在 `%USERPROFILE%\.cursor\mcp.json`（或项目的 `.cursor/mcp.json`）里加入上面的 `mcpServers` 片段。

### 其他客户端

任何支持 stdio MCP 的客户端都一样：命令 = exe 绝对路径，参数 = `--mcp`。配好后可以先用一句「列出我的软件台账里都有哪些机器」验证连通。

## 写入权限

写入只有一个闸门：在「设置 → Agent 集成 (MCP)」勾选「允许 AI agent 写入台账」。关闭时写入工具直接返回错误，只读工具不受影响。

> 同步是手动的，没有会话锁、也没有只读模式，所以不再有「本机是否持锁」这一层限制；agent 写入后，你需要点一次「立即同步」把改动推到远端。

所有写入都会追加到 `data/.mcp/audit.jsonl`（时间 / 工具 / uuid / 改动内容），方便回看与撤销。

写入后，软件主界面会**自动刷新**（前端每 2 秒比对一次 `data/` 的版本指纹，变了才拉全量），你能实时看到 agent 改了什么；如果你正开着抽屉 / 弹窗在编辑，则等空闲后再刷，不打断操作。

## 一个示例

**你**：帮我看看这台电脑该装什么，再给我一份重装方案。

**Agent**（自动完成，你只看到过程）：

1. `list_machines` → 发现当前机器是 `REDMIBOOK`，另有 `PC`、`Surface` 两台；
2. `search_software { machine: "REDMIBOOK", intent: "must" }` → 拿到必须恢复的软件；
3. `search_software { not_on_machine: "REDMIBOOK" }` → 发现别的机器上有、这台没有的，提示你可能漏装；
4. 对每条调 `get_software` → 看到归档里有 `xxx-setup.exe` 就直接用；没有就看 `download_url` 去官网；两者都没有就标记「需自行搜索」；
5. `list_dev_env` → 拿到 Python / Rust / VS Code 扩展 / Node 等工具链的包清单与恢复命令；需要文件原文时（跨机器重装）再调 `get_dev_env_file` 把 `python-requirements.txt`、`vscode-extensions.txt` 等内容拉出来；
6. 输出一份按优先级排序的方案，逐条标注信息来源，等你确认。

它不会擅自安装任何东西——方案里会明确哪些能从你的归档直接恢复、哪些要去官网、哪些得自己找。

## 安全说明

- **默认只读**：不开写开关时没有任何写命令，不改动台账。
- **写入可选且受限**：开启后字段走白名单；删除可回滚、合并不可逆，且全程记入本地审计日志 `data/.mcp/audit.jsonl`。
- **无网络暴露**：走 stdio，不开监听端口，本机没有多出任何可被连接的入口。能读到这份清单的，本就是不接 MCP 也能直接读 `data/software.json` 的进程。
- **不泄露配置里的密钥**：`config.json` 里的 LLM API Key、`webdav.json` 里的密码都不会经 MCP 输出；只暴露机器别名等非敏感字段。
- **归档只给元数据**：保管箱里的文件只返回文件名 / 大小 / 时间，绝不返回文件内容。

## 开发说明

- 服务端代码在 `src-tauri/src/mcp.rs`，纯手写 JSON-RPC，无新增依赖。
- `main.rs` 判断 `--mcp` 后进入 `mcp::serve_stdio()`，否则正常开 GUI。
- **新增工具时**，在 `mcp.rs` 的 `tools_list()` 里登记 schema、在 `call_tool()` 里加分发分支即可；不涉及前端，也不用碰 `lib.rs` 的命令注册表。
- **写入工具**统一先调 `write_gate()`（检查设置开关），并复用 `commands::*` 的后端逻辑；成功后再 `store::append_audit(...)`。字段白名单在 `WRITABLE_FIELDS`。
- 本地验收：`node scripts/mcp-client.js`——一个零依赖的迷你 MCP 客户端，会自动连上最新构建的 exe，跑一遍 `list_machines` / `search_software` / `list_dev_env` 演示。也可 `node scripts/mcp-client.js call get_software '{"name":"Git"}'` 单独调某个工具。
- 底层排查：`printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | windows-software-ledger.exe --mcp`
- ⚠️ **stdout 只能跑协议**，调试输出务必走 stderr，否则会污染 JSON-RPC 流。

## 版本要求

MCP 服务端随应用一同发布，源码已包含；旧版 exe 没有 `--mcp` 参数。发布需在 `src-tauri/Cargo.toml` 升版本号（推送到 `master` 后 CI 会自动构建对应 tag 的 Release）。
