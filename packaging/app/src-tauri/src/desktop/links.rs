//! A folder the user linked into the sandbox, remembered across launches.
//!
//! The sandboxed app can only read `~/.onde` after the user picks it. A
//! security-scoped bookmark remembers that choice, so the app reads and writes
//! the projects in place and never copies them. Linking is not needed outside
//! the sandbox, where `~/.onde` is readable already.
//!
//! The bookmark is stored in `links.json` under the app's own storage root.
//! The guard returned by [`super::bookmark::resolve`] stays alive in
//! [`Links`] for as long as the folder is linked: dropping it ends the
//! sandbox's access.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use super::bookmark::{self, AccessGuard};

/// Where the projects come from, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    pub kind: LocationKind,
    pub path: PathBuf,
    /// Why a link was not restored, or another thing worth telling the user.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// Not sandboxed: the projects are in `~/.onde` already.
    Native,
    /// Sandboxed and not linked: the app's own container.
    Container,
    /// Sandboxed, with the user's folder linked.
    Linked,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct LinksFile {
    /// Base64 of the bookmark for the projects folder.
    projects: Option<String>,
}

struct Active {
    path: PathBuf,
    // Dropping this ends access to the folder.
    _access: AccessGuard,
}

/// The linked projects folder, shared through Tauri state.
pub struct Links {
    file: PathBuf,
    active: Mutex<Option<Active>>,
    /// Set when the saved link could not be used at startup.
    note: Mutex<Option<String>>,
}

impl Links {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            active: Mutex::new(None),
            note: Mutex::new(None),
        }
    }

    /// Read `links.json` and reopen the saved folder. A saved link that can no
    /// longer be used is kept in the file (the user may reconnect a drive) and
    /// reported through [`Links::location`]; it never fails startup.
    pub fn restore(&self) -> Option<PathBuf> {
        let saved = read_file(&self.file)?.projects?;
        let result = STANDARD
            .decode(saved.trim())
            .map_err(|e| format!("the saved link is damaged ({e})"))
            .and_then(|data| bookmark::resolve(&data).map_err(|e| e.to_string()));
        let opened = result.and_then(|resolved| {
            let dir = projects_dir_in(&resolved.path)?;
            Ok((dir, resolved))
        });
        match opened {
            Ok((dir, resolved)) => {
                // Refresh from the resolved URL while access is held: a URL
                // made from the path alone has no security scope.
                if resolved.stale
                    && let Ok(fresh) = resolved.access.refresh_bookmark()
                {
                    let _ = write_file(&self.file, Some(&fresh));
                }
                self.set_active(dir.clone(), resolved.access);
                Some(dir)
            }
            Err(why) => {
                *lock(&self.note) = Some(format!(
                    "The linked projects folder could not be opened ({why}). Showing the app's own projects instead."
                ));
                None
            }
        }
    }

    /// Open the folder the user just picked, check it, remember it and keep
    /// access to it. Returns the projects directory inside it. Nothing changes
    /// if any step fails, so a bad pick never replaces a working link.
    pub fn link(&self, bookmark_data: &[u8]) -> Result<PathBuf, String> {
        let resolved = bookmark::resolve(bookmark_data).map_err(|e| e.to_string())?;
        let dir = projects_dir_in(&resolved.path)?;
        // A fresh `.onde` has no `projects` yet. Make it before the write test.
        std::fs::create_dir_all(&dir).map_err(|e| format!("creating {} ({e})", dir.display()))?;
        ensure_writable(&dir)?;
        write_file(&self.file, Some(bookmark_data)).map_err(|e| format!("saving the link: {e}"))?;
        *lock(&self.note) = None;
        self.set_active(dir.clone(), resolved.access);
        Ok(dir)
    }

    /// Forget the link and release the folder.
    pub fn unlink(&self) -> Result<(), String> {
        write_file(&self.file, None).map_err(|e| format!("removing the link: {e}"))?;
        *lock(&self.note) = None;
        *lock(&self.active) = None;
        Ok(())
    }

    pub fn linked_path(&self) -> Option<PathBuf> {
        lock(&self.active).as_ref().map(|a| a.path.clone())
    }

    /// What the UI shows. `sandboxed` is [`super::storage::is_sandboxed`];
    /// `container_projects` is where projects go without a link.
    pub fn location(&self, sandboxed: bool, container_projects: PathBuf) -> Location {
        if let Some(path) = self.linked_path() {
            return Location {
                kind: LocationKind::Linked,
                path,
                note: None,
            };
        }
        Location {
            kind: if sandboxed {
                LocationKind::Container
            } else {
                LocationKind::Native
            },
            path: container_projects,
            note: lock(&self.note).clone(),
        }
    }

    fn set_active(&self, path: PathBuf, access: AccessGuard) {
        *lock(&self.active) = Some(Active {
            path,
            _access: access,
        });
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn read_file(path: &Path) -> Option<LinksFile> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Write `links.json` through a temporary file, so a crash cannot leave half a
/// file that loses the link.
fn write_file(path: &Path, bookmark: Option<&[u8]>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = LinksFile {
        projects: bookmark.map(|b| STANDARD.encode(b)),
    };
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(tmp, path)
}

/// The projects directory for a folder the user picked: the folder itself when
/// it is named `projects`, otherwise its `projects` child. Picking `~/.onde` is
/// the common case, and a `.onde` that has no `projects` yet (the app was
/// installed before the CLI ever ran) is fine: [`Links::link`] creates it. Any
/// other folder without a `projects` child is refused, so the app never adds
/// directories to a folder that was not meant for it.
pub fn projects_dir_in(picked: &Path) -> Result<PathBuf, String> {
    if picked.file_name().is_some_and(|n| n == "projects") {
        return Ok(picked.to_path_buf());
    }
    let child = picked.join("projects");
    if child.is_dir() || picked.file_name().is_some_and(|n| n == ".onde") {
        return Ok(child);
    }
    Err(format!(
        "{} has no \"projects\" folder. Pick your .onde folder, or a folder named projects.",
        picked.display()
    ))
}

/// Fail early if the folder cannot be written to, so a later run does not
/// discover it halfway through training.
pub fn ensure_writable(dir: &Path) -> Result<(), String> {
    let probe = dir.join(format!(".onde-write-test-{}", std::process::id()));
    std::fs::write(&probe, b"").map_err(|e| format!("{} is not writable ({e})", dir.display()))?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bookmark for a folder the test made, standing in for the one the
    /// open panel would give.
    fn bookmark_for(path: &Path) -> Vec<u8> {
        #[cfg(target_os = "macos")]
        {
            bookmark::create_from_url(&bookmark::file_url(path)).unwrap()
        }
        #[cfg(not(target_os = "macos"))]
        {
            path.to_string_lossy().into_owned().into_bytes()
        }
    }

    #[test]
    fn the_onde_folder_or_its_projects_child_both_work() {
        let tmp = tempfile::tempdir().unwrap();
        let onde = tmp.path().join(".onde");
        std::fs::create_dir_all(onde.join("projects")).unwrap();
        assert_eq!(projects_dir_in(&onde).unwrap(), onde.join("projects"));
        assert_eq!(
            projects_dir_in(&onde.join("projects")).unwrap(),
            onde.join("projects")
        );
    }

    #[test]
    fn a_folder_without_projects_is_refused_with_advice() {
        let tmp = tempfile::tempdir().unwrap();
        let err = projects_dir_in(tmp.path()).unwrap_err();
        assert!(err.contains("Pick your .onde folder"), "{err}");
    }

    #[test]
    fn a_fresh_onde_folder_is_accepted_and_its_projects_folder_created_on_link() {
        let tmp = tempfile::tempdir().unwrap();
        let onde = tmp.path().join(".onde");
        std::fs::create_dir(&onde).unwrap();
        assert_eq!(projects_dir_in(&onde).unwrap(), onde.join("projects"));

        let links = Links::new(tmp.path().join("links.json"));
        let data = bookmark_for(&onde);
        let dir = links.link(&data).expect("a fresh .onde links");
        assert_eq!(
            dir.canonicalize().unwrap(),
            onde.join("projects").canonicalize().unwrap()
        );
        assert!(onde.join("projects").is_dir());
        // The folder was only made because it is named .onde.
        let other = tmp.path().join("stuff");
        std::fs::create_dir(&other).unwrap();
        assert!(links.link(&bookmark_for(&other)).is_err());
        assert!(!other.join("projects").exists());
    }

    #[test]
    fn unwritable_folders_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(ensure_writable(tmp.path()).is_ok());
        assert!(ensure_writable(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn the_link_file_round_trips_and_unlinking_clears_it() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("links.json");
        write_file(&file, Some(b"abc")).unwrap();
        let saved = read_file(&file).unwrap();
        assert_eq!(STANDARD.decode(saved.projects.unwrap()).unwrap(), b"abc");
        write_file(&file, None).unwrap();
        assert_eq!(read_file(&file).unwrap(), LinksFile::default());
        assert!(!tmp.path().join("links.json.tmp").exists());
    }

    #[test]
    fn a_damaged_link_falls_back_with_a_note_and_keeps_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("links.json");
        std::fs::write(&file, r#"{"projects":"@@not base64@@"}"#).unwrap();
        let links = Links::new(file.clone());
        assert_eq!(links.restore(), None);
        let location = links.location(true, PathBuf::from("/container/projects"));
        assert_eq!(location.kind, LocationKind::Container);
        assert!(location.note.unwrap().contains("could not be opened"));
        assert!(file.exists(), "the saved link is left for the user to fix");
    }

    #[test]
    fn without_a_link_the_location_follows_the_sandbox() {
        let links = Links::new(PathBuf::from("/nonexistent/links.json"));
        assert_eq!(links.restore(), None);
        let projects = PathBuf::from("/p");
        assert_eq!(
            links.location(false, projects.clone()).kind,
            LocationKind::Native
        );
        assert_eq!(links.location(true, projects).kind, LocationKind::Container);
    }
}
