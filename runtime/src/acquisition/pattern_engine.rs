//! CSS-selector and regex based data extractor for raw HTML.
//!
//! This module is the fallback extraction layer that fires when structured data
//! (JSON-LD, OpenGraph, microdata) is absent or incomplete. It walks the DOM
//! using CSS selectors and applies regex patterns against visible text to pull
//! out commerce attributes (price, rating, availability), classify the page
//! type, and discover interactive actions (forms, buttons, CTAs).
//!
//! Selector patterns are loaded at compile time from `css_selectors.json` via
//! `include_str!`. All public entry points are **synchronous** because the
//! `scraper` crate's types are `!Send` -- callers should wrap in
//! `tokio::task::spawn_blocking` when integrating with the async runtime.
//!
//! # Confidence model
//!
//! Every extracted value is paired with a confidence score in `[0.0, 1.0]`.
//! Data attributes and itemprop selectors score highest (0.95) because they
//! carry explicit semantic intent. Generic CSS class selectors score lower
//! (0.85), and regex matches on free text are the least confident (0.70).
//! The caller can decide a threshold below which data is discarded.

use crate::map::types::PageType;
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;

// ── Compile-time selector configuration ──────────────────────────────────────

/// Raw JSON content of the selector configuration file, embedded at compile
/// time so there is no runtime file I/O.
const SELECTORS_JSON: &str = include_str!("css_selectors.json");

// ── Public types ─────────────────────────────────────────────────────────────

/// Action discovered from HTML patterns (forms, buttons, links).
///
/// Maps interactive elements to Cortex OpCode pairs so the navigation engine
/// can reason about available actions without rendering the page.
#[derive(Debug, Clone)]
pub struct DiscoveredAction {
    /// Human-readable label (e.g., "Add to Cart", "Search").
    pub label: String,
    /// OpCode `(category, action)` bytes matching the binary spec in 02-map-spec.md.
    pub opcode: (u8, u8),
    /// Confidence that this is the correct action, in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// Form discovered from HTML.
///
/// Contains enough metadata for the navigation engine to know what data a
/// form expects and where it submits, without needing to render the page.
#[derive(Debug, Clone)]
pub struct DiscoveredForm {
    /// Form action URL (may be relative).
    pub action_url: String,
    /// HTTP method (`GET` or `POST`).
    pub method: String,
    /// Field `(name, type)` pairs for every `<input>`, `<select>`, and
    /// `<textarea>` inside the form.
    pub fields: Vec<(String, String)>,
}

/// Result of pattern-based extraction.
///
/// Every `Option` field is a `(value, confidence)` tuple. Fields are `None`
/// when no matching pattern was found. The caller merges these results with
/// structured-data results, preferring whichever source has higher confidence.
#[derive(Debug, Clone, Default)]
pub struct PatternResult {
    /// Extracted price as `(dollar_value, confidence)`.
    pub price: Option<(f32, f32)>,
    /// Original / strike-through price as `(dollar_value, confidence)`.
    pub original_price: Option<(f32, f32)>,
    /// ISO 4217 currency code or symbol.
    pub currency: Option<String>,
    /// Rating normalized to `[0.0, 1.0]` as `(normalized_value, confidence)`.
    pub rating: Option<(f32, f32)>,
    /// Maximum rating scale detected (e.g., 5.0, 10.0).
    pub rating_max: Option<f32>,
    /// Review count as `(count, confidence)`.
    pub review_count: Option<(u32, f32)>,
    /// Availability signal: `0.0` = out of stock, `0.5` = limited,
    /// `1.0` = in stock, paired with confidence.
    pub availability: Option<(f32, f32)>,
    /// Detected page type and confidence.
    pub page_type: Option<(PageType, f32)>,
    /// Interactive actions found on the page.
    pub actions: Vec<DiscoveredAction>,
    /// Forms found on the page.
    pub forms: Vec<DiscoveredForm>,
}

// ── Main entry point ─────────────────────────────────────────────────────────

/// Extract data from raw HTML using CSS selectors and regex patterns.
///
/// This is the main entry point for the pattern engine. It parses the HTML
/// document once and then runs each extraction pass (price, rating,
/// availability, page type, actions) in sequence, stopping each pass at the
/// first confident match.
///
/// # Arguments
///
/// * `html` - Raw HTML string to parse.
/// * `base_url` - Base URL for resolving relative form action URLs.
///
/// # Returns
///
/// A `PatternResult` with all discovered data. Fields that could not be
/// extracted are `None`.
pub fn extract_from_patterns(html: &str, base_url: &str) -> PatternResult {
    let document = Html::parse_document(html);
    let config: Value = serde_json::from_str(SELECTORS_JSON).unwrap_or_default();

    let mut result = PatternResult::default();

    extract_price(&document, &config, &mut result);
    extract_rating(&document, &config, &mut result);
    extract_availability(&document, &config, &mut result);
    extract_page_type(&document, &config, &mut result);
    extract_actions(&document, base_url, &config, &mut result);

    result
}

// ── Price extraction ─────────────────────────────────────────────────────────

/// Try to extract a price from the document using progressively less
/// confident strategies: data attributes > itemprop > CSS selectors > regex.
fn extract_price(document: &Html, config: &Value, result: &mut PatternResult) {
    let price_config = &config["price"];

    // Strategy 1: data attributes (confidence 0.95)
    if let Some(attr_names) = price_config["data_attr_names"].as_array() {
        for attr_name_val in attr_names {
            if let Some(attr_name) = attr_name_val.as_str() {
                if let Some(selector_str) = price_config["data_attributes"]
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|v| v.as_str().map(|s| s.contains(attr_name)).unwrap_or(false))
                    })
                    .and_then(|v| v.as_str())
                {
                    if let Ok(sel) = Selector::parse(selector_str) {
                        for el in document.select(&sel) {
                            if let Some(val) = el.value().attr(attr_name) {
                                if let Some(price) = parse_price_text(val) {
                                    result.price = Some((price, 0.95));
                                    detect_currency_from_text(val, result);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy 2: itemprop="price" (confidence 0.95)
    if let Some(itemprop_sel) = price_config["itemprop"].as_str() {
        if let Ok(sel) = Selector::parse(itemprop_sel) {
            for el in document.select(&sel) {
                // Try content attribute first (common for meta-like itemprop)
                if let Some(content) = el.value().attr("content") {
                    if let Some(price) = parse_price_text(content) {
                        result.price = Some((price, 0.95));
                        detect_currency_from_text(content, result);
                        return;
                    }
                }
                // Fall back to inner text
                let text = element_text(&el);
                if let Some(price) = parse_price_text(&text) {
                    result.price = Some((price, 0.95));
                    detect_currency_from_text(&text, result);
                    return;
                }
            }
        }
    }

    // Strategy 3: CSS class selectors (confidence 0.85)
    if let Some(selectors) = price_config["selectors"].as_array() {
        for sel_val in selectors {
            if let Some(sel_str) = sel_val.as_str() {
                if let Ok(sel) = Selector::parse(sel_str) {
                    for el in document.select(&sel) {
                        let text = element_text(&el);
                        if let Some(price) = parse_price_text(&text) {
                            result.price = Some((price, 0.85));
                            detect_currency_from_text(&text, result);
                            return;
                        }
                    }
                }
            }
        }
    }

    // Strategy 4: regex on full page text (confidence 0.70)
    let body_text = extract_body_text(document);
    let price_re =
        Regex::new(r"[$\u{20AC}\u{00A3}\u{00A5}]\s*[\d,]+\.?\d*").expect("price regex is valid");
    if let Some(mat) = price_re.find(&body_text) {
        let matched = mat.as_str();
        if let Some(price) = parse_price_text(matched) {
            result.price = Some((price, 0.70));
            detect_currency_from_text(matched, result);
        }
    }
}

// ── Rating extraction ────────────────────────────────────────────────────────

/// Try to extract a rating from the document. Normalizes to `[0.0, 1.0]` by
/// dividing by the detected maximum (default 5.0).
fn extract_rating(document: &Html, config: &Value, result: &mut PatternResult) {
    let rating_config = &config["rating"];

    // Strategy 1: data attributes (confidence 0.95)
    if let Some(attr_names) = rating_config["data_attr_names"].as_array() {
        for attr_name_val in attr_names {
            if let Some(attr_name) = attr_name_val.as_str() {
                if let Some(selector_str) = rating_config["data_attributes"]
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|v| v.as_str().map(|s| s.contains(attr_name)).unwrap_or(false))
                    })
                    .and_then(|v| v.as_str())
                {
                    if let Ok(sel) = Selector::parse(selector_str) {
                        for el in document.select(&sel) {
                            if let Some(val) = el.value().attr(attr_name) {
                                if let Some(raw) = parse_number(val) {
                                    let max = detect_rating_max(raw);
                                    result.rating_max = Some(max);
                                    result.rating = Some((raw / max, 0.95));
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy 2: itemprop="ratingValue" (confidence 0.95)
    if let Some(itemprop_sel) = rating_config["itemprop"].as_str() {
        if let Ok(sel) = Selector::parse(itemprop_sel) {
            for el in document.select(&sel) {
                if let Some(content) = el.value().attr("content") {
                    if let Some(raw) = parse_number(content) {
                        let max = detect_rating_max(raw);
                        result.rating_max = Some(max);
                        result.rating = Some((raw / max, 0.95));
                        return;
                    }
                }
                let text = element_text(&el);
                if let Some(raw) = parse_number(&text) {
                    let max = detect_rating_max(raw);
                    result.rating_max = Some(max);
                    result.rating = Some((raw / max, 0.95));
                    return;
                }
            }
        }
    }

    // Strategy 3: aria-label containing rating information (confidence 0.90)
    if let Ok(sel) = Selector::parse("[aria-label]") {
        let out_of_re =
            Regex::new(r"([\d.]+)\s*(?:out of|/)\s*([\d.]+)").expect("out-of regex is valid");
        let star_re = Regex::new(r"([\d.]+)\s*(?:star|rating)").expect("star regex is valid");

        for el in document.select(&sel) {
            if let Some(label) = el.value().attr("aria-label") {
                let label_lower = label.to_lowercase();
                if label_lower.contains("star")
                    || label_lower.contains("rating")
                    || label_lower.contains("out of")
                {
                    // Try "X out of Y" pattern first
                    if let Some(caps) = out_of_re.captures(label) {
                        if let (Some(val), Some(max)) = (
                            caps.get(1).and_then(|m| m.as_str().parse::<f32>().ok()),
                            caps.get(2).and_then(|m| m.as_str().parse::<f32>().ok()),
                        ) {
                            if max > 0.0 {
                                result.rating_max = Some(max);
                                result.rating = Some((val / max, 0.90));
                                return;
                            }
                        }
                    }
                    // Try "X star" pattern
                    if let Some(caps) = star_re.captures(label) {
                        if let Some(val) = caps.get(1).and_then(|m| m.as_str().parse::<f32>().ok())
                        {
                            let max = detect_rating_max(val);
                            result.rating_max = Some(max);
                            result.rating = Some((val / max, 0.90));
                            return;
                        }
                    }
                }
            }
        }
    }

    // Strategy 4: CSS class selectors (confidence 0.85)
    if let Some(selectors) = rating_config["selectors"].as_array() {
        for sel_val in selectors {
            if let Some(sel_str) = sel_val.as_str() {
                if let Ok(sel) = Selector::parse(sel_str) {
                    for el in document.select(&sel) {
                        let text = element_text(&el);
                        if let Some(raw) = parse_number(&text) {
                            let max = detect_rating_max(raw);
                            result.rating_max = Some(max);
                            result.rating = Some((raw / max, 0.85));
                            return;
                        }
                    }
                }
            }
        }
    }
}

// ── Availability extraction ──────────────────────────────────────────────────

/// Try to extract availability status. Maps to a float:
/// `0.0` = out of stock, `0.5` = limited / pre-order, `1.0` = in stock.
fn extract_availability(document: &Html, config: &Value, result: &mut PatternResult) {
    let avail_config = &config["availability"];

    // Strategy 1: itemprop="availability" (confidence 0.95)
    if let Some(itemprop_sel) = avail_config["itemprop"].as_str() {
        if let Ok(sel) = Selector::parse(itemprop_sel) {
            for el in document.select(&sel) {
                // Check href or content attribute (schema.org convention)
                let value = el
                    .value()
                    .attr("href")
                    .or_else(|| el.value().attr("content"))
                    .unwrap_or("");
                let text = if value.is_empty() {
                    element_text(&el)
                } else {
                    value.to_string()
                };
                if let Some(avail) = classify_availability_text(&text) {
                    result.availability = Some((avail, 0.95));
                    return;
                }
            }
        }
    }

    // Strategy 2: class names containing stock-related keywords (confidence 0.90)
    if let Ok(sel) = Selector::parse("[class]") {
        let in_stock_classes = avail_config["in_stock_classes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let out_of_stock_classes = avail_config["out_of_stock_classes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for el in document.select(&sel) {
            if let Some(class_attr) = el.value().attr("class") {
                let class_lower = class_attr.to_lowercase();
                for cls in &out_of_stock_classes {
                    if class_lower.contains(cls.as_str()) {
                        result.availability = Some((0.0, 0.90));
                        return;
                    }
                }
                for cls in &in_stock_classes {
                    if class_lower.contains(cls.as_str()) {
                        result.availability = Some((1.0, 0.90));
                        return;
                    }
                }
            }
        }
    }

    // Strategy 3: text content matching (confidence 0.80)
    let body_text = extract_body_text(document).to_lowercase();
    let stock_phrases: &[(&str, f32)] = &[
        ("out of stock", 0.0),
        ("sold out", 0.0),
        ("currently unavailable", 0.0),
        ("unavailable", 0.0),
        ("pre-order", 0.5),
        ("preorder", 0.5),
        ("pre order", 0.5),
        ("limited stock", 0.5),
        ("limited availability", 0.5),
        ("in stock", 1.0),
        ("available", 1.0),
    ];
    for &(phrase, value) in stock_phrases {
        if body_text.contains(phrase) {
            result.availability = Some((value, 0.80));
            return;
        }
    }
}

// ── Page type classification ─────────────────────────────────────────────────

/// Classify the page type from body/main element class names and IDs.
fn extract_page_type(document: &Html, config: &Value, result: &mut PatternResult) {
    let keywords = &config["page_type_keywords"];

    // Strategy 1: <body> class attribute (confidence 0.90)
    if let Ok(sel) = Selector::parse("body") {
        if let Some(body) = document.select(&sel).next() {
            if let Some(class_attr) = body.value().attr("class") {
                if let Some((pt, _kw)) = match_page_type_keywords(class_attr, keywords) {
                    result.page_type = Some((pt, 0.90));
                    return;
                }
            }
        }
    }

    // Strategy 2: main content area class names (confidence 0.85)
    let main_selectors = ["main", "article", "[role=\"main\"]"];
    for main_sel_str in &main_selectors {
        if let Ok(sel) = Selector::parse(main_sel_str) {
            for el in document.select(&sel) {
                if let Some(class_attr) = el.value().attr("class") {
                    if let Some((pt, _kw)) = match_page_type_keywords(class_attr, keywords) {
                        result.page_type = Some((pt, 0.85));
                        return;
                    }
                }
                if let Some(id_attr) = el.value().attr("id") {
                    if let Some((pt, _kw)) = match_page_type_keywords(id_attr, keywords) {
                        result.page_type = Some((pt, 0.85));
                        return;
                    }
                }
            }
        }
    }

    // Strategy 3: <body> id attribute (confidence 0.80)
    if let Ok(sel) = Selector::parse("body") {
        if let Some(body) = document.select(&sel).next() {
            if let Some(id_attr) = body.value().attr("id") {
                if let Some((pt, _kw)) = match_page_type_keywords(id_attr, keywords) {
                    result.page_type = Some((pt, 0.80));
                }
            }
        }
    }
}

// ── Action detection ─────────────────────────────────────────────────────────

/// Discover interactive actions: buttons, submit inputs, forms, and CTA links.
fn extract_actions(document: &Html, base_url: &str, config: &Value, result: &mut PatternResult) {
    let action_keywords = &config["action_keywords"];

    // 1. Buttons and submit inputs — count ALL buttons as actions,
    //    classify with keyword matching where possible
    let button_selectors = ["button", "input[type=\"submit\"]", "input[type=\"button\"]"];
    for btn_sel_str in &button_selectors {
        if let Ok(sel) = Selector::parse(btn_sel_str) {
            for el in document.select(&sel) {
                let label = el
                    .value()
                    .attr("value")
                    .map(String::from)
                    .unwrap_or_else(|| element_text(&el));
                if label.is_empty() {
                    continue;
                }
                let opcode = classify_action_label(&label, action_keywords).unwrap_or((0x00, 0x00)); // default: navigation click
                let confidence = if opcode != (0x00, 0x00) { 0.90 } else { 0.70 };
                result.actions.push(DiscoveredAction {
                    label,
                    opcode,
                    confidence,
                });
            }
        }
    }

    // 2. Forms
    if let Ok(form_sel) = Selector::parse("form") {
        let field_sel =
            Selector::parse("input, select, textarea").expect("field selector is valid");

        for form in document.select(&form_sel) {
            let action_raw = form.value().attr("action").unwrap_or("");
            let action_url = resolve_url(base_url, action_raw);
            let method = form.value().attr("method").unwrap_or("GET").to_uppercase();

            let mut fields = Vec::new();
            for field in form.select(&field_sel) {
                let name = field.value().attr("name").unwrap_or("").to_string();
                let field_type = field
                    .value()
                    .attr("type")
                    .unwrap_or(field.value().name())
                    .to_string();
                if !name.is_empty() {
                    fields.push((name, field_type));
                }
            }

            result.forms.push(DiscoveredForm {
                action_url,
                method,
                fields,
            });
        }
    }

    // 3. Links styled as buttons (CTA links) — expanded matching
    if let Ok(sel) = Selector::parse("a[class], a[role=\"button\"]") {
        for el in document.select(&sel) {
            let class_lower = el
                .value()
                .attr("class")
                .map(|c| c.to_lowercase())
                .unwrap_or_default();
            let role = el.value().attr("role").unwrap_or("");
            if class_lower.contains("btn")
                || class_lower.contains("button")
                || class_lower.contains("cta")
                || class_lower.contains("action")
                || class_lower.contains("nav-link")
                || class_lower.contains("primary")
                || role == "button"
            {
                let label = element_text(&el);
                if label.is_empty() {
                    continue;
                }
                let opcode = classify_action_label(&label, action_keywords).unwrap_or((0x00, 0x00));
                result.actions.push(DiscoveredAction {
                    label,
                    opcode,
                    confidence: 0.80,
                });
            }
        }
    }

    // 4. Links with action-like hrefs (login, signup, search, contact, etc.)
    if result.actions.is_empty() && result.forms.is_empty() {
        if let Ok(sel) = Selector::parse("a[href]") {
            for el in document.select(&sel) {
                if let Some(href) = el.value().attr("href") {
                    let href_lower = href.to_lowercase();
                    let opcode = if href_lower.contains("login")
                        || href_lower.contains("signin")
                        || href_lower.contains("sign-in")
                    {
                        Some((0x04, 0x01)) // auth: login
                    } else if href_lower.contains("signup")
                        || href_lower.contains("register")
                        || href_lower.contains("sign-up")
                        || href_lower.contains("join")
                    {
                        Some((0x04, 0x00)) // auth: register
                    } else if href_lower.contains("search") {
                        Some((0x01, 0x00)) // navigation: search
                    } else if href_lower.contains("subscribe") {
                        Some((0x03, 0x00)) // content: subscribe
                    } else if href_lower.contains("contact") {
                        Some((0x00, 0x00)) // navigation: contact
                    } else if href_lower.contains("cart") || href_lower.contains("basket") {
                        Some((0x02, 0x01)) // commerce: cart
                    } else {
                        None
                    };
                    if let Some(op) = opcode {
                        let label = element_text(&el);
                        if !label.is_empty() {
                            result.actions.push(DiscoveredAction {
                                label,
                                opcode: op,
                                confidence: 0.70,
                            });
                        }
                    }
                }
            }
        }
    }

    // 5. Search input fields (even outside forms)
    if let Ok(sel) = Selector::parse(
        "input[type=\"search\"], input[name=\"q\"], input[name=\"query\"], input[name=\"search\"]",
    ) {
        if document.select(&sel).next().is_some() {
            result.actions.push(DiscoveredAction {
                label: "Search".to_string(),
                opcode: (0x01, 0x00), // navigation: search
                confidence: 0.85,
            });
        }
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// Collect all visible text content from an element, trimmed and whitespace-
/// collapsed.
fn element_text(el: &scraper::ElementRef<'_>) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract all text content from the `<body>` element.
fn extract_body_text(document: &Html) -> String {
    if let Ok(sel) = Selector::parse("body") {
        if let Some(body) = document.select(&sel).next() {
            return element_text(&body);
        }
    }
    String::new()
}

/// Parse a price string, stripping currency symbols, commas, and whitespace.
/// Returns `None` if the string does not contain a valid number.
fn parse_price_text(text: &str) -> Option<f32> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    // Handle European format (1.234,56) vs US format (1,234.56)
    // If the last separator is a comma and there are exactly 2 digits after it,
    // treat it as a decimal separator.
    let normalized = if cleaned.contains(',') && cleaned.contains('.') {
        // Both present: last one is the decimal separator
        if cleaned.rfind(',') > cleaned.rfind('.') {
            // European: 1.234,56
            cleaned.replace('.', "").replace(',', ".")
        } else {
            // US: 1,234.56
            cleaned.replace(',', "")
        }
    } else if cleaned.contains(',') {
        let after_comma = cleaned.split(',').next_back().unwrap_or("");
        if after_comma.len() <= 2 {
            // Likely decimal: 29,99
            cleaned.replace(',', ".")
        } else {
            // Likely thousands: 1,234
            cleaned.replace(',', "")
        }
    } else {
        cleaned
    };

    normalized.parse::<f32>().ok().filter(|&v| v > 0.0)
}

/// Parse a floating point number from text, stripping non-numeric characters
/// except `.`.
fn parse_number(text: &str) -> Option<f32> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Detect currency from text containing a price.
fn detect_currency_from_text(text: &str, result: &mut PatternResult) {
    if result.currency.is_some() {
        return;
    }
    for ch in text.chars() {
        match ch {
            '$' => {
                result.currency = Some("USD".to_string());
                return;
            }
            '\u{20AC}' => {
                // Euro sign
                result.currency = Some("EUR".to_string());
                return;
            }
            '\u{00A3}' => {
                // Pound sign
                result.currency = Some("GBP".to_string());
                return;
            }
            '\u{00A5}' => {
                // Yen/Yuan sign
                result.currency = Some("JPY".to_string());
                return;
            }
            _ => {}
        }
    }
}

/// Determine the rating scale maximum from a raw value.
/// If the value is > 5 and <= 10, assume 10-point scale.
/// Otherwise, assume 5-point scale.
fn detect_rating_max(raw: f32) -> f32 {
    if raw > 5.0 && raw <= 10.0 {
        10.0
    } else if raw > 10.0 && raw <= 100.0 {
        100.0
    } else {
        5.0
    }
}

/// Classify an availability-related string into a float value.
fn classify_availability_text(text: &str) -> Option<f32> {
    let lower = text.to_lowercase();
    if lower.contains("instock")
        || lower.contains("in_stock")
        || lower.contains("in stock")
        || lower.contains("available")
    {
        Some(1.0)
    } else if lower.contains("outofstock")
        || lower.contains("out_of_stock")
        || lower.contains("out of stock")
        || lower.contains("sold out")
        || lower.contains("soldout")
        || lower.contains("unavailable")
        || lower.contains("discontinued")
    {
        Some(0.0)
    } else if lower.contains("preorder")
        || lower.contains("pre_order")
        || lower.contains("pre-order")
        || lower.contains("backorder")
        || lower.contains("limited")
    {
        Some(0.5)
    } else {
        None
    }
}

/// Match a class or id string against page type keyword patterns from config.
/// Returns the matched `PageType` and the keyword that matched.
fn match_page_type_keywords<'a>(text: &str, keywords: &'a Value) -> Option<(PageType, &'a str)> {
    let text_lower = text.to_lowercase();

    let mapping: &[(&str, PageType)] = &[
        ("product", PageType::ProductDetail),
        ("article", PageType::Article),
        ("search", PageType::SearchResults),
        ("home", PageType::Home),
        ("cart", PageType::Cart),
        ("checkout", PageType::Checkout),
        ("login", PageType::Login),
        ("account", PageType::Account),
        ("listing", PageType::ProductListing),
        ("faq", PageType::Faq),
        ("contact", PageType::ContactPage),
        ("about", PageType::AboutPage),
        ("documentation", PageType::Documentation),
        ("forum", PageType::Forum),
        ("pricing", PageType::PricingPage),
        ("legal", PageType::Legal),
        ("error", PageType::ErrorPage),
        ("media", PageType::MediaPage),
    ];

    for &(key, page_type) in mapping {
        if let Some(kw_array) = keywords[key].as_array() {
            for kw_val in kw_array {
                if let Some(kw) = kw_val.as_str() {
                    if text_lower.contains(kw) {
                        return Some((page_type, kw));
                    }
                }
            }
        }
    }

    None
}

/// Classify a button/link label text into an OpCode `(category, action)` pair
/// by matching against known action keywords from config.
fn classify_action_label(label: &str, action_keywords: &Value) -> Option<(u8, u8)> {
    let label_lower = label.to_lowercase();

    let mapping: &[(&str, (u8, u8))] = &[
        ("commerce_add", (0x02, 0x00)),
        ("form_submit", (0x04, 0x00)),
        ("auth_login", (0x04, 0x01)),
        ("form_subscribe", (0x04, 0x03)),
        ("nav_search", (0x00, 0x01)),
    ];

    for &(key, opcode) in mapping {
        if let Some(kw_array) = action_keywords[key].as_array() {
            for kw_val in kw_array {
                if let Some(kw) = kw_val.as_str() {
                    if label_lower.contains(kw) {
                        return Some(opcode);
                    }
                }
            }
        }
    }

    None
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base_url: &str, relative: &str) -> String {
    if relative.is_empty() {
        return base_url.to_string();
    }
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if let Ok(base) = url::Url::parse(base_url) {
        if let Ok(resolved) = base.join(relative) {
            return resolved.to_string();
        }
    }
    relative.to_string()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_price_usd() {
        assert_eq!(parse_price_text("$29.99"), Some(29.99));
        assert_eq!(parse_price_text("$ 1,234.56"), Some(1234.56));
        assert_eq!(parse_price_text("1234"), Some(1234.0));
    }

    #[test]
    fn test_parse_price_european() {
        // European: comma as decimal
        assert_eq!(parse_price_text("29,99"), Some(29.99));
        // European with thousands dot: 1.234,56
        let result = parse_price_text("1.234,56");
        assert!((result.unwrap() - 1234.56).abs() < 0.01);
    }

    #[test]
    fn test_parse_price_empty() {
        assert_eq!(parse_price_text(""), None);
        assert_eq!(parse_price_text("abc"), None);
        assert_eq!(parse_price_text("$0.00"), None); // zero price filtered out
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_number("4.5"), Some(4.5));
        assert_eq!(parse_number("  3.7 stars"), Some(3.7));
        assert_eq!(parse_number("abc"), None);
    }

    #[test]
    fn test_detect_rating_max() {
        assert_eq!(detect_rating_max(4.5), 5.0);
        assert_eq!(detect_rating_max(7.8), 10.0);
        assert_eq!(detect_rating_max(85.0), 100.0);
        assert_eq!(detect_rating_max(3.0), 5.0);
    }

    #[test]
    fn test_classify_availability_text() {
        assert_eq!(
            classify_availability_text("https://schema.org/InStock"),
            Some(1.0)
        );
        assert_eq!(
            classify_availability_text("https://schema.org/OutOfStock"),
            Some(0.0)
        );
        assert_eq!(
            classify_availability_text("https://schema.org/PreOrder"),
            Some(0.5)
        );
        assert_eq!(classify_availability_text("unknown-value"), None);
    }

    #[test]
    fn test_extract_price_from_data_attribute() {
        let html = r#"
        <html><body>
            <span data-price="49.99">$49.99</span>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.price.is_some());
        let (price, confidence) = result.price.unwrap();
        assert!((price - 49.99).abs() < 0.01);
        assert!(confidence >= 0.95);
    }

    #[test]
    fn test_extract_price_from_itemprop() {
        let html = r#"
        <html><body>
            <meta itemprop="price" content="29.99" />
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.price.is_some());
        let (price, confidence) = result.price.unwrap();
        assert!((price - 29.99).abs() < 0.01);
        assert!(confidence >= 0.95);
    }

    #[test]
    fn test_extract_price_from_class() {
        let html = r#"
        <html><body>
            <span class="product-price">$19.99</span>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.price.is_some());
        let (price, confidence) = result.price.unwrap();
        assert!((price - 19.99).abs() < 0.01);
        assert!((confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_extract_price_from_regex() {
        let html = r#"
        <html><body>
            <p>The total comes to $99.95 for this item.</p>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.price.is_some());
        let (price, confidence) = result.price.unwrap();
        assert!((price - 99.95).abs() < 0.01);
        assert!((confidence - 0.70).abs() < 0.01);
    }

    #[test]
    fn test_extract_rating_from_data_attribute() {
        let html = r#"
        <html><body>
            <div data-rating="4.5">4.5 stars</div>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.rating.is_some());
        let (rating, confidence) = result.rating.unwrap();
        assert!((rating - 0.9).abs() < 0.01); // 4.5 / 5.0 = 0.9
        assert!(confidence >= 0.95);
        assert_eq!(result.rating_max, Some(5.0));
    }

    #[test]
    fn test_extract_rating_from_aria_label() {
        let html = r#"
        <html><body>
            <div aria-label="3.5 out of 5 stars">***</div>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.rating.is_some());
        let (rating, confidence) = result.rating.unwrap();
        assert!((rating - 0.7).abs() < 0.01); // 3.5 / 5.0 = 0.7
        assert!((confidence - 0.90).abs() < 0.01);
        assert_eq!(result.rating_max, Some(5.0));
    }

    #[test]
    fn test_extract_rating_from_aria_label_ten_scale() {
        let html = r#"
        <html><body>
            <div aria-label="7.5 out of 10 rating">7.5</div>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.rating.is_some());
        let (rating, _confidence) = result.rating.unwrap();
        assert!((rating - 0.75).abs() < 0.01); // 7.5 / 10.0 = 0.75
        assert_eq!(result.rating_max, Some(10.0));
    }

    #[test]
    fn test_extract_availability_itemprop() {
        let html = r#"
        <html><body>
            <link itemprop="availability" href="https://schema.org/InStock" />
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.availability.is_some());
        let (avail, confidence) = result.availability.unwrap();
        assert!((avail - 1.0).abs() < 0.01);
        assert!(confidence >= 0.95);
    }

    #[test]
    fn test_extract_availability_class() {
        let html = r#"
        <html><body>
            <span class="out-of-stock">Out of Stock</span>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.availability.is_some());
        let (avail, confidence) = result.availability.unwrap();
        assert!(avail < 0.01);
        assert!((confidence - 0.90).abs() < 0.01);
    }

    #[test]
    fn test_extract_availability_text() {
        let html = r#"
        <html><body>
            <p>This item is currently sold out.</p>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.availability.is_some());
        let (avail, confidence) = result.availability.unwrap();
        assert!(avail < 0.01);
        assert!((confidence - 0.80).abs() < 0.01);
    }

    #[test]
    fn test_extract_page_type_body_class() {
        let html = r#"
        <html><body class="product-page theme-dark">
            <h1>Widget</h1>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.page_type.is_some());
        let (pt, confidence) = result.page_type.unwrap();
        assert_eq!(pt, PageType::ProductDetail);
        assert!((confidence - 0.90).abs() < 0.01);
    }

    #[test]
    fn test_extract_page_type_main_class() {
        let html = r#"
        <html><body>
            <main class="search-results-container">
                <div>Results here</div>
            </main>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.page_type.is_some());
        let (pt, confidence) = result.page_type.unwrap();
        assert_eq!(pt, PageType::SearchResults);
        assert!((confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_extract_page_type_body_id() {
        let html = r#"
        <html><body id="checkout-page">
            <div>Checkout form</div>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.page_type.is_some());
        let (pt, confidence) = result.page_type.unwrap();
        assert_eq!(pt, PageType::Checkout);
        assert!((confidence - 0.80).abs() < 0.01);
    }

    #[test]
    fn test_extract_actions_button() {
        let html = r#"
        <html><body>
            <button>Add to Cart</button>
            <button>Submit</button>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert!(result.actions.len() >= 2);

        let cart_action = result
            .actions
            .iter()
            .find(|a| a.label.to_lowercase().contains("add to cart"));
        assert!(cart_action.is_some());
        assert_eq!(cart_action.unwrap().opcode, (0x02, 0x00));

        let submit_action = result
            .actions
            .iter()
            .find(|a| a.label.to_lowercase().contains("submit"));
        assert!(submit_action.is_some());
        assert_eq!(submit_action.unwrap().opcode, (0x04, 0x00));
    }

    #[test]
    fn test_extract_actions_input_submit() {
        let html = r#"
        <html><body>
            <input type="submit" value="Sign In" />
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        let login_action = result.actions.iter().find(|a| a.opcode == (0x04, 0x01));
        assert!(login_action.is_some());
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
        let result = extract_from_patterns(html, "https://example.com");
        assert_eq!(result.forms.len(), 1);
        assert_eq!(result.forms[0].action_url, "https://example.com/search");
        assert_eq!(result.forms[0].method, "GET");
        assert_eq!(result.forms[0].fields.len(), 1); // submit has no name attr usually
    }

    #[test]
    fn test_extract_forms_with_named_submit() {
        let html = r#"
        <html><body>
            <form action="/login" method="POST">
                <input type="text" name="username" />
                <input type="password" name="password" />
                <input type="submit" name="action" value="Log In" />
            </form>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        assert_eq!(result.forms.len(), 1);
        assert_eq!(result.forms[0].method, "POST");
        assert_eq!(result.forms[0].fields.len(), 3);
    }

    #[test]
    fn test_extract_cta_links() {
        let html = r#"
        <html><body>
            <a href="/signup" class="btn btn-primary">Subscribe</a>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        let subscribe = result
            .actions
            .iter()
            .find(|a| a.label.to_lowercase().contains("subscribe"));
        assert!(subscribe.is_some());
        assert_eq!(subscribe.unwrap().opcode, (0x04, 0x03));
    }

    #[test]
    fn test_extract_cta_link_generic() {
        let html = r#"
        <html><body>
            <a href="/learn-more" class="button-large">Learn More</a>
        </body></html>
        "#;
        let result = extract_from_patterns(html, "https://example.com");
        let cta = result
            .actions
            .iter()
            .find(|a| a.label.contains("Learn More"));
        assert!(cta.is_some());
        // Generic navigation opcode
        assert_eq!(cta.unwrap().opcode, (0x00, 0x00));
    }

    #[test]
    fn test_currency_detection() {
        let html = format!(
            r#"
        <html><body>
            <span class="price">{}49.99</span>
        </body></html>
        "#,
            '\u{00A3}' // pound sign
        );
        let result = extract_from_patterns(&html, "https://example.com");
        assert_eq!(result.currency.as_deref(), Some("GBP"));
    }

    #[test]
    fn test_resolve_url_relative() {
        assert_eq!(
            resolve_url("https://example.com/page", "/search"),
            "https://example.com/search"
        );
        assert_eq!(
            resolve_url("https://example.com/page", "https://other.com/api"),
            "https://other.com/api"
        );
        assert_eq!(
            resolve_url("https://example.com/page", ""),
            "https://example.com/page"
        );
    }

    #[test]
    fn test_empty_html() {
        let result = extract_from_patterns("", "https://example.com");
        assert!(result.price.is_none());
        assert!(result.rating.is_none());
        assert!(result.availability.is_none());
        assert!(result.page_type.is_none());
        assert!(result.actions.is_empty());
        assert!(result.forms.is_empty());
    }

    #[test]
    fn test_full_product_page() {
        let html = r#"
        <html>
        <body class="product-page">
            <div data-price="149.99">
                <span class="price">$149.99</span>
            </div>
            <div data-rating="4.2">
                <span class="star-rating">4.2 out of 5</span>
            </div>
            <link itemprop="availability" href="https://schema.org/InStock" />
            <button>Add to Cart</button>
            <button>Buy Now</button>
            <form action="/cart/add" method="POST">
                <input type="hidden" name="product_id" />
                <input type="number" name="quantity" />
                <input type="submit" value="Add to Cart" />
            </form>
        </body>
        </html>
        "#;

        let result = extract_from_patterns(html, "https://shop.example.com");

        // Price
        assert!(result.price.is_some());
        let (price, conf) = result.price.unwrap();
        assert!((price - 149.99).abs() < 0.01);
        assert!(conf >= 0.95);

        // Rating
        assert!(result.rating.is_some());
        let (rating, _) = result.rating.unwrap();
        assert!((rating - 0.84).abs() < 0.01); // 4.2 / 5.0

        // Availability
        assert!(result.availability.is_some());
        let (avail, _) = result.availability.unwrap();
        assert!((avail - 1.0).abs() < 0.01);

        // Page type
        assert!(result.page_type.is_some());
        assert_eq!(result.page_type.unwrap().0, PageType::ProductDetail);

        // Actions
        assert!(!result.actions.is_empty());
        let add_to_cart = result.actions.iter().any(|a| a.opcode == (0x02, 0x00));
        assert!(add_to_cart);

        // Forms
        assert_eq!(result.forms.len(), 1);
        assert_eq!(
            result.forms[0].action_url,
            "https://shop.example.com/cart/add"
        );
    }
}
