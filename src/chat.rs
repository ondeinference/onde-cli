//! Background GGUF chat worker for the onde-cli TUI.
//!
//! Loads a local GGUF file from an absolute path on disk and handles
//! multi-turn chat via tokio mpsc channels. No HuggingFace Hub access is
//! required — the parent directory of the file is used as the model root,
//! and `TokenSource::None` bypasses all HF authentication.

use std::path::PathBuf;
use std::time::Instant;

use onde::mistralrs::{GgufModelBuilder, RequestBuilder, Response, TextMessageRole, TokenSource};
use tokio::sync::mpsc;

// ── Public types ──────────────────────────────────────────────────────────────

/// Progress events emitted by the background chat worker.
#[derive(Debug, Clone)]
pub enum ChatProgress {
    /// The worker has started loading the model from disk.
    LoadingModel,
    /// The model is loaded and ready to accept messages.
    Ready { model_name: String },
    /// Inference is running; waiting for the reply.
    Thinking,
    /// A complete assistant reply has been produced.
    Reply {
        _text: String,
        duration_display: String,
    },
    /// A partial token arrived from the streaming response.
    StreamDelta(String),
    /// An unrecoverable error occurred. The worker exits after sending this.
    Error(String),
}

/// A single turn in the conversation history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

/// The role of a participant in the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
}

/// Commands sent to the background chat worker.
#[derive(Debug)]
pub enum ChatCommand {
    /// Send a user message and await a reply.
    SendMessage(String),
    /// Shut down the worker cleanly.
    Quit,
}

// ── Duration formatting ───────────────────────────────────────────────────────

/// Format an elapsed [`std::time::Duration`] as a compact human-readable string.
///
/// - Under 60 s  → `"4.2s"`
/// - 60 s or more → `"1m 4.2s"`
fn format_duration(elapsed: std::time::Duration) -> String {
    let total_secs = elapsed.as_secs_f64();
    let mins = (total_secs / 60.0).floor() as u64;
    let secs = total_secs - (mins as f64 * 60.0);
    if mins > 0 {
        format!("{}m {:.1}s", mins, secs)
    } else {
        format!("{:.1}s", total_secs)
    }
}

// ── History helpers ───────────────────────────────────────────────────────────

/// Map a [`ChatRole`] to the corresponding [`TextMessageRole`] expected by
/// `mistralrs`.
#[cfg(test)]
fn to_mistral_role(role: &ChatRole) -> TextMessageRole {
    match role {
        ChatRole::User => TextMessageRole::User,
        ChatRole::Assistant => TextMessageRole::Assistant,
    }
}

/// Build a [`RequestBuilder`] from the accumulated history plus the new user
/// message.  A fixed sampling preset (temperature 0.7, max 512 tokens) is
/// applied so callers do not need to thread sampling config through the
/// command channel.
fn build_request(history: &[(TextMessageRole, String)], user_message: &str) -> RequestBuilder {
    let mut req = RequestBuilder::new()
        .set_sampler_temperature(0.7)
        .set_sampler_max_len(512);

    for (role, content) in history {
        req = req.add_message(role.clone(), content);
    }

    req = req.add_message(TextMessageRole::User, user_message);
    req
}

// ── Worker entry point ────────────────────────────────────────────────────────

/// Start the background GGUF chat worker.
///
/// The worker:
/// 1. Sends [`ChatProgress::LoadingModel`].
/// 2. Loads the GGUF file at `gguf_path` using `mistralrs::GgufModelBuilder`.
///    The parent directory is used as the model root; no HF token is required.
/// 3. Sends [`ChatProgress::Ready`] on success, or [`ChatProgress::Error`] and
///    returns on failure.
/// 4. Loops on `command_rx`:
///    - [`ChatCommand::SendMessage`] → runs inference and sends
///      [`ChatProgress::Thinking`] then [`ChatProgress::Reply`] (or
///      [`ChatProgress::Error`] on failure).
///    - [`ChatCommand::Quit`] → breaks the loop and returns.
///
/// Conversation history is kept in-memory as a `Vec<(TextMessageRole, String)>`
/// so every request includes the full multi-turn context.
pub async fn start_chat(
    gguf_path: PathBuf,
    progress_tx: mpsc::UnboundedSender<ChatProgress>,
    mut command_rx: mpsc::UnboundedReceiver<ChatCommand>,
) {
    // ── 1. Announce that loading has started ─────────────────────────────────
    let _ = progress_tx.send(ChatProgress::LoadingModel);

    // ── 2. Derive model_id (parent dir) and filename from the absolute path ──
    let model_dir = match gguf_path.parent() {
        Some(p) => p.to_string_lossy().to_string(),
        None => {
            let _ = progress_tx.send(ChatProgress::Error(format!(
                "Cannot determine parent directory of: {}",
                gguf_path.display()
            )));
            return;
        }
    };

    let file_name = match gguf_path.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => {
            let _ = progress_tx.send(ChatProgress::Error(format!(
                "Cannot determine file name of: {}",
                gguf_path.display()
            )));
            return;
        }
    };

    // ── 3. Build and load the model ──────────────────────────────────────────
    let model = match GgufModelBuilder::new(&model_dir, vec![file_name.clone()])
        .with_token_source(TokenSource::None)
        .build()
        .await
    {
        Ok(m) => m,
        Err(e) => {
            let _ = progress_tx.send(ChatProgress::Error(format!(
                "Failed to load model \"{file_name}\": {e}"
            )));
            return;
        }
    };

    let _ = progress_tx.send(ChatProgress::Ready {
        model_name: file_name,
    });

    // ── 4. Conversation history (mistralrs roles + content) ──────────────────
    //
    // We store the history as raw (TextMessageRole, String) pairs rather than
    // as `ChatMessage` values so we can hand slices directly to `build_request`
    // without any extra mapping at inference time.
    let mut history: Vec<(TextMessageRole, String)> = Vec::new();

    // ── 5. Command loop ───────────────────────────────────────────────────────
    loop {
        let command = match command_rx.recv().await {
            Some(cmd) => cmd,
            // Sender dropped — treat as a quit signal.
            None => break,
        };

        match command {
            ChatCommand::Quit => break,

            ChatCommand::SendMessage(user_text) => {
                let _ = progress_tx.send(ChatProgress::Thinking);

                let request = build_request(&history, &user_text);

                let start = Instant::now();
                let mut stream = match model.stream_chat_request(request).await {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = progress_tx.send(ChatProgress::Error(format!(
                            "Failed to start streaming: {e}"
                        )));
                        continue;
                    }
                };

                let mut reply_buf = String::new();

                while let Some(response) = stream.next().await {
                    match response {
                        Response::Chunk(chunk) => {
                            if let Some(choice) = chunk.choices.first()
                                && let Some(ref delta) = choice.delta.content
                            {
                                reply_buf.push_str(delta);
                                let _ = progress_tx.send(ChatProgress::StreamDelta(delta.clone()));
                            }
                        }
                        Response::Done(_) => break,
                        Response::ModelError(msg, _) => {
                            let _ = progress_tx
                                .send(ChatProgress::Error(format!("Model error: {msg}")));
                            break;
                        }
                        Response::InternalError(e) => {
                            let _ = progress_tx
                                .send(ChatProgress::Error(format!("Internal error: {e}")));
                            break;
                        }
                        Response::ValidationError(e) => {
                            let _ = progress_tx
                                .send(ChatProgress::Error(format!("Validation error: {e}")));
                            break;
                        }
                        Response::CompletionModelError(msg, _) => {
                            let _ = progress_tx
                                .send(ChatProgress::Error(format!("Completion error: {msg}")));
                            break;
                        }
                        Response::CompletionDone(_) => break,
                        Response::CompletionChunk(_) => {}
                        Response::ImageGeneration(_) => {}
                        Response::Speech { .. } => {}
                        Response::Raw { .. } => {}
                        Response::Embeddings { .. } => {}
                    }
                }

                let elapsed = start.elapsed();

                if !reply_buf.is_empty() {
                    history.push((TextMessageRole::User, user_text));
                    history.push((TextMessageRole::Assistant, reply_buf.trim().to_string()));

                    let _ = progress_tx.send(ChatProgress::Reply {
                        _text: reply_buf.trim().to_string(),
                        duration_display: format_duration(elapsed),
                    });
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_under_one_minute() {
        let d = std::time::Duration::from_secs_f64(4.567);
        assert_eq!(format_duration(d), "4.6s");
    }

    #[test]
    fn format_duration_exactly_one_minute() {
        let d = std::time::Duration::from_secs(60);
        assert_eq!(format_duration(d), "1m 0.0s");
    }

    #[test]
    fn format_duration_over_one_minute() {
        let d = std::time::Duration::from_secs_f64(125.3);
        assert_eq!(format_duration(d), "2m 5.3s");
    }

    #[test]
    fn chat_message_user_role() {
        let msg = ChatMessage::user("hello");
        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.content, "hello");
    }

    #[test]
    fn chat_message_assistant_role() {
        let msg = ChatMessage::assistant("hi there");
        assert_eq!(msg.role, ChatRole::Assistant);
        assert_eq!(msg.content, "hi there");
    }

    #[test]
    fn to_mistral_role_maps_correctly() {
        assert!(matches!(
            to_mistral_role(&ChatRole::User),
            TextMessageRole::User
        ));
        assert!(matches!(
            to_mistral_role(&ChatRole::Assistant),
            TextMessageRole::Assistant
        ));
    }
}
