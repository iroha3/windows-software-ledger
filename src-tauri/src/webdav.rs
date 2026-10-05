// WebDAV 传输层：本项目的同步只需要 GET / PUT / DELETE / MKCOL 四个动作，
// 刻意不用 PROPFIND（避免引入 XML 解析依赖）。远端文件清单由我们自己的
// `manifest.json` 维护，因此不需要向服务器列目录。
//
// 这里用 reqwest 的 blocking 客户端：同步整体跑在 spawn_blocking 里，
// 上传直接用 `Body::from(File)` 流式发送，下载 `copy_to` 边收边落盘，
// 几个 G 的归档也不会整份读进内存。

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

/// 把路径片段做百分号编码（保留 `/`），中文文件名 / 空格都能安全放进 URL。
fn encode_path(p: &str) -> String {
    let mut out = String::new();
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b'/' => out.push('/'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub struct WebDav {
    client: reqwest::blocking::Client,
    base: String,
    user: String,
    pass: String,
    /// 已确认存在的远端目录，避免每次上传都重复 MKCOL。
    created: RefCell<HashSet<String>>,
}

impl WebDav {
    pub fn new(url: &str, user: &str, pass: &str) -> Result<Self, String> {
        let mut base = url.trim().to_string();
        if base.is_empty() {
            return Err("未配置 WebDAV 地址".to_string());
        }
        if !base.starts_with("http://") && !base.starts_with("https://") {
            base = format!("http://{}", base);
        }
        let base = base.trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|e| format!("初始化 HTTP 客户端失败: {}", e))?;
        Ok(Self {
            client,
            base,
            user: user.to_string(),
            pass: pass.to_string(),
            created: RefCell::new(HashSet::new()),
        })
    }

    fn url(&self, rel: &str) -> String {
        let rel = rel.trim_start_matches('/');
        if rel.is_empty() {
            self.base.clone()
        } else {
            format!("{}/{}", self.base, encode_path(rel))
        }
    }

    fn request(&self, method: reqwest::Method, rel: &str) -> reqwest::blocking::RequestBuilder {
        let rb = self.client.request(method, self.url(rel));
        if self.user.is_empty() {
            rb
        } else {
            rb.basic_auth(&self.user, Some(&self.pass))
        }
    }

    /// 确保远端根目录存在（已存在时 MKCOL 返回 405，忽略即可）。
    pub fn ensure_base(&self) {
        let rb = self
            .client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), self.base.clone());
        let rb = if self.user.is_empty() {
            rb
        } else {
            rb.basic_auth(&self.user, Some(&self.pass))
        };
        let _ = rb.send();
    }

    /// 读取文件；404 返回 `Ok(None)`（用于「远端尚无 manifest」判断）。
    pub fn get(&self, rel: &str) -> Result<Option<Vec<u8>>, String> {
        let resp = self
            .request(reqwest::Method::GET, rel)
            .send()
            .map_err(|e| format!("GET {} 失败: {}", rel, e))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(format!("GET {} 返回 {}", rel, resp.status()));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        Ok(Some(bytes))
    }

    pub fn get_json(&self, rel: &str) -> Result<Option<serde_json::Value>, String> {
        match self.get(rel)? {
            Some(b) => Ok(Some(
                serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null),
            )),
            None => Ok(None),
        }
    }

    /// 直接流式下载到本地文件，返回是否下载成功。
    pub fn download_to(&self, rel: &str, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut resp = self
            .request(reqwest::Method::GET, rel)
            .send()
            .map_err(|e| format!("下载 {} 失败: {}", rel, e))?;
        if !resp.status().is_success() {
            return Err(format!("下载 {} 返回 {}", rel, resp.status()));
        }
        let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut resp, &mut file).map_err(|e| format!("写入 {} 失败: {}", rel, e))?;
        Ok(())
    }

    pub fn put(&self, rel: &str, body: Vec<u8>) -> Result<(), String> {
        self.ensure_parents(rel);
        let resp = self
            .request(reqwest::Method::PUT, rel)
            .body(body)
            .send()
            .map_err(|e| format!("PUT {} 失败: {}", rel, e))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("PUT {} 返回 {}", rel, resp.status()))
        }
    }

    pub fn put_file(&self, rel: &str, path: &Path) -> Result<(), String> {
        self.ensure_parents(rel);
        let file = std::fs::File::open(path).map_err(|e| format!("打开 {} 失败: {}", rel, e))?;
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let body = reqwest::blocking::Body::sized(file, len);
        let resp = self
            .request(reqwest::Method::PUT, rel)
            .body(body)
            .send()
            .map_err(|e| format!("上传 {} 失败: {}", rel, e))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("上传 {} 返回 {}", rel, resp.status()))
        }
    }

    pub fn delete(&self, rel: &str) -> Result<(), String> {
        let resp = self
            .request(reqwest::Method::DELETE, rel)
            .send()
            .map_err(|e| format!("DELETE {} 失败: {}", rel, e))?;
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(format!("DELETE {} 返回 {}", rel, resp.status()))
        }
    }

    /// 服务器端移动（用于强制覆盖前的远端快照）；失败仅记录，不阻断。
    pub fn move_to(&self, from: &str, to: &str) -> Result<(), String> {
        self.ensure_parents(to);
        let from_url = self.url(from);
        let to_url = self.url(to);
        let rb = self
            .client
            .request(reqwest::Method::from_bytes(b"MOVE").unwrap(), from_url);
        let rb = if self.user.is_empty() {
            rb
        } else {
            rb.basic_auth(&self.user, Some(&self.pass))
        };
        let resp = rb
            .header("Destination", to_url)
            .header("Overwrite", "T")
            .send()
            .map_err(|e| format!("MOVE {} 失败: {}", from, e))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("MOVE {} 返回 {}", from, resp.status()))
        }
    }

    /// 逐级 MKCOL 父目录；已存在会被忽略，成功过的目录记进缓存不再重试。
    fn ensure_parents(&self, rel: &str) {
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.len() <= 1 {
            return;
        }
        let mut cur = String::new();
        for seg in &parts[..parts.len() - 1] {
            if seg.is_empty() {
                continue;
            }
            if !cur.is_empty() {
                cur.push('/');
            }
            cur.push_str(seg);
            if self.created.borrow().contains(&cur) {
                continue;
            }
            let rb = self.client.request(
                reqwest::Method::from_bytes(b"MKCOL").unwrap(),
                self.url(&cur),
            );
            let rb = if self.user.is_empty() {
                rb
            } else {
                rb.basic_auth(&self.user, Some(&self.pass))
            };
            let _ = rb.send();
            self.created.borrow_mut().insert(cur.clone());
        }
    }

    /// 连通性 / 认证测试：manifest.json 的 404 也算「服务器可达」。
    pub fn test(&self) -> Result<(), String> {
        let resp = self
            .request(reqwest::Method::GET, "manifest.json")
            .send()
            .map_err(|e| format!("连接失败: {}", e))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(format!("认证失败 ({})", status.as_u16()));
        }
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(format!("服务器返回 {}", status.as_u16()))
        }
    }
}
