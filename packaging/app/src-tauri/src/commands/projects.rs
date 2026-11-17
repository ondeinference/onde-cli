use onde_cli::{pipeline::StepRecord, project::DatasetInfo};
use tauri::AppHandle;

use crate::{
    desktop::projects::{self, ProjectDetail, ProjectSummary},
    error::CommandError,
    picker::{self, Mode, Request},
};

// Reading a project walks its runs and validates its dataset line by line, so
// these run on the blocking pool and never stall the webview.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn list_projects() -> Result<Vec<ProjectSummary>, CommandError> {
    blocking(|| Ok(projects::list())).await
}

#[tauri::command]
pub async fn project_detail(repo_id: String) -> Result<ProjectDetail, CommandError> {
    blocking(move || projects::detail(&repo_id)).await
}

#[tauri::command]
pub async fn run_steps(repo_id: String, run: String) -> Result<Vec<StepRecord>, CommandError> {
    blocking(move || projects::steps(&repo_id, &run)).await
}

/// Ask for a `.jsonl` file and make it the project's training data. The old
/// file is kept next to it. Cancelling is not an error.
#[tauri::command]
pub async fn import_dataset(app: AppHandle, repo_id: String) -> Result<DatasetInfo, CommandError> {
    projects::check_repo_id(&repo_id).map_err(CommandError::Internal)?;
    let picked = picker::pick(
        &app,
        Request {
            mode: Mode::File {
                extensions: &["jsonl"],
            },
            title: "Choose a training dataset (.jsonl)",
            start: None,
            want_bookmark: false,
        },
    )
    .await
    .map_err(CommandError::Internal)?
    .ok_or_else(|| CommandError::Cancelled("no file chosen".into()))?;
    let path = picked.path;
    tauri::async_runtime::spawn_blocking(move || projects::import_dataset(&repo_id, &path))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(CommandError::InvalidChoice)
}
