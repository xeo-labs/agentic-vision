# Cortex v0.2 — Test Report

**Date:** 2026-02-18
**Architecture:** Layered Acquisition (HTTP-first, browser fallback)
**Test Suite:** 100 public websites across 10 categories
**Scoring:** 100 points per site (7 categories)

## Executive Summary

| Metric | Value |
|--------|-------|
| Average Score | **81.4/100** |
| Sites >= 90 | **33/100** |
| Sites >= 80 | **64/100** |
| Sites >= 50 | **96/100** |
| Sites < 50 | **4/100** |
| JSON-LD Coverage | **94%** |
| Pattern Engine Coverage | **94%** |
| HTTP Action Coverage | **33%** |
| Total Errors | **17** |
| Daemon Crashes | **0** |

## Architecture Overview

Cortex v0.2 uses a **Layered Acquisition** architecture that maps websites primarily via HTTP without requiring a browser:

| Layer | Method | Purpose |
|-------|--------|---------|
| 0 | Sitemap + Robots + HEAD + Feeds + JS state | URL discovery |
| 0e2 | Browser homepage (fallback) | SPA link extraction |
| 1 | HTTP GET + Structured Data (JSON-LD, OG, Microdata) | Content extraction |
| 1.5 | CSS Selector Pattern Engine | Feature extraction |
| 2 | API Discovery | REST endpoint detection |
| 2.5 | Action Discovery | Form/button/CTA detection |
| 3 | Browser Rendering (fallback) | JS-heavy pages |

The browser is used **only** when HTTP-based layers cannot extract sufficient data (< 20% completeness).

## Scoring Breakdown

Each site is scored across 7 categories:

| Category | Max | Average | Notes |
|----------|-----|---------|-------|
| Mapping | 20 | 16.4 | Node count, edge count, speed, no-browser bonus |
| Data Source Quality | 15 | 11.8 | JSON-LD, OpenGraph, structured data |
| Feature Quality | 15 | 12.7 | Feature vector completeness |
| Querying | 15 | 14.2 | Filter, nearest-neighbor, pagination |
| Pathfinding | 10 | 9.1 | Path exists, reasonable length |
| Action Discovery | 15 | 7.3 | Actions found, HTTP-executable, platform |
| Live Verification | 10 | 9.1 | Browser perceive homepage + interior |

**Key gap:** Action discovery averages 7.3/15 — HTTP-executable actions are only found on 33% of sites because most sites require JavaScript interaction for forms.

## Category Results

| Category | Sites | Avg | Min | Max |
|----------|-------|-----|-----|-----|
| Government | 10 | 87.4 | 67 | 100 |
| Financial | 5 | 87.0 | 73 | 94 |
| Social/Community | 10 | 86.2 | 72 | 99 |
| Documentation | 10 | 85.3 | 63 | 92 |
| Misc | 10 | 84.3 | 67 | 98 |
| Docs & SPA | 10 | 81.5 | 56 | 92 |
| News & Media | 10 | 80.0 | 30 | 94 |
| E-Commerce | 15 | 78.3 | 35 | 97 |
| Food & Local | 5 | 77.4 | 30 | 98 |
| Travel | 10 | 71.7 | 57 | 92 |

## Top Performers (>= 93)

| Score | Site | Nodes | Features |
|-------|------|-------|----------|
| 100 | usa.gov | 1,599 | J P A |
| 99 | news.ycombinator.com | 1,079 | J P A |
| 98 | healthline.com | 1,081 | J P A |
| 95 | zappos.com | 1,633 | J P A |
| 95 | allrecipes.com | 1,715 | J P A |
| 94 | theverge.com | 629 | J P A |
| 94 | coinmarketcap.com | 1,872 | J P |
| 94 | newegg.com | 1,585 | J P |
| 93 | amazon.com | 533 | J P A |
| 93 | target.com | 675 | J P A |
| 93 | linkedin.com | 2,290 | J P A |
| 93 | gov.uk | 628 | J P A |
| 93 | census.gov | 5,287 | J P A |
| 93 | github.com | 721 | J P A |
| 93 | crates.io | 353 | J P A |

Legend: J = JSON-LD, P = Pattern Engine, A = HTTP Actions

## Lowest Performers (< 60)

| Score | Site | Nodes | Issue |
|-------|------|-------|-------|
| 30 | washingtonpost.com | 1 | HTTP/2 protocol error (403 + H2 rejection) |
| 30 | opentable.com | 1 | HTTP/2 protocol error |
| 35 | bestbuy.com | 1 | Heavy SPA, no sitemap, bot detection |
| 35 | reddit.com* | 1 | Cached fallback from prior crash |
| 56 | netflix.com | 19 | Heavy SPA, minimal sitemap, bot detection |
| 57 | hotels.com | 62 | HTTP/2 protocol error in perceive |

*reddit.com scored 92/100 on initial mapping but 35 on a second cached attempt.

## Known Issues

### HTTP/2 Protocol Errors
Sites like washingtonpost.com, costco.com, hotels.com, and opentable.com reject HTTP/2 connections from Chrome's headless browser. The HTTP client has HTTP/1.1 fallback, but Chrome's internal networking does not. This affects the PERCEIVE (live verification) step.

**Impact:** ~10 points lost per affected site (live verification category).

### Heavy SPA Sites
Sites like bestbuy.com and netflix.com render navigation entirely in JavaScript. Without executing JS, the HTTP-only layers discover very few URLs. The browser homepage fallback helps but some sites actively block headless Chrome.

**Impact:** 30-56 point scores on affected sites.

### Action Discovery Gaps
HTTP-executable actions (forms, API endpoints) are only detected on 33% of sites. Most modern sites use JavaScript-based forms and SPAs that don't expose traditional HTML forms.

**Impact:** ~5 points lost per site without HTTP actions.

## Improvements from v0.1

| Metric | v0.1 (est) | v0.2 |
|--------|------------|------|
| Average Score | ~60 | 81.4 |
| Sites >= 80 | ~20 | 64 |
| Sites >= 90 | ~5 | 33 |
| Daemon Crashes | Frequent | 0 |
| JSON-LD Coverage | ~70% | 94% |
| Pattern Coverage | ~50% | 94% |

### Key Fixes Applied (3 iterations)

1. **Iteration 1:** Microdata extraction, HTTP/1.1 fallback, NoopRenderer for graceful degradation, expanded CSS selectors
2. **Iteration 2:** Chrome `--headless=new` for modern Chrome 120+, SingletonLock cleanup, sitemap discovery expansion (4 → 10 paths)
3. **Iteration 3:** Browser homepage fallback for SPA sites, expanded common paths (12 → 55+), per-page browser timeout (20s), panic guard in MAP handler, per-site test timeout (90s)

## Test Methodology

- **100 sites** across 10 categories (e-commerce, news, social, docs, SPA, government, travel, food, financial, misc)
- Each site tested for: mapping quality, data source extraction, feature completeness, query capability, pathfinding, action discovery, and live browser verification
- Tests run sequentially with 1-second delays between sites
- 90-second per-site timeout to prevent hangs
- Daemon stability monitored throughout (0 crashes in final run)

## Files

- Test harness: `test_harness_v2.py`
- Raw results: `test-report-v2.json`
- This report: `docs/TEST-REPORT-V2.md`
