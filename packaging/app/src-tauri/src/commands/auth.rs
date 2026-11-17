use crate::{
    desktop::auth::{self, Profile},
    error::CommandError,
};

#[tauri::command]
pub async fn current_user() -> Option<Profile> {
    auth::restore().await
}

#[tauri::command]
pub async fn sign_in(email: String, password: String) -> Result<Profile, CommandError> {
    auth::sign_in(email, password).await
}

#[tauri::command]
pub async fn sign_up(email: String, password: String) -> Result<String, CommandError> {
    auth::sign_up(email, password).await
}

#[tauri::command]
pub async fn sign_out() {
    auth::sign_out().await
}
