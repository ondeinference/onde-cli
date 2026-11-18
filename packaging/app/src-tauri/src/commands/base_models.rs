use onde_cli::{hf, hf_search::DownloadEvent, models};
use serde::Serialize;
use tauri::{State, ipc::Channel};

use crate::{
    desktop::{
        base_models::{self, BaseModelView},
        projects::ProjectSummary,
    },
    error::CommandError,
    state_downloads::Downloads,
};

/// What the webview hears while a model downloads.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DownloadMsg {
    Progress {
        filename: String,
        file_index: usize,
        total_files: usize,
        file_bytes_done: u64,
        file_bytes_total: u64,
    },
    Done,
    Failed {
        message: String,
    },
    Cancelled,
}

#[tauri::command]
pub async fn list_base_models() -> Result<Vec<BaseModelView>, CommandError> {
    tauri::async_runtime::spawn_blocking(base_models::list)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn create_project(
    repo_id: String,
    base_model_id: String,
) -> Result<ProjectSummary, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        base_models::create_project(&repo_id, &base_model_id)
    })
    .await
    .map_err(|e| CommandError::Internal(e.to_string()))?
    .map_err(CommandError::InvalidChoice)
}

/// Download a catalog model into the shared model cache, reporting progress on
/// `on_event`. Resolves when it finishes, fails or is cancelled.
#[tauri::command]
pub async fn download_base_model(
    repo_id: String,
    on_event: Channel<DownloadMsg>,
    downloads: State<'_, Downloads>,
) -> Result<(), CommandError> {
    if !base_models::in_catalog(&repo_id) {
        return Err(CommandError::InvalidChoice(format!(
            "{repo_id} is not one of the base models this app can download."
        )));
    }
    if models::base_model_dir(&repo_id).is_some() {
        let _ = on_event.send(DownloadMsg::Done);
        return Ok(());
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(onde_cli::hf_search::download_model(
        repo_id.clone(),
        hf::preferred_download_hub(),
        tx,
    ));
    if !downloads.begin(&repo_id, task.abort_handle()) {
        task.abort();
        return Err(CommandError::InvalidChoice(format!(
            "{repo_id} is already downloading."
        )));
    }

    let mut last = DownloadMsg::Cancelled;
    while let Some(event) = rx.recv().await {
        let msg = match event {
            DownloadEvent::Progress(p) => DownloadMsg::Progress {
                filename: p.filename,
                file_index: p.file_index,
                total_files: p.total_files,
                file_bytes_done: p.file_bytes_done,
                file_bytes_total: p.file_bytes_total,
            },
            DownloadEvent::Complete => DownloadMsg::Done,
            DownloadEvent::Failed(message) => DownloadMsg::Failed { message },
        };
        let finished = !matches!(msg, DownloadMsg::Progress { .. });
        let _ = on_event.send(msg.clone());
        last = msg;
        if finished {
            break;
        }
    }
    downloads.finish(&repo_id);
    // The channel closing with no final message means the task was aborted.
    if matches!(last, DownloadMsg::Cancelled) {
        let _ = on_event.send(DownloadMsg::Cancelled);
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_base_download(repo_id: String, downloads: State<'_, Downloads>) -> bool {
    downloads.cancel(&repo_id)
}
