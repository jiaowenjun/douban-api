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
    #[error("网络异常: {0}")]
    Network(String),
}

impl FetchError {
    /// 任务级错误码（设计文档第 9 节）
    pub fn code(&self) -> &'static str {
        match self {
            FetchError::NotFound => "PAGE_NOT_FOUND",
            FetchError::Timeout => "DOUBAN_TIMEOUT",
            FetchError::Blocked(_) | FetchError::Suspicious(_) | FetchError::Network(_) => {
                "DOUBAN_BLOCKED"
            }
        }
    }

    /// 是否可重试（对应 retry_policy 的 Transient 分类）
    pub fn is_transient(&self) -> bool {
        !matches!(self, FetchError::NotFound)
    }
}

pub trait DoubanFetcher: Send + Sync {
    fn fetch_page(
        &self,
        url: &str,
        referer: &str,
    ) -> impl std::future::Future<Output = Result<String, FetchError>> + Send;
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
}
