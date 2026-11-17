//! Security-scoped bookmarks.
//!
//! A sandboxed app loses access to a folder the user picked when it quits. A
//! security-scoped bookmark, saved to disk, lets the next launch open that
//! folder again without asking. This is the only module that talks to
//! Foundation for it.
//!
//! Outside macOS there is no sandbox to cross, so a "bookmark" is just the
//! path, and the guard does nothing.

use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub struct BookmarkError(pub String);

impl std::fmt::Display for BookmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BookmarkError {}

/// A folder reopened from a bookmark.
pub struct Resolved {
    pub path: PathBuf,
    /// The system says the bookmark should be saved again (the folder moved).
    pub stale: bool,
    /// Keeps the folder accessible while it is alive.
    pub access: AccessGuard,
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::{Path, PathBuf};

    use objc2::rc::Retained;
    use objc2_foundation::{
        NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions,
    };

    use super::{BookmarkError, Resolved};

    /// Access to a bookmarked folder; released on drop.
    pub struct AccessGuard {
        url: Retained<NSURL>,
        started: bool,
    }

    impl AccessGuard {
        /// A fresh bookmark for the folder, made from the URL the system
        /// resolved and that this guard keeps open. A URL built from a plain
        /// path carries no security scope, so only this one can be saved.
        pub fn refresh_bookmark(&self) -> Result<Vec<u8>, BookmarkError> {
            create_from_url(&self.url)
        }
    }

    impl Drop for AccessGuard {
        fn drop(&mut self) {
            if self.started {
                // SAFETY: paired with the successful start in `resolve`.
                unsafe { self.url.stopAccessingSecurityScopedResource() };
            }
        }
    }

    /// Bookmark data for `url`. Pass the URL the open panel returned when you
    /// have it: that one carries the grant the user just gave.
    pub fn create_from_url(url: &NSURL) -> Result<Vec<u8>, BookmarkError> {
        let data = url
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                NSURLBookmarkCreationOptions::WithSecurityScope,
                None,
                None,
            )
            .map_err(|e| {
                BookmarkError(format!("creating a bookmark: {}", e.localizedDescription()))
            })?;
        Ok(data.to_vec())
    }

    pub fn file_url(path: &Path) -> Retained<NSURL> {
        let s = NSString::from_str(&path.to_string_lossy());
        NSURL::fileURLWithPath_isDirectory(&s, true)
    }

    pub fn resolve(data: &[u8]) -> Result<Resolved, BookmarkError> {
        let data = NSData::with_bytes(data);
        let mut stale = objc2::runtime::Bool::NO;
        // SAFETY: `stale` outlives the call and is a valid out pointer.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions::WithSecurityScope,
                None,
                &mut stale,
            )
        }
        .map_err(|e| {
            BookmarkError(format!(
                "opening the bookmark: {}",
                e.localizedDescription()
            ))
        })?;
        // SAFETY: a plain Foundation call on a URL we hold.
        let started = unsafe { url.startAccessingSecurityScopedResource() };
        let path = url
            .path()
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| BookmarkError("the bookmark has no path".into()))?;
        Ok(Resolved {
            path,
            stale: stale.as_bool(),
            access: AccessGuard { url, started },
        })
    }
}

#[cfg(target_os = "macos")]
pub use imp::{AccessGuard, create_from_url, file_url, resolve};

#[cfg(not(target_os = "macos"))]
pub struct AccessGuard(PathBuf);

#[cfg(not(target_os = "macos"))]
impl AccessGuard {
    pub fn refresh_bookmark(&self) -> Result<Vec<u8>, BookmarkError> {
        Ok(self.0.to_string_lossy().into_owned().into_bytes())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn resolve(data: &[u8]) -> Result<Resolved, BookmarkError> {
    let path = String::from_utf8(data.to_vec()).map_err(|e| BookmarkError(e.to_string()))?;
    let path = PathBuf::from(path);
    Ok(Resolved {
        path: path.clone(),
        stale: false,
        access: AccessGuard(path),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[cfg(target_os = "macos")]
    fn create(path: &Path) -> Result<Vec<u8>, BookmarkError> {
        create_from_url(&file_url(path))
    }

    #[cfg(not(target_os = "macos"))]
    fn create(path: &Path) -> Result<Vec<u8>, BookmarkError> {
        Ok(path.to_string_lossy().into_owned().into_bytes())
    }

    /// The refresh comes from the open guard, and it opens to the same folder.
    #[test]
    fn a_refreshed_bookmark_opens_the_same_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let resolved = resolve(&create(&dir).unwrap()).unwrap();
        let fresh = resolved.access.refresh_bookmark().expect("refresh");
        let again = resolve(&fresh).expect("the refreshed bookmark resolves");
        assert_eq!(again.path.canonicalize().unwrap(), dir);
    }

    /// Create a bookmark for a folder and open it again. Outside the sandbox
    /// the scope is a no-op, but the data must still round-trip to the path.
    #[test]
    fn a_bookmark_round_trips_to_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let data = create(&dir).expect("bookmark");
        assert!(!data.is_empty());
        let resolved = resolve(&data).expect("resolve");
        assert_eq!(resolved.path.canonicalize().unwrap(), dir);
        assert!(!resolved.stale);
        drop(resolved);
    }

    // Elsewhere a "bookmark" is any UTF-8 path, so only macOS can reject text.
    #[cfg(target_os = "macos")]
    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(resolve(b"definitely not a bookmark").is_err());
    }
}
