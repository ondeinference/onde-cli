use std::{fs, path::PathBuf};

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("onde").join("token"))
}

pub fn load() -> Option<String> {
    let content = fs::read_to_string(path()?).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn save(token: &str) -> std::io::Result<()> {
    let path = path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config directory"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, token)
}

pub fn clear() {
    if let Some(path) = path() {
        let _ = fs::remove_file(path);
    }
}
