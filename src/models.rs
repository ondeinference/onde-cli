//! Finding base models on disk.
//!
//! Downloads land in the Onde App Group's Hugging Face cache; a stock
//! `huggingface-cli` cache is also searched, so a model fetched either way is
//! found.

use std::path::{Path, PathBuf};

/// Hugging Face hub directories to search, most preferred first: the App Group
/// (where Onde downloads), `$HF_HOME/hub`, then `~/.cache/huggingface/hub`.
pub fn hub_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // The real home, not `$HOME`: inside the macOS sandbox `$HOME` points into
    // the app's own container, which has no Group Containers directory.
    #[cfg(target_os = "macos")]
    if let Some(home) = crate::paths::real_home() {
        dirs.push(
            home.join("Library")
                .join("Group Containers")
                .join("group.com.ondeinference.apps")
                .join("models")
                .join("hub"),
        );
    }
    if let Ok(hf_home) = std::env::var("HF_HOME") {
        dirs.push(PathBuf::from(hf_home).join("hub"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".cache").join("huggingface").join("hub"));
    }
    dirs
}

/// The local snapshot directory for a Hub repo id such as `Qwen/Qwen3-0.6B`, or
/// `None` when it is not downloaded.
pub fn resolve_snapshot(repo_id: &str) -> Option<PathBuf> {
    hub_dirs().iter().find_map(|hub| snapshot_in(hub, repo_id))
}

/// `{hub}/models--{org}--{name}/snapshots/{commit}` for the first commit
/// directory. Directories that are not a hex commit id are skipped: fine-tuning
/// creates siblings such as `lora-adapter` that are not model snapshots.
fn snapshot_in(hub: &Path, repo_id: &str) -> Option<PathBuf> {
    let snapshots = hub
        .join(format!("models--{}", repo_id.replace('/', "--")))
        .join("snapshots");
    std::fs::read_dir(snapshots)
        .ok()?
        .flatten()
        .find(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            entry.path().is_dir() && name.len() >= 7 && name.chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_commit_directory_and_skips_other_names() {
        let tmp = tempfile::tempdir().unwrap();
        let snapshots = tmp.path().join("models--Qwen--Qwen3-0.6B/snapshots");
        std::fs::create_dir_all(snapshots.join("lora-adapter")).unwrap();
        std::fs::create_dir_all(snapshots.join("c1899de289a04d12100db370d81485cdf75e47ca"))
            .unwrap();
        assert_eq!(
            snapshot_in(tmp.path(), "Qwen/Qwen3-0.6B"),
            Some(snapshots.join("c1899de289a04d12100db370d81485cdf75e47ca"))
        );
    }

    #[test]
    fn a_model_that_is_not_downloaded_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(snapshot_in(tmp.path(), "Qwen/Qwen3-0.6B"), None);
    }
}
