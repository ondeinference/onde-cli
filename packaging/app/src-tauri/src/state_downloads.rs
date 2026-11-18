//! The base-model downloads in flight, so one can be cancelled by name.

use std::{collections::HashMap, sync::Mutex};

use tokio::task::AbortHandle;

#[derive(Default)]
pub struct Downloads(Mutex<HashMap<String, AbortHandle>>);

impl Downloads {
    /// Record a download. Returns `false`, and records nothing, when one for
    /// the same model is already running.
    pub fn begin(&self, repo_id: &str, handle: AbortHandle) -> bool {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if map.contains_key(repo_id) {
            return false;
        }
        map.insert(repo_id.to_string(), handle);
        true
    }

    pub fn finish(&self, repo_id: &str) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(repo_id);
    }

    /// Stop a running download. Returns whether there was one.
    pub fn cancel(&self, repo_id: &str) -> bool {
        match self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(repo_id)
        {
            Some(handle) => {
                handle.abort();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_download_per_model_and_cancel_stops_it() {
        let downloads = Downloads::default();
        let task = tokio::spawn(std::future::pending::<()>());
        assert!(downloads.begin("Qwen/Qwen3-0.6B", task.abort_handle()));
        let second = tokio::spawn(std::future::pending::<()>());
        assert!(!downloads.begin("Qwen/Qwen3-0.6B", second.abort_handle()));
        assert!(downloads.cancel("Qwen/Qwen3-0.6B"));
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!downloads.cancel("Qwen/Qwen3-0.6B"));
        second.abort();
    }
}
