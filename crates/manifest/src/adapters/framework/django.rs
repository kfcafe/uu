//! Django framework adapter — extracts routes from urls.py and models from models.py.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{
    DataModel, Endpoint, Field, ManifestFragment, Relation, RelationKind, Route, RouteType,
};

pub struct DjangoAdapter;

impl Adapter for DjangoAdapter {
    fn name(&self) -> &str {
        "django"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        has_python_dependency(ctx, "django") || has_python_dependency(ctx, "Django")
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

            let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            match filename {
                "urls.py" => extract_django_urls(&source, &rel, &mut fragment),
                "models.py" => {
                    let tree = match parser.parse(&source, None) {
                        Some(t) => t,
                        None => continue,
                    };
                    extract_django_models(&tree.root_node(), &source, &rel, &mut fragment);
                }
                "views.py" => {
                    let tree = match parser.parse(&source, None) {
                        Some(t) => t,
                        None => continue,
                    };
                    extract_django_views(&tree.root_node(), &source, &rel, &mut fragment);
                }
                _ => {}
            }
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
    // Check pyproject.toml
    let pyproject_path = ctx.root.join("pyproject.toml");
    if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
        if let Ok(toml_val) = content.parse::<toml::Value>() {
            // [project.dependencies] — PEP 621
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
            // [tool.poetry.dependencies]
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

    // Check requirements.txt
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
                "__pycache__" | ".venv" | "venv" | "site-packages" | "migrations"
            ) {
                return true;
            }
        }
    }
    false
}

// -- urls.py parsing (line-based) --------------------------------------------

/// Parse Django url patterns from urls.py content.
/// Handles `path('route/', handler)` and `path('route/', include('app.urls'))`.
fn extract_django_urls(source: &str, file: &str, fragment: &mut ManifestFragment) {
    for line in source.lines() {
        let trimmed = line.trim();

        // Match path(...) calls
        if let Some(inner) = extract_call_args(trimmed, "path") {
            parse_path_call(inner, file, fragment);
        }
        // Match re_path(...) calls
        if let Some(inner) = extract_call_args(trimmed, "re_path") {
            parse_path_call(inner, file, fragment);
        }
    }
}

/// Extract the arguments string from a function call like `path('foo', bar)`.
fn extract_call_args<'a>(line: &'a str, func_name: &str) -> Option<&'a str> {
    // Find `path(` or `re_path(`
    let idx = line.find(&format!("{}(", func_name))?;
    let after = &line[idx + func_name.len() + 1..];
    // Find matching closing paren (simple: just find the last ')' on the line)
    let end = after.rfind(')')?;
    Some(&after[..end])
}

/// Parse a single path() call's arguments.
fn parse_path_call(args: &str, file: &str, fragment: &mut ManifestFragment) {
    // Split on first comma to separate route pattern from handler
    let parts: Vec<&str> = args.splitn(2, ',').collect();
    if parts.is_empty() {
        return;
    }

    let route_str = extract_string_literal(parts[0].trim());
    if route_str.is_empty() {
        return;
    }

    let path = format!("/{}", route_str.trim_start_matches('/'));

    if parts.len() > 1 {
        let handler_part = parts[1].trim();

        // Check for include() — prefix mount
        if handler_part.contains("include(") {
            let route_key = format!("django:INCLUDE:{}", path);
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
        } else {
            // Regular view handler
            let handler_name = handler_part
                .split([',', ')'])
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches("views.")
                .to_string();

            let handler = if handler_name.is_empty() || handler_name.len() > 100 {
                None
            } else {
                Some(handler_name)
            };

            let route_key = format!("django:{}", path);
            fragment.routes.insert(
                route_key.clone(),
                Route {
                    path: path.clone(),
                    file: file.to_string(),
                    route_type: RouteType::Controller,
                    methods: vec![],
                    handler: handler.clone(),
                },
            );

            if let Some(h) = handler {
                fragment.endpoints.insert(
                    route_key,
                    Endpoint {
                        path,
                        file: file.to_string(),
                        method: String::new(),
                        handler: h,
                        middleware: vec![],
                    },
                );
            }
        }
    }
}

/// Extract a string literal value, stripping quotes.
fn extract_string_literal(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        String::new()
    }
}

// -- models.py parsing (tree-sitter) -----------------------------------------

/// Extract Django models from a parsed Python AST.
fn extract_django_models(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "class_definition" {
            try_extract_model(&child, source, file, fragment);
        }
    }
}

/// Check if a class inherits from models.Model and extract its fields.
fn try_extract_model(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    // Check superclasses for models.Model
    let superclasses = match node.child_by_field_name("superclasses") {
        Some(s) => s,
        None => return,
    };

    let mut is_django_model = false;
    let mut sc_cursor = superclasses.walk();
    for child in superclasses.named_children(&mut sc_cursor) {
        let text = node_text(&child, source);
        if text == "models.Model" || text == "Model" {
            is_django_model = true;
            break;
        }
    }

    if !is_django_model {
        return;
    }

    let body = match node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    let mut fields = Vec::new();
    let mut relations = Vec::new();

    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "expression_statement" {
            // Look for assignment: `name = models.CharField(...)`
            let mut inner_cursor = child.walk();
            for inner in child.named_children(&mut inner_cursor) {
                if inner.kind() == "assignment" {
                    extract_model_field(&inner, source, &mut fields, &mut relations);
                }
            }
        }
    }

    let model = DataModel {
        name: name.clone(),
        source: file.to_string(),
        orm: "django".to_string(),
        fields,
        relations,
        indexes: vec![],
    };
    fragment.models.insert(name, model);
}

/// Extract a field or relation from a Django model assignment.
fn extract_model_field(
    node: &Node,
    source: &str,
    fields: &mut Vec<Field>,
    relations: &mut Vec<Relation>,
) {
    let left = match node.child_by_field_name("left") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let right = match node.child_by_field_name("right") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    // Extract the field type from `models.CharField(...)` or `CharField(...)`
    let type_name = right
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches("models.")
        .to_string();

    if type_name.is_empty() {
        return;
    }

    // Check for relation types
    match type_name.as_str() {
        "ForeignKey" => {
            let target = extract_first_arg(&right);
            relations.push(Relation {
                name: left,
                kind: RelationKind::BelongsTo,
                target,
            });
        }
        "ManyToManyField" => {
            let target = extract_first_arg(&right);
            relations.push(Relation {
                name: left,
                kind: RelationKind::ManyToMany,
                target,
            });
        }
        "OneToOneField" => {
            let target = extract_first_arg(&right);
            relations.push(Relation {
                name: left,
                kind: RelationKind::HasOne,
                target,
            });
        }
        _ => {
            let optional = right.contains("null=True") || right.contains("blank=True");
            fields.push(Field {
                name: left,
                type_name,
                optional,
            });
        }
    }
}

/// Extract the first positional argument from a call expression string.
/// e.g. `ForeignKey('Category', on_delete=...)` → "Category"
fn extract_first_arg(call_str: &str) -> String {
    let inner = call_str.split_once('(').map(|(_, rest)| rest).unwrap_or("");

    let arg = inner
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches(')')
        .trim()
        .trim_matches('\'')
        .trim_matches('"');

    arg.to_string()
}

// -- views.py parsing (tree-sitter) ------------------------------------------

/// Basic view detection from views.py — find top-level functions and classes.
fn extract_django_views(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let key = format!("django:view:{}", name);
                    fragment.endpoints.insert(
                        key,
                        Endpoint {
                            path: String::new(),
                            file: file.to_string(),
                            method: String::new(),
                            handler: name,
                            middleware: vec![],
                        },
                    );
                }
            }
            "decorated_definition" => {
                if let Some(def) = child.child_by_field_name("definition") {
                    if def.kind() == "function_definition" {
                        if let Some(name_node) = def.child_by_field_name("name") {
                            let name = node_text(&name_node, source);
                            let key = format!("django:view:{}", name);
                            fragment.endpoints.insert(
                                key,
                                Endpoint {
                                    path: String::new(),
                                    file: file.to_string(),
                                    method: String::new(),
                                    handler: name,
                                    middleware: vec![],
                                },
                            );
                        }
                    }
                }
            }
            "class_definition" => {
                // Class-based views
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    if name.ends_with("View") || name.ends_with("ViewSet") {
                        let key = format!("django:view:{}", name);
                        fragment.endpoints.insert(
                            key,
                            Endpoint {
                                path: String::new(),
                                file: file.to_string(),
                                method: String::new(),
                                handler: name,
                                middleware: vec![],
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Detection tests --

    #[test]
    fn detect_django_from_pyproject_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            r#"
[project]
dependencies = [
    "django>=4.2",
    "celery",
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
        assert!(DjangoAdapter.detect(&ctx));
    }

    #[test]
    fn detect_django_from_requirements_txt() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "Django==4.2\ncelery\n").unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(DjangoAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_django() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "flask==2.0\ncelery\n").unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!DjangoAdapter.detect(&ctx));
    }

    #[test]
    fn detect_django_poetry() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            r#"
[tool.poetry.dependencies]
python = "^3.11"
Django = "^4.2"
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
        assert!(DjangoAdapter.detect(&ctx));
    }

    // -- urls.py tests --

    #[test]
    fn extract_url_patterns() {
        let source = r#"
from django.urls import path, include
from . import views

urlpatterns = [
    path('products/', views.product_list),
    path('products/<int:id>/', views.product_detail),
    path('api/', include('api.urls')),
]
"#;
        let mut fragment = ManifestFragment::default();
        extract_django_urls(source, "myapp/urls.py", &mut fragment);

        assert!(
            fragment.routes.contains_key("django:/products/"),
            "keys: {:?}",
            fragment.routes.keys().collect::<Vec<_>>()
        );
        let route = &fragment.routes["django:/products/"];
        assert_eq!(route.path, "/products/");
        assert_eq!(route.handler.as_deref(), Some("product_list"));

        assert!(fragment.routes.contains_key("django:/products/<int:id>/"));

        // include() should be a middleware/mount
        assert!(fragment.routes.contains_key("django:INCLUDE:/api/"));
        assert_eq!(
            fragment.routes["django:INCLUDE:/api/"].route_type,
            RouteType::Middleware
        );
    }

    // -- models.py tests --

    #[test]
    fn extract_django_model_fields() {
        let source = r#"
from django.db import models

class Product(models.Model):
    name = models.CharField(max_length=200)
    price = models.DecimalField(max_digits=10, decimal_places=2)
    description = models.TextField(null=True, blank=True)
    category = models.ForeignKey('Category', on_delete=models.CASCADE)
    tags = models.ManyToManyField('Tag')
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_django_models(&tree.root_node(), source, "myapp/models.py", &mut fragment);

        assert!(fragment.models.contains_key("Product"));
        let model = &fragment.models["Product"];
        assert_eq!(model.orm, "django");

        // Regular fields
        let field_names: Vec<&str> = model.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"price"));
        assert!(field_names.contains(&"description"));

        let name_field = model.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.type_name, "CharField");

        let desc_field = model
            .fields
            .iter()
            .find(|f| f.name == "description")
            .unwrap();
        assert!(desc_field.optional);

        // Relations
        assert_eq!(model.relations.len(), 2);
        let cat_rel = model
            .relations
            .iter()
            .find(|r| r.name == "category")
            .unwrap();
        assert_eq!(cat_rel.target, "Category");
        assert_eq!(cat_rel.kind, RelationKind::BelongsTo);

        let tag_rel = model.relations.iter().find(|r| r.name == "tags").unwrap();
        assert_eq!(tag_rel.target, "Tag");
        assert_eq!(tag_rel.kind, RelationKind::ManyToMany);
    }

    // -- views.py tests --

    #[test]
    fn extract_django_view_functions() {
        let source = r#"
from django.http import JsonResponse

def product_list(request):
    return JsonResponse({"products": []})

class ProductDetailView(View):
    def get(self, request, pk):
        pass
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_django_views(&tree.root_node(), source, "myapp/views.py", &mut fragment);

        assert!(fragment.endpoints.contains_key("django:view:product_list"));
        assert!(fragment
            .endpoints
            .contains_key("django:view:ProductDetailView"));
    }

    // -- Full extract test --

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let app = dir.path().join("myapp");
        std::fs::create_dir_all(&app).unwrap();

        std::fs::write(dir.path().join("requirements.txt"), "Django==4.2\n").unwrap();

        std::fs::write(
            app.join("urls.py"),
            "from django.urls import path\nfrom . import views\nurlpatterns = [\n    path('items/', views.item_list),\n]\n",
        )
        .unwrap();

        std::fs::write(
            app.join("models.py"),
            "from django.db import models\n\nclass Item(models.Model):\n    name = models.CharField(max_length=100)\n",
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Python { uv: false },
            files: vec![app.join("urls.py"), app.join("models.py")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(DjangoAdapter.detect(&ctx));
        let frag = DjangoAdapter.extract(&ctx).unwrap();
        assert!(frag.routes.contains_key("django:/items/"));
        assert!(frag.models.contains_key("Item"));
    }
}
