//! Project context builder — collects files and parsed config for adapters.

use anyhow::Result;
use ignore::WalkBuilder;
use project_detect::ProjectKind;
use std::path::{Path, PathBuf};

/// Runtime context passed to every adapter. Contains the project root,
/// detected kind, a list of all source files, and pre-parsed config files.
pub struct ProjectContext {
    pub root: PathBuf,
    pub kind: ProjectKind,
    pub files: Vec<PathBuf>,
    pub package_json: Option<serde_json::Value>,
    pub cargo_toml: Option<toml::Value>,
    pub go_mod: Option<String>,
}

impl ProjectContext {
    /// Build a context by scanning the project root for source files
    /// and parsing known config files.
    pub fn build(root: &Path, kind: &ProjectKind) -> Result<Self> {
        let root = root.to_path_buf();
        let files = walk_source_files(&root)?;
        let package_json = read_json(&root.join("package.json"));
        let cargo_toml = read_toml(&root.join("Cargo.toml"));
        let go_mod = read_string(&root.join("go.mod"));

        Ok(Self {
            root,
            kind: kind.clone(),
            files,
            package_json,
            cargo_toml,
            go_mod,
        })
    }
}

/// Walk all files under `root`, respecting `.gitignore` rules.
fn walk_source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkBuilder::new(root).build() {
        let entry = entry?;
        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

/// Try to read and parse a JSON file, returning `None` on any failure.
fn read_json(path: &Path) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Try to read and parse a TOML file, returning `None` on any failure.
fn read_toml(path: &Path) -> Option<toml::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Try to read a file as a string, returning `None` on failure.
fn read_string(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}
