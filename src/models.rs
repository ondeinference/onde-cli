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

/// Every `{hub}/models--{org}--{name}/snapshots/{commit}` directory. Directories
/// that are not a hex commit id are skipped: fine-tuning creates siblings such
/// as `lora-adapter` that are not model snapshots.
fn snapshots(hub: &Path, repo_id: &str) -> Vec<PathBuf> {
    let dir = hub
        .join(format!("models--{}", repo_id.replace('/', "--")))
        .join("snapshots");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            entry.path().is_dir() && name.len() >= 7 && name.chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

fn snapshot_in(hub: &Path, repo_id: &str) -> Option<PathBuf> {
    snapshots(hub, repo_id).into_iter().next()
}

/// True when `dir` holds a base model the trainer can load: `config.json`,
/// `tokenizer.json`, and all the weights, which is either one
/// `model.safetensors` or every shard that `model.safetensors.index.json` names.
/// A download that was cancelled or is still running fails this, because a
/// file only gets its final name once it is whole.
pub fn is_complete(dir: &Path) -> bool {
    if !dir.join("config.json").is_file() || !dir.join("tokenizer.json").is_file() {
        return false;
    }
    if dir.join("model.safetensors").is_file() {
        return true;
    }
    let Ok(raw) = std::fs::read_to_string(dir.join("model.safetensors.index.json")) else {
        return false;
    };
    let Ok(index) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(map) = index.get("weight_map").and_then(|m| m.as_object()) else {
        return false;
    };
    let shards: std::collections::HashSet<&str> = map.values().filter_map(|v| v.as_str()).collect();
    !shards.is_empty() && shards.iter().all(|f| dir.join(f).is_file())
}

/// `{base_root}/{name}` for a repo id such as `Qwen/Qwen3-1.7B`, when that folder
/// holds a complete model. This is where models fetched by hand live, next to
/// the projects, as in `~/.onde/base/Qwen3-1.7B`.
fn base_in(base_root: &Path, repo_id: &str) -> Option<PathBuf> {
    let name = repo_id.rsplit('/').next().filter(|n| !n.is_empty())?;
    let dir = base_root.join(name);
    is_complete(&dir).then_some(dir)
}

/// Where a complete copy of the base model `repo_id` is on this machine, or
/// `None`. A Hugging Face cache snapshot wins, then a folder under `base/` next
/// to the projects folder.
pub fn base_model_dir(repo_id: &str) -> Option<PathBuf> {
    hub_dirs()
        .iter()
        .flat_map(|hub| snapshots(hub, repo_id))
        .find(|dir| is_complete(dir))
        .or_else(|| {
            let root = crate::paths::projects_dir();
            base_in(&root.parent()?.join("base"), repo_id)
        })
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

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn single(dir: &Path) {
        write(&dir.join("config.json"), "{}");
        write(&dir.join("tokenizer.json"), "{}");
        write(&dir.join("model.safetensors"), "w");
    }

    #[test]
    fn a_single_file_model_is_complete_only_with_config_tokenizer_and_weights() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("m");
        single(&dir);
        assert!(is_complete(&dir));
        std::fs::remove_file(dir.join("tokenizer.json")).unwrap();
        assert!(!is_complete(&dir));
        assert!(!is_complete(&tmp.path().join("missing")));
    }

    #[test]
    fn a_sharded_model_needs_every_shard() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("m");
        write(&dir.join("config.json"), "{}");
        write(&dir.join("tokenizer.json"), "{}");
        write(
            &dir.join("model.safetensors.index.json"),
            r#"{"weight_map":{"a":"s-1.safetensors","b":"s-2.safetensors","c":"s-1.safetensors"}}"#,
        );
        write(&dir.join("s-1.safetensors"), "w");
        assert!(!is_complete(&dir), "shard 2 is missing");
        write(&dir.join("s-2.safetensors"), "w");
        assert!(is_complete(&dir));
        // A half-downloaded shard still has its .tmp name, not the real one.
        std::fs::remove_file(dir.join("s-2.safetensors")).unwrap();
        write(&dir.join("s-2.safetensors.tmp"), "w");
        assert!(!is_complete(&dir));
    }

    #[test]
    fn a_base_folder_next_to_the_projects_is_found_by_the_name_part() {
        let tmp = tempfile::tempdir().unwrap();
        single(&tmp.path().join("Qwen3-1.7B"));
        assert_eq!(
            base_in(tmp.path(), "Qwen/Qwen3-1.7B"),
            Some(tmp.path().join("Qwen3-1.7B"))
        );
        assert_eq!(base_in(tmp.path(), "Qwen/Qwen3-4B"), None);
        // An incomplete folder does not count.
        write(&tmp.path().join("Qwen3-4B/config.json"), "{}");
        assert_eq!(base_in(tmp.path(), "Qwen/Qwen3-4B"), None);
    }

    #[test]
    fn snapshots_skip_non_commit_directories_and_come_back_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("models--Qwen--Qwen3-0.6B/snapshots");
        std::fs::create_dir_all(root.join("lora-adapter")).unwrap();
        std::fs::create_dir_all(root.join("bbbbbbbbbb")).unwrap();
        std::fs::create_dir_all(root.join("aaaaaaaaaa")).unwrap();
        assert_eq!(
            snapshots(tmp.path(), "Qwen/Qwen3-0.6B"),
            vec![root.join("aaaaaaaaaa"), root.join("bbbbbbbbbb")]
        );
    }

    /// Start the real download of the smallest catalog model into a temporary
    /// hub, stop it as soon as the weights file begins, and check the half
    /// finished model is not reported as complete. Needs the network.
    #[test]
    #[ignore = "downloads from huggingface.co"]
    fn a_cancelled_download_is_not_a_complete_model() {
        use crate::hf_search::DownloadEvent;

        let hub = tempfile::tempdir().unwrap();
        let repo = "Qwen/Qwen3-0.6B";
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let task = tokio::spawn(crate::hf_search::download_model(
                repo.to_string(),
                hub.path().to_path_buf(),
                tx,
            ));
            while let Some(event) = rx.recv().await {
                match event {
                    DownloadEvent::Progress(p) if p.filename == "model.safetensors" => {
                        task.abort();
                        break;
                    }
                    DownloadEvent::Failed(e) => panic!("download failed: {e}"),
                    DownloadEvent::Complete => panic!("finished before the weights began"),
                    _ => {}
                }
            }
            let _ = task.await;
        });
        let snapshot = snapshot_in(hub.path(), repo).expect("a snapshot directory exists");
        assert!(
            snapshot.join("config.json").is_file(),
            "small files finished"
        );
        assert!(
            snapshot.join("model.safetensors.tmp").exists()
                && !snapshot.join("model.safetensors").exists(),
            "the weights are still a .tmp file"
        );
        assert!(!is_complete(&snapshot));
    }
}
