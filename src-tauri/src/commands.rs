use serde_json::{json, Value};
use std::process::Command;
use std::time::Duration;

use crate::exporter::export_checklists;
use crate::ingest::run_ingest;
use crate::store;

fn as_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_config() -> Value {
    store::get_config()
}

#[tauri::command]
pub fn save_config(payload: Value) -> Value {
    let mut current = store::get_config();
    if let (Value::Object(base), Value::Object(over)) = (&mut current, &payload) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    store::save_config(&current);
    json!({ "success": true, "config": current })
}

// ---------------------------------------------------------------------------
// 状态汇总
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_status() -> Value {
    let software = store::read_software();

    let mut machines: Vec<String> = Vec::new();
    let push_machine = |id: &str, list: &mut Vec<String>| {
        if !id.is_empty() && !list.iter().any(|m| m == id) {
            list.push(id.to_string());
        }
    };

    if let Ok(rd) = std::fs::read_dir(store::evidence_dir()) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                push_machine(&e.file_name().to_string_lossy(), &mut machines);
            }
        }
    }
    for item in &software {
        if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
            for m in ms {
                push_machine(&as_str(m, "machine_id"), &mut machines);
            }
        }
    }
    let cfg = store::get_config();
    let aliases = cfg.get("machine_aliases").cloned().unwrap_or(json!({}));
    if let Some(obj) = aliases.as_object() {
        for k in obj.keys() {
            push_machine(k, &mut machines);
        }
    }

    let mut path_set: Vec<String> = Vec::new();
    let add_path = |p: &str, set: &mut Vec<String>| {
        if p.is_empty() {
            return;
        }
        let bucket = if p.contains("\\AppData\\Local") {
            Some("AppData\\Local")
        } else if p.contains("\\AppData\\Roaming") {
            Some("AppData\\Roaming")
        } else if p.to_lowercase().starts_with("c:\\program files (x86)") {
            Some("C:\\Program Files (x86)")
        } else if p.to_lowercase().starts_with("c:\\program files") {
            Some("C:\\Program Files")
        } else if p.to_lowercase().starts_with("c:\\software") {
            Some("C:\\Software")
        } else if p.starts_with("D:\\") {
            Some("D:\\")
        } else if p.starts_with("E:\\") {
            Some("E:\\")
        } else if p.starts_with("C:\\") {
            Some("C:\\")
        } else {
            None
        };
        if let Some(b) = bucket {
            if !set.iter().any(|x| x == b) {
                set.push(b.to_string());
            }
        }
    };
    for item in &software {
        if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
            for m in ms {
                let install = as_str(m, "install_location");
                let p = if install.is_empty() { as_str(m, "path") } else { install };
                add_path(&p, &mut path_set);
            }
        }
    }
    let mut available_paths = path_set;
    available_paths.sort();

    let count = |pred: &dyn Fn(&Value) -> bool| software.iter().filter(|s| pred(s)).count();
    let stats = json!({
        "total": software.len(),
        "must": count(&|s| as_str(s, "restore_intent") == "must"),
        "should": count(&|s| as_str(s, "restore_intent") == "should"),
        "on_demand": count(&|s| as_str(s, "restore_intent") == "on_demand"),
        "drop": count(&|s| as_str(s, "restore_intent") == "drop"),
        "unreviewed": count(&|s| {
            let i = as_str(s, "restore_intent");
            i.is_empty() || i == "unreviewed"
        }),
        "awesome": count(&|s| s.get("is_awesome").and_then(|v| v.as_bool()).unwrap_or(false)),
        "backupTasks": count(&|s| {
            let st = as_str(s, "backup_strategy");
            st == "copy_dir" || st == "copy_config"
        }),
        "ready": count(&|s| as_str(s, "prep_status") == "ready")
    });

    json!({
        "machines": machines,
        "stats": stats,
        "availablePaths": available_paths,
        "machine_aliases": aliases
    })
}

// ---------------------------------------------------------------------------
// 软件清单 CRUD
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_software() -> Value {
    Value::Array(store::read_software())
}

#[tauri::command]
pub fn update_software(payload: Value) -> Value {
    let id = as_str(&payload, "id");
    let updates = payload.get("updates").cloned().unwrap_or(json!({}));
    let mut software = store::read_software();

    let Some(idx) = software
        .iter()
        .position(|s| as_str(s, "id") == id)
    else {
        return json!({ "success": false, "message": "Item not found" });
    };

    if let (Value::Object(item), Value::Object(upd)) = (&mut software[idx], &updates) {
        for (k, v) in upd {
            item.insert(k.clone(), v.clone());
        }
    }
    let item = software[idx].clone();
    store::write_software(&software);
    json!({ "success": true, "item": item })
}

#[tauri::command]
pub fn batch_update(payload: Value) -> Value {
    let ids: Vec<String> = payload
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let updates = payload.get("updates").cloned().unwrap_or(json!({}));

    let mut software = store::read_software();
    let mut count = 0;
    for item in software.iter_mut() {
        if ids.contains(&as_str(item, "id")) {
            if let (Value::Object(obj), Value::Object(upd)) = (item, &updates) {
                for (k, v) in upd {
                    obj.insert(k.clone(), v.clone());
                }
            }
            count += 1;
        }
    }
    store::write_software(&software);
    json!({ "success": true, "count": count })
}

#[tauri::command]
pub fn delete_software(payload: Value) -> Value {
    let ids: Vec<String> = payload
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let mut software = store::read_software();
    software.retain(|s| !ids.contains(&as_str(s, "id")));
    store::write_software(&software);
    json!({ "success": true, "remaining": software.len() })
}

#[tauri::command]
pub fn batch_add(payload: Value) -> Value {
    let names: Vec<String> = payload
        .get("names")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let restore_intent = payload
        .get("restore_intent")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("must")
        .to_string();

    let mut software = store::read_software();
    let mut current_id: u64 = software
        .iter()
        .filter_map(|s| {
            as_str(s, "id")
                .replace("SW-", "")
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0);

    let mut new_items: Vec<Value> = Vec::new();
    for raw_name in names {
        let clean = raw_name.trim();
        if clean.is_empty() {
            continue;
        }
        current_id += 1;
        let item = json!({
            "id": format!("SW-{:03}", current_id),
            "name": clean,
            "version": "",
            "category": "系统工具",
            "type": "desktop",
            "machines": [{ "machine_id": "DESKTOP-HEGVCTR", "form": "manual" }],
            "restore_intent": restore_intent,
            "backup_strategy": "none",
            "prep_status": "todo",
            "has_config": false,
            "download_url": "",
            "config_notes": "",
            "is_awesome": false,
            "awesome_role": "",
            "is_new": true,
            "created_at": chrono::Local::now().to_rfc3339()
        });
        new_items.push(item.clone());
        software.insert(0, item);
    }

    store::write_software(&software);
    json!({ "success": true, "items": new_items })
}

#[tauri::command]
pub fn merge_software(payload: Value) -> Value {
    let target_id = as_str(&payload, "targetId");
    let merge_ids: Vec<String> = payload
        .get("mergeIds")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let mut software = store::read_software();
    let Some(target_idx) = software.iter().position(|s| as_str(s, "id") == target_id) else {
        return json!({ "success": false, "message": "Target not found" });
    };

    // 收集需要合并的机器与字段
    let merge_items: Vec<Value> = software
        .iter()
        .filter(|s| merge_ids.contains(&as_str(s, "id")))
        .cloned()
        .collect();

    for item in &merge_items {
        if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
            for m in ms {
                let already = software[target_idx]
                    .get("machines")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter().any(|tm| {
                            as_str(tm, "machine_id") == as_str(m, "machine_id")
                                && as_str(tm, "install_location") == as_str(m, "install_location")
                        })
                    })
                    .unwrap_or(false);
                if !already {
                    if let Some(arr) = software[target_idx]
                        .get_mut("machines")
                        .and_then(|x| x.as_array_mut())
                    {
                        arr.push(m.clone());
                    }
                }
            }
        }
        // 补齐字段
        let target = &mut software[target_idx];
        if as_str(target, "version").is_empty() && !as_str(item, "version").is_empty() {
            target["version"] = item["version"].clone();
        }
        if as_str(target, "download_url").is_empty() && !as_str(item, "download_url").is_empty() {
            target["download_url"] = item["download_url"].clone();
        }
        if item.get("has_config").and_then(|v| v.as_bool()).unwrap_or(false) {
            target["has_config"] = json!(true);
        }
        let notes = as_str(item, "config_notes");
        if !notes.is_empty() {
            let existing = as_str(target, "config_notes");
            let merged = if existing.is_empty() {
                notes
            } else {
                format!("{}; {}", existing, notes)
            };
            target["config_notes"] = json!(merged);
        }
    }

    software.retain(|s| !merge_ids.contains(&as_str(s, "id")));
    let target = software
        .iter()
        .find(|s| as_str(s, "id") == target_id)
        .cloned()
        .unwrap_or(Value::Null);
    store::write_software(&software);
    json!({ "success": true, "target": target })
}

// ---------------------------------------------------------------------------
// LLM 辅助分析
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn llm_analyze(payload: Value) -> Value {
    let cfg = store::get_config();
    let url = as_str(&cfg, "llm_url").trim().to_string();
    if url.is_empty() {
        return json!({ "success": false, "error": "未配置 LLM 地址" });
    }
    let model = as_str(&cfg, "llm_model");
    let key = as_str(&cfg, "llm_api_key").trim().to_string();

    let mut endpoint = url.trim_end_matches('/').to_string();
    if !endpoint.contains("/chat/completions") {
        endpoint.push_str("/chat/completions");
    }

    let name = as_str(&payload, "name");
    let category = as_str(&payload, "category");
    let paths = payload
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let paths = if paths.is_empty() { "未记录路径".to_string() } else { paths };
    let category_text = if category.is_empty() { "未知".to_string() } else { category };

    let prompt = format!(
        r#"你是一个 Windows 软件与系统重装迁移专家。请根据给出的软件名称与路径线索，分析并返回标准的分类与配置建议。
软件名称：{name}
已记录路径：{paths}
当前分类：{category_text}

请输出严格的 JSON 格式（不要输出任何多余的 Markdown 或前后缀，只返回一个标准 JSON 对象）：
{{
  "category": "开发工具",
  "type": "desktop",
  "restore_intent": "must",
  "backup_strategy": "redownload",
  "download_url": "https://...",
  "config_notes": "配置位置说明与迁移备忘"
}}

枚举约束说明：
- category: 必须从 [开发工具, 系统工具, 浏览器与网络, 媒体娱乐, 办公与笔记, 通讯与社交, 其他] 中选一个
- type: 必须从 [desktop, portable, cli, runtime] 中选一个
- restore_intent: 必须从 [must, should, on_demand, drop] 中选一个
- backup_strategy: 必须从 [copy_dir, copy_config, redownload, sync_account, none] 中选一个
- download_url: 软件官网或可靠下载页
- config_notes: 简要说明配置文件通常存放在何处（如 AppData、~/.config 或安装目录），或者是否依赖云同步
"#
    );

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(18))
        .build()
    {
        Ok(c) => c,
        Err(e) => return json!({ "success": false, "error": e.to_string() }),
    };

    let mut req = client.post(&endpoint).json(&json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.1
    }));
    if !key.is_empty() {
        req = req.bearer_auth(&key);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return json!({ "success": false, "error": format!("无法连接 LLM [{}]: {}", url, e) })
        }
    };
    let status = resp.status();
    if !status.is_success() {
        return json!({ "success": false, "error": format!("LLM 服务返回状态码: {}", status.as_u16()) });
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return json!({ "success": false, "error": e.to_string() }),
    };
    let raw = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let cleaned = raw.replace("```json", "").replace("```", "").trim().to_string();
    match serde_json::from_str::<Value>(&cleaned) {
        Ok(parsed) => json!({ "success": true, "suggestion": parsed, "id": as_str(&payload, "id") }),
        Err(e) => json!({ "success": false, "error": format!("解析 LLM 输出失败: {}", e), "raw": raw }),
    }
}

// ---------------------------------------------------------------------------
// 扫描本机 + 导出
// ---------------------------------------------------------------------------

/// 采集脚本直接编译进 exe，分发时无需再带 scripts/ 目录。
const COLLECT_PS1: &str = include_str!("../../scripts/collect.ps1");

fn run_powershell(
    script: &std::path::Path,
    output_dir: &std::path::Path,
    custom_dirs: &str,
) -> Result<String, String> {
    let mut last_err = String::new();
    for exe in ["pwsh", "powershell"] {
        let mut cmd = Command::new(exe);
        cmd.arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script)
            .arg("-OutputDir")
            .arg(output_dir);
        if !custom_dirs.is_empty() {
            cmd.arg("-CustomPortableDirs").arg(custom_dirs);
        }
        cmd.current_dir(store::app_root());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        match cmd.output() {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                if out.status.success() {
                    return Ok(stdout);
                }
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                last_err = format!("{} 退出码 {:?}\n{}", exe, out.status.code(), stderr);
            }
            Err(e) => last_err = format!("无法启动 {}: {}", exe, e),
        }
    }
    Err(last_err)
}

#[tauri::command]
pub async fn scan_local() -> Value {
    let result = tauri::async_runtime::spawn_blocking(|| {
        // 释放内置采集脚本到临时文件；带 UTF-8 BOM 以兼容 PowerShell 5.1
        let script = std::env::temp_dir().join("software-ledger-collect.ps1");
        let body = format!("\u{feff}{}", COLLECT_PS1.trim_start_matches('\u{feff}'));
        if let Err(e) = std::fs::write(&script, body) {
            return json!({ "success": false, "error": format!("无法释放采集脚本: {}", e) });
        }

        let machine = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "UNKNOWN".to_string());
        let output_dir = store::evidence_dir().join(&machine);

        let cfg = store::get_config();
        let dirs = cfg
            .get("scan_directories")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(","))
            .unwrap_or_default();

        let res = match run_powershell(&script, &output_dir, &dirs) {
            Ok(output) => {
                let ingest = run_ingest();
                json!({ "success": true, "output": output, "ingestRes": ingest })
            }
            Err(err) => json!({ "success": false, "error": err }),
        };

        let _ = std::fs::remove_file(&script);
        res
    })
    .await;

    match result {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

#[tauri::command]
pub fn export_markdown() -> Value {
    export_checklists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn crud_roundtrip() {
        let root = std::env::temp_dir().join(format!("ledger_cmd_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data")).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let seed = json!([
            { "id": "SW-001", "name": "A", "restore_intent": "must", "machines": [{ "machine_id": "M1", "install_location": "C:\\A" }], "backup_strategy": "none", "prep_status": "todo" },
            { "id": "SW-002", "name": "B", "restore_intent": "unreviewed", "version": "1.0", "machines": [{ "machine_id": "M2", "install_location": "D:\\B" }], "backup_strategy": "copy_dir", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());

        assert_eq!(get_software().as_array().unwrap().len(), 2);

        let r = update_software(json!({ "id": "SW-001", "updates": { "restore_intent": "should", "has_config": true } }));
        assert_eq!(r.get("success").and_then(|v| v.as_bool()), Some(true));
        let updated = get_software();
        let a = updated
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "SW-001")
            .unwrap();
        assert_eq!(a["restore_intent"], "should");
        assert_eq!(a["has_config"], true);

        let r = batch_update(json!({ "ids": ["SW-001", "SW-002"], "updates": { "prep_status": "ready" } }));
        assert_eq!(r["count"], 2);

        let r = merge_software(json!({ "targetId": "SW-001", "mergeIds": ["SW-002"] }));
        assert_eq!(r.get("success").and_then(|v| v.as_bool()), Some(true));
        let merged = get_software();
        let arr = merged.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "SW-001");
        assert_eq!(arr[0]["version"], "1.0");
        assert_eq!(arr[0]["has_config"], true);
        assert_eq!(arr[0]["machines"].as_array().unwrap().len(), 2);

        let status = get_status();
        assert_eq!(status["stats"]["total"], 1);
        assert_eq!(status["stats"]["ready"], 1);

        let added = batch_add(json!({ "names": ["C", "D"], "restore_intent": "on_demand" }));
        assert_eq!(added["items"].as_array().unwrap().len(), 2);
        let after = get_software();
        assert_eq!(after.as_array().unwrap().len(), 3);
        assert_eq!(after[0]["name"], "D");

        let del = delete_software(json!({ "ids": ["SW-001"] }));
        assert_eq!(del["remaining"], 2);

        let ex = export_markdown();
        assert_eq!(ex.get("success").and_then(|v| v.as_bool()), Some(true));

        let cfg = save_config(json!({ "llm_model": "test-model" }));
        assert_eq!(cfg["config"]["llm_model"], "test-model");
        assert_eq!(get_config()["llm_model"], "test-model");

        let _ = fs::remove_dir_all(&root);
    }
}
