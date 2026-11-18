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
    crate::paths::projects_dir()
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
    artifacts.sort_by_key(|a| std::cmp::Reverse(a.modified_ts));
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

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

/// Every run in a project, newest first.
///
/// Runs made by [`crate::pipeline`] are read from their `run.json`. A
/// directory without one (a run made by hand, or by an older version) is
/// listed as [`crate::pipeline::RunStatus::Imported`], with the adapter and GGUF it holds.
pub fn list_runs(project: &OndeProject) -> Vec<crate::pipeline::RunRecord> {
    use crate::pipeline::{RunRecord, RunStatus, read_record};

    let Ok(entries) = std::fs::read_dir(&project.runs_dir) else {
        return Vec::new();
    };
    let mut runs: Vec<RunRecord> = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        if let Ok(record) = read_record(&dir) {
            runs.push(record);
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let gguf = first_gguf(&dir);
        let adapter = ["lora/lora_adapter.safetensors", "lora_adapter.safetensors"]
            .iter()
            .map(|p| dir.join(p))
            .find(|p| p.is_file());
        if gguf.is_none() && adapter.is_none() {
            // Nothing a user could use: a failed or empty directory.
            continue;
        }
        let modified = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        runs.push(RunRecord {
            name,
            spec: None,
            status: RunStatus::Imported,
            backend: None,
            backend_note: None,
            started_at: modified,
            finished_at: Some(modified),
            steps: 0,
            skipped: 0,
            epoch_loss: Vec::new(),
            adapter_path: adapter,
            gguf_size: gguf
                .as_ref()
                .and_then(|g| g.metadata().ok())
                .map(|m| m.len()),
            gguf_path: gguf,
            error: None,
        });
    }
    runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    runs
}

fn first_gguf(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "gguf"))
        .collect();
    found.sort();
    found.into_iter().next()
}

// ---------------------------------------------------------------------------
// Datasets
// ---------------------------------------------------------------------------

/// What a dataset file holds, for showing before training.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatasetInfo {
    pub rows: usize,
    /// Rows written as `{"text": ...}`. The fine-tuner puts loss on every token.
    pub text_rows: usize,
    /// Rows written as `{"messages": [...]}`. Loss is on the assistant turns only.
    pub message_rows: usize,
    /// Message rows that begin with a system turn.
    pub with_system: usize,
    /// Up to three of the first rows, as written.
    pub sample: Vec<serde_json::Value>,
}

/// Check a JSONL dataset line by line. The error names the first bad line.
pub fn inspect_dataset(path: &Path) -> Result<DatasetInfo> {
    use std::io::BufRead;

    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut info = DatasetInfo {
        rows: 0,
        text_rows: 0,
        message_rows: 0,
        with_system: 0,
        sample: Vec::new(),
    };
    for (index, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("reading line {}", index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let n = index + 1;
        let value: serde_json::Value =
            serde_json::from_str(&line).with_context(|| format!("line {n} is not JSON"))?;
        if let Some(text) = value.get("text") {
            anyhow::ensure!(
                text.as_str().is_some_and(|t| !t.trim().is_empty()),
                "line {n}: \"text\" must be a non-empty string"
            );
            info.text_rows += 1;
        } else if let Some(messages) = value.get("messages") {
            let messages = messages
                .as_array()
                .with_context(|| format!("line {n}: \"messages\" must be an array"))?;
            let mut assistant = false;
            for m in messages {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
                let content = m.get("content").and_then(|c| c.as_str());
                anyhow::ensure!(
                    matches!(role, "system" | "user" | "assistant"),
                    "line {n}: role must be system, user or assistant, got {role:?}"
                );
                anyhow::ensure!(
                    content.is_some(),
                    "line {n}: every message needs a string \"content\""
                );
                if role == "assistant" && content.is_some_and(|c| !c.trim().is_empty()) {
                    assistant = true;
                }
            }
            anyhow::ensure!(
                assistant,
                "line {n}: no assistant turn with content, so nothing to learn from"
            );
            if messages
                .first()
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                == Some("system")
            {
                info.with_system += 1;
            }
            info.message_rows += 1;
        } else {
            anyhow::bail!("line {n}: expected a \"text\" or a \"messages\" field");
        }
        info.rows += 1;
        if info.sample.len() < 3 {
            info.sample.push(value);
        }
    }
    anyhow::ensure!(info.rows > 0, "{} has no rows", path.display());
    Ok(info)
}

/// Validate `src` and copy it to the project's `dataset/train.jsonl`. An
/// existing training file is kept as `train.{unix time}.jsonl`.
pub fn import_dataset(project: &OndeProject, src: &Path) -> Result<DatasetInfo> {
    let info = inspect_dataset(src)?;
    if let Some(dir) = project.dataset_path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    if project.dataset_path.exists() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup = project
            .dataset_path
            .with_file_name(format!("train.{ts}.jsonl"));
        std::fs::rename(&project.dataset_path, &backup)
            .with_context(|| format!("keeping the old dataset as {}", backup.display()))?;
    }
    std::fs::copy(src, &project.dataset_path)
        .with_context(|| format!("copying to {}", project.dataset_path.display()))?;
    Ok(info)
}

#[cfg(test)]
mod run_and_dataset_tests {
    use super::*;
    use crate::pipeline::RunStatus;

    fn project(root: &Path) -> OndeProject {
        OndeProject {
            repo_id: "o/p".into(),
            base_model_id: "b".into(),
            base_model_dir: String::new(),
            project_dir: root.to_path_buf(),
            dataset_path: root.join("dataset/train.jsonl"),
            runs_dir: root.join("runs"),
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn hand_made_runs_are_listed_as_imported() {
        let tmp = tempfile::tempdir().unwrap();
        let p = project(tmp.path());
        write(&p.runs_dir.join("run7/model-q8_0.gguf"), "GGUF");
        write(&p.runs_dir.join("e2/lora/lora_adapter.safetensors"), "x");
        std::fs::create_dir_all(p.runs_dir.join("empty")).unwrap();
        let mut names: Vec<_> = list_runs(&p)
            .into_iter()
            .map(|r| (r.name, r.status))
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            names,
            vec![
                ("e2".to_string(), RunStatus::Imported),
                ("run7".to_string(), RunStatus::Imported)
            ]
        );
    }

    #[test]
    fn messages_and_text_rows_are_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("t.jsonl");
        write(
            &file,
            concat!(
                r#"{"messages":[{"role":"system","content":"s"},{"role":"user","content":"u"},{"role":"assistant","content":"a"}]}"#,
                "\n\n",
                r#"{"messages":[{"role":"user","content":"u"},{"role":"assistant","content":"a"}]}"#,
                "\n",
                r#"{"text":"<|im_start|>user\nhi<|im_end|>"}"#,
                "\n"
            ),
        );
        let info = inspect_dataset(&file).unwrap();
        assert_eq!(
            (
                info.rows,
                info.message_rows,
                info.text_rows,
                info.with_system
            ),
            (3, 2, 1, 1)
        );
        assert_eq!(info.sample.len(), 3);
    }

    #[test]
    fn bad_rows_are_named_by_line() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = |text: &str| {
            let f = tmp.path().join("bad.jsonl");
            write(&f, text);
            format!("{:#}", inspect_dataset(&f).unwrap_err())
        };
        assert!(bad("{\"text\":\"ok\"}\nnot json\n").contains("line 2 is not JSON"));
        assert!(
            bad(r#"{"messages":[{"role":"user","content":"u"}]}"#).contains("no assistant turn")
        );
        assert!(bad(r#"{"messages":[{"role":"bot","content":"u"}]}"#).contains("role must be"));
        assert!(bad(r#"{"text":"  "}"#).contains("non-empty"));
        assert!(bad(r#"{"foo":1}"#).contains("expected a"));
        assert!(bad("\n\n").contains("no rows"));
    }

    #[test]
    fn importing_keeps_the_previous_dataset() {
        let tmp = tempfile::tempdir().unwrap();
        let p = project(&tmp.path().join("proj"));
        let a = tmp.path().join("a.jsonl");
        let b = tmp.path().join("b.jsonl");
        write(&a, "{\"text\":\"first\"}\n");
        write(&b, "{\"text\":\"second\"}\n");
        import_dataset(&p, &a).unwrap();
        import_dataset(&p, &b).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p.dataset_path).unwrap(),
            "{\"text\":\"second\"}\n"
        );
        let kept: Vec<_> = std::fs::read_dir(p.dataset_path.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("train.") && n != "train.jsonl")
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
    }

    #[test]
    fn a_rejected_dataset_leaves_the_current_one_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let p = project(&tmp.path().join("proj"));
        let good = tmp.path().join("good.jsonl");
        let bad = tmp.path().join("bad.jsonl");
        write(&good, "{\"text\":\"keep me\"}\n");
        write(&bad, "oops\n");
        import_dataset(&p, &good).unwrap();
        assert!(import_dataset(&p, &bad).is_err());
        assert_eq!(
            std::fs::read_to_string(&p.dataset_path).unwrap(),
            "{\"text\":\"keep me\"}\n"
        );
    }
}
