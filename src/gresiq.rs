//! Onde credentials baked in. Wraps `smbcloud_gresiq_sdk` so callers only
//! need to pass the user's access token.
use smbcloud_gresiq_sdk::{Environment, OndeApp, OndeModel};

pub async fn load_apps(token: &str) -> anyhow::Result<Vec<OndeApp>> {
    smbcloud_gresiq_sdk::list_apps(
        &Environment::Production,
        crate::app::ONDE_APP_ID,
        crate::app::ONDE_APP_SECRET,
        token,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn create_app(token: &str, name: &str) -> anyhow::Result<OndeApp> {
    smbcloud_gresiq_sdk::create_app(
        &Environment::Production,
        crate::app::ONDE_APP_ID,
        crate::app::ONDE_APP_SECRET,
        token,
        name,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn load_models(token: &str) -> anyhow::Result<Vec<OndeModel>> {
    smbcloud_gresiq_sdk::list_models(
        &Environment::Production,
        crate::app::ONDE_APP_ID,
        crate::app::ONDE_APP_SECRET,
        token,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn assign_model(
    token: &str,
    onde_app_id: &str,
    model_id: &str,
) -> anyhow::Result<()> {
    smbcloud_gresiq_sdk::assign_model(
        &Environment::Production,
        crate::app::ONDE_APP_ID,
        crate::app::ONDE_APP_SECRET,
        token,
        onde_app_id,
        model_id,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn rename_app(token: &str, onde_app_id: &str, new_name: &str) -> anyhow::Result<OndeApp> {
    smbcloud_gresiq_sdk::rename_app(
        &Environment::Production,
        crate::app::ONDE_APP_ID,
        crate::app::ONDE_APP_SECRET,
        token,
        onde_app_id,
        new_name,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}
