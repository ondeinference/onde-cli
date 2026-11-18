//! The base-model downloads in flight, so one can be cancelled by name.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::task::AbortHandle;

/// Identifies one download. A cancelled download's handler wakes up after a new
/// download of the same model may have started, and must only clear its own entry.
pub type Ticket = u64;

#[derive(Default)]
pub struct Downloads {
    running: Mutex<HashMap<String, (Ticket, AbortHandle)>>,
    next: AtomicU64,
}

impl Downloads {
    /// Record a download and return its ticket. `None`, with nothing recorded,
    /// when one for the same model is already running.
    pub fn begin(&self, repo_id: &str, handle: AbortHandle) -> Option<Ticket> {
        let mut map = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if map.contains_key(repo_id) {
            return None;
        }
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        map.insert(repo_id.to_string(), (ticket, handle));
        Some(ticket)
    }

    /// Forget a download, but only if `ticket` is still the one on record. After
    /// a cancel and a restart the entry belongs to the newer download.
    pub fn finish(&self, repo_id: &str, ticket: Ticket) {
        let mut map = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if map.get(repo_id).is_some_and(|(t, _)| *t == ticket) {
            map.remove(repo_id);
        }
    }

    /// Stop a running download. Returns whether there was one.
    pub fn cancel(&self, repo_id: &str) -> bool {
        let removed = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(repo_id);
        match removed {
            Some((_, handle)) => {
                handle.abort();
                true
            }
            None => false,
        }
    }

    pub fn is_running(&self, repo_id: &str) -> bool {
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(repo_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> tokio::task::JoinHandle<()> {
        tokio::spawn(std::future::pending::<()>())
    }

    #[tokio::test]
    async fn one_download_per_model_and_cancel_stops_it() {
        let downloads = Downloads::default();
        let first = task();
        assert!(
            downloads
                .begin("Qwen/Qwen3-0.6B", first.abort_handle())
                .is_some()
        );
        let second = task();
        assert!(
            downloads
                .begin("Qwen/Qwen3-0.6B", second.abort_handle())
                .is_none()
        );
        assert!(downloads.is_running("Qwen/Qwen3-0.6B"));
        assert!(downloads.cancel("Qwen/Qwen3-0.6B"));
        assert!(first.await.unwrap_err().is_cancelled());
        assert!(!downloads.is_running("Qwen/Qwen3-0.6B"));
        assert!(!downloads.cancel("Qwen/Qwen3-0.6B"));
        second.abort();
    }

    /// The race from the review: cancel, start again, then the cancelled
    /// download's handler finishes. It must not unregister the new one.
    #[tokio::test]
    async fn a_cancelled_downloads_finish_does_not_remove_its_replacement() {
        let downloads = Downloads::default();
        let old = task();
        let old_ticket = downloads
            .begin("Qwen/Qwen3-0.6B", old.abort_handle())
            .unwrap();
        assert!(downloads.cancel("Qwen/Qwen3-0.6B"));

        let new = task();
        let new_ticket = downloads
            .begin("Qwen/Qwen3-0.6B", new.abort_handle())
            .unwrap();
        assert_ne!(old_ticket, new_ticket);

        downloads.finish("Qwen/Qwen3-0.6B", old_ticket);
        assert!(
            downloads.is_running("Qwen/Qwen3-0.6B"),
            "the new download is still registered"
        );
        assert!(
            downloads.cancel("Qwen/Qwen3-0.6B"),
            "and can still be cancelled"
        );

        downloads.finish("Qwen/Qwen3-0.6B", new_ticket); // nothing left to remove, no panic
        old.abort();
        new.abort();
    }

    #[tokio::test]
    async fn finishing_with_the_right_ticket_frees_the_slot() {
        let downloads = Downloads::default();
        let t = task();
        let ticket = downloads.begin("a/b", t.abort_handle()).unwrap();
        downloads.finish("a/b", ticket);
        assert!(!downloads.is_running("a/b"));
        t.abort();
    }
}
