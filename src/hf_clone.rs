//! HuggingFace repo inspection and base-model cloning for onde-cli.
//!
//! Provides helpers to:
//!   - Check whether a HuggingFace repo exists and if it contains model files
//!   - Fetch the list of recommended base models for fine-tuning
//!   - Clone (download) a base model's safetensors files into the local HF cache

use anyhow::{Context, Result};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of inspecting a HuggingFace repository.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RepoStatus {
    /// Repo does not exist on HuggingFace (404).
    NotFound,
    /// Repo exists but contains no model files (safetensors/GGUF).
    Empty { repo_id: String, files: Vec<String> },
    /// Repo exists and has model files ready for use.
    HasModel {
        repo_id: String,
        files: Vec<String>,
        /// Approximate total size in bytes of all model files.
        model_size_bytes: u64,
    },
}

/// A base model that can be used as a starting point for fine-tuning.
#[derive(Debug, Clone)]
pub struct BaseModel {
    /// HuggingFace repo ID, e.g. `Qwen/Qwen3-0.6B`
    pub repo_id: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Approximate size of the safetensors model files
    pub size_display: &'static str,
    /// Parameter count description
    pub params: &'static str,
    /// Short description of recommended use case
    pub description: &'static str,
}

/// Progress updates emitted while checking or cloning a repo.
#[derive(Debug, Clone)]
pub enum CloneProgress {
    /// Checking if the target repo exists on HuggingFace.
    CheckingRepo,
    /// Repo status has been determined.
    RepoChecked(RepoStatus),
    /// Creating the target repo on HuggingFace.
    CreatingRepo,
    /// Repo was created (or already existed).
    RepoReady,
    /// Failed with an error message.
    Failed(String),
}

// ---------------------------------------------------------------------------
// Base model catalog
// ---------------------------------------------------------------------------

/// Recommended base models for LoRA fine-tuning.
///
/// These are full-precision safetensors models (not GGUF) since fine-tuning
/// requires autograd-capable weights. Ordered by size (smallest first).
pub const BASE_MODELS: &[BaseModel] = &[
    BaseModel {
        repo_id: "Qwen/Qwen3-0.6B",
        display_name: "Qwen3 0.6B",
        size_display: "~1.2 GB",
        params: "751M",
        description: "Smallest Qwen3 — ideal for mobile & quick experiments",
    },
    BaseModel {
        repo_id: "Qwen/Qwen2.5-1.5B-Instruct",
        display_name: "Qwen2.5 1.5B Instruct",
        size_display: "~3.0 GB",
        params: "1.5B",
        description: "Balanced size — good for instruction-following tasks",
    },
    BaseModel {
        repo_id: "Qwen/Qwen3-1.7B",
        display_name: "Qwen3 1.7B",
        size_display: "~3.4 GB",
        params: "1.7B",
        description: "Latest Qwen3 small model — strong reasoning",
    },
    BaseModel {
        repo_id: "Qwen/Qwen3-4B",
        display_name: "Qwen3 4B",
        size_display: "~8.0 GB",
        params: "4B",
        description: "Larger Qwen3 — best quality, macOS recommended",
    },
];

// ---------------------------------------------------------------------------
// HF API response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct HfRepoInfo {
    #[serde(default)]
    siblings: Vec<HfFileSibling>,
}

#[derive(Debug, Deserialize)]
struct HfFileSibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check whether a HuggingFace repo exists and inspect its contents.
///
/// Returns [`RepoStatus::NotFound`] if the repo doesn't exist (HTTP 404),
/// [`RepoStatus::Empty`] if it exists but has no model files, or
/// [`RepoStatus::HasModel`] if it contains safetensors or GGUF files.
pub async fn check_repo(repo_id: &str, hf_token: &str) -> Result<RepoStatus> {
    let client = build_client()?;

    let mut request = client.get(format!("https://huggingface.co/api/models/{repo_id}"));
    if !hf_token.is_empty() {
        request = request.bearer_auth(hf_token);
    }

    let response = request
        .send()
        .await
        .context("Failed to reach HuggingFace API")?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(RepoStatus::NotFound);
    }

    let response = response
        .error_for_status()
        .context("HuggingFace API error")?;

    let info: HfRepoInfo = response
        .json()
        .await
        .context("Failed to parse repo metadata")?;

    let all_files: Vec<String> = info.siblings.iter().map(|s| s.rfilename.clone()).collect();

    // Check for model files: safetensors or GGUF
    let model_files: Vec<&HfFileSibling> = info
        .siblings
        .iter()
        .filter(|s| is_model_file(&s.rfilename))
        .collect();

    if model_files.is_empty() {
        Ok(RepoStatus::Empty {
            repo_id: repo_id.to_string(),
            files: all_files,
        })
    } else {
        let total_size: u64 = model_files.iter().filter_map(|s| s.size).sum();
        Ok(RepoStatus::HasModel {
            repo_id: repo_id.to_string(),
            files: all_files,
            model_size_bytes: total_size,
        })
    }
}

/// Create a HuggingFace repo if it doesn't already exist.
///
/// Returns `Ok(())` on success or if the repo already exists (HTTP 409).
pub async fn create_repo(repo_id: &str, hf_token: &str) -> Result<()> {
    let client = build_client()?;

    let body = if let Some((org, name)) = repo_id.split_once('/') {
        serde_json::json!({
            "type": "model",
            "name": name,
            "organization": org,
            "private": false,
            "sdk": "onde-cli"
        })
    } else {
        serde_json::json!({
            "type": "model",
            "name": repo_id,
            "private": false,
            "sdk": "onde-cli"
        })
    };

    let response = client
        .post("https://huggingface.co/api/repos/create")
        .bearer_auth(hf_token)
        .json(&body)
        .send()
        .await
        .context("Failed to reach HuggingFace API")?;

    let status = response.status();

    // 200 = created, 409 = already exists — both are fine.
    if !status.is_success() && status.as_u16() != 409 {
        let body_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to create repo {repo_id}: {status} {body_text}");
    }

    Ok(())
}

/// Spawn-friendly entry point that checks a repo and sends progress via channel.
pub async fn start_check_repo(
    repo_id: String,
    hf_token: String,
    tx: tokio::sync::mpsc::UnboundedSender<CloneProgress>,
) {
    let _ = tx.send(CloneProgress::CheckingRepo);

    match check_repo(&repo_id, &hf_token).await {
        Ok(status) => {
            let _ = tx.send(CloneProgress::RepoChecked(status));
        }
        Err(e) => {
            let _ = tx.send(CloneProgress::Failed(format!("{e:#}")));
        }
    }
}

/// Create the repo (if needed) and report progress.
pub async fn start_create_repo(
    repo_id: String,
    hf_token: String,
    tx: tokio::sync::mpsc::UnboundedSender<CloneProgress>,
) {
    let _ = tx.send(CloneProgress::CreatingRepo);

    match create_repo(&repo_id, &hf_token).await {
        Ok(()) => {
            let _ = tx.send(CloneProgress::RepoReady);
        }
        Err(e) => {
            let _ = tx.send(CloneProgress::Failed(format!("{e:#}")));
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("onde-cli/0.2")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build HTTP client")
}

/// Check if a filename looks like a model weight file.
fn is_model_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".safetensors")
        || lower.ends_with(".gguf")
        || lower.ends_with(".bin")
        || lower.ends_with(".pt")
        || lower.ends_with(".pth")
}
