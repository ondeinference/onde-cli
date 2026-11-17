//! The storage-root override lives in its own integration test.
//!
//! `set_root` writes the process-wide `ROOT` `OnceLock` once and for good, so
//! running it in the shared unit-test binary would pin the root for every other
//! test that reads `onde_root()`. A `tests/` file is its own process, so the
//! mutation stays contained.

use std::path::PathBuf;

use onde_cli::paths::{onde_root, root_override, set_root};

#[test]
fn override_wins_and_first_call_sticks() {
    let before = onde_root();
    assert!(
        root_override().is_none(),
        "nothing set the root before this"
    );

    let dir = std::env::temp_dir().join("onde-paths-test");
    assert!(set_root(dir.clone()));
    assert_eq!(onde_root(), dir);
    assert_ne!(onde_root(), before);

    assert!(!set_root(PathBuf::from("/elsewhere")));
    assert_eq!(onde_root(), dir);
}
