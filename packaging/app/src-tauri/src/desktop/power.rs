//! Keep the computer awake while a training job runs.
//!
//! A run takes minutes to hours. Left alone, macOS can put an idle Mac to sleep
//! partway through, or slow a background app down (App Nap). An activity
//! assertion tells the system the work is user initiated and must not be
//! interrupted by idle sleep. It does not stop the display from sleeping and it
//! does not stop the user closing the lid.
//!
//! On Windows the guard asks the system to stay up the same way, with
//! `SetThreadExecutionState`. The display may still turn off.
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

#[cfg(windows)]
mod imp {
    use std::marker::PhantomData;

    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };

    /// Ends the request when dropped. The state belongs to the calling thread,
    /// so the guard is not `Send`.
    pub struct KeepAwake {
        _thread_bound: PhantomData<*const ()>,
    }

    impl KeepAwake {
        pub fn begin(_reason: &str) -> Self {
            // SAFETY: the call takes flags only and changes nothing but this
            // thread's execution state.
            let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
            if previous == 0 {
                eprintln!("could not ask Windows to stay awake");
            }
            Self {
                _thread_bound: PhantomData,
            }
        }
    }

    impl Drop for KeepAwake {
        fn drop(&mut self) {
            // SAFETY: as above. `ES_CONTINUOUS` alone clears the request.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
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

    /// The call returns the thread's previous state, so a second guard-free
    /// call shows what the first one left behind.
    #[cfg(windows)]
    #[test]
    fn the_request_is_set_while_held_and_cleared_after() {
        use windows_sys::Win32::System::Power::{
            ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
        };
        // SAFETY: flags only; the state is restored before returning.
        let current = || unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        let guard = KeepAwake::begin("test");
        let held = current();
        assert_ne!(held & ES_SYSTEM_REQUIRED, 0, "set while held");
        drop(guard);
        assert_eq!(current() & ES_SYSTEM_REQUIRED, 0, "cleared on drop");
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn it_does_nothing_elsewhere() {
        drop(KeepAwake::begin("x"));
    }
}
