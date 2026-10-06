// 同步引擎：手动双向同步（无锁） + 文件级镜像 + 记录级并集。
//
// 心智模型：
//   - 同步完全由用户手动触发（设置页「立即同步」），没有会话锁、没有只读模式，
//     两台机器任何时候都可编辑；用户约定同一时刻只在一台上改。
//   - 一次同步 = 拉取并合并远端 → 写回本地 → 把本地新状态推回远端
//     （`pull_merge` + `push`）。乙方未动的文件按 hash 镜像；双方都动过的 JSON 台账
//     走语义合并（`merge_software_with_deletions` 按 uuid 并集 + 墓碑过滤）。
//   - 删除与「合并条目」靠带 uuid 的墓碑传播（`ignored.json`），避免并集复活。
//
// 所有网络与文件操作都是阻塞式的，整体跑在 spawn_blocking 里。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::store;
use crate::webdav::WebDav;

const MANIFEST_REMOTE: &str = "manifest.json";
const BACKUP_DIR: &str = "_backup";
const SCHEMA_VERSION: u32 = 1;

/// 需要语义合并的 JSON 台账文件（其余文件一律按内容 hash 整份传输）。
fn is_json_ledger(rel: &str) -> bool {
    matches!(
        rel,
        "software.json"
            | "ignored.json"
            | "extensions.json"
            | "browsers.json"
            | "config.json"
    )
}

/// 顶层排除项：垃圾桶、同步辅助目录、本机 WebDAV 凭据都不参与同步。
fn is_excluded_top(name: &str) -> bool {
    matches!(name, "trash" | ".sync" | "webdav.json")
}

// ---------------------------------------------------------------------------
// manifest 数据结构
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FileEntry {
    pub hash: String,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub schema_version: u32,
    pub app_version: String,
    pub generated_at: String,
    pub machine_id: String,
    pub files: BTreeMap<String, FileEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            generated_at: String::new(),
            machine_id: String::new(),
            files: BTreeMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct IndexEntry {
    size: u64,
    mtime: i64,
    hash: String,
}
fn hash_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开 {} 失败: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(root: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) {
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if rel.is_empty() && is_excluded_top(&name) {
            continue;
        }
        let child = if rel.is_empty() {
            name
        } else {
            format!("{}/{}", rel, name)
        };
        let p = e.path();
        if p.is_dir() {
            collect_files(root, &child, out);
        } else if p.is_file() {
            out.push((child, p));
        }
    }
}

/// 扫描 `data/` 生成 manifest；用本地指纹缓存跳过未变文件的重算。
pub fn build_manifest() -> Result<Manifest, String> {
    let root = store::data_dir();
    let mut list = Vec::new();
    collect_files(&root, "", &mut list);

    let mut index: BTreeMap<String, IndexEntry> =
        serde_json::from_value(store::read_json(&store::file_index_file())).unwrap_or_default();

    let mut files = BTreeMap::new();
    for (rel, path) in list {
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let reuse = index
            .get(&rel)
            .filter(|e| e.size == size && e.mtime == mtime && !e.hash.is_empty())
            .map(|e| e.hash.clone());
        let hash = match reuse {
            Some(h) => h,
            None => hash_file(&path)?,
        };
        index.insert(
            rel.clone(),
            IndexEntry {
                size,
                mtime,
                hash: hash.clone(),
            },
        );
        files.insert(
            rel,
            FileEntry {
                hash,
                size,
                mtime,
            },
        );
    }
    index.retain(|k, _| files.contains_key(k));
    let _ = store::write_json(&store::file_index_file(), &serde_json::to_value(&index).unwrap());

    Ok(Manifest {
        schema_version: SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: chrono::Local::now().to_rfc3339(),
        machine_id: crate::commands::current_machine(),
        files,
    })
}

fn load_baseline() -> Manifest {
    serde_json::from_value(store::read_json(&store::baseline_manifest_file())).unwrap_or_default()
}

fn load_remote_manifest(client: &WebDav) -> Result<Option<Manifest>, String> {
    match client.get_json(MANIFEST_REMOTE)? {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => {
            let m: Manifest =
                serde_json::from_value(v).map_err(|e| format!("远端 manifest 解析失败: {}", e))?;
            if m.schema_version != SCHEMA_VERSION {
                return Err(format!(
                    "远端数据版本不兼容 (schema {} != {})，请使用相同版本的程序",
                    m.schema_version, SCHEMA_VERSION
                ));
            }
            Ok(Some(m))
        }
    }
}

// ---------------------------------------------------------------------------
// 语义合并（仅在「双方都有且不同」或首次接入时触发）
// ---------------------------------------------------------------------------

fn as_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn is_empty_val(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// 记录的新旧判据：优先 updated_at，回退 created_at，都没有算 0。
fn rec_time(v: &Value) -> i64 {
    for key in ["updated_at", "created_at", "deleted_at"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
                return d.timestamp();
            }
        }
    }
    0
}

fn merge_machines(a: &Value, b: &Value) -> Value {
    let mut out: Vec<Value> = a.as_array().cloned().unwrap_or_default();
    if let Some(bs) = b.as_array() {
        for m in bs {
            let mid = m.get("machine_id").and_then(|x| x.as_str()).unwrap_or("");
            if mid.is_empty() {
                if !out.contains(m) {
                    out.push(m.clone());
                }
                continue;
            }
            if let Some(ex) = out
                .iter_mut()
                .find(|x| x.get("machine_id").and_then(|v| v.as_str()) == Some(mid))
            {
                if let (Some(eo), Some(mo)) = (ex.as_object_mut(), m.as_object()) {
                    for (k, v) in mo {
                        let should = match eo.get(k) {
                            None => true,
                            Some(cv) => is_empty_val(cv) && !is_empty_val(v),
                        };
                        if should {
                            eo.insert(k.clone(), v.clone());
                        }
                    }
                    if m.get("form").and_then(|x| x.as_str()) == Some("portable") {
                        eo.insert("form".to_string(), json!("portable"));
                    }
                }
            } else {
                out.push(m.clone());
            }
        }
    }
    json!(out)
}

fn merge_software_item(local: &Value, remote: &Value) -> Value {
    let local_newer = rec_time(local) >= rec_time(remote);
    let (base, other) = if local_newer { (local, remote) } else { (remote, local) };
    let mut merged = base.clone();
    if let (Some(mo), Some(oo)) = (merged.as_object_mut(), other.as_object()) {
        for (k, v) in oo {
            let should = match mo.get(k) {
                None => true,
                Some(cv) => is_empty_val(cv) && !is_empty_val(v),
            };
            if should {
                mo.insert(k.clone(), v.clone());
            }
        }
    }
    let machines = merge_machines(
        local.get("machines").unwrap_or(&Value::Null),
        remote.get("machines").unwrap_or(&Value::Null),
    );
    if let Some(o) = merged.as_object_mut() {
        o.insert("machines".to_string(), machines);
    }
    merged
}

/// 合并 software.json：按 uuid 并集 + 记录级取新，再按墓碑 uuid 过滤。
pub fn merge_software_with_deletions(
    local: &[Value],
    remote: &[Value],
    tombstones: &[Value],
) -> Vec<Value> {
    let mut result = local.to_vec();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, it) in result.iter().enumerate() {
        let u = as_str(it, "uuid");
        if !u.is_empty() {
            index.insert(u, i);
        }
    }
    for r in remote {
        let u = as_str(r, "uuid");
        if u.is_empty() {
            result.push(r.clone());
            continue;
        }
        if let Some(&i) = index.get(&u) {
            result[i] = merge_software_item(&result[i], r);
        } else {
            index.insert(u, result.len());
            result.push(r.clone());
        }
    }
    apply_deletions(&mut result, tombstones);
    result
}

/// 按墓碑 uuid 过滤：墓碑时间 >= 记录更新时间 → 删掉；记录更新 → 视为
/// 「删除后又被改」（复活），保留。复活的墓碑无需清理：记录时间只会往后走，
/// 后续合并仍会判为复活（幂等）。
fn apply_deletions(records: &mut Vec<Value>, tombstones: &[Value]) {
    if tombstones.is_empty() {
        return;
    }
    records.retain(|rec| {
        let uuid = as_str(rec, "uuid");
        if uuid.is_empty() {
            return true;
        }
        match tombstones.iter().find(|m| as_str(m, "uuid") == uuid) {
            Some(m) => rec_time(m) < rec_time(rec),
            None => true,
        }
    });
}

pub fn merge_ignored(local: &[Value], remote: &[Value]) -> Vec<Value> {
    let mut out = local.to_vec();
    for r in remote {
        let uuid = as_str(r, "uuid");
        if uuid.is_empty() {
            // 无 uuid 的历史墓碑：按整条去重即可，只参与扫描压制。
            if !out.contains(r) {
                out.push(r.clone());
            }
            continue;
        }
        match out.iter().position(|x| as_str(x, "uuid") == uuid) {
            Some(i) => {
                // 同一实体的墓碑：机器条目按 machine_id 并集，每台机器内 paths 也并集；
                // name / deleted_at 取较新的一条作为底子（否则较旧一侧的机器会被丢掉）。
                let machines = merge_tombstone_machines(&out[i], r);
                if rec_time(r) > rec_time(&out[i]) {
                    out[i] = r.clone();
                }
                if let Some(o) = out[i].as_object_mut() {
                    o.insert("machines".to_string(), machines);
                }
            }
            None => out.push(r.clone()),
        }
    }
    out
}

/// 合并两个同名墓碑的 machines：按 machine_id 求并集，同机器内的 paths 求并集。
fn merge_tombstone_machines(a: &Value, b: &Value) -> Value {
    let mut out: Vec<Value> = a
        .get("machines")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(ms) = b.get("machines").and_then(|v| v.as_array()) {
        for m in ms {
            let mid = as_str(m, "machine_id");
            let dst = if mid.is_empty() {
                None
            } else {
                out.iter_mut().find(|x| as_str(x, "machine_id") == mid)
            };
            match dst {
                Some(dst) => {
                    let mut paths = dst
                        .get("paths")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    if let Some(rp) = m.get("paths").and_then(|v| v.as_array()) {
                        for p in rp {
                            if !paths.contains(p) {
                                paths.push(p.clone());
                            }
                        }
                    }
                    // name_only 是布尔标记：任一侧有就保留（或）
                    let name_only = dst
                        .get("name_only")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        || m.get("name_only").and_then(|v| v.as_bool()).unwrap_or(false);
                    if let Some(o) = dst.as_object_mut() {
                        o.insert("paths".to_string(), json!(paths));
                        if name_only {
                            o.insert("name_only".to_string(), json!(true));
                        }
                    }
                }
                None => out.push(m.clone()),
            }
        }
    }
    json!(out)
}

pub fn merge_keyed(local: &Value, remote: &Value) -> Value {
    let mut out = local.as_object().cloned().unwrap_or_default();
    if let Some(ro) = remote.as_object() {
        for (k, v) in ro {
            if !out.contains_key(k) {
                out.insert(k.clone(), v.clone());
                continue;
            }
            if rec_time(v) > rec_time(out.get(k).unwrap()) {
                out.insert(k.clone(), v.clone());
                continue;
            }
            // 保留本地，但补齐缺失字段
            if let (Some(co), Some(vo)) = (out.get_mut(k).and_then(|x| x.as_object_mut()), v.as_object()) {
                for (fk, fv) in vo {
                    let should = match co.get(fk) {
                        None => true,
                        Some(cv) => is_empty_val(cv) && !is_empty_val(fv),
                    };
                    if should {
                        co.insert(fk.clone(), fv.clone());
                    }
                }
            }
        }
    }
    Value::Object(out)
}

pub fn merge_config(local: &Value, remote: &Value) -> Value {
    let mut out = local.as_object().cloned().unwrap_or_default();
    let Some(ro) = remote.as_object() else {
        return Value::Object(out);
    };
    for (k, v) in ro {
        match k.as_str() {
            // 本机凭据，绝不参与同步
            "webdav" => {}
            "machine_aliases" => {
                let mut map = out
                    .get(k)
                    .and_then(|x| x.as_object())
                    .cloned()
                    .unwrap_or_default();
                if let Some(rm) = v.as_object() {
                    for (mk, mv) in rm {
                        if !map.contains_key(mk) {
                            map.insert(mk.clone(), mv.clone());
                        }
                    }
                }
                out.insert(k.clone(), Value::Object(map));
            }
            "scan_directories" => {
                let mut map = out
                    .get(k)
                    .and_then(|x| x.as_object())
                    .cloned()
                    .unwrap_or_default();
                if let Some(rm) = v.as_object() {
                    for (mk, mv) in rm {
                        let entry = map.entry(mk.clone()).or_insert(json!([]));
                        if entry.as_array().is_none() {
                            *entry = json!([]);
                        }
                        if let (Some(arr), Some(radd)) = (entry.as_array_mut(), mv.as_array()) {
                            for d in radd {
                                if !arr.contains(d) {
                                    arr.push(d.clone());
                                }
                            }
                        }
                    }
                }
                out.insert(k.clone(), Value::Object(map));
            }
            _ => {
                let should = match out.get(k) {
                    None => true,
                    Some(cv) => is_empty_val(cv) && !is_empty_val(v),
                };
                if should {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    if !out.contains_key("scan_directories") {
        out.insert("scan_directories".to_string(), json!({}));
    }
    Value::Object(out)
}

fn merge_json_file(rel: &str, local: &Value, remote: &Value) -> Value {
    match rel {
        "software.json" => {
            let l = local.as_array().cloned().unwrap_or_default();
            let r = remote.as_array().cloned().unwrap_or_default();
            // ignored.json 排序先于 software.json，此处读到的是本次已合并的墓碑。
            let tombstones = store::read_ignored();
            json!(merge_software_with_deletions(&l, &r, &tombstones))
        }
        "ignored.json" => {
            let l = local.as_array().cloned().unwrap_or_default();
            let r = remote.as_array().cloned().unwrap_or_default();
            json!(merge_ignored(&l, &r))
        }
        "extensions.json" | "browsers.json" => merge_keyed(local, remote),
        "config.json" => merge_config(local, remote),
        _ => local.clone(),
    }
}

// ---------------------------------------------------------------------------
// 传输
// ---------------------------------------------------------------------------

fn bump(c: &mut Value, key: &str) {
    if let Some(n) = c.get(key).and_then(|v| v.as_u64()) {
        c[key] = json!(n + 1);
    }
}

fn pull_merge(
    client: &WebDav,
    local: &Manifest,
    remote: &Manifest,
    baseline: &Manifest,
    c: &mut Value,
) -> Result<(), String> {
    let root = store::data_dir();
    let mut keys: BTreeSet<String> = local.files.keys().cloned().collect();
    keys.extend(remote.files.keys().cloned());

    for rel in keys {
        let l = local.files.get(&rel);
        let r = remote.files.get(&rel);
        let b = baseline.files.get(&rel);
        match (l, r) {
            (Some(le), Some(re)) => {
                if le.hash == re.hash {
                    continue;
                }
                // 3 路判断：只有双方相对基线都改了，才需要语义合并。
                // 单侧改动直接取该侧——这样「本地删除条目」不会被并集复活。
                let local_changed = b.map(|x| x.hash != le.hash).unwrap_or(true);
                let remote_changed = b.map(|x| x.hash != re.hash).unwrap_or(true);
                if is_json_ledger(&rel) {
                    if local_changed && remote_changed {
                        let path = root.join(&rel);
                        let local_val = store::read_json(&path);
                        let remote_bytes = client.get(&rel)?.unwrap_or_default();
                        let remote_val = serde_json::from_slice(&remote_bytes).unwrap_or(Value::Null);
                        let merged = merge_json_file(&rel, &local_val, &remote_val);
                        store::write_json(&path, &merged);
                        bump(c, "merged");
                    } else if remote_changed {
                        client.download_to(&rel, &root.join(&rel))?;
                        bump(c, "downloaded");
                    } else {
                        // 仅本地改动，保留本地，稍后推送
                    }
                } else if remote_changed && !local_changed {
                    client.download_to(&rel, &root.join(&rel))?;
                    bump(c, "downloaded");
                } else if local_changed && !remote_changed {
                    // 保留本地，稍后推送
                } else {
                    // 双方都变了：本地优先，稍后推送覆盖远端
                    bump(c, "conflicts");
                }
            }
            (Some(le), None) => {
                if let Some(be) = b {
                    if be.hash == le.hash {
                        // 远端删了且本地没动 → 本地同步删除
                        let _ = std::fs::remove_file(root.join(&rel));
                        bump(c, "deletedLocal");
                    }
                }
            }
            (None, Some(re)) => {
                if let Some(be) = b {
                    if be.hash == re.hash {
                        // 本地删了且远端没动 → 交给推送删除远端，不下载
                        continue;
                    }
                }
                client.download_to(&rel, &root.join(&rel))?;
                bump(c, "downloaded");
            }
            (None, None) => {}
        }
    }
    Ok(())
}

fn push(
    client: &WebDav,
    local: &Manifest,
    remote: &Manifest,
    prune: bool,
    c: &mut Value,
) -> Result<(), String> {
    let root = store::data_dir();
    for (rel, le) in &local.files {
        let need = match remote.files.get(rel) {
            Some(re) => re.hash != le.hash,
            None => true,
        };
        if !need {
            continue;
        }
        let path = root.join(rel);
        if is_json_ledger(rel) {
            let bytes = std::fs::read(&path).map_err(|e| format!("读取 {} 失败: {}", rel, e))?;
            client.put(rel, bytes)?;
        } else {
            client.put_file(rel, &path)?;
        }
        bump(c, "uploaded");
    }
    if prune {
        for rel in remote.files.keys() {
            if !local.files.contains_key(rel) {
                client.delete(rel)?;
                bump(c, "deletedRemote");
            }
        }
    }
    Ok(())
}

fn mirror_remote_to_local(client: &WebDav, remote: &Manifest, c: &mut Value) -> Result<(), String> {
    let root = store::data_dir();
    let local = build_manifest()?;
    for rel in local.files.keys() {
        if !remote.files.contains_key(rel) {
            let _ = std::fs::remove_file(root.join(rel));
            bump(c, "deletedLocal");
        }
    }
    for rel in remote.files.keys() {
        client.download_to(rel, &root.join(rel))?;
        bump(c, "downloaded");
    }
    prune_empty_dirs(&root);
    Ok(())
}

/// 删除 `icons/` `vault/` `evidence/` 下的空目录（镜像删除文件后会留下空壳）。
fn prune_empty_dirs(root: &Path) {
    for top in ["icons", "vault", "evidence"] {
        let base = root.join(top);
        if base.is_dir() {
            remove_empty(&base);
        }
    }
}

fn remove_empty(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            remove_empty(&p);
            let _ = std::fs::remove_dir(&p);
        }
    }
}

/// 强制覆盖前把本地可同步文件整体移入垃圾桶，保证「覆盖」也可回滚。
fn snapshot_local() -> Result<String, String> {
    let root = store::data_dir();
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let dest = store::trash_dir().join(format!("sync-{}", stamp));
    let m = build_manifest()?;
    for rel in m.files.keys() {
        let src = root.join(rel);
        let target = dest.join(rel);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(&src, &target);
    }
    prune_empty_dirs(&root);
    let _ = std::fs::write(
        dest.join("meta.json"),
        serde_json::to_string_pretty(&json!({"kind":"force-pull","original":"syncable/"})).unwrap(),
    );
    Ok(dest.to_string_lossy().to_string())
}

/// 强制覆盖前把远端现状移动到 `_backup/<时间戳>/`。
fn snapshot_remote(client: &WebDav, remote: &Manifest, c: &mut Value) {
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    for rel in remote.files.keys() {
        let target = format!("{}/{}/{}", BACKUP_DIR, stamp, rel);
        if client.move_to(rel, &target).is_ok() {
            bump(c, "backedUp");
        }
    }
    let _ = client.move_to(MANIFEST_REMOTE, &format!("{}/{}/{}", BACKUP_DIR, stamp, MANIFEST_REMOTE));
}

// ---------------------------------------------------------------------------
// 对外入口
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum SyncMode {
    Normal,
    ForcePush,
    ForcePull,
}

fn build_client() -> Result<WebDav, String> {
    let cfg = store::get_webdav_config();
    WebDav::new(
        &as_str(&cfg, "url"),
        &as_str(&cfg, "username"),
        &as_str(&cfg, "password"),
    )
}

fn enabled() -> bool {
    store::get_webdav_config()
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn finalize_remote_manifest(client: &WebDav, manifest: &Manifest) -> Result<(), String> {
    let mut m = manifest.clone();
    m.generated_at = chrono::Local::now().to_rfc3339();
    client.put(
        MANIFEST_REMOTE,
        serde_json::to_vec_pretty(&m).unwrap_or_default(),
    )
}

fn record_last(summary: &Value) {
    let _ = store::write_json(
        &store::last_sync_file(),
        &json!({
            "at": chrono::Local::now().to_rfc3339(),
            "machine": crate::commands::current_machine(),
            "summary": summary,
        }),
    );
}

/// 执行一次同步（阻塞）。`mode` 决定是否强制覆盖。
pub fn run_sync(mode: SyncMode) -> Value {
    if !enabled() {
        return json!({"success": false, "enabled": false, "error": "未启用 WebDAV 同步"});
    }
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => return json!({"success": false, "error": e}),
    };
    client.ensure_base();

    let mut c = json!({
        "downloaded": 0, "uploaded": 0, "deletedLocal": 0,
        "deletedRemote": 0, "merged": 0, "conflicts": 0, "backedUp": 0
    });

    let result: Result<(), String> = (|| {
        match mode {
            SyncMode::ForcePull => {
                snapshot_local()?;
                let remote = load_remote_manifest(&client)?.unwrap_or_default();
                mirror_remote_to_local(&client, &remote, &mut c)?;
            }
            SyncMode::ForcePush => {
                let remote = load_remote_manifest(&client)?.unwrap_or_default();
                snapshot_remote(&client, &remote, &mut c);
                let local = build_manifest()?;
                push(&client, &local, &Manifest::default(), false, &mut c)?;
            }
            SyncMode::Normal => {
                let local = build_manifest()?;
                let remote = load_remote_manifest(&client)?;
                let bootstrap = remote.is_none();
                let remote = remote.unwrap_or_default();
                // 远端 manifest 丢失时不信任旧基线，避免把本地数据当「远端已删」清掉
                let baseline = if bootstrap {
                    Manifest::default()
                } else {
                    load_baseline()
                };
                pull_merge(&client, &local, &remote, &baseline, &mut c)?;
                let local = build_manifest()?;
                let remote_fresh = load_remote_manifest(&client)?.unwrap_or_default();
                push(&client, &local, &remote_fresh, !bootstrap, &mut c)?;
            }
        }
        Ok(())
    })();

    if let Err(e) = result {
        return json!({"success": false, "error": e, "summary": c});
    }

    let final_manifest = match build_manifest() {
        Ok(m) => m,
        Err(e) => return json!({"success": false, "error": e, "summary": c}),
    };
    let _ = store::write_json(
        &store::baseline_manifest_file(),
        &serde_json::to_value(&final_manifest).unwrap_or(Value::Null),
    );
    if let Err(e) = finalize_remote_manifest(&client, &final_manifest) {
        return json!({"success": false, "error": e, "summary": c});
    }
    record_last(&c);
    json!({
        "success": true,
        "summary": c,
        "at": chrono::Local::now().to_rfc3339(),
        "totalFiles": final_manifest.files.len(),
    })
}

pub fn test_connection() -> Value {
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => return json!({"success": false, "error": e}),
    };
    client.ensure_base();
    match client.test() {
        Ok(_) => json!({"success": true}),
        Err(e) => json!({"success": false, "error": e}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(uuid: &str, name: &str, updated: &str, machine: &str, loc: &str) -> Value {
        json!({
            "uuid": uuid, "name": name, "updated_at": updated,
            "machines": [{ "machine_id": machine, "install_location": loc, "version": "1.0" }]
        })
    }

    /// 联网冒烟测试的凭据从环境变量读，避免把密码提交进仓库。
    /// 安全护栏：URL 必须指向专用测试目录（路径含 `ledgertest`），否则跳过，
    /// 防止有人对真实备份目录跑测试时被镜像推送误删。
    fn live_wd() -> Option<Value> {
        let url = std::env::var("LEDGER_TEST_WEBDAV_URL").ok()?;
        if url.is_empty() {
            return None;
        }
        if !url.contains("ledgertest") {
            eprintln!(
                "跳过联网测试：LEDGER_TEST_WEBDAV_URL 必须指向含 `ledgertest` 的专用测试目录，"
            );
            eprintln!("以免镜像推送误删真实备份。当前值: {}", url);
            return None;
        }
        Some(json!({
            "enabled": true,
            "url": url,
            "username": std::env::var("LEDGER_TEST_WEBDAV_USER").unwrap_or_default(),
            "password": std::env::var("LEDGER_TEST_WEBDAV_PASS").unwrap_or_default()
        }))
    }

    #[test]
    fn merge_software_is_union_and_keeps_other_machines() {
        let local = vec![item("u1", "A", "2026-01-01T00:00:00+08:00", "M1", "C:\\A")];
        let remote = vec![
            item("u2", "B", "2026-01-01T00:00:00+08:00", "M2", "D:\\B"),
            item("u1", "A", "2026-01-01T00:00:00+08:00", "M2", "D:\\A"),
        ];
        let merged = merge_software_with_deletions(&local, &remote, &[]);
        assert_eq!(merged.len(), 2, "同 uuid 合并、不同 uuid 并集");
        let a = merged.iter().find(|x| as_str(x, "uuid") == "u1").unwrap();
        assert_eq!(a["machines"].as_array().unwrap().len(), 2, "machines 按 machine_id 并集");
    }

    #[test]
    fn merge_software_newer_updated_at_wins_but_fills_blanks() {
        let local = json!([{ "uuid": "u1", "name": "A", "updated_at": "2026-01-02T00:00:00+08:00", "version": "2.0", "machines": [] }]);
        let remote = json!([{ "uuid": "u1", "name": "A", "updated_at": "2026-01-01T00:00:00+08:00", "version": "1.0", "download_url": "https://x", "machines": [] }]);
        let merged = merge_software_with_deletions(local.as_array().unwrap(), remote.as_array().unwrap(), &[]);
        assert_eq!(merged[0]["version"], "2.0", "较新的记录为准");
        assert_eq!(merged[0]["download_url"], "https://x", "新记录的空字段由旧记录补齐");
    }

    #[test]
    fn merge_software_applies_uuid_tombstones() {
        // 两端都改过 software.json 时，被删除/合并掉的条目必须按 uuid 删掉，不能并集复活。
        let local = json!([
            { "uuid": "keep", "name": "Keep", "updated_at": "2026-01-01T00:00:00+08:00", "machines": [] },
            { "uuid": "gone", "name": "Gone", "updated_at": "2026-01-01T00:00:00+08:00", "machines": [] }
        ]);
        let remote = json!([
            { "uuid": "keep", "name": "Keep", "updated_at": "2026-01-01T00:00:00+08:00", "machines": [] }
        ]);
        let tombstones = json!([{ "uuid": "gone", "name": "Gone", "deleted_at": "2026-01-02T00:00:00+08:00", "machines": [] }]);
        let merged = merge_software_with_deletions(
            local.as_array().unwrap(),
            remote.as_array().unwrap(),
            tombstones.as_array().unwrap(),
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["uuid"], "keep");
    }

    #[test]
    fn merge_software_revives_record_edited_after_delete() {
        // 删除之后又被改过（记录更新时间晚于删除时间）→ 复活，保留。
        let local = json!([{ "uuid": "x", "name": "X", "updated_at": "2026-01-03T00:00:00+08:00", "machines": [] }]);
        let tombstones = json!([{ "uuid": "x", "name": "X", "deleted_at": "2026-01-02T00:00:00+08:00", "machines": [] }]);
        let merged = merge_software_with_deletions(
            local.as_array().unwrap(),
            &[],
            tombstones.as_array().unwrap(),
        );
        assert_eq!(merged.len(), 1, "后改的应该复活");
    }

    #[test]
    fn merge_ignored_unions_machines_and_paths() {
        let local = json!([{
            "uuid": "a", "name": "A", "deleted_at": "2026-01-01T00:00:00+08:00",
            "machines": [{ "machine_id": "M1", "paths": ["c:\\a"] }]
        }]);
        let remote = json!([{
            "uuid": "a", "name": "A", "deleted_at": "2026-01-02T00:00:00+08:00",
            "machines": [
                { "machine_id": "M1", "paths": ["d:\\a"], "name_only": true },
                { "machine_id": "M2", "paths": ["c:\\a"] }
            ]
        }]);
        let merged = merge_ignored(local.as_array().unwrap(), remote.as_array().unwrap());
        assert_eq!(merged.len(), 1);
        // 较新一侧为底子，但机器并集不能丢：M1 的路径并集、M2 保留
        assert_eq!(merged[0]["deleted_at"], "2026-01-02T00:00:00+08:00");
        let ms = merged[0]["machines"].as_array().unwrap();
        assert_eq!(ms.len(), 2, "{:?}", merged[0]);
        let m1 = ms.iter().find(|m| m["machine_id"] == "M1").unwrap();
        assert_eq!(m1["paths"].as_array().unwrap().len(), 2);
        assert_eq!(m1["name_only"], json!(true), "name_only 并集：任一侧有就保留");
        assert!(ms.iter().any(|m| m["machine_id"] == "M2"));
    }

    #[test]
    fn merge_config_unions_machine_keyed_scan_dirs_and_skips_webdav() {
        let local = json!({
            "scan_directories": { "M1": ["D:\\Green"] },
            "machine_aliases": { "M1": "主机" },
            "llm_model": "",
            "webdav": { "password": "local-secret" }
        });
        let remote = json!({
            "scan_directories": { "M1": ["D:\\Green", "E:\\X"], "M2": ["F:\\Y"] },
            "machine_aliases": { "M2": "便携" },
            "llm_model": "deepseek-flash"
        });
        let merged = merge_config(&local, &remote);
        assert_eq!(merged["scan_directories"]["M1"].as_array().unwrap().len(), 2, "去重并集");
        assert_eq!(merged["scan_directories"]["M2"].as_array().unwrap().len(), 1);
        assert_eq!(merged["machine_aliases"]["M2"], "便携");
        assert_eq!(merged["llm_model"], "deepseek-flash", "本地为空则由远端补齐");
        assert_eq!(merged["webdav"]["password"], "local-secret", "webdav 凭据不参与同步");
    }

    #[test]
    fn merge_keyed_newer_wins() {
        let local = json!({ "id1": { "updated_at": "2026-01-01T00:00:00+08:00", "notes": "旧" } });
        let remote = json!({ "id1": { "updated_at": "2026-01-02T00:00:00+08:00", "notes": "新" }, "id2": { "notes": "新增" } });
        let merged = merge_keyed(&local, &remote);
        assert_eq!(merged["id1"]["notes"], "新");
        assert_eq!(merged["id2"]["notes"], "新增");
    }

    #[test]
    fn manifest_excludes_local_only_state() {
        let base = std::env::temp_dir().join(format!("ledger_sync_manifest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("data").join("trash")).unwrap();
        std::fs::create_dir_all(base.join("data").join(".sync")).unwrap();
        std::fs::write(base.join("data").join("software.json"), "[]").unwrap();
        std::fs::write(base.join("data").join("webdav.json"), "{}").unwrap();
        std::fs::write(base.join("data").join("trash").join("x.txt"), "x").unwrap();
        std::fs::write(base.join("data").join(".sync").join("manifest.json"), "{}").unwrap();
        fs_guard(&base, |_| {
            let m = build_manifest().unwrap();
            assert!(m.files.contains_key("software.json"));
            assert!(!m.files.contains_key("webdav.json"), "凭据不同步");
            assert!(!m.files.keys().any(|k| k.starts_with("trash/")), "垃圾桶不同步");
            assert!(!m.files.keys().any(|k| k.starts_with(".sync/")), "同步辅助目录不同步");
        });
        let _ = std::fs::remove_dir_all(&base);
    }

    fn fs_guard(root: &Path, f: impl FnOnce(&Path)) {
        let _g = crate::store::test_support::use_root(root);
        f(root);
    }

    #[test]
    #[ignore]
    fn live_sync_smoke() {
        let base = std::env::temp_dir().join("ledger_live_sync");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("data")).unwrap();
        let _g = crate::store::test_support::use_root(&base);
        let Some(wd) = live_wd() else { return };
        store::save_webdav_config(&wd);
        let s1 = r#"[{"uuid":"u1","name":"A","updated_at":"2026-01-01T00:00:00+08:00","machines":[{"machine_id":"M1","install_location":"C:\\A"}]}]"#;
        std::fs::write(base.join("data").join("software.json"), s1).unwrap();
        let r = run_sync(SyncMode::Normal);
        println!("SYNC1: {}", serde_json::to_string_pretty(&r).unwrap());
        assert_eq!(r["success"], true);

        let s2 = r#"[{"uuid":"u1","name":"A2","updated_at":"2026-02-01T00:00:00+08:00","machines":[{"machine_id":"M1","install_location":"C:\\A"}]},{"uuid":"u2","name":"B","updated_at":"2026-02-01T00:00:00+08:00","machines":[]}]"#;
        std::fs::write(base.join("data").join("software.json"), s2).unwrap();
        let r2 = run_sync(SyncMode::Normal);
        println!("SYNC2: {}", serde_json::to_string_pretty(&r2).unwrap());
        assert_eq!(r2["success"], true);

        let c = build_client().unwrap();
        let remote = load_remote_manifest(&c).unwrap().unwrap();
        assert!(remote.files.contains_key("software.json"));
        let bytes = c.get("software.json").unwrap().unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2, "远端应为合并后的 2 条");
        println!("REMOTE software.json = {}", v);
        // 清理远端测试产物，避免污染用户目录
        let _ = c.delete("software.json");
        let _ = c.delete("config.json");
        let _ = c.delete(MANIFEST_REMOTE);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    #[ignore]
    fn live_sync_join_smoke() {
        let root_a = std::env::temp_dir().join("ledger_join_a");
        let root_b = std::env::temp_dir().join("ledger_join_b");
        for r in [&root_a, &root_b] {
            let _ = std::fs::remove_dir_all(r);
            std::fs::create_dir_all(r.join("data")).unwrap();
        }
        let wd = {
            let Some(wd) = live_wd() else { return };
            wd
        };
        // A 机先扫到 u1 并同步（建立远端）
        {
            let _g = crate::store::test_support::use_root(&root_a);
            store::save_webdav_config(&wd);
            std::fs::write(
                root_a.join("data").join("software.json"),
                r#"[{"uuid":"u1","name":"A","updated_at":"2026-01-01T00:00:00+08:00","machines":[{"machine_id":"M-A","install_location":"C:\\A"}]}]"#,
            )
            .unwrap();
            let r = run_sync(SyncMode::Normal);
            assert_eq!(r["success"], true, "{} : {}", "A 首次推送", r);
        }
        // B 机本地已有自己的扫描（u3），首次接入应并集而不是被覆盖
        {
            let _g = crate::store::test_support::use_root(&root_b);
            store::save_webdav_config(&wd);
            std::fs::write(
                root_b.join("data").join("software.json"),
                r#"[{"uuid":"u3","name":"B","updated_at":"2026-01-02T00:00:00+08:00","machines":[{"machine_id":"M-B","install_location":"D:\\B"}]}]"#,
            )
            .unwrap();
            let r = run_sync(SyncMode::Normal);
            println!("JOIN: {}", serde_json::to_string_pretty(&r).unwrap());
            assert_eq!(r["success"], true);
            let local: Value = store::read_json(&root_b.join("data").join("software.json"));
            let arr = local.as_array().unwrap();
            assert_eq!(arr.len(), 2, "并集后本地应同时有 A 与 B 的条目: {}", local);
            assert!(arr.iter().any(|x| as_str(x, "uuid") == "u1"), "A 的条目应被拉下来");
        }
        // 清理远端
        {
            let _g = crate::store::test_support::use_root(&root_a);
            let c = build_client().unwrap();
            let _ = c.delete("software.json");
            let _ = c.delete(MANIFEST_REMOTE);
        }
        let _ = std::fs::remove_dir_all(&root_a);
        let _ = std::fs::remove_dir_all(&root_b);
    }

    #[test]
    #[ignore]
    fn live_delete_and_lock_smoke() {
        let root_a = std::env::temp_dir().join("ledger_del_a");
        let root_b = std::env::temp_dir().join("ledger_del_b");
        for r in [&root_a, &root_b] {
            let _ = std::fs::remove_dir_all(r);
            std::fs::create_dir_all(r.join("data")).unwrap();
        }
        let wd = {
            let Some(wd) = live_wd() else { return };
            wd
        };
        let two = r#"[{"uuid":"u1","name":"A","updated_at":"2026-01-01T00:00:00+08:00","machines":[]},{"uuid":"u2","name":"B","updated_at":"2026-01-01T00:00:00+08:00","machines":[]}]"#;
        let one = r#"[{"uuid":"u1","name":"A","updated_at":"2026-01-01T00:00:00+08:00","machines":[]}]"#;
        // A 推两份
        {
            let _g = crate::store::test_support::use_root(&root_a);
            store::save_webdav_config(&wd);
            std::fs::write(root_a.join("data").join("software.json"), two).unwrap();
            assert_eq!(run_sync(SyncMode::Normal)["success"], true);
        }
        // B 接入
        {
            let _g = crate::store::test_support::use_root(&root_b);
            store::save_webdav_config(&wd);
            std::fs::create_dir_all(root_b.join("data")).unwrap();
            assert_eq!(run_sync(SyncMode::Normal)["success"], true);
            let local: Value = store::read_json(&root_b.join("data").join("software.json"));
            assert_eq!(local.as_array().unwrap().len(), 2);
        }
        // A 删除 u2 后同步
        {
            let _g = crate::store::test_support::use_root(&root_a);
            std::fs::write(root_a.join("data").join("software.json"), one).unwrap();
            let r = run_sync(SyncMode::Normal);
            assert_eq!(r["success"], true);
        }
        // B 再同步，应跟着删掉 u2（删除传播）
        {
            let _g = crate::store::test_support::use_root(&root_b);
            let r = run_sync(SyncMode::Normal);
            println!("DEL: {}", serde_json::to_string_pretty(&r).unwrap());
            assert_eq!(r["success"], true);
            let local: Value = store::read_json(&root_b.join("data").join("software.json"));
            let arr = local.as_array().unwrap();
            assert_eq!(arr.len(), 1, "删除应传播到 B: {}", local);
            assert_eq!(as_str(&arr[0], "uuid"), "u1");
        }
        // 删除后同步（无锁）：不再做锁互斥测试。
        let _ = std::fs::remove_dir_all(&root_a);
        let _ = std::fs::remove_dir_all(&root_b);
    }

    #[test]
    #[ignore]
    fn live_force_overwrite_smoke() {
        let root_a = std::env::temp_dir().join("ledger_force_a");
        let root_b = std::env::temp_dir().join("ledger_force_b");
        for r in [&root_a, &root_b] {
            let _ = std::fs::remove_dir_all(r);
            std::fs::create_dir_all(r.join("data")).unwrap();
        }
        let Some(wd) = live_wd() else { return };
        let two = r#"[{"uuid":"u1","name":"A","updated_at":"2026-01-01T00:00:00+08:00","machines":[]},{"uuid":"u2","name":"B","updated_at":"2026-01-01T00:00:00+08:00","machines":[]}]"#;
        let three = r#"[{"uuid":"u3","name":"C","updated_at":"2026-02-01T00:00:00+08:00","machines":[]}]"#;
        // A 建立远端（u1,u2）
        {
            let _g = crate::store::test_support::use_root(&root_a);
            store::save_webdav_config(&wd);
            std::fs::write(root_a.join("data").join("software.json"), two).unwrap();
            assert_eq!(run_sync(SyncMode::Normal)["success"], true);
        }
        // B 强制拉取：本地应变成远端（u1,u2），并留下本地快照
        {
            let _g = crate::store::test_support::use_root(&root_b);
            store::save_webdav_config(&wd);
            // 放一条本地数据，验证强制拉取前会把它快照进 trash
            std::fs::write(
                root_b.join("data").join("software.json"),
                r#"[{"uuid":"zombie","name":"将被覆盖","updated_at":"2026-01-01T00:00:00+08:00","machines":[]}]"#,
            )
            .unwrap();
            let r = run_sync(SyncMode::ForcePull);
            println!("FORCE PULL: {}", serde_json::to_string_pretty(&r).unwrap());
            assert_eq!(r["success"], true);
            let local: Value = store::read_json(&root_b.join("data").join("software.json"));
            assert_eq!(local.as_array().unwrap().len(), 2, "强制拉取后本地=远端");
            assert!(std::fs::read_dir(crate::store::trash_dir())
                .map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("sync-")))
                .unwrap_or(false), "强制拉取前应快照本地");
        }
        // B 改成 u3 后强制推送：远端应只剩 u3，并留下 _backup
        {
            let _g = crate::store::test_support::use_root(&root_b);
            std::fs::write(root_b.join("data").join("software.json"), three).unwrap();
            let r = run_sync(SyncMode::ForcePush);
            println!("FORCE PUSH: {}", serde_json::to_string_pretty(&r).unwrap());
            assert_eq!(r["success"], true);
            let c = build_client().unwrap();
            let bytes = c.get("software.json").unwrap().unwrap();
            let v: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v.as_array().unwrap().len(), 1);
            assert_eq!(as_str(&v.as_array().unwrap()[0], "uuid"), "u3");
        }
        // A 强制拉取：应变成 u3
        {
            let _g = crate::store::test_support::use_root(&root_a);
            let r = run_sync(SyncMode::ForcePull);
            assert_eq!(r["success"], true);
            let local: Value = store::read_json(&root_a.join("data").join("software.json"));
            let arr = local.as_array().unwrap();
            assert_eq!(arr.len(), 1);
            assert_eq!(as_str(&arr[0], "uuid"), "u3");
            // 清理远端（含快照目录）
            let c = build_client().unwrap();
            let _ = c.delete("software.json");
            let _ = c.delete(MANIFEST_REMOTE);
            let _ = c.delete(BACKUP_DIR);
        }
        let _ = std::fs::remove_dir_all(&root_a);
        let _ = std::fs::remove_dir_all(&root_b);
    }
}

