use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use base64::Engine;

use crate::exporter::export_checklists;
use crate::ingest::{apply_selected, build_candidates, known_icon_refreshes};
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

    for item in &software {
        if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
            for m in ms {
                push_machine(&as_str(m, "machine_id"), &mut machines);
            }
        }
    }
    // 机器列表只来自真实软件条目，不再把别名表里的 key 也算作一台机器，
    // 否则复制过来的 data/config.json 会把别的机器上的旧主机名显示成标签页。
    let cfg = store::get_config();
    let aliases = cfg.get("machine_aliases").cloned().unwrap_or(json!({}));

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
    let mut items = store::read_software();
    // 图标以 data URI 注入（文件本体在 data/icons/<SW-ID>.png），
    // 方便 WebView 直接 <img src>；不写回 software.json。
    for item in items.iter_mut() {
        let id = as_str(item, "id");
        if let Some(uri) = icon_data_uri(&id) {
            if let Some(obj) = item.as_object_mut() {
                obj.insert("icon".to_string(), json!(uri));
            }
        }
    }
    Value::Array(items)
}

/// 读图标文件并编码成 data URI；缺失/读失败返回 None（前端回退通用图标）。
fn icon_data_uri(id: &str) -> Option<String> {
    if id.is_empty() {
        return None;
    }
    let path = store::icons_dir().join(format!("{}.png", safe_component(id)));
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 汇总各机器采集到的开发环境清单（只读证据，不参与软件清单）。
#[tauri::command]
pub fn get_dev_env() -> Value {
    let root = store::evidence_dir();
    let mut machines: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let file = path.join("dev-env.json");
            if !file.exists() {
                continue;
            }
            let data = store::read_json(&file);
            let providers = data.get("providers").cloned().unwrap_or(json!([]));
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let machine_id = as_str(&data, "machine_id");
            let machine_id = if machine_id.is_empty() {
                dir_name.clone()
            } else {
                machine_id
            };
            machines.push(json!({
                "machine_id": machine_id,
                "dir": dir_name,
                "collected_at": as_str(&data, "collected_at"),
                "providers": providers,
            }));
        }
    }
    machines.sort_by(|a, b| as_str(a, "machine_id").cmp(&as_str(b, "machine_id")));
    json!({ "success": true, "machines": machines })
}

// ---------------------------------------------------------------------------
// 浏览器扩展（只读元数据聚合）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_browser_extensions() -> Value {
    let root = store::evidence_dir();
    let user_map = store::read_extensions();
    let mut machines: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let file = path.join("browser-extensions.json");
            if !file.exists() {
                continue;
            }
            let data = store::read_json(&file);
            let mut browsers = data.get("browsers").cloned().unwrap_or(json!([]));
            // 注入用户层字段（备注 / 保留意愿 / 就绪 / 精选，按扩展 ID），重扫不丢失
            if let (Value::Array(bs), Value::Object(users)) = (&mut browsers, &user_map) {
                for b in bs.iter_mut() {
                    if let Some(profiles) = b.get_mut("profiles").and_then(|p| p.as_array_mut()) {
                        for p in profiles.iter_mut() {
                            if let Some(exts) = p.get_mut("extensions").and_then(|e| e.as_array_mut()) {
                                for ext in exts.iter_mut() {
                                    if let Some(obj) = ext.as_object_mut() {
                                        let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        if let Some(user) = users.get(&id).and_then(|v| v.as_object()) {
                                            for (k, v) in user {
                                                if k != "updated_at" {
                                                    obj.insert(k.clone(), v.clone());
                                                }
                                            }
                                        }
                                        obj.insert("vault_count".to_string(), json!(vault_file_count("ext", &id)));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let machine_id = as_str(&data, "machine_id");
            let machine_id = if machine_id.is_empty() {
                dir_name.clone()
            } else {
                machine_id
            };
            machines.push(json!({
                "machine_id": machine_id,
                "dir": dir_name,
                "collected_at": as_str(&data, "collected_at"),
                "browsers": browsers,
            }));
        }
    }
    machines.sort_by(|a, b| as_str(a, "machine_id").cmp(&as_str(b, "machine_id")));
    json!({ "success": true, "machines": machines, "vault_machine": current_machine() })
}

// ---------------------------------------------------------------------------
// 通用文件保管箱：data/vault/<机器>/<kind>/<id>/
// 只存用户手动放入的文件，绝不自动采集。
// ---------------------------------------------------------------------------

fn current_machine() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "UNKNOWN".to_string())
}

/// 把任意字符串收敛成安全的单层路径片段（去掉路径分隔符）。
fn safe_component(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            _ => c,
        })
        .collect();
    let t = cleaned.trim();
    if t.is_empty() || t == "." || t == ".." {
        "_".to_string()
    } else {
        t.to_string()
    }
}

fn vault_target_dir(kind: &str, id: &str) -> PathBuf {
    store::vault_dir()
        .join(current_machine())
        .join(safe_component(kind))
        .join(safe_component(id))
}

/// 统计某保管箱目录下的文件数量（用于表格上的附件角标）。
fn vault_file_count(kind: &str, id: &str) -> u64 {
    std::fs::read_dir(vault_target_dir(kind, id))
        .map(|rd| rd.flatten().filter(|e| e.path().is_file()).count() as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub fn vault_list(kind: String, id: String) -> Value {
    let dir = vault_target_dir(&kind, &id);
    let mut files: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let meta = entry.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Local>::from(t).to_rfc3339())
                .unwrap_or_default();
            files.push(json!({
                "name": entry.file_name().to_string_lossy().to_string(),
                "size": size,
                "modified": modified,
            }));
        }
    }
    files.sort_by(|a, b| as_str(a, "name").cmp(&as_str(b, "name")));
    let machine = current_machine();
    json!({ "success": true, "machine": machine, "files": files })
}

/// 保存某条扩展的用户层字段（data/extensions.json，按扩展 ID 持久化）。
/// 传入的字段会与已有字段合并（不覆盖未提及的键）。
#[tauri::command]
pub fn update_extension(id: String, fields: Value) -> Value {
    if id.is_empty() {
        return json!({ "success": false, "error": "缺少扩展 ID" });
    }
    let Value::Object(incoming) = fields else {
        return json!({ "success": false, "error": "字段格式错误" });
    };
    let mut map = store::read_extensions();
    let now = chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now()).to_rfc3339();
    {
        let Some(root) = map.as_object_mut() else {
            return json!({ "success": false, "error": "标注存储损坏" });
        };
        let entry = root.entry(id).or_insert_with(|| json!({}));
        let Some(eobj) = entry.as_object_mut() else {
            return json!({ "success": false, "error": "标注存储损坏" });
        };
        for (k, v) in incoming {
            if k != "updated_at" {
                eobj.insert(k, v);
            }
        }
        eobj.insert("updated_at".to_string(), json!(now));
    }
    store::write_extensions(&map);
    json!({ "success": true })
}

#[tauri::command]
pub fn vault_add(kind: String, id: String, paths: Vec<String>) -> Value {
    let dir = vault_target_dir(&kind, &id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return json!({ "success": false, "error": format!("无法创建归档目录: {}", e) });
    }
    let mut added = 0;
    let mut skipped = 0;
    for raw in &paths {
        let src = std::path::Path::new(raw);
        if !src.is_file() {
            skipped += 1;
            continue;
        }
        let Some(fname) = src.file_name().and_then(|n| n.to_str()) else {
            skipped += 1;
            continue;
        };
        let fname = safe_component(fname);
        if std::fs::copy(src, dir.join(&fname)).is_ok() {
            added += 1;
        } else {
            skipped += 1;
        }
    }
    json!({ "success": true, "added": added, "skipped": skipped })
}

#[tauri::command]
pub fn vault_delete(kind: String, id: String, name: String) -> Value {
    let dir = vault_target_dir(&kind, &id);
    let name = safe_component(&name);
    let target = dir.join(&name);
    if !target.starts_with(&dir) {
        return json!({ "success": false, "error": "非法文件名" });
    }
    match std::fs::remove_file(&target) {
        Ok(_) => json!({ "success": true }),
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

#[tauri::command]
pub fn vault_export(kind: String, id: String, name: String, dest: String) -> Value {
    let dir = vault_target_dir(&kind, &id);
    let name = safe_component(&name);
    let src = dir.join(&name);
    if !src.is_file() {
        return json!({ "success": false, "error": "归档文件不存在" });
    }
    let dest_path = PathBuf::from(&dest);
    let target = if dest_path.is_dir() { dest_path.join(&name) } else { dest_path };
    match std::fs::copy(&src, &target) {
        Ok(_) => json!({ "success": true, "path": target.to_string_lossy().to_string() }),
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
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
    let mut ignored = store::read_ignored();

    // 删除即为墓碑：记录规范化名称与路径，重扫时默认不勾选，避免垃圾复活。
    for id in &ids {
        let Some(item) = software.iter().find(|s| as_str(s, "id") == *id) else {
            continue;
        };
        let name = as_str(item, "name");
        let key = name.to_lowercase().trim().to_string();
        let mut paths: Vec<String> = Vec::new();
        if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
            for m in ms {
                let loc = {
                    let a = as_str(m, "install_location");
                    if a.is_empty() { as_str(m, "path") } else { a }
                };
                let p = crate::ingest::normalize_path(&loc);
                if !p.is_empty() && !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
        let deleted_at = chrono::Local::now().to_rfc3339();
        if let Some(existing) = ignored.iter_mut().find(|g| as_str(g, "match_key") == key) {
            existing["name"] = json!(name);
            existing["paths"] = json!(paths);
            existing["deleted_at"] = json!(deleted_at);
        } else {
            ignored.push(json!({
                "match_key": key,
                "name": name,
                "paths": paths,
                "deleted_at": deleted_at,
            }));
        }
    }

    software.retain(|s| !ids.contains(&as_str(s, "id")));
    // 级联清理该软件的配置归档与图标，不留孤儿文件
    for id in &ids {
        let _ = std::fs::remove_dir_all(vault_target_dir("soft", id));
        let _ = std::fs::remove_file(store::icons_dir().join(format!("{}.png", safe_component(id))));
    }
    store::write_software(&software);
    store::write_ignored(&ignored);
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

    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "UNKNOWN".to_string());

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
            "machines": [{ "machine_id": hostname.as_str(), "form": "manual" }],
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

/// 临时目录偶尔被杀毒/索引短暂占用，删除失败时重试几次。
fn remove_dir_retry(dir: &std::path::Path) {
    for _ in 0..5 {
        if std::fs::remove_dir_all(dir).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    eprintln!("[scan] 未能删除临时目录: {}", dir.display());
}

/// 采集本机证据到 `data/evidence/<主机名>/`，脚本本身仍在系统临时目录释放执行。
/// 仅覆盖证据 JSON，保留 `screenshots/`（用户手动放的截图）。
fn collect_evidence() -> Result<String, String> {
    let machine = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "UNKNOWN".to_string());
    let evidence_dir = store::evidence_dir().join(&machine);
    std::fs::create_dir_all(&evidence_dir).map_err(|e| format!("无法创建证据目录: {}", e))?;

    for name in [
        "registry-apps.json",
        "portable-apps.json",
        "shortcuts.json",
        "winget-apps.json",
        "scoop-apps.json",
        "cli-tools.json",
        "machine-info.json",
        "dev-env.json",
        "browser-extensions.json",
        "timings.json",
    ] {
        let _ = std::fs::remove_file(evidence_dir.join(name));
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let script_dir = std::env::temp_dir()
        .join(format!("windows-software-ledger-script-{}-{}", std::process::id(), stamp));
    std::fs::create_dir_all(&script_dir).map_err(|e| format!("无法创建临时目录: {}", e))?;
    let script = script_dir.join("collect.ps1");
    // 带 UTF-8 BOM，兼容 PowerShell 5.1
    let body = format!("\u{feff}{}", COLLECT_PS1.trim_start_matches('\u{feff}'));
    std::fs::write(&script, body).map_err(|e| format!("无法释放采集脚本: {}", e))?;

    let cfg = store::get_config();
    let dirs = cfg
        .get("scan_directories")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(","))
        .unwrap_or_default();

    let result = run_powershell(&script, &evidence_dir, &dirs);
    remove_dir_retry(&script_dir);
    result?;
    Ok(machine)
}

/// 待导入的扫描预览（临时文件，跨重启可用，commit 后删除）。
fn pending_scan_file() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("windows-software-ledger");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("pending-scan.json")
}

/// 重扫时为「已知（未变）」的已有条目补齐图标：读 pending 里的 knownIcons，
/// 缺则复制到 data/icons/<SW-ID>.png；失败不阻断导入。
fn apply_known_icons(pending: &Value) {
    let Some(arr) = pending.get("knownIcons").and_then(|v| v.as_array()) else {
        return;
    };
    let dir = store::icons_dir();
    let _ = std::fs::create_dir_all(&dir);
    for pair in arr {
        if let (Some(id), Some(src)) = (
            pair.get(0).and_then(|v| v.as_str()),
            pair.get(1).and_then(|v| v.as_str()),
        ) {
            let dest = dir.join(format!("{}.png", safe_component(id)));
            if !dest.exists() {
                let _ = std::fs::copy(src, &dest);
            }
        }
    }
}

fn summarize_candidates(candidates: &[Value]) -> Value {
    let count = |kind: &str| candidates.iter().filter(|c| as_str(c, "kind") == kind).count();
    json!({
        "total": candidates.len(),
        "new": count("new"),
        "deleted_before": count("deleted_before"),
    })
}

/// 扫描第一步：采集 + 解析候选，不写 software.json。
fn scan_preview_blocking() -> Value {
    let machine = match collect_evidence() {
        Ok(m) => m,
        Err(e) => return json!({ "success": false, "error": e }),
    };
    let existing = store::read_software();
    let ignored = store::read_ignored();
    let candidates = build_candidates(&store::evidence_dir(), &[machine.clone()], &existing, &ignored);
    let known_icons = known_icon_refreshes(&store::evidence_dir(), &[machine.clone()], &existing);
    let summary = summarize_candidates(&candidates);
    let pending = json!({
        "machine": machine,
        "createdAt": chrono::Local::now().to_rfc3339(),
        "candidates": candidates,
        "knownIcons": known_icons,
    });
    store::write_json(&pending_scan_file(), &pending);
    json!({
        "success": true,
        "machine": machine,
        "summary": summary,
        "was_empty": existing.is_empty(),
        "candidates": candidates
    })
}

#[tauri::command]
pub async fn scan_preview() -> Value {
    match tauri::async_runtime::spawn_blocking(scan_preview_blocking).await {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 扫描第二步：只把勾选的候选写入 software.json。
#[tauri::command]
pub fn scan_commit(payload: Value) -> Value {
    let selected_keys: Vec<String> = payload
        .get("selectedKeys")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let pending = store::read_json(&pending_scan_file());
    if !pending.is_object() {
        return json!({ "success": false, "error": "扫描预览已失效，请重新扫描" });
    }
    // 已有条目的图标补齐（与是否勾选新条目无关，缺则补）
    apply_known_icons(&pending);
    if selected_keys.is_empty() {
        let _ = std::fs::remove_file(pending_scan_file());
        return json!({ "success": true, "added": 0, "revived": 0 });
    }
    let candidates: Vec<Value> = pending
        .get("candidates")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut software = store::read_software();
    let was_empty = software.is_empty();
    let result = apply_selected(&mut software, &candidates, &selected_keys, !was_empty);
    store::write_software(&software);

    // 把本次导入候选的图标复制到 data/icons/<SW-ID>.png（仅导入项；失败不阻断导入）
    if !result.icons.is_empty() {
        let dir = store::icons_dir();
        let _ = std::fs::create_dir_all(&dir);
        for (id, src) in &result.icons {
            let dest = dir.join(format!("{}.png", safe_component(id)));
            let _ = std::fs::copy(src, &dest);
        }
    }

    if !result.revived_keys.is_empty() {
        let mut ignored = store::read_ignored();
        ignored.retain(|g| !result.revived_keys.iter().any(|k| as_str(g, "match_key") == *k));
        store::write_ignored(&ignored);
    }

    let _ = std::fs::remove_file(pending_scan_file());
    json!({ "success": true, "added": result.added, "revived": result.revived_keys.len() })
}

#[tauri::command]
pub fn export_markdown() -> Value {
    export_checklists()
}

/// 把导出清单写到用户选定的路径（原生另存为对话框返回的 dest）。
#[tauri::command]
pub fn export_save(which: String, dest: String) -> Value {
    let data = export_checklists();
    if data.get("success").and_then(|v| v.as_bool()) != Some(true) {
        let msg = data.get("message").and_then(|v| v.as_str()).unwrap_or("导出数据生成失败");
        return json!({ "success": false, "error": msg });
    }
    let content = match which.as_str() {
        "checklist" => data.get("checklistContent").and_then(|v| v.as_str()).unwrap_or(""),
        "awesome" => data.get("awesomeContent").and_then(|v| v.as_str()).unwrap_or(""),
        _ => return json!({ "success": false, "error": "未知的导出类型" }),
    }
    .to_string();
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return json!({ "success": false, "error": format!("创建目录失败: {}", e) });
            }
        }
    }
    match std::fs::write(&dest, content) {
        Ok(_) => json!({ "success": true, "path": dest }),
        Err(e) => json!({ "success": false, "error": format!("写入失败: {}", e) }),
    }
}

/// 用系统默认程序打开外链。
/// 只放行 http/https，避免 URL 被当作本地文件或命令执行。
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("仅支持打开 http/https 链接".to_string());
    }
    opener::open(url).map_err(|e| e.to_string())
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

    #[test]
    #[cfg(windows)]
    fn scan_preview_keeps_evidence_and_commit_populates_software() {
        let root = std::env::temp_dir().join(format!("ledger_scan_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let preview = scan_preview_blocking();
        assert_eq!(
            preview.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "preview result: {}",
            preview
        );
        // 证据现在持久化在 data/evidence/<机器名>/
        assert!(store::evidence_dir().exists(), "evidence should persist under data/");

        let candidates = preview
            .get("candidates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let keys: Vec<String> = candidates
            .iter()
            .filter(|c| c.get("kind").and_then(|v| v.as_str()) == Some("new"))
            .filter_map(|c| c.get("key").and_then(|v| v.as_str()).map(String::from))
            .collect();

        let commit = scan_commit(json!({ "selectedKeys": keys }));
        assert_eq!(
            commit.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "commit result: {}",
            commit
        );
        assert!(!store::read_software().is_empty(), "commit should populate software.json");

        // 开发环境清单也应随扫描落盘
        let mut dev_env_found = false;
        if let Ok(entries) = fs::read_dir(store::evidence_dir()) {
            for entry in entries.flatten() {
                let f = entry.path().join("dev-env.json");
                if f.exists() {
                    let v = store::read_json(&f);
                    assert!(
                        v.get("providers").and_then(|p| p.as_array()).map(|a| !a.is_empty()).unwrap_or(false),
                        "dev-env.json should contain providers"
                    );
                    dev_env_found = true;
                }
            }
        }
        assert!(dev_env_found, "scan should produce dev-env.json");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn get_dev_env_aggregates_evidence() {
        let root = std::env::temp_dir().join(format!("ledger_devenv_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let _guard = crate::store::test_support::use_root(&root);

        let dir = store::evidence_dir().join("M-A");
        fs::create_dir_all(&dir).unwrap();
        store::write_json(
            &dir.join("dev-env.json"),
            &json!({
                "schema_version": 1,
                "machine_id": "M-A",
                "collected_at": "2026-01-01T00:00:00+08:00",
                "providers": [
                    { "id": "rust", "label": "Rust", "available": true, "items": [], "restore_commands": ["rustup default stable"] }
                ]
            }),
        );

        let res = get_dev_env();
        assert_eq!(res.get("success").and_then(|v| v.as_bool()), Some(true));
        let machines = res.get("machines").and_then(|v| v.as_array()).unwrap();
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].get("machine_id").and_then(|v| v.as_str()), Some("M-A"));
        assert_eq!(machines[0].get("dir").and_then(|v| v.as_str()), Some("M-A"));
        assert_eq!(
            machines[0].get("providers").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(1)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn vault_roundtrip_add_list_export_delete() {
        let root = std::env::temp_dir().join(format!("ledger_vault_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let src = root.join("bookmarks_2026-08-19.html");
        fs::write(&src, "hello").unwrap();

        let added = vault_add(
            "soft".into(),
            "SW-001".into(),
            vec![src.to_string_lossy().to_string()],
        );
        assert_eq!(added["success"], true);
        assert_eq!(added["added"].as_u64(), Some(1));

        let listed = vault_list("soft".into(), "SW-001".into());
        let files = listed["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["name"], "bookmarks_2026-08-19.html");

        let dest = root.join("out.html");
        let exported = vault_export(
            "soft".into(),
            "SW-001".into(),
            "bookmarks_2026-08-19.html".into(),
            dest.to_string_lossy().to_string(),
        );
        assert_eq!(exported["success"], true);
        assert!(dest.exists());

        let deleted = vault_delete(
            "soft".into(),
            "SW-001".into(),
            "bookmarks_2026-08-19.html".into(),
        );
        assert_eq!(deleted["success"], true);
        assert_eq!(
            vault_list("soft".into(), "SW-001".into())["files"].as_array().unwrap().len(),
            0
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extension_notes_roundtrip_and_machine_in_vault_list() {
        let root = std::env::temp_dir().join(format!("ledger_ext_notes_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let saved = update_extension(
            "uBlock0@raymondhill.net".into(),
            json!({ "notes": "广告拦截，装完必开", "restore_intent": "must" }),
        );
        assert_eq!(saved["success"], true);

        let stored = crate::store::read_extensions();
        assert_eq!(stored["uBlock0@raymondhill.net"]["notes"].as_str(), Some("广告拦截，装完必开"));
        assert_eq!(stored["uBlock0@raymondhill.net"]["restore_intent"].as_str(), Some("must"));

        // vault_list 必须返回真实机器名，前端才能在面板里显示 data/vault/<机器>/...
        let listed = vault_list("ext".into(), "uBlock0@raymondhill.net".into());
        assert!(listed["machine"].as_str().map(|m| !m.is_empty()).unwrap_or(false));

        let _ = fs::remove_dir_all(&root);
    }
}
