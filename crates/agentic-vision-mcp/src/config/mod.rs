//! Configuration loading and resolution.

use std::path::PathBuf;

/// Resolve the vision file path.
pub fn resolve_vision_path(explicit: Option<&str>) -> String {
    if let Some(path) = explicit {
        return path.to_string();
    }

    if let Ok(env_path) = std::env::var("AVIS_FILE") {
        return env_path;
    }

    let cwd_vision = PathBuf::from(".avis/vision.avis");
    if cwd_vision.exists() {
        return cwd_vision.display().to_string();
    }

    resolve_default_vision_path()
}

fn resolve_default_vision_path() -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    format!("{home}/.agentic-vision/vision.avis")
}
