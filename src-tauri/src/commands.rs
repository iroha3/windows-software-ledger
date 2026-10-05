use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use tauri::Manager;

use crate::exporter::export_checklists;
use crate::ingest::{apply_selected, build_candidates, known_icon_refreshes};
use crate::store;

// ---------------------------------------------------------------------------
// 只读镜像（同步）
//
// 另一台机器持有会话锁时，本机进入只读：所有写命令直接拒绝。
// 放在后端是为了覆盖所有页面（卡片速审 / 浏览器页等），而不只是主页。
// ---------------------------------------------------------------------------

static READ_ONLY: AtomicBool = AtomicBool::new(false);

pub fn set_read_only(v: bool) {
    READ_ONLY.store(v, Ordering::SeqCst);
    // 落盘编辑权状态：MCP 是独立进程，读不到这里的 AtomicBool，
    // 只能靠 data/.sync/session.json 判断本机能否写入。
    store::write_session_state(v);
}

fn deny_if_read_only() -> Option<Value> {
    if READ_ONLY.load(Ordering::SeqCst) {
        Some(json!({ "success": false, "error": "本机只读：另一台机器正在编辑，无法修改数据" }))
    } else {
        None
    }
}

fn as_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// 绿色/便携判定：类型为 portable，或任一机器分布标记为 portable。
fn item_is_portable(item: &Value) -> bool {
    if as_str(item, "type") == "portable" {
        return true;
    }
    item.get("machines")
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter().any(|m| as_str(m, "form") == "portable"))
        .unwrap_or(false)
}

/// 依据恢复意愿 + 形态推导处置方式：
/// 必须恢复 → 绿色版压缩目录、安装版重新下载；其余 → 无需操作。
fn derive_strategy(intent: &str, portable: bool) -> &'static str {
    match intent {
        "must" => {
            if portable {
                "copy_dir"
            } else {
                "redownload"
            }
        }
        _ => "none",
    }
}

/// 本次更新若改了恢复意愿、且未同时显式指定处置方式，则按意愿自动推导处置方式。
fn apply_derived_strategy(item: &mut Value, updates: &Value) {
    if updates.get("restore_intent").is_none() || updates.get("backup_strategy").is_some() {
        return;
    }
    let intent = as_str(item, "restore_intent");
    let portable = item_is_portable(item);
    if let Some(obj) = item.as_object_mut() {
        obj.insert(
            "backup_strategy".to_string(),
            json!(derive_strategy(&intent, portable)),
        );
    }
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
    if let Some(v) = deny_if_read_only() {
        return v;
    }
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
// WebDAV 同步
//
// 单写者会话锁：开软件抢锁并拉取，关软件推送并释放；抢不到锁则本机只读。
// 全部网络/文件操作在 spawn_blocking 里跑，避免卡住 UI 线程。
// ---------------------------------------------------------------------------

/// 进程级同步状态：owner 是「主机名#实例短号」，held 表示当前是否持有会话锁。
pub struct SyncState {
    pub owner: String,
    pub held: AtomicBool,
}

impl SyncState {
    pub fn new() -> Self {
        let short = store::new_uuid();
        Self {
            owner: format!("{}#{}", current_machine(), &short[..8]),
            held: AtomicBool::new(false),
        }
    }
}

#[tauri::command]
pub fn get_webdav_config() -> Value {
    store::get_webdav_config()
}

#[tauri::command]
pub fn save_webdav_config(payload: Value) -> Value {
    let mut current = store::get_webdav_config();
    if let (Value::Object(base), Value::Object(over)) = (&mut current, &payload) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    store::save_webdav_config(&current);
    json!({ "success": true, "config": current })
}

#[tauri::command]
pub async fn webdav_test() -> Value {
    match tauri::async_runtime::spawn_blocking(crate::sync::test_connection).await {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 开软件时调用：抢会话锁；抢到则拉取远端并保持锁，未抢到则返回持有者信息（本机只读）。
#[tauri::command]
pub async fn sync_session_start(app: tauri::AppHandle) -> Value {
    let (owner, host) = {
        let state = app.state::<SyncState>();
        (state.owner.clone(), current_machine())
    };
    let owner2 = owner.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let lock = crate::sync::try_acquire(&owner2, &host);
        if lock.get("success").and_then(|v| v.as_bool()) != Some(true) {
            // 未启用 WebDAV（或地址不全）时不限制编辑；网络错误则保持原状态。
            if lock.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                set_read_only(false);
            }
            return lock;
        }
        if lock.get("acquired").and_then(|v| v.as_bool()) != Some(true) {
            set_read_only(true);
            return json!({
                "success": true,
                "acquired": false,
                "lock": lock.get("lock").cloned().unwrap_or(Value::Null)
            });
        }
        set_read_only(false);
        let sync = crate::sync::run_sync(crate::sync::SyncMode::Normal);
        json!({ "success": true, "acquired": true, "sync": sync })
    })
    .await;
    match res {
        Ok(v) => {
            if v.get("acquired").and_then(|x| x.as_bool()) == Some(true) {
                app.state::<SyncState>().held.store(true, Ordering::SeqCst);
            }
            v
        }
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 关软件时调用：最后一次推送后释放会话锁。
#[tauri::command]
pub async fn sync_session_end(app: tauri::AppHandle) -> Value {
    let (owner, was_held) = {
        let state = app.state::<SyncState>();
        (state.owner.clone(), state.held.swap(false, Ordering::SeqCst))
    };
    let res = tauri::async_runtime::spawn_blocking(move || {
        // 只读机（未持锁）绝不推送，否则会覆盖持锁机的改动。
        if !was_held {
            return json!({ "success": true, "skipped": true });
        }
        let sync = crate::sync::run_sync(crate::sync::SyncMode::Normal);
        let release = crate::sync::release(&owner);
        json!({ "success": true, "sync": sync, "release": release })
    })
    .await;
    match res {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 前端定时调用，续租会话锁（每 5 分钟）。
#[tauri::command]
pub async fn sync_heartbeat(app: tauri::AppHandle) -> Value {
    let (owner, held) = {
        let state = app.state::<SyncState>();
        (state.owner.clone(), state.held.load(Ordering::SeqCst))
    };
    if !held {
        return json!({ "success": true, "held": false });
    }
    let host = current_machine();
    match tauri::async_runtime::spawn_blocking(move || crate::sync::renew(&owner, &host)).await {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 手动同步。mode: `auto`（默认）/ `push`（本地覆盖远端）/ `pull`（远端覆盖本地）。
#[tauri::command]
pub async fn sync_now(mode: String, app: tauri::AppHandle) -> Value {
    let owner = app.state::<SyncState>().owner.clone();
    let host = current_machine();
    let sync_mode = match mode.as_str() {
        "push" => crate::sync::SyncMode::ForcePush,
        "pull" => crate::sync::SyncMode::ForcePull,
        _ => crate::sync::SyncMode::Normal,
    };
    let res = tauri::async_runtime::spawn_blocking(move || {
        let lock = crate::sync::try_acquire(&owner, &host);
        if lock.get("success").and_then(|v| v.as_bool()) != Some(true) {
            if lock.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                set_read_only(false);
            }
            return lock;
        }
        if lock.get("acquired").and_then(|v| v.as_bool()) != Some(true) {
            set_read_only(true);
            return json!({
                "success": false,
                "locked": true,
                "error": "另一台机器正在同步，无法执行",
                "lock": lock.get("lock").cloned().unwrap_or(Value::Null)
            });
        }
        set_read_only(false);
        let sync = crate::sync::run_sync(sync_mode);
        let ok = sync.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        json!({ "success": ok, "acquired": true, "sync": sync })
    })
    .await;
    match res {
        Ok(v) => {
            if v.get("acquired").and_then(|x| x.as_bool()) == Some(true) {
                app.state::<SyncState>().held.store(true, Ordering::SeqCst);
            }
            v
        }
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 强制解除远端会话锁（另一台机器卡死时用）。
#[tauri::command]
pub async fn sync_unlock(app: tauri::AppHandle) -> Value {
    app.state::<SyncState>().held.store(false, Ordering::SeqCst);
    // 解除远端锁后本机不再受只读限制（随后会重新尝试抢锁）。
    set_read_only(false);
    match tauri::async_runtime::spawn_blocking(crate::sync::unlock).await {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 查询锁状态与上次同步结果（供设置页展示）。
#[tauri::command]
pub async fn sync_status(app: tauri::AppHandle) -> Value {
    let (owner, owner_resp, held) = {
        let state = app.state::<SyncState>();
        (state.owner.clone(), state.owner.clone(), state.held.load(Ordering::SeqCst))
    };
    let last = store::read_json(&store::last_sync_file());
    match tauri::async_runtime::spawn_blocking(move || crate::sync::lock_status(&owner)).await {
        Ok(mut v) => {
            if let Some(o) = v.as_object_mut() {
                o.insert("we_hold".to_string(), json!(held));
                o.insert("owner".to_string(), json!(owner_resp));
                o.insert("last".to_string(), last);
            }
            v
        }
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

// ---------------------------------------------------------------------------
// 状态汇总
// ---------------------------------------------------------------------------

/// 应用版本号（唯一来源：src-tauri/Cargo.toml）。关于弹窗动态读取，避免各页面硬编码漂移。
#[tauri::command]
pub fn get_version() -> Value {
    json!({ "version": env!("CARGO_PKG_VERSION") })
}

/// MCP（agent 集成）配置信息：返回当前 exe 绝对路径，供设置页生成各客户端的配置片段。
/// 数据目录始终是 exe 同级的 data/，所以客户端只认这个 exe 路径就够了。
#[tauri::command]
pub fn get_mcp_info() -> Value {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let write_enabled = store::get_config()
        .get("mcp_write_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    json!({
        "exe_path": exe,
        "args": ["--mcp"],
        "server_name": "software-ledger",
        "version": env!("CARGO_PKG_VERSION"),
        "write_enabled": write_enabled,
    })
}

/// 台账数据版本指纹：供前端轮询，检测到外部（如 MCP 写工具）改动后自动刷新表格。
#[tauri::command]
pub fn get_ledger_revision() -> Value {
    json!({ "revision": store::ledger_revision() })
}

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
        "on_demand": count(&|s| as_str(s, "restore_intent") == "on_demand"),
        "drop": count(&|s| as_str(s, "restore_intent") == "drop"),
        "unreviewed": count(&|s| {
            let i = as_str(s, "restore_intent");
            i.is_empty() || i == "unreviewed"
        }),
        "awesome": count(&|s| s.get("is_awesome").and_then(|v| v.as_bool()).unwrap_or(false)),
        "backupTasks": count(&|s| as_str(s, "backup_strategy") == "copy_dir"),
        "ready": count(&|s| as_str(s, "prep_status") == "ready")
    });

    json!({
        "machines": machines,
        "stats": stats,
        "availablePaths": available_paths,
        "machine_aliases": aliases,
        "current_machine": current_machine()
    })
}

// ---------------------------------------------------------------------------
// 软件清单 CRUD
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_software() -> Value {
    let mut items = store::read_software();
    // 图标以 data URI 注入（文件本体在 data/icons/，由条目的 icon_file 字段指向），
    // 方便 WebView 直接 <img src>；不写回 software.json。
    for item in items.iter_mut() {
        let file = as_str(item, "icon_file");
        if let Some(uri) = icon_data_uri(&file) {
            if let Some(obj) = item.as_object_mut() {
                obj.insert("icon".to_string(), json!(uri));
            }
        }
    }
    Value::Array(items)
}

/// 读图标文件并编码成 data URI；缺失/读失败返回 None（前端回退通用图标）。
/// `file` 取自条目的 `icon_file` 字段，先经 safe_component 处理以防路径穿越。
fn icon_data_uri(file: &str) -> Option<String> {
    if file.is_empty() {
        return None;
    }
    let path = store::icons_dir().join(safe_component(file));
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
    collect_browser_extensions(true)
}

/// 只读版本：给 MCP 用。认回 uuid 时只在内存里铸号，绝不写 extensions.json / browsers.json，
/// 保持 MCP「只提供信息、不落盘」的定位。
pub fn get_browser_extensions_readonly() -> Value {
    collect_browser_extensions(false)
}

fn collect_browser_extensions(commit: bool) -> Value {
    let root = store::evidence_dir();
    // 用户层：扩展 / 浏览器均按 uuid 存，匹配键内嵌。重扫时据此把 uuid 认回来。
    let mut ext_map = store::read_extensions();
    let mut browser_map = store::read_browsers();
    let mut ext_changed = false;
    let mut browser_changed = false;
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
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let machine_id = as_str(&data, "machine_id");
            let machine_id = if machine_id.is_empty() {
                dir_name.clone()
            } else {
                machine_id
            };
            let mut browsers = data.get("browsers").cloned().unwrap_or(json!([]));
            if let Value::Array(bs) = &mut browsers {
                for b in bs.iter_mut() {
                    let browser_id = b
                        .get("id")
                        .and_then(|v| v.as_str())
                        .or_else(|| b.get("label").and_then(|v| v.as_str()))
                        .unwrap_or("")
                        .to_string();
                    let (browser_uuid, created) =
                        resolve_browser_uuid(&mut browser_map, &machine_id, &browser_id);
                    browser_changed |= created;
                    if let Some(obj) = b.as_object_mut() {
                        obj.insert("uuid".to_string(), json!(browser_uuid));
                    }
                    let Some(profiles) = b.get_mut("profiles").and_then(|p| p.as_array_mut()) else {
                        continue;
                    };
                    for p in profiles.iter_mut() {
                        let profile = p.get("profile").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let Some(exts) = p.get_mut("extensions").and_then(|e| e.as_array_mut()) else {
                            continue;
                        };
                        for ext in exts.iter_mut() {
                            let Some(obj) = ext.as_object_mut() else {
                                continue;
                            };
                            let ext_id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let (uuid, created) = resolve_ext_uuid(
                                &mut ext_map,
                                &machine_id,
                                &browser_id,
                                &profile,
                                &ext_id,
                            );
                            ext_changed |= created;
                            // 注入用户层字段（备注 / 保留意愿 / 就绪 / 精选），重扫不丢失
                            if let Some(user) = ext_map.get(uuid.as_str()).and_then(|v| v.as_object()) {
                                for (k, v) in user {
                                    if matches!(
                                        k.as_str(),
                                        "updated_at" | "machine_id" | "browser_id" | "profile" | "ext_id"
                                    ) {
                                        continue;
                                    }
                                    obj.insert(k.clone(), v.clone());
                                }
                            }
                            obj.insert("uuid".to_string(), json!(uuid.clone()));
                            obj.insert("vault_count".to_string(), json!(vault_file_count("ext", &uuid)));
                        }
                    }
                }
            }
            machines.push(json!({
                "machine_id": machine_id,
                "dir": dir_name,
                "collected_at": as_str(&data, "collected_at"),
                "browsers": browsers,
            }));
        }
    }
    if commit && ext_changed {
        store::write_extensions(&ext_map);
    }
    if commit && browser_changed {
        store::write_browsers(&browser_map);
    }
    machines.sort_by(|a, b| as_str(a, "machine_id").cmp(&as_str(b, "machine_id")));
    json!({ "success": true, "machines": machines, "vault_machine": current_machine() })
}

// ---------------------------------------------------------------------------
// 通用文件保管箱：data/vault/<机器>/<kind>/<id>/
// 只存用户手动放入的文件，绝不自动采集。
// ---------------------------------------------------------------------------

pub fn current_machine() -> String {
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

/// 保管箱目录：`data/vault/<kind>/<uuid>/`（不再分主机名，data 不追求人类可读）。
/// `id` 参数一律是实体的 uuid（软件 / 扩展 / 浏览器）。
fn vault_target_dir(kind: &str, id: &str) -> PathBuf {
    store::vault_dir()
        .join(safe_component(kind))
        .join(safe_component(id))
}

/// 把文件/目录移入垃圾桶（软删除）：单独放进 `trash/<stamp>-<rand>/`，
/// 并用 `meta.json` 记下原始相对路径，恢复时精确还原。同盘 rename；
/// 失败则回滚、原样保留，绝不销毁。
fn move_to_trash(src: &std::path::Path, original_rel: &str) -> bool {
    if !src.exists() {
        return true;
    }
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S");
    let rand = store::new_uuid();
    let slot = store::trash_dir().join(format!("{}-{}", stamp, &rand[..8.min(rand.len())]));
    if std::fs::create_dir_all(&slot).is_err() {
        return false;
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let meta = json!({ "original": original_rel, "name": name });
    let _ = std::fs::write(
        slot.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    );
    if std::fs::rename(src, slot.join(&name)).is_ok() {
        true
    } else {
        let _ = std::fs::remove_dir_all(&slot);
        false
    }
}

/// 按 `(machine_id, browser_id, profile, ext_id)` 在扩展用户层里认回 uuid；认不到才铸新。
/// 返回 (uuid, 是否新建)。扩展跟软件一样按机器分开：同一扩展装在两台机器上 = 两个独立实体，
/// 各自有自己的意愿 / 备注 / 附件，绝不跨机共用一条。
fn resolve_ext_uuid(
    map: &mut Value,
    machine_id: &str,
    browser_id: &str,
    profile: &str,
    ext_id: &str,
) -> (String, bool) {
    if let Some(root) = map.as_object() {
        for (uuid, v) in root {
            if v.get("machine_id").and_then(|x| x.as_str()).unwrap_or("") == machine_id
                && v.get("browser_id").and_then(|x| x.as_str()).unwrap_or("") == browser_id
                && v.get("profile").and_then(|x| x.as_str()).unwrap_or("") == profile
                && v.get("ext_id").and_then(|x| x.as_str()).unwrap_or("") == ext_id
            {
                return (uuid.clone(), false);
            }
        }
    }
    let uuid = store::new_uuid();
    if let Some(root) = map.as_object_mut() {
        root.insert(
            uuid.clone(),
            json!({
                "machine_id": machine_id,
                "browser_id": browser_id,
                "profile": profile,
                "ext_id": ext_id,
            }),
        );
    }
    (uuid, true)
}

/// 按 `(machine_id, browser_id)` 在浏览器用户层里认回 uuid；认不到才铸新。
/// 浏览器整份配置归档同样按机器分开，不跨机共用。
fn resolve_browser_uuid(map: &mut Value, machine_id: &str, browser_id: &str) -> (String, bool) {
    if let Some(root) = map.as_object() {
        for (uuid, v) in root {
            if v.get("machine_id").and_then(|x| x.as_str()).unwrap_or("") == machine_id
                && v.get("browser_id").and_then(|x| x.as_str()).unwrap_or("") == browser_id
            {
                return (uuid.clone(), false);
            }
        }
    }
    let uuid = store::new_uuid();
    if let Some(root) = map.as_object_mut() {
        root.insert(
            uuid.clone(),
            json!({ "machine_id": machine_id, "browser_id": browser_id }),
        );
    }
    (uuid, true)
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

/// 保存某条扩展的用户层字段（data/extensions.json，按 uuid 持久化）。
/// 前端给 `(machine_id, browser_id, profile, ext_id)` 匹配键，uuid 由后端认回 / 铸新。
/// 缺 machine_id 时退回本机。传入字段与已有字段合并（不覆盖未提及的键）。
#[tauri::command]
pub fn update_extension(payload: Value) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    let machine_id = {
        let m = as_str(&payload, "machine_id");
        if m.is_empty() {
            current_machine()
        } else {
            m
        }
    };
    let browser_id = as_str(&payload, "browser_id");
    let profile = as_str(&payload, "profile");
    let ext_id = as_str(&payload, "ext_id");
    if ext_id.is_empty() {
        return json!({ "success": false, "error": "缺少扩展 ID" });
    }
    let Value::Object(incoming) = payload.get("fields").cloned().unwrap_or(json!({})) else {
        return json!({ "success": false, "error": "字段格式错误" });
    };
    let mut map = store::read_extensions();
    let (uuid, _) = resolve_ext_uuid(&mut map, &machine_id, &browser_id, &profile, &ext_id);
    let now = chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now()).to_rfc3339();
    {
        let Some(root) = map.as_object_mut() else {
            return json!({ "success": false, "error": "标注存储损坏" });
        };
        let entry = root.entry(uuid.clone()).or_insert_with(|| json!({}));
        let Some(eobj) = entry.as_object_mut() else {
            return json!({ "success": false, "error": "标注存储损坏" });
        };
        for (k, v) in incoming {
            if k != "updated_at" {
                eobj.insert(k, v);
            }
        }
        eobj.insert("machine_id".to_string(), json!(machine_id));
        eobj.insert("browser_id".to_string(), json!(browser_id));
        eobj.insert("profile".to_string(), json!(profile));
        eobj.insert("ext_id".to_string(), json!(ext_id));
        eobj.insert("updated_at".to_string(), json!(now));
    }
    store::write_extensions(&map);
    json!({ "success": true, "uuid": uuid })
}

#[tauri::command]
pub fn vault_add(kind: String, id: String, paths: Vec<String>) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
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
    if let Some(v) = deny_if_read_only() {
        return v;
    }
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
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    let uuid = as_str(&payload, "uuid");
    let updates = payload.get("updates").cloned().unwrap_or(json!({}));
    if uuid.is_empty() {
        return json!({ "success": false, "message": "Item not found" });
    }
    let mut software = store::read_software();

    let Some(idx) = software.iter().position(|s| as_str(s, "uuid") == uuid) else {
        return json!({ "success": false, "message": "Item not found" });
    };

    if let (Value::Object(item), Value::Object(upd)) = (&mut software[idx], &updates) {
        for (k, v) in upd {
            item.insert(k.clone(), v.clone());
        }
    }
    apply_derived_strategy(&mut software[idx], &updates);
    store::touch(&mut software[idx]);
    let item = software[idx].clone();
    store::write_software(&software);
    json!({ "success": true, "item": item })
}

#[tauri::command]
pub fn batch_update(payload: Value) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    let ids: Vec<String> = payload
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let updates = payload.get("updates").cloned().unwrap_or(json!({}));

    let mut software = store::read_software();
    let mut count = 0;
    for item in software.iter_mut() {
        if ids.iter().any(|k| as_str(item, "uuid") == *k) {
            if let (Value::Object(obj), Value::Object(upd)) = (&mut *item, &updates) {
                for (k, v) in upd {
                    obj.insert(k.clone(), v.clone());
                }
            }
            apply_derived_strategy(item, &updates);
            store::touch(item);
            count += 1;
        }
    }
    store::write_software(&software);
    json!({ "success": true, "count": count })
}

#[tauri::command]
pub fn delete_software(payload: Value) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    let ids: Vec<String> = payload
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let mut software = store::read_software();
    let mut ignored = store::read_ignored();

    // 按 uuid 定位待删条目
    let doomed: Vec<Value> = ids
        .iter()
        .filter_map(|k| software.iter().find(|s| as_str(s, "uuid") == *k).cloned())
        .collect();
    let doomed_uuids: Vec<String> = doomed.iter().map(|s| as_str(s, "uuid")).collect();

    // 删除即为墓碑：记录规范化名称与路径，重扫时默认不勾选，避免垃圾复活。
    for item in &doomed {
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

    software.retain(|s| !doomed_uuids.contains(&as_str(s, "uuid")));

    // 软删除归档：整个 `data/vault/soft/<uuid>/` 移入垃圾桶，绝不当场抹掉。
    // 图标每条记录一个（`<uuid>.png`），体积可忽略，直接删。
    for uuid in &doomed_uuids {
        if !uuid.is_empty() {
            let _ = move_to_trash(
                &vault_target_dir("soft", uuid),
                &format!("vault/soft/{}", safe_component(uuid)),
            );
            let _ = std::fs::remove_file(
                store::icons_dir().join(format!("{}.png", safe_component(uuid))),
            );
        }
    }

    store::write_software(&software);
    store::write_ignored(&ignored);
    json!({ "success": true, "remaining": software.len() })
}

#[tauri::command]
pub fn batch_add(payload: Value) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
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

    let hostname = current_machine();

    let mut new_items: Vec<Value> = Vec::new();
    for raw_name in names {
        let clean = raw_name.trim();
        if clean.is_empty() {
            continue;
        }
        current_id += 1;
        let item = json!({
            "id": format!("SW-{:03}", current_id),
            "uuid": store::new_uuid(),
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
            "created_at": chrono::Local::now().to_rfc3339(),
            "updated_at": chrono::Local::now().to_rfc3339()
        });
        new_items.push(item.clone());
        software.insert(0, item);
    }

    store::write_software(&software);
    json!({ "success": true, "items": new_items })
}

#[tauri::command]
pub fn merge_software(payload: Value) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    let mut software = store::read_software();

    // 锚点 / 被并项一律按 uuid（内部唯一标识）。
    let target_uuid = as_str(&payload, "targetUuid");
    if target_uuid.is_empty() {
        return json!({ "success": false, "message": "Target not found" });
    }
    let merge_uuids: Vec<String> = payload
        .get("mergeUuids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let Some(target_idx) = software.iter().position(|s| as_str(s, "uuid") == target_uuid) else {
        return json!({ "success": false, "message": "Target not found" });
    };

    // 收集需要合并的机器与字段
    let merge_items: Vec<Value> = software
        .iter()
        .filter(|s| merge_uuids.contains(&as_str(s, "uuid")))
        .cloned()
        .collect();

    for item in &merge_items {
        if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
            for m in ms {
                let mid = as_str(m, "machine_id");
                let Some(arr) = software[target_idx]
                    .get_mut("machines")
                    .and_then(|x| x.as_array_mut())
                else {
                    continue;
                };
                // 同一台机器只保留一条：锚点已有则只补齐缺失字段，不再重复追加。
                if let Some(existing) = arr.iter_mut().find(|tm| as_str(tm, "machine_id") == mid) {
                    if as_str(existing, "install_location").is_empty() {
                        let loc = as_str(m, "install_location");
                        if !loc.is_empty() {
                            existing["install_location"] = json!(loc);
                        }
                    }
                    if as_str(existing, "version").is_empty() {
                        let v = as_str(m, "version");
                        if !v.is_empty() {
                            existing["version"] = json!(v);
                        }
                    }
                    if as_str(m, "form") == "portable" {
                        existing["form"] = json!("portable");
                    }
                } else {
                    arr.push(m.clone());
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

    // 合并结果要留得住图标：锚点自己有就保留；否则从第一个有图标的子项复制过来。
    if as_str(&software[target_idx], "icon_file").is_empty() {
        for mu in &merge_uuids {
            let icon = software
                .iter()
                .find(|s| as_str(s, "uuid") == *mu)
                .map(|s| as_str(s, "icon_file"))
                .unwrap_or_default();
            if !icon.is_empty() {
                let src = store::icons_dir().join(safe_component(&icon));
                let dest_file = store::icon_file_name(&target_uuid);
                let dest = store::icons_dir().join(safe_component(&dest_file));
                if std::fs::copy(&src, &dest).is_ok() {
                    software[target_idx]["icon_file"] = json!(dest_file);
                }
                break;
            }
        }
    }

    store::touch(&mut software[target_idx]);
    software.retain(|s| !merge_uuids.contains(&as_str(s, "uuid")));

    // 删除被合并条目的图标（每条记录一个，无共享）
    for mu in &merge_uuids {
        if !mu.is_empty() {
            let _ = std::fs::remove_file(
                store::icons_dir().join(format!("{}.png", safe_component(mu))),
            );
        }
    }

    // 归档合并：子项文件搬入锚点目录；同名时锚点优先，子项同名文件进垃圾桶。
    let target_dir = vault_target_dir("soft", &target_uuid);
    for mu in &merge_uuids {
        let child_dir = vault_target_dir("soft", mu);
        if !child_dir.exists() {
            continue;
        }
        let _ = std::fs::create_dir_all(&target_dir);
        if let Ok(entries) = std::fs::read_dir(&child_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if !p.is_file() {
                    continue;
                }
                let fname = e.file_name().to_string_lossy().to_string();
                let dest = target_dir.join(e.file_name());
                if dest.exists() {
                    let _ = move_to_trash(
                        &p,
                        &format!("vault/soft/{}/{}", safe_component(mu), fname),
                    );
                } else {
                    let _ = std::fs::rename(&p, &dest);
                }
            }
        }
        // 剩余空目录 / 未处理项整体进垃圾桶
        let _ = move_to_trash(&child_dir, &format!("vault/soft/{}", safe_component(mu)));
    }

    let target = software
        .iter()
        .find(|s| as_str(s, "uuid") == target_uuid)
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
  "download_url": "https://...",
  "config_notes": "配置位置说明与迁移备忘"
}}

枚举约束说明：
- category: 必须从 [开发工具, 系统工具, 浏览器与网络, 媒体娱乐, 办公与笔记, 通讯与社交, 其他] 中选一个
- type: 必须从 [desktop, portable, cli, runtime] 中选一个
- restore_intent: 必须从 [must, on_demand, drop] 中选一个
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

    let mut body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.1
    });
    // DeepSeek 默认开启思考模式；AI 预判只需结构化 JSON，显式关闭以降延迟与费用。
    // 仅对 DeepSeek 端点下发，避免 OpenAI / 本地 LM Studio 因未知参数报错。
    if endpoint.contains("api.deepseek.com") {
        body["thinking"] = json!({ "type": "disabled" });
    }

    let mut req = client.post(&endpoint).json(&body);
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
    let body_text = match resp.text().await {
        Ok(t) => t,
        Err(e) => return json!({ "success": false, "error": e.to_string() }),
    };
    if !status.is_success() {
        let detail = body_text.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!(" - {}", detail)
        };
        return json!({ "success": false, "error": format!("LLM 服务返回状态码: {}{}", status.as_u16(), detail) });
    }
    let body: Value = match serde_json::from_str(&body_text) {
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
    machine_id: &str,
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
            .arg(output_dir)
            .arg("-MachineId")
            .arg(machine_id);
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
    // 机器 id 就是主机名（不引入额外稳定标识）。
    let machine = current_machine();
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
    // 扫描目录按主机名分键（每台机只读自己那份）；兼容旧版扁平数组。
    let dirs = cfg
        .get("scan_directories")
        .and_then(|v| {
            if let Some(arr) = v.as_array() {
                Some(arr.iter().filter_map(|x| x.as_str()).map(String::from).collect::<Vec<_>>())
            } else {
                v.get(&machine).and_then(|x| x.as_array()).map(|a| {
                    a.iter().filter_map(|x| x.as_str()).map(String::from).collect::<Vec<_>>()
                })
            }
        })
        .unwrap_or_default()
        .join(",");

    let result = run_powershell(&script, &evidence_dir, &dirs, &machine);
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

/// 重扫时为「已知（未变）」的已有条目补齐缺失图标：读 pending 里的 knownIcons，
/// 为条目生成稳定的 `icon_file` 字段并登记落盘。返回待复制的 (文件名, 证据源路径)。
/// 已有图标字段且文件仍在的条目不动；失败不阻断导入。
fn fill_known_icons(software: &mut [Value], pending: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Some(arr) = pending.get("knownIcons").and_then(|v| v.as_array()) else {
        return out;
    };
    for pair in arr {
        let (Some(id), Some(src)) = (
            pair.get(0).and_then(|v| v.as_str()),
            pair.get(1).and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let Some(idx) = software.iter().position(|s| as_str(s, "uuid") == id) else {
            continue;
        };
        let existing = as_str(&software[idx], "icon_file");
        if !existing.is_empty() && store::icons_dir().join(safe_component(&existing)).exists() {
            continue;
        }
        let file = store::icon_file_name(id);
        if let Some(obj) = software[idx].as_object_mut() {
            obj.insert("icon_file".to_string(), json!(file.clone()));
        }
        out.push((file, src.to_string()));
    }
    out
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
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    match tauri::async_runtime::spawn_blocking(scan_preview_blocking).await {
        Ok(v) => v,
        Err(e) => json!({ "success": false, "error": e.to_string() }),
    }
}

/// 扫描第二步：只把勾选的候选写入 software.json。
#[tauri::command]
pub fn scan_commit(payload: Value) -> Value {
    if let Some(v) = deny_if_read_only() {
        return v;
    }
    let selected_keys: Vec<String> = payload
        .get("selectedKeys")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let pending = store::read_json(&pending_scan_file());
    if !pending.is_object() {
        return json!({ "success": false, "error": "扫描预览已失效，请重新扫描" });
    }
    let candidates: Vec<Value> = pending
        .get("candidates")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut software = store::read_software();
    let was_empty = software.is_empty();
    let result = apply_selected(&mut software, &candidates, &selected_keys, !was_empty);
    // 补齐已有条目的图标（会写回 icon_file 字段），与是否勾选新条目无关
    let known_icons = fill_known_icons(&mut software, &pending);
    store::write_software(&software);

    // 把图标文件落到 data/icons/（导入项 + 补齐项；失败不阻断导入）
    let dir = store::icons_dir();
    let _ = std::fs::create_dir_all(&dir);
    for (file, src) in result.icons.iter().chain(known_icons.iter()) {
        let _ = std::fs::copy(src, dir.join(safe_component(file)));
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

/// 导出软件清单为 xlsx（零依赖手写，直接写盘）。
#[tauri::command]
pub fn export_xlsx(dest: String) -> Value {
    let rows = crate::exporter::export_xlsx_rows();
    let widths = [
        22.0, 14.0, 12.0, 10.0, 24.0, 34.0, 20.0, 22.0, 12.0, 8.0, 36.0, 46.0,
    ];
    let meta = vec![format!(
        "导出时间：{}    软件版本：v{}    共 {} 条",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        env!("CARGO_PKG_VERSION"),
        rows.len().saturating_sub(1)
    )];
    let bytes = crate::xlsx::build("软件清单", Some("软件备份台账 · 软件清单"), &meta, &rows, &widths);
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return json!({ "success": false, "error": format!("创建目录失败: {}", e) });
            }
        }
    }
    match std::fs::write(&dest, bytes) {
        Ok(_) => json!({ "success": true, "path": dest }),
        Err(e) => json!({ "success": false, "error": format!("写入失败: {}", e) }),
    }
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
            { "id": "SW-001", "uuid": "uuid-a", "name": "A", "restore_intent": "must", "machines": [{ "machine_id": "M1", "install_location": "C:\\A" }], "backup_strategy": "none", "prep_status": "todo" },
            { "id": "SW-002", "uuid": "uuid-b", "name": "B", "restore_intent": "unreviewed", "version": "1.0", "machines": [{ "machine_id": "M2", "install_location": "D:\\B" }], "backup_strategy": "copy_dir", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());

        assert_eq!(get_software().as_array().unwrap().len(), 2);

        let r = update_software(json!({ "uuid": "uuid-a", "updates": { "restore_intent": "on_demand", "has_config": true } }));
        assert_eq!(r.get("success").and_then(|v| v.as_bool()), Some(true));
        let updated = get_software();
        let a = updated
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "SW-001")
            .unwrap();
        assert_eq!(a["restore_intent"], "on_demand");
        assert_eq!(a["has_config"], true);

        let r = batch_update(json!({ "ids": ["uuid-a", "uuid-b"], "updates": { "prep_status": "ready" } }));
        assert_eq!(r["count"], 2);

        let r = merge_software(json!({ "targetUuid": "uuid-a", "mergeUuids": ["uuid-b"] }));
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

        let del = delete_software(json!({ "ids": ["uuid-a"] }));
        assert_eq!(del["remaining"], 2);

        let ex = export_markdown();
        assert_eq!(ex.get("success").and_then(|v| v.as_bool()), Some(true));

        let cfg = save_config(json!({ "llm_model": "test-model" }));
        assert_eq!(cfg["config"]["llm_model"], "test-model");
        assert_eq!(get_config()["llm_model"], "test-model");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_keeps_target_icon_and_removes_merged_icon() {
        let root = std::env::temp_dir().join(format!("ledger_merge_icon_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data")).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let seed = json!([
            { "id": "SW-001", "uuid": "uuid-a", "name": "A", "machines": [{ "machine_id": "M1", "install_location": "C:\\A" }], "icon_file": "uuid-a.png", "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" },
            { "id": "SW-002", "uuid": "uuid-b", "name": "B", "machines": [{ "machine_id": "M1", "install_location": "C:\\B" }], "icon_file": "uuid-b.png", "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());
        fs::create_dir_all(store::icons_dir()).unwrap();
        fs::write(store::icons_dir().join("uuid-a.png"), b"a").unwrap();
        fs::write(store::icons_dir().join("uuid-b.png"), b"b").unwrap();

        let r = merge_software(json!({ "targetUuid": "uuid-a", "mergeUuids": ["uuid-b"] }));
        assert_eq!(r.get("success").and_then(|v| v.as_bool()), Some(true));

        // target（第一个选中）的图标必须保留
        let sw = store::read_software();
        assert_eq!(sw.len(), 1);
        assert_eq!(sw[0]["icon_file"], "uuid-a.png");
        assert!(store::icons_dir().join("uuid-a.png").exists(), "target 图标不应被删");
        // 被合并条目的图标清理掉
        assert!(!store::icons_dir().join("uuid-b.png").exists(), "被合并条目的图标应清理");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_shared_icon_across_machines_is_not_deleted() {
        let root = std::env::temp_dir().join(format!("ledger_merge_shared_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data")).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        // 跨机器：同一软件在 A / B 机各一条（各自 uuid、各自图标）
        let seed = json!([
            { "id": "SW-001", "uuid": "uuid-a", "name": "Visual Studio Code", "machines": [{ "machine_id": "M-A", "install_location": "C:\\A" }], "icon_file": "uuid-a.png", "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" },
            { "id": "SW-002", "uuid": "uuid-b", "name": "Visual Studio Code", "machines": [{ "machine_id": "M-B", "install_location": "D:\\B" }], "icon_file": "uuid-b.png", "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());
        fs::create_dir_all(store::icons_dir()).unwrap();
        fs::write(store::icons_dir().join("uuid-a.png"), b"ico").unwrap();
        fs::write(store::icons_dir().join("uuid-b.png"), b"ico").unwrap();

        let r = merge_software(json!({ "targetUuid": "uuid-a", "mergeUuids": ["uuid-b"] }));
        assert_eq!(r.get("success").and_then(|v| v.as_bool()), Some(true));

        let sw = store::read_software();
        assert_eq!(sw.len(), 1);
        assert_eq!(sw[0]["icon_file"], "uuid-a.png");
        assert!(store::icons_dir().join("uuid-a.png").exists(), "锚点图标应保留");
        assert!(!store::icons_dir().join("uuid-b.png").exists(), "被并条目图标应清理");
        assert_eq!(sw[0]["machines"].as_array().unwrap().len(), 2);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_adopts_icon_when_target_has_none() {
        let root = std::env::temp_dir().join(format!("ledger_merge_adopt_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data")).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        // target 无图标，被合并项有图标
        let seed = json!([
            { "id": "SW-001", "uuid": "uuid-a", "name": "A", "machines": [{ "machine_id": "M1" }], "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" },
            { "id": "SW-002", "uuid": "uuid-b", "name": "B", "machines": [{ "machine_id": "M1" }], "icon_file": "uuid-b.png", "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());
        fs::create_dir_all(store::icons_dir()).unwrap();
        fs::write(store::icons_dir().join("uuid-b.png"), b"b").unwrap();

        let r = merge_software(json!({ "targetUuid": "uuid-a", "mergeUuids": ["uuid-b"] }));
        assert_eq!(r.get("success").and_then(|v| v.as_bool()), Some(true));

        let sw = store::read_software();
        assert_eq!(sw.len(), 1);
        // 锚点无图标：从子项复制到锚点的 uuid 文件名下，子项图标清理
        assert_eq!(sw[0]["icon_file"], "uuid-a.png");
        assert!(store::icons_dir().join("uuid-a.png").exists(), "采用后的图标应落在锚点 uuid 下");
        assert!(!store::icons_dir().join("uuid-b.png").exists(), "子项图标应清理");

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
    fn delete_software_soft_deletes_vault_into_trash() {
        let root = std::env::temp_dir().join(format!("ledger_softdel_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data")).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let seed = json!([
            { "id": "SW-001", "uuid": "uuid-a", "name": "A", "machines": [], "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());
        let vdir = vault_target_dir("soft", "uuid-a");
        fs::create_dir_all(&vdir).unwrap();
        fs::write(vdir.join("cfg.ini"), b"x").unwrap();

        let r = delete_software(json!({ "ids": ["uuid-a"] }));
        assert_eq!(r["success"], true);
        assert!(!vdir.exists(), "原归档目录应被移走");
        // 软删除：`data/trash/<slot>/` 内留 meta.json + 目录本体，未物理销毁。
        let soft = fs::read_dir(store::trash_dir())
            .map(|rd| {
                rd.flatten().any(|e| {
                    let meta = store::read_json(&e.path().join("meta.json"));
                    as_str(&meta, "original").contains("uuid-a")
                        && e.path().join(as_str(&meta, "name")).exists()
                })
            })
            .unwrap_or(false);
        assert!(soft, "归档应连同 meta.json 一起留在 data/trash/ 下");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_collapses_same_machine() {
        let root = std::env::temp_dir().join(format!("ledger_merge_machine_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data")).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        // 同一台机器上同名两条（注册表 + 快捷方式，路径不同）
        let seed = json!([
            { "id": "SW-001", "uuid": "uuid-a", "name": "A", "machines": [{ "machine_id": "M1", "form": "installed", "install_location": "C:\\A" }], "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" },
            { "id": "SW-002", "uuid": "uuid-b", "name": "A", "machines": [{ "machine_id": "M1", "form": "shortcut", "install_location": "C:\\A\\a.exe" }], "restore_intent": "unreviewed", "backup_strategy": "none", "prep_status": "todo" }
        ]);
        store::write_software(seed.as_array().unwrap());

        let r = merge_software(json!({ "targetUuid": "uuid-a", "mergeUuids": ["uuid-b"] }));
        assert_eq!(r["success"], true);
        let sw = store::read_software();
        assert_eq!(sw.len(), 1);
        assert_eq!(
            sw[0]["machines"].as_array().unwrap().len(),
            1,
            "同一台机器合并后应只剩一条"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extension_notes_roundtrip_and_machine_in_vault_list() {
        let root = std::env::temp_dir().join(format!("ledger_ext_notes_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _guard = crate::store::test_support::use_root(&root);

        let saved = update_extension(json!({
            "machine_id": "PC-A",
            "browser_id": "edge",
            "profile": "Default",
            "ext_id": "uBlock0@raymondhill.net",
            "fields": { "notes": "广告拦截，装完必开", "restore_intent": "must" }
        }));
        assert_eq!(saved["success"], true);
        let uuid = saved["uuid"].as_str().unwrap().to_string();

        let stored = crate::store::read_extensions();
        let entry = stored.get(uuid.as_str()).unwrap();
        assert_eq!(entry["notes"].as_str(), Some("广告拦截，装完必开"));
        assert_eq!(entry["restore_intent"].as_str(), Some("must"));

        // 同一台机器上，同一个扩展 ID 在 Chrome / Edge 下是不同实体（uuid 不同）
        let chrome = update_extension(json!({
            "machine_id": "PC-A",
            "browser_id": "chrome",
            "profile": "Default",
            "ext_id": "uBlock0@raymondhill.net",
            "fields": {}
        }));
        assert_ne!(chrome["uuid"].as_str().unwrap(), uuid);

        // 同一扩展装在另一台机器上 = 另一个独立实体，绝不跨机共用标注
        let other_pc = update_extension(json!({
            "machine_id": "PC-B",
            "browser_id": "edge",
            "profile": "Default",
            "ext_id": "uBlock0@raymondhill.net",
            "fields": {}
        }));
        assert_ne!(other_pc["uuid"].as_str().unwrap(), uuid);

        // vault_list 必须返回真实机器名
        let listed = vault_list("ext".into(), uuid.clone());
        assert!(listed["machine"].as_str().map(|m| !m.is_empty()).unwrap_or(false));

        let _ = fs::remove_dir_all(&root);
    }
}
