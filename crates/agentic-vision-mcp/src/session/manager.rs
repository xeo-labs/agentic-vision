//! Visual memory session lifecycle, file I/O, and session tracking.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use image::GenericImageView;

use agentic_vision::{
    capture_from_base64, capture_from_file, compute_diff, cosine_similarity, find_similar,
    generate_thumbnail, AvisReader, AvisWriter, EmbeddingEngine, ObservationMeta,
    SimilarityMatch, VisualDiff, VisualMemoryStore, VisualObservation, EMBEDDING_DIM,
};

use crate::types::{McpError, McpResult};

const DEFAULT_AUTO_SAVE_SECS: u64 = 30;

/// Manages the visual memory lifecycle, file I/O, and session state.
pub struct VisionSessionManager {
    store: VisualMemoryStore,
    engine: EmbeddingEngine,
    file_path: PathBuf,
    current_session: u32,
    dirty: bool,
    last_save: Instant,
    auto_save_interval: Duration,
}

impl VisionSessionManager {
    /// Open or create a vision file at the given path.
    pub fn open(path: &str, model_path: Option<&str>) -> McpResult<Self> {
        let file_path = PathBuf::from(path);

        let store = if file_path.exists() {
            tracing::info!("Opening existing vision file: {}", file_path.display());
            AvisReader::read_from_file(&file_path)
                .map_err(|e| McpError::VisionError(format!("Failed to read vision file: {e}")))?
        } else {
            tracing::info!("Creating new vision file: {}", file_path.display());
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    McpError::Io(std::io::Error::other(format!(
                        "Failed to create directory {}: {e}",
                        parent.display()
                    )))
                })?;
            }
            VisualMemoryStore::new(EMBEDDING_DIM)
        };

        let current_session = store.session_count + 1;

        let engine = EmbeddingEngine::new(model_path)
            .map_err(|e| McpError::VisionError(format!("Failed to initialize embedding engine: {e}")))?;

        tracing::info!(
            "Session {} started. Store has {} observations. Embedding model: {}",
            current_session,
            store.count(),
            if engine.has_model() { "loaded" } else { "fallback" }
        );

        Ok(Self {
            store,
            engine,
            file_path,
            current_session,
            dirty: false,
            last_save: Instant::now(),
            auto_save_interval: Duration::from_secs(DEFAULT_AUTO_SAVE_SECS),
        })
    }

    /// Get the visual memory store.
    pub fn store(&self) -> &VisualMemoryStore {
        &self.store
    }

    /// Current session ID.
    pub fn current_session_id(&self) -> u32 {
        self.current_session
    }

    /// Start a new session.
    pub fn start_session(&mut self, explicit_id: Option<u32>) -> McpResult<u32> {
        let session_id = explicit_id.unwrap_or(self.current_session + 1);
        self.current_session = session_id;
        self.store.session_count = self.store.session_count.max(session_id);
        tracing::info!("Started session {session_id}");
        Ok(session_id)
    }

    /// End the current session.
    pub fn end_session(&mut self) -> McpResult<u32> {
        let session_id = self.current_session;
        self.save()?;
        tracing::info!("Ended session {session_id}");
        Ok(session_id)
    }

    /// Capture an image from a source.
    pub fn capture(
        &mut self,
        source_type: &str,
        source_data: &str,
        mime: Option<&str>,
        labels: Vec<String>,
        description: Option<String>,
        _extract_ocr: bool,
    ) -> McpResult<CaptureResult> {
        let (img, source) = match source_type {
            "file" => capture_from_file(source_data)
                .map_err(|e| McpError::VisionError(format!("Failed to capture from file: {e}")))?,
            "base64" => {
                let m = mime.unwrap_or("image/png");
                capture_from_base64(source_data, m)
                    .map_err(|e| McpError::VisionError(format!("Failed to decode base64: {e}")))?
            }
            _ => {
                return Err(McpError::InvalidParams(format!(
                    "Unsupported source type: {source_type}. Use 'file' or 'base64'."
                )));
            }
        };

        let (orig_w, orig_h) = img.dimensions();
        let thumbnail = generate_thumbnail(&img);
        let thumb_img = image::load_from_memory(&thumbnail)
            .map_err(|e| McpError::VisionError(format!("Failed to load thumbnail: {e}")))?;
        let (thumb_w, thumb_h) = thumb_img.dimensions();

        let embedding = self
            .engine
            .embed(&img)
            .map_err(|e| McpError::VisionError(format!("Embedding failed: {e}")))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let obs = VisualObservation {
            id: 0, // assigned by store
            timestamp: now,
            session_id: self.current_session,
            source,
            embedding,
            thumbnail,
            metadata: ObservationMeta {
                width: thumb_w,
                height: thumb_h,
                original_width: orig_w,
                original_height: orig_h,
                labels,
                description,
            },
            memory_link: None,
        };

        let id = self.store.add(obs);
        self.dirty = true;
        self.maybe_auto_save()?;

        Ok(CaptureResult {
            capture_id: id,
            timestamp: now,
            width: orig_w,
            height: orig_h,
            embedding_dims: EMBEDDING_DIM,
        })
    }

    /// Compare two captures by cosine similarity.
    pub fn compare(&self, id_a: u64, id_b: u64) -> McpResult<f32> {
        let a = self
            .store
            .get(id_a)
            .ok_or(McpError::CaptureNotFound(id_a))?;
        let b = self
            .store
            .get(id_b)
            .ok_or(McpError::CaptureNotFound(id_b))?;

        Ok(cosine_similarity(&a.embedding, &b.embedding))
    }

    /// Find similar captures.
    pub fn find_similar(
        &self,
        capture_id: u64,
        top_k: usize,
        min_similarity: f32,
    ) -> McpResult<Vec<SimilarityMatch>> {
        let obs = self
            .store
            .get(capture_id)
            .ok_or(McpError::CaptureNotFound(capture_id))?;

        let mut matches = find_similar(&obs.embedding, &self.store.observations, top_k + 1, min_similarity);
        // Remove self from results
        matches.retain(|m| m.id != capture_id);
        matches.truncate(top_k);
        Ok(matches)
    }

    /// Find similar by raw embedding.
    pub fn find_similar_by_embedding(
        &self,
        embedding: &[f32],
        top_k: usize,
        min_similarity: f32,
    ) -> Vec<SimilarityMatch> {
        find_similar(embedding, &self.store.observations, top_k, min_similarity)
    }

    /// Compute visual diff between two captures.
    pub fn diff(&self, id_a: u64, id_b: u64) -> McpResult<VisualDiff> {
        let a = self
            .store
            .get(id_a)
            .ok_or(McpError::CaptureNotFound(id_a))?;
        let b = self
            .store
            .get(id_b)
            .ok_or(McpError::CaptureNotFound(id_b))?;

        let img_a = image::load_from_memory(&a.thumbnail)
            .map_err(|e| McpError::VisionError(format!("Failed to load thumbnail A: {e}")))?;
        let img_b = image::load_from_memory(&b.thumbnail)
            .map_err(|e| McpError::VisionError(format!("Failed to load thumbnail B: {e}")))?;

        compute_diff(id_a, id_b, &img_a, &img_b)
            .map_err(|e| McpError::VisionError(format!("Diff failed: {e}")))
    }

    /// Link a capture to a memory node.
    pub fn link(&mut self, capture_id: u64, memory_node_id: u64) -> McpResult<()> {
        let obs = self
            .store
            .get_mut(capture_id)
            .ok_or(McpError::CaptureNotFound(capture_id))?;
        obs.memory_link = Some(memory_node_id);
        self.dirty = true;
        Ok(())
    }

    /// Save to file.
    pub fn save(&mut self) -> McpResult<()> {
        if !self.dirty {
            return Ok(());
        }

        AvisWriter::write_to_file(&self.store, &self.file_path)
            .map_err(|e| McpError::VisionError(format!("Failed to write vision file: {e}")))?;

        self.dirty = false;
        self.last_save = Instant::now();
        tracing::debug!("Saved vision file: {}", self.file_path.display());
        Ok(())
    }

    fn maybe_auto_save(&mut self) -> McpResult<()> {
        if self.dirty && self.last_save.elapsed() >= self.auto_save_interval {
            self.save()?;
        }
        Ok(())
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

impl Drop for VisionSessionManager {
    fn drop(&mut self) {
        if self.dirty {
            if let Err(e) = self.save() {
                tracing::error!("Failed to save on drop: {e}");
            }
        }
    }
}

/// Result of a capture operation.
pub struct CaptureResult {
    pub capture_id: u64,
    pub timestamp: u64,
    pub width: u32,
    pub height: u32,
    pub embedding_dims: u32,
}
