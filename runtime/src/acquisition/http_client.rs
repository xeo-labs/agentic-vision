//! Async HTTP client wrapping reqwest.
//!
//! Not a browser — just HTTP requests. Handles redirects, timeouts,
//! retry on 5xx, and exponential backoff on 429.

use anyhow::Result;
use std::time::Duration;

/// Response from an HTTP GET request.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// Original requested URL.
    pub url: String,
    /// Final URL after redirects.
    pub final_url: String,
    /// HTTP status code.
    pub status: u16,
    /// Response headers (selected subset).
    pub headers: Vec<(String, String)>,
    /// Response body as text.
    pub body: String,
}

/// Response from an HTTP HEAD request.
#[derive(Debug, Clone)]
pub struct HeadResponse {
    /// Requested URL.
    pub url: String,
    /// HTTP status code.
    pub status: u16,
    /// Content-Type header.
    pub content_type: Option<String>,
    /// Content-Language header.
    pub content_language: Option<String>,
    /// Last-Modified header.
    pub last_modified: Option<String>,
    /// Cache-Control header.
    pub cache_control: Option<String>,
}

/// HTTP client for the acquisition engine.
#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    /// HTTP/1.1-only fallback client for sites that reject HTTP/2.
    h1_client: reqwest::Client,
}

impl HttpClient {
    /// Create a new HTTP client with standard Chrome user-agent.
    pub fn new(timeout_ms: u64) -> Self {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                  AppleWebKit/537.36 (KHTML, like Gecko) \
                  Chrome/131.0.0.0 Safari/537.36";

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(ua)
            .build()
            .unwrap_or_default();

        let h1_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(ua)
            .http1_only()
            .build()
            .unwrap_or_default();

        Self { client, h1_client }
    }

    /// Perform a single GET request with retry on 5xx and backoff on 429.
    ///
    /// Falls back to HTTP/1.1 on protocol errors (some CDNs reject HTTP/2).
    pub async fn get(&self, url: &str, timeout_ms: u64) -> Result<HttpResponse> {
        match self.get_inner(&self.client, url, timeout_ms).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                // If the error looks like a protocol issue, retry with HTTP/1.1
                let err_str = format!("{e}");
                if err_str.contains("http2")
                    || err_str.contains("protocol")
                    || err_str.contains("connection closed")
                {
                    self.get_inner(&self.h1_client, url, timeout_ms).await
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn get_inner(
        &self,
        client: &reqwest::Client,
        url: &str,
        timeout_ms: u64,
    ) -> Result<HttpResponse> {
        let mut retries = 0u32;
        let max_retries = 2;

        loop {
            let resp = client
                .get(url)
                .timeout(Duration::from_millis(timeout_ms))
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let status = r.status().as_u16();
                    let final_url = r.url().to_string();

                    // Retry on 5xx
                    if status >= 500 && retries < max_retries {
                        retries += 1;
                        let delay = Duration::from_millis(500 * 2u64.pow(retries - 1));
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    // Backoff on 429
                    if status == 429 && retries < max_retries {
                        retries += 1;
                        let retry_after = r
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(2);
                        let delay = Duration::from_secs(retry_after.min(10));
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    let headers: Vec<(String, String)> = r
                        .headers()
                        .iter()
                        .filter(|(k, _)| {
                            matches!(
                                k.as_str(),
                                "content-type"
                                    | "content-language"
                                    | "last-modified"
                                    | "cache-control"
                                    | "x-robots-tag"
                            )
                        })
                        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                        .collect();

                    let body = r.text().await.unwrap_or_default();

                    return Ok(HttpResponse {
                        url: url.to_string(),
                        final_url,
                        status,
                        headers,
                        body,
                    });
                }
                Err(e) => {
                    if retries < max_retries {
                        retries += 1;
                        let delay = Duration::from_millis(500 * 2u64.pow(retries - 1));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
    }

    /// POST form data (url-encoded) and return a response with all headers.
    ///
    /// Unlike `get()`, this captures *all* response headers (not a filtered
    /// subset), because callers like the auth module need `set-cookie` headers.
    pub async fn post_form(
        &self,
        url: &str,
        form_fields: &[(String, String)],
        extra_headers: &[(String, String)],
        timeout_ms: u64,
    ) -> Result<HttpResponse> {
        let mut builder = self
            .client
            .post(url)
            .timeout(Duration::from_millis(timeout_ms));

        for (name, value) in extra_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        builder = builder.form(form_fields);

        let r = builder.send().await?;
        let status = r.status().as_u16();
        let final_url = r.url().to_string();

        let headers: Vec<(String, String)> = r
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body = r.text().await.unwrap_or_default();

        Ok(HttpResponse {
            url: url.to_string(),
            final_url,
            status,
            headers,
            body,
        })
    }

    /// Perform parallel GET requests with bounded concurrency.
    pub async fn get_many(
        &self,
        urls: &[String],
        concurrency: usize,
        timeout_ms: u64,
    ) -> Vec<Result<HttpResponse>> {
        use futures::stream::{self, StreamExt};

        let results: Vec<Result<HttpResponse>> = stream::iter(urls.iter())
            .map(|url| {
                let client = self.clone();
                let u = url.clone();
                let t = timeout_ms;
                async move { client.get(&u, t).await }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        results
    }

    /// Perform parallel HEAD requests with bounded concurrency.
    pub async fn head_many(
        &self,
        urls: &[String],
        concurrency: usize,
    ) -> Vec<Result<HeadResponse>> {
        use futures::stream::{self, StreamExt};

        let results: Vec<Result<HeadResponse>> = stream::iter(urls.iter())
            .map(|url| {
                let client = self.client.clone();
                let u = url.clone();
                async move {
                    let resp = client
                        .head(&u)
                        .timeout(Duration::from_secs(10))
                        .send()
                        .await?;

                    let status = resp.status().as_u16();
                    let content_type = resp
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let content_language = resp
                        .headers()
                        .get("content-language")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let last_modified = resp
                        .headers()
                        .get("last-modified")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let cache_control = resp
                        .headers()
                        .get("cache-control")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());

                    Ok(HeadResponse {
                        url: u,
                        status,
                        content_type,
                        content_language,
                        last_modified,
                        cache_control,
                    })
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_creation() {
        let client = HttpClient::new(10000);
        // Just verify it doesn't panic
        let _ = client;
    }

    #[test]
    fn test_head_response_defaults() {
        let resp = HeadResponse {
            url: "https://example.com".to_string(),
            status: 200,
            content_type: Some("text/html".to_string()),
            content_language: None,
            last_modified: None,
            cache_control: None,
        };
        assert_eq!(resp.status, 200);
    }
}
