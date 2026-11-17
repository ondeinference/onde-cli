use crate::{
    desktop::{
        auth::Profile,
        oauth::{self, Provider},
    },
    error::CommandError,
    state::PendingSignIn,
};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, State};
use tauri_plugin_oauth::{OauthConfig, cancel, start_with_config};
use tauri_plugin_opener::OpenerExt;
use tokio::{sync::oneshot, time::timeout};

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

const RESPONSE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Signed in to Onde Desktop</title>
<style>
  body { margin: 0; min-height: 100vh; display: flex; align-items: center; justify-content: center;
         background: #fff; color: #111; font: 16px/1.5 -apple-system, system-ui, sans-serif; text-align: center; }
  @media (prefers-color-scheme: dark) { body { background: #0b0d10; color: #e5e7eb; } }
  h1 { font-size: 1.25rem; margin: 0 0 .25rem; }
  p { margin: 0; opacity: .7; }
</style>
</head>
<body>
<main>
  <h1>You're signed in to Onde Desktop</h1>
  <p>You can close this tab.</p>
</main>
</body>
</html>"#;

fn internal(msg: &str) -> CommandError {
    CommandError::Internal(msg.into())
}

#[tauri::command]
pub async fn oauth_sign_in(
    app: AppHandle,
    pending: State<'_, PendingSignIn>,
    provider: Provider,
) -> Result<Profile, CommandError> {
    let state = oauth::new_state();
    let cancelled = pending.begin();

    // The listener calls back for every request it sees; only the callback
    // path is forwarded, and only the first one.
    let (tx, rx) = oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    let port = start_with_config(
        OauthConfig {
            ports: None,
            response: Some(RESPONSE.into()),
            redirect_uri: None,
        },
        move |url| {
            if let Ok(parsed) = url::Url::parse(&url)
                && parsed.path() == "/callback"
                && let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                let _ = tx.send(url);
            }
        },
    )
    .map_err(|_| internal("Couldn't start the local sign-in listener."))?;

    let result = wait_for_callback(&app, provider, port, &state, rx, cancelled).await;
    // Free the port on every exit path. Fails harmlessly if it already closed.
    let _ = cancel(port);

    oauth::complete(&result?).await
}

async fn wait_for_callback(
    app: &AppHandle,
    provider: Provider,
    port: u16,
    state: &str,
    rx: oneshot::Receiver<String>,
    cancelled: oneshot::Receiver<()>,
) -> Result<String, CommandError> {
    let start = oauth::start_url(
        provider,
        &format!("http://127.0.0.1:{port}/callback"),
        state,
    );
    app.opener()
        .open_url(start.as_str(), None::<&str>)
        .map_err(|_| internal("Couldn't open your browser."))?;

    let url = tokio::select! {
        _ = cancelled => return Err(CommandError::Cancelled("Sign-in cancelled.".into())),
        r = timeout(SIGN_IN_TIMEOUT, rx) => match r {
            Err(_) => return Err(CommandError::TimedOut("Sign-in timed out. Try again.".into())),
            Ok(Err(_)) => return Err(CommandError::Cancelled("Sign-in cancelled.".into())),
            Ok(Ok(url)) => url,
        },
    };
    let parsed =
        url::Url::parse(&url).map_err(|_| internal("Sign-in returned an unreadable address."))?;
    oauth::parse_callback(&parsed, state)
}

#[tauri::command]
pub fn oauth_sign_in_cancel(pending: State<'_, PendingSignIn>) {
    pending.cancel();
}
