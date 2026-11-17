//! Where Onde Desktop keeps its data.
//!
//! The app is sandboxed, so it keeps everything in the App Group container it
//! shares with the other Onde apps. Models downloaded there are visible to
//! them, and to the CLI's model picker.

use std::path::{Path, PathBuf};

pub const APP_GROUP: &str = "group.com.ondeinference.apps";

/// Subdirectory of the group container that belongs to this app.
pub const APP_DIR: &str = "desktop";

/// The group container's directory for this app under `home`.
pub fn group_root(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Group Containers")
        .join(APP_GROUP)
        .join(APP_DIR)
}

/// True when running inside the App Sandbox. `APP_SANDBOX_CONTAINER_ID=1
/// pnpm tauri dev` forces this branch without signing.
pub fn is_sandboxed() -> bool {
    std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some()
}

/// The storage root to hand to `onde_cli::paths::set_root`, or `None` to keep
/// the CLI's `~/.onde` (an unsandboxed dev run).
pub fn resolve_root(sandboxed: bool, real_home: Option<PathBuf>) -> Option<PathBuf> {
    if !sandboxed {
        return None;
    }
    real_home.map(|home| group_root(&home))
}

/// Resolve and apply the storage root. Call before anything touches state.
pub fn init() -> std::io::Result<()> {
    let root = resolve_root(is_sandboxed(), onde_cli::paths::real_home());
    if let Some(root) = root {
        std::fs::create_dir_all(&root)?;
        onde_cli::paths::set_root(root);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandboxed_uses_the_group_container() {
        let home = PathBuf::from("/Users/me");
        assert_eq!(
            resolve_root(true, Some(home)),
            Some(PathBuf::from(
                "/Users/me/Library/Group Containers/group.com.ondeinference.apps/desktop"
            ))
        );
    }

    #[test]
    fn unsandboxed_keeps_the_cli_default() {
        assert_eq!(resolve_root(false, Some(PathBuf::from("/Users/me"))), None);
    }

    #[test]
    fn sandboxed_without_a_home_has_no_root() {
        assert_eq!(resolve_root(true, None), None);
    }
}
