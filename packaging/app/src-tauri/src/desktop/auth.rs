use crate::error::CommandError;
use onde_cli::{config::credentials, token};
use serde::Serialize;
use smbcloud_auth_sdk::{
    login::login_with_client, logout::logout_with_client, me::me_with_client,
    signup::signup_with_client,
};
use smbcloud_model::{
    error_codes::{ErrorCode, ErrorResponse},
    login::AccountStatus,
};
use smbcloud_network::environment::Environment;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Profile {
    pub email: String,
}

fn error_text(e: &ErrorResponse) -> String {
    match e {
        ErrorResponse::Error { message, .. } => message.clone(),
    }
}

/// The signed-in user, if a saved token is still valid. A rejected token is
/// cleared so the next launch starts signed out.
pub async fn restore() -> Option<Profile> {
    let saved = token::load()?;
    match me_with_client(Environment::Production, credentials(), &saved).await {
        Ok(user) => Some(Profile { email: user.email }),
        // Only a rejected token should sign the user out. A network or parse
        // failure (offline launch, timeout) leaves the token in place so the
        // next launch with connectivity can still use it.
        Err(ErrorResponse::Error {
            error_code: ErrorCode::Unauthorized,
            ..
        }) => {
            token::clear();
            None
        }
        Err(_) => None,
    }
}

/// Saves a fresh bearer token and looks up who it belongs to. Shared by the
/// password and browser sign-ins. `fallback_email` covers a failed lookup.
pub async fn finish_sign_in(
    access_token: &str,
    fallback_email: Option<String>,
) -> Result<Profile, CommandError> {
    token::save(access_token)?;
    match me_with_client(Environment::Production, credentials(), access_token).await {
        Ok(user) => Ok(Profile { email: user.email }),
        Err(e) => match fallback_email {
            Some(email) => Ok(Profile { email }),
            None => {
                token::clear();
                Err(CommandError::Auth(error_text(&e)))
            }
        },
    }
}

pub async fn sign_in(email: String, password: String) -> Result<Profile, CommandError> {
    let status = login_with_client(
        Environment::Production,
        credentials(),
        email.clone(),
        password,
    )
    .await
    .map_err(|e| CommandError::Auth(error_text(&e)))?;

    match status {
        AccountStatus::Ready { access_token } => finish_sign_in(&access_token, Some(email)).await,
        AccountStatus::Incomplete { .. } => Err(CommandError::EmailNotConfirmed(
            "Check your email first. We sent a confirmation link when you signed up.".into(),
        )),
        AccountStatus::NotFound => Err(CommandError::UnknownEmail(
            "That email isn't in our system.".into(),
        )),
    }
}

pub async fn sign_up(email: String, password: String) -> Result<String, CommandError> {
    signup_with_client(Environment::Production, credentials(), email, password)
        .await
        .map(|r| r.message)
        .map_err(|e| CommandError::Auth(error_text(&e)))
}

pub async fn sign_out() {
    if let Some(t) = token::load() {
        let _ = logout_with_client(Environment::Production, credentials(), t).await;
    }
    token::clear();
}
