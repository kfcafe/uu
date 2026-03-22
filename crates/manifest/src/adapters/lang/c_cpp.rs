//! C/C++ language adapter — extracts structs, classes, enums, unions, typedefs, and functions.
//!
//! Uses tree-sitter-c for `.c` and `.h` files, tree-sitter-cpp for `.cpp`, `.cc`,
//! `.cxx`, `.hpp`, `.hxx` files. Focuses on header files for the public API surface.

use anyhow::Result;
use project_detect::ProjectKind;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

/// File extensions parsed with tree-sitter-c.
const C_EXTENSIONS: &[&str] = &["c", "h"];

/// File extensions parsed with tree-sitter-cpp.
const CPP_EXTENSIONS: &[&str] = &["cpp", "cc", "cxx", "hpp", "hxx"];

/// Header file extensions — declarations here are considered public API.
const HEADER_EXTENSIONS: &[&str] = &["h", "hpp", "hxx"];

pub struct CCppAdapter;

impl Adapter for CCppAdapter {
    fn name(&self) -> &str {
        "c/c++"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(
            ctx.kind,
            ProjectKind::CMake | ProjectKind::Meson | ProjectKind::Make
        )
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        let mut c_parser = Parser::new();
        c_parser.set_language(&tree_sitter_c::LANGUAGE.into())?;

        let mut cpp_parser = Parser::new();
        cpp_parser.set_language(&tree_sitter_cpp::LANGUAGE.into())?;

        for file in &ctx.files {
            let ext = match file.extension().and_then(|e| e.to_str()) {
                Some(e) => e,
                None => continue,
            };

            let is_c = C_EXTENSIONS.contains(&ext);
            let is_cpp = CPP_EXTENSIONS.contains(&ext);

            if !is_c && !is_cpp {
                continue;
            }

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let parser = if is_cpp {
                &mut cpp_parser
            } else {
                &mut c_parser
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

            let is_header = HEADER_EXTENSIONS.contains(&ext);
            extract_c_cpp(
                &tree.root_node(),
                &source,
                &rel,
                is_header,
                is_cpp,
                &mut fragment,
            );
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

fn extract_c_cpp(
    root: &Node,
    source: &str,
    file: &str,
    is_header: bool,
    is_cpp: bool,
    fragment: &mut ManifestFragment,
) {
    walk_declarations(root, source, file, is_header, is_cpp, fragment, None);

    // Create a module entry for the file
    let module_name = file.replace('/', "::");
    let module = Module {
        path: module_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment.modules.insert(module_name, module);
}

fn walk_declarations(
    node: &Node,
    source: &str,
    file: &str,
    is_header: bool,
    is_cpp: bool,
    fragment: &mut ManifestFragment,
    enclosing_type: Option<&str>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "struct_specifier" => {
                extract_struct_or_union(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Struct,
                    is_header,
                    is_cpp,
                    enclosing_type,
                );
            }
            "union_specifier" => {
                extract_struct_or_union(
                    &child,
                    source,
                    file,
                    fragment,
                    TypeKind::Union,
                    is_header,
                    is_cpp,
                    enclosing_type,
                );
            }
            "enum_specifier" => {
                extract_enum(&child, source, file, fragment, is_header, enclosing_type);
            }
            "class_specifier" if is_cpp => {
                extract_cpp_class(&child, source, file, fragment, is_header, enclosing_type);
            }
            "type_definition" => {
                extract_typedef(&child, source, file, fragment, is_header, enclosing_type);
            }
            "function_definition" => {
                extract_function_def(&child, source, file, fragment, is_header, enclosing_type);
            }
            "declaration" => {
                // Could be a function declaration (prototype) in a header
                if is_header {
                    extract_function_decl(&child, source, file, fragment, enclosing_type);
                }
                // Also walk into it for nested types
                walk_declarations(
                    &child,
                    source,
                    file,
                    is_header,
                    is_cpp,
                    fragment,
                    enclosing_type,
                );
            }
            "namespace_definition" if is_cpp => {
                handle_cpp_namespace(&child, source, file, is_header, fragment, enclosing_type);
            }
            _ => {
                walk_declarations(
                    &child,
                    source,
                    file,
                    is_header,
                    is_cpp,
                    fragment,
                    enclosing_type,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_struct_or_union(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    kind: TypeKind,
    is_header: bool,
    is_cpp: bool,
    enclosing: Option<&str>,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return, // anonymous struct/union
    };

    // Only register if it has a body (definition, not just forward declaration)
    let has_body = node.child_by_field_name("body").is_some();
    if !has_body {
        return;
    }

    let full_name = qualify(enclosing, &name);
    let visibility = if is_header {
        Visibility::Public
    } else {
        Visibility::Internal
    };

    // For C++ structs, extract methods from the body
    let mut methods = Vec::new();
    if is_cpp {
        if let Some(body) = node.child_by_field_name("body") {
            extract_cpp_methods(&body, source, file, fragment, &full_name, &mut methods);
        }
    }

    let type_def = TypeDef {
        name,
        source: file.to_string(),
        kind,
        visibility,
        methods,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
}

fn extract_enum(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    is_header: bool,
    enclosing: Option<&str>,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return, // anonymous enum
    };

    let has_body = node.child_by_field_name("body").is_some();
    if !has_body {
        return;
    }

    let full_name = qualify(enclosing, &name);
    let visibility = if is_header {
        Visibility::Public
    } else {
        Visibility::Internal
    };
    let variants = extract_enum_variants(node, source);

    let type_def = TypeDef {
        name,
        source: file.to_string(),
        kind: TypeKind::Enum,
        variants,
        visibility,
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

fn extract_cpp_class(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    is_header: bool,
    enclosing: Option<&str>,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let has_body = node.child_by_field_name("body").is_some();
    if !has_body {
        return;
    }

    let full_name = qualify(enclosing, &name);
    let visibility = if is_header {
        Visibility::Public
    } else {
        Visibility::Internal
    };

    // Extract base classes from base_class_clause.
    // Grammar may use base_class_specifier wrappers or have type_identifier directly.
    let mut implements = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "base_class_clause" {
            extract_base_types(&child, source, &mut implements);
        }
    }

    // Extract methods
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        extract_cpp_methods(&body, source, file, fragment, &full_name, &mut methods);
    }

    let type_def = TypeDef {
        name,
        source: file.to_string(),
        kind: TypeKind::Class,
        visibility,
        implements,
        methods,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
}

fn extract_cpp_methods(
    body: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    type_name: &str,
    methods: &mut Vec<String>,
) {
    let mut cursor = body.walk();
    let mut current_access = "private"; // C++ class default is private

    for child in body.named_children(&mut cursor) {
        if child.kind() == "access_specifier" {
            let text = node_text(&child, source)
                .trim_end_matches(':')
                .trim()
                .to_string();
            current_access = match text.as_str() {
                "public" => "public",
                "private" => "private",
                "protected" => "protected",
                _ => current_access,
            };
        }

        if child.kind() == "function_definition" || child.kind() == "declaration" {
            let method_name = match child.child_by_field_name("declarator") {
                Some(decl) => extract_declarator_name(&decl, source),
                None => continue,
            };

            if method_name.is_empty() {
                continue;
            }

            let visibility = match current_access {
                "public" => Visibility::Public,
                "private" => Visibility::Private,
                _ => Visibility::Internal,
            };

            let sig = node_text(&child, source);
            let signature = sig
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();

            let qualified = format!("{}.{}", type_name, method_name);

            let function = Function {
                name: method_name.clone(),
                source: file.to_string(),
                signature,
                visibility,
                ..Default::default()
            };
            fragment.functions.insert(qualified, function);
            methods.push(method_name);
        }
    }
}

fn extract_typedef(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    is_header: bool,
    enclosing: Option<&str>,
) {
    let name = match node.child_by_field_name("declarator") {
        Some(n) => extract_declarator_name(&n, source),
        None => return,
    };

    if name.is_empty() {
        return;
    }

    let full_name = qualify(enclosing, &name);
    let visibility = if is_header {
        Visibility::Public
    } else {
        Visibility::Internal
    };

    let type_def = TypeDef {
        name,
        source: file.to_string(),
        kind: TypeKind::TypeAlias,
        visibility,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
}

fn extract_function_def(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    is_header: bool,
    enclosing: Option<&str>,
) {
    let name = match node.child_by_field_name("declarator") {
        Some(decl) => extract_declarator_name(&decl, source),
        None => return,
    };

    if name.is_empty() {
        return;
    }

    // `static` functions are private
    let is_static = has_storage_class(node, source, "static");
    let visibility = if is_static {
        Visibility::Private
    } else if is_header {
        Visibility::Public
    } else {
        Visibility::Internal
    };

    let sig = node_text(node, source);
    let signature = sig
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('{')
        .trim()
        .to_string();

    let qualified = qualify(enclosing, &name);

    let function = Function {
        name,
        source: file.to_string(),
        signature,
        visibility,
        ..Default::default()
    };
    fragment.functions.insert(qualified, function);
}

fn extract_function_decl(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    enclosing: Option<&str>,
) {
    // A declaration in a header that looks like a function prototype
    let decl = match node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return,
    };

    // Must be a function declarator (has parameters)
    if decl.kind() != "function_declarator" && !has_function_declarator(&decl) {
        return;
    }

    let name = extract_declarator_name(&decl, source);
    if name.is_empty() {
        return;
    }

    // Skip if already registered (from a function_definition)
    let qualified = qualify(enclosing, &name);
    if fragment.functions.contains_key(&qualified) {
        return;
    }

    let is_static = has_storage_class(node, source, "static");
    let visibility = if is_static {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let sig = node_text(node, source)
        .trim_end_matches(';')
        .trim()
        .to_string();

    let function = Function {
        name,
        source: file.to_string(),
        signature: sig,
        visibility,
        ..Default::default()
    };
    fragment.functions.insert(qualified, function);
}

fn handle_cpp_namespace(
    node: &Node,
    source: &str,
    file: &str,
    is_header: bool,
    fragment: &mut ManifestFragment,
    _enclosing: Option<&str>,
) {
    let ns_name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_default();

    if ns_name.is_empty() {
        return;
    }

    let module = Module {
        path: ns_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment
        .modules
        .insert(format!("{}:{}", ns_name, file), module);

    if let Some(body) = node.child_by_field_name("body") {
        walk_declarations(
            &body,
            source,
            file,
            is_header,
            true,
            fragment,
            Some(&ns_name),
        );
    }
}

// -- Helpers ------------------------------------------------------------------

fn qualify(enclosing: Option<&str>, name: &str) -> String {
    match enclosing {
        Some(parent) => format!("{}::{}", parent, name),
        None => name.to_string(),
    }
}

/// Recursively collect type identifiers from a base_class_clause.
/// Handles both `base_class_specifier` wrappers and direct type_identifier children.
fn extract_base_types(node: &Node, source: &str, result: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "qualified_identifier" | "template_type" => {
                let text = node_text(&child, source);
                if !text.is_empty() {
                    result.push(text);
                }
            }
            "base_class_specifier" => {
                // Recurse into the specifier
                extract_base_types(&child, source, result);
            }
            _ => {}
        }
    }
}

fn extract_declarator_name(node: &Node, source: &str) -> String {
    match node.kind() {
        "identifier" | "type_identifier" | "field_identifier" | "primitive_type" => {
            node_text(node, source)
        }
        "function_declarator" => {
            // The name is in the `declarator` field of the function_declarator
            match node.child_by_field_name("declarator") {
                Some(inner) => extract_declarator_name(&inner, source),
                None => String::new(),
            }
        }
        "pointer_declarator" => match node.child_by_field_name("declarator") {
            Some(inner) => extract_declarator_name(&inner, source),
            None => String::new(),
        },
        "parenthesized_declarator" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                let name = extract_declarator_name(&child, source);
                if !name.is_empty() {
                    return name;
                }
            }
            String::new()
        }
        _ => {
            // Try the declarator field recursively
            match node.child_by_field_name("declarator") {
                Some(inner) => extract_declarator_name(&inner, source),
                None => String::new(),
            }
        }
    }
}

fn has_function_declarator(node: &Node) -> bool {
    if node.kind() == "function_declarator" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if has_function_declarator(&child) {
            return true;
        }
    }
    false
}

fn has_storage_class(node: &Node, source: &str, class: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" && node_text(&child, source) == class {
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

    fn parse_c(source: &str, file: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        let is_header = file.ends_with(".h");
        extract_c_cpp(
            &tree.root_node(),
            source,
            file,
            is_header,
            false,
            &mut fragment,
        );
        fragment
    }

    fn parse_cpp(source: &str, file: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        let is_header = file.ends_with(".h") || file.ends_with(".hpp") || file.ends_with(".hxx");
        extract_c_cpp(
            &tree.root_node(),
            source,
            file,
            is_header,
            true,
            &mut fragment,
        );
        fragment
    }

    #[test]
    fn c_struct_detected() {
        let frag = parse_c(
            r#"
struct Point {
    int x;
    int y;
};
"#,
            "include/point.h",
        );

        assert!(
            frag.types.contains_key("Point"),
            "Expected Point, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Point"];
        assert_eq!(td.kind, TypeKind::Struct);
        assert_eq!(td.visibility, Visibility::Public); // header → public
    }

    #[test]
    fn c_enum_detected() {
        let frag = parse_c(
            r#"
enum Color {
    RED,
    GREEN,
    BLUE
};
"#,
            "include/color.h",
        );

        assert!(
            frag.types.contains_key("Color"),
            "Expected Color, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Color"];
        assert_eq!(td.kind, TypeKind::Enum);
        assert_eq!(td.variants, vec!["RED", "GREEN", "BLUE"]);
    }

    #[test]
    fn c_union_detected() {
        let frag = parse_c(
            r#"
union Value {
    int i;
    float f;
    char* s;
};
"#,
            "include/value.h",
        );

        assert!(
            frag.types.contains_key("Value"),
            "Expected Value, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Value"];
        assert_eq!(td.kind, TypeKind::Union);
    }

    #[test]
    fn c_typedef_detected() {
        let frag = parse_c(
            r#"
typedef unsigned long size_t;
"#,
            "include/types.h",
        );

        assert!(
            frag.types.contains_key("size_t"),
            "Expected size_t, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["size_t"];
        assert_eq!(td.kind, TypeKind::TypeAlias);
    }

    #[test]
    fn header_function_is_public() {
        let frag = parse_c(
            r#"
int calculate(int a, int b);
"#,
            "include/calc.h",
        );

        assert!(
            frag.functions.contains_key("calculate"),
            "Expected calculate, got keys: {:?}",
            frag.functions.keys().collect::<Vec<_>>()
        );
        assert_eq!(frag.functions["calculate"].visibility, Visibility::Public);
    }

    #[test]
    fn static_function_is_private() {
        let frag = parse_c(
            r#"
static int helper(int x) {
    return x + 1;
}
"#,
            "src/util.c",
        );

        assert!(
            frag.functions.contains_key("helper"),
            "Expected helper, got keys: {:?}",
            frag.functions.keys().collect::<Vec<_>>()
        );
        assert_eq!(frag.functions["helper"].visibility, Visibility::Private);
    }

    #[test]
    fn cpp_class_detected() {
        let frag = parse_cpp(
            r#"
class Widget {
public:
    void draw();
private:
    int x_;
};
"#,
            "include/widget.hpp",
        );

        assert!(
            frag.types.contains_key("Widget"),
            "Expected Widget, got keys: {:?}",
            frag.types.keys().collect::<Vec<_>>()
        );
        let td = &frag.types["Widget"];
        assert_eq!(td.kind, TypeKind::Class);
        assert_eq!(td.visibility, Visibility::Public); // header
    }

    #[test]
    fn cpp_inheritance() {
        let frag = parse_cpp(
            r#"
class Button : public Widget {
public:
    void click();
};
"#,
            "include/button.hpp",
        );

        let td = &frag.types["Button"];
        assert!(
            td.implements.iter().any(|i| i.contains("Widget")),
            "Expected Widget in {:?}",
            td.implements
        );
    }

    #[test]
    fn dual_parser_c_vs_cpp() {
        // C file should not recognize `class`
        let c_frag = parse_c("struct Data { int x; };", "src/data.h");
        assert!(c_frag.types.contains_key("Data"));
        assert_eq!(c_frag.types["Data"].kind, TypeKind::Struct);

        // C++ file should recognize `class`
        let cpp_frag = parse_cpp("class Data { int x; };", "src/data.hpp");
        assert!(cpp_frag.types.contains_key("Data"));
        assert_eq!(cpp_frag.types["Data"].kind, TypeKind::Class);
    }

    #[test]
    fn source_file_not_header_is_internal() {
        let frag = parse_c(
            r#"
struct InternalData {
    int value;
};
"#,
            "src/internal.c",
        );

        assert!(frag.types.contains_key("InternalData"));
        assert_eq!(frag.types["InternalData"].visibility, Visibility::Internal);
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = CCppAdapter;

        for kind in [ProjectKind::CMake, ProjectKind::Meson, ProjectKind::Make] {
            let ctx = ProjectContext {
                root: std::path::PathBuf::from("/tmp"),
                kind,
                files: vec![],
                package_json: None,
                cargo_toml: None,
                go_mod: None,
            };
            assert!(adapter.detect(&ctx), "Should detect {:?}", ctx.kind);
        }

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
