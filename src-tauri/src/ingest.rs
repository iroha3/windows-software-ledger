use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::store;

static NOISE_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(update|uninstall|unins\d*|卸载|microsoft visual c\+\+ \d{4}-\d{4} redistributable|windows sdk|microsoft\.net|directx|vulkan run time)").unwrap()
});
static NOISE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(unins\d*\.exe|uninstall\.exe|helper\.exe|crashpad_handler\.exe)$").unwrap()
});
static CLEAN_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*\(x64\)|\s*\(64-bit\)|\s*\(32-bit\)|\s*\(User\)|\s*版本\s*[\d\.]+").unwrap()
});

struct Rule {
    re: Regex,
    name: &'static str,
    category: &'static str,
    kind: &'static str,
    url: &'static str,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let raw: &[(&str, &str, &str, &str, &str)] = &[
        (r"(?i)7-zip", "7-Zip", "系统工具", "desktop", "https://www.7-zip.org/"),
        (r"(?i)visual studio code|vscode", "Visual Studio Code", "开发工具", "desktop", "https://code.visualstudio.com/"),
        (r"(?i)antigravity ide", "Antigravity IDE", "开发工具", "desktop", ""),
        (r"(?i)git", "Git", "开发工具", "cli", "https://git-scm.com/"),
        (r"(?i)firefox", "Firefox", "浏览器与网络", "desktop", "https://www.mozilla.org/firefox/"),
        (r"(?i)google chrome|chrome", "Google Chrome", "浏览器与网络", "desktop", "https://www.google.com/chrome/"),
        (r"(?i)node\.js|nodejs", "Node.js", "开发工具", "runtime", "https://nodejs.org/"),
        (r"(?i)python", "Python", "开发工具", "runtime", "https://www.python.org/"),
        (r"(?i)bun", "Bun", "开发工具", "runtime", "https://bun.sh/"),
        (r"(?i)rust", "Rust (rustup/cargo)", "开发工具", "runtime", "https://www.rust-lang.org/"),
        (r"(?i)everything", "Everything", "系统工具", "desktop", "https://www.voidtools.com/"),
        (r"(?i)potplayer", "PotPlayer", "媒体娱乐", "desktop", ""),
        (r"(?i)vlc", "VLC Media Player", "媒体娱乐", "desktop", "https://www.videolan.org/"),
        (r"(?i)mpc-be", "MPC-BE", "媒体娱乐", "desktop", ""),
        (r"(?i)honeyview", "Honeyview", "媒体娱乐", "desktop", ""),
        (r"(?i)snipaste", "Snipaste", "系统工具", "desktop", "https://zh.snipaste.com/"),
        (r"(?i)pixpin", "PixPin", "系统工具", "desktop", ""),
        (r"(?i)obsidian", "Obsidian", "办公与笔记", "desktop", "https://obsidian.md/"),
        (r"(?i)notion", "Notion", "办公与笔记", "desktop", "https://www.notion.so/"),
        (r"(?i)cherry-studio", "Cherry Studio", "开发工具", "desktop", ""),
        (r"(?i)dbeaver", "DBeaver", "开发工具", "desktop", "https://dbeaver.io/"),
        (r"(?i)navicat", "Navicat", "开发工具", "desktop", ""),
        (r"(?i)docker", "Docker Desktop", "开发工具", "desktop", "https://www.docker.com/"),
        (r"(?i)qbittorrent", "qBittorrent", "浏览器与网络", "desktop", "https://www.qbittorrent.org/"),
        (r"(?i)steam", "Steam", "媒体娱乐", "desktop", "https://store.steampowered.com/"),
        (r"(?i)wechat|微信", "微信 (WeChat)", "通讯与社交", "desktop", ""),
        (r"(?i)telegram", "Telegram", "通讯与社交", "desktop", "https://telegram.org/"),
        (r"(?i)qq", "QQ", "通讯与社交", "desktop", ""),
        (r"图吧工具箱", "图吧工具箱", "系统工具", "portable", ""),
        (r"(?i)hibit uninstaller", "HiBit Uninstaller", "系统工具", "desktop", ""),
        (r"(?i)angry ip scanner", "Angry IP Scanner", "系统工具", "desktop", ""),
        (r"(?i)afterchat", "AfterChat", "开发工具", "desktop", ""),
    ];
    raw.iter()
        .map(|(re, name, cat, kind, url)| Rule {
            re: Regex::new(re).unwrap(),
            name,
            category: cat,
            kind,
            url,
        })
        .collect()
});

fn guess_category(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("sdk") || n.contains("compiler") || n.contains("git") || n.contains("code")
        || n.contains("ide") || n.contains("node") || n.contains("python")
    {
        return "开发工具";
    }
    if n.contains("player") || n.contains("media") || n.contains("music") || n.contains("video")
        || n.contains("audio") || n.contains("game")
    {
        return "媒体娱乐";
    }
    if n.contains("browser") || n.contains("torrent") || n.contains("download")
        || n.contains("network") || n.contains("ssh") || n.contains("ftp")
    {
        return "浏览器与网络";
    }
    if n.contains("note") || n.contains("office") || n.contains("pdf") || n.contains("doc")
        || n.contains("excel")
    {
        return "办公与笔记";
    }
    "系统工具"
}

struct Norm {
    name: String,
    category: String,
    kind: String,
    download_url: String,
}

fn normalize_name(raw_name: &str) -> Norm {
    let clean = CLEAN_SUFFIX.replace_all(raw_name.trim(), "").trim().to_string();

    for rule in RULES.iter() {
        if rule.re.is_match(&clean) {
            return Norm {
                name: rule.name.to_string(),
                category: rule.category.to_string(),
                kind: rule.kind.to_string(),
                download_url: rule.url.to_string(),
            };
        }
    }

    Norm {
        name: clean.clone(),
        category: guess_category(&clean).to_string(),
        kind: "desktop".to_string(),
        download_url: String::new(),
    }
}

fn update_machine(item: &mut Value, machine: Value) {
    let machine_id = machine
        .get("machine_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if let Some(arr) = item.get_mut("machines").and_then(|m| m.as_array_mut()) {
        let pos = arr.iter().position(|m| {
            m.get("machine_id").and_then(|v| v.as_str()) == Some(machine_id.as_str())
        });
        match pos {
            Some(p) => {
                let ex = &mut arr[p];
                if ex.get("version").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
                    if let Some(v) = machine.get("version").and_then(|x| x.as_str()) {
                        if !v.is_empty() {
                            ex["version"] = json!(v);
                        }
                    }
                }
                if ex.get("install_location").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
                    if let Some(v) = machine.get("install_location").and_then(|x| x.as_str()) {
                        if !v.is_empty() {
                            ex["install_location"] = json!(v);
                        }
                    }
                }
                if machine.get("form").and_then(|v| v.as_str()) == Some("portable") {
                    ex["form"] = json!("portable");
                }
            }
            None => arr.push(machine),
        }
    }
}

fn add_or_update(
    items: &mut Vec<Value>,
    index: &mut HashMap<String, usize>,
    next_id: &mut u64,
    raw_name: &str,
    machine: Value,
    forced_type: Option<&str>,
    icon_path: &str,
) {
    let norm = normalize_name(raw_name);
    let key = norm.name.to_lowercase().trim().to_string();
    // 绿色/便携软件无需推断处置方式：直接就是「保留/压缩整个目录」。
    let is_portable = forced_type == Some("portable");
    let default_strategy = if is_portable { "copy_dir" } else { "none" };

    if let Some(&i) = index.get(&key) {
        let item = &mut items[i];
        if item.get("version").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            if let Some(v) = machine.get("version").and_then(|x| x.as_str()) {
                if !v.is_empty() {
                    item["version"] = json!(v);
                }
            }
        }
        // 已存在的便携项：仅当用户尚未设置（空 / none）时补成 copy_dir，不覆盖人工选择。
        if is_portable {
            let current = item.get("backup_strategy").and_then(|v| v.as_str()).unwrap_or("");
            if current.is_empty() || current == "none" {
                item["backup_strategy"] = json!("copy_dir");
            }
        }
        update_machine(item, machine);
        if icon_path.is_empty() {
            return;
        }
        if item.get("icon_path").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            item["icon_path"] = json!(icon_path);
        }
        return;
    }

    *next_id += 1;
    let id = format!("SW-{:03}", next_id);
    let mut item = json!({
        "id": id,
        "name": norm.name,
        "category": norm.category,
        "type": forced_type.unwrap_or(norm.kind.as_str()),
        "version": machine.get("version").and_then(|v| v.as_str()).unwrap_or(""),
        "machines": [],
        "restore_intent": "unreviewed",
        "backup_strategy": default_strategy,
        "prep_status": "todo",
        "has_config": false,
        "download_url": norm.download_url,
        "config_notes": "",
        "is_awesome": false,
        "awesome_role": "",
        "icon_path": icon_path,
        "created_at": chrono::Local::now().to_rfc3339(),
    });
    update_machine(&mut item, machine);
    index.insert(key, items.len());
    items.push(item);
}

/// 路径归一化：统一分隔符、小写、去尾斜杠，用于「完全相等」判定。
/// 只做等价比较，不做模糊匹配；识别不出就当新条目（符合「尽力保证」）。
pub fn normalize_path(p: &str) -> String {
    let mut s = p.trim().replace('/', "\\").to_lowercase();
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// 把证据目录里的 `icon_file` 拼成绝对路径，交给后续导入时复制。
fn evidence_icon_path(mdir: &Path, entry: &Value) -> String {
    let file = entry.get("icon_file").and_then(|v| v.as_str()).unwrap_or("");
    if file.is_empty() {
        return String::new();
    }
    mdir.join("app-icons")
        .join(file)
        .to_string_lossy()
        .to_string()
}

/// 解析指定机器目录下的证据，聚合成候选条目的原始列表（尚未做已知/墓碑判定）。
fn parse_evidence(
    evidence_root: &Path,
    machines: &[String],
    items: &mut Vec<Value>,
    index: &mut HashMap<String, usize>,
    next_id: &mut u64,
) {
    for machine_id in machines {
        let mdir = evidence_root.join(machine_id);

        // A. 注册表安装项
        if let Value::Array(apps) = store::read_json(&mdir.join("registry-apps.json")) {
            for app in apps {
                let name = app.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() || NOISE_NAME.is_match(name) {
                    continue;
                }
                let machine = json!({
                    "machine_id": machine_id,
                    "form": "installed",
                    "version": app.get("version").and_then(|v| v.as_str()).unwrap_or(""),
                    "install_location": app.get("install_location").and_then(|v| v.as_str()).unwrap_or(""),
                    "publisher": app.get("publisher").and_then(|v| v.as_str()).unwrap_or(""),
                });
                add_or_update(items, index, next_id, name, machine, None, &evidence_icon_path(&mdir, &app));
            }
        }

        // B. 便携软件扫描
        if let Value::Array(ports) = store::read_json(&mdir.join("portable-apps.json")) {
            for port in ports {
                let name = port.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let machine = json!({
                    "machine_id": machine_id,
                    "form": "portable",
                    "version": port.get("version").and_then(|v| v.as_str()).unwrap_or(""),
                    "install_location": port.get("folder_path").and_then(|v| v.as_str()).unwrap_or(""),
                    "main_exe": port.get("main_exe").and_then(|v| v.as_str()).unwrap_or(""),
                });
                add_or_update(
                    items,
                    index,
                    next_id,
                    name,
                    machine,
                    Some("portable"),
                    &evidence_icon_path(&mdir, &port),
                );
            }
        }

        // C. 桌面与开始菜单快捷方式
        if let Value::Array(scs) = store::read_json(&mdir.join("shortcuts.json")) {
            for sc in scs {
                let name = sc.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let target = sc.get("target_path").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() || target.is_empty() {
                    continue;
                }
                if NOISE_PATH.is_match(target) || NOISE_NAME.is_match(name) {
                    continue;
                }
                if name.starts_with("卸载") || name.to_lowercase().contains("uninstall") {
                    continue;
                }
                let form = if target.to_lowercase().contains("portable") {
                    "portable"
                } else {
                    "shortcut"
                };
                let machine = json!({
                    "machine_id": machine_id,
                    "form": form,
                    "install_location": target,
                    "link_file": sc.get("link_file").and_then(|v| v.as_str()).unwrap_or(""),
                });
                add_or_update(items, index, next_id, name, machine, None, &evidence_icon_path(&mdir, &sc));
            }
        }
    }
}

fn candidate_paths(item: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
        for m in ms {
            let loc = {
                let a = m.get("install_location").and_then(|v| v.as_str()).unwrap_or("");
                if a.is_empty() {
                    m.get("path").and_then(|v| v.as_str()).unwrap_or("")
                } else {
                    a
                }
            };
            let p = normalize_path(loc);
            if !p.is_empty() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn candidate_machine_ids(item: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
        for m in ms {
            let mid = m.get("machine_id").and_then(|v| v.as_str()).unwrap_or("");
            if !mid.is_empty() && !out.iter().any(|x| x == mid) {
                out.push(mid.to_string());
            }
        }
    }
    out
}

/// 已知判定：已有条目包含「本次机器」的记录，且名称相同 或 路径完全相等。
/// 命中则视为已知，扫描对它零改动。
fn find_known<'a>(candidate: &Value, existing: &'a [Value]) -> Option<&'a Value> {
    let cand_name = candidate
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase()
        .trim()
        .to_string();
    let cand_paths = candidate_paths(candidate);
    let cand_machines = candidate_machine_ids(candidate);

    for ex in existing {
        let ex_name = ex
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase()
            .trim()
            .to_string();
        let Some(ms) = ex.get("machines").and_then(|m| m.as_array()) else {
            continue;
        };
        for m in ms {
            let mid = m.get("machine_id").and_then(|v| v.as_str()).unwrap_or("");
            if !cand_machines.iter().any(|x| x == mid) {
                continue;
            }
            let loc = {
                let a = m.get("install_location").and_then(|v| v.as_str()).unwrap_or("");
                if a.is_empty() {
                    m.get("path").and_then(|v| v.as_str()).unwrap_or("")
                } else {
                    a
                }
            };
            let same_name = !cand_name.is_empty() && cand_name == ex_name;
            let same_path = {
                let p = normalize_path(loc);
                !p.is_empty() && cand_paths.iter().any(|x| *x == p)
            };
            if same_name || same_path {
                return Some(ex);
            }
        }
    }
    None
}

fn is_known(candidate: &Value, existing: &[Value]) -> bool {
    find_known(candidate, existing).is_some()
}

/// 已有条目缺图标的补充来源：返回 (SW-ID, 图标绝对路径)。
/// 这些条目不在候选列表里（已知即跳过），但重扫时仍应把图标补齐。
pub fn known_icon_refreshes(
    evidence_root: &Path,
    machines: &[String],
    existing: &[Value],
) -> Vec<(String, String)> {
    let mut items: Vec<Value> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut next_id = 0u64;
    parse_evidence(evidence_root, machines, &mut items, &mut index, &mut next_id);

    let mut out: Vec<(String, String)> = Vec::new();
    for item in &items {
        let icon = item.get("icon_path").and_then(|v| v.as_str()).unwrap_or("");
        if icon.is_empty() {
            continue;
        }
        if let Some(ex) = find_known(item, existing) {
            let id = ex.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !id.is_empty() && !out.iter().any(|(i, _)| i == id) {
                out.push((id.to_string(), icon.to_string()));
            }
        }
    }
    out
}

/// 墓碑判定：名称或路径命中「已删除」记录。
fn tombstone_match(candidate: &Value, ignored: &[Value]) -> bool {
    let cand_name = candidate
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase()
        .trim()
        .to_string();
    let cand_paths = candidate_paths(candidate);
    for g in ignored {
        let key = g.get("match_key").and_then(|v| v.as_str()).unwrap_or("");
        if !cand_name.is_empty() && key == cand_name {
            return true;
        }
        if let Some(paths) = g.get("paths").and_then(|v| v.as_array()) {
            for gp in paths.iter().filter_map(|p| p.as_str()) {
                if cand_paths.iter().any(|x| *x == gp) {
                    return true;
                }
            }
        }
    }
    false
}

/// 解析证据目录，产出扫描候选：未知项 + 墓碑项。已有条目在此阶段零改动。
pub fn build_candidates(
    evidence_root: &Path,
    machines: &[String],
    existing: &[Value],
    ignored: &[Value],
) -> Vec<Value> {
    let mut items: Vec<Value> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut next_id = 0u64;
    parse_evidence(evidence_root, machines, &mut items, &mut index, &mut next_id);

    let mut candidates: Vec<Value> = Vec::new();
    for mut item in items {
        if is_known(&item, existing) {
            continue;
        }
        let kind = if tombstone_match(&item, ignored) {
            "deleted_before"
        } else {
            "new"
        };
        let key = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase()
            .trim()
            .to_string();
        if let Some(obj) = item.as_object_mut() {
            obj.remove("id");
            obj.insert("key".to_string(), json!(key));
            obj.insert("kind".to_string(), json!(kind));
        }
        candidates.push(item);
    }
    candidates
}

pub struct ApplyResult {
    pub added: usize,
    pub revived_keys: Vec<String>,
    /// 需要落盘的图标：(SW-ID, 证据里的绝对源路径)。
    pub icons: Vec<(String, String)>,
}

/// 只应用勾选的候选：清空旧 is_new，把勾选项追加到最前。
/// `mark_new` 为 true 时导入项置 `is_new`（增量导入）；
/// 空台账的全量重建传 false，不把整机扫描当成“新增”。
/// 已有条目的任何其他字段都不改动。
pub fn apply_selected(
    software: &mut Vec<Value>,
    candidates: &[Value],
    selected_keys: &[String],
    mark_new: bool,
) -> ApplyResult {
    for item in software.iter_mut() {
        if let Some(obj) = item.as_object_mut() {
            obj.insert("is_new".to_string(), json!(false));
        }
    }

    let mut max_id: u64 = software
        .iter()
        .filter_map(|s| {
            s.get("id")
                .and_then(|v| v.as_str())
                .and_then(|id| id.replace("SW-", "").parse::<u64>().ok())
        })
        .max()
        .unwrap_or(0);

    let mut new_items: Vec<Value> = Vec::new();
    let mut revived_keys: Vec<String> = Vec::new();
    let mut icons: Vec<(String, String)> = Vec::new();
    for cand in candidates {
        let key = cand.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !selected_keys.iter().any(|k| k == key) {
            continue;
        }
        max_id += 1;
        let new_id = format!("SW-{:03}", max_id);
        let icon_src = cand
            .get("icon_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut item = cand.clone();
        if let Some(obj) = item.as_object_mut() {
            obj.insert("id".to_string(), json!(new_id.clone()));
            obj.insert("is_new".to_string(), json!(mark_new));
            obj.remove("key");
            obj.remove("kind");
            // icon_path 是证据目录的临时绝对路径，不落进 software.json；
            // 图标按 SW-ID 复制到 data/icons/ 后由 get_software 注入为 data URI。
            obj.remove("icon_path");
        }
        if !icon_src.is_empty() {
            icons.push((new_id, icon_src));
        }
        if cand.get("kind").and_then(|v| v.as_str()) == Some("deleted_before") {
            revived_keys.push(key.to_string());
        }
        new_items.push(item);
    }

    let added = new_items.len();
    new_items.extend(std::mem::take(software));
    *software = new_items;
    ApplyResult {
        added,
        revived_keys,
        icons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ingest_normalizes_and_filters() {
        let root = std::env::temp_dir().join(format!("ledger_ingest_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let evidence_root = root.join("evidence");
        let machine_dir = evidence_root.join("TEST-MACHINE");
        fs::create_dir_all(&machine_dir).unwrap();

        let registry = json!([
            { "name": "7-Zip 23.01 (x64)", "version": "23.01", "install_location": "C:\\Program Files\\7-Zip", "publisher": "Igor Pavlov" },
            { "name": "Microsoft Visual C++ 2015-2022 Redistributable (x64)", "version": "14.0", "install_location": "" },
            { "name": "Google Chrome", "version": "120.0", "install_location": "C:\\Program Files\\Google\\Chrome" }
        ]);
        fs::write(machine_dir.join("registry-apps.json"), registry.to_string()).unwrap();

        let portable = json!([
            { "name": "图吧工具箱", "folder_path": "D:\\Portable\\图吧工具箱", "main_exe": "D:\\Portable\\图吧工具箱\\tool.exe", "version": "1.0" }
        ]);
        fs::write(machine_dir.join("portable-apps.json"), portable.to_string()).unwrap();

        let _guard = crate::store::test_support::use_root(&root);
        let candidates = build_candidates(&evidence_root, &["TEST-MACHINE".to_string()], &[], &[]);
        let keys: Vec<String> = candidates
            .iter()
            .filter_map(|c| c.get("key").and_then(|v| v.as_str()).map(String::from))
            .collect();
        let mut software: Vec<Value> = Vec::new();
        let res = apply_selected(&mut software, &candidates, &keys, true);
        assert_eq!(res.added, candidates.len());
        store::write_software(&software);

        let items = store::read_software();
        let names: Vec<String> = items
            .iter()
            .filter_map(|i| i.get("name").and_then(|v| v.as_str()).map(String::from))
            .collect();
        assert!(names.iter().any(|n| n == "7-Zip"), "7-Zip normalized: {:?}", names);
        assert!(names.iter().any(|n| n == "Google Chrome"), "Chrome normalized: {:?}", names);
        assert!(
            !names.iter().any(|n| n.contains("Redistributable")),
            "noise should be filtered: {:?}",
            names
        );

        let seven = items
            .iter()
            .find(|i| i.get("name").and_then(|v| v.as_str()) == Some("7-Zip"))
            .unwrap();
        assert_eq!(seven.get("category").and_then(|v| v.as_str()), Some("系统工具"));
        assert_eq!(
            seven.get("download_url").and_then(|v| v.as_str()),
            Some("https://www.7-zip.org/")
        );
        let machines = seven.get("machines").and_then(|v| v.as_array()).unwrap();
        assert_eq!(machines.len(), 1);
        assert_eq!(
            machines[0].get("machine_id").and_then(|v| v.as_str()),
            Some("TEST-MACHINE")
        );

        // 绿色/便携软件：形态为 portable，处置方式直接确定性为 copy_dir。
        let portable_item = items
            .iter()
            .find(|i| i.get("name").and_then(|v| v.as_str()) == Some("图吧工具箱"))
            .unwrap();
        assert_eq!(portable_item.get("type").and_then(|v| v.as_str()), Some("portable"));
        assert_eq!(
            portable_item.get("backup_strategy").and_then(|v| v.as_str()),
            Some("copy_dir")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn candidates_classify_new_existing_deleted() {
        let root = std::env::temp_dir().join(format!("ledger_cand_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let evidence_root = root.join("evidence");
        let machine_dir = evidence_root.join("M-B");
        fs::create_dir_all(&machine_dir).unwrap();

        let registry = json!([
            { "name": "Google Chrome", "install_location": "C:\\Program Files\\Google\\Chrome" },
            { "name": "Visual Studio Code", "install_location": "C:\\Program Files\\Microsoft VS Code" },
            { "name": "7-Zip 23.01 (x64)", "install_location": "C:\\Program Files\\7-Zip" },
            { "name": "HiBit Uninstaller", "install_location": "C:\\Program Files\\HiBit Uninstaller" }
        ]);
        fs::write(machine_dir.join("registry-apps.json"), registry.to_string()).unwrap();

        let existing = json!([
            { "id": "SW-001", "name": "Google Chrome", "machines": [{ "machine_id": "M-A", "install_location": "C:\\Program Files\\Google\\Chrome" }] },
            { "id": "SW-002", "name": "VS Code", "machines": [{ "machine_id": "M-B", "install_location": "C:\\Program Files\\Microsoft VS Code" }] },
            { "id": "SW-003", "name": "7-Zip", "machines": [{ "machine_id": "M-B", "install_location": "C:\\Program Files\\7-Zip" }] }
        ]);
        let ignored = json!([
            { "match_key": "hibit uninstaller", "name": "HiBit Uninstaller", "paths": ["c:\\program files\\hibit uninstaller"] }
        ]);

        let candidates = build_candidates(
            &evidence_root,
            &["M-B".to_string()],
            existing.as_array().unwrap(),
            ignored.as_array().unwrap(),
        );
        let names: Vec<String> = candidates
            .iter()
            .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(String::from))
            .collect();

        // 新机器同名 -> 新行候选；同机命中名称/路径 -> 已知排除；墓碑 -> deleted_before
        assert!(
            names.iter().any(|n| n == "Google Chrome"),
            "new-machine same-name should be a candidate: {:?}",
            names
        );
        assert!(
            !names.iter().any(|n| n == "Visual Studio Code"),
            "path+machine match should be known: {:?}",
            names
        );
        assert!(
            !names.iter().any(|n| n == "7-Zip"),
            "name+machine match should be known: {:?}",
            names
        );

        let chrome = candidates
            .iter()
            .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("Google Chrome"))
            .unwrap();
        assert_eq!(chrome.get("kind").and_then(|v| v.as_str()), Some("new"));
        let hibit = candidates
            .iter()
            .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("HiBit Uninstaller"))
            .unwrap();
        assert_eq!(hibit.get("kind").and_then(|v| v.as_str()), Some("deleted_before"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_selected_clears_is_new_and_preserves_existing() {
        let mut software = vec![json!({
            "id": "SW-001",
            "name": "Old",
            "restore_intent": "must",
            "is_new": true,
            "backup_strategy": "copy_config"
        })];
        let candidates = vec![
            json!({ "key": "new one", "kind": "new", "name": "New One", "restore_intent": "unreviewed", "backup_strategy": "none" }),
            json!({ "key": "revived", "kind": "deleted_before", "name": "Revived", "restore_intent": "unreviewed", "backup_strategy": "none" }),
            json!({ "key": "unselected", "kind": "new", "name": "Unselected", "restore_intent": "unreviewed", "backup_strategy": "none" }),
        ];
        let res = apply_selected(
            &mut software,
            &candidates,
            &["new one".to_string(), "revived".to_string()],
            true,
        );
        assert_eq!(res.added, 2);
        assert_eq!(res.revived_keys, vec!["revived".to_string()]);

        // 已有条目：is_new 被清掉，其他字段零改动
        let old = software.iter().find(|s| s["id"] == "SW-001").unwrap();
        assert_eq!(old["is_new"], false);
        assert_eq!(old["restore_intent"], "must");
        assert_eq!(old["backup_strategy"], "copy_config");

        // 未勾选的候选不导入；勾选的置 is_new 并分配 id
        assert!(software.iter().all(|s| s["name"] != "Unselected"));
        let n = software.iter().find(|s| s["name"] == "New One").unwrap();
        assert_eq!(n["is_new"], true);
        assert!(n.get("key").is_none() && n.get("kind").is_none());
        assert_eq!(n["id"], "SW-002");

        // 空台账（首次全量扫描）：导入的条目不标 is_new
        let mut fresh: Vec<Value> = Vec::new();
        apply_selected(
            &mut fresh,
            &candidates,
            &["new one".to_string()],
            false,
        );
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["is_new"], false);
    }

    #[test]
    fn apply_selected_collects_icon_and_strips_transient_path() {
        let candidates = vec![json!({
            "key": "7-zip",
            "kind": "new",
            "name": "7-Zip",
            "icon_path": "C:\\evidence\\M\\app-icons\\abc.png",
            "machines": []
        })];
        let mut software: Vec<Value> = Vec::new();
        let res = apply_selected(&mut software, &candidates, &["7-zip".to_string()], false);
        // 图标来源被收集，供 scan_commit 复制到 data/icons/<SW-ID>.png
        assert_eq!(res.icons.len(), 1);
        assert_eq!(res.icons[0].0, "SW-001");
        assert_eq!(res.icons[0].1, "C:\\evidence\\M\\app-icons\\abc.png");
        // 证据目录的临时绝对路径不落进 software.json
        assert!(software[0].get("icon_path").is_none());
    }
}
