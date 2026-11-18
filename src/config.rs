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

/// What a cloud feature says when this build carries no Onde auth credentials:
/// a source build made without a `.env`, or `cargo install`. Sign-in is off,
/// and with no way to sign in, publishing and app assignment are too.
pub const NOT_CONNECTED: &str = "This build has no Onde credentials, so cloud sign-in, publishing and app assignment are off. Fine-tuning, merging, export and eval still work. Official releases include the credentials.";

/// What the apps-and-models features say when sign-in works (the Onde auth
/// credentials are present) but the GresIQ keys are missing. Reaching this
/// path means the user already signed in, so it does not claim sign-in is off.
pub const NOT_CONNECTED_GRESIQ: &str = "This build has no GresIQ credentials, so publishing and app assignment are off. Sign-in and the local tools still work. Official releases include the credentials.";

fn connected(app_id: &str, app_secret: &str) -> bool {
    !app_id.is_empty() && !app_secret.is_empty()
}

/// True when this build can talk to Onde's auth backend.
pub fn is_connected() -> bool {
    connected(ONDE_APP_ID, ONDE_APP_SECRET)
}

/// True when this build can talk to GresIQ (apps and models).
pub fn gresiq_connected() -> bool {
    connected(GRESIQ_API_KEY, GRESIQ_API_SECRET)
}

/// The Auth client credentials identifying this build to the backend.
pub fn credentials() -> ClientCredentials<'static> {
    ClientCredentials {
        app_id: ONDE_APP_ID,
        app_secret: ONDE_APP_SECRET,
    }
}

#[cfg(test)]
mod tests {
    use super::connected;

    #[test]
    fn a_build_is_connected_only_with_both_values() {
        assert!(connected("id", "secret"));
        assert!(!connected("", "secret"));
        assert!(!connected("id", ""));
        assert!(!connected("", ""));
    }
}
