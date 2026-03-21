//! JavaScript language adapter — extracts classes, functions, and module exports.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};
use uu_detect::ProjectKind;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct JavaScriptAdapter;

impl Adapter for JavaScriptAdapter {
    fn name(&self) -> &str {
        "javascript"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Node { .. }) && !ctx.root.join("tsconfig.json").exists()
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec![
            "node_modules",
            "dist",
            "build",
            ".next",
            "coverage",
            ".turbo",
        ];

        for file in &ctx.files {
            let ext = match file.extension().and_then(|e| e.to_str()) {
                Some(e) => e,
                None => continue,
            };

            if !matches!(ext, "js" | "jsx" | "mjs" | "cjs") {
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

            extract_javascript(&tree.root_node(), &source, &rel, &mut fragment);
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        99
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

/// Derive a module name from a relative file path.
/// e.g. "src/utils/helpers.js" → "src/utils/helpers"
fn module_name_from_file(rel_path: &str) -> String {
    for ext in &[".jsx", ".mjs", ".cjs", ".js"] {
        if let Some(stripped) = rel_path.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    rel_path.to_string()
}

fn extract_javascript(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let module_name = module_name_from_file(file);
    let mut exports = Vec::new();

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                extract_export(&child, source, file, &module_name, fragment, &mut exports);
            }
            "class_declaration" => {
                extract_class(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
            }
            "function_declaration" => {
                extract_function_decl(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
            }
            "lexical_declaration" | "variable_declaration" => {
                extract_lexical_functions(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
            }
            "expression_statement" => {
                // Check for module.exports = ...
                extract_commonjs_export(&child, source, file, &module_name, fragment, &mut exports);
            }
            _ => {}
        }
    }

    let module = Module {
        path: module_name.clone(),
        file: file.to_string(),
        exports,
        ..Default::default()
    };
    fragment.modules.insert(module_name, module);
}

fn extract_export(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    fragment: &mut ManifestFragment,
    exports: &mut Vec<String>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class_declaration" => {
                let name = extract_class(
                    &child,
                    source,
                    file,
                    module_name,
                    Visibility::Public,
                    fragment,
                );
                if let Some(n) = name {
                    exports.push(n);
                }
            }
            "function_declaration" => {
                let name = extract_function_decl(
                    &child,
                    source,
                    file,
                    module_name,
                    Visibility::Public,
                    fragment,
                );
                if let Some(n) = name {
                    exports.push(n);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let names = extract_lexical_functions(
                    &child,
                    source,
                    file,
                    module_name,
                    Visibility::Public,
                    fragment,
                );
                exports.extend(names);
            }
            _ => {}
        }
    }
}

fn extract_class(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    visibility: Visibility,
    fragment: &mut ManifestFragment,
) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    let name_str = node_text(&name, source);
    let full_name = format!("{}.{}", module_name, name_str);

    // Extract extends
    let mut implements = Vec::new();
    extract_heritage(node, source, &mut implements);

    // Extract methods from class body
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        extract_class_methods(&body, source, file, &full_name, fragment, &mut methods);
    }

    let type_def = TypeDef {
        name: name_str.clone(),
        source: file.to_string(),
        kind: TypeKind::Class,
        visibility,
        implements,
        methods,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
    Some(name_str)
}

fn extract_heritage(node: &Node, source: &str, implements: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let mut inner_cursor = child.walk();
            for inner in child.named_children(&mut inner_cursor) {
                let text = node_text(&inner, source);
                if !text.is_empty() {
                    implements.push(text);
                }
            }
        }
    }
}

fn extract_class_methods(
    body: &Node,
    source: &str,
    file: &str,
    class_name: &str,
    fragment: &mut ManifestFragment,
    methods: &mut Vec<String>,
) {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "method_definition" {
            let name = match child.child_by_field_name("name") {
                Some(n) => node_text(&n, source),
                None => continue,
            };

            let is_async = has_child_kind(&child, "async");

            let params = child
                .child_by_field_name("parameters")
                .map(|p| node_text(&p, source))
                .unwrap_or_default();

            let signature = if is_async {
                format!("async {}{}", name, params)
            } else {
                format!("{}{}", name, params)
            };

            let qualified = format!("{}.{}", class_name, name);
            let function = Function {
                name: name.clone(),
                source: file.to_string(),
                signature,
                visibility: Visibility::Public,
                is_async,
                ..Default::default()
            };
            fragment.functions.insert(qualified, function);
            methods.push(name);
        }
    }
}

fn extract_function_decl(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    visibility: Visibility,
    fragment: &mut ManifestFragment,
) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    let name_str = node_text(&name, source);
    let full_name = format!("{}.{}", module_name, name_str);

    let is_async = has_child_kind(node, "async");

    let params = node
        .child_by_field_name("parameters")
        .map(|p| node_text(&p, source))
        .unwrap_or_default();

    let async_prefix = if is_async { "async " } else { "" };
    let signature = format!("{}function {}{}", async_prefix, name_str, params);

    let function = Function {
        name: name_str.clone(),
        source: file.to_string(),
        signature,
        visibility,
        is_async,
        ..Default::default()
    };
    fragment.functions.insert(full_name, function);
    Some(name_str)
}

/// Extract arrow function or function expression from lexical/variable declarations.
fn extract_lexical_functions(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    visibility: Visibility,
    fragment: &mut ManifestFragment,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name = match child.child_by_field_name("name") {
                Some(n) => node_text(&n, source),
                None => continue,
            };

            let value = match child.child_by_field_name("value") {
                Some(v) => v,
                None => continue,
            };

            let is_func = matches!(
                value.kind(),
                "arrow_function" | "function_expression" | "function"
            );

            if !is_func {
                continue;
            }

            let is_async = has_child_kind(&value, "async");
            let full_name = format!("{}.{}", module_name, name);

            let params = value
                .child_by_field_name("parameters")
                .map(|p| node_text(&p, source))
                .unwrap_or_default();

            let async_prefix = if is_async { "async " } else { "" };
            let signature = format!(
                "{}const {} = {}",
                async_prefix,
                name,
                params.chars().take(50).collect::<String>()
            );

            let function = Function {
                name: name.clone(),
                source: file.to_string(),
                signature,
                visibility: visibility.clone(),
                is_async,
                ..Default::default()
            };
            fragment.functions.insert(full_name, function);
            names.push(name);
        }
    }
    names
}

/// Handle `module.exports = { ... }` or `module.exports.foo = function() { ... }`
fn extract_commonjs_export(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    fragment: &mut ManifestFragment,
    exports: &mut Vec<String>,
) {
    // Look for assignment_expression children
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "assignment_expression" {
            let left = match child.child_by_field_name("left") {
                Some(l) => l,
                None => continue,
            };

            let left_text = node_text(&left, source);

            // module.exports = something
            if left_text == "module.exports" {
                let right = match child.child_by_field_name("right") {
                    Some(r) => r,
                    None => continue,
                };

                // If it's a function, register it
                if matches!(
                    right.kind(),
                    "arrow_function" | "function_expression" | "function"
                ) {
                    let full_name = format!("{}.default", module_name);
                    let function = Function {
                        name: "default".to_string(),
                        source: file.to_string(),
                        signature: "module.exports".to_string(),
                        visibility: Visibility::Public,
                        ..Default::default()
                    };
                    fragment.functions.insert(full_name, function);
                    exports.push("default".to_string());
                }

                // If it's an object, extract its shorthand properties
                if right.kind() == "object" {
                    let mut obj_cursor = right.walk();
                    for prop in right.named_children(&mut obj_cursor) {
                        if prop.kind() == "shorthand_property_identifier" {
                            let name = node_text(&prop, source);
                            exports.push(name);
                        } else if prop.kind() == "pair" {
                            if let Some(key) = prop.child_by_field_name("key") {
                                let name = node_text(&key, source);
                                exports.push(name);
                            }
                        }
                    }
                }
            }

            // module.exports.foo = ...
            if left_text.starts_with("module.exports.") {
                if let Some(name) = left_text.strip_prefix("module.exports.") {
                    let right = match child.child_by_field_name("right") {
                        Some(r) => r,
                        None => continue,
                    };

                    if matches!(
                        right.kind(),
                        "arrow_function" | "function_expression" | "function"
                    ) {
                        let full_name = format!("{}.{}", module_name, name);
                        let function = Function {
                            name: name.to_string(),
                            source: file.to_string(),
                            signature: format!("module.exports.{}", name),
                            visibility: Visibility::Public,
                            ..Default::default()
                        };
                        fragment.functions.insert(full_name, function);
                    }

                    exports.push(name.to_string());
                }
            }
        }
    }
}

fn has_child_kind(node: &Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    let result = node.children(&mut cursor).any(|c| c.kind() == kind);
    result
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_js(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_javascript(&tree.root_node(), source, "src/utils.js", &mut fragment);
        fragment
    }

    #[test]
    fn class_detected() {
        let frag = parse_js(
            r#"
class UserService {
    constructor(db) {
        this.db = db;
    }

    async findById(id) {
        return this.db.find(id);
    }
}
"#,
        );

        assert!(frag.types.contains_key("src/utils.UserService"));
        let td = &frag.types["src/utils.UserService"];
        assert_eq!(td.kind, TypeKind::Class);
        assert!(td.methods.contains(&"constructor".to_string()));
        assert!(td.methods.contains(&"findById".to_string()));

        let find_method = &frag.functions["src/utils.UserService.findById"];
        assert!(find_method.is_async);
    }

    #[test]
    fn class_extends() {
        let frag = parse_js(
            r#"
class Admin extends User {
    get role() {
        return "admin";
    }
}
"#,
        );

        let td = &frag.types["src/utils.Admin"];
        assert!(
            td.implements.iter().any(|i| i.contains("User")),
            "Expected User in implements: {:?}",
            td.implements
        );
    }

    #[test]
    fn exported_function() {
        let frag = parse_js(
            r#"
export function createUser(name) {
    return { name };
}
"#,
        );

        assert!(frag.functions.contains_key("src/utils.createUser"));
        let func = &frag.functions["src/utils.createUser"];
        assert_eq!(func.visibility, Visibility::Public);
    }

    #[test]
    fn arrow_function_export() {
        let frag = parse_js(
            r#"
export const greet = (name) => {
    return `Hello, ${name}`;
};
"#,
        );

        assert!(frag.functions.contains_key("src/utils.greet"));
        let func = &frag.functions["src/utils.greet"];
        assert_eq!(func.visibility, Visibility::Public);
    }

    #[test]
    fn commonjs_module_exports_function() {
        let frag = parse_js(
            r#"
function helper() {
    return 42;
}

module.exports.helper = function() {};
"#,
        );

        // The named export via module.exports.helper
        assert!(
            frag.modules["src/utils"]
                .exports
                .contains(&"helper".to_string()),
            "Expected helper in exports: {:?}",
            frag.modules["src/utils"].exports
        );
    }

    #[test]
    fn commonjs_module_exports_object() {
        let frag = parse_js(
            r#"
function add(a, b) { return a + b; }
function subtract(a, b) { return a - b; }

module.exports = { add, subtract };
"#,
        );

        let exports = &frag.modules["src/utils"].exports;
        assert!(exports.contains(&"add".to_string()));
        assert!(exports.contains(&"subtract".to_string()));
    }

    #[test]
    fn non_exported_is_private() {
        let frag = parse_js(
            r#"
function helper() {
    return 42;
}
"#,
        );

        let func = &frag.functions["src/utils.helper"];
        assert_eq!(func.visibility, Visibility::Private);
    }

    #[test]
    fn async_function_detected() {
        let frag = parse_js(
            r#"
export async function fetchData(url) {
}
"#,
        );

        let func = &frag.functions["src/utils.fetchData"];
        assert!(func.is_async);
    }

    #[test]
    fn module_created_from_file() {
        let frag = parse_js("");

        assert!(frag.modules.contains_key("src/utils"));
        let module = &frag.modules["src/utils"];
        assert_eq!(module.file, "src/utils.js");
    }

    #[test]
    fn detect_without_tsconfig() {
        let adapter = JavaScriptAdapter;

        // Non-Node project → false
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

    #[test]
    fn module_name_stripping() {
        assert_eq!(module_name_from_file("src/app.js"), "src/app");
        assert_eq!(module_name_from_file("src/app.jsx"), "src/app");
        assert_eq!(module_name_from_file("lib/utils.mjs"), "lib/utils");
        assert_eq!(module_name_from_file("lib/config.cjs"), "lib/config");
    }
}
