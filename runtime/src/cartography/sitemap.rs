//! Parse sitemap.xml and sitemap index files.

use anyhow::Result;
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;

/// An entry from a sitemap.
#[derive(Debug, Clone)]
pub struct SitemapEntry {
    pub url: String,
    pub lastmod: Option<DateTime<Utc>>,
    pub priority: Option<f32>,
}

/// Parse a sitemap XML string into entries.
/// Handles both urlset and sitemap index (recursive).
pub fn parse_sitemap(xml: &str) -> Result<Vec<SitemapEntry>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut entries = Vec::new();
    let mut buf = Vec::new();

    let mut in_url = false;
    let mut in_sitemap = false;
    let mut current_tag = String::new();
    let mut current_loc = String::new();
    let mut current_lastmod = String::new();
    let mut current_priority = String::new();
    let mut sitemap_urls = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name =
                    String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "url" => {
                        in_url = true;
                        current_loc.clear();
                        current_lastmod.clear();
                        current_priority.clear();
                    }
                    "sitemap" => {
                        in_sitemap = true;
                        current_loc.clear();
                    }
                    _ => {
                        current_tag = name;
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name =
                    String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "url" if in_url => {
                        if !current_loc.is_empty() {
                            let lastmod = if current_lastmod.is_empty() {
                                None
                            } else {
                                parse_date(&current_lastmod)
                            };
                            let priority = if current_priority.is_empty() {
                                None
                            } else {
                                current_priority.trim().parse::<f32>().ok()
                            };
                            entries.push(SitemapEntry {
                                url: current_loc.clone(),
                                lastmod,
                                priority,
                            });
                        }
                        in_url = false;
                    }
                    "sitemap" if in_sitemap => {
                        if !current_loc.is_empty() {
                            sitemap_urls.push(current_loc.clone());
                        }
                        in_sitemap = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                if (in_url || in_sitemap) && current_tag == "loc" {
                    current_loc = text.trim().to_string();
                } else if in_url && current_tag == "lastmod" {
                    current_lastmod = text.trim().to_string();
                } else if in_url && current_tag == "priority" {
                    current_priority = text.trim().to_string();
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(anyhow::anyhow!("XML parse error: {e}"));
            }
            _ => {}
        }
        buf.clear();
    }

    // Note: sitemap index URLs would need recursive fetching (handled by mapper)
    // For now, we return the sitemap index URLs as entries with high priority
    for url in sitemap_urls {
        entries.push(SitemapEntry {
            url,
            lastmod: None,
            priority: Some(1.0),
        });
    }

    Ok(entries)
}

fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    // Try RFC 3339 first
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    // Try date-only format
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
        return Some(dt.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_urlset() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
          <url>
            <loc>https://example.com/</loc>
            <priority>1.0</priority>
          </url>
          <url>
            <loc>https://example.com/about</loc>
            <lastmod>2024-01-15</lastmod>
            <priority>0.5</priority>
          </url>
          <url>
            <loc>https://example.com/blog/post-1</loc>
            <priority>0.8</priority>
          </url>
        </urlset>"#;

        let entries = parse_sitemap(xml).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].url, "https://example.com/");
        assert_eq!(entries[0].priority, Some(1.0));
        assert_eq!(entries[1].url, "https://example.com/about");
        assert!(entries[1].lastmod.is_some());
        assert_eq!(entries[2].priority, Some(0.8));
    }

    #[test]
    fn test_parse_sitemap_index() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
          <sitemap>
            <loc>https://example.com/sitemap-products.xml</loc>
          </sitemap>
          <sitemap>
            <loc>https://example.com/sitemap-blog.xml</loc>
          </sitemap>
        </sitemapindex>"#;

        let entries = parse_sitemap(xml).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].url.contains("sitemap-products"));
        assert!(entries[1].url.contains("sitemap-blog"));
    }

    /// Fuzz test: sitemap parser must never panic on arbitrary input.
    #[test]
    fn test_fuzz_sitemap_parser() {
        let fuzz_inputs = [
            "",
            "not xml at all",
            "<",
            "<url>",
            "<url><loc>",
            "<<<>>>",
            "<urlset><url></url></urlset>",
            "<urlset><url><loc></loc></url></urlset>",
            "<urlset><url><loc>http://x</loc><priority>not-a-number</priority></url></urlset>",
            "<urlset><url><loc>http://x</loc><lastmod>not-a-date</lastmod></url></urlset>",
            &"<url>".repeat(10000),
            "\x00\x01\x02\x03",
            "<?xml version=\"1.0\"?><urlset></urlset>",
            "<sitemapindex></sitemapindex>",
            "<urlset><url><loc>http://x</loc></url><sitemap><loc>http://y</loc></sitemap></urlset>",
        ];

        for input in &fuzz_inputs {
            // Must not panic — returning Err or empty Vec is fine
            let _ = parse_sitemap(input);
        }
    }
}
