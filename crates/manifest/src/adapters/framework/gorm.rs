//! GORM adapter — extracts data models from Go structs embedding gorm.Model.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{DataModel, Field, ManifestFragment};

pub struct GormAdapter;

impl Adapter for GormAdapter {
    fn name(&self) -> &str {
        "gorm"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        let Some(go_mod) = &ctx.go_mod else {
            return false;
        };
        go_mod.contains("gorm.io/gorm")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into())?;

        for file in &ctx.files {
            if file
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| e != "go")
            {
                continue;
            }

            if should_skip(file, &ctx.root) {
                continue;
            }

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Quick filter — skip files that don't mention gorm
            if !source.contains("gorm") {
                continue;
            }

            let tree = match parser.parse(&source, None) {
                Some(t) => t,
                None => continue,
            };

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            extract_gorm_models(&tree.root_node(), &source, &rel, &mut fragment);
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        50
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Framework
    }
}

fn should_skip(file: &Path, root: &Path) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), "vendor" | "testdata") {
                return true;
            }
        }
    }
    false
}

/// Walk AST looking for struct type declarations that embed gorm.Model.
fn extract_gorm_models(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "type_declaration" {
            try_extract_gorm_struct(&child, source, file, fragment);
        }
    }
}

/// Check if a type declaration is a struct embedding gorm.Model.
fn try_extract_gorm_struct(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut inner_cursor = node.walk();
    for spec in node.named_children(&mut inner_cursor) {
        if spec.kind() != "type_spec" {
            continue;
        }

        let name = match spec.child_by_field_name("name") {
            Some(n) => node_text(&n, source),
            None => continue,
        };

        let type_node = match spec.child_by_field_name("type") {
            Some(t) => t,
            None => continue,
        };

        if type_node.kind() != "struct_type" {
            continue;
        }

        // The struct body is a field_declaration_list
        let mut has_gorm_model = false;
        let mut fields = Vec::new();

        let mut struct_cursor = type_node.walk();
        for child in type_node.named_children(&mut struct_cursor) {
            if child.kind() != "field_declaration_list" {
                continue;
            }

            let mut field_cursor = child.walk();
            for field_node in child.named_children(&mut field_cursor) {
                if field_node.kind() != "field_declaration" {
                    continue;
                }

                // Embedded field (no name, only type) — check for gorm.Model
                let has_name = field_node.child_by_field_name("name").is_some();
                if !has_name {
                    // Check if the type is a qualified_type "gorm.Model"
                    if let Some(type_node) = field_node.child_by_field_name("type") {
                        let type_text = node_text(&type_node, source);
                        if type_text == "gorm.Model" {
                            has_gorm_model = true;
                        }
                    }
                    continue;
                }

                // Named field: extract name and type
                if let Some(field_name_node) = field_node.child_by_field_name("name") {
                    let field_name = node_text(&field_name_node, source);
                    let field_type = field_node
                        .child_by_field_name("type")
                        .map(|t| node_text(&t, source))
                        .unwrap_or_default();

                    if !field_name.is_empty() {
                        fields.push(Field {
                            name: field_name,
                            type_name: field_type,
                            optional: false,
                        });
                    }
                }
            }
        }

        if has_gorm_model {
            let model = DataModel {
                name: name.clone(),
                source: file.to_string(),
                orm: "gorm".to_string(),
                fields,
                relations: vec![],
                indexes: vec![],
            };
            fragment.models.insert(name, model);
        }
    }
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_go(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_gorm_models(&tree.root_node(), source, "models.go", &mut fragment);
        fragment
    }

    #[test]
    fn detect_gorm() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Go,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: Some("module example.com/myapp\n\nrequire gorm.io/gorm v1.25.5\n".to_string()),
        };
        assert!(GormAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_gorm() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Go,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: Some("module example.com/myapp\n".to_string()),
        };
        assert!(!GormAdapter.detect(&ctx));
    }

    #[test]
    fn extract_gorm_model() {
        let frag = parse_go(
            r#"
package models

import "gorm.io/gorm"

type User struct {
    gorm.Model
    Name  string
    Email string
    Age   int
}
"#,
        );

        assert!(
            frag.models.contains_key("User"),
            "keys: {:?}",
            frag.models.keys().collect::<Vec<_>>()
        );
        let model = &frag.models["User"];
        assert_eq!(model.orm, "gorm");

        let field_names: Vec<&str> = model.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"Name"));
        assert!(field_names.contains(&"Email"));
        assert!(field_names.contains(&"Age"));

        let email = model.fields.iter().find(|f| f.name == "Email").unwrap();
        assert_eq!(email.type_name, "string");
    }

    #[test]
    fn skip_non_gorm_structs() {
        let frag = parse_go(
            r#"
package models

type Config struct {
    Host string
    Port int
}

import "gorm.io/gorm"

type Product struct {
    gorm.Model
    Name  string
    Price float64
}
"#,
        );

        assert!(!frag.models.contains_key("Config"));
        assert!(frag.models.contains_key("Product"));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();

        std::fs::write(
            dir.path().join("models.go"),
            "package models\n\nimport \"gorm.io/gorm\"\n\ntype Item struct {\n\tgorm.Model\n\tName string\n}\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Go,
            files: vec![dir.path().join("models.go")],
            package_json: None,
            cargo_toml: None,
            go_mod: Some("module example.com/app\nrequire gorm.io/gorm v1.25.5\n".to_string()),
        };

        assert!(GormAdapter.detect(&ctx));
        let frag = GormAdapter.extract(&ctx).unwrap();
        assert!(frag.models.contains_key("Item"));
    }
}
