//! Axum framework adapter — extracts routes and endpoints from Router definitions.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Endpoint, ManifestFragment, Route, RouteType};

pub struct AxumAdapter;

impl Adapter for AxumAdapter {
    fn name(&self) -> &str {
        "axum"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        let Some(cargo) = &ctx.cargo_toml else {
            return false;
        };
        cargo
            .get("dependencies")
            .and_then(|d| d.get("axum"))
            .is_some()
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

        for file in &ctx.files {
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("rs")) {
                continue;
            }
            if should_skip(file, &ctx.root) {
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

            extract_axum_routes(&tree.root_node(), &source, &rel, &mut fragment);
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
            if name.as_ref() == "target" {
                return true;
            }
        }
    }
    false
}

/// HTTP method functions we recognize in Axum route definitions.
const AXUM_METHODS: &[&str] = &[
    "get",
    "post",
    "put",
    "delete",
    "patch",
    "head",
    "options",
    "trace",
    "get_service",
    "post_service",
];

/// Walk the AST looking for `.route("/path", method(handler))` calls.
fn extract_axum_routes(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    walk_for_routes(root, source, file, fragment);
}

fn walk_for_routes(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    if node.kind() == "call_expression" {
        try_extract_route_call(node, source, file, fragment);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_for_routes(&child, source, file, fragment);
    }
}

/// Try to extract a `.route("/path", get(handler))` call.
fn try_extract_route_call(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // The call_expression should have a function child that's a field_expression
    // with `.route` as the method name
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };

    let method_name = if func.kind() == "field_expression" {
        match func.child_by_field_name("field") {
            Some(f) => node_text(&f, source),
            None => return,
        }
    } else {
        return;
    };

    if method_name != "route" {
        return;
    }

    // Get arguments
    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };

    let mut cursor = args.walk();
    let arg_children: Vec<Node> = args.named_children(&mut cursor).collect();

    if arg_children.len() < 2 {
        return;
    }

    // First arg: path string literal
    let path = extract_rust_string(&arg_children[0], source);
    if path.is_empty() {
        return;
    }

    // Second arg: method handler expression like `get(handler)` or `get(handler).post(handler2)`
    let handler_node = &arg_children[1];
    let methods_and_handlers = extract_method_handlers(handler_node, source);

    if methods_and_handlers.is_empty() {
        return;
    }

    let all_methods: Vec<String> = methods_and_handlers
        .iter()
        .map(|(m, _)| m.to_uppercase())
        .collect();

    // Create a route entry with all methods
    let route_key = format!("axum:{}", path);
    fragment.routes.insert(
        route_key,
        Route {
            path: path.clone(),
            file: file.to_string(),
            route_type: RouteType::ApiRoute,
            methods: all_methods,
            handler: methods_and_handlers.first().map(|(_, h)| h.clone()),
        },
    );

    // Create individual endpoint entries per method
    for (method, handler) in &methods_and_handlers {
        let method_upper = method.to_uppercase();
        let endpoint_key = format!("axum:{}:{}", method_upper, path);
        fragment.endpoints.insert(
            endpoint_key,
            Endpoint {
                path: path.clone(),
                file: file.to_string(),
                method: method_upper,
                handler: handler.clone(),
                middleware: vec![],
            },
        );
    }
}

/// Extract method+handler pairs from expressions like `get(handler)` or
/// `get(handler).post(handler2)`.
fn extract_method_handlers(node: &Node, source: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    collect_method_handlers(node, source, &mut results);
    results
}

fn collect_method_handlers(node: &Node, source: &str, results: &mut Vec<(String, String)>) {
    match node.kind() {
        "call_expression" => {
            let func = match node.child_by_field_name("function") {
                Some(f) => f,
                None => return,
            };

            match func.kind() {
                // Simple call: `get(handler)`
                "identifier" => {
                    let method = node_text(&func, source);
                    if is_axum_method(&method) {
                        let handler = extract_handler_arg(node, source);
                        results.push((method, handler));
                    }
                }
                // Chained call: `get(handler).post(handler2)` — the `.post(handler2)` part
                "field_expression" => {
                    let field = match func.child_by_field_name("field") {
                        Some(f) => node_text(&f, source),
                        None => return,
                    };

                    if is_axum_method(&field) {
                        let handler = extract_handler_arg(node, source);
                        results.push((field, handler));
                    }

                    // Recurse into the object (left side of the dot)
                    if let Some(obj) = func.child_by_field_name("value") {
                        collect_method_handlers(&obj, source, results);
                    }
                }
                _ => {}
            }
        }
        // method_call(...) chains via field_expression
        "field_expression" => {
            if let Some(obj) = node.child_by_field_name("value") {
                collect_method_handlers(&obj, source, results);
            }
        }
        _ => {}
    }
}

fn is_axum_method(name: &str) -> bool {
    AXUM_METHODS.contains(&name)
}

/// Extract the handler name from a method call like `get(handler)`.
fn extract_handler_arg(call_node: &Node, source: &str) -> String {
    let args = match call_node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return String::new(),
    };

    let mut cursor = args.walk();
    let children: Vec<Node> = args.named_children(&mut cursor).collect();

    if children.is_empty() {
        return String::new();
    }

    let text = node_text(&children[0], source);
    if text.len() > 100 {
        String::new()
    } else {
        text
    }
}

/// Extract a string literal value from a Rust string node.
fn extract_rust_string(node: &Node, source: &str) -> String {
    let text = node_text(node, source);
    if node.kind() == "string_literal" || text.starts_with('"') {
        // Strip the quotes
        text.trim_matches('"').to_string()
    } else {
        String::new()
    }
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rust(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_axum_routes(&tree.root_node(), source, "src/main.rs", &mut fragment);
        fragment
    }

    // -- Detection tests --

    #[test]
    fn detect_axum_from_cargo_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Cargo,
            files: vec![],
            package_json: None,
            cargo_toml: Some(
                toml::from_str(
                    r#"
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
"#,
                )
                .unwrap(),
            ),
            go_mod: None,
        };
        assert!(AxumAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_axum() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Cargo,
            files: vec![],
            package_json: None,
            cargo_toml: Some(
                toml::from_str(
                    r#"
[dependencies]
actix-web = "4"
"#,
                )
                .unwrap(),
            ),
            go_mod: None,
        };
        assert!(!AxumAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_cargo_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Cargo,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!AxumAdapter.detect(&ctx));
    }

    // -- Route extraction tests --

    #[test]
    fn simple_route() {
        let frag = parse_rust(
            r#"
fn app() -> Router {
    Router::new()
        .route("/users", get(list_users))
        .route("/health", get(health_check))
}
"#,
        );

        assert!(
            frag.routes.contains_key("axum:/users"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
        assert!(frag.routes.contains_key("axum:/health"));

        let route = &frag.routes["axum:/users"];
        assert_eq!(route.path, "/users");
        assert_eq!(route.methods, vec!["GET"]);
        assert_eq!(route.handler.as_deref(), Some("list_users"));

        let endpoint = &frag.endpoints["axum:GET:/users"];
        assert_eq!(endpoint.method, "GET");
        assert_eq!(endpoint.handler, "list_users");
    }

    #[test]
    fn multiple_methods_on_route() {
        let frag = parse_rust(
            r#"
fn app() -> Router {
    Router::new()
        .route("/items", get(list_items).post(create_item))
}
"#,
        );

        assert!(frag.routes.contains_key("axum:/items"));
        let route = &frag.routes["axum:/items"];
        assert!(route.methods.contains(&"GET".to_string()));
        assert!(route.methods.contains(&"POST".to_string()));

        assert!(frag.endpoints.contains_key("axum:GET:/items"));
        assert!(frag.endpoints.contains_key("axum:POST:/items"));

        assert_eq!(frag.endpoints["axum:GET:/items"].handler, "list_items");
        assert_eq!(frag.endpoints["axum:POST:/items"].handler, "create_item");
    }

    #[test]
    fn various_http_methods() {
        let frag = parse_rust(
            r#"
fn app() -> Router {
    Router::new()
        .route("/a", post(create))
        .route("/b", put(update))
        .route("/c", delete(remove))
        .route("/d", patch(modify))
}
"#,
        );

        assert!(frag.endpoints.contains_key("axum:POST:/a"));
        assert!(frag.endpoints.contains_key("axum:PUT:/b"));
        assert!(frag.endpoints.contains_key("axum:DELETE:/c"));
        assert!(frag.endpoints.contains_key("axum:PATCH:/d"));
    }

    // -- Full extract test --

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        std::fs::write(
            src.join("main.rs"),
            r#"
use axum::{Router, routing::get};

fn app() -> Router {
    Router::new()
        .route("/api/items", get(list_items))
}

async fn list_items() -> &'static str {
    "[]"
}
"#,
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Cargo,
            files: vec![src.join("main.rs")],
            package_json: None,
            cargo_toml: Some(
                toml::from_str(
                    r#"
[dependencies]
axum = "0.7"
"#,
                )
                .unwrap(),
            ),
            go_mod: None,
        };

        assert!(AxumAdapter.detect(&ctx));
        let frag = AxumAdapter.extract(&ctx).unwrap();
        assert!(
            frag.routes.contains_key("axum:/api/items"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
    }
}
