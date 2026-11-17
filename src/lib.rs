//! The Onde pipeline as a library: sign in, model library, fine-tune, merge,
//! GGUF export, local chat, Hugging Face upload and app assignment.
//!
//! The `onde` binary (the `tui` feature) and desktop hosts such as Onde Studio
//! both build on these modules. Long jobs report progress over
//! `tokio::sync::mpsc` channels and own their threads; nothing here draws UI.

pub mod chat;
pub mod config;
pub mod finetune;
pub mod gguf;
pub mod gresiq;
pub mod hf;
pub mod hf_clone;
pub mod hf_search;
pub mod hf_upload;
pub mod merge;
pub mod paths;
pub mod project;
pub mod token;
