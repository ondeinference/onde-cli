//! Storage-root seam.
//!
//! The CLI keeps its data under `~/.onde`. A host that cannot use the home
//! directory, such as a sandboxed desktop app whose data lives in an App Group
//! container, calls [`set_root`] once at startup and everything that persists
//! state (projects, the session token) follows it.
//!
//! Without an override nothing changes: [`onde_root`] is `~/.onde` and the
//! token stays in the platform config directory.

use std::{
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock},
};

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Projects can live somewhere else than the root, and that can change while
/// the host runs: a sandboxed app links the user's `~/.onde` after startup.
static PROJECTS: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Point the storage root at `root`. Only the first call wins; returns `false`
/// if a root was already set. Call it before anything reads or writes state.
pub fn set_root(root: PathBuf) -> bool {
    ROOT.set(root).is_ok()
}

/// The root chosen with [`set_root`], if any.
pub fn root_override() -> Option<&'static Path> {
    ROOT.get().map(PathBuf::as_path)
}

/// The directory holding Onde's own data: the override, or `~/.onde`.
pub fn onde_root() -> PathBuf {
    match root_override() {
        Some(root) => root.to_path_buf(),
        None => default_root(),
    }
}

/// Point the projects directory at `dir`, or back at `onde_root()/projects`
/// with `None`. Unlike [`set_root`] this can be called at any time; the
/// session token is not affected.
pub fn set_projects_dir(dir: Option<PathBuf>) {
    *PROJECTS.write().unwrap_or_else(|e| e.into_inner()) = dir;
}

/// Where projects live: the directory from [`set_projects_dir`], else
/// `onde_root()/projects`.
pub fn projects_dir() -> PathBuf {
    PROJECTS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| onde_root().join("projects"))
}

// The default root honours `$HOME` through `dirs::home_dir()`, so containers,
// CI and `env -i`/sudo wrappers that redirect it keep landing their data where
// `$HOME` points. A sandboxed host whose `$HOME` is a container path calls
// `set_root()` instead; `real_home()` (passwd) exists only for the Hugging Face
// cache lookups that must escape the container.
fn default_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".onde")
}

/// The user's real home directory.
///
/// Inside the macOS App Sandbox `$HOME` points into the app's container, so
/// `dirs::home_dir()` would never find `~/Library/Group Containers/...`. The
/// passwd database still reports the real one.
#[cfg(unix)]
pub fn real_home() -> Option<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};

    // SAFETY: getpwuid_r writes into `pwd` and `buf`, both of which outlive
    // the call. `pw_dir` is read only while `buf` is alive and is copied out.
    unsafe {
        let mut size = libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX);
        if size <= 0 {
            size = 1024;
        }
        let mut buf = vec![0u8; size as usize];
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let rc = libc::getpwuid_r(
            libc::getuid(),
            &mut pwd,
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut result,
        );
        if rc != 0 || result.is_null() || pwd.pw_dir.is_null() {
            return dirs::home_dir();
        }
        let dir = CStr::from_ptr(pwd.pw_dir);
        if dir.to_bytes().is_empty() {
            return dirs::home_dir();
        }
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes())))
    }
}

#[cfg(not(unix))]
pub fn real_home() -> Option<PathBuf> {
    dirs::home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_root_is_dot_onde_under_home() {
        let home = dirs::home_dir().expect("home");
        assert_eq!(default_root(), home.join(".onde"));
    }

    #[test]
    fn real_home_is_an_absolute_existing_dir() {
        let home = real_home().expect("home");
        assert!(home.is_absolute());
        assert!(home.is_dir());
    }

    // The `set_root` override test lives in `tests/paths.rs`: it mutates the
    // process-wide `ROOT` `OnceLock`, which would leak into every other test in
    // this binary, so it needs its own process.
}
