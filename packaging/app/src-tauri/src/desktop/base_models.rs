//! The base models a project can start from, and creating a project.
//!
//! The catalog is the one the terminal UI offers (`hf_clone::BASE_MODELS`).
//! Whether a model is downloaded is decided by [`onde_cli::models`], which
//! checks that the weights are all there, so a half-finished download does not
//! count.

use std::path::PathBuf;

use onde_cli::{hf_clone::BASE_MODELS, models, paths, project};
use serde::Serialize;

use super::projects::{ProjectSummary, check_repo_id};

/// One entry of the base-model list.
#[derive(Debug, Clone, Serialize)]
pub struct BaseModelView {
    pub repo_id: String,
    pub display_name: String,
    pub size_display: String,
    pub params: String,
    pub description: String,
    /// Where the complete model is on disk, when it is.
    pub dir: Option<PathBuf>,
}

pub fn list() -> Vec<BaseModelView> {
    BASE_MODELS
        .iter()
        .map(|m| BaseModelView {
            repo_id: m.repo_id.to_string(),
            display_name: m.display_name.to_string(),
            size_display: m.size_display.to_string(),
            params: m.params.to_string(),
            description: m.description.to_string(),
            dir: models::base_model_dir(m.repo_id),
        })
        .collect()
}

/// Only models from the catalog can be downloaded from the app.
pub fn in_catalog(repo_id: &str) -> bool {
    BASE_MODELS.iter().any(|m| m.repo_id == repo_id)
}

/// Make a new project. `repo_id` is where the model will be published
/// (`org/name`) and names the project folder; it must be new.
pub fn create_project(repo_id: &str, base_model_id: &str) -> Result<ProjectSummary, String> {
    check_repo_id(repo_id)?;
    check_repo_id(base_model_id)?;
    if paths::projects_dir()
        .join(repo_id)
        .join("project.json")
        .exists()
    {
        return Err(format!("A project named {repo_id} already exists."));
    }
    project::create_project(repo_id, base_model_id).map_err(|e| format!("{e:#}"))?;
    super::projects::list()
        .into_iter()
        .find(|p| p.repo_id == repo_id)
        .ok_or_else(|| "The project was created but could not be read back.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_matches_the_terminal_ui_and_every_id_is_valid() {
        let models = list();
        assert!(models.len() >= 3, "{models:?}");
        for m in &models {
            assert!(check_repo_id(&m.repo_id).is_ok(), "{}", m.repo_id);
            assert!(in_catalog(&m.repo_id));
        }
        assert!(!in_catalog("someone/else"));
    }

    #[test]
    fn bad_ids_are_refused_before_anything_is_written() {
        assert!(create_project("../x", "Qwen/Qwen3-0.6B").is_err());
        assert!(create_project("org/name", "not-an-id").is_err());
    }
}
