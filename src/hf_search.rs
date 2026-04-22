//! HuggingFace Hub model search and download for onde-cli.
//!
//! `search_hf` queries the public HF API (no auth required for public models).
//! `download_model` streams all files into the standard HF hub cache layout:
//!   `{hub}/models--{org}--{name}/snapshots/{sha}/{filename}`

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Deserialize;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Compact model info returned by the HF search API.
#[derive(Debug, Clone, Deserialize)]
pub struct HfModelInfo {
    #[serde(rename = "id")]
    pub model_id: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Commit SHA of the latest revision.
    pub sha: Option<String>,
    /// Files present in the repository.
    #[serde(default)]
    pub siblings: Vec<HfSibling>,
}

/// A file entry in a HuggingFace model repository.
#[derive(Debug, Clone, Deserialize)]
pub struct HfSibling {
    pub rfilename: String,
}

/// Progress update for an in-progress model download.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub model_id: String,
    pub filename: String,
    pub file_index: usize,
    pub total_files: usize,
    pub file_bytes_done: u64,
    pub file_bytes_total: u64,
}

/// Events emitted by the download background task.
pub enum DownloadEvent {
    Progress(DownloadProgress),
    Complete,
    Failed(String),
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Search the HuggingFace Hub for models matching `query` (up to 20 results).
pub async fn search_hf(query: &str) -> Result<Vec<HfModelInfo>> {
    let client = build_client()?;
    let results: Vec<HfModelInfo> = client
        .get("https://huggingface.co/api/models")
        .query(&[("search", query), ("limit", "20"), ("full", "false")])
        .send()
        .await
        .context("HuggingFace API unreachable")?
        .error_for_status()
        .context("HuggingFace API returned an error")?
        .json()
        .await
        .context("Failed to parse HuggingFace API response")?;
    Ok(results)
}

/// Download all files of a HuggingFace model into `hub_dir` using the standard
/// HF hub cache layout.  Progress and completion are reported via `tx`.
///
/// This function is infallible — all errors are sent as `DownloadEvent::Failed`.
pub async fn download_model(
    model_id: String,
    hub_dir: PathBuf,
    tx: tokio::sync::mpsc::UnboundedSender<DownloadEvent>,
) {
    match run_download(model_id, hub_dir, &tx).await {
        Ok(()) => {
            let _ = tx.send(DownloadEvent::Complete);
        }
        Err(e) => {
            let _ = tx.send(DownloadEvent::Failed(format!("{e:#}")));
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("onde-cli/0.1")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build HTTP client")
}

/// Fetch full model metadata (commit SHA + file list).
async fn fetch_model_info(client: &reqwest::Client, model_id: &str) -> Result<HfModelInfo> {
    let url = format!("https://huggingface.co/api/models/{model_id}");
    let info: HfModelInfo = client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch model metadata")?
        .error_for_status()
        .context("Model not found or inaccessible")?
        .json()
        .await
        .context("Failed to parse model metadata")?;
    Ok(info)
}

async fn run_download(
    model_id: String,
    hub_dir: PathBuf,
    tx: &tokio::sync::mpsc::UnboundedSender<DownloadEvent>,
) -> Result<()> {
    let client = build_client()?;

    // Fetch metadata to get the commit SHA and file list.
    let info = fetch_model_info(&client, &model_id).await?;
    let sha = info.sha.as_deref().unwrap_or("main");

    // Build snapshot directory: {hub}/models--{org}--{name}/snapshots/{sha}/
    let dir_name = format!("models--{}", model_id.replace('/', "--"));
    let snapshot_dir = hub_dir.join(&dir_name).join("snapshots").join(sha);
    tokio::fs::create_dir_all(&snapshot_dir)
        .await
        .context("Failed to create snapshot directory")?;

    // Write refs/main so the standard HF cache tools can find the revision.
    let refs_dir = hub_dir.join(&dir_name).join("refs");
    tokio::fs::create_dir_all(&refs_dir).await?;
    tokio::fs::write(refs_dir.join("main"), sha).await?;

    // Download each file.
    let files: Vec<String> = info.siblings.iter().map(|s| s.rfilename.clone()).collect();
    let total_files = files.len();

    for (file_index, filename) in files.iter().enumerate() {
        let dest = snapshot_dir.join(filename);

        // Create parent dirs for nested paths (e.g. "subfolder/file.json").
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Skip files that are already fully present.
        if dest.exists() {
            // Still send a progress event so the UI reflects the file count.
            let _ = tx.send(DownloadEvent::Progress(DownloadProgress {
                model_id: model_id.clone(),
                filename: filename.clone(),
                file_index,
                total_files,
                file_bytes_done: 0,
                file_bytes_total: 0,
            }));
            continue;
        }

        let url = format!("https://huggingface.co/{model_id}/resolve/{sha}/{filename}");
        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to download {filename}"))?
            .error_for_status()
            .with_context(|| format!("Server error downloading {filename}"))?;

        let total_bytes = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut last_reported: u64 = 0;

        // Write to a temp file first, then rename on success.
        let tmp_dest = snapshot_dir.join(format!("{filename}.tmp"));
        let mut file = tokio::fs::File::create(&tmp_dest)
            .await
            .with_context(|| format!("Failed to create {}", tmp_dest.display()))?;

        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.with_context(|| format!("Download error for {filename}"))?;
            file.write_all(&chunk)
                .await
                .with_context(|| format!("Write error for {filename}"))?;
            downloaded += chunk.len() as u64;

            // Report progress roughly every 512 KB.
            if downloaded.saturating_sub(last_reported) >= 512 * 1024
                || (total_bytes > 0 && downloaded >= total_bytes)
            {
                last_reported = downloaded;
                let _ = tx.send(DownloadEvent::Progress(DownloadProgress {
                    model_id: model_id.clone(),
                    filename: filename.clone(),
                    file_index,
                    total_files,
                    file_bytes_done: downloaded,
                    file_bytes_total: total_bytes,
                }));
            }
        }

        file.flush().await?;
        drop(file);
        tokio::fs::rename(&tmp_dest, &dest)
            .await
            .with_context(|| format!("Failed to finalize {filename}"))?;
    }

    Ok(())
}
