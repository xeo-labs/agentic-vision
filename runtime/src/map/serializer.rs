//! Serialize a SiteMap to the binary CTX format.
//!
//! The format ends with a 4-byte CRC32 checksum (IEEE) of all preceding bytes,
//! allowing integrity verification on load.

use crate::map::types::*;
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::Write;

/// Compute CRC32 (IEEE/ISO 3309) checksum of data.
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC32_TABLE[index] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// CRC32 lookup table (IEEE polynomial 0xEDB88320).
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = 0xEDB8_8320 ^ (crc >> 1);
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

impl SiteMap {
    /// Serialize the SiteMap to binary CTX format with trailing CRC32 checksum.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write_to(&mut buf).expect("serialization to Vec should not fail");

        // Append CRC32 checksum of all preceding bytes
        let checksum = crc32(&buf);
        buf.write_u32::<LittleEndian>(checksum)
            .expect("checksum write to Vec should not fail");

        buf
    }

    fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        // ─── Header ───────────────────────────────────────
        w.write_u32::<LittleEndian>(self.header.magic)?;
        w.write_u16::<LittleEndian>(self.header.format_version)?;

        let domain_bytes = self.header.domain.as_bytes();
        w.write_u16::<LittleEndian>(domain_bytes.len() as u16)?;
        w.write_all(domain_bytes)?;

        w.write_u64::<LittleEndian>(self.header.mapped_at)?;
        w.write_u32::<LittleEndian>(self.header.node_count)?;
        w.write_u32::<LittleEndian>(self.header.edge_count)?;
        w.write_u16::<LittleEndian>(self.header.cluster_count)?;
        w.write_u16::<LittleEndian>(self.header.flags)?;

        // ─── Node Table ───────────────────────────────────
        for node in &self.nodes {
            w.write_u8(node.page_type as u8)?;
            w.write_u8(node.confidence)?;
            w.write_u8(node.freshness)?;
            w.write_u8(node.flags.0)?;
            w.write_u32::<LittleEndian>(node.content_hash)?;
            w.write_u32::<LittleEndian>(node.rendered_at)?;
            w.write_u16::<LittleEndian>(node.http_status)?;
            w.write_u16::<LittleEndian>(node.depth)?;
            w.write_u16::<LittleEndian>(node.inbound_count)?;
            w.write_u16::<LittleEndian>(node.outbound_count)?;
            w.write_f32::<LittleEndian>(node.feature_norm)?;
            w.write_u32::<LittleEndian>(node.reserved)?;
        }

        // ─── Edge Table ───────────────────────────────────
        for edge in &self.edges {
            w.write_u32::<LittleEndian>(edge.target_node)?;
            w.write_u8(edge.edge_type as u8)?;
            w.write_u8(edge.weight)?;
            w.write_u8(edge.flags.0)?;
            w.write_u8(edge.reserved)?;
        }

        // Edge CSR index
        for &idx in &self.edge_index {
            w.write_u32::<LittleEndian>(idx)?;
        }

        // ─── Feature Matrix ──────────────────────────────
        for feat_vec in &self.features {
            for &f in feat_vec {
                w.write_f32::<LittleEndian>(f)?;
            }
        }

        // ─── Action Catalog ──────────────────────────────
        // Action count
        w.write_u32::<LittleEndian>(self.actions.len() as u32)?;

        for action in &self.actions {
            w.write_u16::<LittleEndian>(action.opcode.as_u16())?;
            w.write_i32::<LittleEndian>(action.target_node)?;
            w.write_u8(action.cost_hint)?;
            w.write_u8(action.risk)?;
        }

        // Action CSR index
        for &idx in &self.action_index {
            w.write_u32::<LittleEndian>(idx)?;
        }

        // ─── Cluster Table ───────────────────────────────
        for &assignment in &self.cluster_assignments {
            w.write_u16::<LittleEndian>(assignment)?;
        }
        for centroid in &self.cluster_centroids {
            for &f in centroid {
                w.write_f32::<LittleEndian>(f)?;
            }
        }

        // ─── URL Table ───────────────────────────────────
        // First write all URL bytes concatenated with null terminators
        let mut url_data = Vec::new();
        let mut url_offsets = Vec::new();
        for url in &self.urls {
            url_offsets.push(url_data.len() as u32);
            url_data.extend_from_slice(url.as_bytes());
            url_data.push(0); // null terminator
        }

        // URL data length
        w.write_u32::<LittleEndian>(url_data.len() as u32)?;
        w.write_all(&url_data)?;

        // URL offsets
        for &offset in &url_offsets {
            w.write_u32::<LittleEndian>(offset)?;
        }

        Ok(())
    }
}
