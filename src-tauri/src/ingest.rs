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
    // 处置方式不在扫描时硬编码：默认「无需操作」，待用户评恢复意愿时按形态推导。
    let default_strategy = "none";

    if let Some(&i) = index.get(&key) {
        let item = &mut items[i];
        if item.get("version").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            if let Some(v) = machine.get("version").and_then(|x| x.as_str()) {
                if !v.is_empty() {
                    item["version"] = json!(v);
                }
            }
        }
        // 已存在的条目：不再按便携形态覆盖处置方式，交由评档推导。
        update_machine(item, machine);
        crate::store::touch(item);
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
        "uuid": store::new_uuid(),
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
    crate::store::touch(&mut item);
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

/// 机器条目的安装路径：优先 `install_location`，否则退回 `path`。
fn machine_location(m: &Value) -> &str {
    let a = m.get("install_location").and_then(|v| v.as_str()).unwrap_or("");
    if a.is_empty() {
        m.get("path").and_then(|v| v.as_str()).unwrap_or("")
    } else {
        a
    }
}

fn candidate_paths(item: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
        for m in ms {
            let p = normalize_path(machine_location(m));
            if !p.is_empty() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// 机器 + 规范化路径（无路径为 None）的候选定位键。
fn candidate_machine_locations(item: &Value) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
        for m in ms {
            let mid = m.get("machine_id").and_then(|v| v.as_str()).unwrap_or("");
            if mid.is_empty() {
                continue;
            }
            let p = normalize_path(machine_location(m));
            let pair = (mid.to_string(), if p.is_empty() { None } else { Some(p) });
            if !out.contains(&pair) {
                out.push(pair);
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

/// 已知判定：已有条目包含「本次机器」的记录，且安装路径完全相等；
/// 仅当候选本身没有任何路径时，才退回「名称相同」。
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
    // 候选自带路径时，只认路径。否则「手动添加的同名条目（无路径）」会把
    // 「扫描到的另一份安装」判成已知而整条吞掉（同名 ≠ 同一实体）。
    let cand_has_path = !cand_paths.is_empty();

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
            let same_path = {
                let p = normalize_path(loc);
                !p.is_empty() && cand_paths.iter().any(|x| *x == p)
            };
            let same_name = !cand_name.is_empty() && cand_name == ex_name;
            // 有路径：只认路径；无路径：退回名称匹配。
            if same_path || (!cand_has_path && same_name) {
                return Some(ex);
            }
        }
    }
    None
}

fn is_known(candidate: &Value, existing: &[Value]) -> bool {
    find_known(candidate, existing).is_some()
}

/// 已有条目缺图标的补充来源：返回 (uuid, 图标绝对路径)。
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
            let uuid = ex.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
            if !uuid.is_empty() && !out.iter().any(|(i, _)| i == uuid) {
                out.push((uuid.to_string(), icon.to_string()));
            }
        }
    }
    out
}

/// 墓碑判定：路径命中只认「机器 + 路径」；无路径候选则按「机器 + 同名」匹配。
/// 两者都带机器维度 —— 一台机器上的删除不会波及别的机器。
fn tombstone_match(candidate: &Value, ignored: &[Value]) -> bool {
    let cand = candidate_machine_locations(candidate);
    if cand.is_empty() {
        return false;
    }
    let cand_name = candidate
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase()
        .trim()
        .to_string();
    for g in ignored {
        let g_key = g.get("match_key").and_then(|v| v.as_str()).unwrap_or("");
        let Some(gms) = g.get("machines").and_then(|v| v.as_array()) else {
            continue;
        };
        for gm in gms {
            let gmid = gm.get("machine_id").and_then(|v| v.as_str()).unwrap_or("");
            if gmid.is_empty() {
                continue;
            }
            let g_paths = gm.get("paths").and_then(|v| v.as_array());
            let g_name_only = gm.get("name_only").and_then(|v| v.as_bool()).unwrap_or(false);
            for (cmid, cpath) in &cand {
                if cmid != gmid {
                    continue;
                }
                match cpath {
                    Some(p) => {
                        let hit = g_paths
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str())
                                    .any(|gp| normalize_path(gp) == *p)
                            })
                            .unwrap_or(false);
                        if hit {
                            return true;
                        }
                    }
                    None => {
                        if g_name_only && !cand_name.is_empty() && g_key == cand_name {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// 写入删除墓碑：有路径的记「机器 + 路径」，无路径的记「机器 + 同名」（`name_only`）。
/// 同名条目归并到同一条墓碑记录，路径按机器取**并集**（不覆盖），避免先删的被后来者抹掉。
pub fn record_tombstones(ignored: &mut Vec<Value>, doomed: &[Value]) {
    let deleted_at = chrono::Local::now().to_rfc3339();
    for item in doomed {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let key = name.to_lowercase().trim().to_string();
        let locs = candidate_machine_locations(item);
        if locs.is_empty() {
            continue;
        }
        let idx = match ignored
            .iter()
            .position(|g| g.get("match_key").and_then(|v| v.as_str()) == Some(key.as_str()))
        {
            Some(i) => i,
            None => {
                ignored.push(json!({
                    "match_key": key,
                    "name": name,
                    "machines": [],
                    "deleted_at": deleted_at,
                }));
                ignored.len() - 1
            }
        };
        let g = &mut ignored[idx];
        g["name"] = json!(name);
        g["deleted_at"] = json!(deleted_at);
        if g.get("machines").and_then(|v| v.as_array()).is_none() {
            g["machines"] = json!([]);
        }
        let gms = g.get_mut("machines").unwrap().as_array_mut().unwrap();
        for (mid, path) in &locs {
            let gm = gms.iter_mut().find(|m| {
                m.get("machine_id").and_then(|v| v.as_str()) == Some(mid.as_str())
            });
            match gm {
                Some(gm) => match path {
                    Some(path) => match gm.get_mut("paths").and_then(|v| v.as_array_mut()) {
                        Some(arr) => {
                            if !arr.iter().any(|x| x.as_str() == Some(path.as_str())) {
                                arr.push(json!(path));
                            }
                        }
                        None => gm["paths"] = json!([path]),
                    },
                    None => gm["name_only"] = json!(true),
                },
                None => gms.push(match path {
                    Some(path) => json!({ "machine_id": mid, "paths": [path] }),
                    None => json!({ "machine_id": mid, "paths": [], "name_only": true }),
                }),
            }
        }
    }
}

/// 复活后从墓碑里精确移除：有路径按 (机器, 路径)，无路径按 (机器, 同名) 清 `name_only`。
/// 墓碑机器条目既无路径又无 `name_only` 了就删掉，整条记录空了再删记录。
pub fn clear_tombstones(ignored: &mut Vec<Value>, revived: &[RevivedTombstone]) {
    for g in ignored.iter_mut() {
        let g_key = g.get("match_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let Some(ms) = g.get_mut("machines").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for m in ms.iter_mut() {
            let mid = m.get("machine_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if mid.is_empty() {
                continue;
            }
            if let Some(paths) = m.get_mut("paths").and_then(|v| v.as_array_mut()) {
                paths.retain(|p| {
                    let np = p.as_str().unwrap_or("");
                    !revived
                        .iter()
                        .any(|r| r.machine_id == mid && r.path.as_deref() == Some(np))
                });
            }
            let clear_name = revived
                .iter()
                .any(|r| r.machine_id == mid && r.path.is_none() && r.name_key == g_key);
            if clear_name {
                if let Some(o) = m.as_object_mut() {
                    o.remove("name_only");
                }
            }
        }
        ms.retain(|m| {
            m.get("name_only").and_then(|v| v.as_bool()).unwrap_or(false)
                || m.get("paths")
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
        });
    }
    ignored.retain(|g| {
        g.get("machines")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    });
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

/// 复活墓碑的定位键：机器 + 小写名 + 可选路径（None = 机器内的名字墓碑）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RevivedTombstone {
    pub machine_id: String,
    pub name_key: String,
    pub path: Option<String>,
}

pub struct ApplyResult {
    pub added: usize,
    /// 复活项对应的墓碑定位键，用于从墓碑里精确清除。
    pub revived: Vec<RevivedTombstone>,
    /// 需要落盘的图标：(图标文件名, 证据里的绝对源路径)。
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
    let mut revived: Vec<RevivedTombstone> = Vec::new();
    let mut icons: Vec<(String, String)> = Vec::new();
    for cand in candidates {
        let key = cand.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !selected_keys.iter().any(|k| k == key) {
            continue;
        }
        max_id += 1;
        let new_id = format!("SW-{:03}", max_id);
        let uuid = crate::store::new_uuid();
        let icon_src = cand
            .get("icon_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // 图标文件名 = `<uuid>.png`，与软件名 / 显示号无关。
        let icon_file = if icon_src.is_empty() {
            String::new()
        } else {
            crate::store::icon_file_name(&uuid)
        };
        let mut item = cand.clone();
        if let Some(obj) = item.as_object_mut() {
            obj.insert("id".to_string(), json!(new_id.clone()));
            obj.insert("uuid".to_string(), json!(uuid.clone()));
            obj.insert("is_new".to_string(), json!(mark_new));
            obj.remove("key");
            obj.remove("kind");
            // icon_path 是证据目录的临时绝对路径，不落进 software.json；
            // 文件本体写到 data/icons/<icon_file>，由 get_software 注入为 data URI。
            obj.remove("icon_path");
            if !icon_file.is_empty() {
                obj.insert("icon_file".to_string(), json!(icon_file.clone()));
            }
        }
        if !icon_file.is_empty() {
            icons.push((icon_file, icon_src));
        }
        crate::store::touch(&mut item);
        if cand.get("kind").and_then(|v| v.as_str()) == Some("deleted_before") {
            for (mid, path) in candidate_machine_locations(cand) {
                let r = RevivedTombstone {
                    machine_id: mid,
                    name_key: key.to_string(),
                    path,
                };
                if !revived.contains(&r) {
                    revived.push(r);
                }
            }
        }
        new_items.push(item);
    }

    let added = new_items.len();
    new_items.extend(std::mem::take(software));
    *software = new_items;
    ApplyResult {
        added,
        revived,
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

        // 绿色/便携软件：形态为 portable，但处置方式不再硬编码，默认「无需操作」，评档时再推导。
        let portable_item = items
            .iter()
            .find(|i| i.get("name").and_then(|v| v.as_str()) == Some("图吧工具箱"))
            .unwrap();
        assert_eq!(portable_item.get("type").and_then(|v| v.as_str()), Some("portable"));
        assert_eq!(
            portable_item.get("backup_strategy").and_then(|v| v.as_str()),
            Some("none")
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
            { "match_key": "hibit uninstaller", "name": "HiBit Uninstaller",
              "machines": [{ "machine_id": "M-B", "paths": ["c:\\program files\\hibit uninstaller"] }] }
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
    fn same_name_manual_entry_does_not_mask_scanned_install() {
        // 复现：扫到 7-Zip -> 手动再加一个同名 7-Zip（无安装路径）-> 删除扫到的那份 -> 重扫。
        // 同名的无路径手动条目不能把扫描候选整条吞掉，否则删除后永远恢复不了。
        let root = std::env::temp_dir().join(format!("ledger_mask_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let evidence_root = root.join("evidence");
        let machine_dir = evidence_root.join("M-B");
        fs::create_dir_all(&machine_dir).unwrap();
        fs::write(
            machine_dir.join("registry-apps.json"),
            json!([{ "name": "7-Zip 24.07 (x64)", "install_location": "C:\\Program Files\\7-Zip" }])
                .to_string(),
        )
        .unwrap();

        // 手动添加的同名条目：同机器、无安装路径
        let existing = json!([
            { "id": "SW-236", "name": "7-Zip", "machines": [{ "machine_id": "M-B", "form": "manual" }] }
        ]);
        let candidates = build_candidates(
            &evidence_root,
            &["M-B".to_string()],
            existing.as_array().unwrap(),
            &[],
        );
        assert_eq!(
            candidates.len(),
            1,
            "同名的无路径手动条目不应遮蔽扫描候选: {:?}",
            candidates
        );
        assert_eq!(candidates[0].get("name").and_then(|v| v.as_str()), Some("7-Zip"));
        assert_eq!(candidates[0].get("kind").and_then(|v| v.as_str()), Some("new"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tombstone_requires_machine_and_path() {
        // 墓碑只认「机器 + 路径」：换机器、换路径、或候选没路径，都不该被误伤。
        let cand = json!({
            "name": "7-Zip",
            "machines": [{ "machine_id": "M-B", "install_location": "C:\\Program Files\\7-Zip" }]
        });
        let g = |mid: &str, path: &str| {
            json!([
                { "match_key": "7-zip", "name": "7-Zip",
                  "machines": [{ "machine_id": mid, "paths": [path] }] }
            ])
        };
        // 同机同路径 -> 命中
        assert!(tombstone_match(
            &cand,
            g("M-B", "c:\\program files\\7-zip").as_array().unwrap()
        ));
        // 同路径异机 -> 不命中（机器维度生效）
        assert!(!tombstone_match(
            &cand,
            g("M-A", "c:\\program files\\7-zip").as_array().unwrap()
        ));
        // 同机异路径 -> 不命中
        assert!(!tombstone_match(
            &cand,
            g("M-B", "c:\\other\\7-zip").as_array().unwrap()
        ));
        // 无路径候选：同机名字墓碑（name_only）才命中
        let no_path = json!({ "name": "7-Zip", "machines": [{ "machine_id": "M-B" }] });
        let name_only = json!([{
            "match_key": "7-zip", "name": "7-Zip",
            "machines": [{ "machine_id": "M-B", "paths": [], "name_only": true }]
        }]);
        assert!(tombstone_match(&no_path, name_only.as_array().unwrap()));
        // 有路径的墓碑不拦无路径候选
        assert!(!tombstone_match(
            &no_path,
            g("M-B", "c:\\program files\\7-zip").as_array().unwrap()
        ));
        // 名字墓碑同样受机器维度约束：别的机器不命中
        let other_machine = json!([{
            "match_key": "7-zip", "name": "7-Zip",
            "machines": [{ "machine_id": "M-A", "paths": [], "name_only": true }]
        }]);
        assert!(!tombstone_match(&no_path, other_machine.as_array().unwrap()));
    }

    #[test]
    fn record_and_clear_tombstones_are_per_machine_union() {
        let mut ignored: Vec<Value> = Vec::new();
        // 同名不同机器的两条记录先后删除：按机器取并集，不互相覆盖
        record_tombstones(
            &mut ignored,
            &[json!({
                "name": "7-Zip",
                "machines": [{ "machine_id": "M-A", "install_location": "C:\\Program Files\\7-Zip" }]
            })],
        );
        record_tombstones(
            &mut ignored,
            &[json!({
                "name": "7-Zip",
                "machines": [{ "machine_id": "M-B", "install_location": "C:\\Program Files\\7-Zip" }]
            })],
        );
        assert_eq!(ignored.len(), 1);
        assert_eq!(ignored[0]["machines"].as_array().unwrap().len(), 2, "{:?}", ignored);

        // 无路径条目 -> 写「机器 + 同名」墓碑（name_only）
        record_tombstones(
            &mut ignored,
            &[json!({ "name": "Python", "machines": [{ "machine_id": "M-A" }] })],
        );
        let py = ignored.iter().find(|g| g["match_key"] == "python").unwrap();
        assert_eq!(py["machines"][0]["name_only"], json!(true));

        // 精确清除 (M-A, path)：只清这台机器，记录保留
        clear_tombstones(
            &mut ignored,
            &[RevivedTombstone {
                machine_id: "M-A".to_string(),
                name_key: "7-zip".to_string(),
                path: Some("c:\\program files\\7-zip".to_string()),
            }],
        );
        assert_eq!(ignored.len(), 2);
        let zip = ignored.iter().find(|g| g["match_key"] == "7-zip").unwrap();
        assert_eq!(zip["machines"].as_array().unwrap().len(), 1);

        // 名字墓碑按 (机器, 同名) 清除
        clear_tombstones(
            &mut ignored,
            &[RevivedTombstone {
                machine_id: "M-A".to_string(),
                name_key: "python".to_string(),
                path: None,
            }],
        );
        assert!(ignored.iter().all(|g| g["match_key"] != "python"), "{:?}", ignored);

        // 清掉最后一台 -> 整条记录消失
        clear_tombstones(
            &mut ignored,
            &[RevivedTombstone {
                machine_id: "M-B".to_string(),
                name_key: "7-zip".to_string(),
                path: Some("c:\\program files\\7-zip".to_string()),
            }],
        );
        assert!(ignored.iter().all(|g| g["match_key"] != "7-zip"), "{:?}", ignored);
    }

    #[test]
    fn apply_selected_clears_is_new_and_preserves_existing() {
        let mut software = vec![json!({
            "id": "SW-001",
            "name": "Old",
            "restore_intent": "must",
            "is_new": true,
            "backup_strategy": "copy_dir"
        })];
        let candidates = vec![
            json!({ "key": "new one", "kind": "new", "name": "New One", "restore_intent": "unreviewed", "backup_strategy": "none" }),
            json!({ "key": "revived", "kind": "deleted_before", "name": "Revived", "restore_intent": "unreviewed", "backup_strategy": "none", "machines": [{ "machine_id": "M-A", "install_location": "C:\\x" }] }),
            json!({ "key": "unselected", "kind": "new", "name": "Unselected", "restore_intent": "unreviewed", "backup_strategy": "none" }),
        ];
        let res = apply_selected(
            &mut software,
            &candidates,
            &["new one".to_string(), "revived".to_string()],
            true,
        );
        assert_eq!(res.added, 2);
        assert_eq!(
            res.revived,
            vec![RevivedTombstone {
                machine_id: "M-A".to_string(),
                name_key: "revived".to_string(),
                path: Some("c:\\x".to_string()),
            }]
        );

        // 已有条目：is_new 被清掉，其他字段零改动
        let old = software.iter().find(|s| s["id"] == "SW-001").unwrap();
        assert_eq!(old["is_new"], false);
        assert_eq!(old["restore_intent"], "must");
        assert_eq!(old["backup_strategy"], "copy_dir");

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
        // 图标文件名 = `<uuid>.png`，与软件名 / 显示号无关
        let uuid = software[0]["uuid"].as_str().unwrap();
        let expected = crate::store::icon_file_name(uuid);
        assert_eq!(res.icons.len(), 1);
        assert_eq!(res.icons[0].0, expected);
        assert_eq!(res.icons[0].1, "C:\\evidence\\M\\app-icons\\abc.png");
        // 条目记下 icon_file；证据目录的临时绝对路径不落进 software.json
        assert_eq!(software[0]["icon_file"], json!(expected));
        assert!(!software[0]["uuid"].as_str().unwrap().is_empty());
        assert!(software[0].get("icon_path").is_none());

        // 无图标来源的候选不产生 icon_file，也不进落盘清单
        let no_icon = vec![json!({ "key": "x", "kind": "new", "name": "NoIcon", "machines": [] })];
        let mut sw2: Vec<Value> = Vec::new();
        let r2 = apply_selected(&mut sw2, &no_icon, &["x".to_string()], false);
        assert!(r2.icons.is_empty());
        assert!(sw2[0].get("icon_file").is_none());
    }
}
