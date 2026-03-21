//! TypeScript language adapter — extracts interfaces, types, classes, enums, and functions.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};
use uu_detect::ProjectKind;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct TypeScriptAdapter;

impl Adapter for TypeScriptAdapter {
    fn name(&self) -> &str {
        "typescript"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Node { .. }) && ctx.root.join("tsconfig.json").exists()
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        let mut ts_parser = Parser::new();
        ts_parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())?;

        let mut tsx_parser = Parser::new();
        tsx_parser.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())?;

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

            // Skip .d.ts declaration files
            if file.to_string_lossy().ends_with(".d.ts") {
                continue;
            }

            let is_tsx = match ext {
                "ts" => false,
                "tsx" => true,
                _ => continue,
            };

            if should_skip(file, &ctx.root, &skip_dirs) {
                continue;
            }

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let parser = if is_tsx {
                &mut tsx_parser
            } else {
                &mut ts_parser
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

            extract_typescript(&tree.root_node(), &source, &rel, &mut fragment);
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

/// Derive a module name from a relative file path.
/// e.g. "src/models/user.ts" → "src/models/user"
fn module_name_from_file(rel_path: &str) -> String {
    rel_path
        .strip_suffix(".tsx")
        .or_else(|| rel_path.strip_suffix(".ts"))
        .unwrap_or(rel_path)
        .to_string()
}

fn extract_typescript(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let module_name = module_name_from_file(file);
    let mut exports = Vec::new();

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            // export interface Foo { ... }
            "export_statement" => {
                extract_export(&child, source, file, &module_name, fragment, &mut exports);
            }
            // interface Foo { ... } (non-exported)
            "interface_declaration" => {
                extract_interface(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
            }
            // type Foo = ... (non-exported)
            "type_alias_declaration" => {
                extract_type_alias(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
            }
            // enum Foo { ... } (non-exported)
            "enum_declaration" => {
                extract_enum(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
            }
            // class Foo { ... } (non-exported)
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
            // function foo() { ... } (non-exported)
            "function_declaration" => {
                extract_function_decl(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    false,
                    fragment,
                );
            }
            // const foo = () => ... (non-exported)
            "lexical_declaration" => {
                extract_lexical_functions(
                    &child,
                    source,
                    file,
                    &module_name,
                    Visibility::Private,
                    fragment,
                );
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
    // Check for `export default`
    let is_default = has_child_kind(node, "default");

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "interface_declaration" => {
                let name = extract_interface(
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
            "type_alias_declaration" => {
                let name = extract_type_alias(
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
            "enum_declaration" => {
                let name = extract_enum(
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
                    is_default,
                    fragment,
                );
                if let Some(n) = name {
                    exports.push(n);
                }
            }
            "lexical_declaration" => {
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

fn extract_interface(
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

    // Extract extends clause
    let mut implements = Vec::new();
    extract_heritage(node, source, &mut implements);

    let type_def = TypeDef {
        name: name_str.clone(),
        source: file.to_string(),
        kind: TypeKind::Interface,
        visibility,
        implements,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
    Some(name_str)
}

fn extract_type_alias(
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

    let type_def = TypeDef {
        name: name_str.clone(),
        source: file.to_string(),
        kind: TypeKind::TypeAlias,
        visibility,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
    Some(name_str)
}

fn extract_enum(
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
    let variants = extract_enum_variants(node, source);

    let type_def = TypeDef {
        name: name_str.clone(),
        source: file.to_string(),
        kind: TypeKind::Enum,
        variants,
        visibility,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
    Some(name_str)
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
        // extends_clause or implements_clause in TS AST
        if child.kind() == "extends_clause"
            || child.kind() == "extends_type_clause"
            || child.kind() == "implements_clause"
        {
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
        if child.kind() == "method_definition" || child.kind() == "public_field_definition" {
            let name = match child.child_by_field_name("name") {
                Some(n) => node_text(&n, source),
                None => continue,
            };

            // Only track method_definition as methods
            if child.kind() == "method_definition" {
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
}

fn extract_function_decl(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    visibility: Visibility,
    _is_default: bool,
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

    let return_type = node
        .child_by_field_name("return_type")
        .map(|r| node_text(&r, source))
        .unwrap_or_default();

    let async_prefix = if is_async { "async " } else { "" };
    let signature = format!(
        "{}function {}{}{}",
        async_prefix, name_str, params, return_type
    );

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

/// Extract arrow function or function expression exports from lexical declarations.
/// e.g. `const foo = () => ...` or `const bar = function() { ... }`
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

            // Check if value is an arrow function or function expression
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

    fn parse_ts(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_typescript(&tree.root_node(), source, "src/models.ts", &mut fragment);
        fragment
    }

    fn parse_tsx(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_typescript(&tree.root_node(), source, "src/App.tsx", &mut fragment);
        fragment
    }

    #[test]
    fn interface_detected() {
        let frag = parse_ts(
            r#"
export interface User {
    name: string;
    email: string;
}
"#,
        );

        assert!(frag.types.contains_key("src/models.User"));
        let td = &frag.types["src/models.User"];
        assert_eq!(td.kind, TypeKind::Interface);
        assert_eq!(td.visibility, Visibility::Public);
    }

    #[test]
    fn type_alias_detected() {
        let frag = parse_ts(
            r#"
export type UserId = string;
"#,
        );

        assert!(frag.types.contains_key("src/models.UserId"));
        let td = &frag.types["src/models.UserId"];
        assert_eq!(td.kind, TypeKind::TypeAlias);
        assert_eq!(td.visibility, Visibility::Public);
    }

    #[test]
    fn enum_detected() {
        let frag = parse_ts(
            r#"
export enum Status {
    Active = "active",
    Inactive = "inactive",
}
"#,
        );

        assert!(frag.types.contains_key("src/models.Status"));
        let td = &frag.types["src/models.Status"];
        assert_eq!(td.kind, TypeKind::Enum);
        assert_eq!(td.variants, vec!["Active", "Inactive"]);
    }

    #[test]
    fn class_detected() {
        let frag = parse_ts(
            r#"
export class UserService {
    async findById(id: string): Promise<User> {
        return {} as User;
    }
}
"#,
        );

        assert!(frag.types.contains_key("src/models.UserService"));
        let td = &frag.types["src/models.UserService"];
        assert_eq!(td.kind, TypeKind::Class);
        assert!(td.methods.contains(&"findById".to_string()));

        let method = &frag.functions["src/models.UserService.findById"];
        assert!(method.is_async);
    }

    #[test]
    fn exported_function() {
        let frag = parse_ts(
            r#"
export function createUser(name: string): User {
    return { name };
}
"#,
        );

        assert!(frag.functions.contains_key("src/models.createUser"));
        let func = &frag.functions["src/models.createUser"];
        assert_eq!(func.visibility, Visibility::Public);
        assert_eq!(func.name, "createUser");
    }

    #[test]
    fn async_function_detected() {
        let frag = parse_ts(
            r#"
export async function fetchData(url: string): Promise<void> {
}
"#,
        );

        let func = &frag.functions["src/models.fetchData"];
        assert!(func.is_async);
    }

    #[test]
    fn arrow_function_export() {
        let frag = parse_ts(
            r#"
export const greet = (name: string): string => {
    return `Hello, ${name}`;
};
"#,
        );

        assert!(frag.functions.contains_key("src/models.greet"));
        let func = &frag.functions["src/models.greet"];
        assert_eq!(func.visibility, Visibility::Public);
    }

    #[test]
    fn non_exported_is_private() {
        let frag = parse_ts(
            r#"
interface InternalConfig {
    debug: boolean;
}

function helper(): void {
}
"#,
        );

        let td = &frag.types["src/models.InternalConfig"];
        assert_eq!(td.visibility, Visibility::Private);

        let func = &frag.functions["src/models.helper"];
        assert_eq!(func.visibility, Visibility::Private);
    }

    #[test]
    fn interface_extends() {
        let frag = parse_ts(
            r#"
export interface Admin extends User {
    role: string;
}
"#,
        );

        let td = &frag.types["src/models.Admin"];
        assert!(
            td.implements.iter().any(|i| i.contains("User")),
            "Expected User in implements: {:?}",
            td.implements
        );
    }

    #[test]
    fn tsx_parsed_correctly() {
        let frag = parse_tsx(
            r#"
export function App(): JSX.Element {
    return <div>Hello</div>;
}
"#,
        );

        assert!(frag.functions.contains_key("src/App.App"));
    }

    #[test]
    fn module_created_from_file() {
        let frag = parse_ts("");

        assert!(frag.modules.contains_key("src/models"));
        let module = &frag.modules["src/models"];
        assert_eq!(module.file, "src/models.ts");
    }

    #[test]
    fn detect_requires_tsconfig() {
        let adapter = TypeScriptAdapter;

        // Node project without tsconfig → false
        let node_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp/nonexistent"),
            kind: ProjectKind::Node {
                manager: uu_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!adapter.detect(&node_ctx));

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
}
