//! `cortex query <domain>` — query a mapped site for matching pages.

use anyhow::Result;

/// Run the query command.
pub async fn run(
    domain: &str,
    page_type: Option<&str>,
    price_lt: Option<f32>,
    rating_gt: Option<f32>,
    limit: u32,
) -> Result<()> {
    println!("cortex query: not yet implemented");
    println!("  domain: {domain}");
    if let Some(t) = page_type {
        println!("  type:   {t}");
    }
    if let Some(p) = price_lt {
        println!("  price<  {p}");
    }
    if let Some(r) = rating_gt {
        println!("  rating> {r}");
    }
    println!("  limit:  {limit}");
    Ok(())
}
