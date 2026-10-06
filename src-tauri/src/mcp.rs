//! 只读 MCP (Model Context Protocol) 服务端。
//!
//! 定位：把软件台账当成一个**只提供信息**的数据源递给 AI agent，让它自己规划重装。
//! 因此这里**绝不写盘、不下载、不执行**——所有工具都只读。新增工具在 `tools_list` /
//! `call_tool` 两处登记即可。
//!
//! 传输：stdio。按 MCP 规范，一行一个 JSON-RPC 2.0 消息（消息内不得含换行）。
//! 客户端配置里 `command` 指向本 exe、`args` 为 `["--mcp"]`。
//!
//! ⚠️ stdout 只允许跑协议。任何调试输出都必须走 stderr，否则会污染 JSON-RPC 流。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};

use serde_json::{json, Value};

use crate::store;

/// 本服务端支持的 MCP 协议版本（客户端若声明自己的版本则回显，最大化兼容）。
const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "windows-software-ledger";

/// 初始化时下发给 agent 的说明。用大白话讲清楚「这是什么、怎么用、你别指望它替你干活」。
const INSTRUCTIONS: &str = "\
这是「软件备份台账」的只读信息源，记录用户多台 Windows 机器上装过的软件。\
每条软件包含：名称、分类、形态（desktop/portable/cli/runtime）、版本、所在机器与安装路径、\
恢复意愿（must/on_demand/drop/unreviewed）、处置方式（copy_dir/redownload/sync_account/none）、\
官网或下载链接、配置备注，以及用户手动归档的文件（可能是安装包，也可能是配置文件）。\
\n\n使用建议：先调 list_machines 看清有哪些机器和当前机器；再用 search_software / get_software 查询；\
用 get_software 可以看到某条软件归档里到底存了什么文件，以及官网链接。\
如果某条软件既没有归档安装包、也没有 download_url，说明台账不知道去哪下载，需要你自己上网搜索。\
可以用 search_software 的 not_on_machine 参数找出「别的机器装了、这台没有」的软件。\
\n开发环境（Python / Rust / Node / Go / .NET / Git / VS Code 扩展等）用 list_dev_env 查看：\
它给出每台机器上每个工具链的包清单、配置文件列表与短恢复命令，是重装开发机最直接的信息源。\
恢复命令里引用的清单文件（如 python-requirements.txt）可用 get_dev_env_file 读取原文，\
这样即便目标机器和台账不在同一台电脑上，也能把包列表带过去。\
\n重要：默认只读取台账。若用户已开启写入，你也可能拿到 update_software / batch_update / add_software / delete_software / merge_software 这些写入工具（改动会记入 data/.mcp/audit.jsonl），写入前先用只读工具确认目标 uuid。要不要安装、怎么安装，由你根据台账信息自行规划，并交给用户确认。";

// ---------------------------------------------------------------------------
// stdio 主循环
// ---------------------------------------------------------------------------

/// 以 stdio MCP 服务端模式运行，直到 stdin 关闭。
pub fn serve_stdio() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout();

    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                // 解析失败无法得知 id，按 JSON-RPC 约定用 null。
                eprintln!("[mcp] 无法解析消息: {e}");
                write_message(&mut stdout, &error_response(Value::Null, -32700, "Parse error"));
                continue;
            }
        };
        if let Some(resp) = dispatch(&msg) {
            write_message(&mut stdout, &resp);
        }
    }
}

fn write_message(out: &mut impl Write, msg: &Value) {
    if let Ok(line) = serde_json::to_string(msg) {
        let _ = out.write_all(line.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// 分发一条消息。通知（无 id）返回 None（不回复）。
fn dispatch(msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    if id.is_none() {
        // 通知：notifications/initialized、notifications/cancelled 等，静默处理。
        return None;
    }

    let outcome: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(handle_initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => call_tool(&params),
        "resources/list" => Ok(resources_list()),
        "resources/read" => read_resource(&params),
        "prompts/list" => Ok(prompts_list()),
        "prompts/get" => get_prompt(&params),
        other => Err((-32601, format!("Method not found: {other}"))),
    };

    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(id.unwrap_or(Value::Null), code, &message),
    })
}

fn handle_initialize(params: &Value) -> Value {
    let client_version = get_str(params, "protocolVersion");
    let version = if client_version.is_empty() {
        PROTOCOL_VERSION.to_string()
    } else {
        client_version
    };
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "subscribe": false, "listChanged": false },
            "prompts": { "listChanged": false }
        },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS
    })
}

// ---------------------------------------------------------------------------
// 取值小工具
// ---------------------------------------------------------------------------

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn get_str(v: &Value, key: &str) -> String {
    s(v, key).to_string()
}

fn get_bool(v: &Value, key: &str) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn get_limit(v: &Value, default: usize) -> usize {
    v.get("limit")
        .and_then(|x| x.as_i64())
        .map(|n| n.clamp(1, 500) as usize)
        .unwrap_or(default)
}

fn truthy(v: &Value, key: &str) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(false)
}

/// 安装路径：优先 `install_location`，回退 `path`。
fn loc_of(m: &Value) -> &str {
    let a = s(m, "install_location");
    if a.is_empty() {
        s(m, "path")
    } else {
        a
    }
}

fn machine_aliases() -> Value {
    store::get_config()
        .get("machine_aliases")
        .cloned()
        .unwrap_or_else(|| json!({}))
}

fn display_machine(aliases: &Value, mid: &str) -> String {
    aliases
        .get(mid)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(mid)
        .to_string()
}

/// 恢复意愿：空串归一为 `unreviewed`，便于 agent 判断。
fn normalized_intent(item: &Value) -> &str {
    let i = s(item, "restore_intent");
    if i.is_empty() {
        "unreviewed"
    } else {
        i
    }
}

// ---------------------------------------------------------------------------
// 条目投影：给 agent 的两种粒度
// ---------------------------------------------------------------------------

fn machines_of(item: &Value, aliases: &Value) -> Value {
    let mut out: Vec<Value> = Vec::new();
    if let Some(arr) = item.get("machines").and_then(|m| m.as_array()) {
        for m in arr {
            let mid = s(m, "machine_id");
            out.push(json!({
                "machine": mid,
                "alias": display_machine(aliases, mid),
                "form": s(m, "form"),
                "install_location": loc_of(m),
                "main_exe": s(m, "main_exe"),
                "version": s(m, "version"),
                "publisher": s(m, "publisher"),
            }));
        }
    }
    Value::Array(out)
}

/// 精简条目：列表用，省 token。
fn brief_item(item: &Value, aliases: &Value) -> Value {
    json!({
        "uuid": s(item, "uuid"),
        "id": s(item, "id"),
        "name": s(item, "name"),
        "category": s(item, "category"),
        "type": s(item, "type"),
        "restore_intent": normalized_intent(item),
        "backup_strategy": s(item, "backup_strategy"),
        "prep_status": s(item, "prep_status"),
        "is_awesome": truthy(item, "is_awesome"),
        "version": s(item, "version"),
        "download_url": s(item, "download_url"),
        "has_config": truthy(item, "has_config"),
        "config_notes": s(item, "config_notes"),
        "machines": machines_of(item, aliases),
    })
}

/// 全量条目：详情用，附带归档文件清单（agent 据此判断「有没有存安装包 / 配置文件」）。
fn full_item(item: &Value, aliases: &Value) -> Value {
    let uuid = s(item, "uuid").to_string();
    let mut v = brief_item(item, aliases);
    if let Some(o) = v.as_object_mut() {
        o.insert("awesome_role".into(), json!(s(item, "awesome_role")));
        o.insert("vault_files".into(), vault_files("soft", &uuid));
    }
    v
}

/// 列某实体归档目录下的文件（只给名称 / 大小 / 时间，不给内容）。
fn vault_files(kind: &str, uuid: &str) -> Value {
    if uuid.is_empty() {
        return json!([]);
    }
    let res = crate::commands::vault_list(kind.to_string(), uuid.to_string());
    res.get("files").cloned().unwrap_or_else(|| json!([]))
}

// ---------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "list_machines",
                "description": "列出台账里记录的所有机器：主机名、别名、是否为当前机器、每台的软件数与必须恢复数。用来了解有哪些机器，并配合 search_software 找出「别的机器装了、本机没有」的软件。",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "search_software",
                "description": "按条件搜索软件，返回精简条目列表。支持关键词、机器、意愿、形态、处置方式、精选过滤。用 not_on_machine 可以找出「某台机器上缺失」的软件。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "关键词，匹配名称 / 分类 / 官网 / 备注 / 安装路径 / 发布者" },
                        "machine": { "type": "string", "description": "只看装在这台机器上的（主机名或别名，忽略大小写）" },
                        "not_on_machine": { "type": "string", "description": "排除装在这台机器上的，用于找本机缺失的软件" },
                        "intent": { "type": "string", "enum": ["must", "on_demand", "drop", "unreviewed"], "description": "恢复意愿" },
                        "type": { "type": "string", "enum": ["desktop", "portable", "cli", "runtime"], "description": "软件形态" },
                        "strategy": { "type": "string", "enum": ["copy_dir", "redownload", "sync_account", "none"], "description": "处置方式" },
                        "awesome_only": { "type": "boolean", "description": "只返回精选（★）软件" },
                        "limit": { "type": "integer", "description": "最多返回多少条，默认 100，上限 500" }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "get_software",
                "description": "按 uuid 或名称取一条（或若干条）软件的完整信息：所有机器分布、安装路径、官网链接、配置备注，以及归档文件清单（可能包含安装包或配置文件）。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "uuid": { "type": "string", "description": "软件 uuid（精确匹配，推荐）" },
                        "name": { "type": "string", "description": "软件名称（模糊匹配，可能返回多条）" }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "list_vault_files",
                "description": "列出某个实体归档目录里的文件（名称 / 大小 / 修改时间），不含文件内容。kind 默认 soft（软件），也可查 browser / ext。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "uuid": { "type": "string", "description": "实体 uuid" },
                        "kind": { "type": "string", "enum": ["soft", "browser", "ext"], "description": "归档类型，默认 soft" }
                    },
                    "required": ["uuid"],
                    "additionalProperties": false
                }
            },
            {
                "name": "list_dev_env",
                "description": "列出各机器采集到的开发环境清单：Python / Rust / Node / Go / .NET / Git / VS Code 扩展等的全局包、配置文件列表与短恢复命令。restore_commands 里的路径已展开为真实绝对路径。这是重装开发机最直接的信息源。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "machine": { "type": "string", "description": "只看某台机器（可选，主机名或别名）" }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "get_dev_env_file",
                "description": "读取某台机器开发环境清单里某个文件的原文（如 python-requirements.txt、vscode-extensions.txt、node-globals.txt）。list_dev_env 只给出文件名与恢复命令；要跨机器复现时，需要这个工具拿到包列表内容本身。文件名取自 list_dev_env 的 files[].name。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "machine": { "type": "string", "description": "机器主机名或目录名" },
                        "file": { "type": "string", "description": "dev-env 目录下的文件名，如 python-requirements.txt" }
                    },
                    "required": ["machine", "file"],
                    "additionalProperties": false
                }
            },
            {
                "name": "list_browser_extensions",
                "description": "列出各机器已安装浏览器里的扩展只读元数据（名称 / 版本 / 来源 / 启用状态）与用户备注。不包含 Cookie、密码或扩展数据。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "machine": { "type": "string", "description": "只看某台机器（可选）" },
                        "browser": { "type": "string", "description": "只看某个浏览器（可选，按 id 或标签模糊匹配）" }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "get_recovery_doc",
                "description": "返回台账生成的两份 Markdown 文档原文：checklist = 重装恢复清单，awesome = 精选软件资产库。适合想要人类可读总览时使用。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "which": { "type": "string", "enum": ["checklist", "awesome"], "description": "要哪份文档，默认 checklist" }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "update_software",
                "description": "【写入】更新一条已存在的软件记录（按 uuid 定位）。可改字段：name / category / type / version / restore_intent / backup_strategy / prep_status / has_config / download_url / config_notes / is_awesome / awesome_role。适合修正版本号、补官网链接、调整恢复意愿 / 处置方式、写配置备注。要求：在「Agent 集成」里开启了写入开关。先 search_software / get_software 拿到 uuid。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "uuid": { "type": "string", "description": "目标软件 uuid" },
                        "updates": { "type": "object", "description": "要写入的字段（仅限上面列出的可改字段）", "additionalProperties": true }
                    },
                    "required": ["uuid", "updates"],
                    "additionalProperties": false
                }
            },
            {
                "name": "batch_update",
                "description": "【写入】对多条软件批量写入相同字段（按 uuid 列表）。字段白名单同 update_software。例如把一批软件统一设为 on_demand。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "uuids": { "type": "array", "items": { "type": "string" }, "description": "目标软件 uuid 列表" },
                        "updates": { "type": "object", "description": "要写入的字段（白名单同 update_software）", "additionalProperties": true }
                    },
                    "required": ["uuids", "updates"],
                    "additionalProperties": false
                }
            },
            {
                "name": "add_software",
                "description": "【写入】把台账里还没有的软件新增入库，自动挂到当前机器（form=manual，处置方式后续可在软件里评）。传入软件名列表。同名不会自动合并，请先 search_software 确认是否已存在。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "names": { "type": "array", "items": { "type": "string" }, "description": "要新增的软件名列表" },
                        "restore_intent": { "type": "string", "enum": ["must", "on_demand", "drop", "unreviewed"], "description": "恢复意愿，默认 must" }
                    },
                    "required": ["names"],
                    "additionalProperties": false
                }
            },
            {
                "name": "delete_software",
                "description": "【写入·可回滚】删除若干软件记录（按 uuid）。删除会写墓碑（防止重扫复活）并把归档目录移入 data/trash/，可人工找回。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "uuids": { "type": "array", "items": { "type": "string" }, "description": "要删除的软件 uuid 列表" }
                    },
                    "required": ["uuids"],
                    "additionalProperties": false
                }
            },
            {
                "name": "merge_software",
                "description": "【写入·不可逆】把若干条目合并进一个锚点条目（按 uuid）：机器分布并入锚点，随后被并项删除。用于同一款软件被重复建档时。合并前请先 get_software 确认内容，不可逆。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target_uuid": { "type": "string", "description": "保留的锚点条目 uuid" },
                        "merge_uuids": { "type": "array", "items": { "type": "string" }, "description": "要并入锚点的条目 uuid 列表" }
                    },
                    "required": ["target_uuid", "merge_uuids"],
                    "additionalProperties": false
                }
            }
        ]
    })
}

fn call_tool(params: &Value) -> Result<Value, (i64, String)> {
    let name = get_str(params, "name");
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    let outcome: Result<Value, String> = match name.as_str() {
        "list_machines" => tool_list_machines(),
        "search_software" => tool_search_software(&args),
        "get_software" => tool_get_software(&args),
        "list_vault_files" => tool_list_vault_files(&args),
        "list_dev_env" => tool_list_dev_env(&args),
        "get_dev_env_file" => tool_get_dev_env_file(&args),
        "list_browser_extensions" => tool_list_browser_extensions(&args),
        "get_recovery_doc" => tool_get_recovery_doc(&args),
        "update_software" => tool_update_software(&args),
        "batch_update" => tool_batch_update(&args),
        "add_software" => tool_add_software(&args),
        "delete_software" => tool_delete_software(&args),
        "merge_software" => tool_merge_software(&args),
        other => return Err((-32602, format!("Unknown tool: {other}"))),
    };

    // 工具级错误按 MCP 约定放在 result.isError 里，而不是 JSON-RPC error。
    Ok(match outcome {
        Ok(v) => json!({
            "content": [{ "type": "text", "text": pretty(&v) }],
            "isError": false
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        }),
    })
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn tool_list_machines() -> Result<Value, String> {
    let items = store::read_software();
    let aliases = machine_aliases();
    let current = crate::commands::current_machine();

    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for item in &items {
        // 一台机器上可能有多条记录（注册表安装 + 快捷方式等），但只算「一款软件」一次，
        // 口径与 UI 标签页一致（distinct 软件数）。
        let mut seen: Vec<String> = Vec::new();
        if let Some(arr) = item.get("machines").and_then(|m| m.as_array()) {
            for m in arr {
                let mid = s(m, "machine_id");
                if mid.is_empty() {
                    continue;
                }
                if !order.iter().any(|x| x == mid) {
                    order.push(mid.to_string());
                }
                if seen.iter().any(|x| x == mid) {
                    continue;
                }
                seen.push(mid.to_string());
                let entry = counts.entry(mid.to_string()).or_insert((0, 0));
                entry.0 += 1;
                if s(item, "restore_intent") == "must" {
                    entry.1 += 1;
                }
            }
        }
    }

    let machines: Vec<Value> = order
        .iter()
        .map(|mid| {
            let (total, must) = counts.get(mid).copied().unwrap_or((0, 0));
            json!({
                "machine": mid,
                "alias": display_machine(&aliases, mid),
                "is_current": mid.eq_ignore_ascii_case(&current),
                "software_count": total,
                "must_count": must,
            })
        })
        .collect();

    Ok(json!({
        "current_machine": current,
        "total_software": items.len(),
        "machines": machines,
    }))
}

/// 把用户给的机器名（可能是别名）解析成一组可比较的 machine_id（小写）。
fn resolve_machine_filter(aliases: &Value, filter: &str) -> Option<Vec<String>> {
    let f = filter.trim().to_lowercase();
    if f.is_empty() {
        return None;
    }
    let mut out = vec![f.clone()];
    if let Some(obj) = aliases.as_object() {
        for (mid, alias) in obj {
            let al = alias.as_str().unwrap_or("").to_lowercase();
            if al == f || mid.to_lowercase() == f {
                out.push(mid.to_lowercase());
            }
        }
    }
    out.sort();
    out.dedup();
    Some(out)
}

fn item_on_any_machine(item: &Value, targets: &[String]) -> bool {
    if let Some(arr) = item.get("machines").and_then(|m| m.as_array()) {
        for m in arr {
            let mid = s(m, "machine_id").to_lowercase();
            if targets.iter().any(|t| *t == mid) {
                return true;
            }
        }
    }
    false
}

fn matches_query(item: &Value, q: &str) -> bool {
    let mut hay = String::new();
    for k in ["name", "category", "download_url", "config_notes", "awesome_role", "id"] {
        hay.push_str(s(item, k));
        hay.push('\n');
    }
    if let Some(arr) = item.get("machines").and_then(|m| m.as_array()) {
        for m in arr {
            hay.push_str(s(m, "machine_id"));
            hay.push('\n');
            hay.push_str(loc_of(m));
            hay.push('\n');
            hay.push_str(s(m, "main_exe"));
            hay.push('\n');
            hay.push_str(s(m, "publisher"));
            hay.push('\n');
        }
    }
    hay.to_lowercase().contains(q)
}

fn tool_search_software(args: &Value) -> Result<Value, String> {
    let items = store::read_software();
    let aliases = machine_aliases();
    let query = get_str(args, "query").to_lowercase();
    let intent = get_str(args, "intent");
    let typ = get_str(args, "type");
    let strategy = get_str(args, "strategy");
    let awesome_only = get_bool(args, "awesome_only");
    let limit = get_limit(args, 100);

    let want = resolve_machine_filter(&aliases, &get_str(args, "machine"));
    let exclude = resolve_machine_filter(&aliases, &get_str(args, "not_on_machine"));

    let mut out: Vec<Value> = Vec::new();
    let mut truncated = false;
    for item in &items {
        if !query.is_empty() && !matches_query(item, &query) {
            continue;
        }
        if let Some(targets) = &want {
            if !item_on_any_machine(item, targets) {
                continue;
            }
        }
        if let Some(targets) = &exclude {
            if item_on_any_machine(item, targets) {
                continue;
            }
        }
        if !intent.is_empty() && normalized_intent(item) != intent {
            continue;
        }
        if !typ.is_empty() && s(item, "type") != typ {
            continue;
        }
        if !strategy.is_empty() && s(item, "backup_strategy") != strategy {
            continue;
        }
        if awesome_only && !truthy(item, "is_awesome") {
            continue;
        }
        if out.len() >= limit {
            truncated = true;
            break;
        }
        out.push(brief_item(item, &aliases));
    }

    Ok(json!({
        "count": out.len(),
        "truncated": truncated,
        "software": out,
    }))
}

fn tool_get_software(args: &Value) -> Result<Value, String> {
    let items = store::read_software();
    let aliases = machine_aliases();
    let uuid = get_str(args, "uuid");
    let name = get_str(args, "name");

    if uuid.trim().is_empty() && name.trim().is_empty() {
        return Err("必须提供 uuid 或 name 之一".to_string());
    }

    let matched: Vec<&Value> = if !uuid.trim().is_empty() {
        items.iter().filter(|it| s(it, "uuid") == uuid).collect()
    } else {
        let n = name.to_lowercase();
        items
            .iter()
            .filter(|it| s(it, "name").to_lowercase().contains(&n))
            .collect()
    };

    if matched.is_empty() {
        return Err(format!(
            "没有找到匹配的软件（uuid={:?}, name={:?}），可用 search_software 先搜索",
            uuid, name
        ));
    }

    let software: Vec<Value> = matched.iter().map(|it| full_item(it, &aliases)).collect();
    Ok(json!({ "count": software.len(), "software": software }))
}

fn tool_list_vault_files(args: &Value) -> Result<Value, String> {
    let uuid = get_str(args, "uuid");
    if uuid.trim().is_empty() {
        return Err("必须提供 uuid".to_string());
    }
    let kind = {
        let k = get_str(args, "kind");
        if k.is_empty() {
            "soft".to_string()
        } else {
            k
        }
    };
    Ok(json!({
        "uuid": uuid,
        "kind": kind,
        "files": vault_files(&kind, &uuid),
    }))
}

/// 证据目录的绝对路径（`data/evidence/<机器目录名>`）。
fn evidence_abs(dir: &str) -> String {
    store::evidence_dir().join(dir).to_string_lossy().to_string()
}

/// 把 provider 恢复命令里的 `{{EVIDENCE}}` 占位符展开成真实绝对路径。
/// 前端（dev_env.js）也会做同样的事；MCP 必须自己做，否则 agent 拿到的是没法用的占位符。
fn expand_evidence_placeholders(providers: &mut Value, base: &str) {
    let Some(arr) = providers.as_array_mut() else { return };
    for p in arr.iter_mut() {
        let Some(cmds) = p.get_mut("restore_commands").and_then(|c| c.as_array_mut()) else {
            continue;
        };
        for c in cmds.iter_mut() {
            if let Some(txt) = c.as_str() {
                *c = json!(txt.replace("{{EVIDENCE}}", base));
            }
        }
    }
}

/// 只允许单层文件名（拒绝路径分隔符与 `..`）。
fn safe_file_name(name: &str) -> Option<String> {
    let p = std::path::Path::new(name);
    if p.components().count() != 1 {
        return None;
    }
    match p.file_name().and_then(|n| n.to_str()) {
        Some(n) if !n.is_empty() && n != "." && n != ".." => Some(n.to_string()),
        _ => None,
    }
}

fn find_dev_env_dir(machine: &str) -> Option<String> {
    let all = crate::commands::get_dev_env();
    let mf = machine.trim().to_lowercase();
    all.get("machines")
        .and_then(|m| m.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|m| {
                let mid = s(m, "machine_id").to_lowercase();
                let dir = s(m, "dir").to_lowercase();
                if mid == mf || dir == mf || mid.contains(&mf) || dir.contains(&mf) {
                    Some(s(m, "dir").to_string())
                } else {
                    None
                }
            })
        })
}

fn tool_list_dev_env(args: &Value) -> Result<Value, String> {
    let mut all = crate::commands::get_dev_env();
    if let Some(arr) = all.get_mut("machines").and_then(|m| m.as_array_mut()) {
        for m in arr.iter_mut() {
            let dir = s(m, "dir").to_string();
            let base = evidence_abs(&dir);
            if let Some(o) = m.as_object_mut() {
                o.insert("evidence_dir".into(), json!(base.clone()));
            }
            if let Some(providers) = m.get_mut("providers") {
                expand_evidence_placeholders(providers, &base);
            }
        }
    }

    let filter = get_str(args, "machine").to_lowercase();
    if filter.trim().is_empty() {
        return Ok(all);
    }
    let machines = all
        .get("machines")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let filtered: Vec<Value> = machines
        .into_iter()
        .filter(|m| {
            s(m, "machine_id").to_lowercase().contains(&filter)
                || s(m, "dir").to_lowercase().contains(&filter)
        })
        .collect();
    Ok(json!({ "machines": filtered }))
}

/// 读取 `data/evidence/<机器>/dev-env/<文件>` 的原文。只读、且只允许该目录下的单层文件。
fn tool_get_dev_env_file(args: &Value) -> Result<Value, String> {
    let machine = get_str(args, "machine");
    let file = get_str(args, "file");
    if machine.trim().is_empty() || file.trim().is_empty() {
        return Err("必须提供 machine 与 file".to_string());
    }
    let Some(file_name) = safe_file_name(&file) else {
        return Err(format!("非法文件名: {file}"));
    };
    let dir = find_dev_env_dir(&machine).ok_or_else(|| format!("找不到机器: {machine}"))?;

    let root = store::evidence_dir().join(&dir).join("dev-env");
    let path = root.join(&file_name);
    if !path.starts_with(&root) {
        return Err("非法路径".to_string());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取失败（{}）：{}", path.display(), e))?;

    // 防意外拉爆上下文：超过 512KB 截断。
    const MAX: usize = 512 * 1024;
    let (content, truncated) = if raw.len() > MAX {
        (raw.chars().take(MAX).collect::<String>(), true)
    } else {
        (raw, false)
    };

    Ok(json!({
        "machine": dir,
        "file": file_name,
        "truncated": truncated,
        "content": content,
    }))
}

fn tool_list_browser_extensions(args: &Value) -> Result<Value, String> {
    let all = crate::commands::get_browser_extensions_readonly();
    let machine_filter = get_str(args, "machine").to_lowercase();
    let browser_filter = get_str(args, "browser").to_lowercase();

    let machines = all
        .get("machines")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    let mut filtered: Vec<Value> = Vec::new();
    for m in machines {
        if !machine_filter.is_empty() {
            let mid = s(&m, "machine_id").to_lowercase();
            let dir = s(&m, "dir").to_lowercase();
            if !mid.contains(&machine_filter) && !dir.contains(&machine_filter) {
                continue;
            }
        }
        let mut entry = m.clone();
        if !browser_filter.is_empty() {
            if let Some(obj) = entry.as_object_mut() {
                if let Some(bs) = obj.get_mut("browsers").and_then(|b| b.as_array_mut()) {
                    bs.retain(|b| {
                        let id = s(b, "id").to_lowercase();
                        let label = s(b, "label").to_lowercase();
                        id.contains(&browser_filter) || label.contains(&browser_filter)
                    });
                }
            }
        }
        filtered.push(entry);
    }
    Ok(json!({ "machines": filtered }))
}

fn tool_get_recovery_doc(args: &Value) -> Result<Value, String> {
    let docs = crate::exporter::export_checklists();
    if !truthy(&docs, "success") {
        return Err(s(&docs, "message").to_string());
    }
    let which = get_str(args, "which");
    let (filename, content) = match which.as_str() {
        "awesome" => ("AWESOME_LIST.md", s(&docs, "awesomeContent")),
        "" | "checklist" => ("RECOVERY_CHECKLIST.md", s(&docs, "checklistContent")),
        other => return Err(format!("which 只能是 checklist 或 awesome，收到 {other}")),
    };
    Ok(json!({ "filename": filename, "markdown": content }))
}

// ---------------------------------------------------------------------------
// resources
// ---------------------------------------------------------------------------

fn resources_list() -> Value {
    json!({
        "resources": [
            { "uri": "ledger://machines", "name": "机器与统计", "description": "所有机器、别名、软件数与当前机器", "mimeType": "application/json" },
            { "uri": "ledger://software", "name": "完整软件台账", "description": "全部软件的精简条目列表", "mimeType": "application/json" },
            { "uri": "ledger://checklist", "name": "重装恢复清单", "description": "RECOVERY_CHECKLIST.md 原文", "mimeType": "text/markdown" },
            { "uri": "ledger://awesome", "name": "精选软件资产库", "description": "AWESOME_LIST.md 原文", "mimeType": "text/markdown" },
            { "uri": "ledger://dev-env", "name": "开发环境清单", "description": "各机器的开发环境与包列表", "mimeType": "application/json" }
        ]
    })
}

fn read_resource(params: &Value) -> Result<Value, (i64, String)> {
    let uri = get_str(params, "uri");
    let (mime, text) = match uri.as_str() {
        "ledger://machines" => {
            let machines = tool_list_machines().unwrap_or_else(|_| json!({}));
            ("application/json", pretty(&machines))
        }
        "ledger://software" => {
            let aliases = machine_aliases();
            let list: Vec<Value> = store::read_software().iter().map(|i| brief_item(i, &aliases)).collect();
            ("application/json", pretty(&Value::Array(list)))
        }
        "ledger://checklist" => ("text/markdown", doc_text("checklist")),
        "ledger://awesome" => ("text/markdown", doc_text("awesome")),
        "ledger://dev-env" => ("application/json", pretty(&crate::commands::get_dev_env())),
        other => return Err((-32602, format!("Unknown resource: {other}"))),
    };
    Ok(json!({
        "contents": [{ "uri": uri, "mimeType": mime, "text": text }]
    }))
}

fn doc_text(which: &str) -> String {
    let docs = crate::exporter::export_checklists();
    let key = if which == "awesome" {
        "awesomeContent"
    } else {
        "checklistContent"
    };
    s(&docs, key).to_string()
}

// ---------------------------------------------------------------------------
// 写入工具（需在设置里开启「允许 AI agent 写入台账」，且本机当前可写）
// ---------------------------------------------------------------------------

/// 允许 agent 改写的字段白名单：身份（id / uuid）与机器分布等结构字段不可改，
/// 否则 agent 很容易把台账结构搞坏。处置方式（backup_strategy）仍由 derive 逻辑辅助推导。
const WRITABLE_FIELDS: &[&str] = &[
    "name",
    "category",
    "type",
    "version",
    "restore_intent",
    "backup_strategy",
    "prep_status",
    "has_config",
    "download_url",
    "config_notes",
    "is_awesome",
    "awesome_role",
];

/// 写入准入：设置里开了写开关。
fn write_gate() -> Result<(), String> {
    let enabled = store::get_config()
        .get("mcp_write_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !enabled {
        return Err(
            "MCP 写入未启用：请在软件「设置 → Agent 集成 (MCP)」勾选「允许 AI agent 写入台账」后重试（只读工具不受影响）。"
                .to_string(),
        );
    }
    Ok(())
}

/// 只保留白名单字段；其余（含 uuid / id / machines 等结构字段）静默丢弃。
fn filter_updates(raw: &Value) -> Value {
    let mut out = serde_json::Map::new();
    if let Value::Object(obj) = raw {
        for (k, v) in obj {
            if WRITABLE_FIELDS.contains(&k.as_str()) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    Value::Object(out)
}

fn uuid_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn empty_updates_err() -> String {
    format!(
        "updates 为空或没有可写字段；允许的字段：{}",
        WRITABLE_FIELDS.join(" / ")
    )
}

fn tool_update_software(args: &Value) -> Result<Value, String> {
    write_gate()?;
    let uuid = get_str(args, "uuid");
    if uuid.is_empty() {
        return Err("缺少 uuid 参数".to_string());
    }
    let updates = filter_updates(args.get("updates").unwrap_or(&json!({})));
    if updates.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err(empty_updates_err());
    }
    let res = crate::commands::update_software(json!({ "uuid": uuid, "updates": updates }));
    if res.get("success").and_then(|v| v.as_bool()) == Some(true) {
        store::append_audit("update_software", &json!({ "uuid": uuid, "updates": updates }));
    }
    Ok(res)
}

fn tool_batch_update(args: &Value) -> Result<Value, String> {
    write_gate()?;
    let ids = uuid_list(args, "uuids");
    if ids.is_empty() {
        return Err("uuids 不能为空".to_string());
    }
    let updates = filter_updates(args.get("updates").unwrap_or(&json!({})));
    if updates.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err(empty_updates_err());
    }
    let res = crate::commands::batch_update(json!({ "ids": ids, "updates": updates }));
    if res.get("success").and_then(|v| v.as_bool()) == Some(true) {
        store::append_audit(
            "batch_update",
            &json!({ "uuids": ids, "updates": updates, "count": res.get("count") }),
        );
    }
    Ok(res)
}

fn tool_add_software(args: &Value) -> Result<Value, String> {
    write_gate()?;
    let names: Vec<String> = args
        .get("names")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|y| y.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return Err("names 不能为空".to_string());
    }
    let mut intent = get_str(args, "restore_intent");
    if intent.is_empty() {
        intent = "must".to_string();
    }
    let res = crate::commands::batch_add(json!({ "names": names, "restore_intent": intent }));
    if res.get("success").and_then(|v| v.as_bool()) == Some(true) {
        store::append_audit(
            "add_software",
            &json!({ "names": names, "restore_intent": intent }),
        );
    }
    Ok(res)
}

fn tool_delete_software(args: &Value) -> Result<Value, String> {
    write_gate()?;
    let ids = uuid_list(args, "uuids");
    if ids.is_empty() {
        return Err("uuids 不能为空".to_string());
    }
    let res = crate::commands::delete_software(json!({ "ids": ids }));
    if res.get("success").and_then(|v| v.as_bool()) == Some(true) {
        store::append_audit("delete_software", &json!({ "uuids": ids }));
    }
    Ok(res)
}

fn tool_merge_software(args: &Value) -> Result<Value, String> {
    write_gate()?;
    let target = get_str(args, "target_uuid");
    let merges = uuid_list(args, "merge_uuids");
    if target.is_empty() {
        return Err("缺少 target_uuid 参数".to_string());
    }
    if merges.is_empty() {
        return Err("merge_uuids 不能为空".to_string());
    }
    let res =
        crate::commands::merge_software(json!({ "targetUuid": target, "mergeUuids": merges }));
    if res.get("success").and_then(|v| v.as_bool()) == Some(true) {
        store::append_audit(
            "merge_software",
            &json!({ "target_uuid": target, "merge_uuids": merges }),
        );
    }
    Ok(res)
}

// ---------------------------------------------------------------------------
// prompts
// ---------------------------------------------------------------------------

fn prompts_list() -> Value {
    json!({
        "prompts": [
            {
                "name": "reinstall_plan",
                "description": "引导 agent 用只读信息为某台机器整理一份重装方案（不自动执行）。",
                "arguments": [
                    { "name": "machine", "description": "目标机器（主机名或别名），留空表示当前机器", "required": false }
                ]
            }
        ]
    })
}

fn get_prompt(params: &Value) -> Result<Value, (i64, String)> {
    let name = get_str(params, "name");
    if name != "reinstall_plan" {
        return Err((-32602, format!("Unknown prompt: {name}")));
    }
    let machine = params
        .get("arguments")
        .and_then(|a| a.get("machine"))
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let target = if machine.is_empty() {
        "当前机器".to_string()
    } else {
        format!("机器「{machine}」")
    };

    let text = format!(
        "请借助软件备份台账的 MCP 工具，为{target}整理一份重装方案。要求：\n\
         1. 先调用 list_machines 了解有哪些机器和当前机器；\n\
         2. 用 search_software（必要时配合 not_on_machine / intent / type 等过滤）找出该机器上恢复意愿为 must 的软件，以及别的机器上装了但该机器没有的软件；\n\
         3. 对每条重要软件调用 get_software，查看它的官网链接、配置备注，以及归档文件清单——如果归档里有安装包就直接用，没有再看 download_url，两者都没有就需要你上网搜索官方下载地址；\n\
         4. 用 list_dev_env 补充开发环境复现：逐条看各工具链的包清单与恢复命令，必要时用 get_dev_env_file 读取清单文件原文（跨机器重装时尤其需要）；再用 list_browser_extensions 补充浏览器扩展；\n\
         5. 把方案按「先装什么、怎么装、装完怎么恢复配置」整理清楚，逐条标注信息来源（台账归档 / 官网链接 / 需自行搜索）。\n\
         只做规划和信息整理，不要执行任何安装或下载动作；把方案交给我确认。"
    );

    Ok(json!({
        "description": format!("为{target}整理重装方案"),
        "messages": [
            { "role": "user", "content": { "type": "text", "text": text } }
        ]
    }))
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_echoes_client_protocol_version() {
        let msg = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }
        });
        let resp = dispatch(&msg).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notifications_are_not_answered() {
        let msg = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(dispatch(&msg).is_none());
    }

    #[test]
    fn tools_list_has_core_tools() {
        let msg = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let resp = dispatch(&msg).unwrap();
        let names: Vec<String> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| s(t, "name").to_string())
            .collect();
        for want in [
            "list_machines",
            "search_software",
            "get_software",
            "list_dev_env",
            "get_dev_env_file",
            "update_software",
            "add_software",
            "delete_software",
            "merge_software",
        ] {
            assert!(names.iter().any(|n| n == want), "缺少工具 {want}");
        }
    }

    #[test]
    fn unknown_method_is_an_error() {
        let msg = json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" });
        let resp = dispatch(&msg).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn get_software_requires_selector() {
        let params = json!({ "name": "get_software", "arguments": {} });
        let resp = call_tool(&params).unwrap();
        assert_eq!(resp["isError"], true);
    }

    #[test]
    fn write_tools_are_gated_by_setting() {
        let base = std::env::temp_dir().join(format!("ledger_mcp_gate_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("data")).unwrap();
        {
            let _guard = store::test_support::use_root(&base);
            // 默认未开启写开关 → 所有写入工具都应被拒绝
            let err = tool_update_software(&json!({ "uuid": "x", "updates": { "version": "1" } }))
                .unwrap_err();
            assert!(err.contains("未启用"), "got: {err}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn update_software_writes_and_audits_when_enabled() {
        let base = std::env::temp_dir().join(format!("ledger_mcp_write_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("data")).unwrap();
        {
            let _guard = store::test_support::use_root(&base);
            // 开启写开关；写工具是否允许完全由该开关决定。
            store::save_config(&json!({ "mcp_write_enabled": true }));
            let uuid = "11111111-1111-1111-1111-111111111111";
            store::write_software(&[json!({
                "id": "SW-001", "uuid": uuid, "name": "Zed", "version": "1.0",
                "machines": [], "restore_intent": "must"
            })]);

            // 白名单外的结构字段（uuid / machines）应被丢弃
            let res = tool_update_software(&json!({
                "uuid": uuid,
                "updates": { "version": "2.0", "uuid": "HACK", "machines": [] }
            }))
            .unwrap();
            assert_eq!(res["success"], json!(true));
            assert_eq!(res["item"]["version"], json!("2.0"));
            assert_eq!(res["item"]["uuid"], json!(uuid));

            // 写入已落盘 + 审计日志已追加
            assert_eq!(store::read_software()[0]["version"], json!("2.0"));
            assert!(store::audit_file().is_file());
            let log = std::fs::read_to_string(store::audit_file()).unwrap();
            assert!(log.contains("update_software"));
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
