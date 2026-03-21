//! Express framework adapter — extracts routes and endpoints from Express apps.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Endpoint, ManifestFragment, Route, RouteType};

pub struct ExpressAdapter;

impl Adapter for ExpressAdapter {
    fn name(&self) -> &str {
        "express"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        has_dependency(ctx, "express")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut js_parser = Parser::new();
        js_parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
        let mut ts_parser = Parser::new();
        ts_parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())?;

        for file in &ctx.files {
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            let parser = match ext {
                "js" | "mjs" | "cjs" | "jsx" => &mut js_parser,
                "ts" | "tsx" => &mut ts_parser,
                _ => continue,
            };

            // Skip node_modules and test files
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

            extract_express_routes(&tree.root_node(), &source, &rel, &mut fragment);
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

fn has_dependency(ctx: &ProjectContext, name: &str) -> bool {
    let Some(pkg) = &ctx.package_json else {
        return false;
    };
    for section in ["dependencies", "devDependencies"] {
        if pkg.get(section).and_then(|s| s.get(name)).is_some() {
            return true;
        }
    }
    false
}

fn should_skip(file: &Path, root: &Path) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if name.as_ref() == "node_modules" || name.as_ref() == "dist" {
                return true;
            }
        }
    }
    false
}

/// Walk AST looking for method calls like `app.get('/path', handler)`.
fn extract_express_routes(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    walk_node(node, source, file, fragment);
}

fn walk_node(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    if node.kind() == "call_expression" {
        if let Some(callee) = node.child_by_field_name("function") {
            if callee.kind() == "member_expression" {
                try_extract_route_call(&callee, node, source, file, fragment);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_node(&child, source, file, fragment);
    }
}

/// HTTP methods we recognize on Express app/router objects.
const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "options", "head"];

fn try_extract_route_call(
    callee: &Node,
    call: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
) {
    let method_node = match callee.child_by_field_name("property") {
        Some(n) => n,
        None => return,
    };
    let method_name = node_text(&method_node, source);

    // Check for use() — middleware mounting
    let is_use = method_name == "use";
    let is_http = HTTP_METHODS.contains(&method_name.as_str());

    if !is_http && !is_use {
        return;
    }

    // Get the arguments
    let args = match call.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };

    // First argument should be a string literal (the path)
    let mut cursor = args.walk();
    let arg_children: Vec<Node> = args.named_children(&mut cursor).collect();

    if arg_children.is_empty() {
        return;
    }

    let first_arg = &arg_children[0];
    let path = extract_string_value(first_arg, source);
    if path.is_empty() {
        return;
    }

    let method_upper = method_name.to_uppercase();

    if is_http {
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

        let route_key = format!("express:{}:{}", method_upper, path);
        fragment.routes.insert(
            route_key.clone(),
            Route {
                path: path.clone(),
                file: file.to_string(),
                route_type: RouteType::ApiRoute,
                methods: vec![method_upper.clone()],
                handler: handler_name.clone(),
            },
        );

        fragment.endpoints.insert(
            route_key,
            Endpoint {
                path: path.clone(),
                file: file.to_string(),
                method: method_upper,
                handler: handler_name.unwrap_or_default(),
                middleware: vec![],
            },
        );
    } else if is_use {
        // Middleware mounting: app.use('/prefix', router)
        let route_key = format!("express:USE:{}", path);
        fragment.routes.insert(
            route_key,
            Route {
                path,
                file: file.to_string(),
                route_type: RouteType::Middleware,
                methods: vec![],
                handler: None,
            },
        );
    }
}

/// Extract the string value from a string node, stripping quotes.
fn extract_string_value(node: &Node, source: &str) -> String {
    let text = node_text(node, source);
    match node.kind() {
        "string" | "template_string" => {
            // Strip quotes: "..." or '...' or `...`
            if text.len() >= 2 {
                text[1..text.len() - 1].to_string()
            } else {
                text
            }
        }
        _ => String::new(),
    }
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
        extract_express_routes(&tree.root_node(), source, "server.js", &mut fragment);
        fragment
    }

    #[test]
    fn detect_express() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Node {
                manager: uu_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: Some(serde_json::json!({
                "dependencies": { "express": "4.18.0" }
            })),
            cargo_toml: None,
            go_mod: None,
        };
        assert!(ExpressAdapter.detect(&ctx));
    }

    #[test]
    fn app_get_route() {
        let frag = parse_js(
            r#"
const app = express();
app.get('/users', getUsers);
app.post('/users', createUser);
"#,
        );

        assert!(
            frag.routes.contains_key("express:GET:/users"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
        assert!(frag.routes.contains_key("express:POST:/users"));

        let route = &frag.routes["express:GET:/users"];
        assert_eq!(route.path, "/users");
        assert_eq!(route.methods, vec!["GET"]);

        let endpoint = &frag.endpoints["express:GET:/users"];
        assert_eq!(endpoint.method, "GET");
        assert_eq!(endpoint.handler, "getUsers");
    }

    #[test]
    fn router_methods() {
        let frag = parse_js(
            r#"
const router = express.Router();
router.get('/items', listItems);
router.delete('/items/:id', deleteItem);
"#,
        );

        assert!(frag.routes.contains_key("express:GET:/items"));
        assert!(frag.routes.contains_key("express:DELETE:/items/:id"));
    }

    #[test]
    fn app_use_middleware() {
        let frag = parse_js(
            r#"
const app = express();
app.use('/api', apiRouter);
"#,
        );

        assert!(frag.routes.contains_key("express:USE:/api"));
        assert_eq!(
            frag.routes["express:USE:/api"].route_type,
            RouteType::Middleware
        );
    }
}
