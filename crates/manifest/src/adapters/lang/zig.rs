//! Zig language adapter — extracts structs, enums, unions, functions, and tests.

use std::path::Path;

use anyhow::Result;
use project_detect::ProjectKind;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Field, Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct ZigAdapter;

impl Adapter for ZigAdapter {
    fn name(&self) -> &str {
        "zig"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Zig)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_zig::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec![".git", "zig-out", ".zig-cache", "zig-cache"];

        for file in &ctx.files {
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("zig")) {
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

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let tree = match parser.parse(&source, None) {
                Some(t) => t,
                None => continue,
            };

            extract_zig(&tree.root_node(), &source, &rel, &mut fragment);
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

/// Check whether a declaration node starts with `pub`.
fn is_pub(node: &Node, source: &str) -> bool {
    let text = node_text(node, source);
    text.starts_with("pub ")
}

/// Find the first named child with the given node kind.
fn find_child_by_kind<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node.named_children(&mut cursor).find(|c| c.kind() == kind);
    result
}

fn extract_zig(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // Create module from file path
    let mod_key = file.trim_end_matches(".zig").replace('/', "::");
    if !mod_key.is_empty() {
        fragment.modules.entry(mod_key).or_insert_with(|| Module {
            path: file.trim_end_matches(".zig").to_string(),
            file: file.to_string(),
            ..Default::default()
        });
    }

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "variable_declaration" => {
                extract_variable_decl(&child, source, file, fragment);
            }
            "function_declaration" => {
                extract_function(&child, source, file, None, fragment);
            }
            "test_declaration" => {
                extract_test(&child, source, file, fragment);
            }
            _ => {}
        }
    }
}

/// Extract a `const`/`var` declaration. If the value is a struct, enum, union,
/// or error set, register it as a type. Otherwise skip.
fn extract_variable_decl(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // Find the name from the first identifier child (field names unreliable
    // in tree-sitter-zig — child_by_field_name("name") returns None).
    let name = match find_child_by_kind(node, "identifier") {
        Some(n) => node_text(&n, source).to_string(),
        None => return,
    };

    let vis = if is_pub(node, source) {
        Visibility::Public
    } else {
        Visibility::Private
    };

    // Find the initializer expression — look for struct/enum/union/error_set
    // among the node's children.
    let mut inner = node.walk();
    for child in node.named_children(&mut inner) {
        match child.kind() {
            "struct_declaration" => {
                let (fields, methods) =
                    extract_container_members(&child, source, file, &name, fragment);
                let key = name.clone();
                fragment.types.insert(
                    key,
                    TypeDef {
                        name,
                        source: source_loc(file, node),
                        kind: TypeKind::Struct,
                        fields,
                        methods,
                        visibility: vis,
                        ..Default::default()
                    },
                );
                return;
            }
            "enum_declaration" => {
                let (variants, methods) =
                    extract_enum_members(&child, source, file, &name, fragment);
                let key = name.clone();
                fragment.types.insert(
                    key,
                    TypeDef {
                        name,
                        source: source_loc(file, node),
                        kind: TypeKind::Enum,
                        variants,
                        methods,
                        visibility: vis,
                        ..Default::default()
                    },
                );
                return;
            }
            "union_declaration" => {
                let (fields, methods) =
                    extract_container_members(&child, source, file, &name, fragment);
                let key = name.clone();
                fragment.types.insert(
                    key,
                    TypeDef {
                        name,
                        source: source_loc(file, node),
                        kind: TypeKind::Union,
                        fields,
                        methods,
                        visibility: vis,
                        ..Default::default()
                    },
                );
                return;
            }
            "error_set_declaration" => {
                let variants = extract_error_set_members(&child, source);
                let key = name.clone();
                fragment.types.insert(
                    key,
                    TypeDef {
                        name,
                        source: source_loc(file, node),
                        kind: TypeKind::Enum,
                        variants,
                        visibility: vis,
                        ..Default::default()
                    },
                );
                return;
            }
            _ => {}
        }
    }
}

/// Extract fields and method names from a struct or union declaration.
/// Also registers methods as qualified functions in the fragment.
fn extract_container_members(
    node: &Node,
    source: &str,
    file: &str,
    type_name: &str,
    fragment: &mut ManifestFragment,
) -> (Vec<Field>, Vec<String>) {
    let mut fields = Vec::new();
    let mut methods = Vec::new();

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "container_field" => {
                if let Some(field) = extract_container_field(&child, source) {
                    fields.push(field);
                }
            }
            "function_declaration" => {
                if let Some(fn_name) = child.child_by_field_name("name") {
                    let method_name = node_text(&fn_name, source).to_string();
                    methods.push(method_name.clone());
                    extract_function(&child, source, file, Some(type_name), fragment);
                }
            }
            _ => {}
        }
    }

    (fields, methods)
}

/// Extract a single container field (struct/union field).
fn extract_container_field(node: &Node, source: &str) -> Option<Field> {
    // Find the field name — first identifier child.
    let name_node = find_child_by_kind(node, "identifier")?;
    let name = node_text(&name_node, source).to_string();

    // Find the type — the named child after the identifier that isn't the
    // identifier itself. In practice it's a type expression like slice_type,
    // builtin_type, nullable_type, etc.
    let mut type_name = String::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.id() != name_node.id() && child.kind() != "identifier" {
            type_name = node_text(&child, source).to_string();
            break;
        }
    }

    let optional = type_name.starts_with('?');

    Some(Field {
        name,
        type_name,
        optional,
    })
}

/// Extract variant names and methods from an enum declaration.
fn extract_enum_members(
    node: &Node,
    source: &str,
    file: &str,
    type_name: &str,
    fragment: &mut ManifestFragment,
) -> (Vec<String>, Vec<String>) {
    let mut variants = Vec::new();
    let mut methods = Vec::new();

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "container_field" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if name_node.kind() == "identifier" {
                        variants.push(node_text(&name_node, source).to_string());
                    }
                }
            }
            "function_declaration" => {
                if let Some(fn_name) = child.child_by_field_name("name") {
                    let method_name = node_text(&fn_name, source).to_string();
                    methods.push(method_name.clone());
                    extract_function(&child, source, file, Some(type_name), fragment);
                }
            }
            _ => {}
        }
    }

    (variants, methods)
}

/// Extract error names from an error set declaration (`error { A, B, C }`).
fn extract_error_set_members(node: &Node, source: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            members.push(node_text(&child, source).to_string());
        }
    }
    members
}

/// Extract a function declaration. If `parent_type` is Some, this is a method.
fn extract_function(
    node: &Node,
    source: &str,
    file: &str,
    parent_type: Option<&str>,
    fragment: &mut ManifestFragment,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source).to_string(),
        None => return,
    };

    let vis = if is_pub(node, source) {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let sig = build_fn_signature(node, source, &name);

    let key = match parent_type {
        Some(t) => format!("{t}.{name}"),
        None => name.clone(),
    };

    fragment.functions.insert(
        key,
        Function {
            name,
            source: source_loc(file, node),
            signature: sig,
            visibility: vis,
            is_async: false,
            is_test: false,
        },
    );
}

/// Extract a test declaration as a test function.
fn extract_test(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // Test name comes from the string child: `test "name" { ... }`
    let mut test_name = String::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "string" {
            let raw = node_text(&child, source);
            test_name = raw.trim_matches('"').to_string();
            break;
        }
    }

    if test_name.is_empty() {
        test_name = format!("test_{}", node.start_position().row + 1);
    }

    let key = format!("test \"{}\"", test_name);
    fragment.functions.insert(
        key,
        Function {
            name: test_name,
            source: source_loc(file, node),
            signature: String::new(),
            visibility: Visibility::Private,
            is_async: false,
            is_test: true,
        },
    );
}

fn build_fn_signature(node: &Node, source: &str, name: &str) -> String {
    let params = node
        .child_by_field_name("parameters")
        .map(|n| node_text(&n, source))
        .unwrap_or("()");

    // The return type is in the `type` field
    let ret = node
        .child_by_field_name("type")
        .map(|n| node_text(&n, source))
        .unwrap_or("void");

    format!("fn {name}{params} {ret}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TypeKind;
    use std::fs;
    use tempfile::tempdir;

    fn parse_zig(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_zig::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_zig(&tree.root_node(), source, "main.zig", &mut fragment);
        fragment
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = ZigAdapter;
        let ctx = ProjectContext {
            root: "/tmp".into(),
            kind: ProjectKind::Zig,
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
    fn pub_struct_extracted() {
        let frag = parse_zig(
            r#"
pub const User = struct {
    name: []const u8,
    age: u32,
    email: ?[]const u8,
};
"#,
        );
        assert!(frag.types.contains_key("User"));
        let t = &frag.types["User"];
        assert_eq!(t.kind, TypeKind::Struct);
        assert_eq!(t.visibility, Visibility::Public);
        assert_eq!(t.fields.len(), 3);
        assert_eq!(t.fields[0].name, "name");
        assert!(!t.fields[0].optional);
        assert!(t.fields[2].optional); // ?[]const u8
    }

    #[test]
    fn private_struct() {
        let frag = parse_zig(
            r#"
const Config = struct {
    debug: bool,
};
"#,
        );
        let t = &frag.types["Config"];
        assert_eq!(t.visibility, Visibility::Private);
    }

    #[test]
    fn enum_extracted() {
        let frag = parse_zig(
            r#"
pub const Color = enum {
    red,
    green,
    blue,
};
"#,
        );
        let t = &frag.types["Color"];
        assert_eq!(t.kind, TypeKind::Enum);
        assert_eq!(t.variants.len(), 3);
        assert!(t.variants.contains(&"red".to_string()));
        assert!(t.variants.contains(&"green".to_string()));
        assert!(t.variants.contains(&"blue".to_string()));
    }

    #[test]
    fn union_extracted() {
        let frag = parse_zig(
            r#"
pub const Value = union {
    int: i64,
    float: f64,
};
"#,
        );
        let t = &frag.types["Value"];
        assert_eq!(t.kind, TypeKind::Union);
        assert_eq!(t.fields.len(), 2);
    }

    #[test]
    fn error_set_extracted() {
        let frag = parse_zig(
            r#"
pub const AllocError = error {
    OutOfMemory,
    InvalidAlignment,
};
"#,
        );
        let t = &frag.types["AllocError"];
        assert_eq!(t.kind, TypeKind::Enum);
        assert!(t.variants.contains(&"OutOfMemory".to_string()));
        assert!(t.variants.contains(&"InvalidAlignment".to_string()));
    }

    #[test]
    fn pub_function_extracted() {
        let frag = parse_zig(
            r#"
pub fn add(a: i32, b: i32) i32 {
    return a + b;
}
"#,
        );
        let f = &frag.functions["add"];
        assert_eq!(f.visibility, Visibility::Public);
        assert!(f.signature.contains("add"));
    }

    #[test]
    fn private_function() {
        let frag = parse_zig(
            r#"
fn helper() void {}
"#,
        );
        let f = &frag.functions["helper"];
        assert_eq!(f.visibility, Visibility::Private);
    }

    #[test]
    fn struct_method_extracted() {
        let frag = parse_zig(
            r#"
pub const Server = struct {
    port: u16,

    pub fn start(self: *Server) void {}
    fn stop(self: *Server) void {}
};
"#,
        );
        let t = &frag.types["Server"];
        assert!(t.methods.contains(&"start".to_string()));
        assert!(t.methods.contains(&"stop".to_string()));
        assert!(frag.functions.contains_key("Server.start"));
        assert!(frag.functions.contains_key("Server.stop"));
    }

    #[test]
    fn test_declaration_detected() {
        let frag = parse_zig(
            r#"
test "addition works" {
    const x = 1 + 1;
    _ = x;
}
"#,
        );
        let f = &frag.functions["test \"addition works\""];
        assert!(f.is_test);
        assert_eq!(f.name, "addition works");
    }

    #[test]
    fn module_created_from_file() {
        let frag = parse_zig("pub fn init() void {}");
        assert!(frag.modules.contains_key("main"));
    }

    #[test]
    fn optional_field_detected() {
        let frag = parse_zig(
            r#"
const Opts = struct {
    timeout: ?u64,
    name: []const u8,
};
"#,
        );
        let t = &frag.types["Opts"];
        assert!(t.fields[0].optional); // ?u64
        assert!(!t.fields[1].optional); // []const u8
    }

    #[test]
    fn full_extract_on_tempdir() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("build.zig"), "").unwrap();
        fs::write(
            dir.path()
                .join("src")
                .join("main.zig")
                .to_string_lossy()
                .to_string(),
            "",
        )
        .unwrap_or_default();

        // Create src directory and write file
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("main.zig"),
            r#"const std = @import("std");

pub const App = struct {
    name: []const u8,

    pub fn init(name: []const u8) App {
        return .{ .name = name };
    }
};

pub fn main() !void {
    const app = App.init("hello");
    _ = app;
}

test "app init" {
    const app = App.init("test");
    _ = app;
}
"#,
        )
        .unwrap();

        let kind = project_detect::detect(dir.path()).unwrap();
        let ctx = ProjectContext::build(dir.path(), &kind).unwrap();
        let adapter = ZigAdapter;
        assert!(adapter.detect(&ctx));
        let frag = adapter.extract(&ctx).unwrap();
        assert!(frag.types.contains_key("App"));
        assert!(frag.functions.contains_key("main"));
        assert!(frag.functions.contains_key("App.init"));
    }
}
