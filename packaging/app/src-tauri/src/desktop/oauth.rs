//! Browser sign-in through ondeinference.com. No Tauri types here, so tests
//! call it directly.
//!
//! onde-web runs the provider round trip with the smbCloud client secret and
//! redirects the browser to our loopback `app_redirect` with
//! `?access_token=…&state=…`, or `?error=…`.
use crate::{
    desktop::auth::{self, Profile},
    error::CommandError,
};
use rand::{Rng, distr::Alphanumeric};
use serde::Deserialize;
use url::Url;

pub const START_BASE: &str = "https://ondeinference.com/api/auth";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Google,
    Github,
    Gitlab,
    Apple,
}

impl Provider {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Apple => "apple",
        }
    }
}

/// 32 random URL-safe characters, used to tie the callback to this attempt.
pub fn new_state() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

pub fn start_url(provider: Provider, redirect: &str, state: &str) -> Url {
    let mut url = Url::parse(&format!("{START_BASE}/{}/start", provider.slug()))
        .expect("START_BASE is a valid URL");
    url.query_pairs_mut()
        .append_pair("env", "production")
        .append_pair("app_redirect", redirect)
        .append_pair("app_state", state);
    url
}

fn interrupted() -> CommandError {
    CommandError::Auth("Sign-in was interrupted. Try again.".into())
}

/// Pulls the token out of the callback, rejecting anything that does not
/// carry this attempt's `state`. Never logs the URL, since it holds the token.
pub fn parse_callback(url: &Url, expected_state: &str) -> Result<String, CommandError> {
    let mut token = None;
    let mut state = None;
    let mut error = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "access_token" if !v.is_empty() => token = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if let Some(msg) = error {
        return Err(CommandError::Auth(msg.chars().take(200).collect()));
    }
    if state.as_deref() != Some(expected_state) {
        return Err(interrupted());
    }
    token.ok_or_else(|| CommandError::Auth("Sign-in did not return a session. Try again.".into()))
}

pub async fn complete(token: &str) -> Result<Profile, CommandError> {
    auth::finish_sign_in(token, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cb(q: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:5000/callback?{q}")).unwrap()
    }

    #[test]
    fn start_url_has_provider_path_and_encoded_params() {
        for p in [
            Provider::Google,
            Provider::Github,
            Provider::Gitlab,
            Provider::Apple,
        ] {
            let u = start_url(p, "http://127.0.0.1:5000/callback", "abc123");
            assert_eq!(u.path(), format!("/api/auth/{}/start", p.slug()));
            let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
            assert_eq!(q["env"], "production");
            assert_eq!(q["app_redirect"], "http://127.0.0.1:5000/callback");
            assert_eq!(q["app_state"], "abc123");
        }
        let u = start_url(Provider::Github, "http://127.0.0.1:5000/callback", "s");
        assert!(
            u.as_str()
                .contains("app_redirect=http%3A%2F%2F127.0.0.1%3A5000%2Fcallback")
        );
    }

    #[test]
    fn new_state_is_long_url_safe_and_unique() {
        let (a, b) = (new_state(), new_state());
        assert!(a.len() >= 16);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }

    #[test]
    fn callback_with_matching_state_gives_the_token() {
        assert_eq!(
            parse_callback(&cb("access_token=tok&state=s1"), "s1").unwrap(),
            "tok"
        );
    }

    #[test]
    fn callback_with_wrong_or_missing_state_is_rejected() {
        let want = Err(interrupted());
        assert_eq!(
            parse_callback(&cb("access_token=tok&state=other"), "s1"),
            want
        );
        assert_eq!(parse_callback(&cb("access_token=tok"), "s1"), want);
    }

    #[test]
    fn callback_error_is_surfaced() {
        assert_eq!(
            parse_callback(&cb("error=access_denied&state=s1"), "s1"),
            Err(CommandError::Auth("access_denied".into()))
        );
    }

    #[test]
    fn callback_without_token_is_rejected() {
        assert!(matches!(
            parse_callback(&cb("state=s1"), "s1"),
            Err(CommandError::Auth(_))
        ));
        assert!(matches!(
            parse_callback(&cb("access_token=&state=s1"), "s1"),
            Err(CommandError::Auth(_))
        ));
    }

    #[test]
    fn provider_names_match_the_typescript_strings() {
        for (s, p) in [
            ("google", Provider::Google),
            ("github", Provider::Github),
            ("gitlab", Provider::Gitlab),
            ("apple", Provider::Apple),
        ] {
            assert_eq!(
                serde_json::from_str::<Provider>(&format!("\"{s}\"")).unwrap(),
                p
            );
            assert_eq!(p.slug(), s);
        }
    }
}
