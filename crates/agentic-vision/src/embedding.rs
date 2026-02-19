//! CLIP embedding generation via ONNX Runtime.

use image::DynamicImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;

use crate::types::{VisionError, VisionResult};

/// Default embedding dimension for CLIP ViT-B/32.
pub const EMBEDDING_DIM: u32 = 512;

/// Default model directory.
const MODEL_DIR: &str = ".agentic-vision/models";

/// Default model filename.
const MODEL_FILENAME: &str = "clip-vit-base-patch32-visual.onnx";

/// CLIP image preprocessing constants.
const CLIP_IMAGE_SIZE: u32 = 224;
#[allow(clippy::excessive_precision)]
const CLIP_MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
#[allow(clippy::excessive_precision)]
const CLIP_STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

/// Engine for generating CLIP image embeddings.
pub struct EmbeddingEngine {
    session: Option<Session>,
}

impl EmbeddingEngine {
    /// Create a new embedding engine.
    ///
    /// If `model_path` is provided, loads the model from that path.
    /// Otherwise, looks in `~/.agentic-vision/models/`.
    /// If no model is found, the engine operates in fallback mode (zero vectors).
    pub fn new(model_path: Option<&str>) -> VisionResult<Self> {
        let path = if let Some(p) = model_path {
            std::path::PathBuf::from(p)
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            std::path::PathBuf::from(home)
                .join(MODEL_DIR)
                .join(MODEL_FILENAME)
        };

        if !path.exists() {
            tracing::warn!(
                "CLIP model not found at {}. Running in fallback mode (zero embeddings). \
                 Download a CLIP ONNX model to enable semantic similarity.",
                path.display()
            );
            return Ok(Self { session: None });
        }

        tracing::info!("Loading CLIP model from {}", path.display());

        let session = Session::builder()
            .and_then(|b| b.with_intra_threads(1))
            .and_then(|b| b.commit_from_file(&path))
            .map_err(|e| VisionError::Embedding(format!("Failed to load ONNX model: {e}")))?;

        tracing::info!("CLIP model loaded successfully");
        Ok(Self {
            session: Some(session),
        })
    }

    /// Check if the engine has a loaded model.
    pub fn has_model(&self) -> bool {
        self.session.is_some()
    }

    /// Generate an embedding for an image.
    ///
    /// Returns a 512-dimensional vector. If no model is loaded, returns zeros.
    pub fn embed(&mut self, img: &DynamicImage) -> VisionResult<Vec<f32>> {
        let session = match &mut self.session {
            Some(s) => s,
            None => {
                tracing::debug!("No model loaded, returning zero embedding");
                return Ok(vec![0.0; EMBEDDING_DIM as usize]);
            }
        };

        // Preprocess: resize to 224x224, normalize with CLIP mean/std
        let resized = img.resize_exact(
            CLIP_IMAGE_SIZE,
            CLIP_IMAGE_SIZE,
            image::imageops::FilterType::Lanczos3,
        );
        let rgb = resized.to_rgb8();

        // Create NCHW tensor [1, 3, 224, 224]
        let mut tensor =
            Array4::<f32>::zeros((1, 3, CLIP_IMAGE_SIZE as usize, CLIP_IMAGE_SIZE as usize));

        for y in 0..CLIP_IMAGE_SIZE {
            for x in 0..CLIP_IMAGE_SIZE {
                let pixel = rgb.get_pixel(x, y);
                for c in 0..3usize {
                    let val = pixel[c] as f32 / 255.0;
                    let normalized = (val - CLIP_MEAN[c]) / CLIP_STD[c];
                    tensor[[0, c, y as usize, x as usize]] = normalized;
                }
            }
        }

        let input_tensor = Tensor::from_array(tensor)
            .map_err(|e| VisionError::Embedding(format!("Failed to create input tensor: {e}")))?;

        let outputs = session
            .run(ort::inputs![input_tensor])
            .map_err(|e| VisionError::Embedding(format!("ONNX inference failed: {e}")))?;

        // Extract the embedding from the first output
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VisionError::Embedding(format!("Failed to extract output: {e}")))?;

        let embedding: Vec<f32> = data.to_vec();

        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            Ok(embedding.iter().map(|x| x / norm).collect())
        } else {
            Ok(embedding)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_mode() {
        let mut engine = EmbeddingEngine::new(Some("/nonexistent/model.onnx")).unwrap();
        assert!(!engine.has_model());

        let img = DynamicImage::new_rgb8(100, 100);
        let embedding = engine.embed(&img).unwrap();
        assert_eq!(embedding.len(), EMBEDDING_DIM as usize);
        assert!(embedding.iter().all(|&v| v == 0.0));
    }
}
