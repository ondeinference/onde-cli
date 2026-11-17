//! The webview is untrusted: it must not be granted the filesystem or native
//! dialogs. File pickers open from Rust and the webview refers to projects and
//! models by id.

use std::{fs, path::Path};

fn permissions(path: &Path) -> Vec<String> {
    let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    json["permissions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| match p {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Object(o) => o
                        .get("identifier")
                        .and_then(|i| i.as_str())
                        .map(String::from),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn no_capability_grants_fs_or_dialog() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            checked += 1;
            for p in permissions(&path) {
                assert!(
                    !p.starts_with("fs:") && !p.starts_with("dialog:"),
                    "{} grants {p}",
                    path.display()
                );
            }
        }
    }
    assert!(checked > 0, "no capability files found");
}
