//! Onde credentials baked in. Wraps `smbcloud_gresiq_sdk` so callers only
//! need to pass the user's access token.
use smbcloud_gresiq_sdk::{CreateModelParams, Environment, OndeApp, OndeModel};

fn api_key() -> &'static str {
    crate::app::GRESIQ_API_KEY
}

fn api_secret() -> &'static str {
    crate::app::GRESIQ_API_SECRET
}

pub async fn load_apps(token: &str) -> anyhow::Result<Vec<OndeApp>> {
    smbcloud_gresiq_sdk::list_apps(&Environment::Production, api_key(), api_secret(), token)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn create_app(token: &str, name: &str) -> anyhow::Result<OndeApp> {
    smbcloud_gresiq_sdk::create_app(
        &Environment::Production,
        api_key(),
        api_secret(),
        token,
        name,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn load_models(token: &str) -> anyhow::Result<Vec<OndeModel>> {
    smbcloud_gresiq_sdk::list_models(&Environment::Production, api_key(), api_secret(), token)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Registers a fine-tuned model (just uploaded to Hugging Face) into the
/// catalog, private to this account, so it can then be assigned to an app.
#[allow(clippy::too_many_arguments)]
pub async fn create_model(
    token: &str,
    hf_repo_id: &str,
    name: &str,
    family: &str,
    parameter_class: &str,
    gguf_file: Option<&str>,
    approx_size_bytes: Option<i64>,
) -> anyhow::Result<OndeModel> {
    smbcloud_gresiq_sdk::create_model(
        &Environment::Production,
        api_key(),
        api_secret(),
        token,
        CreateModelParams {
            hf_repo_id,
            name,
            org: "Custom",
            family,
            parameter_class,
            format: "gguf",
            gguf_file,
            modality: Some("text"),
            description: Some("Fine-tuned with onde-cli."),
            approx_size_bytes,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn assign_model(token: &str, onde_app_id: &str, model_id: &str) -> anyhow::Result<()> {
    smbcloud_gresiq_sdk::assign_model(
        &Environment::Production,
        api_key(),
        api_secret(),
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
        api_key(),
        api_secret(),
        token,
        onde_app_id,
        new_name,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}
