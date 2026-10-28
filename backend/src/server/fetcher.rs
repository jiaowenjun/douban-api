//! 豆瓣页面抓取抽象——生产用 reqwest 异步实现，测试可注入 fake。

use crate::crawler::{BASE_HEADERS, UA};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// 404：条目/用户不存在，永久失败
    #[error("页面不存在（404）")]
    NotFound,
    /// 403/418/5xx：被豆瓣限流
    #[error("HTTP {0}")]
    Blocked(u16),
    #[error("请求超时")]
    Timeout,
    #[error("响应过短({0}B)疑似拦截")]
    Suspicious(usize),
    /// 解析哨兵命中：页面正常返回但内容与约定结构矛盾
    /// （改版/登录墙/JS challenge 会以 200+0 条呈现），重试无意义
    #[error("页面解析异常: {0}")]
    ParseAnomaly(String),
    #[error("网络异常: {0}")]
    Network(String),
}

impl FetchError {
    /// 任务级错误码（设计文档第 9 节）
    pub fn code(&self) -> &'static str {
        match self {
            FetchError::NotFound => "PAGE_NOT_FOUND",
            FetchError::Timeout => "DOUBAN_TIMEOUT",
            FetchError::ParseAnomaly(_) => "PARSE_ERROR",
            FetchError::Blocked(_) | FetchError::Suspicious(_) | FetchError::Network(_) => {
                "DOUBAN_BLOCKED"
            }
        }
    }

    /// 是否可重试（对应 retry_policy 的 Transient 分类）。
    /// ParseAnomaly 与 NotFound 一样按永久失败处理——确定性结构证据，
    /// 且不应计入熔断（不是豆瓣拒绝服务）。
    pub fn is_transient(&self) -> bool {
        !matches!(self, FetchError::NotFound | FetchError::ParseAnomaly(_))
    }
}

pub trait DoubanFetcher: Send + Sync {
    fn fetch_page(
        &self,
        url: &str,
        referer: &str,
    ) -> impl std::future::Future<Output = Result<String, FetchError>> + Send;

    /// 抓条目结构化元信息（m.douban.com rexxar 内部 JSON 接口，免登录）。
    /// 校验语义：200 + JSON 合法 + 含 title 键（不能沿用页面的 >5KB 字节阈值，
    /// rexxar 响应可小至数 KB）。
    fn fetch_subject(
        &self,
        category: &str,
        subject_id: &str,
    ) -> impl std::future::Future<Output = Result<serde_json::Value, FetchError>> + Send;

    /// 抓个人书影音动态 RSS。默认实现复用 fetch_page（测试 fake 走此路径即可）；
    /// 生产 ReqwestFetcher 已覆盖——RSS 仅数 KB，不能套用页面抓取的
    /// >5KB 反爬阈值，改以 XML 特征校验（looks_like_feed）。
    fn fetch_feed(
        &self,
        uid: &str,
    ) -> impl std::future::Future<Output = Result<String, FetchError>> + Send {
        let url = crate::crawler::feed_url(uid);
        let referer = crate::crawler::referer_for(uid);
        async move { self.fetch_page(&url, &referer).await }
    }
}

pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl Default for ReqwestFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        Self::with_cookie("")
    }

    /// 带豆瓣登录 Cookie 构造（如 `dbcl2="..."`）。
    /// 免登录抓取看不到被豆瓣隐藏的条目（计数含但列表不渲染），
    /// 配置 Cookie 后以登录身份抓取即可补全；留空保持免登录行为。
    pub fn with_cookie(cookie: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        for (k, v) in BASE_HEADERS {
            headers.insert(k, v.parse().expect("static header"));
        }
        if !cookie.is_empty() {
            headers.insert(
                reqwest::header::COOKIE,
                cookie.parse().expect("合法 cookie 头"),
            );
        }
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("build reqwest client");
        Self { client }
    }
}

impl DoubanFetcher for ReqwestFetcher {
    async fn fetch_page(&self, url: &str, referer: &str) -> Result<String, FetchError> {
        let resp = self
            .client
            .get(url)
            .header("Referer", referer)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    FetchError::Timeout
                } else {
                    FetchError::Network(e.to_string())
                }
            })?;

        let code = resp.status().as_u16();
        if code == 404 {
            return Err(FetchError::NotFound);
        }
        if !resp.status().is_success() {
            return Err(FetchError::Blocked(code));
        }
        let html = String::from_utf8_lossy(&resp.bytes().await.map_err(|e| FetchError::Network(e.to_string()))?)
            .into_owned();
        if html.len() <= 5000 {
            return Err(FetchError::Suspicious(html.len())); // 正常页面 >30KB
        }
        Ok(html)
    }

    async fn fetch_subject(
        &self,
        category: &str,
        subject_id: &str,
    ) -> Result<serde_json::Value, FetchError> {
        let resp = self
            .client
            .get(crate::server::subject::rexxar_url(category, subject_id))
            // rexxar 校验移动端来源：Referer 必须指向 m.douban.com，UA 用移动端
            //（请求级 header 覆盖 client 级桌面默认值）
            .header("Referer", crate::server::subject::REXXAR_REFERER)
            .header(reqwest::header::USER_AGENT, crate::server::subject::MOBILE_UA)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    FetchError::Timeout
                } else {
                    FetchError::Network(e.to_string())
                }
            })?;

        let code = resp.status().as_u16();
        if code == 404 {
            return Err(FetchError::NotFound);
        }
        if !resp.status().is_success() {
            return Err(FetchError::Blocked(code));
        }
        let text = String::from_utf8_lossy(&resp.bytes().await.map_err(|e| FetchError::Network(e.to_string()))?)
            .into_owned();
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| FetchError::Suspicious(text.len()))?;
        if value.get("title").is_none() {
            return Err(FetchError::Suspicious(text.len()));
        }
        Ok(value)
    }

    /// RSS 抓取：与页面的差异在真伪判定——RSS 仅数 KB，>5KB 字节阈值
    /// 不适用，改以 XML 特征校验（looks_like_feed）。
    async fn fetch_feed(&self, uid: &str) -> Result<String, FetchError> {
        let resp = self
            .client
            .get(crate::crawler::feed_url(uid))
            .header("Referer", crate::crawler::referer_for(uid))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    FetchError::Timeout
                } else {
                    FetchError::Network(e.to_string())
                }
            })?;

        let code = resp.status().as_u16();
        if code == 404 {
            return Err(FetchError::NotFound);
        }
        if !resp.status().is_success() {
            return Err(FetchError::Blocked(code));
        }
        let text = String::from_utf8_lossy(&resp.bytes().await.map_err(|e| FetchError::Network(e.to_string()))?)
            .into_owned();
        if !looks_like_feed(&text) {
            return Err(FetchError::Suspicious(text.len()));
        }
        Ok(text)
    }
}

/// RSS 响应真伪判定：合法 feed 以 XML 声明或 `<rss>` 根元素开头
/// （容忍 BOM 与前导空白）；拦截页/HTML 错误页不满足该特征。
pub fn looks_like_feed(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with("<?xml") || t.starts_with("<rss")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_feed_accepts_real_shapes() {
        // 2026-10-28 实抓 feed 开头（含 BOM/前导空白容忍）
        assert!(looks_like_feed("<?xml version=\"1.0\" encoding=\"UTF-8\"?><rss version=\"2.0\">"));
        assert!(looks_like_feed("<rss version=\"2.0\"><channel>"));
        assert!(looks_like_feed("\u{feff}\n  <?xml version=\"1.0\"?>"));
    }

    #[test]
    fn looks_like_feed_rejects_non_xml() {
        // 拦截页（200 + HTML）与空响应都不能当 feed 用
        assert!(!looks_like_feed("<!DOCTYPE html><html><body>blocked</body></html>"));
        assert!(!looks_like_feed(""));
        assert!(!looks_like_feed("请开启 JavaScript"));
    }
}
