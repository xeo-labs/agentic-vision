//! Core SiteMap types matching the binary format specification in 02-map-spec.md.

use serde::{Deserialize, Serialize};
use std::fmt;

// ─── Magic bytes and version ──────────────────────────────────────────────────

/// Magic bytes: "CTX\0"
pub const SITEMAP_MAGIC: u32 = 0x43545800;

/// Current binary format version
pub const FORMAT_VERSION: u16 = 1;

// ─── Feature vector dimension constants ───────────────────────────────────────

// Dimensions 0-15: Page Identity
pub const FEAT_PAGE_TYPE: usize = 0;
pub const FEAT_PAGE_TYPE_CONFIDENCE: usize = 1;
pub const FEAT_CONTENT_LANGUAGE: usize = 2;
pub const FEAT_PAGE_DEPTH: usize = 3;
pub const FEAT_IS_AUTH_AREA: usize = 4;
pub const FEAT_HAS_PAYWALL: usize = 5;
pub const FEAT_IS_MOBILE_OPTIMIZED: usize = 6;
pub const FEAT_LOAD_TIME: usize = 7;
pub const FEAT_IS_HTTPS: usize = 8;
pub const FEAT_URL_PATH_DEPTH: usize = 9;
pub const FEAT_URL_HAS_QUERY: usize = 10;
pub const FEAT_URL_HAS_FRAGMENT: usize = 11;
pub const FEAT_IS_CANONICAL: usize = 12;
pub const FEAT_HAS_STRUCTURED_DATA: usize = 13;
pub const FEAT_META_ROBOTS_INDEX: usize = 14;
pub const FEAT_RESERVED_IDENTITY: usize = 15;

// Dimensions 16-47: Content Metrics
pub const FEAT_TEXT_DENSITY: usize = 16;
pub const FEAT_TEXT_LENGTH_LOG: usize = 17;
pub const FEAT_HEADING_COUNT: usize = 18;
pub const FEAT_PARAGRAPH_COUNT: usize = 19;
pub const FEAT_IMAGE_COUNT: usize = 20;
pub const FEAT_VIDEO_PRESENT: usize = 21;
pub const FEAT_TABLE_COUNT: usize = 22;
pub const FEAT_LIST_COUNT: usize = 23;
pub const FEAT_FORM_FIELD_COUNT: usize = 24;
pub const FEAT_LINK_COUNT_INTERNAL: usize = 25;
pub const FEAT_LINK_COUNT_EXTERNAL: usize = 26;
pub const FEAT_AD_DENSITY: usize = 27;
pub const FEAT_CONTENT_UNIQUENESS: usize = 28;
pub const FEAT_READING_LEVEL: usize = 29;
pub const FEAT_SENTIMENT: usize = 30;
pub const FEAT_TOPIC_EMBED_START: usize = 31;
pub const FEAT_TOPIC_EMBED_END: usize = 46;
pub const FEAT_STRUCTURED_DATA_RICHNESS: usize = 47;

// Dimensions 48-63: Commerce Features
pub const FEAT_PRICE: usize = 48;
pub const FEAT_PRICE_ORIGINAL: usize = 49;
pub const FEAT_DISCOUNT_PCT: usize = 50;
pub const FEAT_AVAILABILITY: usize = 51;
pub const FEAT_RATING: usize = 52;
pub const FEAT_REVIEW_COUNT_LOG: usize = 53;
pub const FEAT_REVIEW_SENTIMENT: usize = 54;
pub const FEAT_SHIPPING_FREE: usize = 55;
pub const FEAT_SHIPPING_SPEED: usize = 56;
pub const FEAT_RETURN_POLICY: usize = 57;
pub const FEAT_SELLER_REPUTATION: usize = 58;
pub const FEAT_VARIANT_COUNT: usize = 59;
pub const FEAT_COMPARISON_AVAILABLE: usize = 60;
pub const FEAT_PRICE_TREND: usize = 61;
pub const FEAT_CATEGORY_PRICE_PERCENTILE: usize = 62;
pub const FEAT_DEAL_SCORE: usize = 63;

// Dimensions 64-79: Navigation Features
pub const FEAT_OUTBOUND_LINKS: usize = 64;
pub const FEAT_PAGINATION_PRESENT: usize = 65;
pub const FEAT_PAGINATION_POSITION: usize = 66;
pub const FEAT_BREADCRUMB_DEPTH: usize = 67;
pub const FEAT_NAV_MENU_ITEMS: usize = 68;
pub const FEAT_SEARCH_AVAILABLE: usize = 69;
pub const FEAT_FILTER_COUNT: usize = 70;
pub const FEAT_SORT_OPTIONS: usize = 71;
pub const FEAT_RELATED_CONTENT_COUNT: usize = 72;
pub const FEAT_ESTIMATED_NEXT_RELEVANCE: usize = 73;
pub const FEAT_IS_DEAD_END: usize = 74;
pub const FEAT_SITE_SECTION_DEPTH: usize = 75;
pub const FEAT_SITE_SECTION_BREADTH: usize = 76;
pub const FEAT_GOAL_DISTANCE_ESTIMATE: usize = 77;
pub const FEAT_LOOP_RISK: usize = 78;
pub const FEAT_EXIT_PROBABILITY: usize = 79;

// Dimensions 80-95: Trust & Safety
pub const FEAT_TLS_VALID: usize = 80;
pub const FEAT_DOMAIN_AGE: usize = 81;
pub const FEAT_DOMAIN_REPUTATION: usize = 82;
pub const FEAT_DARK_PATTERN_COUNT: usize = 83;
pub const FEAT_PII_EXPOSURE_RISK: usize = 84;
pub const FEAT_CONTENT_CONSISTENCY: usize = 85;
pub const FEAT_BOT_CHALLENGE_PRESENT: usize = 86;
pub const FEAT_BOT_CHALLENGE_SEVERITY: usize = 87;
pub const FEAT_COOKIE_CONSENT_BLOCKING: usize = 88;
pub const FEAT_POPUP_COUNT: usize = 89;
pub const FEAT_REDIRECT_COUNT: usize = 90;
pub const FEAT_MIXED_CONTENT: usize = 91;
pub const FEAT_TRACKER_COUNT: usize = 92;
pub const FEAT_CONTENT_FRESHNESS: usize = 93;
pub const FEAT_AUTHORITY_SCORE: usize = 94;
pub const FEAT_SCAM_PROBABILITY: usize = 95;

// Dimensions 96-111: Action Features
pub const FEAT_ACTION_COUNT: usize = 96;
pub const FEAT_SAFE_ACTION_RATIO: usize = 97;
pub const FEAT_CAUTIOUS_ACTION_RATIO: usize = 98;
pub const FEAT_DESTRUCTIVE_ACTION_RATIO: usize = 99;
pub const FEAT_AUTH_REQUIRED_RATIO: usize = 100;
pub const FEAT_FORM_COMPLETENESS: usize = 101;
pub const FEAT_FORM_STEPS_REMAINING: usize = 102;
pub const FEAT_CART_ITEM_COUNT: usize = 103;
pub const FEAT_CART_TOTAL: usize = 104;
pub const FEAT_CHECKOUT_STEPS_REMAINING: usize = 105;
pub const FEAT_PRIMARY_CTA_PRESENT: usize = 106;
pub const FEAT_PRIMARY_CTA_CATEGORY: usize = 107;
pub const FEAT_DOWNLOAD_AVAILABLE: usize = 108;
pub const FEAT_SHARE_AVAILABLE: usize = 109;
pub const FEAT_SAVE_AVAILABLE: usize = 110;
pub const FEAT_UNDO_AVAILABLE: usize = 111;

// Dimensions 112-127: Session & Context
pub const FEAT_SESSION_PAGE_COUNT: usize = 112;
pub const FEAT_SESSION_ACTION_COUNT: usize = 113;
pub const FEAT_SESSION_DURATION: usize = 114;
pub const FEAT_UNIQUE_DOMAINS: usize = 115;
pub const FEAT_FLOW_STEP_CURRENT: usize = 116;
pub const FEAT_FLOW_STEP_TOTAL: usize = 117;
pub const FEAT_FLOW_COMPLETION: usize = 118;
pub const FEAT_BACKTRACK_COUNT: usize = 119;
pub const FEAT_REVISIT_RATIO: usize = 120;
pub const FEAT_DATA_EXTRACTED: usize = 121;
pub const FEAT_GOAL_SIMILARITY: usize = 122;
pub const FEAT_TIME_BUDGET_REMAINING: usize = 123;
pub const FEAT_PAGE_BUDGET_REMAINING: usize = 124;
pub const FEAT_ERROR_COUNT: usize = 125;
pub const FEAT_BLOCKED_COUNT: usize = 126;
pub const FEAT_SESSION_HEALTH: usize = 127;

/// Number of dimensions in the feature vector.
pub const FEATURE_DIM: usize = 128;

// ─── PageType enum ────────────────────────────────────────────────────────────

/// Classification of a web page by its function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PageType {
    Unknown = 0x00,
    Home = 0x01,
    SearchResults = 0x02,
    ProductListing = 0x03,
    ProductDetail = 0x04,
    Article = 0x05,
    Documentation = 0x06,
    FormPage = 0x07,
    Login = 0x08,
    Checkout = 0x09,
    Cart = 0x0A,
    Account = 0x0B,
    ErrorPage = 0x0C,
    Captcha = 0x0D,
    MediaPage = 0x0E,
    Comparison = 0x0F,
    ReviewList = 0x10,
    MapLocation = 0x11,
    Dashboard = 0x12,
    ApiDocs = 0x13,
    Legal = 0x14,
    DownloadPage = 0x15,
    SocialFeed = 0x16,
    Forum = 0x17,
    Messaging = 0x18,
    Calendar = 0x19,
    FileBrowser = 0x1A,
    PricingPage = 0x1B,
    AboutPage = 0x1C,
    ContactPage = 0x1D,
    Faq = 0x1E,
    SitemapPage = 0x1F,
}

impl PageType {
    /// Total number of defined page types.
    pub const COUNT: usize = 32;

    /// Convert from u8 to PageType.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x00 => Self::Unknown,
            0x01 => Self::Home,
            0x02 => Self::SearchResults,
            0x03 => Self::ProductListing,
            0x04 => Self::ProductDetail,
            0x05 => Self::Article,
            0x06 => Self::Documentation,
            0x07 => Self::FormPage,
            0x08 => Self::Login,
            0x09 => Self::Checkout,
            0x0A => Self::Cart,
            0x0B => Self::Account,
            0x0C => Self::ErrorPage,
            0x0D => Self::Captcha,
            0x0E => Self::MediaPage,
            0x0F => Self::Comparison,
            0x10 => Self::ReviewList,
            0x11 => Self::MapLocation,
            0x12 => Self::Dashboard,
            0x13 => Self::ApiDocs,
            0x14 => Self::Legal,
            0x15 => Self::DownloadPage,
            0x16 => Self::SocialFeed,
            0x17 => Self::Forum,
            0x18 => Self::Messaging,
            0x19 => Self::Calendar,
            0x1A => Self::FileBrowser,
            0x1B => Self::PricingPage,
            0x1C => Self::AboutPage,
            0x1D => Self::ContactPage,
            0x1E => Self::Faq,
            0x1F => Self::SitemapPage,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for PageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Unknown => "unknown",
            Self::Home => "home",
            Self::SearchResults => "search_results",
            Self::ProductListing => "product_listing",
            Self::ProductDetail => "product_detail",
            Self::Article => "article",
            Self::Documentation => "documentation",
            Self::FormPage => "form_page",
            Self::Login => "login",
            Self::Checkout => "checkout",
            Self::Cart => "cart",
            Self::Account => "account",
            Self::ErrorPage => "error_page",
            Self::Captcha => "captcha",
            Self::MediaPage => "media_page",
            Self::Comparison => "comparison",
            Self::ReviewList => "review_list",
            Self::MapLocation => "map_location",
            Self::Dashboard => "dashboard",
            Self::ApiDocs => "api_docs",
            Self::Legal => "legal",
            Self::DownloadPage => "download_page",
            Self::SocialFeed => "social_feed",
            Self::Forum => "forum",
            Self::Messaging => "messaging",
            Self::Calendar => "calendar",
            Self::FileBrowser => "file_browser",
            Self::PricingPage => "pricing_page",
            Self::AboutPage => "about_page",
            Self::ContactPage => "contact_page",
            Self::Faq => "faq",
            Self::SitemapPage => "sitemap_page",
        };
        write!(f, "{s}")
    }
}

// ─── EdgeType enum ────────────────────────────────────────────────────────────

/// Classification of an edge (link) between pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EdgeType {
    Navigation = 0x00,
    ContentLink = 0x01,
    Pagination = 0x02,
    Related = 0x03,
    Breadcrumb = 0x04,
    FormSubmit = 0x05,
    ActionResult = 0x06,
    Redirect = 0x07,
    External = 0x08,
    Anchor = 0x09,
}

impl EdgeType {
    /// Convert from u8 to EdgeType.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x00 => Self::Navigation,
            0x01 => Self::ContentLink,
            0x02 => Self::Pagination,
            0x03 => Self::Related,
            0x04 => Self::Breadcrumb,
            0x05 => Self::FormSubmit,
            0x06 => Self::ActionResult,
            0x07 => Self::Redirect,
            0x08 => Self::External,
            0x09 => Self::Anchor,
            _ => Self::Navigation,
        }
    }
}

// ─── NodeFlags ────────────────────────────────────────────────────────────────

/// Bitfield flags for a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NodeFlags(pub u8);

impl NodeFlags {
    pub const RENDERED: u8 = 1 << 0;
    pub const ESTIMATED: u8 = 1 << 1;
    pub const STALE: u8 = 1 << 2;
    pub const BLOCKED: u8 = 1 << 3;
    pub const AUTH_REQUIRED: u8 = 1 << 4;
    pub const HAS_FORM: u8 = 1 << 5;
    pub const HAS_PRICE: u8 = 1 << 6;
    pub const HAS_MEDIA: u8 = 1 << 7;

    pub fn is_rendered(self) -> bool {
        self.0 & Self::RENDERED != 0
    }
    pub fn is_estimated(self) -> bool {
        self.0 & Self::ESTIMATED != 0
    }
    pub fn is_stale(self) -> bool {
        self.0 & Self::STALE != 0
    }
    pub fn is_blocked(self) -> bool {
        self.0 & Self::BLOCKED != 0
    }
    pub fn is_auth_required(self) -> bool {
        self.0 & Self::AUTH_REQUIRED != 0
    }
    pub fn has_form(self) -> bool {
        self.0 & Self::HAS_FORM != 0
    }
    pub fn has_price(self) -> bool {
        self.0 & Self::HAS_PRICE != 0
    }
    pub fn has_media(self) -> bool {
        self.0 & Self::HAS_MEDIA != 0
    }
}

// ─── EdgeFlags ────────────────────────────────────────────────────────────────

/// Bitfield flags for an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EdgeFlags(pub u8);

impl EdgeFlags {
    pub const REQUIRES_AUTH: u8 = 1 << 0;
    pub const REQUIRES_FORM: u8 = 1 << 1;
    pub const CHANGES_STATE: u8 = 1 << 2;
    pub const OPENS_NEW_CONTEXT: u8 = 1 << 3;
    pub const IS_DOWNLOAD: u8 = 1 << 4;
    pub const IS_NOFOLLOW: u8 = 1 << 5;

    pub fn requires_auth(self) -> bool {
        self.0 & Self::REQUIRES_AUTH != 0
    }
    pub fn changes_state(self) -> bool {
        self.0 & Self::CHANGES_STATE != 0
    }
}

// ─── NodeRecord ───────────────────────────────────────────────────────────────

/// Fixed-size record for a single page node (32 bytes in binary format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRecord {
    pub page_type: PageType,
    /// 0-255 maps to 0.0-1.0
    pub confidence: u8,
    /// 0-255 maps to 0.0-1.0
    pub freshness: u8,
    pub flags: NodeFlags,
    /// FNV-1a hash of extracted content
    pub content_hash: u32,
    /// Seconds since mapping start (0 if never rendered)
    pub rendered_at: u32,
    /// HTTP status code or 0 if unknown
    pub http_status: u16,
    /// Distance from root node in hops
    pub depth: u16,
    /// Number of edges pointing TO this node
    pub inbound_count: u16,
    /// Number of edges FROM this node
    pub outbound_count: u16,
    /// L2 norm of feature vector (precomputed)
    pub feature_norm: f32,
    /// Reserved for future use
    pub reserved: u32,
}

impl Default for NodeRecord {
    fn default() -> Self {
        Self {
            page_type: PageType::Unknown,
            confidence: 0,
            freshness: 0,
            flags: NodeFlags::default(),
            content_hash: 0,
            rendered_at: 0,
            http_status: 0,
            depth: 0,
            inbound_count: 0,
            outbound_count: 0,
            feature_norm: 0.0,
            reserved: 0,
        }
    }
}

// ─── EdgeRecord ───────────────────────────────────────────────────────────────

/// Fixed-size record for a link between pages (8 bytes in binary format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRecord {
    /// Target node index
    pub target_node: u32,
    pub edge_type: EdgeType,
    /// Traversal cost: 0=free, 255=expensive
    pub weight: u8,
    pub flags: EdgeFlags,
    pub reserved: u8,
}

// ─── ActionRecord ─────────────────────────────────────────────────────────────

/// OpCode for an action available on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpCode {
    pub category: u8,
    pub action: u8,
}

impl OpCode {
    pub fn new(category: u8, action: u8) -> Self {
        Self { category, action }
    }

    pub fn as_u16(self) -> u16 {
        ((self.category as u16) << 8) | (self.action as u16)
    }

    pub fn from_u16(val: u16) -> Self {
        Self {
            category: (val >> 8) as u8,
            action: (val & 0xFF) as u8,
        }
    }
}

/// Fixed-size record for an available action on a page (8 bytes in binary format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub opcode: OpCode,
    /// Node this action navigates to (-1 stays on page, -2 unknown)
    pub target_node: i32,
    /// 0=free, 1-254=relative cost, 255=unknown
    pub cost_hint: u8,
    /// 0=safe, 1=cautious, 2=destructive
    pub risk: u8,
}

// ─── MapHeader ────────────────────────────────────────────────────────────────

/// Header for the SiteMap binary format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapHeader {
    pub magic: u32,
    pub format_version: u16,
    pub domain: String,
    /// Unix timestamp seconds
    pub mapped_at: u64,
    pub node_count: u32,
    pub edge_count: u32,
    pub cluster_count: u16,
    /// Bit 0: has_sitemap, bit 1: progressive_active, bit 2: cached
    pub flags: u16,
}

impl MapHeader {
    pub fn has_sitemap(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn is_progressive_active(&self) -> bool {
        self.flags & 2 != 0
    }
    pub fn is_cached(&self) -> bool {
        self.flags & 4 != 0
    }
}

// ─── SiteMap ──────────────────────────────────────────────────────────────────

/// The primary data structure: a navigable binary graph of an entire website.
#[derive(Debug, Clone)]
pub struct SiteMap {
    pub header: MapHeader,
    pub nodes: Vec<NodeRecord>,
    pub edges: Vec<EdgeRecord>,
    /// CSR format: edge_index[i]..edge_index[i+1] = edges for node i
    pub edge_index: Vec<u32>,
    /// Feature matrix: one 128-float vector per node
    pub features: Vec<[f32; FEATURE_DIM]>,
    pub actions: Vec<ActionRecord>,
    /// CSR format: action_index[i]..action_index[i+1] = actions for node i
    pub action_index: Vec<u32>,
    pub cluster_assignments: Vec<u16>,
    pub cluster_centroids: Vec<[f32; FEATURE_DIM]>,
    pub urls: Vec<String>,
}

// ─── Query/result types ───────────────────────────────────────────────────────

/// Range filter for a feature dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureRange {
    pub dimension: usize,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

/// Query for filtering nodes in a SiteMap.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeQuery {
    pub page_types: Option<Vec<PageType>>,
    pub feature_ranges: Vec<FeatureRange>,
    pub require_flags: Option<NodeFlags>,
    pub exclude_flags: Option<NodeFlags>,
    pub sort_by_feature: Option<usize>,
    pub sort_ascending: bool,
    pub limit: usize,
}

/// A matched node from a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMatch {
    pub index: u32,
    pub url: String,
    pub page_type: PageType,
    pub confidence: f32,
    pub features: Vec<(usize, f32)>,
    pub similarity: Option<f32>,
}

/// Constraints for pathfinding.
#[derive(Debug, Clone, Default)]
pub struct PathConstraints {
    pub avoid_auth: bool,
    pub avoid_state_changes: bool,
    pub minimize: PathMinimize,
}

/// What to minimize in pathfinding.
#[derive(Debug, Clone, Default)]
pub enum PathMinimize {
    #[default]
    Hops,
    Weight,
    StateChanges,
}

/// A path through the SiteMap graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    pub nodes: Vec<u32>,
    pub total_weight: f32,
    pub hops: u32,
    pub required_actions: Vec<PathAction>,
}

/// An action required at a specific node along a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathAction {
    pub at_node: u32,
    pub opcode: OpCode,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;

    #[test]
    fn test_page_type_round_trip() {
        for i in 0..=0x1F {
            let pt = PageType::from_u8(i);
            assert_eq!(pt as u8, i);
        }
        assert_eq!(PageType::from_u8(0xFF), PageType::Unknown);
    }

    #[test]
    fn test_page_type_display() {
        assert_eq!(PageType::Home.to_string(), "home");
        assert_eq!(PageType::ProductDetail.to_string(), "product_detail");
        assert_eq!(PageType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_node_flags() {
        let flags = NodeFlags(NodeFlags::RENDERED | NodeFlags::HAS_PRICE);
        assert!(flags.is_rendered());
        assert!(flags.has_price());
        assert!(!flags.is_stale());
        assert!(!flags.is_blocked());
    }

    #[test]
    fn test_opcode_round_trip() {
        let op = OpCode::new(0x02, 0x00); // add_to_cart
        assert_eq!(op.as_u16(), 0x0200);
        let back = OpCode::from_u16(0x0200);
        assert_eq!(back.category, 0x02);
        assert_eq!(back.action, 0x00);
    }

    #[test]
    fn test_build_serialize_deserialize_round_trip() {
        let mut builder = SiteMapBuilder::new("example.com");

        // Add 10 nodes
        let mut features_list = Vec::new();
        for i in 0..10 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PAGE_TYPE] = (i % 5) as f32 / 31.0;
            feats[FEAT_PRICE] = 100.0 + (i as f32 * 20.0);
            feats[FEAT_RATING] = 0.5 + (i as f32 * 0.05);
            features_list.push(feats);

            let pt = match i % 5 {
                0 => PageType::Home,
                1 => PageType::Article,
                2 => PageType::ProductDetail,
                3 => PageType::AboutPage,
                _ => PageType::ContactPage,
            };

            builder.add_node(
                &format!("https://example.com/page-{i}"),
                pt,
                feats,
                200 - (i * 5) as u8,
            );
        }

        // Add 15 edges
        for i in 0..10 {
            builder.add_edge(i, (i + 1) % 10, EdgeType::Navigation, 1, EdgeFlags::default());
            if i < 5 {
                builder.add_edge(
                    i,
                    i + 5,
                    EdgeType::ContentLink,
                    2,
                    EdgeFlags::default(),
                );
            }
        }

        // Add some actions
        builder.add_action(2, OpCode::new(0x02, 0x00), -1, 0, 1); // add_to_cart on product
        builder.add_action(2, OpCode::new(0x02, 0x05), -1, 0, 0); // add_to_wishlist

        let map = builder.build();

        // Verify structure
        assert_eq!(map.header.domain, "example.com");
        assert_eq!(map.nodes.len(), 10);
        assert_eq!(map.edges.len(), 15);
        assert_eq!(map.urls.len(), 10);
        assert_eq!(map.features.len(), 10);

        // Serialize
        let data = map.serialize();
        assert!(!data.is_empty());

        // Deserialize
        let map2 = SiteMap::deserialize(&data).expect("deserialize failed");

        // Verify equality
        assert_eq!(map2.header.domain, "example.com");
        assert_eq!(map2.nodes.len(), 10);
        assert_eq!(map2.edges.len(), 15);
        assert_eq!(map2.urls.len(), 10);
        assert_eq!(map2.features.len(), 10);

        // Verify feature values round-trip
        for i in 0..10 {
            assert_eq!(map2.features[i][FEAT_PRICE], map.features[i][FEAT_PRICE]);
            assert_eq!(map2.features[i][FEAT_RATING], map.features[i][FEAT_RATING]);
        }

        // Verify actions round-trip
        assert_eq!(map2.actions.len(), map.actions.len());
    }

    #[test]
    fn test_filter_by_page_type() {
        let mut builder = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];

        builder.add_node("https://test.com/", PageType::Home, feats, 255);
        builder.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        builder.add_node("https://test.com/p2", PageType::ProductDetail, feats, 200);
        builder.add_node("https://test.com/about", PageType::AboutPage, feats, 200);

        let map = builder.build();

        let query = NodeQuery {
            page_types: Some(vec![PageType::ProductDetail]),
            limit: 100,
            ..Default::default()
        };

        let results = map.filter(&query);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.page_type == PageType::ProductDetail));
    }

    #[test]
    fn test_nearest_neighbor() {
        let mut builder = SiteMapBuilder::new("test.com");

        // Create nodes with distinct feature directions
        for i in 0..5 {
            let mut feats = [0.0f32; FEATURE_DIM];
            // Each node emphasizes a different dimension
            feats[i] = 1.0;
            // Add a small shared component so no node is orthogonal
            feats[0] += 0.1;
            builder.add_node(
                &format!("https://test.com/page-{i}"),
                PageType::Article,
                feats,
                200,
            );
        }

        let map = builder.build();

        // Target strongly aligned with dimension 2
        let mut target = [0.0f32; FEATURE_DIM];
        target[2] = 1.0;
        target[0] = 0.1;

        let results = map.nearest(&target, 2);
        assert_eq!(results.len(), 2);
        // Node 2 should be closest (same feature emphasis)
        assert_eq!(results[0].index, 2);
    }

    #[test]
    fn test_checksum_integrity() {
        let mut builder = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder.add_node("https://test.com/", PageType::Home, feats, 255);
        let map = builder.build();

        let data = map.serialize();
        // Round-trip works
        SiteMap::deserialize(&data).expect("valid data should deserialize");

        // Corrupt one byte in the middle
        let mut corrupted = data.clone();
        corrupted[data.len() / 2] ^= 0xFF;
        let err = SiteMap::deserialize(&corrupted);
        assert!(err.is_err(), "corrupted data should fail checksum");
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("checksum mismatch"),
            "error should mention checksum: {msg}"
        );
    }

    #[test]
    fn test_truncated_map_file() {
        // Too short to even have a checksum
        assert!(SiteMap::deserialize(&[]).is_err());
        assert!(SiteMap::deserialize(&[0x01]).is_err());
        assert!(SiteMap::deserialize(&[0x01, 0x02, 0x03]).is_err());
    }
}
