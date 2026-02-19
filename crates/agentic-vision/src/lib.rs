//! AgenticVision — core vision library for image capture, embedding, similarity, and visual memory.

pub mod capture;
pub mod diff;
pub mod embedding;
pub mod similarity;
pub mod storage;
pub mod types;

pub use capture::{capture_from_base64, capture_from_file, generate_thumbnail};
pub use diff::compute_diff;
pub use embedding::{EmbeddingEngine, EMBEDDING_DIM};
pub use similarity::{cosine_similarity, find_similar};
pub use storage::{AvisReader, AvisWriter};
pub use types::*;
