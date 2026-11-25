//! The folder and file chooser, opened from Rust.
//!
//! The webview is untrusted, so it gets no filesystem or dialog permission
//! (`capability_tests.rs` enforces that). It asks a command, and the command
//! opens the panel here and returns only what the user chose.

use std::path::PathBuf;

use tauri::AppHandle;

pub enum Mode {
    Folder,
    File { extensions: &'static [&'static str] },
}

pub struct Picked {
    pub path: PathBuf,
    /// A security-scoped bookmark made from the panel's own URL, when asked.
    pub bookmark: Option<Vec<u8>>,
}

pub struct Request {
    pub mode: Mode,
    pub title: &'static str,
    pub start: Option<PathBuf>,
    pub want_bookmark: bool,
}

/// Show the panel and wait for the answer. `Ok(None)` means the user cancelled.
#[cfg(target_os = "macos")]
pub async fn pick(app: &AppHandle, request: Request) -> Result<Option<Picked>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(run(request));
    })
    .map_err(|e| format!("opening the chooser: {e}"))?;
    rx.await
        .map_err(|_| "the chooser closed unexpectedly".to_string())?
}

#[cfg(target_os = "macos")]
fn run(request: Request) -> Result<Option<Picked>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSArray, NSString};

    let mtm = MainThreadMarker::new().ok_or("the chooser must open on the main thread")?;
    let panel = NSOpenPanel::openPanel(mtm);
    let folder = matches!(request.mode, Mode::Folder);
    panel.setCanChooseDirectories(folder);
    panel.setCanChooseFiles(!folder);
    panel.setAllowsMultipleSelection(false);
    // `~/.onde` is a hidden folder.
    panel.setShowsHiddenFiles(true);
    panel.setMessage(Some(&NSString::from_str(request.title)));
    if let Some(start) = &request.start {
        panel.setDirectoryURL(Some(&crate::desktop::bookmark::file_url(start)));
    }
    if let Mode::File { extensions } = request.mode {
        let types: Vec<_> = extensions.iter().map(|e| NSString::from_str(e)).collect();
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));
    }
    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    let Some(url) = panel.URL() else {
        return Ok(None);
    };
    let path = url
        .path()
        .map(|p| PathBuf::from(p.to_string()))
        .ok_or("the chosen item has no path")?;
    let bookmark = if request.want_bookmark {
        Some(crate::desktop::bookmark::create_from_url(&url).map_err(|e| e.to_string())?)
    } else {
        None
    };
    Ok(Some(Picked { path, bookmark }))
}

/// Show the system dialog and wait for the answer. `Ok(None)` means the user
/// cancelled. The blocking calls must stay off the main thread.
#[cfg(not(target_os = "macos"))]
pub async fn pick(app: &AppHandle, request: Request) -> Result<Option<Picked>, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || run(&app, request))
        .await
        .map_err(|e| format!("opening the chooser: {e}"))?
}

#[cfg(not(target_os = "macos"))]
fn run(app: &AppHandle, request: Request) -> Result<Option<Picked>, String> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file().set_title(request.title);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    if let Some(start) = &request.start {
        dialog = dialog.set_directory(start);
    }
    let chosen = match request.mode {
        Mode::Folder => dialog.blocking_pick_folder(),
        Mode::File { extensions } => dialog
            .add_filter("Supported files", extensions)
            .blocking_pick_file(),
    };
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|e| format!("the chosen item has no path ({e})"))?;
    // There is no sandbox here, so the "bookmark" is the path itself.
    let bookmark = request
        .want_bookmark
        .then(|| crate::desktop::bookmark::create(&path));
    Ok(Some(Picked { path, bookmark }))
}
