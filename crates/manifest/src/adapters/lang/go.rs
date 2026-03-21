//! Go language adapter — extracts structs, interfaces, functions, and packages.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};
use uu_detect::ProjectKind;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Field, Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct GoAdapter;

impl Adapter for GoAdapter {
    fn name(&self) -> &str {
        "go"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Go)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec!["vendor", ".git"];

        for file in &ctx.files {
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("go")) {
                continue;
            }
            if should_skip(file, &ctx.root, &skip_dirs) {
                continue;
            }

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            let is_test = rel.ends_with("_test.go");

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let tree = match parser.parse(&source, None) {
                Some(t) => t,
                None => continue,
            };

            extract_go(&tree.root_node(), &source, &rel, is_test, &mut fragment);
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        100
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Language
    }
}

fn should_skip(file: &Path, root: &Path, skip_dirs: &[&str]) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if skip_dirs.contains(&name.as_ref()) {
                return true;
            }
        }
    }
    false
}

fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn source_loc(file: &str, node: &Node) -> String {
    format!("{}:{}", file, node.start_position().row + 1)
}

/// Determine visibility from Go naming convention: uppercase = exported (public).
fn go_visibility(name: &str) -> Visibility {
    if name.starts_with(|c: char| c.is_uppercase()) {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn extract_go(
    root: &Node,
    source: &str,
    file: &str,
    is_test: bool,
    fragment: &mut ManifestFragment,
) {
    // Extract package name for module
    let mut pkg_name = String::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "package_clause" {
            // The package name is the package_identifier child
            let mut pkg_cursor = child.walk();
            for pkg_child in child.named_children(&mut pkg_cursor) {
                if pkg_child.kind() == "package_identifier" {
                    pkg_name = node_text(&pkg_child, source).to_string();
                }
            }
            // Fallback: try field name
            if pkg_name.is_empty() {
                if let Some(name_node) = child.child_by_field_name("name") {
                    pkg_name = node_text(&name_node, source).to_string();
                }
            }
        }
    }

    let dir = std::path::Path::new(file)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let mod_key = if dir.is_empty() {
        pkg_name.clone()
    } else {
        dir
    };
    if !mod_key.is_empty() {
        fragment.modules.entry(mod_key).or_insert_with(|| Module {
            path: pkg_name,
            file: file.to_string(),
            ..Default::default()
        });
    }

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "type_declaration" => extract_type_decl(&child, source, file, fragment),
            "function_declaration" => {
                extract_function(&child, source, file, is_test, fragment);
            }
            "method_declaration" => {
                extract_method(&child, source, file, is_test, fragment);
            }
            _ => {}
        }
    }
}

fn extract_type_decl(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type_spec" {
            let name = match child.child_by_field_name("name") {
                Some(n) => node_text(&n, source).to_string(),
                None => continue,
            };
            let vis = go_visibility(&name);

            let type_node = match child.child_by_field_name("type") {
                Some(n) => n,
                None => continue,
            };

            match type_node.kind() {
                "struct_type" => {
                    let fields = extract_struct_fields(&type_node, source);
                    let key = name.clone();
                    fragment.types.insert(
                        key,
                        TypeDef {
                            name,
                            source: source_loc(file, &child),
                            kind: TypeKind::Struct,
                            fields,
                            visibility: vis,
                            ..Default::default()
                        },
                    );
                }
                "interface_type" => {
                    let methods = extract_interface_methods(&type_node, source);
                    let key = name.clone();
                    fragment.types.insert(
                        key,
                        TypeDef {
                            name,
                            source: source_loc(file, &child),
                            kind: TypeKind::Interface,
                            methods,
                            visibility: vis,
                            ..Default::default()
                        },
                    );
                }
                _ => {
                    // type alias or other
                    let key = name.clone();
                    fragment.types.insert(
                        key,
                        TypeDef {
                            name,
                            source: source_loc(file, &child),
                            kind: TypeKind::TypeAlias,
                            visibility: vis,
                            ..Default::default()
                        },
                    );
                }
            }
        }
    }
}

fn extract_struct_fields(node: &Node, source: &str) -> Vec<Field> {
    let mut fields = Vec::new();
    // tree-sitter-go: struct_type has a field_declaration_list child
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "field_declaration_list" {
            let mut list_cursor = child.walk();
            for field_node in child.named_children(&mut list_cursor) {
                if field_node.kind() == "field_declaration" {
                    extract_single_field(&field_node, source, &mut fields);
                }
            }
        } else if child.kind() == "field_declaration" {
            extract_single_field(&child, source, &mut fields);
        }
    }
    fields
}

fn extract_single_field(field_node: &Node, source: &str, fields: &mut Vec<Field>) {
    let type_node = field_node.child_by_field_name("type");
    let type_name = type_node
        .map(|t| node_text(&t, source).to_string())
        .unwrap_or_default();

    let mut inner = field_node.walk();
    for name_child in field_node.named_children(&mut inner) {
        if name_child.kind() == "field_identifier" {
            let name = node_text(&name_child, source).to_string();
            let optional = type_name.starts_with('*');
            fields.push(Field {
                name,
                type_name: type_name.clone(),
                optional,
            });
        }
    }
}

fn extract_interface_methods(node: &Node, source: &str) -> Vec<String> {
    let mut methods = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // method_spec in tree-sitter-go
        if child.kind() == "method_spec" || child.kind() == "method_elem" {
            if let Some(name_node) = child.child_by_field_name("name") {
                methods.push(node_text(&name_node, source).to_string());
            }
        }
    }
    methods
}

fn extract_function(
    node: &Node,
    source: &str,
    file: &str,
    is_test_file: bool,
    fragment: &mut ManifestFragment,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source).to_string(),
        None => return,
    };
    let vis = go_visibility(&name);
    let is_test = is_test_file && name.starts_with("Test");
    let sig = build_fn_signature(node, source, &name);

    let key = name.clone();
    fragment.functions.insert(
        key,
        Function {
            name,
            source: source_loc(file, node),
            signature: sig,
            visibility: vis,
            is_async: false, // Go doesn't have async keyword
            is_test,
        },
    );
}

fn extract_method(
    node: &Node,
    source: &str,
    file: &str,
    is_test_file: bool,
    fragment: &mut ManifestFragment,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source).to_string(),
        None => return,
    };

    // Get receiver type name
    let receiver_type = extract_receiver_type(node, source);

    let vis = go_visibility(&name);
    let is_test = is_test_file && name.starts_with("Test");
    let sig = build_fn_signature(node, source, &name);

    // Register as a qualified function
    let qualified = if receiver_type.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", receiver_type, name)
    };
    fragment.functions.insert(
        qualified,
        Function {
            name: name.clone(),
            source: source_loc(file, node),
            signature: sig,
            visibility: vis,
            is_async: false,
            is_test,
        },
    );

    // Add method to existing type
    if !receiver_type.is_empty() {
        if let Some(typedef) = fragment.types.get_mut(&receiver_type) {
            if !typedef.methods.contains(&name) {
                typedef.methods.push(name);
            }
        }
    }
}

fn extract_receiver_type(node: &Node, source: &str) -> String {
    let receiver = match node.child_by_field_name("receiver") {
        Some(r) => r,
        None => return String::new(),
    };
    let mut cursor = receiver.walk();
    for child in receiver.named_children(&mut cursor) {
        if child.kind() == "parameter_declaration" {
            if let Some(type_node) = child.child_by_field_name("type") {
                let text = node_text(&type_node, source);
                return text.trim_start_matches('*').to_string();
            }
        }
    }
    String::new()
}

fn build_fn_signature(node: &Node, source: &str, name: &str) -> String {
    let params = node
        .child_by_field_name("parameters")
        .map(|n| node_text(&n, source))
        .unwrap_or("()");
    let result = node
        .child_by_field_name("result")
        .map(|n| format!(" {}", node_text(&n, source)))
        .unwrap_or_default();
    format!("func {name}{params}{result}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TypeKind;
    use std::fs;
    use tempfile::tempdir;

    fn parse_go(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_go(&tree.root_node(), source, "main.go", false, &mut fragment);
        fragment
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = GoAdapter;
        let ctx = ProjectContext {
            root: "/tmp".into(),
            kind: ProjectKind::Go,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&ctx));

        let ctx2 = ProjectContext {
            kind: ProjectKind::Cargo,
            ..ctx
        };
        assert!(!adapter.detect(&ctx2));
    }

    #[test]
    fn struct_extracted() {
        let frag = parse_go(
            r#"
package models

type User struct {
    Name  string
    Email string
    Age   int
}
"#,
        );
        assert!(frag.types.contains_key("User"));
        let t = &frag.types["User"];
        assert_eq!(t.kind, TypeKind::Struct);
        assert_eq!(t.visibility, Visibility::Public);
        assert_eq!(t.fields.len(), 3);
        assert_eq!(t.fields[0].name, "Name");
    }

    #[test]
    fn unexported_struct() {
        let frag = parse_go(
            r#"
package internal

type config struct {
    debug bool
}
"#,
        );
        let t = &frag.types["config"];
        assert_eq!(t.visibility, Visibility::Private);
    }

    #[test]
    fn interface_extracted() {
        let frag = parse_go(
            r#"
package store

type Repository interface {
    Get(id string) error
    Save(item Item) error
}
"#,
        );
        let t = &frag.types["Repository"];
        assert_eq!(t.kind, TypeKind::Interface);
        assert_eq!(t.methods.len(), 2);
        assert!(t.methods.contains(&"Get".to_string()));
        assert!(t.methods.contains(&"Save".to_string()));
    }

    #[test]
    fn exported_function() {
        let frag = parse_go(
            r#"
package main

func ProcessData(input string) (string, error) {
    return input, nil
}
"#,
        );
        let f = &frag.functions["ProcessData"];
        assert_eq!(f.visibility, Visibility::Public);
        assert!(f.signature.contains("ProcessData"));
    }

    #[test]
    fn unexported_function() {
        let frag = parse_go(
            r#"
package main

func helper() int {
    return 42
}
"#,
        );
        let f = &frag.functions["helper"];
        assert_eq!(f.visibility, Visibility::Private);
    }

    #[test]
    fn method_adds_to_type() {
        let frag = parse_go(
            r#"
package main

type Server struct {
    Port int
}

func (s *Server) Start() error {
    return nil
}

func (s *Server) Stop() {
}
"#,
        );
        let t = &frag.types["Server"];
        assert!(t.methods.contains(&"Start".to_string()));
        assert!(t.methods.contains(&"Stop".to_string()));
        assert!(frag.functions.contains_key("Server.Start"));
    }

    #[test]
    fn test_function_detected() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let source = r#"
package main

func TestAdd(t *testing.T) {
}
"#;
        let tree = parser.parse(source, None).unwrap();
        let mut frag = ManifestFragment::default();
        extract_go(&tree.root_node(), source, "main_test.go", true, &mut frag);
        let f = &frag.functions["TestAdd"];
        assert!(f.is_test);
    }

    #[test]
    fn package_creates_module() {
        let frag = parse_go(
            r#"
package handlers

func Index() {}
"#,
        );
        assert!(!frag.modules.is_empty());
    }

    #[test]
    fn pointer_field_is_optional() {
        let frag = parse_go(
            r#"
package main

type Config struct {
    Name    string
    Parent  *Config
}
"#,
        );
        let t = &frag.types["Config"];
        assert!(!t.fields[0].optional); // string
        assert!(t.fields[1].optional); // *Config
    }

    #[test]
    fn full_extract_on_tempdir() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module example.com/test\n\ngo 1.21\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("main.go"),
            r#"package main

type App struct {
    Name string
}

func NewApp(name string) *App {
    return &App{Name: name}
}
"#,
        )
        .unwrap();

        let kind = uu_detect::detect(dir.path()).unwrap();
        let ctx = ProjectContext::build(dir.path(), &kind).unwrap();
        let adapter = GoAdapter;
        assert!(adapter.detect(&ctx));
        let frag = adapter.extract(&ctx).unwrap();
        assert!(frag.types.contains_key("App"));
        assert!(frag.functions.contains_key("NewApp"));
    }
}
