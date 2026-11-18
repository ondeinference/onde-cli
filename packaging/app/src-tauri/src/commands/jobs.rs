use onde_cli::pipeline::RunProgress;
use tauri::{State, ipc::Channel};

use crate::{
    desktop::jobs::{JobInfo, JobRequest, Jobs},
    error::CommandError,
};

/// A sink that forwards each event to the webview. A webview that has gone away
/// makes `send` fail, and the job carries on without it.
fn sink(channel: Channel<RunProgress>) -> crate::desktop::jobs::Sink {
    Box::new(move |event| {
        let _ = channel.send(event);
    })
}

/// Start a census or a training run. Events arrive on `on_event` until the run
/// ends with `done`, `failed` or `cancelled`.
///
/// `async` runs it off the main thread. Checking the project reads and parses
/// the whole dataset, which must not freeze the window.
#[tauri::command(async)]
pub fn start_job(
    request: JobRequest,
    on_event: Channel<RunProgress>,
    jobs: State<'_, Jobs>,
) -> Result<JobInfo, CommandError> {
    jobs.start(&request, sink(on_event))
        .map_err(CommandError::InvalidChoice)
}

/// The job in progress, if any, and from now on its events go to `on_event`.
/// A webview that reloaded mid-run calls this to pick the run back up.
#[tauri::command]
pub fn attach_job(on_event: Channel<RunProgress>, jobs: State<'_, Jobs>) -> Option<JobInfo> {
    jobs.attach(sink(on_event))
}

/// Ask the running job to stop. Training stops after the current step.
#[tauri::command]
pub fn cancel_job(jobs: State<'_, Jobs>) -> bool {
    jobs.cancel()
}
