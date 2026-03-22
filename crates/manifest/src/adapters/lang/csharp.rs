//! C# language adapter — extracts classes, interfaces, enums, records, and methods.

use std::path::Path;

use anyhow::Result;
use project_detect::ProjectKind;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct CSharpAdapter;

impl Adapter for CSharpAdapter {
    fn name(&self) -> &str {
        "csharp"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::DotNet { .. })
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec!["bin", "obj"];

        for file in &ctx.files {
            if file.extension().and_then(|e| e.to_str()) != Some("cs") {
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

            extract_csharp(&tree.root_node(), &source, &rel, &mut fragment);
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

fn extract_csharp(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    walk_csharp(root, source, file, fragment, &[]);
}

fn walk_csharp(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    namespace_stack: &[String],
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "namespace_declaration" | "file_scoped_namespace_declaration" => {
                handle_namespace(&child, source, file, fragment, namespace_stack);
            }
            "class_declaration" => {
                extract_type_decl(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Class,
                    namespace_stack,
                );
            }
            "interface_declaration" => {
                extract_type_decl(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Interface,
                    namespace_stack,
                );
            }
            "enum_declaration" => {
                extract_type_decl(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Enum,
                    namespace_stack,
                );
            }
            "record_declaration" => {
                extract_type_decl(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Struct,
                    namespace_stack,
                );
            }
            "struct_declaration" => {
                extract_type_decl(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Struct,
                    namespace_stack,
                );
            }
            _ => {
                walk_csharp(&child, source, file, fragment, namespace_stack);
            }
        }
    }
}

fn handle_namespace(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    parent_namespaces: &[String],
) {
    let ns_name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let full_ns = if parent_namespaces.is_empty() {
        ns_name.clone()
    } else {
        format!("{}.{}", parent_namespaces.join("."), ns_name)
    };

    let module = Module {
        path: full_ns.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment
        .modules
        .insert(format!("{}:{}", full_ns, file), module);

    let mut new_stack = parent_namespaces.to_vec();
    new_stack.push(ns_name);

    // Walk children inside the namespace body
    if let Some(body) = node.child_by_field_name("body") {
        walk_csharp(&body, source, file, fragment, &new_stack);
    }

    // For file-scoped namespaces, walk the remaining children directly
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "struct_declaration" => {
                let kind = match child.kind() {
                    "class_declaration" => TypeKind::Class,
                    "interface_declaration" => TypeKind::Interface,
                    "enum_declaration" => TypeKind::Enum,
                    "record_declaration" | "struct_declaration" => TypeKind::Struct,
                    _ => continue,
                };
                extract_type_decl(&child, source, file, fragment, kind, &new_stack);
            }
            _ => {}
        }
    }
}

fn extract_type_decl(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    kind: TypeKind,
    namespace_stack: &[String],
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let full_name = if namespace_stack.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", namespace_stack.join("."), name)
    };

    let visibility = extract_modifier_visibility(node, source);
    let implements = extract_bases(node, source);
    let variants = if kind == TypeKind::Enum {
        extract_enum_variants(node, source)
    } else {
        Vec::new()
    };

    // Extract methods
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        extract_methods(&body, source, file, fragment, &full_name, &mut methods);
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
    let Some(body) = node.child_by_field_name("body") else {
        return Vec::new();
    };

    let mut variants = Vec::new();
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if let Some(name_node) = child.child_by_field_name("name") {
            variants.push(node_text(&name_node, source));
        }
    }
    variants
}

fn extract_modifier_visibility(node: &Node, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" {
            let text = node_text(&child, source);
            match text.as_str() {
                "public" => return Visibility::Public,
                "private" => return Visibility::Private,
                "internal" | "protected" => return Visibility::Internal,
                _ => {}
            }
        }
    }
    // C# default is internal for top-level types
    Visibility::Internal
}

fn extract_bases(node: &Node, source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "base_list" {
            let mut inner = child.walk();
            for base_child in child.named_children(&mut inner) {
                // Each item can be an identifier, qualified_name, generic_name, etc.
                let text = node_text(&base_child, source);
                let text = text.trim().to_string();
                if !text.is_empty() {
                    result.push(text);
                }
            }
        }
    }
    result
}

fn extract_methods(
    body: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    type_name: &str,
    methods: &mut Vec<String>,
) {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "method_declaration" {
            let method_name = match child.child_by_field_name("name") {
                Some(n) => node_text(&n, source),
                None => continue,
            };

            let visibility = extract_modifier_visibility(&child, source);

            let is_async = has_modifier(&child, source, "async");

            // Build signature
            let return_type = child
                .child_by_field_name("type")
                .or_else(|| child.child_by_field_name("returns"))
                .map(|t| node_text(&t, source))
                .unwrap_or_default();
            let params = child
                .child_by_field_name("parameters")
                .map(|p| node_text(&p, source))
                .unwrap_or_default();
            let signature = format!("{} {}{}", return_type, method_name, params);

            let qualified = format!("{}.{}", type_name, method_name);

            let function = Function {
                name: method_name.clone(),
                source: file.to_string(),
                signature,
                visibility,
                is_async,
                ..Default::default()
            };
            fragment.functions.insert(qualified, function);
            methods.push(method_name);
        }
    }
}

fn has_modifier(node: &Node, source: &str, modifier: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" && node_text(&child, source) == modifier {
            return true;
        }
    }
    false
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_csharp(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_csharp(&tree.root_node(), source, "src/App.cs", &mut fragment);
        fragment
    }

    #[test]
    fn class_detected() {
        let frag = parse_csharp(
            r#"
namespace MyApp {
    public class UserService {
    }
}
"#,
        );

        assert!(
            frag.types.contains_key("MyApp.UserService"),
            "Expected MyApp.UserService, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["MyApp.UserService"];
        assert_eq!(td.kind, TypeKind::Class);
        assert_eq!(td.visibility, Visibility::Public);
    }

    #[test]
    fn interface_detected() {
        let frag = parse_csharp(
            r#"
public interface IRepository {
    void Save(object entity);
}
"#,
        );

        assert!(
            frag.types.contains_key("IRepository"),
            "Expected IRepository, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["IRepository"];
        assert_eq!(td.kind, TypeKind::Interface);
    }

    #[test]
    fn enum_detected() {
        let frag = parse_csharp(
            r#"
public enum Status {
    Active,
    Inactive
}
"#,
        );

        assert!(
            frag.types.contains_key("Status"),
            "Expected Status, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Status"];
        assert_eq!(td.kind, TypeKind::Enum);
        assert_eq!(td.variants, vec!["Active", "Inactive"]);
    }

    #[test]
    fn record_detected_as_struct() {
        let frag = parse_csharp(
            r#"
public record Point(int X, int Y);
"#,
        );

        assert!(
            frag.types.contains_key("Point"),
            "Expected Point, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Point"];
        assert_eq!(td.kind, TypeKind::Struct);
    }

    #[test]
    fn namespace_creates_module() {
        let frag = parse_csharp(
            r#"
namespace MyApp.Services {
    public class App { }
}
"#,
        );

        assert!(
            frag.modules.values().any(|m| m.path == "MyApp.Services"),
            "modules: {:?}",
            frag.modules
        );
    }

    #[test]
    fn implements_and_extends() {
        let frag = parse_csharp(
            r#"
public class UserService : BaseService, IDisposable {
    public void Dispose() { }
}
"#,
        );

        let td = &frag.types["UserService"];
        assert!(
            td.implements.iter().any(|i| i.contains("BaseService")),
            "Expected BaseService in {:?}",
            td.implements
        );
        assert!(
            td.implements.iter().any(|i| i.contains("IDisposable")),
            "Expected IDisposable in {:?}",
            td.implements
        );
    }

    #[test]
    fn method_visibility() {
        let frag = parse_csharp(
            r#"
public class App {
    public void PublicMethod() { }
    private void PrivateMethod() { }
    internal void InternalMethod() { }
}
"#,
        );

        assert_eq!(
            frag.functions["App.PublicMethod"].visibility,
            Visibility::Public,
        );
        assert_eq!(
            frag.functions["App.PrivateMethod"].visibility,
            Visibility::Private,
        );
        assert_eq!(
            frag.functions["App.InternalMethod"].visibility,
            Visibility::Internal,
        );
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = CSharpAdapter;

        let dotnet_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::DotNet { sln: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&dotnet_ctx));

        let dotnet_sln_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::DotNet { sln: true },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&dotnet_sln_ctx));

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
