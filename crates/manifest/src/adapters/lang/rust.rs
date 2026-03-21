//! Rust language adapter — extracts structs, enums, traits, impls, and functions.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};
use uu_detect::ProjectKind;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Field, Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct RustAdapter;

impl Adapter for RustAdapter {
    fn name(&self) -> &str {
        "rust"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Cargo)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec!["target", ".git"];

        for file in &ctx.files {
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("rs")) {
                continue;
            }
            if should_skip(file, &ctx.root, &skip_dirs) {
                continue;
            }

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let tree = match parser.parse(&source, None) {
                Some(t) => t,
                None => continue,
            };

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            extract_rust(&tree.root_node(), &source, &rel, &mut fragment);
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

/// Derive module path from a relative file path.
/// "src/auth/mod.rs" → "auth", "src/main.rs" → "main"
fn module_path_from_file(rel: &str) -> String {
    let without_ext = rel.strip_suffix(".rs").unwrap_or(rel);
    let cleaned = without_ext.strip_prefix("src/").unwrap_or(without_ext);
    let dotted = cleaned.replace('/', "::");
    dotted.strip_suffix("::mod").unwrap_or(&dotted).to_string()
}

fn source_loc(file: &str, node: &Node) -> String {
    format!("{}:{}", file, node.start_position().row + 1)
}

fn extract_rust(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mod_name = module_path_from_file(file);
    let module = Module {
        path: mod_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment.modules.insert(mod_name.clone(), module);

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "struct_item" => extract_struct(&child, source, file, fragment),
            "enum_item" => extract_enum(&child, source, file, fragment),
            "trait_item" => extract_trait(&child, source, file, fragment),
            "impl_item" => extract_impl(&child, source, file, fragment),
            "function_item" => extract_function(&child, source, file, fragment),
            "mod_item" => extract_mod(&child, source, file, &mod_name, fragment),
            _ => {}
        }
    }
}

fn get_visibility(node: &Node, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = node_text(&child, source);
            return if text.contains("pub(crate)") || text.contains("pub(super)") {
                Visibility::Internal
            } else {
                Visibility::Public
            };
        }
    }
    Visibility::Private
}

fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn get_name(node: &Node, source: &str) -> Option<String> {
    // Different item types store the name in different child fields
    for field_name in &["name", "type"] {
        if let Some(name_node) = node.child_by_field_name(field_name) {
            return Some(node_text(&name_node, source).to_string());
        }
    }
    None
}

fn extract_struct(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let name = match get_name(node, source) {
        Some(n) => n,
        None => return,
    };
    let vis = get_visibility(node, source);
    let fields = extract_fields(node, source);

    let key = name.clone();
    fragment.types.insert(
        key,
        TypeDef {
            name,
            source: source_loc(file, node),
            kind: TypeKind::Struct,
            fields,
            visibility: vis,
            ..Default::default()
        },
    );
}

fn extract_fields(node: &Node, source: &str) -> Vec<Field> {
    let mut fields = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "field_declaration" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();
                    let type_name = child
                        .child_by_field_name("type")
                        .map(|t| node_text(&t, source).to_string())
                        .unwrap_or_default();
                    let optional = type_name.starts_with("Option<");
                    fields.push(Field {
                        name,
                        type_name,
                        optional,
                    });
                }
            }
        }
    }
    fields
}

fn extract_enum(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let name = match get_name(node, source) {
        Some(n) => n,
        None => return,
    };
    let vis = get_visibility(node, source);

    let mut variants = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "enum_variant" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    variants.push(node_text(&name_node, source).to_string());
                }
            }
        }
    }

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
}

fn extract_trait(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let name = match get_name(node, source) {
        Some(n) => n,
        None => return,
    };
    let vis = get_visibility(node, source);

    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "function_signature_item" || child.kind() == "function_item" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    methods.push(node_text(&name_node, source).to_string());
                }
            }
        }
    }

    let key = name.clone();
    fragment.types.insert(
        key,
        TypeDef {
            name,
            source: source_loc(file, node),
            kind: TypeKind::Trait,
            methods,
            visibility: vis,
            ..Default::default()
        },
    );
}

fn extract_impl(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // Get the type being implemented
    let type_node = match node.child_by_field_name("type") {
        Some(n) => n,
        None => return,
    };
    let type_name = node_text(&type_node, source).to_string();

    // Check if it's a trait impl: `impl Trait for Type`
    let trait_name = node
        .child_by_field_name("trait")
        .map(|t| node_text(&t, source).to_string());

    // Collect method names
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "function_item" {
                let vis = get_visibility(&child, source);
                if let Some(name_node) = child.child_by_field_name("name") {
                    let method_name = node_text(&name_node, source).to_string();
                    methods.push(method_name.clone());

                    // Also register public impl methods as functions
                    if matches!(vis, Visibility::Public) {
                        let sig = build_fn_signature(&child, source);
                        let is_async = has_async(&child, source);
                        let is_test = has_test_attr(&child, source);
                        let qualified = format!("{}::{}", type_name, method_name);
                        fragment.functions.insert(
                            qualified,
                            Function {
                                name: method_name,
                                source: source_loc(file, &child),
                                signature: sig,
                                visibility: vis,
                                is_async,
                                is_test,
                            },
                        );
                    }
                }
            }
        }
    }

    // If we have an existing type entry, add methods and trait impl
    if let Some(typedef) = fragment.types.get_mut(&type_name) {
        for m in &methods {
            if !typedef.methods.contains(m) {
                typedef.methods.push(m.clone());
            }
        }
        if let Some(trait_name) = &trait_name {
            if !typedef.implements.contains(trait_name) {
                typedef.implements.push(trait_name.clone());
            }
        }
    }
}

fn extract_function(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source).to_string(),
        None => return,
    };
    let vis = get_visibility(node, source);
    let sig = build_fn_signature(node, source);
    let is_async = has_async(node, source);
    let is_test = has_test_attr(node, source);

    let key = name.clone();
    fragment.functions.insert(
        key,
        Function {
            name,
            source: source_loc(file, node),
            signature: sig,
            visibility: vis,
            is_async,
            is_test,
        },
    );
}

fn extract_mod(
    node: &Node,
    source: &str,
    _file: &str,
    parent_mod: &str,
    fragment: &mut ManifestFragment,
) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let mod_name = node_text(&name_node, source).to_string();
        // Add to parent module's exports
        if let Some(parent) = fragment.modules.get_mut(parent_mod) {
            parent.exports.push(mod_name);
        }
    }
}

fn build_fn_signature(node: &Node, source: &str) -> String {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or("?");
    let params = node
        .child_by_field_name("parameters")
        .map(|n| node_text(&n, source))
        .unwrap_or("()");
    let ret = node
        .child_by_field_name("return_type")
        .map(|n| format!(" -> {}", node_text(&n, source)))
        .unwrap_or_default();
    let async_prefix = if has_async(node, source) {
        "async "
    } else {
        ""
    };
    format!("{async_prefix}fn {name}{params}{ret}")
}

fn has_async(node: &Node, source: &str) -> bool {
    // Check for "async" keyword before "fn"
    let text = node_text(node, source);
    text.starts_with("async ") || text.starts_with("pub async ") || text.contains(" async fn ")
}

fn has_test_attr(node: &Node, source: &str) -> bool {
    // Look for #[test] or #[cfg(test)] attribute on the node
    // Attributes are siblings before the function in tree-sitter-rust
    if let Some(parent) = node.parent() {
        let idx = node.start_byte();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            if child.start_byte() >= idx {
                break;
            }
            if child.kind() == "attribute_item" || child.kind() == "inner_attribute_item" {
                let text = node_text(&child, source);
                if text.contains("test") {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TypeKind;
    use std::fs;
    use tempfile::tempdir;

    fn parse_rust(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_rust(&tree.root_node(), source, "src/lib.rs", &mut fragment);
        fragment
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = RustAdapter;
        let ctx = ProjectContext {
            root: "/tmp".into(),
            kind: ProjectKind::Cargo,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&ctx));

        let ctx2 = ProjectContext {
            kind: ProjectKind::Go,
            ..ctx
        };
        assert!(!adapter.detect(&ctx2));
    }

    #[test]
    fn pub_struct_extracted() {
        let frag = parse_rust(
            r#"
pub struct User {
    pub name: String,
    pub age: u32,
    pub email: Option<String>,
}
"#,
        );
        assert!(frag.types.contains_key("User"));
        let t = &frag.types["User"];
        assert_eq!(t.kind, TypeKind::Struct);
        assert_eq!(t.visibility, Visibility::Public);
        assert_eq!(t.fields.len(), 3);
        assert_eq!(t.fields[0].name, "name");
        assert_eq!(t.fields[0].type_name, "String");
        assert!(!t.fields[0].optional);
        assert!(t.fields[2].optional); // Option<String>
    }

    #[test]
    fn private_struct_extracted() {
        let frag = parse_rust("struct Internal { x: i32 }");
        let t = &frag.types["Internal"];
        assert_eq!(t.visibility, Visibility::Private);
    }

    #[test]
    fn enum_extracted() {
        let frag = parse_rust(
            r#"
pub enum Color {
    Red,
    Green,
    Blue,
}
"#,
        );
        let t = &frag.types["Color"];
        assert_eq!(t.kind, TypeKind::Enum);
        assert_eq!(t.variants, vec!["Red", "Green", "Blue"]);
    }

    #[test]
    fn trait_extracted() {
        let frag = parse_rust(
            r#"
pub trait Drawable {
    fn draw(&self);
    fn resize(&mut self, w: u32, h: u32);
}
"#,
        );
        let t = &frag.types["Drawable"];
        assert_eq!(t.kind, TypeKind::Trait);
        assert_eq!(t.methods.len(), 2);
        assert!(t.methods.contains(&"draw".to_string()));
        assert!(t.methods.contains(&"resize".to_string()));
    }

    #[test]
    fn pub_function_extracted() {
        let frag = parse_rust("pub fn process(input: &str) -> Result<String> { todo!() }");
        let f = &frag.functions["process"];
        assert_eq!(f.visibility, Visibility::Public);
        assert!(f.signature.contains("process"));
        assert!(f.signature.contains("-> Result<String>"));
    }

    #[test]
    fn async_function_detected() {
        let frag = parse_rust("pub async fn fetch_data(url: &str) -> Vec<u8> { todo!() }");
        let f = &frag.functions["fetch_data"];
        assert!(f.is_async);
        assert!(f.signature.starts_with("async fn"));
    }

    #[test]
    fn impl_adds_methods_to_type() {
        let frag = parse_rust(
            r#"
pub struct Foo { val: i32 }
impl Foo {
    pub fn new(val: i32) -> Self { Self { val } }
    fn internal(&self) {}
}
"#,
        );
        let t = &frag.types["Foo"];
        assert!(t.methods.contains(&"new".to_string()));
        assert!(t.methods.contains(&"internal".to_string()));
        // Public method registered as a function
        assert!(frag.functions.contains_key("Foo::new"));
    }

    #[test]
    fn trait_impl_adds_implements() {
        let frag = parse_rust(
            r#"
pub struct MyStruct;
impl Display for MyStruct {
    fn fmt(&self, f: &mut Formatter) -> Result { todo!() }
}
"#,
        );
        let t = &frag.types["MyStruct"];
        assert!(t.implements.contains(&"Display".to_string()));
    }

    #[test]
    fn module_created_from_file() {
        let frag = parse_rust("pub fn hello() {}");
        assert!(frag.modules.contains_key("lib"));
    }

    #[test]
    fn source_location_format() {
        let frag = parse_rust("\npub struct Pos;");
        assert_eq!(frag.types["Pos"].source, "src/lib.rs:2");
    }

    #[test]
    fn full_extract_on_tempdir() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub struct Foo { pub x: i32 }\npub fn bar() -> bool { true }\n",
        )
        .unwrap();

        let kind = uu_detect::detect(dir.path()).unwrap();
        let ctx = ProjectContext::build(dir.path(), &kind).unwrap();
        let adapter = RustAdapter;
        assert!(adapter.detect(&ctx));
        let frag = adapter.extract(&ctx).unwrap();
        assert!(frag.types.contains_key("Foo"));
        assert!(frag.functions.contains_key("bar"));
    }
}
