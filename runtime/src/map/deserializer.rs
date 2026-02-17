//! Deserialize a SiteMap from the binary CTX format.
//!
//! Verifies the trailing CRC32 checksum to detect corruption.

use crate::map::serializer::crc32;
use crate::map::types::*;
use anyhow::{bail, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

impl SiteMap {
    /// Deserialize a SiteMap from binary CTX format.
    ///
    /// Verifies the trailing CRC32 checksum. Returns an error if the
    /// file is truncated or corrupted.
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        // Verify trailing CRC32 checksum (last 4 bytes)
        if data.len() < 4 {
            bail!("map file too small: {} bytes", data.len());
        }
        let payload = &data[..data.len() - 4];
        let stored_checksum = {
            let mut c = Cursor::new(&data[data.len() - 4..]);
            c.read_u32::<LittleEndian>().context("reading checksum")?
        };
        let computed_checksum = crc32(payload);
        if stored_checksum != computed_checksum {
            bail!(
                "map file integrity check failed: checksum mismatch \
                 (stored 0x{:08X}, computed 0x{:08X}). File may be corrupted.",
                stored_checksum,
                computed_checksum
            );
        }

        let mut r = Cursor::new(payload);

        // ─── Header ───────────────────────────────────────
        let magic = r.read_u32::<LittleEndian>().context("reading magic")?;
        if magic != SITEMAP_MAGIC {
            bail!(
                "invalid magic bytes: expected 0x{:08X}, got 0x{:08X}",
                SITEMAP_MAGIC,
                magic
            );
        }

        let format_version = r.read_u16::<LittleEndian>().context("reading version")?;
        if format_version != FORMAT_VERSION {
            bail!(
                "unsupported format version: expected {FORMAT_VERSION}, got {format_version}"
            );
        }

        let domain_length = r.read_u16::<LittleEndian>().context("reading domain length")? as usize;
        let mut domain_bytes = vec![0u8; domain_length];
        std::io::Read::read_exact(&mut r, &mut domain_bytes).context("reading domain")?;
        let domain = String::from_utf8(domain_bytes).context("domain not valid utf8")?;

        let mapped_at = r.read_u64::<LittleEndian>().context("reading mapped_at")?;
        let node_count = r.read_u32::<LittleEndian>().context("reading node_count")? as usize;
        let edge_count = r.read_u32::<LittleEndian>().context("reading edge_count")? as usize;
        let cluster_count = r.read_u16::<LittleEndian>().context("reading cluster_count")? as usize;
        let flags = r.read_u16::<LittleEndian>().context("reading flags")?;

        // ─── Node Table ───────────────────────────────────
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let page_type = PageType::from_u8(r.read_u8()?);
            let confidence = r.read_u8()?;
            let freshness = r.read_u8()?;
            let node_flags = NodeFlags(r.read_u8()?);
            let content_hash = r.read_u32::<LittleEndian>()?;
            let rendered_at = r.read_u32::<LittleEndian>()?;
            let http_status = r.read_u16::<LittleEndian>()?;
            let depth = r.read_u16::<LittleEndian>()?;
            let inbound_count = r.read_u16::<LittleEndian>()?;
            let outbound_count = r.read_u16::<LittleEndian>()?;
            let feature_norm = r.read_f32::<LittleEndian>()?;
            let reserved = r.read_u32::<LittleEndian>()?;

            nodes.push(NodeRecord {
                page_type,
                confidence,
                freshness,
                flags: node_flags,
                content_hash,
                rendered_at,
                http_status,
                depth,
                inbound_count,
                outbound_count,
                feature_norm,
                reserved,
            });
        }

        // ─── Edge Table ───────────────────────────────────
        let mut edges = Vec::with_capacity(edge_count);
        for _ in 0..edge_count {
            let target_node = r.read_u32::<LittleEndian>()?;
            let edge_type = EdgeType::from_u8(r.read_u8()?);
            let weight = r.read_u8()?;
            let edge_flags = EdgeFlags(r.read_u8()?);
            let reserved = r.read_u8()?;

            edges.push(EdgeRecord {
                target_node,
                edge_type,
                weight,
                flags: edge_flags,
                reserved,
            });
        }

        // Edge CSR index
        let mut edge_index = Vec::with_capacity(node_count + 1);
        for _ in 0..=node_count {
            edge_index.push(r.read_u32::<LittleEndian>()?);
        }

        // ─── Feature Matrix ──────────────────────────────
        let mut features = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let mut feat = [0.0f32; FEATURE_DIM];
            for f in &mut feat {
                *f = r.read_f32::<LittleEndian>()?;
            }
            features.push(feat);
        }

        // ─── Action Catalog ──────────────────────────────
        let action_count = r.read_u32::<LittleEndian>()? as usize;
        let mut actions = Vec::with_capacity(action_count);
        for _ in 0..action_count {
            let opcode_raw = r.read_u16::<LittleEndian>()?;
            let target_node = r.read_i32::<LittleEndian>()?;
            let cost_hint = r.read_u8()?;
            let risk = r.read_u8()?;

            actions.push(ActionRecord {
                opcode: OpCode::from_u16(opcode_raw),
                target_node,
                cost_hint,
                risk,
            });
        }

        // Action CSR index
        let mut action_index = Vec::with_capacity(node_count + 1);
        for _ in 0..=node_count {
            action_index.push(r.read_u32::<LittleEndian>()?);
        }

        // ─── Cluster Table ───────────────────────────────
        let mut cluster_assignments = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            cluster_assignments.push(r.read_u16::<LittleEndian>()?);
        }
        let mut cluster_centroids = Vec::with_capacity(cluster_count);
        for _ in 0..cluster_count {
            let mut centroid = [0.0f32; FEATURE_DIM];
            for f in &mut centroid {
                *f = r.read_f32::<LittleEndian>()?;
            }
            cluster_centroids.push(centroid);
        }

        // ─── URL Table ───────────────────────────────────
        let url_data_len = r.read_u32::<LittleEndian>()? as usize;
        let mut url_data = vec![0u8; url_data_len];
        std::io::Read::read_exact(&mut r, &mut url_data)?;

        let mut url_offsets = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            url_offsets.push(r.read_u32::<LittleEndian>()? as usize);
        }

        // Parse URLs from null-terminated strings
        let mut urls = Vec::with_capacity(node_count);
        for &offset in &url_offsets {
            let end = url_data[offset..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| offset + p)
                .unwrap_or(url_data_len);
            let url = String::from_utf8_lossy(&url_data[offset..end]).to_string();
            urls.push(url);
        }

        let header = MapHeader {
            magic,
            format_version,
            domain,
            mapped_at,
            node_count: node_count as u32,
            edge_count: edge_count as u32,
            cluster_count: cluster_count as u16,
            flags,
        };

        Ok(SiteMap {
            header,
            nodes,
            edges,
            edge_index,
            features,
            actions,
            action_index,
            cluster_assignments,
            cluster_centroids,
            urls,
        })
    }
}
