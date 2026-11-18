//! The projects-directory override has its own test binary for the same reason
//! as `tests/paths.rs`: it writes process-wide state, and this test must not
//! run beside one that sets the storage root.

use std::path::PathBuf;

use onde_cli::paths::{onde_root, projects_dir, set_projects_dir};

#[test]
fn projects_dir_can_be_linked_and_unlinked() {
    let default = onde_root().join("projects");
    assert_eq!(projects_dir(), default);

    let linked = PathBuf::from("/Users/me/.onde/projects");
    set_projects_dir(Some(linked.clone()));
    assert_eq!(projects_dir(), linked);

    set_projects_dir(None);
    assert_eq!(projects_dir(), default);
}
