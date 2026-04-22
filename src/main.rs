mod app;
mod gresiq;
mod hf;
mod token;
mod ui;

/// Point stderr at ~/.cache/onde/debug.log before ratatui owns the terminal.
/// Without this, any stray eprintln! or log output tears up the display.
#[cfg(unix)]
fn redirect_stderr() {
    use std::fs::{self, OpenOptions};
    use std::os::fd::IntoRawFd;

    let log_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("onde");

    if fs::create_dir_all(&log_dir).is_err() {
        return;
    }

    if let Ok(file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("debug.log"))
    {
        // SAFETY: called before tokio or ratatui start, so no threads yet.
        unsafe {
            let fd = file.into_raw_fd();
            libc::dup2(fd, libc::STDERR_FILENO);
            libc::close(fd);
        }
    }
}

#[cfg(not(unix))]
fn redirect_stderr() {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    redirect_stderr();

    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal).await;
    ratatui::restore();
    result
}
