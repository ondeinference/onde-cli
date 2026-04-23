#![allow(dead_code)]
//! Project workspace management for onde-cli fine-tuning.
//!
//! Each target HuggingFace repo (e.g. `ondeinference/joko`) gets its own
//! workspace under `~/.onde/projects/{org}/{name}/`.  The base model lives
//! in the shared HuggingFace cache (read-only); all generated artifacts
//! (LoRA adapters, merged models, GGUF exports) go into the project dir.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Metadata stored in `project.json` at the project root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    /// Target HuggingFace repo ID, e.g. `ondeinference/joko`.
    pub repo_id: String,
    /// Base model HF repo used for fine-tuning, e.g. `Qwen/Qwen3-0.6B`.
    pub base_model_id: String,
    /// Unix timestamp when the project was created.
    pub created_at: u64,
}

/// A resolved project workspace with all paths computed.
#[derive(Debug, Clone)]
pub struct OndeProject {
    /// Target HF repo, e.g. `ondeinference/joko`.
    pub repo_id: String,
    /// Base model HF repo, e.g. `Qwen/Qwen3-0.6B`.
    pub base_model_id: String,
    /// Resolved path to the base model in the local HF cache.
    pub base_model_dir: String,
    /// Root of this project workspace, e.g. `~/.onde/projects/ondeinference/joko`.
    pub project_dir: PathBuf,
    /// Path to the dataset file: `{project_dir}/dataset/train.jsonl`.
    pub dataset_path: PathBuf,
    /// Path to the runs directory: `{project_dir}/runs/`.
    pub runs_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Root directory for all onde projects.
pub fn projects_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".onde")
        .join("projects")
}

/// Create a new project workspace on disk and return its metadata.
///
/// - Creates `~/.onde/projects/{repo_id}/` with `dataset/` and `runs/` subdirs.
/// - Writes `project.json` with the repo ID and base model.
/// - Does NOT overwrite an existing `project.json`.
pub fn create_project(repo_id: &str, base_model_id: &str) -> Result<OndeProject> {
    let project_dir = projects_root().join(repo_id);
    let dataset_dir = project_dir.join("dataset");
    let runs_dir = project_dir.join("runs");

    std::fs::create_dir_all(&dataset_dir)
        .with_context(|| format!("creating {}", dataset_dir.display()))?;
    std::fs::create_dir_all(&runs_dir)
        .with_context(|| format!("creating {}", runs_dir.display()))?;

    let meta_path = project_dir.join("project.json");
    let meta = if meta_path.exists() {
        // Load existing — don't overwrite (user may have changed base_model_id)
        let raw = std::fs::read_to_string(&meta_path)
            .with_context(|| format!("reading {}", meta_path.display()))?;
        serde_json::from_str::<ProjectMeta>(&raw)
            .with_context(|| format!("parsing {}", meta_path.display()))?
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let meta = ProjectMeta {
            repo_id: repo_id.to_string(),
            base_model_id: base_model_id.to_string(),
            created_at: now,
        };
        let json = serde_json::to_string_pretty(&meta).context("serializing project.json")?;
        std::fs::write(&meta_path, json)
            .with_context(|| format!("writing {}", meta_path.display()))?;
        meta
    };

    Ok(OndeProject {
        repo_id: meta.repo_id,
        base_model_id: meta.base_model_id.clone(),
        base_model_dir: String::new(), // resolved later by the caller
        project_dir,
        dataset_path: dataset_dir.join("train.jsonl"),
        runs_dir,
    })
}

/// Load an existing project from disk.
pub fn load_project(repo_id: &str) -> Result<OndeProject> {
    let project_dir = projects_root().join(repo_id);
    let meta_path = project_dir.join("project.json");

    if !meta_path.exists() {
        anyhow::bail!("no project found at {}", project_dir.display());
    }

    let raw = std::fs::read_to_string(&meta_path)
        .with_context(|| format!("reading {}", meta_path.display()))?;
    let meta: ProjectMeta =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", meta_path.display()))?;

    let dataset_path = project_dir.join("dataset").join("train.jsonl");
    let runs_dir = project_dir.join("runs");

    Ok(OndeProject {
        repo_id: meta.repo_id,
        base_model_id: meta.base_model_id,
        base_model_dir: String::new(),
        project_dir,
        dataset_path,
        runs_dir,
    })
}

/// List all existing projects under `~/.onde/projects/`.
///
/// Returns `(repo_id, ProjectMeta)` pairs sorted alphabetically.
pub fn list_projects() -> Vec<(String, ProjectMeta)> {
    let root = projects_root();
    let mut projects = Vec::new();

    let Ok(orgs) = std::fs::read_dir(&root) else {
        return projects;
    };

    for org_entry in orgs.flatten() {
        let org_path = org_entry.path();
        if !org_path.is_dir() {
            continue;
        }
        let org_name = org_entry.file_name().to_string_lossy().to_string();

        let Ok(repos) = std::fs::read_dir(&org_path) else {
            continue;
        };

        for repo_entry in repos.flatten() {
            let repo_path = repo_entry.path();
            let meta_path = repo_path.join("project.json");
            if !meta_path.exists() {
                continue;
            }

            let repo_name = repo_entry.file_name().to_string_lossy().to_string();
            let repo_id = format!("{org_name}/{repo_name}");

            if let Ok(raw) = std::fs::read_to_string(&meta_path)
                && let Ok(meta) = serde_json::from_str::<ProjectMeta>(&raw)
            {
                projects.push((repo_id, meta));
            }
        }
    }

    projects.sort_by(|a, b| a.0.cmp(&b.0));
    projects
}

/// Generate a new timestamped run directory under the project.
///
/// Returns the path to the run dir (e.g. `{project}/runs/1750012345/`).
/// The directory is created on disk.
pub fn new_run_dir(project: &OndeProject) -> Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let run_dir = project.runs_dir.join(format!("{ts}"));
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("creating run dir {}", run_dir.display()))?;
    Ok(run_dir)
}

/// Scan a project's runs directory for all artifacts (LoRA adapters and GGUFs).
///
/// Each run is a timestamped subdirectory under `{project}/runs/`.
/// Returns entries sorted by modification time (newest first).
pub fn scan_project_artifacts(project: &OndeProject) -> Vec<ProjectArtifact> {
    let mut artifacts = Vec::new();

    let Ok(runs) = std::fs::read_dir(&project.runs_dir) else {
        return artifacts;
    };

    for run_entry in runs.flatten() {
        let run_dir = run_entry.path();
        if !run_dir.is_dir() {
            continue;
        }
        let run_name = run_entry.file_name().to_string_lossy().to_string();

        // Check for LoRA adapter
        let lora_file = run_dir.join("lora_adapter.safetensors");
        if lora_file.is_file()
            && let Some(a) = artifact_from_path(&lora_file, &run_name, ArtifactType::LoraAdapter)
        {
            artifacts.push(a);
        }

        // Check for merged model
        let merged_dir = run_dir.join("merged");
        if merged_dir.is_dir() {
            let merged_safetensors = merged_dir.join("model.safetensors");
            if merged_safetensors.is_file()
                && let Some(a) =
                    artifact_from_path(&merged_safetensors, &run_name, ArtifactType::MergedModel)
            {
                artifacts.push(a);
            }
        }

        // Check for GGUF files
        if let Ok(files) = std::fs::read_dir(&run_dir) {
            for file in files.flatten() {
                let fname = file.file_name().to_string_lossy().to_string();
                if fname.ends_with(".gguf")
                    && let Some(a) = artifact_from_path(&file.path(), &run_name, ArtifactType::Gguf)
                {
                    artifacts.push(a);
                }
            }
        }
    }

    // Newest first
    artifacts.sort_by(|a, b| b.modified_ts.cmp(&a.modified_ts));
    artifacts
}

// ---------------------------------------------------------------------------
// Artifact types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactType {
    LoraAdapter,
    MergedModel,
    Gguf,
}

/// An artifact found in a project's runs directory.
#[derive(Debug, Clone)]
pub struct ProjectArtifact {
    /// Full path to the artifact file.
    pub path: PathBuf,
    /// Name of the run directory (timestamp).
    pub run_name: String,
    /// Filename.
    pub file_name: String,
    /// Human-readable file size.
    pub size_display: String,
    /// Relative time display ("just now", "3h ago").
    pub modified_display: String,
    /// Raw modification timestamp for sorting.
    pub modified_ts: u64,
    /// What kind of artifact this is.
    pub kind: ArtifactType,
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn artifact_from_path(path: &Path, run_name: &str, kind: ArtifactType) -> Option<ProjectArtifact> {
    let meta = std::fs::metadata(path).ok()?;
    let size = if meta.is_dir() {
        dir_size(path)
    } else {
        meta.len()
    };
    let modified_ts = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age_secs = now.saturating_sub(modified_ts);
    let modified_display = format_age(age_secs);

    Some(ProjectArtifact {
        path: path.to_path_buf(),
        run_name: run_name.to_string(),
        file_name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size_display: format_size(size),
        modified_display,
        modified_ts,
        kind,
    })
}

fn dir_size(path: &Path) -> u64 {
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
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{}MB", bytes / MB)
    } else if bytes >= KB {
        format!("{}KB", bytes / KB)
    } else {
        format!("{}B", bytes)
    }
}

fn format_age(secs: u64) -> String {
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
