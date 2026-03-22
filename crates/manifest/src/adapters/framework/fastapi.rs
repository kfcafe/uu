//! FastAPI framework adapter — extracts routes, endpoints, and Pydantic models.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{
    DataModel, Endpoint, Field, ManifestFragment, Route, RouteType, TypeDef, TypeKind,
};

pub struct FastApiAdapter;

impl Adapter for FastApiAdapter {
    fn name(&self) -> &str {
        "fastapi"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        has_python_dependency(ctx, "fastapi")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into())?;

        for file in &ctx.files {
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("py")) {
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

            extract_fastapi(&tree.root_node(), &source, &rel, &mut fragment);
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

/// Check if a Python dependency is listed in pyproject.toml or requirements.txt.
fn has_python_dependency(ctx: &ProjectContext, name: &str) -> bool {
    let pyproject_path = ctx.root.join("pyproject.toml");
    if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
        if let Ok(toml_val) = content.parse::<toml::Value>() {
            if let Some(deps) = toml_val
                .get("project")
                .and_then(|p| p.get("dependencies"))
                .and_then(|d| d.as_array())
            {
                for dep in deps {
                    if let Some(s) = dep.as_str() {
                        let pkg = s
                            .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                            .next()
                            .unwrap_or("");
                        if pkg.eq_ignore_ascii_case(name) {
                            return true;
                        }
                    }
                }
            }
            if let Some(deps) = toml_val
                .get("tool")
                .and_then(|t| t.get("poetry"))
                .and_then(|p| p.get("dependencies"))
                .and_then(|d| d.as_table())
            {
                for key in deps.keys() {
                    if key.eq_ignore_ascii_case(name) {
                        return true;
                    }
                }
            }
        }
    }

    let req_path = ctx.root.join("requirements.txt");
    if let Ok(content) = std::fs::read_to_string(&req_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
                continue;
            }
            let pkg = trimmed
                .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                .next()
                .unwrap_or("");
            if pkg.eq_ignore_ascii_case(name) {
                return true;
            }
        }
    }

    false
}

fn should_skip(file: &Path, root: &Path) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if matches!(
                name.as_ref(),
                "__pycache__" | ".venv" | "venv" | "site-packages"
            ) {
                return true;
            }
        }
    }
    false
}

/// HTTP methods we recognize on FastAPI app/router decorators.
const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "options", "head"];

/// Extract FastAPI routes and Pydantic models from a Python file.
fn extract_fastapi(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "decorated_definition" => {
                extract_decorated_route(&child, source, file, fragment);
            }
            "class_definition" => {
                try_extract_pydantic_model(&child, source, file, fragment);
            }
            _ => {}
        }
    }
}

/// Extract a decorated function as a FastAPI route endpoint.
fn extract_decorated_route(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    // Collect decorators
    let mut route_decorator: Option<(String, String)> = None; // (method, path)

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "decorator" {
            if let Some((method, path)) = parse_route_decorator(&child, source) {
                route_decorator = Some((method, path));
            }
        }
    }

    let (method, path) = match route_decorator {
        Some(r) => r,
        None => return,
    };

    // Get the function name
    let definition = match node.child_by_field_name("definition") {
        Some(d) => d,
        None => return,
    };

    if definition.kind() != "function_definition" {
        return;
    }

    let handler_name = match definition.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let method_upper = method.to_uppercase();
    let route_key = format!("fastapi:{}:{}", method_upper, path);

    fragment.routes.insert(
        route_key.clone(),
        Route {
            path: path.clone(),
            file: file.to_string(),
            route_type: RouteType::ApiRoute,
            methods: vec![method_upper.clone()],
            handler: Some(handler_name.clone()),
        },
    );

    fragment.endpoints.insert(
        route_key,
        Endpoint {
            path,
            file: file.to_string(),
            method: method_upper,
            handler: handler_name,
            middleware: vec![],
        },
    );
}

/// Parse a decorator like `@app.get("/path")` or `@router.post("/path")`.
/// Returns (method, path) if it matches.
fn parse_route_decorator(node: &Node, source: &str) -> Option<(String, String)> {
    // Decorator text looks like `@app.get("/path")` or `@router.get("/path", ...)`
    let text = node_text(node, source);
    let text = text.strip_prefix('@')?.trim();

    // Find the method name: look for `.get(`, `.post(`, etc.
    let dot_idx = text.find('.')?;
    let after_dot = &text[dot_idx + 1..];
    let paren_idx = after_dot.find('(')?;
    let method = &after_dot[..paren_idx];

    if !HTTP_METHODS.contains(&method) {
        return None;
    }

    // Extract the path argument
    let args_start = dot_idx + 1 + paren_idx + 1;
    let args_str = &text[args_start..];

    // Find the first string literal
    let path = extract_first_string_arg(args_str)?;

    Some((method.to_string(), path))
}

/// Extract the first quoted string from an arguments string.
fn extract_first_string_arg(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else if let Some(rest) = s.strip_prefix('\'') {
        let end = rest.find('\'')?;
        Some(rest[..end].to_string())
    } else {
        None
    }
}

/// Try to extract a Pydantic BaseModel class.
fn try_extract_pydantic_model(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let superclasses = match node.child_by_field_name("superclasses") {
        Some(s) => s,
        None => return,
    };

    let mut is_pydantic = false;
    let mut sc_cursor = superclasses.walk();
    for child in superclasses.named_children(&mut sc_cursor) {
        let text = node_text(&child, source);
        if text == "BaseModel" || text == "pydantic.BaseModel" {
            is_pydantic = true;
            break;
        }
    }

    if !is_pydantic {
        return;
    }

    let body = match node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    let mut fields = Vec::new();

    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        // Pydantic fields: `name: str`, `age: int = 0`, `email: Optional[str] = None`
        if child.kind() == "expression_statement" {
            let mut inner_cursor = child.walk();
            for inner in child.named_children(&mut inner_cursor) {
                if inner.kind() == "assignment" {
                    // `name: str = "default"` parses as typed assignment
                    extract_typed_field(&inner, source, &mut fields);
                }
            }
        }
        // Type-annotated without default: `name: str`
        if child.kind() == "expression_statement" {
            let mut inner_cursor = child.walk();
            for inner in child.named_children(&mut inner_cursor) {
                if inner.kind() == "type" {
                    // This would be `name: str` (type annotation)
                    extract_annotation_field(&inner, source, &mut fields);
                }
            }
        }
    }

    // Also try to parse annotation assignments directly in the body
    extract_pydantic_fields_from_body(&body, source, &mut fields);

    // Deduplicate fields by name
    let mut seen = std::collections::HashSet::new();
    fields.retain(|f| seen.insert(f.name.clone()));

    // Store as both a DataModel (pydantic) and a TypeDef
    let model = DataModel {
        name: name.clone(),
        source: file.to_string(),
        orm: "pydantic".to_string(),
        fields: fields.clone(),
        relations: vec![],
        indexes: vec![],
    };
    fragment.models.insert(name.clone(), model);

    let type_def = TypeDef {
        name: name.clone(),
        source: file.to_string(),
        kind: TypeKind::Class,
        fields,
        implements: vec!["BaseModel".to_string()],
        ..Default::default()
    };
    fragment.types.insert(name, type_def);
}

/// Extract fields from Pydantic model body using type annotations.
fn extract_pydantic_fields_from_body(body: &Node, source: &str, fields: &mut Vec<Field>) {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() != "expression_statement" {
            continue;
        }

        let mut inner_cursor = child.walk();
        for inner in child.named_children(&mut inner_cursor) {
            match inner.kind() {
                // `name: str` (no default)
                "type" => {
                    extract_annotation_field(&inner, source, fields);
                }
                // `name: str = "default"` or `name: Optional[str] = None`
                "assignment" => {
                    extract_typed_field(&inner, source, fields);
                }
                _ => {}
            }
        }
    }
}

fn extract_typed_field(node: &Node, source: &str, fields: &mut Vec<Field>) {
    let left = match node.child_by_field_name("left") {
        Some(n) => n,
        None => return,
    };

    // For typed assignments like `name: str = "default"`, left is the identifier
    // and there's a `type` child
    let name = node_text(&left, source);
    if name.starts_with('_')
        || name.starts_with("class")
        || name == "Config"
        || name == "model_config"
    {
        return;
    }

    let type_node = node.child_by_field_name("type");
    let type_name = type_node.map(|t| node_text(&t, source)).unwrap_or_default();

    if type_name.is_empty() {
        return;
    }

    let optional = type_name.starts_with("Optional") || type_name.contains("None");

    fields.push(Field {
        name,
        type_name,
        optional,
    });
}

fn extract_annotation_field(node: &Node, source: &str, fields: &mut Vec<Field>) {
    // A bare annotation like `name: str`
    let text = node_text(node, source);
    let parts: Vec<&str> = text.splitn(2, ':').collect();
    if parts.len() != 2 {
        return;
    }

    let name = parts[0].trim().to_string();
    let type_name = parts[1].trim().to_string();

    if name.starts_with('_') || name == "Config" || name == "model_config" {
        return;
    }

    if type_name.is_empty() {
        return;
    }

    let optional = type_name.starts_with("Optional") || type_name.contains("None");

    fields.push(Field {
        name,
        type_name,
        optional,
    });
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_python(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_fastapi(&tree.root_node(), source, "main.py", &mut fragment);
        fragment
    }

    // -- Detection tests --

    #[test]
    fn detect_fastapi_from_pyproject() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            r#"
[project]
dependencies = [
    "fastapi>=0.100",
    "uvicorn",
]
"#,
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(FastApiAdapter.detect(&ctx));
    }

    #[test]
    fn detect_fastapi_from_requirements() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "fastapi==0.104.1\nuvicorn\n",
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(FastApiAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_fastapi() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "flask==2.0\n").unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!FastApiAdapter.detect(&ctx));
    }

    // -- Route extraction tests --

    #[test]
    fn extract_decorated_routes() {
        let frag = parse_python(
            r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
async def list_users():
    return []

@app.post("/users")
async def create_user(user: UserCreate):
    pass

@app.delete("/users/{user_id}")
async def delete_user(user_id: int):
    pass
"#,
        );

        assert!(
            frag.routes.contains_key("fastapi:GET:/users"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
        assert!(frag.routes.contains_key("fastapi:POST:/users"));
        assert!(frag.routes.contains_key("fastapi:DELETE:/users/{user_id}"));

        let route = &frag.routes["fastapi:GET:/users"];
        assert_eq!(route.path, "/users");
        assert_eq!(route.methods, vec!["GET"]);
        assert_eq!(route.handler.as_deref(), Some("list_users"));

        let endpoint = &frag.endpoints["fastapi:POST:/users"];
        assert_eq!(endpoint.method, "POST");
        assert_eq!(endpoint.handler, "create_user");
    }

    #[test]
    fn extract_router_routes() {
        let frag = parse_python(
            r#"
from fastapi import APIRouter

router = APIRouter()

@router.get("/items")
async def list_items():
    return []

@router.put("/items/{item_id}")
async def update_item(item_id: int):
    pass
"#,
        );

        assert!(frag.routes.contains_key("fastapi:GET:/items"));
        assert!(frag.routes.contains_key("fastapi:PUT:/items/{item_id}"));
    }

    // -- Pydantic model tests --

    #[test]
    fn extract_pydantic_models() {
        let frag = parse_python(
            r#"
from pydantic import BaseModel
from typing import Optional

class UserCreate(BaseModel):
    name: str
    email: str
    age: Optional[int] = None
"#,
        );

        assert!(frag.models.contains_key("UserCreate"));
        let model = &frag.models["UserCreate"];
        assert_eq!(model.orm, "pydantic");

        assert!(frag.types.contains_key("UserCreate"));
        let td = &frag.types["UserCreate"];
        assert!(td.implements.contains(&"BaseModel".to_string()));
    }

    // -- Full extract test --

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "fastapi==0.104.1\n").unwrap();

        std::fs::write(
            dir.path().join("main.py"),
            r#"from fastapi import FastAPI
app = FastAPI()

@app.get("/health")
async def health():
    return {"status": "ok"}
"#,
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![dir.path().join("main.py")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(FastApiAdapter.detect(&ctx));
        let frag = FastApiAdapter.extract(&ctx).unwrap();
        assert!(
            frag.routes.contains_key("fastapi:GET:/health"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
    }
}
