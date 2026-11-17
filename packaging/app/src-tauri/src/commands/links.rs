use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::{
    desktop::{
        links::{Links, Location},
        storage,
    },
    error::CommandError,
    picker::{self, Mode, Request},
};

/// Where projects are kept when nothing is linked: the app's own storage root
/// when sandboxed, `~/.onde` otherwise.
fn container_projects() -> PathBuf {
    onde_cli::paths::onde_root().join("projects")
}

fn location(links: &Links) -> Location {
    links.location(storage::is_sandboxed(), container_projects())
}

#[tauri::command]
pub fn projects_location(links: State<'_, Links>) -> Location {
    location(&links)
}

/// Let the user pick their `~/.onde` (or its `projects` folder) and keep
/// working in it from then on. Cancelling is not an error.
#[tauri::command]
pub async fn link_projects_folder(
    app: AppHandle,
    links: State<'_, Links>,
) -> Result<Location, CommandError> {
    let picked = picker::pick(
        &app,
        Request {
            mode: Mode::Folder,
            title: "Choose your .onde folder, or the projects folder inside it",
            start: onde_cli::paths::real_home(),
            want_bookmark: true,
        },
    )
    .await
    .map_err(CommandError::Internal)?
    .ok_or_else(|| CommandError::Cancelled("no folder chosen".into()))?;
    let bookmark = picked
        .bookmark
        .ok_or_else(|| CommandError::Internal("the chooser made no bookmark".into()))?;
    let dir = links.link(&bookmark).map_err(CommandError::InvalidChoice)?;
    onde_cli::paths::set_projects_dir(Some(dir));
    Ok(location(&links))
}

#[tauri::command]
pub fn unlink_projects_folder(links: State<'_, Links>) -> Result<Location, CommandError> {
    links.unlink().map_err(CommandError::Internal)?;
    onde_cli::paths::set_projects_dir(None);
    Ok(location(&links))
}
