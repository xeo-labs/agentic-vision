//! Parse structured data from raw HTML without DOM rendering.
//!
//! This is the core of the no-browser acquisition engine. Extracts JSON-LD,
//! OpenGraph, meta tags, links, headings, and forms from raw HTML using
//! the `scraper` crate for CSS selector-based parsing.

use crate::map::types::PageType;
use scraper::{Html, Selector};
use serde_json::Value;

/// All structured data extracted from a single HTML page.
#[derive(Debug, Clone, Default)]
pub struct StructuredData {
    /// Raw JSON-LD blocks.
    pub jsonld: Vec<Value>,
    /// Extracted product data from JSON-LD.
    pub products: Vec<JsonLdProduct>,
    /// Extracted article data from JSON-LD.
    pub articles: Vec<JsonLdArticle>,
    /// Breadcrumb trail from JSON-LD BreadcrumbList.
    pub breadcrumbs: Vec<BreadcrumbItem>,
    /// Detected page type and confidence from structured data.
    pub page_type: Option<(PageType, f32)>,
    /// OpenGraph metadata.
    pub og: OpenGraphData,
    /// Standard meta tags.
    pub meta: MetaTags,
    /// Links extracted from `<a href>` tags.
    pub links: Vec<ExtractedLink>,
    /// Heading hierarchy.
    pub headings: Vec<(u8, String)>,
    /// Forms and their fields.
    pub forms: Vec<ExtractedForm>,
    /// Whether JSON-LD was found.
    pub has_jsonld: bool,
    /// Whether OpenGraph tags were found.
    pub has_opengraph: bool,
    /// Whether microdata (itemprop) was found.
    pub has_microdata: bool,
}

/// Product data extracted from JSON-LD.
#[derive(Debug, Clone, Default)]
pub struct JsonLdProduct {
    pub name: Option<String>,
    pub description: Option<String>,
    pub brand: Option<String>,
    pub sku: Option<String>,
    pub price: Option<f64>,
    pub original_price: Option<f64>,
    pub price_currency: Option<String>,
    pub availability: Option<String>,
    pub rating_value: Option<f64>,
    pub rating_best: Option<f64>,
    pub review_count: Option<u64>,
    pub image: Option<String>,
    pub category: Option<String>,
    pub date_modified: Option<String>,
}

/// Article data extracted from JSON-LD.
#[derive(Debug, Clone, Default)]
pub struct JsonLdArticle {
    pub headline: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub date_published: Option<String>,
    pub date_modified: Option<String>,
    pub image: Option<String>,
    pub word_count: Option<u64>,
}

/// A breadcrumb item from JSON-LD BreadcrumbList.
#[derive(Debug, Clone)]
pub struct BreadcrumbItem {
    pub name: String,
    pub url: Option<String>,
    pub position: u32,
}

/// OpenGraph metadata.
#[derive(Debug, Clone, Default)]
pub struct OpenGraphData {
    pub og_type: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    pub price_amount: Option<String>,
    pub price_currency: Option<String>,
}

/// Standard meta tags.
#[derive(Debug, Clone, Default)]
pub struct MetaTags {
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub robots: Option<String>,
    pub author: Option<String>,
    pub canonical: Option<String>,
}

/// A link extracted from HTML.
#[derive(Debug, Clone)]
pub struct ExtractedLink {
    pub href: String,
    pub text: String,
    pub is_internal: bool,
}

/// A form extracted from HTML.
#[derive(Debug, Clone)]
pub struct ExtractedForm {
    pub action: Option<String>,
    pub method: String,
    pub fields: Vec<FormField>,
}

/// A field within a form.
#[derive(Debug, Clone)]
pub struct FormField {
    pub name: Option<String>,
    pub field_type: String,
}

/// Extract all structured data from raw HTML.
///
/// This is the core of the no-browser acquisition engine.
/// Parses JSON-LD, OpenGraph, meta tags, links, headings, and forms
/// from raw HTML without any JavaScript execution.
pub fn extract_structured_data(html: &str, base_url: &str) -> StructuredData {
    let mut sd = StructuredData::default();
    let document = Html::parse_document(html);

    // 1. JSON-LD
    extract_jsonld(&document, &mut sd);

    // 2. OpenGraph
    extract_opengraph(&document, &mut sd);

    // 3. Meta tags
    extract_meta_tags(&document, &mut sd);

    // 4. Microdata (itemprop attributes — complements JSON-LD)
    extract_microdata(&document, &mut sd);

    // 5. Links
    extract_links(&document, base_url, &mut sd);

    // 6. Headings
    extract_headings(&document, &mut sd);

    // 7. Forms
    extract_forms(&document, &mut sd);

    sd
}

/// Extract links from raw HTML as a simple list of internal URLs.
///
/// Convenience function for extracting internal links from raw HTML,
/// used by server.rs fallback code and mapper.
pub fn extract_links_from_html(html: &str, base_url: &str) -> Vec<String> {
    let sd = extract_structured_data(html, base_url);
    sd.links
        .into_iter()
        .filter(|l| l.is_internal)
        .map(|l| l.href)
        .collect()
}

/// Compute a data completeness score for structured data (0.0 to 1.0).
///
/// Used by the mapper to decide if browser fallback (Layer 3) is needed.
/// Returns the fraction of key feature dimensions that can be filled.
pub fn data_completeness(sd: &StructuredData) -> f32 {
    let mut filled = 0u32;
    let total = 10u32;

    // Identity: page type from JSON-LD or microdata
    if sd.page_type.is_some() {
        filled += 2;
    }
    // Content: headings
    if !sd.headings.is_empty() {
        filled += 1;
    }
    // Content: links
    if !sd.links.is_empty() {
        filled += 1;
    }
    // Commerce: product data (from JSON-LD or microdata)
    if !sd.products.is_empty() {
        filled += 2;
    }
    // Navigation: breadcrumbs
    if !sd.breadcrumbs.is_empty() {
        filled += 1;
    }
    // Meta: description
    if sd.meta.description.is_some() || sd.og.description.is_some() {
        filled += 1;
    }
    // OpenGraph or microdata
    if sd.has_opengraph || sd.has_microdata {
        filled += 1;
    }
    // Forms
    if !sd.forms.is_empty() {
        filled += 1;
    }

    filled as f32 / total as f32
}

// ── JSON-LD extraction ──────────────────────────────────────────────────────

fn extract_jsonld(document: &Html, sd: &mut StructuredData) {
    let sel = Selector::parse(r#"script[type="application/ld+json"]"#).unwrap();
    for element in document.select(&sel) {
        let text = element.inner_html();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            sd.has_jsonld = true;
            process_jsonld_value(&value, sd);
            sd.jsonld.push(value);
        }
    }
}

fn process_jsonld_value(value: &Value, sd: &mut StructuredData) {
    // Handle @graph arrays
    if let Some(graph) = value.get("@graph").and_then(|g| g.as_array()) {
        for item in graph {
            classify_jsonld_object(item, sd);
        }
    } else {
        classify_jsonld_object(value, sd);
    }
}

fn classify_jsonld_object(value: &Value, sd: &mut StructuredData) {
    let ld_type = value.get("@type").and_then(|t| t.as_str()).unwrap_or("");

    let (page_type, confidence) = jsonld_type_to_page_type(ld_type);
    if confidence > 0.5
        && (sd.page_type.is_none()
            || sd.page_type.as_ref().map(|p| p.1).unwrap_or(0.0) < confidence)
    {
        sd.page_type = Some((page_type, confidence));
    }

    match ld_type {
        "Product" | "ProductGroup" => {
            sd.products.push(parse_product(value));
        }
        "Article" | "NewsArticle" | "BlogPosting" | "TechArticle" | "ScholarlyArticle" => {
            sd.articles.push(parse_article(value));
        }
        "BreadcrumbList" => {
            sd.breadcrumbs = parse_breadcrumbs(value);
        }
        _ => {}
    }
}

/// Map JSON-LD @type to PageType with confidence.
pub fn jsonld_type_to_page_type(ld_type: &str) -> (PageType, f32) {
    match ld_type {
        "Product" | "ProductGroup" => (PageType::ProductDetail, 0.99),
        "Article" | "NewsArticle" => (PageType::Article, 0.99),
        "BlogPosting" | "TechArticle" | "ScholarlyArticle" => (PageType::Article, 0.95),
        "FAQPage" => (PageType::Faq, 0.99),
        "SearchResultsPage" => (PageType::SearchResults, 0.99),
        "CollectionPage" | "ItemList" => (PageType::ProductListing, 0.90),
        "CheckoutPage" => (PageType::Checkout, 0.99),
        "AboutPage" => (PageType::AboutPage, 0.99),
        "ContactPage" => (PageType::ContactPage, 0.99),
        "ProfilePage" => (PageType::Account, 0.80),
        "Recipe" => (PageType::Article, 0.95),
        "Event" => (PageType::Article, 0.80),
        "QAPage" => (PageType::Forum, 0.90),
        "RealEstateListing" | "JobPosting" | "SoftwareApplication" => {
            (PageType::ProductDetail, 0.85)
        }
        "MedicalWebPage" => (PageType::Documentation, 0.85),
        "WebPage" | "WebSite" => (PageType::Unknown, 0.5),
        _ => (PageType::Unknown, 0.3),
    }
}

fn parse_product(v: &Value) -> JsonLdProduct {
    let offers = v.get("offers").and_then(|o| {
        if o.is_array() {
            o.as_array().and_then(|arr| arr.first())
        } else {
            Some(o)
        }
    });

    let (price, original_price, currency, availability) = if let Some(offer) = offers {
        (
            offer.get("price").and_then(|p| {
                p.as_f64()
                    .or_else(|| p.as_str().and_then(|s| s.parse().ok()))
            }),
            offer.get("highPrice").and_then(|p| {
                p.as_f64()
                    .or_else(|| p.as_str().and_then(|s| s.parse().ok()))
            }),
            offer
                .get("priceCurrency")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string()),
            offer
                .get("availability")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string()),
        )
    } else {
        (None, None, None, None)
    };

    let (rating_value, rating_best, review_count) = if let Some(rating) = v.get("aggregateRating") {
        (
            rating.get("ratingValue").and_then(|r| {
                r.as_f64()
                    .or_else(|| r.as_str().and_then(|s| s.parse().ok()))
            }),
            rating.get("bestRating").and_then(|r| {
                r.as_f64()
                    .or_else(|| r.as_str().and_then(|s| s.parse().ok()))
            }),
            rating.get("reviewCount").and_then(|r| {
                r.as_u64()
                    .or_else(|| r.as_str().and_then(|s| s.parse().ok()))
            }),
        )
    } else {
        (None, None, None)
    };

    JsonLdProduct {
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
        description: v
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        brand: v
            .get("brand")
            .and_then(|b| {
                b.get("name")
                    .and_then(|n| n.as_str())
                    .or_else(|| b.as_str())
            })
            .map(|s| s.to_string()),
        sku: v.get("sku").and_then(|s| s.as_str()).map(|s| s.to_string()),
        price,
        original_price,
        price_currency: currency,
        availability,
        rating_value,
        rating_best,
        review_count,
        image: v
            .get("image")
            .and_then(|i| {
                i.as_str().or_else(|| {
                    i.as_array()
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                })
            })
            .map(|s| s.to_string()),
        category: v
            .get("category")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string()),
        date_modified: v
            .get("dateModified")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
    }
}

fn parse_article(v: &Value) -> JsonLdArticle {
    JsonLdArticle {
        headline: v
            .get("headline")
            .and_then(|h| h.as_str())
            .map(|s| s.to_string()),
        description: v
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        author: v
            .get("author")
            .and_then(|a| {
                a.get("name")
                    .and_then(|n| n.as_str())
                    .or_else(|| a.as_str())
            })
            .map(|s| s.to_string()),
        date_published: v
            .get("datePublished")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        date_modified: v
            .get("dateModified")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        image: v
            .get("image")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string()),
        word_count: v.get("wordCount").and_then(|w| w.as_u64()),
    }
}

fn parse_breadcrumbs(v: &Value) -> Vec<BreadcrumbItem> {
    let mut items = Vec::new();
    if let Some(list) = v.get("itemListElement").and_then(|l| l.as_array()) {
        for item in list {
            let name = item
                .get("name")
                .or_else(|| item.get("item").and_then(|i| i.get("name")))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("item")
                .and_then(|i| {
                    i.as_str()
                        .or_else(|| i.get("@id").and_then(|id| id.as_str()))
                })
                .map(|s| s.to_string());
            let position = item.get("position").and_then(|p| p.as_u64()).unwrap_or(0) as u32;
            items.push(BreadcrumbItem {
                name,
                url,
                position,
            });
        }
    }
    items
}

// ── OpenGraph extraction ────────────────────────────────────────────────────

fn extract_opengraph(document: &Html, sd: &mut StructuredData) {
    let og_sel = Selector::parse(r#"meta[property^="og:"]"#).unwrap();
    for element in document.select(&og_sel) {
        sd.has_opengraph = true;
        let property = element.value().attr("property").unwrap_or("");
        let content = element.value().attr("content").unwrap_or("").to_string();
        match property {
            "og:type" => sd.og.og_type = Some(content),
            "og:title" => sd.og.title = Some(content),
            "og:description" => sd.og.description = Some(content),
            "og:image" => sd.og.image = Some(content),
            "og:url" => sd.og.url = Some(content),
            "og:price:amount" => sd.og.price_amount = Some(content),
            "og:price:currency" => sd.og.price_currency = Some(content),
            _ => {}
        }
    }
}

// ── Microdata extraction (itemprop) ─────────────────────────────────────────

fn extract_microdata(document: &Html, sd: &mut StructuredData) {
    // Extract product data from itemprop attributes (common on eBay, Best Buy, etc.)
    let mut found_any = false;

    // Helper: get itemprop value from content attr or inner text
    fn itemprop_text(el: &scraper::ElementRef<'_>) -> String {
        el.value()
            .attr("content")
            .map(|s| s.to_string())
            .unwrap_or_else(|| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
    }

    // Product name from itemprop="name"
    let mut product_name: Option<String> = None;
    if let Ok(sel) = Selector::parse("[itemprop=\"name\"]") {
        if let Some(el) = document.select(&sel).next() {
            let text = itemprop_text(&el);
            if !text.is_empty() {
                product_name = Some(text);
                found_any = true;
            }
        }
    }

    // Price from itemprop="price"
    let mut price: Option<f64> = None;
    let mut currency: Option<String> = None;
    if let Ok(sel) = Selector::parse("[itemprop=\"price\"]") {
        if let Some(el) = document.select(&sel).next() {
            let text = itemprop_text(&el);
            price = text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .parse::<f64>()
                .ok()
                .filter(|v| *v > 0.0);
            if price.is_some() {
                found_any = true;
            }
        }
    }
    if let Ok(sel) = Selector::parse("[itemprop=\"priceCurrency\"]") {
        if let Some(el) = document.select(&sel).next() {
            currency = el.value().attr("content").map(|s| s.to_string());
        }
    }

    // Rating from itemprop="ratingValue"
    let mut rating_value: Option<f64> = None;
    let mut review_count: Option<u64> = None;
    if let Ok(sel) = Selector::parse("[itemprop=\"ratingValue\"]") {
        if let Some(el) = document.select(&sel).next() {
            let text = itemprop_text(&el);
            rating_value = text.parse::<f64>().ok().filter(|v| v.is_finite());
            if rating_value.is_some() {
                found_any = true;
            }
        }
    }
    if let Ok(sel) = Selector::parse("[itemprop=\"reviewCount\"]") {
        if let Some(el) = document.select(&sel).next() {
            let text = itemprop_text(&el);
            review_count = text
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .ok();
        }
    }

    // Availability from itemprop="availability"
    let mut availability: Option<String> = None;
    if let Ok(sel) = Selector::parse("[itemprop=\"availability\"]") {
        if let Some(el) = document.select(&sel).next() {
            availability = el
                .value()
                .attr("href")
                .or_else(|| el.value().attr("content"))
                .map(|s| s.to_string());
            if availability.is_some() {
                found_any = true;
            }
        }
    }

    // Build a product if we found price or name from microdata (and no JSON-LD product)
    if found_any && sd.products.is_empty() && (product_name.is_some() || price.is_some()) {
        sd.products.push(JsonLdProduct {
            name: product_name,
            price,
            price_currency: currency,
            rating_value,
            review_count,
            availability,
            ..Default::default()
        });
        // Set page type if not already set from JSON-LD
        if sd.page_type.is_none() || sd.page_type.as_ref().map(|p| p.1).unwrap_or(0.0) < 0.85 {
            sd.page_type = Some((PageType::ProductDetail, 0.85));
        }
    }

    // Description from itemprop="description"
    if sd.meta.description.is_none() {
        if let Ok(sel) = Selector::parse("[itemprop=\"description\"]") {
            if let Some(el) = document.select(&sel).next() {
                let text = itemprop_text(&el);
                if !text.is_empty() && text.len() < 500 {
                    sd.meta.description = Some(text);
                    found_any = true;
                }
            }
        }
    }

    sd.has_microdata = found_any;
}

// ── Meta tag extraction ─────────────────────────────────────────────────────

fn extract_meta_tags(document: &Html, sd: &mut StructuredData) {
    if let Ok(sel) = Selector::parse(r#"meta[name="description"]"#) {
        if let Some(el) = document.select(&sel).next() {
            sd.meta.description = el.value().attr("content").map(|s| s.to_string());
        }
    }
    if let Ok(sel) = Selector::parse(r#"meta[name="keywords"]"#) {
        if let Some(el) = document.select(&sel).next() {
            sd.meta.keywords = el.value().attr("content").map(|s| s.to_string());
        }
    }
    if let Ok(sel) = Selector::parse(r#"meta[name="robots"]"#) {
        if let Some(el) = document.select(&sel).next() {
            sd.meta.robots = el.value().attr("content").map(|s| s.to_string());
        }
    }
    if let Ok(sel) = Selector::parse(r#"meta[name="author"]"#) {
        if let Some(el) = document.select(&sel).next() {
            sd.meta.author = el.value().attr("content").map(|s| s.to_string());
        }
    }
    if let Ok(sel) = Selector::parse(r#"link[rel="canonical"]"#) {
        if let Some(el) = document.select(&sel).next() {
            sd.meta.canonical = el.value().attr("href").map(|s| s.to_string());
        }
    }
}

// ── Link extraction ─────────────────────────────────────────────────────────

fn extract_links(document: &Html, base_url: &str, sd: &mut StructuredData) {
    let sel = Selector::parse("a[href]").unwrap();
    let base = url::Url::parse(base_url).ok();
    let base_host = base
        .as_ref()
        .and_then(|u| u.host_str().map(|h| h.to_string()));

    for element in document.select(&sel) {
        let href = element.value().attr("href").unwrap_or("");
        if href.is_empty() || href.starts_with('#') || href.starts_with("javascript:") {
            continue;
        }

        let resolved = if let Some(ref base) = base {
            base.join(href)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| href.to_string())
        } else {
            href.to_string()
        };

        let text = element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        let is_internal =
            if let (Some(ref bh), Ok(link_url)) = (&base_host, url::Url::parse(&resolved)) {
                link_url
                    .host_str()
                    .map(|h| {
                        h == bh.as_str()
                            || h.strip_prefix("www.")
                                .unwrap_or(h)
                                .eq(bh.strip_prefix("www.").unwrap_or(bh))
                    })
                    .unwrap_or(false)
            } else {
                href.starts_with('/')
            };

        sd.links.push(ExtractedLink {
            href: resolved,
            text,
            is_internal,
        });
    }
}

// ── Heading extraction ──────────────────────────────────────────────────────

fn extract_headings(document: &Html, sd: &mut StructuredData) {
    for level in 1..=6u8 {
        let tag = format!("h{level}");
        let sel = match Selector::parse(&tag) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for element in document.select(&sel) {
            let text = element
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            if !text.is_empty() {
                sd.headings.push((level, text));
            }
        }
    }
}

// ── Form extraction ─────────────────────────────────────────────────────────

fn extract_forms(document: &Html, sd: &mut StructuredData) {
    let form_sel = Selector::parse("form").unwrap();
    let input_sel = Selector::parse("input, select, textarea").unwrap();

    for form in document.select(&form_sel) {
        let action = form.value().attr("action").map(|s| s.to_string());
        let method = form.value().attr("method").unwrap_or("get").to_uppercase();

        let mut fields = Vec::new();
        for field in form.select(&input_sel) {
            let name = field.value().attr("name").map(|s| s.to_string());
            let field_type = field
                .value()
                .attr("type")
                .unwrap_or(field.value().name())
                .to_string();
            fields.push(FormField { name, field_type });
        }

        sd.forms.push(ExtractedForm {
            action,
            method,
            fields,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_jsonld_product() {
        let html = r#"
        <html><head>
        <script type="application/ld+json">
        {
          "@type": "Product",
          "name": "Test Widget",
          "offers": {
            "@type": "Offer",
            "price": 29.99,
            "priceCurrency": "USD",
            "availability": "https://schema.org/InStock"
          },
          "aggregateRating": {
            "@type": "AggregateRating",
            "ratingValue": 4.5,
            "bestRating": 5,
            "reviewCount": 128
          }
        }
        </script>
        </head><body></body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com/product/1");
        assert!(sd.has_jsonld);
        assert_eq!(sd.products.len(), 1);
        let p = &sd.products[0];
        assert_eq!(p.name.as_deref(), Some("Test Widget"));
        assert_eq!(p.price, Some(29.99));
        assert_eq!(p.price_currency.as_deref(), Some("USD"));
        assert_eq!(p.rating_value, Some(4.5));
        assert_eq!(p.review_count, Some(128));
        assert!(matches!(sd.page_type, Some((PageType::ProductDetail, c)) if c > 0.9));
    }

    #[test]
    fn test_extract_jsonld_article() {
        let html = r#"
        <html><head>
        <script type="application/ld+json">
        {
          "@type": "NewsArticle",
          "headline": "Breaking: Test Passes",
          "author": {"@type": "Person", "name": "Jane Doe"},
          "datePublished": "2026-01-15"
        }
        </script>
        </head><body></body></html>
        "#;

        let sd = extract_structured_data(html, "https://news.example.com/article/1");
        assert_eq!(sd.articles.len(), 1);
        assert_eq!(
            sd.articles[0].headline.as_deref(),
            Some("Breaking: Test Passes")
        );
        assert_eq!(sd.articles[0].author.as_deref(), Some("Jane Doe"));
        assert!(matches!(sd.page_type, Some((PageType::Article, c)) if c > 0.9));
    }

    #[test]
    fn test_extract_graph_array() {
        let html = r#"
        <html><head>
        <script type="application/ld+json">
        {
          "@context": "https://schema.org",
          "@graph": [
            {"@type": "WebSite", "name": "Example"},
            {"@type": "Product", "name": "Widget", "offers": {"price": 10}}
          ]
        }
        </script>
        </head><body></body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        assert_eq!(sd.products.len(), 1);
        assert_eq!(sd.products[0].price, Some(10.0));
    }

    #[test]
    fn test_extract_opengraph() {
        let html = r#"
        <html><head>
        <meta property="og:type" content="product" />
        <meta property="og:title" content="My Product" />
        <meta property="og:price:amount" content="49.99" />
        <meta property="og:price:currency" content="EUR" />
        </head><body></body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        assert!(sd.has_opengraph);
        assert_eq!(sd.og.og_type.as_deref(), Some("product"));
        assert_eq!(sd.og.title.as_deref(), Some("My Product"));
        assert_eq!(sd.og.price_amount.as_deref(), Some("49.99"));
    }

    #[test]
    fn test_extract_meta_tags() {
        let html = r#"
        <html><head>
        <meta name="description" content="A great page" />
        <meta name="robots" content="index, follow" />
        <link rel="canonical" href="https://example.com/canonical" />
        </head><body></body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        assert_eq!(sd.meta.description.as_deref(), Some("A great page"));
        assert_eq!(sd.meta.robots.as_deref(), Some("index, follow"));
        assert_eq!(
            sd.meta.canonical.as_deref(),
            Some("https://example.com/canonical")
        );
    }

    #[test]
    fn test_extract_links() {
        let html = r##"
        <html><body>
        <a href="/about">About</a>
        <a href="https://example.com/products">Products</a>
        <a href="https://external.com/foo">External</a>
        <a href="#section">Anchor</a>
        </body></html>
        "##;

        let sd = extract_structured_data(html, "https://example.com/");
        let internal: Vec<_> = sd.links.iter().filter(|l| l.is_internal).collect();
        let external: Vec<_> = sd.links.iter().filter(|l| !l.is_internal).collect();
        assert_eq!(internal.len(), 2);
        assert_eq!(external.len(), 1);
    }

    #[test]
    fn test_extract_headings() {
        let html = r#"
        <html><body>
        <h1>Main Title</h1>
        <h2>Section A</h2>
        <h2>Section B</h2>
        <h3>Subsection</h3>
        </body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        assert_eq!(sd.headings.len(), 4);
        assert_eq!(sd.headings[0], (1, "Main Title".to_string()));
    }

    #[test]
    fn test_extract_forms() {
        let html = r#"
        <html><body>
        <form action="/search" method="GET">
            <input type="text" name="q" />
            <input type="submit" value="Search" />
        </form>
        </body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        assert_eq!(sd.forms.len(), 1);
        assert_eq!(sd.forms[0].method, "GET");
        assert_eq!(sd.forms[0].fields.len(), 2);
    }

    #[test]
    fn test_extract_links_from_html_convenience() {
        let html = r#"
        <html><body>
        <a href="/about">About</a>
        <a href="https://example.com/products">Products</a>
        <a href="https://external.com/foo">External</a>
        </body></html>
        "#;

        let links = extract_links_from_html(html, "https://example.com/");
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|l| l.contains("/about")));
        assert!(links.iter().any(|l| l.contains("/products")));
    }

    #[test]
    fn test_data_completeness() {
        let html = r#"
        <html><head>
        <script type="application/ld+json">
        {"@type": "Product", "name": "Widget", "offers": {"price": 10}}
        </script>
        <meta property="og:title" content="Widget" />
        <meta name="description" content="A widget" />
        </head><body>
        <h1>Widget</h1>
        <a href="/other">Other</a>
        </body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        let completeness = data_completeness(&sd);
        assert!(completeness > 0.5);
    }

    #[test]
    fn test_extract_microdata_product() {
        let html = r#"
        <html><body>
        <div itemscope itemtype="https://schema.org/Product">
            <h1 itemprop="name">Wireless Mouse</h1>
            <meta itemprop="price" content="29.99" />
            <meta itemprop="priceCurrency" content="USD" />
            <meta itemprop="ratingValue" content="4.3" />
            <link itemprop="availability" href="https://schema.org/InStock" />
        </div>
        </body></html>
        "#;

        let sd = extract_structured_data(html, "https://shop.example.com/mouse");
        assert!(sd.has_microdata);
        assert_eq!(sd.products.len(), 1);
        let p = &sd.products[0];
        assert_eq!(p.name.as_deref(), Some("Wireless Mouse"));
        assert_eq!(p.price, Some(29.99));
        assert_eq!(p.price_currency.as_deref(), Some("USD"));
        assert_eq!(p.rating_value, Some(4.3));
        assert!(matches!(sd.page_type, Some((PageType::ProductDetail, c)) if c >= 0.85));
    }

    #[test]
    fn test_no_panic_on_malformed_jsonld() {
        let html = r#"
        <html><head>
        <script type="application/ld+json">
        {not valid json}
        </script>
        <script type="application/ld+json">
        {"@type": "Product"}
        </script>
        </head><body></body></html>
        "#;

        let sd = extract_structured_data(html, "https://example.com");
        assert_eq!(sd.products.len(), 1);
    }

    #[test]
    fn test_jsonld_type_to_page_type() {
        assert!(matches!(
            jsonld_type_to_page_type("Product"),
            (PageType::ProductDetail, c) if c > 0.9
        ));
        assert!(matches!(
            jsonld_type_to_page_type("NewsArticle"),
            (PageType::Article, c) if c > 0.9
        ));
        assert!(matches!(
            jsonld_type_to_page_type("FAQPage"),
            (PageType::Faq, c) if c > 0.9
        ));
        assert!(matches!(
            jsonld_type_to_page_type("UnknownType"),
            (PageType::Unknown, _)
        ));
    }
}
