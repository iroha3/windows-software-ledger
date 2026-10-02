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

/// 浏览器扩展的用户层标注（备注等）。与采集证据分离，重扫不会覆盖。
pub fn extensions_file() -> PathBuf {
    data_dir().join("extensions.json")
}

/// 读取扩展标注：始终返回对象（键 = 扩展 ID）。
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

/// 通用文件保管箱：`data/vault/<主机名>/<kind>/<id>/`。
/// 与扫描证据解耦，删除软件时可级联清理。
pub fn vault_dir() -> PathBuf {
    data_dir().join("vault")
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
    if let Ok(s) = serde_json::to_string_pretty(value) {
        let _ = fs::write(path, s);
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
        "scan_directories": [],
        "machine_aliases": {}
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
        assert_eq!(cfg["scan_directories"], json!([]));
        assert_eq!(cfg["llm_url"], "");
        assert_eq!(cfg["llm_model"], "");
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
