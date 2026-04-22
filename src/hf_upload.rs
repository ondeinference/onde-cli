//! Upload fine-tuned GGUF models to a HuggingFace repository.
//!
//! Uses the HuggingFace Hub LFS upload flow:
//!   1. `POST /api/repos/create` — create the repo (409 = already exists, OK)
//!   2. `POST /api/models/{repo}/preupload/main` — check upload mode
//!   3. Compute SHA-256 of the local file
//!   4. `POST /{repo}.git/info/lfs/objects/batch` — get the S3 upload URL
//!   5. `PUT {s3_url}` — stream the file to S3
//!   6. `POST /{repo}.git/info/lfs/objects/verify` — confirm the upload
//!   7. `POST /api/models/{repo}/commit/main` — create the commit
//!
//! Progress is reported through a `tokio::sync::mpsc::UnboundedSender` so the
//! TUI can drive a progress bar without blocking.

use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Everything needed to upload a single file to HuggingFace.
pub struct UploadConfig {
    /// Path to the local `.gguf` file.
    pub file_path: PathBuf,
    /// Full repo identifier, e.g. `ondeinference/qwen2.5-0.5b-finetuned`.
    pub repo_id: String,
    /// Destination filename inside the repo, e.g. `model-finetuned-q8_0.gguf`.
    pub path_in_repo: String,
    /// HuggingFace Bearer token.
    pub hf_token: String,
    /// Commit message shown on HuggingFace.
    pub commit_message: String,
}

/// Progress updates emitted during the upload.
pub enum UploadProgress {
    /// About to create (or confirm) the HuggingFace repo.
    CreatingRepo,
    /// Computing SHA-256 hash of the local file.
    Hashing { bytes_done: u64, bytes_total: u64 },
    /// Streaming file bytes to the server.
    Uploading { bytes_sent: u64, bytes_total: u64 },
    /// Creating the commit on HuggingFace.
    Committing,
    /// Upload finished successfully.
    Done { url: String },
    /// Something went wrong.
    Failed(String),
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Spawn-friendly wrapper that never returns an error — failures are sent
/// through `tx` as [`UploadProgress::Failed`] instead.
pub async fn start_upload(
    config: UploadConfig,
    tx: tokio::sync::mpsc::UnboundedSender<UploadProgress>,
) {
    if let Err(error) = run_upload(&config, &tx).await {
        eprintln!("[upload] error: {error:#}");
        let _ = tx.send(UploadProgress::Failed(format!("{error:#}")));
    }
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

/// Size of each chunk streamed during upload (1 MiB).
const CHUNK_SIZE: usize = 1_048_576;

/// Size of each chunk read during SHA-256 hashing (4 MiB).
const HASH_CHUNK_SIZE: usize = 4 * 1_048_576;

async fn run_upload(
    config: &UploadConfig,
    tx: &tokio::sync::mpsc::UnboundedSender<UploadProgress>,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("onde-cli/0.2")
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .context("failed to build HTTP client")?;

    let file_size = std::fs::metadata(&config.file_path)
        .with_context(|| format!("failed to stat {}", config.file_path.display()))?
        .len();

    // -- Step 1: Create the repo (or confirm it already exists) ---------------

    let _ = tx.send(UploadProgress::CreatingRepo);
    create_repo(&client, &config.hf_token, &config.repo_id).await?;

    // -- Step 2: Preupload check ----------------------------------------------

    let upload_mode = preupload_check(
        &client,
        &config.hf_token,
        &config.repo_id,
        &config.path_in_repo,
        file_size,
    )
    .await?;

    eprintln!("[upload] upload mode: {upload_mode}");

    // -- Step 3: Compute SHA-256 of the file ----------------------------------

    let _ = tx.send(UploadProgress::Hashing {
        bytes_done: 0,
        bytes_total: file_size,
    });

    let sha256 = compute_sha256(&config.file_path, file_size, tx).await?;
    eprintln!("[upload] sha256: {sha256}");

    if upload_mode == "lfs" {
        // -- Step 4: LFS batch — get the S3 upload URL ------------------------

        let lfs_info = lfs_batch(
            &client,
            &config.hf_token,
            &config.repo_id,
            &sha256,
            file_size,
        )
        .await?;

        match lfs_info {
            LfsBatchResult::AlreadyExists => {
                eprintln!("[upload] LFS object already exists, skipping upload");
            }
            LfsBatchResult::NeedsUpload {
                upload_url,
                verify_url,
                verify_token,
            } => {
                // -- Step 5: PUT to S3 ----------------------------------------

                let _ = tx.send(UploadProgress::Uploading {
                    bytes_sent: 0,
                    bytes_total: file_size,
                });

                upload_to_s3(&client, &upload_url, &config.file_path, file_size, tx).await?;

                // -- Step 6: LFS verify ---------------------------------------

                lfs_verify(&client, &verify_url, &verify_token, &sha256, file_size).await?;
            }
        }

        // -- Step 7: Commit with LFS pointer ----------------------------------

        let _ = tx.send(UploadProgress::Committing);

        lfs_commit(
            &client,
            &config.hf_token,
            &config.repo_id,
            &config.path_in_repo,
            &config.commit_message,
            &sha256,
            file_size,
        )
        .await?;
    } else {
        // Regular (non-LFS) commit for small files.
        let _ = tx.send(UploadProgress::Uploading {
            bytes_sent: 0,
            bytes_total: file_size,
        });

        regular_commit(
            &client,
            &config.hf_token,
            &config.repo_id,
            &config.path_in_repo,
            &config.commit_message,
            &config.file_path,
            file_size,
            tx,
        )
        .await?;
    }

    let model_url = format!("https://huggingface.co/{}", config.repo_id);
    let _ = tx.send(UploadProgress::Done { url: model_url });

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 1: Create repo
// ---------------------------------------------------------------------------

async fn create_repo(client: &reqwest::Client, token: &str, repo_id: &str) -> Result<()> {
    // The HF API expects "name" (repo only) and "organization" as separate fields.
    let create_body = if let Some((org, name)) = repo_id.split_once('/') {
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
        .bearer_auth(token)
        .json(&create_body)
        .send()
        .await
        .context("failed to reach HuggingFace API")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    eprintln!("[upload] create repo: {status} {body}");

    // 200 = created, 409 = already exists — both are fine.
    if !status.is_success() && status.as_u16() != 409 {
        anyhow::bail!("failed to create repo {repo_id}: {status} {body}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 2: Preupload check
// ---------------------------------------------------------------------------

async fn preupload_check(
    client: &reqwest::Client,
    token: &str,
    repo_id: &str,
    path_in_repo: &str,
    file_size: u64,
) -> Result<String> {
    let body = serde_json::json!({
        "files": [{
            "path": path_in_repo,
            "size": file_size,
            "sample": ""
        }]
    });

    let url = format!("https://huggingface.co/api/models/{repo_id}/preupload/main");

    let response = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("preupload check failed")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    eprintln!("[upload] preupload: {status} {text}");

    if !status.is_success() {
        // Default to LFS for large files if preupload endpoint fails.
        if file_size > 10_000_000 {
            return Ok("lfs".to_string());
        }
        anyhow::bail!("preupload check failed: {status} {text}");
    }

    // Parse: {"files":[{"path":"...","uploadMode":"lfs"|"regular",...}],...}
    let parsed: serde_json::Value =
        serde_json::from_str(&text).context("failed to parse preupload response")?;

    let upload_mode = parsed
        .get("files")
        .and_then(|f| f.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("uploadMode"))
        .and_then(|v| v.as_str())
        .unwrap_or("lfs")
        .to_string();

    Ok(upload_mode)
}

// ---------------------------------------------------------------------------
// Step 3: Compute SHA-256
// ---------------------------------------------------------------------------

async fn compute_sha256(
    path: &std::path::Path,
    file_size: u64,
    tx: &tokio::sync::mpsc::UnboundedSender<UploadProgress>,
) -> Result<String> {
    use std::io::Read;

    let path = path.to_path_buf();
    let tx = tx.clone();

    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;

        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; HASH_CHUNK_SIZE];
        let mut done: u64 = 0;

        loop {
            let n = file
                .read(&mut buf)
                .with_context(|| format!("failed to read {}", path.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            done += n as u64;

            let _ = tx.send(UploadProgress::Hashing {
                bytes_done: done,
                bytes_total: file_size,
            });
        }

        let hash = hasher.finalize();
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        Ok(hex)
    })
    .await
    .context("SHA-256 task panicked")?
}

// ---------------------------------------------------------------------------
// Step 4: LFS batch
// ---------------------------------------------------------------------------

enum LfsBatchResult {
    /// The object already exists on the server; no upload needed.
    AlreadyExists,
    /// The object must be uploaded to the given URL.
    NeedsUpload {
        upload_url: String,
        verify_url: String,
        verify_token: String,
    },
}

async fn lfs_batch(
    client: &reqwest::Client,
    token: &str,
    repo_id: &str,
    sha256: &str,
    file_size: u64,
) -> Result<LfsBatchResult> {
    let url = format!("https://huggingface.co/{repo_id}.git/info/lfs/objects/batch");

    let body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{
            "oid": sha256,
            "size": file_size
        }],
        "hash_algo": "sha256"
    });

    let response = client
        .post(&url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .json(&body)
        .send()
        .await
        .context("LFS batch request failed")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    eprintln!(
        "[upload] lfs batch: {status} (response len: {})",
        text.len()
    );

    if !status.is_success() {
        anyhow::bail!("LFS batch failed: {status} {text}");
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&text).context("failed to parse LFS batch response")?;

    let obj = parsed
        .get("objects")
        .and_then(|o| o.as_array())
        .and_then(|a| a.first())
        .context("no objects in LFS batch response")?;

    // If there are no "actions", the object already exists on the server.
    let actions = match obj.get("actions") {
        Some(a) => a,
        None => return Ok(LfsBatchResult::AlreadyExists),
    };

    let upload_url = actions
        .get("upload")
        .and_then(|u| u.get("href"))
        .and_then(|v| v.as_str())
        .context("no upload href in LFS batch response")?
        .to_string();

    let verify_url = actions
        .get("verify")
        .and_then(|v| v.get("href"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let verify_token = actions
        .get("verify")
        .and_then(|v| v.get("header"))
        .and_then(|h| h.get("Authorization"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(LfsBatchResult::NeedsUpload {
        upload_url,
        verify_url,
        verify_token,
    })
}

// ---------------------------------------------------------------------------
// Step 5: Upload to S3
// ---------------------------------------------------------------------------

async fn upload_to_s3(
    client: &reqwest::Client,
    upload_url: &str,
    file_path: &std::path::Path,
    file_size: u64,
    tx: &tokio::sync::mpsc::UnboundedSender<UploadProgress>,
) -> Result<()> {
    let file = tokio::fs::File::open(file_path)
        .await
        .with_context(|| format!("failed to open {}", file_path.display()))?;

    let reader = tokio::io::BufReader::new(file);
    let progress_tx = tx.clone();

    let stream = futures::stream::unfold(
        (reader, 0u64, file_size, progress_tx),
        |(mut reader, mut sent, total, progress)| async move {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; CHUNK_SIZE];
            match reader.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    sent += n as u64;
                    let _ = progress.send(UploadProgress::Uploading {
                        bytes_sent: sent,
                        bytes_total: total,
                    });
                    buf.truncate(n);
                    Some((
                        Ok::<_, std::io::Error>(buf),
                        (reader, sent, total, progress),
                    ))
                }
                Err(error) => Some((Err(error), (reader, sent, total, progress))),
            }
        },
    );

    let body = reqwest::Body::wrap_stream(stream);

    let response = client
        .put(upload_url)
        .header("Content-Length", file_size)
        .header("Content-Type", "application/octet-stream")
        .body(body)
        .send()
        .await
        .context("S3 upload request failed")?;

    let status = response.status();
    if !status.is_success() {
        let resp_body = response.text().await.unwrap_or_default();
        anyhow::bail!("S3 upload failed: {status} {resp_body}");
    }

    eprintln!("[upload] S3 upload complete: {status}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 6: LFS verify
// ---------------------------------------------------------------------------

async fn lfs_verify(
    client: &reqwest::Client,
    verify_url: &str,
    verify_token: &str,
    sha256: &str,
    file_size: u64,
) -> Result<()> {
    if verify_url.is_empty() {
        eprintln!("[upload] no verify URL, skipping");
        return Ok(());
    }

    let body = serde_json::json!({
        "oid": sha256,
        "size": file_size
    });

    let mut request = client
        .post(verify_url)
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .json(&body);

    if !verify_token.is_empty() {
        request = request.header("Authorization", verify_token);
    }

    let response = request.send().await.context("LFS verify request failed")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    eprintln!("[upload] lfs verify: {status} {text}");

    if !status.is_success() {
        anyhow::bail!("LFS verify failed: {status} {text}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 7: LFS commit
// ---------------------------------------------------------------------------

async fn lfs_commit(
    client: &reqwest::Client,
    token: &str,
    repo_id: &str,
    path_in_repo: &str,
    commit_message: &str,
    sha256: &str,
    file_size: u64,
) -> Result<()> {
    let url = format!("https://huggingface.co/api/models/{repo_id}/commit/main");

    // Build the LFS pointer content that HuggingFace expects.
    let lfs_pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{sha256}\nsize {file_size}\n"
    );

    let body = serde_json::json!({
        "summary": commit_message,
        "files": [{
            "path": path_in_repo,
            "content": lfs_pointer,
            "lfs": {
                "oid": sha256,
                "size": file_size
            }
        }]
    });

    let response = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("commit request failed")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    eprintln!("[upload] commit: {status} {text}");

    if !status.is_success() {
        anyhow::bail!("commit failed: {status} {text}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Regular (non-LFS) commit for small files
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn regular_commit(
    client: &reqwest::Client,
    token: &str,
    repo_id: &str,
    path_in_repo: &str,
    commit_message: &str,
    file_path: &std::path::Path,
    file_size: u64,
    tx: &tokio::sync::mpsc::UnboundedSender<UploadProgress>,
) -> Result<()> {
    let content = tokio::fs::read(file_path)
        .await
        .with_context(|| format!("failed to read {}", file_path.display()))?;

    let _ = tx.send(UploadProgress::Uploading {
        bytes_sent: file_size,
        bytes_total: file_size,
    });

    // For small regular files, send content as a UTF-8 string.
    // Binary files should always go through the LFS path instead.
    let content_str = String::from_utf8_lossy(&content).into_owned();

    let url = format!("https://huggingface.co/api/models/{repo_id}/commit/main");

    let body = serde_json::json!({
        "summary": commit_message,
        "files": [{
            "path": path_in_repo,
            "content": content_str
        }]
    });

    let _ = tx.send(UploadProgress::Committing);

    let response = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("commit request failed")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    eprintln!("[upload] regular commit: {status} {text}");

    if !status.is_success() {
        anyhow::bail!("commit failed: {status} {text}");
    }

    Ok(())
}
