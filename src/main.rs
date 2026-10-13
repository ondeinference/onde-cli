mod app;
mod chat;
mod finetune;
mod gguf;
mod gresiq;
mod hf;
mod hf_clone;
mod hf_search;
mod hf_upload;
mod mcp;
mod merge;
mod project;
mod token;
mod ui;

use std::io::{IsTerminal, Write};

/// Usage text, printed for `--help`, for bare `onde`, and (to stderr) for
/// unknown arguments.
fn usage() -> String {
    format!(
        "onde {}\n{}\n\n\
         Usage:\n  \
         onde            Print this usage and exit\n  \
         onde --tui      Launch the interactive terminal UI (requires a TTY)\n  \
         onde --mcp      Run the MCP JSON-RPC server over stdio\n  \
         onde --version  Print the version and exit\n  \
         onde --help     Print this help and exit",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_DESCRIPTION"),
    )
}

/// Redirect **both** stdout and stderr to `~/.cache/onde/debug.log` before
/// ratatui takes over the alternate screen.
///
/// mistral.rs writes to both file descriptors:
///   - `stderr` — tracing/log output from `mistralrs_core`
///   - `stdout` — `println!` in `print_metadata`, pipeline info, etc.
///
/// Without redirecting stdout the `println!` calls write raw text into the
/// alternate screen buffer, tearing up the TUI layout.
///
/// We `dup` the real stdout *before* the redirect so ratatui can still
/// render to the terminal via the returned `File` handle.
#[cfg(unix)]
fn redirect_output() -> Option<std::fs::File> {
    use std::fs::{self, OpenOptions};
    use std::os::fd::{AsRawFd, FromRawFd};

    let log_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("onde");

    if fs::create_dir_all(&log_dir).is_err() {
        return None;
    }

    let log_file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("debug.log"))
    {
        Ok(f) => f,
        Err(_) => return None,
    };

    // SAFETY: called before tokio or ratatui start, so no threads yet.
    unsafe {
        // Preserve the real stdout so ratatui can render to the terminal.
        let tty_fd = libc::dup(libc::STDOUT_FILENO);
        if tty_fd < 0 {
            return None;
        }

        let log_fd = log_file.as_raw_fd();

        // Point both stdout and stderr at the log file.
        libc::dup2(log_fd, libc::STDOUT_FILENO);
        libc::dup2(log_fd, libc::STDERR_FILENO);

        Some(std::fs::File::from_raw_fd(tty_fd))
    }
}

#[cfg(not(unix))]
fn redirect_output() -> Option<std::fs::File> {
    None
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // MCP mode runs a JSON-RPC server over stdio instead of the TUI. stdout is
    // the JSON-RPC channel, so — unlike the TUI path below — we must NOT redirect
    // stdout/stderr, and we return before touching the terminal at all.
    if std::env::args().skip(1).any(|arg| arg == "--mcp") {
        return mcp::serve().await;
    }

    // Bare `--version`/`--help` must work without a terminal.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("onde {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", usage());
        return Ok(());
    }

    // Bare `onde` is headless: winget's validator launches the exe with no
    // arguments and expects exit 0, so it must not open the TUI. A real
    // one-shot mode can replace this later.
    if args.is_empty() {
        println!("{}", usage());
        return Ok(());
    }

    // Anything but `--tui` is a typo; don't let it silently succeed.
    if args != ["--tui"] {
        eprintln!("{}", usage());
        std::process::exit(2);
    }

    // The TUI needs a real terminal. When stdout isn't a TTY (`onde --tui | cat`),
    // enabling raw mode / the alternate screen below fails, so print usage and
    // exit 0 instead of launching ratatui into a pipe.
    if !std::io::stdout().is_terminal() {
        println!("{}", usage());
        return Ok(());
    }

    let tty = redirect_output();

    // Build a writer that targets the real terminal.  When
    // redirect_output() moved fd 1 to the log file, `tty` holds
    // the dup'd real terminal fd.  Otherwise we use stdout.
    let mut writer: Box<dyn Write> = match tty {
        Some(f) => Box::new(std::io::BufWriter::new(f)),
        None => Box::new(std::io::stdout()),
    };

    // Enter raw mode + alternate screen.  The control sequences must go
    // to the real terminal writer, not stdout/stderr (which may be
    // redirected to the log file).
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        writer,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(writer);
    let mut terminal = ratatui::Terminal::new(backend)?;
    let result = app::run(&mut terminal).await;

    // Restore terminal state via the backend writer.
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    crossterm::terminal::disable_raw_mode()?;
    terminal.show_cursor()?;

    result
}
