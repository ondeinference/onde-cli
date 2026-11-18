//! What the Projects screen shows, read from disk through the onde-cli library.
//!
//! The webview names a project by its repo id and a run by its folder name.
//! Both are checked here before they become part of a path, so a crafted id
//! cannot reach outside the projects folder.

use std::path::Path;

use onde_cli::{
    pipeline::{self, RunRecord, StepRecord},
    project::{self, DatasetInfo, ProjectMeta},
};
use serde::Serialize;

/// Whether a project has a usable dataset, for the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetKind {
    Missing,
    /// `train.jsonl` exists but does not validate; the page shows why.
    Invalid,
    Ready,
}

/// One row of the project list.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub repo_id: String,
    pub base_model_id: String,
    pub created_at: u64,
    pub dataset: DatasetKind,
    /// Rows of a valid dataset.
    pub dataset_rows: Option<usize>,
    pub runs: usize,
    pub latest_run: Option<String>,
}

/// Everything on one project's page.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectDetail {
    pub repo_id: String,
    pub base_model_id: String,
    pub created_at: u64,
    pub dataset: DatasetState,
    pub runs: Vec<RunRecord>,
    /// An `eval/eval_inputs.json` exists, so the project can be evaluated.
    pub has_eval: bool,
}

/// The dataset as the screen needs it: missing, broken (with the reason) or fine.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DatasetState {
    Missing,
    Invalid { error: String },
    Ready(DatasetInfo),
}

/// `org/name`: two parts of letters, digits, `.`, `-` and `_`, neither of them
/// `.` or `..`. Hugging Face repo ids have this shape.
pub fn check_repo_id(repo_id: &str) -> Result<(), String> {
    let parts: Vec<&str> = repo_id.split('/').collect();
    let part_ok = |p: &str| {
        !p.is_empty()
            && p != "."
            && p != ".."
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    };
    if parts.len() == 2 && parts.iter().all(|p| part_ok(p)) {
        Ok(())
    } else {
        Err(format!(
            "{repo_id:?} is not a project id of the form org/name"
        ))
    }
}

fn dataset_state(path: &Path) -> DatasetState {
    if !path.exists() {
        return DatasetState::Missing;
    }
    match project::inspect_dataset(path) {
        Ok(info) => DatasetState::Ready(info),
        Err(e) => DatasetState::Invalid {
            error: format!("{e:#}"),
        },
    }
}

fn dataset_kind(path: &Path) -> (DatasetKind, Option<usize>) {
    match dataset_state(path) {
        DatasetState::Missing => (DatasetKind::Missing, None),
        DatasetState::Invalid { .. } => (DatasetKind::Invalid, None),
        DatasetState::Ready(info) => (DatasetKind::Ready, Some(info.rows)),
    }
}

fn summary(repo_id: String, meta: ProjectMeta) -> ProjectSummary {
    let ((dataset, dataset_rows), runs) = match project::load_project(&repo_id) {
        Ok(p) => (dataset_kind(&p.dataset_path), project::list_runs(&p)),
        Err(_) => ((DatasetKind::Missing, None), Vec::new()),
    };
    ProjectSummary {
        repo_id,
        base_model_id: meta.base_model_id,
        created_at: meta.created_at,
        dataset,
        dataset_rows,
        latest_run: runs.first().map(|r| r.name.clone()),
        runs: runs.len(),
    }
}

/// Every project in the current projects folder, newest first.
pub fn list() -> Vec<ProjectSummary> {
    let mut all: Vec<ProjectSummary> = project::list_projects()
        .into_iter()
        .map(|(id, meta)| summary(id, meta))
        .collect();
    all.sort_by_key(|p| std::cmp::Reverse(p.created_at));
    all
}

pub fn detail(repo_id: &str) -> Result<ProjectDetail, String> {
    check_repo_id(repo_id)?;
    let p = project::load_project(repo_id).map_err(|e| format!("{e:#}"))?;
    let meta = project::list_projects()
        .into_iter()
        .find(|(id, _)| id == repo_id)
        .map(|(_, m)| m);
    Ok(ProjectDetail {
        repo_id: p.repo_id.clone(),
        base_model_id: p.base_model_id.clone(),
        created_at: meta.map_or(0, |m| m.created_at),
        dataset: dataset_state(&p.dataset_path),
        runs: project::list_runs(&p),
        has_eval: p.project_dir.join("eval/eval_inputs.json").is_file(),
    })
}

/// The per-step records of one run. A run made outside the app has none.
pub fn steps(repo_id: &str, run: &str) -> Result<Vec<StepRecord>, String> {
    check_repo_id(repo_id)?;
    pipeline::validate_run_name(run).map_err(|e| format!("{e:#}"))?;
    let p = project::load_project(repo_id).map_err(|e| format!("{e:#}"))?;
    Ok(pipeline::read_steps(&p.runs_dir.join(run)))
}

/// Validate the chosen file and make it the project's training data.
pub fn import_dataset(repo_id: &str, file: &Path) -> Result<DatasetInfo, String> {
    check_repo_id(repo_id)?;
    let p = project::load_project(repo_id).map_err(|e| format!("{e:#}"))?;
    project::import_dataset(&p, file).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_ids_must_stay_inside_the_projects_folder() {
        for ok in ["ondeinference/mailx", "Qwen/Qwen3-0.6B", "a_b/c.d-e"] {
            assert!(check_repo_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "mailx",
            "a/b/c",
            "../x",
            "a/..",
            "./b",
            "/abs/path",
            "a/ b",
            "a\\b",
        ] {
            assert!(check_repo_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_missing_or_broken_dataset_is_reported_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("train.jsonl");
        assert!(matches!(dataset_state(&file), DatasetState::Missing));
        std::fs::write(&file, "not json\n").unwrap();
        match dataset_state(&file) {
            DatasetState::Invalid { error } => assert!(error.contains("line 1"), "{error}"),
            other => panic!("{other:?}"),
        }
        std::fs::write(&file, "{\"text\":\"hi\"}\n").unwrap();
        assert!(matches!(dataset_state(&file), DatasetState::Ready(i) if i.rows == 1));
    }

    #[test]
    fn the_list_tells_a_missing_dataset_from_a_broken_one() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("train.jsonl");
        assert_eq!(dataset_kind(&file), (DatasetKind::Missing, None));
        std::fs::write(&file, "not json\n").unwrap();
        assert_eq!(dataset_kind(&file), (DatasetKind::Invalid, None));
        std::fs::write(&file, "{\"text\":\"a\"}\n{\"text\":\"b\"}\n").unwrap();
        assert_eq!(dataset_kind(&file), (DatasetKind::Ready, Some(2)));
    }

    #[test]
    fn steps_refuse_a_run_name_that_is_a_path() {
        let err = steps("o/p", "../../etc").unwrap_err();
        assert!(err.contains("run name"), "{err}");
    }
}
