//! 封面代理与磁盘缓存——cache-then-serve。
//!
//! 豆瓣图片 CDN 各子域名 Referer 防盗链策略不一致：浏览器直连时 img9 放行、
//! img2/img3 一律 403（2026-10 实测），导致前端封面大量灰块。本模块在服务端
//! 带豆瓣 Referer 回源一次并落盘 `data/covers/`，之后永久本地化：
//! 存量记录首次访问即自动"回填"，浏览不再外呼豆瓣、离线可用。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Semaphore;

use super::error::AppError;
use crate::crawler::UA;

/// 允许回源的路径前缀（库内三类豆瓣图：电影海报 /view/photo/、书音封面
/// /view/subject/、用户头像 /icon/——浏览器直连头像域 418，必须代理）
const ALLOWED_PATH_PREFIXES: [&str; 3] = ["/view/photo/", "/view/subject/", "/icon/"];

/// 回源时伪装的豆瓣来源页（实测 www/movie 域 Referer 对全部 img 子域名均放行）
const ORIGIN_REFERER: &str = "https://www.douban.com/";

#[derive(Clone)]
pub struct CoverService {
    root: PathBuf,
    client: reqwest::Client,
    /// 回源并发上限——豆瓣 CDN 对快速连发敏感（实测连续请求时偶发空响应），
    /// 限额排队避免一页 20 张封面同时回源触发限流
    origin_slots: Arc<Semaphore>,
    tmp_seq: Arc<AtomicU64>,
}

pub struct Cover {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

impl CoverService {
    pub fn new(data_dir: &Path, concurrency: u32) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("build reqwest client");
        Self {
            root: data_dir.join("covers"),
            client,
            origin_slots: Arc::new(Semaphore::new(concurrency.max(1) as usize)),
            tmp_seq: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 取封面：磁盘命中直接返回；未命中则限流回源、落盘后返回。
    pub async fn get(&self, url: &str) -> Result<Cover, AppError> {
        let (url, filename, content_type) = parse_cover_url(url)?;
        let path = self.root.join(&filename);
        if let Some(bytes) = read_nonempty(&path).await {
            return Ok(Cover {
                bytes,
                content_type,
            });
        }

        let _permit = self.origin_slots.acquire().await.expect("semaphore 未关闭");
        // 拿到许可后复查：并发同图时后到者直接命中先到者落盘的结果
        if let Some(bytes) = read_nonempty(&path).await {
            return Ok(Cover {
                bytes,
                content_type,
            });
        }

        let resp = self
            .client
            .get(&url)
            .header(reqwest::header::REFERER, ORIGIN_REFERER)
            .send()
            .await
            .map_err(|e| AppError::CoverUpstream(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(AppError::CoverUpstream(format!("HTTP {}", resp.status())));
        }
        // 防御错误页落盘：只接受声明为图片的响应
        let upstream_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !upstream_type.starts_with("image/") {
            return Err(AppError::CoverUpstream(format!(
                "非图片响应: {upstream_type}"
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::CoverUpstream(e.to_string()))?
            .to_vec();
        if bytes.is_empty() {
            return Err(AppError::CoverUpstream("空响应".to_string()));
        }

        self.save(&filename, &bytes).await?;
        Ok(Cover {
            bytes,
            content_type,
        })
    }

    /// 落盘：写临时文件后原子改名，避免并发读到半截内容
    async fn save(&self, filename: &str, bytes: &[u8]) -> Result<(), AppError> {
        tokio::fs::create_dir_all(&self.root).await?;
        let tmp = self.root.join(format!(
            "{filename}.tmp-{}",
            self.tmp_seq.fetch_add(1, Ordering::Relaxed)
        ));
        tokio::fs::write(&tmp, bytes).await?;
        if tokio::fs::rename(&tmp, self.root.join(filename))
            .await
            .is_err()
        {
            // 并发下他人已抢先改名，本份内容一致，丢弃即可
            let _ = tokio::fs::remove_file(&tmp).await;
        }
        Ok(())
    }
}

/// 校验封面 URL 并解析出（规范化 URL、缓存文件名、Content-Type）。
/// 白名单必须收紧：本端点本质是代理，放任意 URL 会变成开放中继。
pub fn parse_cover_url(url: &str) -> Result<(String, String, &'static str), AppError> {
    let bad = |msg: &str| AppError::InvalidArgument(format!("非法封面 URL（{msg}）"));

    let parsed = url::Url::parse(url).map_err(|_| bad("无法解析"))?;
    if parsed.scheme() != "https" {
        return Err(bad("仅允许 https"));
    }
    let Some(host) = parsed.host_str() else {
        return Err(bad("缺少主机名"));
    };
    if !host.ends_with(".doubanio.com") {
        return Err(bad("仅允许 doubanio.com 图片域"));
    }
    let path = parsed.path();
    if !ALLOWED_PATH_PREFIXES.iter().any(|p| path.starts_with(p)) {
        return Err(bad("路径前缀不在白名单"));
    }
    let filename = path.rsplit('/').next().unwrap_or_default();
    let Some(content_type) = content_type_for(filename) else {
        return Err(bad("文件名或扩展名非法"));
    };
    Ok((parsed.to_string(), filename.to_string(), content_type))
}

/// 缓存文件名合法性：非空、不以点开头、仅字母数字与 `-_`、扩展名受限。
/// 同时天然排除路径分隔符与 `..` 穿越。
fn content_type_for(filename: &str) -> Option<&'static str> {
    let (stem, ext) = filename.rsplit_once('.')?;
    if stem.is_empty()
        || !stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    match ext.to_ascii_lowercase().as_str() {
        "webp" => Some("image/webp"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

async fn read_nonempty(path: &Path) -> Option<Vec<u8>> {
    let bytes = tokio::fs::read(path).await.ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

/// 把库里的豆瓣封面 URL 改写为本地代理端点（空值原样返回）。
/// 相对路径，前端经自身反向代理（dev 下 vite proxy）转发到本服务。
pub fn cover_proxy_url(cover: &str) -> String {
    if cover.is_empty() {
        return String::new();
    }
    format!("/api/v1/covers?u={}", encode_query_component(cover))
}

/// 查询参数百分号编码（保留 RFC 3986 unreserved 字符）
pub fn encode_query_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// --------------------------------------------------------------------------- //
// 测试
// --------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_known_cover_shapes() {
        let (url, name, ct) = parse_cover_url(
            "https://img3.doubanio.com/view/photo/s_ratio_poster/public/p2935834896.webp",
        )
        .unwrap();
        assert_eq!(name, "p2935834896.webp");
        assert_eq!(ct, "image/webp");
        assert_eq!(
            url.as_str(),
            "https://img3.doubanio.com/view/photo/s_ratio_poster/public/p2935834896.webp"
        );

        let (_, name, ct) =
            parse_cover_url("https://img2.doubanio.com/view/subject/s/public/s27018761.jpg")
                .unwrap();
        assert_eq!(name, "s27018761.jpg");
        assert_eq!(ct, "image/jpeg");
        // 用户头像（文件名带版本号后缀，stem 校验允许 `-`）
        let (_, name, ct) =
            parse_cover_url("https://img2.doubanio.com/icon/u189180823-11.jpg").unwrap();
        assert_eq!(name, "u189180823-11.jpg");
        assert_eq!(ct, "image/jpeg");
    }

    #[test]
    fn parse_rejects_non_whitelisted() {
        // 非豆瓣域（开放中继风险）
        assert!(parse_cover_url("https://evil.com/view/photo/p1.webp").is_err());
        // http 明文
        assert!(parse_cover_url("http://img3.doubanio.com/view/photo/p1.webp").is_err());
        // 路径前缀不在白名单
        assert!(parse_cover_url("https://img3.doubanio.com/secret/p1.webp").is_err());
        assert!(parse_cover_url("https://img3.doubanio.com/img/music/p1.webp").is_err());
        // 路径穿越 / 无扩展名 / 非法扩展名 / 点开头
        assert!(parse_cover_url("https://img3.doubanio.com/view/photo/../../etc/passwd").is_err());
        assert!(parse_cover_url("https://img3.doubanio.com/view/photo/public/p1").is_err());
        assert!(parse_cover_url("https://img3.doubanio.com/view/photo/public/p1.html").is_err());
        assert!(
            parse_cover_url("https://img3.doubanio.com/view/photo/public/.hidden.webp").is_err()
        );
        // 无法解析
        assert!(parse_cover_url("not a url").is_err());
    }

    #[test]
    fn proxy_url_and_encoding() {
        assert_eq!(cover_proxy_url(""), "");
        assert_eq!(
            cover_proxy_url("https://img3.doubanio.com/view/photo/s/public/p1.webp"),
            "/api/v1/covers?u=https%3A%2F%2Fimg3.doubanio.com%2Fview%2Fphoto%2Fs%2Fpublic%2Fp1.webp"
        );
        assert_eq!(encode_query_component("aZ09-._~"), "aZ09-._~");
    }
}
