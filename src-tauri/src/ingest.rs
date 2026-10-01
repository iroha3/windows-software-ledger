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
) {
    let norm = normalize_name(raw_name);
    let key = norm.name.to_lowercase().trim().to_string();

    if let Some(&i) = index.get(&key) {
        let item = &mut items[i];
        if item.get("version").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            if let Some(v) = machine.get("version").and_then(|x| x.as_str()) {
                if !v.is_empty() {
                    item["version"] = json!(v);
                }
            }
        }
        update_machine(item, machine);
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
        "backup_strategy": "none",
        "prep_status": "todo",
        "has_config": false,
        "download_url": norm.download_url,
        "config_notes": "",
        "is_awesome": false,
        "awesome_role": "",
        "created_at": chrono::Local::now().to_rfc3339(),
    });
    update_machine(&mut item, machine);
    index.insert(key, items.len());
    items.push(item);
}

/// 把某个证据目录下的所有机器结果合并进 `software.json`。
/// 证据目录只是入参，调用方（扫描）用临时目录，用完自行删除。
pub fn run_ingest(evidence_root: &Path) -> Value {
    let ev = evidence_root;
    if !ev.exists() {
        return json!({ "success": false, "message": "Evidence directory does not exist" });
    }

    let existing = store::read_software();
    let mut items: Vec<Value> = existing.clone();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
            index.insert(name.to_lowercase().trim().to_string(), i);
        }
    }

    let mut next_id: u64 = existing
        .iter()
        .filter_map(|s| {
            s.get("id")
                .and_then(|v| v.as_str())
                .and_then(|id| id.replace("SW-", "").parse::<u64>().ok())
        })
        .max()
        .unwrap_or(0);

    let mut machine_dirs: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(ev) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                machine_dirs.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }

    for machine_id in &machine_dirs {
        let mdir = ev.join(machine_id);

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
                add_or_update(&mut items, &mut index, &mut next_id, name, machine, None);
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
                add_or_update(&mut items, &mut index, &mut next_id, name, machine, Some("portable"));
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
                add_or_update(&mut items, &mut index, &mut next_id, name, machine, None);
            }
        }
    }

    fn intent_order(s: &Value) -> u8 {
        match s.get("restore_intent").and_then(|v| v.as_str()).unwrap_or("unreviewed") {
            "must" => 1,
            "should" => 2,
            "on_demand" => 3,
            "unreviewed" => 4,
            "drop" => 5,
            _ => 4,
        }
    }
    items.sort_by(|a, b| {
        let d = intent_order(a).cmp(&intent_order(b));
        if d != std::cmp::Ordering::Equal {
            return d;
        }
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });

    store::write_software(&items);
    json!({ "success": true, "total": items.len(), "machines": machine_dirs })
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

        let _guard = crate::store::test_support::use_root(&root);
        let res = run_ingest(&evidence_root);
        assert_eq!(res.get("success").and_then(|v| v.as_bool()), Some(true));

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

        let _ = fs::remove_dir_all(&root);
    }
}
