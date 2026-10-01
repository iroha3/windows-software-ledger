use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// 测试之间串行化对 `SOFTWARE_LEDGER_ROOT` 的修改，避免并行测试互相干扰。
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 数据根目录：优先环境变量，其次从 exe 向上寻找 package.json / data 目录，
/// 这样开发时（target/debug）与打包后（exe 同级放 data/）都能命中同一个项目目录。
pub fn app_root() -> PathBuf {
    if let Ok(p) = std::env::var("SOFTWARE_LEDGER_ROOT") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(start) = exe.parent() {
            return resolve_root_from(start);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 从给定目录向上寻找含 `data/` 或 `package.json` 的目录；
/// 便携场景下 exe 与 `data/` 同级，会在第一层直接命中。
fn resolve_root_from(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    for _ in 0..8 {
        if dir.join("package.json").exists() || dir.join("data").exists() {
            return dir;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break,
        }
    }
    start.to_path_buf()
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
    fn resolves_portable_layout() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SOFTWARE_LEDGER_ROOT");
        let base = std::env::temp_dir().join(format!("ledger_root_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        // exe 与 data/ 同级 -> 直接命中 exe 目录
        let portable = base.join("app");
        fs::create_dir_all(portable.join("data")).unwrap();
        assert_eq!(resolve_root_from(&portable), portable);

        // 只有 exe、data/ 尚未创建 -> 回退到 exe 同级
        let fresh = base.join("fresh");
        fs::create_dir_all(&fresh).unwrap();
        assert_eq!(resolve_root_from(&fresh), fresh);

        // 嵌套子目录向上回溯
        let nested = base.join("nested");
        fs::create_dir_all(nested.join("data")).unwrap();
        let deep = nested.join("a").join("b");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(resolve_root_from(&deep), nested);

        // 实际 exe 路径解析：测试二进制位于 target/debug/deps，
        // 应回溯到仓库根目录（含 package.json / data），而非任何临时目录。
        let real = app_root();
        assert!(real.join("package.json").exists(), "app_root={:?}", real);
        assert!(real.join("data").exists(), "app_root={:?}", real);
        assert_eq!(evidence_dir(), real.join("evidence"));
        assert_eq!(scripts_dir(), real.join("scripts"));

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
