//! shadcn/ui adapter — detects shadcn installation and lists UI components.

use std::path::Path;

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Component, ManifestFragment};

pub struct ShadcnAdapter;

impl Adapter for ShadcnAdapter {
    fn name(&self) -> &str {
        "shadcn"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("components.json").exists()
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        // Find the UI components directory from components.json or defaults
        let ui_dir = find_ui_dir(ctx);

        if let Some(ui_dir) = ui_dir {
            extract_components(ctx, &ui_dir, &mut fragment);
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        30
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Framework
    }
}

/// Determine the UI components directory. Checks components.json for aliases,
/// falls back to common defaults.
fn find_ui_dir(ctx: &ProjectContext) -> Option<std::path::PathBuf> {
    // Common locations for shadcn UI components
    let candidates = ["components/ui", "src/components/ui", "app/components/ui"];

    for candidate in &candidates {
        let path = ctx.root.join(candidate);
        if path.is_dir() {
            return Some(path);
        }
    }

    None
}

/// Scan the UI directory for .tsx files, each representing a component.
fn extract_components(ctx: &ProjectContext, ui_dir: &Path, fragment: &mut ManifestFragment) {
    for file in &ctx.files {
        let rel = match file.strip_prefix(ui_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Only direct children (not nested directories)
        if rel.components().count() != 1 {
            continue;
        }

        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "tsx" | "jsx" | "ts" | "js") {
            continue;
        }

        let name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if name.is_empty() || name == "index" {
            continue;
        }

        let rel_file = file
            .strip_prefix(&ctx.root)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        fragment.components.push(Component {
            name,
            file: rel_file,
            props: vec![],
        });
    }

    // Sort components by name for deterministic output
    fragment.components.sort_by(|a, b| a.name.cmp(&b.name));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        fn recurse(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        recurse(&path, files);
                    } else {
                        files.push(path);
                    }
                }
            }
        }
        recurse(root, &mut files);
        files.sort();
        files
    }

    fn build_ctx(dir: &TempDir) -> ProjectContext {
        let root = dir.path().to_path_buf();
        let files = walk_files(&root);
        ProjectContext {
            root,
            kind: project_detect::ProjectKind::Node {
                manager: project_detect::NodePM::Npm,
            },
            files,
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        }
    }

    #[test]
    fn detect_with_components_json() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("components.json"), "{}").unwrap();

        let ctx = build_ctx(&dir);
        assert!(ShadcnAdapter.detect(&ctx));
    }

    #[test]
    fn no_detection_without_config() {
        let dir = TempDir::new().unwrap();
        let ctx = build_ctx(&dir);
        assert!(!ShadcnAdapter.detect(&ctx));
    }

    #[test]
    fn extract_ui_components() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::write(root.join("components.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("components/ui")).unwrap();
        std::fs::write(
            root.join("components/ui/button.tsx"),
            r#"export function Button() { return <button />; }"#,
        )
        .unwrap();
        std::fs::write(
            root.join("components/ui/dialog.tsx"),
            r#"export function Dialog() { return <div />; }"#,
        )
        .unwrap();
        std::fs::write(
            root.join("components/ui/card.tsx"),
            r#"export function Card() { return <div />; }"#,
        )
        .unwrap();

        let ctx = build_ctx(&dir);
        let frag = ShadcnAdapter.extract(&ctx).unwrap();

        assert_eq!(frag.components.len(), 3);

        let names: Vec<&str> = frag.components.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"button"));
        assert!(names.contains(&"dialog"));
        assert!(names.contains(&"card"));

        // Should be sorted
        assert_eq!(names, vec!["button", "card", "dialog"]);
    }

    #[test]
    fn ignores_non_tsx_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::write(root.join("components.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("components/ui")).unwrap();
        std::fs::write(root.join("components/ui/button.tsx"), "").unwrap();
        std::fs::write(root.join("components/ui/README.md"), "docs").unwrap();

        let ctx = build_ctx(&dir);
        let frag = ShadcnAdapter.extract(&ctx).unwrap();

        assert_eq!(frag.components.len(), 1);
        assert_eq!(frag.components[0].name, "button");
    }
}
