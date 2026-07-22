//! MCP (Model Context Protocol) server interface.
//!
//! When `onde` is started with `--mcp`, it runs as an MCP server over stdio
//! (see <https://modelcontextprotocol.io/specification/draft/basic/transports/stdio>)
//! instead of launching the TUI. The server exposes the same Onde account and
//! model-catalog operations the TUI drives — listing/creating apps, browsing the
//! model catalog, registering a fine-tuned model, and assigning a model to an
//! app — as MCP tools built on the official `rmcp` SDK.
//!
//! Tools call the same library functions the TUI uses (`crate::gresiq`,
//! `smbcloud_auth_sdk`) but return structured JSON rather than rendering the
//! ratatui screens. stdout is the JSON-RPC channel and must stay free of console
//! output, so — unlike the TUI path — `main` deliberately does **not** redirect
//! the process's stdout/stderr when `--mcp` is active.
//!
//! Authentication reuses the token persisted by a TUI sign-in (or the `login`
//! tool below), stored at the same path `crate::token` reads, so an MCP session
//! and a terminal session share credentials.

use {
    crate::{gresiq, token},
    anyhow::{Result, anyhow},
    rmcp::{
        ErrorData, ServiceExt,
        handler::server::{ServerHandler, wrapper::Parameters},
        model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
        tool, tool_handler, tool_router,
        transport::stdio,
    },
    schemars::JsonSchema,
    serde::Deserialize,
    smbcloud_auth_sdk::{
        client_credentials::ClientCredentials, login::login_with_client,
        logout::logout_with_client, me::me_with_client,
    },
    smbcloud_model::login::AccountStatus,
    smbcloud_network::environment::Environment,
};

/// The Onde MCP server. Onde only talks to the Production API, so there is no
/// per-session state to hold; the `rmcp` `#[tool_router]`/`#[tool_handler]`
/// macros carry the tool routing.
pub struct OndeMcpServer;

/// The baked-in Auth client credentials identifying the CLI to the backend.
fn credentials() -> ClientCredentials<'static> {
    ClientCredentials {
        app_id: crate::app::ONDE_APP_ID,
        app_secret: crate::app::ONDE_APP_SECRET,
    }
}

impl OndeMcpServer {
    pub fn new() -> Self {
        Self
    }

    /// Resolve the stored auth token, mapping "not logged in" to an MCP error.
    /// Every tool that hits the account API goes through this.
    fn access_token(&self) -> Result<String, ErrorData> {
        token::load().ok_or_else(|| {
            ErrorData::invalid_request(
                "Not logged in. Sign in with the `login` tool or run `onde` and sign in.",
                None,
            )
        })
    }
}

impl Default for OndeMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Serialize any JSON-able value into a single-content successful tool result.
fn json_result<T: serde::Serialize>(value: &T) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::json(value)?]))
}

fn text_result(message: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        message.into(),
    )]))
}

fn to_error_data(error: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

// ── Tool argument schemas ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
struct LoginArgs {
    /// Account email address.
    email: String,
    /// Account password.
    password: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AppCreateArgs {
    /// Name for the new Onde app.
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AppRenameArgs {
    /// The Onde app ID to rename.
    app_id: String,
    /// The new name for the app.
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ModelRegisterArgs {
    /// Hugging Face repo ID the GGUF was uploaded to (e.g. `user/model-gguf`).
    hf_repo_id: String,
    /// Display name for the catalog entry.
    name: String,
    /// Model family (e.g. `qwen3`, `qwen2.5`).
    family: String,
    /// Parameter class label (e.g. `0.6B`, `3B`).
    parameter_class: String,
    /// Specific GGUF filename within the repo, if the repo has several.
    #[serde(default)]
    gguf_file: Option<String>,
    /// Approximate on-disk size of the GGUF in bytes, if known.
    #[serde(default)]
    approx_size_bytes: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ModelAssignArgs {
    /// The Onde app ID to assign the model to.
    app_id: String,
    /// The catalog model ID to assign.
    model_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HfSearchArgs {
    /// Search query for the Hugging Face Hub (up to 20 results).
    query: String,
}

// ── Tools ────────────────────────────────────────────────────────────────────

#[tool_router]
impl OndeMcpServer {
    #[tool(
        description = "Sign in to Onde with email and password and persist the access token \
                       to the same location the TUI uses. Returns the signed-in user as JSON."
    )]
    async fn login(
        &self,
        Parameters(args): Parameters<LoginArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let status = login_with_client(
            Environment::Production,
            credentials(),
            args.email,
            args.password,
        )
        .await
        .map_err(|e| ErrorData::invalid_request(format!("{e:?}"), None))?;

        match status {
            AccountStatus::Ready { access_token } => {
                token::save(&access_token).map_err(to_error_data)?;
                let user = me_with_client(Environment::Production, credentials(), &access_token)
                    .await
                    .map_err(|e| to_error_data(format!("{e:?}")))?;
                json_result(&user)
            }
            AccountStatus::Incomplete { .. } => Err(ErrorData::invalid_request(
                "Check your email first — confirm your account before signing in.",
                None,
            )),
            AccountStatus::NotFound => Err(ErrorData::invalid_request(
                "That email isn't in our system.",
                None,
            )),
        }
    }

    #[tool(description = "Sign out: revoke and clear the stored Onde access token.")]
    async fn logout(&self) -> Result<CallToolResult, ErrorData> {
        if let Some(t) = token::load() {
            let _ = logout_with_client(Environment::Production, credentials(), t).await;
        }
        token::clear();
        text_result("Signed out.")
    }

    #[tool(
        description = "Get the authenticated Onde user's account info. Requires a prior \
                       `login`; returns the user as JSON."
    )]
    async fn me(&self) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        let user = me_with_client(Environment::Production, credentials(), &token)
            .await
            .map_err(|e| to_error_data(format!("{e:?}")))?;
        json_result(&user)
    }

    #[tool(description = "List the authenticated user's Onde apps as a JSON array.")]
    async fn apps_list(&self) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        let apps = gresiq::load_apps(&token).await.map_err(to_error_data)?;
        json_result(&apps)
    }

    #[tool(
        description = "Create a new Onde app with the given name. Returns the created app as JSON."
    )]
    async fn app_create(
        &self,
        Parameters(args): Parameters<AppCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        let app = gresiq::create_app(&token, &args.name)
            .await
            .map_err(to_error_data)?;
        json_result(&app)
    }

    #[tool(description = "Rename an Onde app by ID. Returns the updated app as JSON.")]
    async fn app_rename(
        &self,
        Parameters(args): Parameters<AppRenameArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        let app = gresiq::rename_app(&token, &args.app_id, &args.name)
            .await
            .map_err(to_error_data)?;
        json_result(&app)
    }

    #[tool(
        description = "List the Onde model catalog (including this user's custom models) as a JSON array."
    )]
    async fn models_list(&self) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        let models = gresiq::load_models(&token).await.map_err(to_error_data)?;
        json_result(&models)
    }

    #[tool(
        description = "Register a fine-tuned GGUF model (already uploaded to Hugging Face) into \
                       the catalog, private to this account, so it can be assigned to an app. \
                       Returns the created catalog model as JSON."
    )]
    async fn model_register(
        &self,
        Parameters(args): Parameters<ModelRegisterArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        let model = gresiq::create_model(
            &token,
            &args.hf_repo_id,
            &args.name,
            &args.family,
            &args.parameter_class,
            args.gguf_file.as_deref(),
            args.approx_size_bytes,
        )
        .await
        .map_err(to_error_data)?;
        json_result(&model)
    }

    #[tool(
        description = "Assign a catalog model to an Onde app by their IDs. The end app then \
                       loads this model through the Onde SDK."
    )]
    async fn model_assign(
        &self,
        Parameters(args): Parameters<ModelAssignArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = self.access_token()?;
        gresiq::assign_model(&token, &args.app_id, &args.model_id)
            .await
            .map_err(to_error_data)?;
        text_result("Model assigned.")
    }

    #[tool(
        description = "Search the public Hugging Face Hub for models matching a query \
                       (up to 20 results). Returns compact model info as a JSON array. \
                       No Onde login required."
    )]
    async fn hf_search(
        &self,
        Parameters(args): Parameters<HfSearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let results = crate::hf_search::search_hf(&args.query)
            .await
            .map_err(to_error_data)?;
        json_result(&results)
    }
}

#[tool_handler]
impl ServerHandler for OndeMcpServer {
    fn get_info(&self) -> ServerInfo {
        // `Implementation` is `#[non_exhaustive]`, so start from the build-env
        // default and override the identity fields to report `onde`, not `rmcp`.
        let mut server_info = Implementation::from_build_env();
        server_info.name = "onde".to_string();
        server_info.version = env!("CARGO_PKG_VERSION").to_string();

        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info)
            .with_instructions(
                "Onde CLI exposed as MCP tools. Authentication uses the token stored by the TUI \
                 sign-in or the `login` tool; tools run non-interactively and return JSON. Typical \
                 flow: `login` → `apps_list` / `models_list` → `model_register` (after uploading a \
                 GGUF to Hugging Face) → `model_assign`.",
            )
    }
}

/// Run the MCP server over stdio until the client disconnects.
pub async fn serve() -> Result<()> {
    let running = OndeMcpServer::new()
        .serve(stdio())
        .await
        .map_err(|e| anyhow!("Failed to start MCP server: {e}"))?;
    running
        .waiting()
        .await
        .map_err(|e| anyhow!("MCP server stopped unexpectedly: {e}"))?;
    Ok(())
}
