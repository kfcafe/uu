//! Phoenix framework adapter — extracts routes from router.ex files.

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Endpoint, ManifestFragment, Route, RouteType};

pub struct PhoenixAdapter;

impl Adapter for PhoenixAdapter {
    fn name(&self) -> &str {
        "phoenix"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        has_mix_dep(ctx, ":phoenix")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        for file in &ctx.files {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if !name.ends_with(".ex") {
                continue;
            }

            // Look for router files (router.ex or files containing Phoenix.Router)
            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if !source.contains("Phoenix.Router") && !name.contains("router") {
                continue;
            }

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            extract_phoenix_routes(&source, &rel, &mut fragment);
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

/// Check if a mix.exs dependency is present.
fn has_mix_dep(ctx: &ProjectContext, dep_atom: &str) -> bool {
    let mix_path = ctx.root.join("mix.exs");
    let content = match std::fs::read_to_string(mix_path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    content.contains(dep_atom)
}

/// HTTP methods recognized in Phoenix router definitions.
const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "options", "head"];

/// Parse Phoenix router file line-by-line extracting routes.
fn extract_phoenix_routes(source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut scope_prefixes: Vec<String> = Vec::new();
    let mut pipeline_stack: Vec<String> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Track scope nesting: `scope "/api" do`
        if trimmed.starts_with("scope ") {
            if let Some(path) = extract_elixir_string(trimmed) {
                scope_prefixes.push(path);
            }
            continue;
        }

        // Track pipe_through: `pipe_through [:browser]` or `pipe_through :api`
        if trimmed.starts_with("pipe_through") {
            let rest = trimmed.trim_start_matches("pipe_through").trim();
            let pipeline = rest
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim_start_matches(':')
                .trim_matches(|c: char| c == ' ' || c == ',')
                .to_string();
            if !pipeline.is_empty() {
                pipeline_stack.push(pipeline);
            }
            continue;
        }

        // End of scope/pipeline block
        if trimmed == "end" {
            if !scope_prefixes.is_empty() {
                scope_prefixes.pop();
            }
            continue;
        }

        // Match resources: `resources "/users", UserController`
        if trimmed.starts_with("resources ") {
            if let Some((path, controller)) = parse_route_line(trimmed, "resources") {
                let full_path = build_full_path(&scope_prefixes, &path);
                expand_resources(&full_path, &controller, file, &pipeline_stack, fragment);
            }
            continue;
        }

        // Match HTTP method routes: `get "/users", UserController, :index`
        for method in HTTP_METHODS {
            if trimmed.starts_with(method) && trimmed[method.len()..].starts_with(' ') {
                if let Some((path, controller)) = parse_route_line(trimmed, method) {
                    let full_path = build_full_path(&scope_prefixes, &path);
                    let method_upper = method.to_uppercase();

                    // Extract action if present (third arg after controller)
                    let action = extract_action(trimmed);
                    let handler = if let Some(act) = &action {
                        Some(format!("{}:{}", controller, act))
                    } else {
                        Some(controller.clone())
                    };

                    let route_key = format!("phoenix:{}:{}", method_upper, full_path);
                    fragment.routes.insert(
                        route_key.clone(),
                        Route {
                            path: full_path.clone(),
                            file: file.to_string(),
                            route_type: RouteType::Controller,
                            methods: vec![method_upper.clone()],
                            handler: handler.clone(),
                        },
                    );

                    fragment.endpoints.insert(
                        route_key,
                        Endpoint {
                            path: full_path,
                            file: file.to_string(),
                            method: method_upper,
                            handler: handler.unwrap_or_default(),
                            middleware: pipeline_stack.clone(),
                        },
                    );
                }
                break;
            }
        }
    }
}

/// Build a full path by joining scope prefixes.
fn build_full_path(prefixes: &[String], path: &str) -> String {
    let mut full = String::new();
    for prefix in prefixes {
        let p = prefix.trim_end_matches('/');
        full.push_str(p);
    }
    if !path.starts_with('/') {
        full.push('/');
    }
    full.push_str(path);
    if full.is_empty() {
        "/".to_string()
    } else {
        full
    }
}

/// Expand `resources "/users", UserController` into CRUD routes.
fn expand_resources(
    path: &str,
    controller: &str,
    file: &str,
    pipelines: &[String],
    fragment: &mut ManifestFragment,
) {
    let singular_path = format!("{}/:id", path.trim_end_matches('/'));
    let crud = [
        ("GET", path.to_string(), "index"),
        ("GET", format!("{}/new", path.trim_end_matches('/')), "new"),
        ("POST", path.to_string(), "create"),
        ("GET", singular_path.clone(), "show"),
        ("GET", format!("{}/edit", singular_path), "edit"),
        ("PUT", singular_path.clone(), "update"),
        ("PATCH", singular_path.clone(), "update"),
        ("DELETE", singular_path, "delete"),
    ];

    for (method, route_path, action) in &crud {
        let handler = format!("{}:{}", controller, action);
        let route_key = format!("phoenix:{}:{}", method, route_path);
        fragment.routes.insert(
            route_key.clone(),
            Route {
                path: route_path.clone(),
                file: file.to_string(),
                route_type: RouteType::Controller,
                methods: vec![method.to_string()],
                handler: Some(handler.clone()),
            },
        );
        fragment.endpoints.insert(
            route_key,
            Endpoint {
                path: route_path.clone(),
                file: file.to_string(),
                method: method.to_string(),
                handler,
                middleware: pipelines.to_vec(),
            },
        );
    }
}

/// Parse a route line like `get "/users", UserController, :index` returning (path, controller).
fn parse_route_line(line: &str, keyword: &str) -> Option<(String, String)> {
    let rest = line[keyword.len()..].trim();
    let path = extract_elixir_string_from(rest)?;

    // Find the controller name after the path string and comma
    let after_path = rest.find(',').map(|i| rest[i + 1..].trim()).unwrap_or("");

    let controller = after_path
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    if controller.is_empty() {
        return None;
    }

    Some((path, controller))
}

/// Extract the action atom from a line like `get "/users", UserController, :index`.
fn extract_action(line: &str) -> Option<String> {
    // Find the last `:atom` in the line
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() >= 3 {
        let action_part = parts.last()?.trim();
        if let Some(stripped) = action_part.strip_prefix(':') {
            return Some(stripped.to_string());
        }
    }
    None
}

/// Extract a quoted string from Elixir source like `"/users"`.
fn extract_elixir_string(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let end = s[start + 1..].find('"')?;
    Some(s[start + 1..start + 1 + end].to_string())
}

/// Extract a quoted string starting from the beginning of a string.
fn extract_elixir_string_from(s: &str) -> Option<String> {
    extract_elixir_string(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_phoenix() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mix.exs"),
            r#"
defmodule MyApp.MixProject do
  defp deps do
    [{:phoenix, "~> 1.7"}, {:ecto_sql, "~> 3.10"}]
  end
end
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Elixir { escript: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(PhoenixAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_phoenix() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mix.exs"),
            r#"
defmodule MyApp.MixProject do
  defp deps do
    [{:plug, "~> 1.14"}]
  end
end
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Elixir { escript: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!PhoenixAdapter.detect(&ctx));
    }

    #[test]
    fn extract_simple_routes() {
        let source = r#"
defmodule MyAppWeb.Router do
  use MyAppWeb, :router
  use Phoenix.Router

  get "/", PageController, :index
  post "/login", AuthController, :create
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_phoenix_routes(source, "lib/my_app_web/router.ex", &mut fragment);

        assert!(fragment.routes.contains_key("phoenix:GET:/"));
        assert!(fragment.routes.contains_key("phoenix:POST:/login"));

        let route = &fragment.routes["phoenix:GET:/"];
        assert_eq!(route.methods, vec!["GET"]);
        assert_eq!(route.handler.as_deref(), Some("PageController:index"));
    }

    #[test]
    fn extract_scoped_routes() {
        let source = r#"
defmodule MyAppWeb.Router do
  use Phoenix.Router

  scope "/api" do
    get "/users", UserController, :index
    post "/users", UserController, :create
  end
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_phoenix_routes(source, "lib/router.ex", &mut fragment);

        assert!(
            fragment.routes.contains_key("phoenix:GET:/api/users"),
            "keys: {:?}",
            fragment.routes.keys().collect::<Vec<_>>()
        );
        assert!(fragment.routes.contains_key("phoenix:POST:/api/users"));
    }

    #[test]
    fn expand_resources() {
        let source = r#"
defmodule MyAppWeb.Router do
  use Phoenix.Router

  resources "/posts", PostController
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_phoenix_routes(source, "lib/router.ex", &mut fragment);

        assert!(fragment.routes.contains_key("phoenix:GET:/posts"));
        assert!(fragment.routes.contains_key("phoenix:POST:/posts"));
        assert!(fragment.routes.contains_key("phoenix:GET:/posts/:id"));
        assert!(fragment.routes.contains_key("phoenix:DELETE:/posts/:id"));

        let index = &fragment.routes["phoenix:GET:/posts"];
        assert_eq!(index.handler.as_deref(), Some("PostController:index"));
    }

    #[test]
    fn pipe_through_captured() {
        let source = r#"
defmodule MyAppWeb.Router do
  use Phoenix.Router

  scope "/api" do
    pipe_through :api
    get "/items", ItemController, :index
  end
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_phoenix_routes(source, "lib/router.ex", &mut fragment);

        let ep = &fragment.endpoints["phoenix:GET:/api/items"];
        assert!(ep.middleware.contains(&"api".to_string()));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let lib = dir.path().join("lib/my_app_web");
        std::fs::create_dir_all(&lib).unwrap();

        std::fs::write(
            dir.path().join("mix.exs"),
            "defmodule MyApp do\n  defp deps do\n    [{:phoenix, \"~> 1.7\"}]\n  end\nend\n",
        )
        .unwrap();

        std::fs::write(
            lib.join("router.ex"),
            "defmodule MyAppWeb.Router do\n  use Phoenix.Router\n  get \"/\", PageController, :index\nend\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Elixir { escript: false },
            files: vec![lib.join("router.ex")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(PhoenixAdapter.detect(&ctx));
        let frag = PhoenixAdapter.extract(&ctx).unwrap();
        assert!(frag.routes.contains_key("phoenix:GET:/"));
    }
}
