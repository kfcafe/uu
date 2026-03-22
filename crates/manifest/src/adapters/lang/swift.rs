//! Swift language adapter — extracts structs, classes, protocols, enums, and functions.

use std::path::Path;

use anyhow::Result;
use project_detect::ProjectKind;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct SwiftAdapter;

impl Adapter for SwiftAdapter {
    fn name(&self) -> &str {
        "swift"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Swift)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_swift::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec![".build"];

        for file in &ctx.files {
            if file.extension().and_then(|e| e.to_str()) != Some("swift") {
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

            extract_swift(&tree.root_node(), &source, &rel, &mut fragment);
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

fn extract_swift(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        walk_swift(&child, source, file, fragment, None);
    }

    // Create a module entry for the file
    let module_name = file
        .strip_suffix(".swift")
        .unwrap_or(file)
        .replace('/', ".");
    let module = Module {
        path: module_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment.modules.insert(module_name, module);
}

fn walk_swift(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    enclosing_type: Option<&str>,
) {
    match node.kind() {
        "class_declaration" => {
            // tree-sitter-swift uses class_declaration for class, struct, and enum.
            // Determine the actual kind from the keyword child node.
            let kind = infer_class_kind(node);
            extract_type_decl(node, source, file, fragment, kind, enclosing_type);
        }
        "protocol_declaration" => {
            extract_type_decl(
                node,
                source,
                file,
                fragment,
                TypeKind::Protocol,
                enclosing_type,
            );
        }
        "function_declaration" => {
            extract_function(node, source, file, fragment, enclosing_type);
        }
        _ => {
            // Recurse into children
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_swift(&child, source, file, fragment, enclosing_type);
            }
        }
    }
}

/// Determine the actual type kind from a class_declaration node by inspecting
/// the keyword child (struct, enum, class).
fn infer_class_kind(node: &Node) -> TypeKind {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "struct" => return TypeKind::Struct,
            "enum" => return TypeKind::Enum,
            "class" => return TypeKind::Class,
            _ => {}
        }
    }
    TypeKind::Class
}

fn extract_type_decl(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    kind: TypeKind,
    enclosing_type: Option<&str>,
) {
    let name = match find_name_child(node, source) {
        Some(n) => n,
        None => return,
    };

    let full_name = match enclosing_type {
        Some(parent) => format!("{}.{}", parent, name),
        None => name.clone(),
    };

    let visibility = extract_visibility(node, source);

    // Extract inheritance (protocols/superclass)
    let implements = extract_inheritance(node, source);
    let variants = if kind == TypeKind::Enum {
        extract_enum_variants(node, source)
    } else {
        Vec::new()
    };

    // Extract methods from the body — look in class_body, enum_class_body, or protocol_body
    let mut methods = Vec::new();
    let body = find_body_child(node);
    if let Some(body) = body {
        let mut body_cursor = body.walk();
        for child in body.named_children(&mut body_cursor) {
            if child.kind() == "function_declaration" {
                let method_name = match find_name_child(&child, source) {
                    Some(n) => n,
                    None => continue,
                };
                extract_function(&child, source, file, fragment, Some(&full_name));
                methods.push(method_name);
            } else {
                // Recurse for nested types
                walk_swift(&child, source, file, fragment, Some(&full_name));
            }
        }
    }

    let type_def = TypeDef {
        name: name.clone(),
        source: file.to_string(),
        kind,
        visibility,
        implements,
        variants,
        methods,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
}

fn extract_enum_variants(node: &Node, source: &str) -> Vec<String> {
    let Some(body) = find_body_child(node) else {
        return Vec::new();
    };

    let mut variants = Vec::new();
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() != "enum_entry" {
            continue;
        }

        if let Some(name_node) = child.child_by_field_name("name") {
            variants.push(node_text(&name_node, source));
        }
    }
    variants
}

/// Find the body child of a type declaration. tree-sitter-swift uses different
/// body node names: class_body, enum_class_body, protocol_body.
fn find_body_child<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    if let Some(body) = node.child_by_field_name("body") {
        return Some(body);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "class_body" | "enum_class_body" | "protocol_body"
        ) {
            return Some(child);
        }
    }
    None
}

/// Find the name of a declaration node. tree-sitter-swift uses `type_identifier`
/// for type names and `simple_identifier` for function names.
fn find_name_child(node: &Node, source: &str) -> Option<String> {
    // Try the `name` field first
    if let Some(n) = node.child_by_field_name("name") {
        return Some(node_text(&n, source));
    }
    // Fall back to scanning children for type_identifier or simple_identifier
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "simple_identifier" => {
                return Some(node_text(&child, source));
            }
            _ => {}
        }
    }
    None
}

fn extract_function(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    enclosing_type: Option<&str>,
) {
    let name = match find_name_child(node, source) {
        Some(n) => n,
        None => return,
    };

    let visibility = extract_visibility(node, source);
    let is_async = has_modifier(node, source, "async");

    // Build signature from the first line
    let sig = node_text(node, source);
    let signature = sig
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('{')
        .trim()
        .to_string();

    let qualified = match enclosing_type {
        Some(parent) => format!("{}.{}", parent, name),
        None => name.clone(),
    };

    let function = Function {
        name,
        source: file.to_string(),
        signature,
        visibility,
        is_async,
        ..Default::default()
    };
    fragment.functions.insert(qualified, function);
}

fn extract_visibility(node: &Node, source: &str) -> Visibility {
    // Check modifiers before the declaration
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if kind == "modifiers" || kind == "modifier" {
            let text = node_text(&child, source);
            if text.contains("public") || text.contains("open") {
                return Visibility::Public;
            } else if text.contains("private") || text.contains("fileprivate") {
                return Visibility::Private;
            }
            // "internal" is explicit but also the default
        }
        // Also check direct modifier children
        if kind == "visibility_modifier" || kind == "ownership_modifier" {
            let text = node_text(&child, source);
            if text.contains("public") || text.contains("open") {
                return Visibility::Public;
            } else if text.contains("private") || text.contains("fileprivate") {
                return Visibility::Private;
            }
        }
    }
    // Swift default is internal
    Visibility::Internal
}

fn has_modifier(node: &Node, source: &str, modifier: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let text = node_text(&child, source);
        if text == modifier {
            return true;
        }
    }
    false
}

fn extract_inheritance(node: &Node, source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = node.walk();
    // In tree-sitter-swift, inheritance specifiers are direct children of class_declaration
    // e.g. class MyView: UIView, Codable { }
    // Produces: [class] [type_identifier:"MyView"] [:] [inheritance_specifier] [,] [inheritance_specifier] [class_body]
    for child in node.children(&mut cursor) {
        if child.kind() == "inheritance_specifier" {
            // Inside: user_type → type_identifier
            let mut inner_cursor = child.walk();
            for inner_child in child.named_children(&mut inner_cursor) {
                if inner_child.kind() == "user_type" {
                    let mut type_cursor = inner_child.walk();
                    for type_child in inner_child.named_children(&mut type_cursor) {
                        if type_child.kind() == "type_identifier" {
                            let text = node_text(&type_child, source);
                            if !text.is_empty() {
                                result.push(text);
                            }
                        }
                    }
                } else if inner_child.kind() == "type_identifier" {
                    let text = node_text(&inner_child, source);
                    if !text.is_empty() {
                        result.push(text);
                    }
                }
            }
        }
    }
    result
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_swift(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_swift(
            &tree.root_node(),
            source,
            "Sources/App.swift",
            &mut fragment,
        );
        fragment
    }

    #[test]
    fn struct_detected() {
        let frag = parse_swift("struct Point { }");

        assert!(
            frag.types.contains_key("Point"),
            "Expected Point, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Point"];
        assert_eq!(td.kind, TypeKind::Struct);
    }

    #[test]
    fn class_detected() {
        let frag = parse_swift("public class UserService { }");

        assert!(
            frag.types.contains_key("UserService"),
            "Expected UserService, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["UserService"];
        assert_eq!(td.kind, TypeKind::Class);
        assert_eq!(td.visibility, Visibility::Public);
    }

    #[test]
    fn protocol_detected() {
        let frag = parse_swift("protocol Drawable { }");

        assert!(
            frag.types.contains_key("Drawable"),
            "Expected Drawable, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Drawable"];
        assert_eq!(td.kind, TypeKind::Protocol);
    }

    #[test]
    fn enum_detected() {
        let frag = parse_swift(
            r#"
enum Direction {
    case north
    case south
}
"#,
        );

        assert!(
            frag.types.contains_key("Direction"),
            "Expected Direction, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Direction"];
        assert_eq!(td.kind, TypeKind::Enum);
        assert_eq!(td.variants, vec!["north", "south"]);
    }

    #[test]
    fn visibility_levels() {
        let frag = parse_swift(
            r#"
public func publicFunc() { }
func internalFunc() { }
private func privateFunc() { }
"#,
        );

        assert_eq!(
            frag.functions.get("publicFunc").map(|f| &f.visibility),
            Some(&Visibility::Public),
            "functions: {:?}",
            frag.functions.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            frag.functions.get("internalFunc").map(|f| &f.visibility),
            Some(&Visibility::Internal),
        );
        assert_eq!(
            frag.functions.get("privateFunc").map(|f| &f.visibility),
            Some(&Visibility::Private),
        );
    }

    #[test]
    fn method_in_class() {
        let frag = parse_swift(
            r#"
class App {
    public func run() { }
    private func setup() { }
}
"#,
        );

        assert!(frag.functions.contains_key("App.run"));
        assert!(frag.functions.contains_key("App.setup"));
        let td = &frag.types["App"];
        assert!(td.methods.contains(&"run".to_string()));
        assert!(td.methods.contains(&"setup".to_string()));
    }

    #[test]
    fn protocol_conformance() {
        let frag = parse_swift(
            r#"
class MyView: UIView, Codable {
}
"#,
        );

        let td = &frag.types["MyView"];
        assert!(
            td.implements.iter().any(|i| i.contains("UIView")),
            "Expected UIView in {:?}",
            td.implements
        );
        assert!(
            td.implements.iter().any(|i| i.contains("Codable")),
            "Expected Codable in {:?}",
            td.implements
        );
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = SwiftAdapter;

        let swift_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Swift,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&swift_ctx));

        let rust_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Cargo,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!adapter.detect(&rust_ctx));
    }
}
