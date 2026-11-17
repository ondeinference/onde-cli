use std::sync::Mutex;
use tokio::sync::oneshot;

/// The browser sign-in that is waiting for its callback, if any. Sending on
/// the stored sender (or dropping it) tells the waiting command to stop.
#[derive(Default)]
pub struct PendingSignIn(Mutex<Option<oneshot::Sender<()>>>);

impl PendingSignIn {
    /// Registers a new sign-in. A previous one still waiting is cancelled,
    /// because dropping its sender wakes it.
    pub fn begin(&self) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        rx
    }

    /// Cancels the waiting sign-in. Does nothing when none is pending.
    pub fn cancel(&self) {
        if let Some(tx) = self.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_wakes_the_waiter() {
        let slot = PendingSignIn::default();
        let rx = slot.begin();
        slot.cancel();
        assert!(rx.await.is_ok());
    }

    #[tokio::test]
    async fn a_new_sign_in_cancels_the_old_one() {
        let slot = PendingSignIn::default();
        let old = slot.begin();
        let _new = slot.begin();
        assert!(old.await.is_err());
    }
}
