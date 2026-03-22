//! Gin framework adapter — extracts routes from gin router registrations.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Endpoint, ManifestFragment, Route, RouteType};

pub struct GinAdapter;

impl Adapter for GinAdapter {
    fn name(&self) -> &str {
        "gin"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        let Some(go_mod) = &ctx.go_mod else {
            return false;
        };
        go_mod.contains("github.com/gin-gonic/gin")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into())?;

        for file in &ctx.files {
            if file
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| e != "go")
            {
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

            extract_gin_routes(&tree.root_node(), &source, &rel, &mut fragment);
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
            if matches!(name.as_ref(), "vendor" | "testdata") {
                return true;
            }
        }
    }
    false
}

/// HTTP methods we recognize on gin.Engine / gin.RouterGroup.
const GIN_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];

/// Walk AST looking for gin route registrations like `r.GET("/path", handler)`.
fn extract_gin_routes(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    walk_node(root, source, file, fragment);
}

fn walk_node(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    if node.kind() == "call_expression" {
        try_extract_gin_route(node, source, file, fragment);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_node(&child, source, file, fragment);
    }
}

/// Try to extract a route from a call like `r.GET("/users", listUsers)`.
fn try_extract_gin_route(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };

    // Must be a selector expression: `r.GET`
    if func.kind() != "selector_expression" {
        return;
    }

    let field = match func.child_by_field_name("field") {
        Some(f) => f,
        None => return,
    };

    let method_name = node_text(&field, source);
    if !GIN_METHODS.contains(&method_name.as_str()) {
        return;
    }

    // Get arguments
    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };

    let mut cursor = args.walk();
    let arg_children: Vec<Node> = args.named_children(&mut cursor).collect();

    if arg_children.is_empty() {
        return;
    }

    // First argument should be a string literal (the path)
    let path = extract_go_string(&arg_children[0], source);
    if path.is_empty() {
        return;
    }

    // Handler is the last argument
    let handler_name = if arg_children.len() > 1 {
        let handler_node = arg_children.last().unwrap();
        let text = node_text(handler_node, source);
        if text.len() < 100 {
            Some(text)
        } else {
            None
        }
    } else {
        None
    };

    let route_key = format!("gin:{}:{}", method_name, path);
    fragment.routes.insert(
        route_key.clone(),
        Route {
            path: path.clone(),
            file: file.to_string(),
            route_type: RouteType::ApiRoute,
            methods: vec![method_name.clone()],
            handler: handler_name.clone(),
        },
    );

    fragment.endpoints.insert(
        route_key,
        Endpoint {
            path,
            file: file.to_string(),
            method: method_name,
            handler: handler_name.unwrap_or_default(),
            middleware: vec![],
        },
    );
}

/// Extract a Go string literal value, stripping quotes.
fn extract_go_string(node: &Node, source: &str) -> String {
    let text = node_text(node, source);
    if node.kind() == "interpreted_string_literal" || node.kind() == "raw_string_literal" {
        // Strip quotes: "..." or `...`
        if text.len() >= 2 {
            text[1..text.len() - 1].to_string()
        } else {
            text
        }
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

    fn parse_go(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_gin_routes(&tree.root_node(), source, "main.go", &mut fragment);
        fragment
    }

    #[test]
    fn detect_gin() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Go,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: Some(
                "module example.com/myapp\n\nrequire github.com/gin-gonic/gin v1.9.1\n".to_string(),
            ),
        };
        assert!(GinAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_gin() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Go,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: Some(
                "module example.com/myapp\n\nrequire github.com/labstack/echo/v4 v4.11.0\n"
                    .to_string(),
            ),
        };
        assert!(!GinAdapter.detect(&ctx));
    }

    #[test]
    fn extract_simple_routes() {
        let frag = parse_go(
            r#"
package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    r.GET("/users", listUsers)
    r.POST("/users", createUser)
    r.DELETE("/users/:id", deleteUser)
}
"#,
        );

        assert!(
            frag.routes.contains_key("gin:GET:/users"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
        assert!(frag.routes.contains_key("gin:POST:/users"));
        assert!(frag.routes.contains_key("gin:DELETE:/users/:id"));

        let route = &frag.routes["gin:GET:/users"];
        assert_eq!(route.path, "/users");
        assert_eq!(route.methods, vec!["GET"]);
        assert_eq!(route.handler.as_deref(), Some("listUsers"));

        let ep = &frag.endpoints["gin:GET:/users"];
        assert_eq!(ep.handler, "listUsers");
    }

    #[test]
    fn extract_group_routes() {
        let frag = parse_go(
            r#"
package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    api := r.Group("/api")
    api.GET("/items", listItems)
    api.PUT("/items/:id", updateItem)
}
"#,
        );

        // Group routes register with the path in the GET call, not the full path
        // (the group prefix is applied at runtime, not in the AST)
        assert!(frag.routes.contains_key("gin:GET:/items"));
        assert!(frag.routes.contains_key("gin:PUT:/items/:id"));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();

        std::fs::write(
            dir.path().join("main.go"),
            r#"package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    r.GET("/health", healthCheck)
}
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Go,
            files: vec![dir.path().join("main.go")],
            package_json: None,
            cargo_toml: None,
            go_mod: Some(
                "module example.com/app\nrequire github.com/gin-gonic/gin v1.9.1\n".to_string(),
            ),
        };

        assert!(GinAdapter.detect(&ctx));
        let frag = GinAdapter.extract(&ctx).unwrap();
        assert!(frag.routes.contains_key("gin:GET:/health"));
    }
}
