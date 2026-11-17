use serde::Serialize;

/// What a command returns to the webview on failure. `message` is worded for
/// the user; `kind` lets the UI branch without parsing text.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum CommandError {
    EmailNotConfirmed(String),
    UnknownEmail(String),
    Auth(String),
    /// The user backed out. The UI stays quiet.
    Cancelled(String),
    /// The browser round trip took too long. The UI stays quiet.
    TimedOut(String),
    Internal(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (Self::EmailNotConfirmed(m)
        | Self::UnknownEmail(m)
        | Self::Auth(m)
        | Self::Cancelled(m)
        | Self::TimedOut(m)
        | Self::Internal(m)) = self;
        f.write_str(m)
    }
}

impl std::error::Error for CommandError {}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_kind_and_message() {
        let json = serde_json::to_value(CommandError::UnknownEmail("nope".into())).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "unknown_email", "message": "nope"})
        );
    }
}
