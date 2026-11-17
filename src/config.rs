//! Credentials baked in at compile time (see `build.rs`).
//!
//! Onde is a tenant in smbCloud Auth. These values identify the client to the
//! backend. Put them in `.env` for local builds, or inject them as CI secrets.

use smbcloud_auth_sdk::client_credentials::ClientCredentials;

const ONDE_APP_ID: &str = env!("ONDE_APP_ID");
const ONDE_APP_SECRET: &str = env!("ONDE_APP_SECRET");
pub const HF_TOKEN: &str = env!("HF_TOKEN");

// GresIQ credentials for the same tenant. These are separate from Auth.
pub(crate) const GRESIQ_API_KEY: &str = env!("GRESIQ_API_KEY");
pub(crate) const GRESIQ_API_SECRET: &str = env!("GRESIQ_API_SECRET");

/// The Auth client credentials identifying this build to the backend.
pub fn credentials() -> ClientCredentials<'static> {
    ClientCredentials {
        app_id: ONDE_APP_ID,
        app_secret: ONDE_APP_SECRET,
    }
}
