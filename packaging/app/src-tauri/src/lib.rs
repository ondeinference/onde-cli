mod commands;
mod desktop;
mod error;
mod state;

#[cfg(test)]
mod capability_tests;

pub fn run() {
    // The storage root must be set before anything reads the token or
    // projects, so it comes first.
    if let Err(e) = desktop::storage::init() {
        eprintln!("could not prepare the storage directory: {e}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .manage(state::PendingSignIn::default())
        .invoke_handler(tauri::generate_handler![
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
