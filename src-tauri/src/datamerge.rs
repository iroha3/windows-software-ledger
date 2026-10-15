// 手动合并：把另一台机器的 `data/` 目录并入本机台账。
//
// 场景：两台 PC 各用各的 `data/`（没配 WebDAV），想把它们并成一份。
// 语义与 `scripts/combine_data.py` 一致，直接复用 `sync.rs` 里同步时的合并函数：
//   - 软件 / 扩展 / 浏览器标注：按 uuid 纯并集；同名保留为多条（要不要并，用户在 app 里决定）
//   - 墓碑 / 配置：按 uuid / 主机名并集（webdav 凭据不参与）
//   - 图标 / 保管箱 / 证据：只补缺，不覆盖、不删除
// 合并前把本机 JSON 台账快照到 `data/trash/merge-import-<时间>-<随机>/`，可回滚。

use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::store;
use crate::sync;

/// 允许选 exe 目录（含 `data/`）或直接选 `data/`，都归一到真正的数据目录。
fn resolve_source(source: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(source.trim());
    if !p.is_dir() {
        return Err(format!("路径不存在或不是文件夹：{}", p.display()));
    }
    let nested = p.join("data");
    if nested.join("software.json").is_file() {
        return Ok(nested);
    }
    if p.join("software.json").is_file() {
        return Ok(p);
    }
    Err("没找到 software.json：请选另一台机器的 data 文件夹（或含 data 的 exe 目录）".to_string())
}

fn read_json_opt(path: &Path) -> Option<Value> {
    path.is_file().then(|| store::read_json(path))
}

fn read_array(path: &Path) -> Result<Vec<Value>, String> {
    match read_json_opt(path) {
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(format!("{} 不是数组", path.display())),
        None => Ok(vec![]),
    }
}

fn uuid_of(v: &Value) -> String {
    v.get("uuid").and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn same_name_key(v: &Value) -> String {
    v.get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

fn uuid_set(items: &[Value]) -> HashSet<String> {
    items.iter().map(uuid_of).filter(|s| !s.is_empty()).collect()
}

/// 只数「本地没有的新 uuid」。同名但 uuid 不同算新增（会被保留为多条）。
fn count_new_software(local: &[Value], incoming: &[Value]) -> usize {
    let seen = uuid_set(local);
    incoming
        .iter()
        .filter(|v| {
            let u = uuid_of(v);
            !u.is_empty() && !seen.contains(&u)
        })
        .count()
}

/// 同名但不同 uuid 的条数：合并不动它们，只提示用户在 app 里决定要不要并。
fn count_same_name_hints(local: &[Value], incoming: &[Value]) -> usize {
    let local_names: HashSet<String> = local
        .iter()
        .map(same_name_key)
        .filter(|s| !s.is_empty())
        .collect();
    let seen = uuid_set(local);
    incoming
        .iter()
        .filter(|v| {
            let key = same_name_key(v);
            !key.is_empty() && !seen.contains(&uuid_of(v)) && local_names.contains(&key)
        })
        .count()
}

fn count_new_keys(local: &Value, incoming: &Value) -> usize {
    let mut n = 0;
    if let Some(inc) = incoming.as_object() {
        let empty = serde_json::Map::new();
        let loc = local.as_object().unwrap_or(&empty);
        for k in inc.keys() {
            if !loc.contains_key(k) {
                n += 1;
            }
        }
    }
    n
}

struct Incoming {
    software: Vec<Value>,
    ignored: Vec<Value>,
    extensions: Value,
    browsers: Value,
    config: Option<Value>,
}

fn load_incoming(src: &Path) -> Result<Incoming, String> {
    let software = read_array(&src.join("software.json"))?;
    let missing = software
        .iter()
        .filter(|v| uuid_of(v).is_empty())
        .count();
    if missing > 0 {
        return Err(format!(
            "源台账有 {} 条记录缺 uuid（旧版本数据）。请先用当前版本程序打开该 data/，\
             或运行 scripts/migrate_to_v2.py 迁移后再合并。",
            missing
        ));
    }
    Ok(Incoming {
        software,
        ignored: read_array(&src.join("ignored.json"))?,
        extensions: read_json_opt(&src.join("extensions.json")).unwrap_or_else(|| json!({})),
        browsers: read_json_opt(&src.join("browsers.json")).unwrap_or_else(|| json!({})),
        config: read_json_opt(&src.join("config.json")),
    })
}

/// 合并前把本机 JSON 台账快照到 `data/trash/merge-import-<时间>-<随机>/`。
fn snapshot_ledgers() -> String {
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let rand = store::new_uuid();
    let slot = store::trash_dir().join(format!(
        "merge-import-{}-{}",
        stamp,
        &rand[..8.min(rand.len())]
    ));
    if std::fs::create_dir_all(&slot).is_err() {
        return String::new();
    }
    for (from, name) in [
        (store::software_file(), "software.json"),
        (store::ignored_file(), "ignored.json"),
        (store::extensions_file(), "extensions.json"),
        (store::browsers_file(), "browsers.json"),
        (store::config_file(), "config.json"),
    ] {
        if from.is_file() {
            let _ = std::fs::copy(&from, slot.join(name));
        }
    }
    let meta = json!({
        "reason": "merge-import",
        "at": chrono::Local::now().to_rfc3339(),
    });
    let _ = std::fs::write(
        slot.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    );
    slot.to_string_lossy().to_string()
}

/// 递归把 src 下的文件补进 dst：已存在的不覆盖（图标 / 保管箱 / 证据都只增不改）。
fn copy_missing(src: &Path, dst: &Path) -> u64 {
    if !src.is_dir() {
        return 0;
    }
    let mut copied = 0u64;
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((s, d)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&s) else {
            continue;
        };
        for e in rd.flatten() {
            let sp = e.path();
            let dp = d.join(e.file_name());
            if sp.is_dir() {
                stack.push((sp, dp));
            } else if sp.is_file() && !dp.exists() {
                if let Some(parent) = dp.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::copy(&sp, &dp).is_ok() {
                    copied += 1;
                }
            }
        }
    }
    copied
}

/// 入口：`apply=false` 只预览（不落盘），`apply=true` 才真正合并。
pub fn merge_from_dir(source: &str, apply: bool) -> Value {
    let src = match resolve_source(source) {
        Ok(p) => p,
        Err(e) => return json!({ "success": false, "error": e }),
    };
    if src == store::data_dir() {
        return json!({ "success": false, "error": "这就是本机自己的 data 目录，不用并" });
    }
    let incoming = match load_incoming(&src) {
        Ok(v) => v,
        Err(e) => return json!({ "success": false, "error": e }),
    };

    let local_software = store::read_software();
    let local_ext = store::read_extensions();
    let local_browsers = store::read_browsers();
    let local_ignored = store::read_ignored();

    let preview = json!({
        "source": src.to_string_lossy(),
        "newSoftware": count_new_software(&local_software, &incoming.software),
        "sameNameHints": count_same_name_hints(&local_software, &incoming.software),
        "totalSoftware": incoming.software.len(),
        "newExtensions": count_new_keys(&local_ext, &incoming.extensions),
        "newBrowsers": count_new_keys(&local_browsers, &incoming.browsers),
        "newTombstones": count_new_software(&local_ignored, &incoming.ignored),
    });
    if !apply {
        return json!({ "success": true, "apply": false, "preview": preview });
    }

    let _w = store::lock_writes();
    let snapshot = snapshot_ledgers();

    // 顺序与同步一致：先并墓碑，再并软件（用并好的墓碑过滤被删/被并条目）。
    let merged_ignored = sync::merge_ignored(&local_ignored, &incoming.ignored);
    store::write_ignored(&merged_ignored);
    let merged_software =
        sync::merge_software_with_deletions(&local_software, &incoming.software, &merged_ignored);
    store::write_software(&merged_software);

    store::write_extensions(&sync::merge_keyed(&local_ext, &incoming.extensions));
    store::write_browsers(&sync::merge_keyed(&local_browsers, &incoming.browsers));

    if let Some(inc_cfg) = &incoming.config {
        let merged = sync::merge_config(&store::get_config(), inc_cfg);
        store::save_config(&merged);
    }

    let files = json!({
        "icons": copy_missing(&src.join("icons"), &store::icons_dir()),
        "vault": copy_missing(&src.join("vault"), &store::vault_dir()),
        "evidence": copy_missing(&src.join("evidence"), &store::evidence_dir()),
    });

    json!({
        "success": true,
        "apply": true,
        "preview": preview,
        "files": files,
        "snapshot": snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(uuid: &str, name: &str, machine: &str) -> Value {
        json!({
            "uuid": uuid, "id": "SW-001", "name": name,
            "restore_intent": "unreviewed", "backup_strategy": "none",
            "machines": [{ "machine_id": machine, "install_location": "C:\\X" }]
        })
    }

    #[test]
    fn merge_unions_by_uuid_and_keeps_same_name_separate() {
        let root = std::env::temp_dir().join(format!("ledger_dm_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let local_data = root.join("data");
        let src = root.join("other").join("data");
        std::fs::create_dir_all(&local_data).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        // 本机：A(u1) 与 Chrome(u2)
        std::fs::write(
            local_data.join("software.json"),
            serde_json::to_string(&json!([item("u1", "A", "M1"), item("u2", "Chrome", "M1")]))
                .unwrap(),
        )
        .unwrap();
        std::fs::write(local_data.join("config.json"), r#"{"scan_directories":{"M1":["D:\\G"]}}"#).unwrap();
        // 另一端：Chrome(u3，同名不同 uuid) 与 B(u4)
        std::fs::write(
            src.join("software.json"),
            serde_json::to_string(&json!([item("u3", "Chrome", "M2"), item("u4", "B", "M2")]))
                .unwrap(),
        )
        .unwrap();
        std::fs::write(src.join("config.json"), r#"{"scan_directories":{"M2":["E:\\S"]},"machine_aliases":{"M2":"便携"}}"#).unwrap();
        // 图标只增
        std::fs::create_dir_all(src.join("icons")).unwrap();
        std::fs::write(src.join("icons").join("x.png"), "png").unwrap();

        let _g = store::test_support::use_root(&root);
        // 预览不动数据
        let pre = merge_from_dir(src.to_str().unwrap(), false);
        assert_eq!(pre["success"], true);
        assert_eq!(pre["preview"]["newSoftware"], 2, "{}", pre);
        assert_eq!(pre["preview"]["sameNameHints"], 1, "Chrome 同名提示");
        assert_eq!(store::read_software().len(), 2, "预览不落盘");

        let res = merge_from_dir(src.to_str().unwrap(), true);
        assert_eq!(res["success"], true, "{}", res);
        let merged = store::read_software();
        assert_eq!(merged.len(), 4, "uuid 并集：同名两条都保留");
        assert!(merged.iter().any(|s| uuid_of(s) == "u4"));
        // 扫描目录按主机名并集
        let cfg = store::get_config();
        assert_eq!(cfg["scan_directories"]["M1"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["scan_directories"]["M2"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["machine_aliases"]["M2"], "便携");
        // 图标补进来了
        assert!(store::icons_dir().join("x.png").is_file());
        // 有快照
        assert!(std::fs::read_dir(store::trash_dir())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("merge-import-")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_legacy_data_without_uuid() {
        let root = std::env::temp_dir().join(format!("ledger_dm_legacy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        let src = root.join("old").join("data");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("software.json"), r#"[{"id":"SW-001","name":"旧版"}]"#).unwrap();
        let _g = store::test_support::use_root(&root);
        let res = merge_from_dir(src.to_str().unwrap(), true);
        assert_eq!(res["success"], false);
        assert!(res["error"].as_str().unwrap().contains("缺 uuid"), "{}", res);
        let _ = std::fs::remove_dir_all(&root);
    }
}
