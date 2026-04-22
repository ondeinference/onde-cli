//! Local HuggingFace model cache scanning and catalog merging.
//!
//! Checks two locations and merges the results:
//!   1. Apple App Group container — `~/Library/Group Containers/
//!      group.com.ondeinference.apps/models/hub` (macOS only)
//!   2. Standard HF cache — respects `HF_HUB_CACHE`, `HUGGINGFACE_HUB_CACHE`,
//!      `HF_HOME`, then falls back to `~/.cache/huggingface/hub`
//!
//! Models found in both locations are reported once, with `AppGroup` winning.
//! `merge_models` joins the remote Onde Inference catalog with local scan results
//! so the TUI can show all supported models with their download status.

use smbcloud_gresiq_sdk::OndeModel;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A model directory found in a local HuggingFace cache.
#[derive(Debug, Clone)]
pub struct LocalModel {
    /// Full `org/name` identifier, e.g. `bartowski/Qwen2.5-1.5B-Instruct-GGUF`
    pub model_id: String,
    /// Short name segment shown in the TUI list
    pub display_name: String,
    /// Human-readable on-disk size (e.g. `941 MB`)
    pub size_display: String,
    /// Which cache this model came from
    pub source: CacheSource,
}

/// A model from the Onde Inference catalog, enriched with local download status.
#[derive(Debug, Clone)]
pub struct MergedModel {
    /// GresIQ internal model ID (used for assignment). `None` for local-only models.
    pub catalog_id: Option<String>,
    /// HuggingFace repo ID, e.g. `bartowski/Qwen2.5-1.5B-Instruct-GGUF`
    pub model_id: String,
    /// Human-readable name (from catalog when available, parsed from repo ID otherwise)
    pub display_name: String,
    /// On-disk size if downloaded, expected size from catalog otherwise
    pub size_display: String,
    /// Whether the model is present in a local HF cache
    pub downloaded: bool,
    /// Which cache it was found in (`None` if not downloaded)
    pub source: Option<CacheSource>,
    /// Full catalog entry — `None` for local-only models not in the catalog
    pub catalog_model: Option<OndeModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CacheSource {
    /// Apple App Group — shared across all Onde Inference apps on this Mac
    AppGroup,
    /// Standard HuggingFace home cache (`~/.cache/huggingface/hub` or `$HF_HOME`)
    HfCache,
}

impl CacheSource {
    pub fn label(&self) -> &'static str {
        match self {
            CacheSource::AppGroup => "App Group",
            CacheSource::HfCache => "HF Cache",
        }
    }
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Resolve `~/Library/Group Containers/group.com.ondeinference.apps/models/hub`
/// on macOS. Returns `None` on non-macOS or if the directory does not exist.
#[cfg(target_os = "macos")]
fn app_group_hub() -> Option<PathBuf> {
    let p = dirs::home_dir()?
        .join("Library")
        .join("Group Containers")
        .join("group.com.ondeinference.apps")
        .join("models")
        .join("hub");
    p.is_dir().then_some(p)
}

#[cfg(not(target_os = "macos"))]
fn app_group_hub() -> Option<PathBuf> {
    None
}

/// Resolve the standard HuggingFace hub cache directory.
///
/// Priority order (mirrors `hf-hub` and `onde` crate behaviour):
///   1. `HF_HUB_CACHE` / `HUGGINGFACE_HUB_CACHE` env vars
///   2. `$HF_HOME/hub`
///   3. `~/.cache/huggingface/hub`
fn hf_home_hub() -> Option<PathBuf> {
    for var in ["HF_HUB_CACHE", "HUGGINGFACE_HUB_CACHE"] {
        if let Ok(val) = std::env::var(var) {
            let p = PathBuf::from(val);
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    if let Ok(hf_home) = std::env::var("HF_HOME") {
        let p = PathBuf::from(hf_home).join("hub");
        if p.is_dir() {
            return Some(p);
        }
    }

    dirs::home_dir()
        .map(|h| h.join(".cache").join("huggingface").join("hub"))
        .filter(|p| p.is_dir())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn dir_size(path: &PathBuf) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                total += p.metadata().map(|m| m.len()).unwrap_or(0);
            } else if p.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Scan a single hub cache directory and return all `models--*` entries.
fn scan_dir(dir: &PathBuf, source: CacheSource) -> Vec<LocalModel> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut models = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };

        // HF hub stores models as "models--{org}--{name}"
        if !dir_name.starts_with("models--") {
            continue;
        }

        let remainder = &dir_name["models--".len()..];
        let model_id = remainder.replace("--", "/");
        let display_name = model_id.split('/').last().unwrap_or(&model_id).to_string();

        let size_bytes = dir_size(&path);

        models.push(LocalModel {
            model_id,
            display_name,
            size_display: format_size(size_bytes),
            source: source.clone(),
        });
    }

    models
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Merge the remote Onde Inference model catalog with local HF cache scan results.
///
/// - Every catalog model is included; `downloaded` is set based on the local scan.
/// - Local models not present in the catalog are appended with `catalog_id: None`.
/// - The merge key is `local.model_id == catalog.hf_repo_id`.
/// - Sort order: downloaded first, then alphabetically by display name.
pub fn merge_models(catalog: &[OndeModel], local: Vec<LocalModel>) -> Vec<MergedModel> {
    // Index local models by their HF repo ID for O(1) lookup.
    let local_by_id: HashMap<&str, &LocalModel> =
        local.iter().map(|m| (m.model_id.as_str(), m)).collect();

    let mut merged = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Walk the catalog first so every supported model appears in the list.
    for cm in catalog {
        let hf_id = cm.hf_repo_id.as_deref().unwrap_or(cm.id.as_str());
        seen.insert(hf_id.to_string());

        let local_match = local_by_id.get(hf_id);
        let downloaded = local_match.is_some();
        let source = local_match.map(|lm| lm.source.clone());

        let size_display = if let Some(lm) = local_match {
            lm.size_display.clone()
        } else if let Some(bytes) = cm.approx_size_bytes {
            format_size(bytes as u64)
        } else {
            "–".to_string()
        };

        let display_name = cm
            .name
            .as_deref()
            .unwrap_or_else(|| hf_id.split('/').last().unwrap_or(hf_id))
            .to_string();

        merged.push(MergedModel {
            catalog_id: Some(cm.id.clone()),
            model_id: hf_id.to_string(),
            display_name,
            size_display,
            downloaded,
            source,
            catalog_model: Some(cm.clone()),
        });
    }

    // Append any locally downloaded models that are not in the catalog.
    for lm in &local {
        if !seen.contains(lm.model_id.as_str()) {
            merged.push(MergedModel {
                catalog_id: None,
                model_id: lm.model_id.clone(),
                display_name: lm.display_name.clone(),
                size_display: lm.size_display.clone(),
                downloaded: true,
                source: Some(lm.source.clone()),
                catalog_model: None,
            });
        }
    }

    // Downloaded models first, then alphabetically.
    merged.sort_by(|a, b| {
        b.downloaded.cmp(&a.downloaded).then(
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase()),
        )
    });

    merged
}

/// Scan all local HuggingFace caches and return every downloaded model.
///
/// The App Group container is checked first. If a model appears in both
/// locations it is reported once with [`CacheSource::AppGroup`]. Results are
/// sorted by `model_id` for a stable, predictable order.
///
/// This function is synchronous and does filesystem I/O. Call it from a
/// `tokio::task::spawn_blocking` context to avoid blocking the async runtime.
pub fn list_local_models() -> Vec<LocalModel> {
    let mut models = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1. Apple App Group cache (shared across all Onde Inference apps)
    if let Some(path) = app_group_hub() {
        for m in scan_dir(&path, CacheSource::AppGroup) {
            seen.insert(m.model_id.clone());
            models.push(m);
        }
    }

    // 2. Standard HF cache — skip anything already found in the App Group
    if let Some(path) = hf_home_hub() {
        for m in scan_dir(&path, CacheSource::HfCache) {
            if !seen.contains(&m.model_id) {
                models.push(m);
            }
        }
    }

    models.sort_by_key(|m| m.model_id.to_lowercase());
    models
}

/// Returns the preferred hub directory for downloading new models.
///
/// On macOS, prefers the App Group container so downloads are shared with
/// the Onde Inference apps. Falls back to the standard HF home cache, then
/// `~/.cache/huggingface/hub` as a last resort.
pub fn preferred_download_hub() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(p) = app_group_hub() {
        return p;
    }

    if let Some(p) = hf_home_hub() {
        return p;
    }

    dirs::home_dir()
        .map(|h| h.join(".cache").join("huggingface").join("hub"))
        .unwrap_or_else(|| std::path::PathBuf::from(".cache/huggingface/hub"))
}
