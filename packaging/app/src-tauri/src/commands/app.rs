use crate::desktop::app;

/// The build number (CFBundleVersion), or `None` in a dev run.
#[tauri::command]
pub fn app_build() -> Option<String> {
    app::build_number(option_env!("ONDE_DESKTOP_BUILD"))
}
