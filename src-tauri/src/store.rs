use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// 数据根目录 = 可执行文件所在目录。
/// 便携发布：exe 与 `data/` 放同一个文件夹，`data/` 首次运行自动创建。
pub fn app_root() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = test_support::root_override() {
        return root;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// 仅测试使用：把数据根临时指向临时目录，并串行化并行测试。
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};

    static ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);
    static SERIAL: Mutex<()> = Mutex::new(());

    pub(crate) fn root_override() -> Option<PathBuf> {
        ROOT.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) struct Guard(#[allow(dead_code)] MutexGuard<'static, ()>);

    impl Drop for Guard {
        fn drop(&mut self) {
            *ROOT.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    pub(crate) fn use_root(root: &Path) -> Guard {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        *ROOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(root.to_path_buf());
        Guard(serial)
    }
}

pub fn data_dir() -> PathBuf {
    app_root().join("data")
}
pub fn software_file() -> PathBuf {
    data_dir().join("software.json")
}
pub fn config_file() -> PathBuf {
    data_dir().join("config.json")
}
/// 已删除软件的墓碑：重扫时命中则默认不勾选，避免垃圾复活。
pub fn ignored_file() -> PathBuf {
    data_dir().join("ignored.json")
}
/// 采集证据目录：`data/evidence/<主机名>/`，长期保留（含 screenshots/）。
pub fn evidence_dir() -> PathBuf {
    data_dir().join("evidence")
}

/// 浏览器扩展的用户层标注（备注等），键 = 扩展的 uuid。
/// 与采集证据分离，重扫不会覆盖。每条记录内嵌匹配键
/// `(browser_id, profile, ext_id)`，重扫时用它把 uuid 认回来。
pub fn extensions_file() -> PathBuf {
    data_dir().join("extensions.json")
}

/// 读取扩展标注：键 = 扩展 uuid，始终返回对象。
pub fn read_extensions() -> Value {
    let v = read_json(&extensions_file());
    if v.is_object() {
        v
    } else {
        json!({})
    }
}

pub fn write_extensions(value: &Value) {
    write_json(&extensions_file(), value);
}

/// 浏览器用户层：键 = 浏览器 uuid，值内嵌 `browser_id` 匹配键。
/// 目前只为保管箱提供稳定身份，后续可承载浏览器级标注。
pub fn browsers_file() -> PathBuf {
    data_dir().join("browsers.json")
}

pub fn read_browsers() -> Value {
    let v = read_json(&browsers_file());
    if v.is_object() {
        v
    } else {
        json!({})
    }
}

pub fn write_browsers(value: &Value) {
    write_json(&browsers_file(), value);
}

/// 给实体打上 `updated_at`：同步合并时用它判断新旧（优先于 created_at）。
pub fn touch(item: &mut Value) {
    if let Some(o) = item.as_object_mut() {
        o.insert(
            "updated_at".to_string(),
            json!(chrono::Local::now().to_rfc3339()),
        );
    }
}

/// WebDAV 账户与地址：`data/webdav.json`。**本地文件，绝不参与同步**
/// （推上去等于「用存密码的文件去存密码」）。
pub fn webdav_file() -> PathBuf {
    data_dir().join("webdav.json")
}

pub fn default_webdav_config() -> Value {
    json!({
        "enabled": false,
        "url": "",
        "username": "",
        "password": ""
    })
}

/// 读取 WebDAV 配置，缺失字段回退默认值。
pub fn get_webdav_config() -> Value {
    let mut cfg = default_webdav_config();
    if let (Value::Object(base), Value::Object(over)) = (&mut cfg, read_json(&webdav_file())) {
        for (k, v) in over {
            base.insert(k, v);
        }
    }
    cfg
}

pub fn save_webdav_config(value: &Value) {
    write_json(&webdav_file(), value);
}

/// 同步辅助目录：`data/.sync/`。**本地文件，绝不参与同步**。
pub fn sync_dir() -> PathBuf {
    data_dir().join(".sync")
}

/// 本地基线 manifest：上次同步完成时双方一致的状态。
pub fn baseline_manifest_file() -> PathBuf {
    sync_dir().join("manifest.json")
}

/// 本地文件指纹缓存：避免每次同步都对几个 G 的归档重算 sha256。
pub fn file_index_file() -> PathBuf {
    sync_dir().join("index.json")
}

/// 上次同步结果（本地展示用）。
pub fn last_sync_file() -> PathBuf {
    sync_dir().join("last.json")
}

/// 编辑权状态：GUI 把「本机是否持锁（可写）」落到磁盘，供独立的 MCP 进程
/// 判断能否写入。**本地文件，绝不参与同步**。
pub fn sync_session_file() -> PathBuf {
    sync_dir().join("session.json")
}

/// 记录当前编辑权状态：`read_only = true` 表示本机是只读镜像。
pub fn write_session_state(read_only: bool) {
    let enabled = get_webdav_config()
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    write_json(
        &sync_session_file(),
        &json!({
            "webdav_enabled": enabled,
            "read_only": read_only,
            "updated_at": chrono::Local::now().to_rfc3339(),
        }),
    );
}

/// MCP 写入准入：未启用 WebDAV → 允许；启用 → 仅当本机持锁（`read_only=false`）。
/// 返回 `Some(原因)` 表示拒绝写入。
pub fn mcp_write_denied() -> Option<String> {
    let sess = read_json(&sync_session_file());
    let enabled = sess
        .get("webdav_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| {
            get_webdav_config()
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        });
    if !enabled {
        return None;
    }
    let read_only = sess.get("read_only").and_then(|v| v.as_bool()).unwrap_or(true);
    if read_only {
        return Some(
            "本机当前是只读镜像（另一台机器持有同步锁）。请在持锁的机器上操作，\
             或先在软件里取得编辑权 / 关闭 WebDAV 同步，再让 agent 写入。"
                .to_string(),
        );
    }
    None
}

/// MCP 写入审计日志：`data/.mcp/audit.jsonl`，一行一次写入，方便回看 / 撤销。
/// **本地文件，绝不参与同步**。
pub fn audit_file() -> PathBuf {
    data_dir().join(".mcp").join("audit.jsonl")
}

pub fn append_audit(tool: &str, detail: &Value) {
    let path = audit_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let line = json!({
        "at": chrono::Local::now().to_rfc3339(),
        "tool": tool,
        "detail": detail,
    });
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write as _;
        let _ = writeln!(f, "{}", line);
    }
}

/// 通用文件保管箱：`data/vault/<kind>/<uuid>/`（不再分主机名）。
/// 与扫描证据解耦；删除条目时整个目录移入垃圾桶，绝不物理销毁。
pub fn vault_dir() -> PathBuf {
    data_dir().join("vault")
}

/// 软删除垃圾桶：删除条目时把归档目录整体移入，避免「删一条记录」
/// 变成「瞬间抹掉几个 G」的高风险操作。用户可在设置里查看体积并清空。
pub fn trash_dir() -> PathBuf {
    data_dir().join("trash")
}

/// 生成内部唯一标识（UUID v4）。`SW-ID` 只是给人看的显示号，
/// 所有内部绑定（保管箱 / 图标 / 合并 / 删除）一律以 uuid 为准。
pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 软件主程序图标目录：文件名 = `<uuid>.png`。扫描时从 exe 抽取，
/// 仅用于展示，不含任何敏感信息。一条记录一个图标，删除 / 合并跟着记录走，
/// 不再按软件名共享，也不再需要引用计数。
pub fn icons_dir() -> PathBuf {
    data_dir().join("icons")
}

/// 图标文件名 = `<uuid>.png`，与显示号 / 软件名无关。
pub fn icon_file_name(uuid: &str) -> String {
    format!("{}.png", uuid)
}

/// 读取 JSON，自动剥离 PowerShell 5.1 写入的 UTF-8 BOM。
pub fn read_json(path: &Path) -> Value {
    match fs::read_to_string(path) {
        Ok(raw) => {
            let s = raw.trim_start_matches('\u{feff}');
            serde_json::from_str(s).unwrap_or(Value::Null)
        }
        Err(_) => Value::Null,
    }
}

pub fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // 原子写：先写同目录临时文件再改名。GUI 与 MCP 是两个进程，
    // 直接 fs::write 会在并发时互相截断，改名则保证读者只看到完整文件。
    if let Ok(s) = serde_json::to_string_pretty(value) {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "data.json".to_string());
        let suffix = new_uuid();
        let tmp = path.with_file_name(format!(".{}.{}.tmp", file_name, &suffix[..8]));
        if fs::write(&tmp, s).is_ok() && fs::rename(&tmp, path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }
}

pub fn read_software() -> Vec<Value> {
    match read_json(&software_file()) {
        Value::Array(items) => items,
        _ => Vec::new(),
    }
}

pub fn write_software(items: &[Value]) {
    write_json(&software_file(), &Value::Array(items.to_vec()));
}

/// 台账数据版本指纹：software.json / ignored.json 的修改时间。
/// 前端轮询它判断是否被外部（如 MCP 写工具）改动，避免每次拉全量清单。
pub fn ledger_revision() -> String {
    let stamp = |p: &Path| -> String {
        match fs::metadata(p).and_then(|m| m.modified()) {
            Ok(t) => t
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos().to_string())
                .unwrap_or_else(|_| "0".to_string()),
            Err(_) => "0".to_string(),
        }
    };
    format!("{}:{}", stamp(&software_file()), stamp(&ignored_file()))
}

pub fn read_ignored() -> Vec<Value> {
    match read_json(&ignored_file()) {
        Value::Array(items) => items,
        _ => Vec::new(),
    }
}

pub fn write_ignored(items: &[Value]) {
    write_json(&ignored_file(), &Value::Array(items.to_vec()));
}

pub fn default_config() -> Value {
    // 不预置任何与具体机器 / 用户目录 / LLM 端点相关的值。
    // 便携发布时 data/config.json 会跟着 exe 走，预置具体值会把 A 机的信息带到 B 机。
    json!({
        "llm_url": "",
        "llm_model": "",
        "llm_api_key": "",
        // 扫描目录按主机名分键：每台机器只读自己那份，全量镜像也不会互相覆盖。
        "scan_directories": {},
        "machine_aliases": {},
        // MCP 写入开关：默认关（只读信息源）。开启后 agent 可改台账。
        "mcp_write_enabled": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_machine_specific_values() {
        // 默认配置不得预置具体机器 / 用户目录 / LLM 端点，否则便携拷机会泄漏 A 机信息。
        let cfg = default_config();
        assert_eq!(cfg["machine_aliases"], json!({}));
        assert_eq!(cfg["scan_directories"], json!({}));
        assert_eq!(cfg["llm_url"], "");
        assert_eq!(cfg["llm_model"], "");
    }

    #[test]
    fn icon_file_name_is_uuid_based() {
        let uuid = "0f8fad5b-d9cb-469f-a165-70867728950e";
        assert_eq!(icon_file_name(uuid), "0f8fad5b-d9cb-469f-a165-70867728950e.png");
    }

    #[test]
    fn read_software_is_a_plain_read() {
        let base = std::env::temp_dir().join(format!("ledger_plain_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("data")).unwrap();
        {
            let _guard = test_support::use_root(&base);
            fs::write(software_file(), r#"[{"id":"SW-001","name":"Git"}]"#).unwrap();
            // 不做任何兼容处理：原样读出，不偷偷补字段 / 改值
            let items = read_software();
            assert_eq!(items[0]["name"], "Git");
            assert!(items[0].get("uuid").is_none());
            assert_eq!(read_json(&software_file()), Value::Array(items.clone()));
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn read_software_ignores_evidence() {
        // evidence 与台账解耦：evidence 无论怎么改 / 删，read_software 都原样返回。
        let base = std::env::temp_dir().join(format!("ledger_decouple_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("data")).unwrap();
        {
            let _guard = test_support::use_root(&base);
            let guid = "0f8fad5b-d9cb-469f-a165-70867728950e";
            fs::write(
                software_file(),
                format!(
                    r#"[{{"id":"SW-001","name":"Git","machines":[{{"machine_id":"{guid}","form":"installed"}}]}}]"#
                ),
            )
            .unwrap();
            // 故意放一个内容完全无关的 machine-info.json
            let ev = evidence_dir().join(guid);
            fs::create_dir_all(&ev).unwrap();
            fs::write(
                ev.join("machine-info.json"),
                r#"{"machine_id":"WRONG","hostname":"NOT-PC"}"#,
            )
            .unwrap();
            assert_eq!(read_software()[0]["machines"][0]["machine_id"], guid);

            // 整个 evidence 删掉，行为不变
            fs::remove_dir_all(evidence_dir()).unwrap();
            assert_eq!(read_software()[0]["machines"][0]["machine_id"], guid);
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn ledger_revision_changes_after_write() {
        let base = std::env::temp_dir().join(format!("ledger_rev_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("data")).unwrap();
        {
            let _guard = test_support::use_root(&base);
            let before = ledger_revision();
            write_software(&[json!({ "id": "SW-001", "name": "Zed" })]);
            assert_ne!(before, ledger_revision());
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn data_paths_live_under_root() {
        let base = std::env::temp_dir().join(format!("ledger_root_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        {
            let _guard = test_support::use_root(&base);
            assert_eq!(data_dir(), base.join("data"));
            assert_eq!(software_file(), base.join("data").join("software.json"));
            assert_eq!(config_file(), base.join("data").join("config.json"));
        }
        let _ = fs::remove_dir_all(&base);
    }
}

pub fn get_config() -> Value {
    let mut cfg = default_config();
    if let (Value::Object(base), Value::Object(over)) = (&mut cfg, read_json(&config_file())) {
        for (k, v) in over {
            base.insert(k, v);
        }
    }
    cfg
}

pub fn save_config(value: &Value) {
    write_json(&config_file(), value);
}
