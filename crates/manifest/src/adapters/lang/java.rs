//! Java language adapter — extracts classes, interfaces, enums, and methods.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};
use uu_detect::ProjectKind;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct JavaAdapter;

impl Adapter for JavaAdapter {
    fn name(&self) -> &str {
        "java"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Gradle { .. } | ProjectKind::Maven)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec!["build", "target", ".gradle"];

        for file in &ctx.files {
            if file.extension().and_then(|e| e.to_str()) != Some("java") {
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

            extract_java(&tree.root_node(), &source, &rel, &mut fragment);
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

fn extract_java(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // Extract package name
    let package = find_package(root, source);

    // Walk top-level declarations
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "class_declaration" => {
                extract_class(&child, source, file, fragment, &package, TypeKind::Class);
            }
            "interface_declaration" => {
                extract_class(
                    &child,
                    source,
                    file,
                    fragment,
                    &package,
                    TypeKind::Interface,
                );
            }
            "enum_declaration" => {
                extract_class(&child, source, file, fragment, &package, TypeKind::Enum);
            }
            _ => {}
        }
    }

    // Create a module entry for the file
    if !package.is_empty() {
        let module = Module {
            path: package.clone(),
            file: file.to_string(),
            ..Default::default()
        };
        fragment
            .modules
            .insert(format!("{}:{}", package, file), module);
    }
}

fn find_package(root: &Node, source: &str) -> String {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "package_declaration" {
            // Package name is a scoped_identifier or identifier child
            let mut pkg_cursor = child.walk();
            for pkg_child in child.named_children(&mut pkg_cursor) {
                if pkg_child.kind() == "scoped_identifier" || pkg_child.kind() == "identifier" {
                    return node_text(&pkg_child, source);
                }
            }
        }
    }
    String::new()
}

fn extract_class(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    package: &str,
    kind: TypeKind,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let full_name = if package.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", package, name)
    };

    // Determine visibility from modifiers
    let visibility = extract_type_visibility(node, source);

    // Extract implements/extends
    let mut implements = Vec::new();

    // superclass: `extends Base`
    if let Some(superclass) = node.child_by_field_name("superclass") {
        // The superclass field contains a type_identifier
        let mut sc_cursor = superclass.walk();
        for child in superclass.named_children(&mut sc_cursor) {
            let text = node_text(&child, source);
            if !text.is_empty() {
                implements.push(text);
            }
        }
        // If no named children, the superclass itself is the type
        if implements.is_empty() {
            let text = node_text(&superclass, source);
            if !text.is_empty() {
                implements.push(text);
            }
        }
    }

    // interfaces: `implements X, Y`
    if let Some(interfaces) = node.child_by_field_name("interfaces") {
        extract_type_list(&interfaces, source, &mut implements);
    }

    let variants = if kind == TypeKind::Enum {
        extract_enum_variants(node, source)
    } else {
        Vec::new()
    };

    // Extract methods from the body
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
        if child.kind() != "enum_constant" {
            continue;
        }

        if let Some(name_node) = child.child_by_field_name("name") {
            variants.push(node_text(&name_node, source));
        }
    }
    variants
}

fn extract_type_visibility(node: &Node, source: &str) -> Visibility {
    // Check for modifiers preceding the declaration
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mod_text = node_text(&child, source);
            if mod_text.contains("public") {
                return Visibility::Public;
            } else if mod_text.contains("private") {
                return Visibility::Private;
            } else if mod_text.contains("protected") {
                return Visibility::Internal;
            }
        }
    }
    // Java default: package-private
    Visibility::Internal
}

fn extract_method_visibility(node: &Node, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mod_text = node_text(&child, source);
            if mod_text.contains("public") {
                return Visibility::Public;
            } else if mod_text.contains("private") {
                return Visibility::Private;
            } else if mod_text.contains("protected") {
                return Visibility::Internal;
            }
        }
    }
    Visibility::Private
}

fn extract_type_list(node: &Node, source: &str, out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type_identifier" || child.kind() == "generic_type" {
            out.push(node_text(&child, source));
        } else if child.kind() == "type_list" {
            // Recurse into type_list
            extract_type_list(&child, source, out);
        }
    }
}

fn extract_methods(
    body: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    class_name: &str,
    methods: &mut Vec<String>,
) {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "method_declaration" {
            let method_name = match child.child_by_field_name("name") {
                Some(n) => node_text(&n, source),
                None => continue,
            };

            let visibility = extract_method_visibility(&child, source);

            // Check for annotations
            let is_test = has_annotation(&child, source, "Test");

            // Build signature
            let return_type = child
                .child_by_field_name("type")
                .map(|t| node_text(&t, source))
                .unwrap_or_default();
            let params = child
                .child_by_field_name("parameters")
                .map(|p| node_text(&p, source))
                .unwrap_or_default();
            let signature = format!("{} {}{}", return_type, method_name, params);

            let qualified = format!("{}.{}", class_name, method_name);

            let function = Function {
                name: method_name.clone(),
                source: file.to_string(),
                signature,
                visibility,
                is_test,
                ..Default::default()
            };
            fragment.functions.insert(qualified, function);
            methods.push(method_name);
        }
    }
}

fn has_annotation(node: &Node, source: &str, annotation_name: &str) -> bool {
    // Annotations are in the modifiers child, as marker_annotation or annotation
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mut mod_cursor = child.walk();
            for mod_child in child.named_children(&mut mod_cursor) {
                if mod_child.kind() == "marker_annotation" || mod_child.kind() == "annotation" {
                    if let Some(name) = mod_child.child_by_field_name("name") {
                        if node_text(&name, source) == annotation_name {
                            return true;
                        }
                    }
                }
            }
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

    fn parse_java(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_java(
            &tree.root_node(),
            source,
            "src/main/java/App.java",
            &mut fragment,
        );
        fragment
    }

    #[test]
    fn class_detected() {
        let frag = parse_java(
            r#"
package com.example;

public class UserService {
    public void createUser(String name) {
    }
}
"#,
        );

        assert!(frag.types.contains_key("com.example.UserService"));
        let td = &frag.types["com.example.UserService"];
        assert_eq!(td.kind, TypeKind::Class);
        assert_eq!(td.visibility, Visibility::Public);
    }

    #[test]
    fn interface_detected() {
        let frag = parse_java(
            r#"
public interface Repository {
    void save(Object entity);
}
"#,
        );

        assert!(frag.types.contains_key("Repository"));
        let td = &frag.types["Repository"];
        assert_eq!(td.kind, TypeKind::Interface);
    }

    #[test]
    fn enum_detected() {
        let frag = parse_java(
            r#"
public enum Status {
    ACTIVE,
    INACTIVE
}
"#,
        );

        assert!(frag.types.contains_key("Status"));
        let td = &frag.types["Status"];
        assert_eq!(td.kind, TypeKind::Enum);
        assert_eq!(td.variants, vec!["ACTIVE", "INACTIVE"]);
    }

    #[test]
    fn implements_and_extends() {
        let frag = parse_java(
            r#"
public class UserService extends BaseService implements Serializable, Cloneable {
    public void doWork() {
    }
}
"#,
        );

        let td = &frag.types["UserService"];
        assert!(td.implements.contains(&"BaseService".to_string()));
        assert!(td.implements.contains(&"Serializable".to_string()));
        assert!(td.implements.contains(&"Cloneable".to_string()));
    }

    #[test]
    fn method_visibility() {
        let frag = parse_java(
            r#"
public class App {
    public void publicMethod() {
    }

    private void privateMethod() {
    }

    protected void protectedMethod() {
    }
}
"#,
        );

        assert_eq!(
            frag.functions["App.publicMethod"].visibility,
            Visibility::Public
        );
        assert_eq!(
            frag.functions["App.privateMethod"].visibility,
            Visibility::Private
        );
        assert_eq!(
            frag.functions["App.protectedMethod"].visibility,
            Visibility::Internal
        );
    }

    #[test]
    fn test_annotation_detected() {
        let frag = parse_java(
            r#"
public class AppTest {
    @Test
    public void testSomething() {
    }

    public void helper() {
    }
}
"#,
        );

        assert!(frag.functions["AppTest.testSomething"].is_test);
        assert!(!frag.functions["AppTest.helper"].is_test);
    }

    #[test]
    fn package_creates_module() {
        let frag = parse_java(
            r#"
package com.example.app;

public class Main {
}
"#,
        );

        assert!(frag.modules.values().any(|m| m.path == "com.example.app"));
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = JavaAdapter;

        let gradle_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Gradle { wrapper: true },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&gradle_ctx));

        let maven_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Maven,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&maven_ctx));

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
