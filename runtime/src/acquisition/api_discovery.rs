//! Known public API discovery.
//!
//! Ships a mapping of well-known domains to their public APIs.
//! For known domains, tries the API for richer data. Unknown domains skip silently.

use super::http_client::HttpClient;
use serde_json::Value;
use std::collections::HashMap;

/// A record returned from a public API.
#[derive(Debug, Clone)]
pub struct ApiRecord {
    /// The original URL this record corresponds to.
    pub url: String,
    /// The API endpoint that was queried.
    pub api_url: String,
    /// Raw JSON response from the API.
    pub data: Value,
}

/// Known API configurations.
struct KnownApi {
    /// API URL template. Use `{path}` for the URL path portion.
    api_template: &'static str,
    /// Type of API: "rest" (full URL template) or "json_suffix" (append .json).
    api_type: &'static str,
}

fn known_apis() -> HashMap<&'static str, KnownApi> {
    let mut m = HashMap::new();
    m.insert(
        "en.wikipedia.org",
        KnownApi {
            api_template: "https://en.wikipedia.org/api/rest_v1/page/summary/{title}",
            api_type: "rest",
        },
    );
    m.insert(
        "github.com",
        KnownApi {
            api_template: "https://api.github.com/repos/{owner}/{repo}",
            api_type: "rest",
        },
    );
    m.insert(
        "reddit.com",
        KnownApi {
            api_template: "{url}.json",
            api_type: "json_suffix",
        },
    );
    m.insert(
        "www.reddit.com",
        KnownApi {
            api_template: "{url}.json",
            api_type: "json_suffix",
        },
    );
    m.insert(
        "www.npmjs.com",
        KnownApi {
            api_template: "https://registry.npmjs.org/{package}",
            api_type: "rest",
        },
    );
    m.insert(
        "npmjs.com",
        KnownApi {
            api_template: "https://registry.npmjs.org/{package}",
            api_type: "rest",
        },
    );
    m.insert(
        "pypi.org",
        KnownApi {
            api_template: "https://pypi.org/pypi/{package}/json",
            api_type: "rest",
        },
    );
    m.insert(
        "crates.io",
        KnownApi {
            api_template: "https://crates.io/api/v1/crates/{crate_name}",
            api_type: "rest",
        },
    );
    m
}

/// Try to fetch data from a known public API for the given domain.
///
/// Returns `Some(records)` if the domain has a known API and data was fetched.
/// Returns `None` for unknown domains (silent skip).
pub async fn try_api(domain: &str, urls: &[String], client: &HttpClient) -> Option<Vec<ApiRecord>> {
    let apis = known_apis();
    let api = apis.get(domain)?;

    let mut records = Vec::new();

    for url in urls.iter().take(10) {
        let api_url = match api.api_type {
            "json_suffix" => format!("{url}.json"),
            "rest" => build_rest_url(api.api_template, url, domain),
            _ => continue,
        };

        if let Ok(resp) = client.get(&api_url, 5000).await {
            if resp.status == 200 {
                if let Ok(data) = serde_json::from_str::<Value>(&resp.body) {
                    records.push(ApiRecord {
                        url: url.clone(),
                        api_url,
                        data,
                    });
                }
            }
        }
    }

    if records.is_empty() {
        None
    } else {
        Some(records)
    }
}

/// Check if a domain has a known API (without making any requests).
pub fn has_known_api(domain: &str) -> bool {
    known_apis().contains_key(domain)
}

fn build_rest_url(template: &str, url: &str, domain: &str) -> String {
    let path = url
        .strip_prefix(&format!("https://{domain}"))
        .or_else(|| url.strip_prefix(&format!("http://{domain}")))
        .unwrap_or("");

    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let mut result = template.to_string();

    // Wikipedia: /wiki/Title → {title}
    if domain.contains("wikipedia.org") {
        if let Some(title) = parts.get(1) {
            result = result.replace("{title}", title);
        } else {
            return String::new();
        }
    }
    // GitHub: /owner/repo → {owner}/{repo}
    else if domain == "github.com" {
        if parts.len() >= 2 {
            result = result.replace("{owner}", parts[0]);
            result = result.replace("{repo}", parts[1]);
        } else {
            return String::new();
        }
    }
    // npm: /package/name → {package}
    // PyPI: /project/name → {package}
    else if domain.contains("npmjs.com") || domain == "pypi.org" {
        if let Some(pkg) = parts.get(1).or(parts.first()) {
            result = result.replace("{package}", pkg);
        } else {
            return String::new();
        }
    }
    // crates.io: /crates/name → {crate_name}
    else if domain == "crates.io" {
        if let Some(crate_name) = parts.get(1).or(parts.first()) {
            result = result.replace("{crate_name}", crate_name);
        } else {
            return String::new();
        }
    }
    // Reddit: append .json
    else if domain.contains("reddit.com") {
        result = result.replace("{url}", url);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_known_api() {
        assert!(has_known_api("en.wikipedia.org"));
        assert!(has_known_api("github.com"));
        assert!(has_known_api("crates.io"));
        assert!(!has_known_api("example.com"));
        assert!(!has_known_api("google.com"));
    }

    #[test]
    fn test_build_rest_url_github() {
        let url = build_rest_url(
            "https://api.github.com/repos/{owner}/{repo}",
            "https://github.com/agentralabs/agentic-vision",
            "github.com",
        );
        assert_eq!(url, "https://api.github.com/repos/agentralabs/agentic-vision");
    }

    #[test]
    fn test_build_rest_url_wikipedia() {
        let url = build_rest_url(
            "https://en.wikipedia.org/api/rest_v1/page/summary/{title}",
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "en.wikipedia.org",
        );
        assert_eq!(
            url,
            "https://en.wikipedia.org/api/rest_v1/page/summary/Rust_(programming_language)"
        );
    }

    #[test]
    fn test_build_rest_url_npm() {
        let url = build_rest_url(
            "https://registry.npmjs.org/{package}",
            "https://www.npmjs.com/package/express",
            "www.npmjs.com",
        );
        assert_eq!(url, "https://registry.npmjs.org/express");
    }

    #[test]
    fn test_build_rest_url_crates() {
        let url = build_rest_url(
            "https://crates.io/api/v1/crates/{crate_name}",
            "https://crates.io/crates/serde",
            "crates.io",
        );
        assert_eq!(url, "https://crates.io/api/v1/crates/serde");
    }
}
