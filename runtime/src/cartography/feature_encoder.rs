//! Encode extraction results into 128-float feature vectors.
//!
//! ## Missing data strategy
//!
//! - `0.0` means "absent/unknown" for all optional features.
//! - [`NodeFlags`] bits distinguish "value is genuinely zero" from "unknown":
//!   - `features[48] = 0.0` + `HAS_PRICE`  → price is genuinely $0 (free)
//!   - `features[48] = 0.0` + no `HAS_PRICE` → price not found on page
//! - Non-USD currency: stored raw in original currency. No conversion.
//! - Price ranges ("$200–$350"): low end stored in `features[48]`.
//! - Text ratings ("Excellent"): mapped to numeric 0.0–1.0.

use crate::extraction::loader::ExtractionResult;
use crate::map::types::*;
use crate::renderer::NavigationResult;

/// Result of feature encoding, including computed flags.
pub struct FeatureEncodeResult {
    pub features: [f32; FEATURE_DIM],
    pub flags: NodeFlags,
}

/// Encode extraction results + navigation info into a 128-float feature vector.
///
/// Returns both the feature vector and computed [`NodeFlags`] so the caller
/// can set flags like `HAS_PRICE`, `HAS_MEDIA`, `HAS_FORM` accurately.
pub fn encode_features(
    extraction: &ExtractionResult,
    nav_result: &NavigationResult,
    url: &str,
    page_type: PageType,
    confidence: f32,
) -> [f32; FEATURE_DIM] {
    let result = encode_features_with_flags(extraction, nav_result, url, page_type, confidence);
    result.features
}

/// Encode extraction results and also return computed [`NodeFlags`].
pub fn encode_features_with_flags(
    extraction: &ExtractionResult,
    nav_result: &NavigationResult,
    url: &str,
    page_type: PageType,
    confidence: f32,
) -> FeatureEncodeResult {
    let mut feats = [0.0f32; FEATURE_DIM];
    let mut flag_bits: u8 = NodeFlags::RENDERED;

    // ── Page Identity (0-15) ──
    feats[FEAT_PAGE_TYPE] = (page_type as u8) as f32 / 31.0;
    feats[FEAT_PAGE_TYPE_CONFIDENCE] = confidence;
    feats[FEAT_LOAD_TIME] = normalize_load_time(nav_result.load_time_ms);
    feats[FEAT_IS_HTTPS] = if url.starts_with("https://") { 1.0 } else { 0.0 };
    feats[FEAT_URL_PATH_DEPTH] = count_path_depth(url) as f32 / 10.0;
    feats[FEAT_URL_HAS_QUERY] = if url.contains('?') { 1.0 } else { 0.0 };
    feats[FEAT_URL_HAS_FRAGMENT] = if url.contains('#') { 1.0 } else { 0.0 };
    feats[FEAT_HAS_STRUCTURED_DATA] = has_structured_data(&extraction.metadata);
    feats[FEAT_META_ROBOTS_INDEX] = meta_robots_index(&extraction.metadata);
    feats[FEAT_REDIRECT_COUNT] = nav_result.redirect_chain.len() as f32 / 5.0;

    // ── Content Metrics (16-47) ──
    let has_form = encode_content_features(
        &extraction.content,
        &extraction.structure,
        &mut feats,
    );
    if has_form {
        flag_bits |= NodeFlags::HAS_FORM;
    }

    // ── Commerce Features (48-63) ──
    let (has_price, has_media_from_commerce) = encode_commerce_features(
        &extraction.content,
        &extraction.metadata,
        &mut feats,
    );
    if has_price {
        flag_bits |= NodeFlags::HAS_PRICE;
    }

    // ── Navigation Features (64-79) ──
    encode_navigation_features(&extraction.navigation, &extraction.structure, &mut feats);

    // ── Trust & Safety (80-95) ──
    feats[FEAT_TLS_VALID] = if url.starts_with("https://") { 1.0 } else { 0.0 };
    feats[FEAT_CONTENT_FRESHNESS] = 1.0; // Just mapped, so fresh

    // ── Action Features (96-111) ──
    encode_action_features(&extraction.actions, &mut feats);

    // Check for media (video or many images)
    if has_media_from_commerce
        || feats[FEAT_VIDEO_PRESENT] > 0.0
        || feats[FEAT_IMAGE_COUNT] > 0.3
    {
        flag_bits |= NodeFlags::HAS_MEDIA;
    }

    // Session dimensions (112-127) default to 0.0 at mapping time

    FeatureEncodeResult {
        features: feats,
        flags: NodeFlags(flag_bits),
    }
}

fn normalize_load_time(ms: u64) -> f32 {
    // Normalize: 0ms=1.0 (best), 10000ms=0.0 (worst)
    1.0 - (ms as f32 / 10_000.0).clamp(0.0, 1.0)
}

fn count_path_depth(url: &str) -> usize {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let path = rest.split('?').next().unwrap_or(rest);
    let path = path.split('#').next().unwrap_or(path);
    // Count path segments after domain
    if let Some(slash_pos) = path.find('/') {
        path[slash_pos..]
            .split('/')
            .filter(|s| !s.is_empty())
            .count()
    } else {
        0
    }
}

fn has_structured_data(metadata: &serde_json::Value) -> f32 {
    let has_jsonld = metadata.get("jsonLd").is_some_and(|v| !v.is_null());
    let has_schema = metadata.get("schemaOrg").is_some_and(|v| !v.is_null());
    let has_og = metadata.get("openGraph").is_some_and(|v| !v.is_null());
    if has_jsonld || has_schema {
        1.0
    } else if has_og {
        0.5
    } else {
        0.0
    }
}

fn meta_robots_index(metadata: &serde_json::Value) -> f32 {
    let robots = metadata
        .get("robots")
        .and_then(|v| v.as_str())
        .unwrap_or("index");
    if robots.contains("noindex") {
        0.0
    } else {
        1.0
    }
}

/// Encode content features. Returns `true` if forms were detected.
fn encode_content_features(
    content: &serde_json::Value,
    structure: &serde_json::Value,
    feats: &mut [f32; FEATURE_DIM],
) -> bool {
    let text_density = structure
        .get("textDensity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    feats[FEAT_TEXT_DENSITY] = text_density;

    if let Some(arr) = content.as_array() {
        let heading_count = arr
            .iter()
            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("heading"))
            .count();
        let paragraph_count = arr
            .iter()
            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("paragraph"))
            .count();
        let image_count = arr
            .iter()
            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("image"))
            .count();
        let table_count = arr
            .iter()
            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("table"))
            .count();
        let list_count = arr
            .iter()
            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("list"))
            .count();

        feats[FEAT_HEADING_COUNT] = (heading_count as f32 / 10.0).clamp(0.0, 1.0);
        feats[FEAT_PARAGRAPH_COUNT] = (paragraph_count as f32 / 20.0).clamp(0.0, 1.0);
        feats[FEAT_IMAGE_COUNT] = (image_count as f32 / 20.0).clamp(0.0, 1.0);
        feats[FEAT_TABLE_COUNT] = (table_count as f32 / 5.0).clamp(0.0, 1.0);
        feats[FEAT_LIST_COUNT] = (list_count as f32 / 10.0).clamp(0.0, 1.0);

        // Text length estimate (zero vector for pages with no text)
        let total_text_len: usize = arr
            .iter()
            .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
            .map(|s| s.len())
            .sum();
        feats[FEAT_TEXT_LENGTH_LOG] =
            ((total_text_len as f32 + 1.0).ln() / 12.0).clamp(0.0, 1.0);
    }

    // Form field count from structure
    let form_fields = structure
        .get("formFieldCount")
        .or_else(|| structure.get("formCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    feats[FEAT_FORM_FIELD_COUNT] = (form_fields as f32 / 20.0).clamp(0.0, 1.0);

    // Video present
    let has_video = structure
        .get("videoCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        > 0;
    feats[FEAT_VIDEO_PRESENT] = if has_video { 1.0 } else { 0.0 };

    form_fields > 0
}

/// Encode commerce features. Returns `(has_price, has_media)`.
///
/// ## Edge case handling
///
/// - **Non-USD currency**: stored raw in original currency — no conversion.
/// - **Price ranges** ("$200–$350"): low end stored in `features[48]`.
/// - **"From $X" / "Starting at $X"**: store the starting price.
/// - **No price visible**: `features[48] = 0.0`, `HAS_PRICE` flag = false.
/// - **Text ratings** ("Excellent", "Good"): mapped to numeric 0.0–1.0.
/// - **Non-5-star scales** (1-10, percentage): detected via `maxRating` field.
fn encode_commerce_features(
    content: &serde_json::Value,
    metadata: &serde_json::Value,
    feats: &mut [f32; FEATURE_DIM],
) -> (bool, bool) {
    let mut has_price = false;
    let mut has_media = false;

    if let Some(arr) = content.as_array() {
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("price") {
                // Extract price — store raw in original currency
                if let Some(val) = item.get("value").and_then(|v| v.as_f64()) {
                    feats[FEAT_PRICE] = val as f32;
                    has_price = true;
                } else if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    // Try to parse price from text (handles ranges, "From $X", etc.)
                    if let Some(val) = parse_price_text(text) {
                        feats[FEAT_PRICE] = val;
                        has_price = true;
                    }
                }

                if let Some(original) = item.get("original").and_then(|v| v.as_f64()) {
                    feats[FEAT_PRICE_ORIGINAL] = original as f32;
                    if original > 0.0 {
                        let current = feats[FEAT_PRICE];
                        if current > 0.0 {
                            feats[FEAT_DISCOUNT_PCT] =
                                ((1.0 - current as f64 / original) as f32).clamp(0.0, 1.0);
                        }
                    }
                }
            }
            if item.get("type").and_then(|t| t.as_str()) == Some("rating") {
                // Detect max rating (default 5.0, but could be 10, 100)
                let max_rating = item
                    .get("maxRating")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(5.0);

                if let Some(val) = item.get("value").and_then(|v| v.as_f64()) {
                    // Normalize to 0.0-1.0 using actual max
                    feats[FEAT_RATING] = if max_rating > 0.0 {
                        (val / max_rating) as f32
                    } else {
                        0.0
                    }
                    .clamp(0.0, 1.0);
                } else if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    // Map text ratings to numeric
                    feats[FEAT_RATING] = text_rating_to_numeric(text);
                }

                if let Some(count) = item.get("reviewCount").and_then(|v| v.as_f64()) {
                    feats[FEAT_REVIEW_COUNT_LOG] =
                        ((count as f32 + 1.0).ln() / 10.0).clamp(0.0, 1.0);
                }
            }
            // Check for media content
            if matches!(
                item.get("type").and_then(|t| t.as_str()),
                Some("video") | Some("audio") | Some("gallery")
            ) {
                has_media = true;
            }
        }
    }

    // Try schema.org metadata for availability
    if let Some(offers) = metadata
        .get("jsonLd")
        .and_then(|v| v.get("offers"))
        .or_else(|| metadata.get("schemaOrg").and_then(|v| v.get("offers")))
    {
        let avail = offers
            .get("availability")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        feats[FEAT_AVAILABILITY] = if avail.contains("InStock") {
            1.0
        } else if avail.contains("OutOfStock") {
            0.0
        } else {
            0.5
        };

        // Also try to get price from schema if not found in content
        if !has_price {
            if let Some(price) = offers.get("price").and_then(|v| v.as_f64()) {
                feats[FEAT_PRICE] = price as f32;
                has_price = true;
            } else if let Some(price_str) = offers.get("price").and_then(|v| v.as_str()) {
                if let Ok(val) = price_str.parse::<f32>() {
                    feats[FEAT_PRICE] = val;
                    has_price = true;
                }
            }
        }
    }

    (has_price, has_media)
}

/// Parse a price from text, handling ranges, "From $X", currency symbols.
///
/// Returns the low-end price as a raw float (no currency conversion).
fn parse_price_text(text: &str) -> Option<f32> {
    // Strip common currency symbols and whitespace
    let stripped: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',' || *c == '-' || *c == ' ')
        .collect();
    let stripped = stripped.trim();

    if stripped.is_empty() {
        return None;
    }

    // Handle ranges: "200 - 350" or "200-350" → take the low end
    if let Some(dash_pos) = stripped.find('-') {
        let left = stripped[..dash_pos].trim().replace(',', "");
        if let Ok(val) = left.parse::<f32>() {
            return Some(val);
        }
    }

    // Handle comma-separated thousands: "1,299.99" → "1299.99"
    let cleaned = stripped.replace(',', "");
    // Take first number
    let num_str: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    num_str.parse::<f32>().ok()
}

/// Map text-based ratings to a normalized 0.0–1.0 value.
///
/// Handles common English rating words and percentage strings.
fn text_rating_to_numeric(text: &str) -> f32 {
    let lower = text.trim().to_lowercase();

    // Percentage: "87%" → 0.87
    if lower.ends_with('%') {
        if let Ok(val) = lower.trim_end_matches('%').trim().parse::<f32>() {
            return (val / 100.0).clamp(0.0, 1.0);
        }
    }

    // Common English text ratings
    match lower.as_str() {
        "excellent" | "outstanding" | "perfect" | "amazing" => 1.0,
        "very good" | "great" => 0.9,
        "good" => 0.8,
        "above average" => 0.7,
        "average" | "okay" | "ok" | "fair" => 0.6,
        "below average" | "mediocre" => 0.4,
        "poor" | "bad" => 0.3,
        "very poor" | "very bad" => 0.2,
        "terrible" | "awful" | "horrible" => 0.1,
        _ => {
            // Try to parse as a number (e.g., "4.5")
            if let Ok(val) = lower.parse::<f32>() {
                // Assume out of 5 if ≤ 5, out of 10 if ≤ 10
                if val <= 5.0 {
                    (val / 5.0).clamp(0.0, 1.0)
                } else if val <= 10.0 {
                    (val / 10.0).clamp(0.0, 1.0)
                } else {
                    (val / 100.0).clamp(0.0, 1.0)
                }
            } else {
                0.0 // Unknown text rating
            }
        }
    }
}

fn encode_navigation_features(
    navigation: &serde_json::Value,
    structure: &serde_json::Value,
    feats: &mut [f32; FEATURE_DIM],
) {
    if let Some(arr) = navigation.as_array() {
        let internal_count = arr
            .iter()
            .filter(|n| n.get("type").and_then(|t| t.as_str()) == Some("internal"))
            .count();
        let external_count = arr
            .iter()
            .filter(|n| n.get("type").and_then(|t| t.as_str()) == Some("external"))
            .count();
        let has_pagination = arr
            .iter()
            .any(|n| n.get("type").and_then(|t| t.as_str()) == Some("pagination"));
        let breadcrumb_count = arr
            .iter()
            .filter(|n| n.get("type").and_then(|t| t.as_str()) == Some("breadcrumb"))
            .count();

        feats[FEAT_LINK_COUNT_INTERNAL] = (internal_count as f32 / 100.0).clamp(0.0, 1.0);
        feats[FEAT_LINK_COUNT_EXTERNAL] = (external_count as f32 / 50.0).clamp(0.0, 1.0);
        feats[FEAT_OUTBOUND_LINKS] =
            ((internal_count + external_count) as f32 / 100.0).clamp(0.0, 1.0);
        feats[FEAT_PAGINATION_PRESENT] = if has_pagination { 1.0 } else { 0.0 };
        feats[FEAT_BREADCRUMB_DEPTH] = (breadcrumb_count as f32 / 5.0).clamp(0.0, 1.0);
    }

    // Search available
    let has_search = structure
        .get("hasSearch")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    feats[FEAT_SEARCH_AVAILABLE] = if has_search { 1.0 } else { 0.0 };

    // Dead end detection: no outbound links
    feats[FEAT_IS_DEAD_END] = if feats[FEAT_OUTBOUND_LINKS] < 0.01 {
        1.0
    } else {
        0.0
    };
}

fn encode_action_features(actions: &serde_json::Value, feats: &mut [f32; FEATURE_DIM]) {
    if let Some(arr) = actions.as_array() {
        let total = arr.len() as f32;
        feats[FEAT_ACTION_COUNT] = (total / 20.0).clamp(0.0, 1.0);

        if total > 0.0 {
            let safe_count = arr
                .iter()
                .filter(|a| a.get("risk").and_then(|r| r.as_u64()) == Some(0))
                .count() as f32;
            let cautious_count = arr
                .iter()
                .filter(|a| a.get("risk").and_then(|r| r.as_u64()) == Some(1))
                .count() as f32;
            let destructive_count = arr
                .iter()
                .filter(|a| a.get("risk").and_then(|r| r.as_u64()) == Some(2))
                .count() as f32;

            feats[FEAT_SAFE_ACTION_RATIO] = safe_count / total;
            feats[FEAT_CAUTIOUS_ACTION_RATIO] = cautious_count / total;
            feats[FEAT_DESTRUCTIVE_ACTION_RATIO] = destructive_count / total;
        }

        // Check for primary CTA
        let has_cta = arr.iter().any(|a| {
            let opcode = a.get("opcode").and_then(|v| v.as_u64()).unwrap_or(0);
            // Commerce or auth actions are primary CTAs
            let category = (opcode >> 8) as u8;
            category == 0x02 || category == 0x04
        });
        feats[FEAT_PRIMARY_CTA_PRESENT] = if has_cta { 1.0 } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nav() -> NavigationResult {
        NavigationResult {
            final_url: "https://example.com/page".to_string(),
            status: 200,
            redirect_chain: vec![],
            load_time_ms: 500,
        }
    }

    #[test]
    fn test_encode_basic_features() {
        let extraction = ExtractionResult {
            content: serde_json::json!([
                {"type": "heading", "text": "Hello World"},
                {"type": "paragraph", "text": "Some content here"}
            ]),
            actions: serde_json::json!([
                {"opcode": 0x0000, "risk": 0},
                {"opcode": 0x0200, "risk": 1}
            ]),
            navigation: serde_json::json!([
                {"type": "internal", "url": "/about"},
                {"type": "external", "url": "https://other.com"}
            ]),
            structure: serde_json::json!({
                "textDensity": 0.6,
                "formCount": 0
            }),
            metadata: serde_json::json!({}),
        };

        let feats = encode_features(
            &extraction,
            &make_nav(),
            "https://example.com/page",
            PageType::Article,
            0.8,
        );

        assert!(feats[FEAT_PAGE_TYPE] > 0.0);
        assert_eq!(feats[FEAT_PAGE_TYPE_CONFIDENCE], 0.8);
        assert_eq!(feats[FEAT_IS_HTTPS], 1.0);
        assert!(feats[FEAT_TEXT_DENSITY] > 0.0);
        assert!(feats[FEAT_HEADING_COUNT] > 0.0);
        assert!(feats[FEAT_ACTION_COUNT] > 0.0);
        assert!(feats[FEAT_CONTENT_FRESHNESS] == 1.0);
    }

    #[test]
    fn test_path_depth() {
        assert_eq!(count_path_depth("https://example.com/"), 0);
        assert_eq!(count_path_depth("https://example.com/a"), 1);
        assert_eq!(count_path_depth("https://example.com/a/b/c"), 3);
    }

    #[test]
    fn test_price_stored_raw() {
        let extraction = ExtractionResult {
            content: serde_json::json!([
                {"type": "price", "value": 29.99}
            ]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({}),
        };

        let result = encode_features_with_flags(
            &extraction,
            &make_nav(),
            "https://example.com",
            PageType::ProductDetail,
            0.9,
        );

        // Price stored raw, not normalized
        assert!((result.features[FEAT_PRICE] - 29.99).abs() < 0.01);
        assert!(result.flags.has_price());
    }

    #[test]
    fn test_missing_price_flag() {
        let extraction = ExtractionResult {
            content: serde_json::json!([
                {"type": "heading", "text": "No price here"}
            ]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({}),
        };

        let result = encode_features_with_flags(
            &extraction,
            &make_nav(),
            "https://example.com",
            PageType::ProductDetail,
            0.9,
        );

        assert_eq!(result.features[FEAT_PRICE], 0.0);
        assert!(!result.flags.has_price());
    }

    #[test]
    fn test_price_range_takes_low_end() {
        let extraction = ExtractionResult {
            content: serde_json::json!([
                {"type": "price", "text": "$200 - $350"}
            ]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({}),
        };

        let result = encode_features_with_flags(
            &extraction,
            &make_nav(),
            "https://example.com",
            PageType::ProductDetail,
            0.9,
        );

        assert!((result.features[FEAT_PRICE] - 200.0).abs() < 0.01);
        assert!(result.flags.has_price());
    }

    #[test]
    fn test_text_rating_to_numeric() {
        assert!((text_rating_to_numeric("Excellent") - 1.0).abs() < 0.01);
        assert!((text_rating_to_numeric("Good") - 0.8).abs() < 0.01);
        assert!((text_rating_to_numeric("average") - 0.6).abs() < 0.01);
        assert!((text_rating_to_numeric("Poor") - 0.3).abs() < 0.01);
        assert!((text_rating_to_numeric("Terrible") - 0.1).abs() < 0.01);
        assert!((text_rating_to_numeric("87%") - 0.87).abs() < 0.01);
    }

    #[test]
    fn test_rating_non_standard_max() {
        let extraction = ExtractionResult {
            content: serde_json::json!([
                {"type": "rating", "value": 8.5, "maxRating": 10.0, "reviewCount": 100}
            ]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({}),
        };

        let feats = encode_features(
            &extraction,
            &make_nav(),
            "https://example.com",
            PageType::ProductDetail,
            0.9,
        );

        // 8.5/10 = 0.85
        assert!((feats[FEAT_RATING] - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_text_rating_in_content() {
        let extraction = ExtractionResult {
            content: serde_json::json!([
                {"type": "rating", "text": "Excellent", "reviewCount": 50}
            ]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({}),
        };

        let feats = encode_features(
            &extraction,
            &make_nav(),
            "https://example.com",
            PageType::ProductDetail,
            0.9,
        );

        assert!((feats[FEAT_RATING] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_form_flag_set() {
        let extraction = ExtractionResult {
            content: serde_json::json!([]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({"formFieldCount": 5}),
            metadata: serde_json::json!({}),
        };

        let result = encode_features_with_flags(
            &extraction,
            &make_nav(),
            "https://example.com/login",
            PageType::Login,
            0.9,
        );

        assert!(result.flags.has_form());
    }

    #[test]
    fn test_zero_vector_for_empty_page() {
        let extraction = ExtractionResult {
            content: serde_json::json!([]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({}),
        };

        let feats = encode_features(
            &extraction,
            &make_nav(),
            "https://example.com",
            PageType::Unknown,
            0.0,
        );

        // Content, commerce, navigation features should all be 0
        assert_eq!(feats[FEAT_TEXT_DENSITY], 0.0);
        assert_eq!(feats[FEAT_PRICE], 0.0);
        assert_eq!(feats[FEAT_RATING], 0.0);
        assert_eq!(feats[FEAT_OUTBOUND_LINKS], 0.0);
    }

    #[test]
    fn test_parse_price_text_variants() {
        assert!((parse_price_text("$29.99").unwrap() - 29.99).abs() < 0.01);
        assert!((parse_price_text("€1,299.99").unwrap() - 1299.99).abs() < 0.01);
        assert!((parse_price_text("$200 - $350").unwrap() - 200.0).abs() < 0.01);
        assert!((parse_price_text("200-350").unwrap() - 200.0).abs() < 0.01);
        assert!(parse_price_text("Free").is_none());
        assert!(parse_price_text("").is_none());
    }

    #[test]
    fn test_schema_org_price_fallback() {
        let extraction = ExtractionResult {
            content: serde_json::json!([]),
            actions: serde_json::json!([]),
            navigation: serde_json::json!([]),
            structure: serde_json::json!({}),
            metadata: serde_json::json!({
                "jsonLd": {
                    "offers": {
                        "price": 49.99,
                        "availability": "https://schema.org/InStock"
                    }
                }
            }),
        };

        let result = encode_features_with_flags(
            &extraction,
            &make_nav(),
            "https://example.com/product",
            PageType::ProductDetail,
            0.9,
        );

        assert!((result.features[FEAT_PRICE] - 49.99).abs() < 0.01);
        assert!(result.flags.has_price());
        assert_eq!(result.features[FEAT_AVAILABILITY], 1.0);
    }
}
