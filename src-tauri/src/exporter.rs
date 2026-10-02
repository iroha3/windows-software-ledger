use serde_json::{json, Value};

use crate::store;

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn intent_label(intent: &str) -> &'static str {
    match intent {
        "must" => "🔴 必须恢复 (Must Restore)",
        "should" => "🟡 建议恢复 (Should Restore)",
        "on_demand" => "🔵 用到再装 (On Demand)",
        "drop" => "⚫ 淘汰弃用 (Drop / Deprecate)",
        _ => "⚪ 待确认 (Unreviewed)",
    }
}

fn strategy_label(strategy: &str) -> &'static str {
    match strategy {
        "copy_dir" => "📦 保留/压缩整个目录",
        "copy_config" => "⚙️ 手动导出/备份配置",
        "redownload" => "🌐 重新下载安装",
        "sync_account" => "☁️ 账号登录同步",
        _ => "➖ 无需操作",
    }
}

fn group_by_category<'a>(items: &'a [&'a Value]) -> Vec<(String, Vec<&'a Value>)> {
    let mut groups: Vec<(String, Vec<&Value>)> = Vec::new();
    for item in items {
        let cat = {
            let c = s(item, "category");
            if c.is_empty() { "未分类".to_string() } else { c.to_string() }
        };
        if let Some(g) = groups.iter_mut().find(|(name, _)| *name == cat) {
            g.1.push(item);
        } else {
            groups.push((cat, vec![*item]));
        }
    }
    groups
}

fn render_group_checklist(items: &[&Value]) -> String {
    let mut out: Vec<String> = Vec::new();
    for (cat, list) in group_by_category(items) {
        out.push(format!("### 📂 {}", cat));
        for item in list {
            let version = s(item, "version");
            let ver_text = if version.is_empty() { String::new() } else { format!(" `{}`", version) };
            let url = s(item, "download_url");
            let url_text = if url.is_empty() { String::new() } else { format!(" [官网/下载]({})", url) };
            let mut machines: Vec<String> = Vec::new();
            if let Some(ms) = item.get("machines").and_then(|m| m.as_array()) {
                for m in ms {
                    let mid = s(m, "machine_id");
                    if !mid.is_empty() && !machines.iter().any(|x| x == mid) {
                        machines.push(mid.to_string());
                    }
                }
            }
            let machines_text = machines.join(", ");
            let kind = {
                let t = s(item, "type");
                if t.is_empty() { "desktop".to_string() } else { t.to_string() }
            };
            let strategy = s(item, "backup_strategy");
            let strat_text = if !strategy.is_empty() && strategy != "none" {
                format!(" *(处置: {})*", strategy_label(strategy))
            } else {
                String::new()
            };
            let notes = s(item, "config_notes");
            let notes_text = if notes.is_empty() {
                String::new()
            } else {
                format!("\n  - 💡 **备注/配置说明**: {}", notes)
            };
            let ready = s(item, "prep_status") == "ready";
            let check_mark = if ready { "[x]" } else { "[ ]" };
            let ready_badge = if ready { " *(已就绪)*" } else { "" };

            out.push(format!(
                "- {} **{}**{} ({}){} — 机器: `{}`{}{}{}",
                check_mark, s(item, "name"), ver_text, kind, url_text, machines_text, strat_text, ready_badge, notes_text
            ));
        }
        out.push(String::new());
    }
    out.join("\n")
}

fn render_compact_list(items: &[&Value]) -> String {
    let mut out = String::new();
    for item in items {
        let url = s(item, "download_url");
        let url_text = if url.is_empty() { String::new() } else { format!(" ([链接]({}))", url) };
        out.push_str(&format!("- [ ] **{}** ({}){}\n", s(item, "name"), s(item, "category"), url_text));
    }
    out
}

fn render_drop_list(items: &[&Value]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(&format!("- ❌ ~~{}~~ ({})\n", s(item, "name"), s(item, "category")));
    }
    out
}

fn render_awesome_list(items: &[&Value]) -> String {
    let mut out: Vec<String> = Vec::new();
    for (cat, list) in group_by_category(items) {
        out.push(format!("## 📌 {}\n", cat));
        for item in list {
            let url = s(item, "download_url");
            let url_text = if url.is_empty() { String::new() } else { format!(" - [官方站点]({})", url) };
            let role = s(item, "awesome_role");
            let role_text = if role.is_empty() { String::new() } else { format!("\n> **工作流定位**: {}", role) };
            let notes = s(item, "config_notes");
            let notes_text = if notes.is_empty() { String::new() } else { format!("\n- **实践经验**: {}", notes) };
            out.push(format!("### {}{}{}{}\n", s(item, "name"), url_text, role_text, notes_text));
        }
    }
    out.join("\n")
}

pub fn export_checklists() -> Value {
    let software_path = store::software_file();
    if !software_path.exists() {
        return json!({ "success": false, "message": "software.json does not exist" });
    }

    let items = store::read_software();

    let mut must: Vec<&Value> = Vec::new();
    let mut should: Vec<&Value> = Vec::new();
    let mut on_demand: Vec<&Value> = Vec::new();
    let mut unreviewed: Vec<&Value> = Vec::new();
    let mut drop: Vec<&Value> = Vec::new();
    let mut backup_tasks: Vec<&Value> = Vec::new();
    let mut awesome_items: Vec<&Value> = Vec::new();

    for item in &items {
        match s(item, "restore_intent") {
            "must" => must.push(item),
            "should" => should.push(item),
            "on_demand" => on_demand.push(item),
            "drop" => drop.push(item),
            _ => unreviewed.push(item),
        }
        let strategy = s(item, "backup_strategy");
        if strategy == "copy_dir" || strategy == "copy_config" {
            backup_tasks.push(item);
        }
        if item.get("is_awesome").and_then(|v| v.as_bool()).unwrap_or(false) {
            awesome_items.push(item);
        }
    }

    let now = chrono::Local::now().format("%Y/%m/%d %H:%M:%S").to_string();

    let backup_table = if backup_tasks.is_empty() {
        "*暂无标记为需要打包目录的软件。在软件备份台账中将处置方式标记为「保留/压缩目录」后将在此列出。*".to_string()
    } else {
        let rows: Vec<String> = backup_tasks
            .iter()
            .map(|t| {
                let mut paths: Vec<String> = Vec::new();
                if let Some(ms) = t.get("machines").and_then(|m| m.as_array()) {
                    for m in ms {
                        let loc = {
                            let a = s(m, "install_location");
                            if a.is_empty() { s(m, "path").to_string() } else { a.to_string() }
                        };
                        let loc = if loc.is_empty() { "未记录路径".to_string() } else { loc };
                        paths.push(format!("`{}`: {}", s(m, "machine_id"), loc));
                    }
                }
                let notes = {
                    let n = s(t, "config_notes");
                    if n.is_empty() { "—".to_string() } else { n.to_string() }
                };
                format!(
                    "| **{}** | {} | {} | {} |",
                    s(t, "name"),
                    strategy_label(s(t, "backup_strategy")),
                    paths.join("<br>"),
                    notes
                )
            })
            .collect();
        format!(
            "| 软件名称 | 处置方式 | 机器分布与路径 | 备份备忘与配置说明 |\n|---|---|---|---|\n{}",
            rows.join("\n")
        )
    };

    let checklist_lines = vec![
        "# 电脑重装恢复备忘清单 (Recovery Checklist)".to_string(),
        String::new(),
        format!("> 生成时间: {}  ", now),
        format!(
            "> 统计概览: 软件总数 **{}** | 必须恢复 **{}** | 建议恢复 **{}** | 用到再装 **{}** | 待确认 **{}** | 待备份资产 **{}**",
            items.len(), must.len(), should.len(), on_demand.len(), unreviewed.len(), backup_tasks.len()
        ),
        String::new(),
        "---".to_string(),
        String::new(),
        "## ⚠️ 重装前必须备份的资产清单 (Pre-install Backup Tasks)".to_string(),
        String::new(),
        backup_table,
        String::new(),
        "---".to_string(),
        String::new(),
        format!("## 一、{}", intent_label("must")),
        String::new(),
        if must.is_empty() { "*暂无必须恢复的软件*".to_string() } else { render_group_checklist(&must) },
        String::new(),
        format!("## 二、{}", intent_label("should")),
        String::new(),
        if should.is_empty() { "*暂无建议恢复的软件*".to_string() } else { render_group_checklist(&should) },
        String::new(),
        format!("## 三、{}", intent_label("on_demand")),
        String::new(),
        if on_demand.is_empty() { "*暂无*".to_string() } else { render_compact_list(&on_demand) },
        String::new(),
        format!("## 四、{}", intent_label("drop")),
        String::new(),
        if drop.is_empty() { "*暂无*".to_string() } else { render_drop_list(&drop) },
    ];
    let checklist_content = checklist_lines.join("\n");

    let awesome_lines = vec![
        "# 个人工作流精选软件资产库 (Awesome Software List)".to_string(),
        String::new(),
        format!("> 汇编时间: {}  ", now),
        format!("> 精选收录: **{}** 款核心工具", awesome_items.len()),
        String::new(),
        "这份清单记录了深度融入日常开发与生产力工作流的高价值工具。".to_string(),
        String::new(),
        if awesome_items.is_empty() {
            "*暂无收录。在软件备份台账表格中为认可的软件点亮星标 ★ 即可收录至此。*".to_string()
        } else {
            render_awesome_list(&awesome_items)
        },
    ];
    let awesome_content = awesome_lines.join("\n");

    json!({
        "success": true,
        "checklistFilename": "RECOVERY_CHECKLIST.md",
        "awesomeFilename": "AWESOME_LIST.md",
        "checklistContent": checklist_content,
        "awesomeContent": awesome_content,
        "stats": {
            "total": items.len(),
            "must": must.len(),
            "should": should.len(),
            "backupTasks": backup_tasks.len(),
            "awesome": awesome_items.len()
        }
    })
}
