//! Keep the Mac awake while a training job runs.
//!
//! A run takes minutes to hours. Left alone, macOS can put an idle Mac to sleep
//! partway through, or slow a background app down (App Nap). An activity
//! assertion tells the system the work is user initiated and must not be
//! interrupted by idle sleep. It does not stop the display from sleeping and it
//! does not stop the user closing the lid.
//!
//! Elsewhere there is nothing to hold, and the guard does nothing.

#[cfg(target_os = "macos")]
mod imp {
    use objc2::{rc::Retained, runtime::ProtocolObject};
    use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};

    /// Ends the assertion when dropped. Not `Send`: take it and drop it on the
    /// same thread.
    pub struct KeepAwake {
        activity: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    }

    impl KeepAwake {
        pub fn begin(reason: &str) -> Self {
            let info = NSProcessInfo::processInfo();
            let activity = info.beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiated,
                &NSString::from_str(reason),
            );
            Self { activity }
        }
    }

    impl Drop for KeepAwake {
        fn drop(&mut self) {
            // SAFETY: `activity` came from `beginActivityWithOptions:reason:` on
            // this same process info and has not been ended before.
            unsafe { NSProcessInfo::processInfo().endActivity(&self.activity) };
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub struct KeepAwake;

    impl KeepAwake {
        pub fn begin(_reason: &str) -> Self {
            Self
        }
    }
}

pub use imp::KeepAwake;

#[cfg(test)]
mod tests {
    use super::KeepAwake;

    /// The assertion is real on macOS: `pmset -g assertions` lists it while the
    /// guard is alive and drops it afterwards.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_assertion_is_listed_while_held_and_gone_after() {
        let reason = format!("onde-desktop-test-{}", std::process::id());
        let listed = |reason: &str| {
            let out = std::process::Command::new("pmset")
                .args(["-g", "assertions"])
                .output()
                .expect("pmset runs");
            String::from_utf8_lossy(&out.stdout).contains(reason)
        };
        let guard = KeepAwake::begin(&reason);
        assert!(
            listed(&reason),
            "the assertion should be visible while held"
        );
        drop(guard);
        assert!(!listed(&reason), "the assertion should end on drop");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn it_does_nothing_off_macos() {
        drop(KeepAwake::begin("x"));
    }
}
