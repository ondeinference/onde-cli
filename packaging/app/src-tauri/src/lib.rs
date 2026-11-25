mod commands;
mod desktop;
mod error;
mod picker;
mod state;
mod state_downloads;

#[cfg(test)]
mod capability_tests;

pub fn run() {
    // The storage root must be set before anything reads the token or
    // projects, so it comes first.
    if let Err(e) = desktop::storage::init() {
        eprintln!("could not prepare the storage directory: {e}");
    }

    // A folder the user linked in an earlier session is reopened before any
    // project is read. Outside the sandbox there is nothing to reopen.
    let links = desktop::links::Links::new(onde_cli::paths::onde_root().join("links.json"));
    if desktop::storage::is_sandboxed()
        && let Some(dir) = links.restore()
    {
        onde_cli::paths::set_projects_dir(Some(dir));
    }

    let builder = tauri::Builder::default();
    // macOS drives NSOpenPanel itself (see `picker.rs`).
    #[cfg(not(target_os = "macos"))]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .manage(state::PendingSignIn::default())
        .manage(links)
        .manage(state_downloads::Downloads::default())
        .manage(desktop::jobs::Jobs::default())
        .invoke_handler(tauri::generate_handler![
            commands::app::app_build,
            commands::links::projects_location,
            commands::links::link_projects_folder,
            commands::links::unlink_projects_folder,
            commands::base_models::list_base_models,
            commands::base_models::create_project,
            commands::base_models::download_base_model,
            commands::base_models::cancel_base_download,
            commands::base_models::base_download_running,
            commands::jobs::start_job,
            commands::jobs::attach_job,
            commands::jobs::cancel_job,
            commands::projects::list_projects,
            commands::projects::project_detail,
            commands::projects::run_steps,
            commands::projects::import_dataset,
            commands::auth::current_user,
            commands::auth::sign_in,
            commands::auth::sign_up,
            commands::auth::sign_out,
            commands::oauth::oauth_sign_in,
            commands::oauth::oauth_sign_in_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Onde Desktop");
}
