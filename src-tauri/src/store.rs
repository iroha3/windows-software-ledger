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
pub fn evidence_dir() -> PathBuf {
    app_root().join("evidence")
}
pub fn scripts_dir() -> PathBuf {
    app_root().join("scripts")
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

pub fn default_config() -> Value {
    json!({
        "llm_url": "http://127.0.0.1:1234/v1/chat/completions",
        "llm_model": "qwen3.5-4b",
        "llm_api_key": "",
        "scan_directories": [
            "D:\\Portable",
            "D:\\Tools",
            "D:\\Software",
            "E:\\Portable",
            "E:\\Tools",
            "E:\\Software",
            "C:\\Software",
            "C:\\Portable"
        ],
        "machine_aliases": {
            "DESKTOP-HEGVCTR": "台式工作站"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(evidence_dir(), base.join("evidence"));
            assert_eq!(scripts_dir(), base.join("scripts"));
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
