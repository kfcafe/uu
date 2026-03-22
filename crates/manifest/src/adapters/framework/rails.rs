//! Rails framework adapter — extracts routes from config/routes.rb and models from app/models/.

use std::path::Path;

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{DataModel, Endpoint, ManifestFragment, Route, RouteType};

pub struct RailsAdapter;

impl Adapter for RailsAdapter {
    fn name(&self) -> &str {
        "rails"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        let gemfile = ctx.root.join("Gemfile");
        let content = match std::fs::read_to_string(gemfile) {
            Ok(s) => s,
            Err(_) => return false,
        };
        // Match gem 'rails' or gem "rails"
        content.contains("'rails'") || content.contains("\"rails\"")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        // Parse routes
        let routes_path = ctx.root.join("config/routes.rb");
        if let Ok(source) = std::fs::read_to_string(&routes_path) {
            let rel = routes_path
                .strip_prefix(&ctx.root)
                .unwrap_or(&routes_path)
                .to_string_lossy()
                .to_string();
            extract_rails_routes(&source, &rel, &mut fragment);
        }

        // Parse models from app/models/*.rb
        let models_dir = ctx.root.join("app/models");
        if models_dir.is_dir() {
            for file in &ctx.files {
                if !file.starts_with(&models_dir) {
                    continue;
                }
                if file
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_none_or(|e| e != "rb")
                {
                    continue;
                }
                // Skip application_record.rb and concerns directory
                if should_skip_model(file, &models_dir) {
                    continue;
                }

                let source = match std::fs::read_to_string(file) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let rel = file
                    .strip_prefix(&ctx.root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .to_string();

                extract_rails_model(&source, &rel, &mut fragment);
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

fn should_skip_model(file: &Path, models_dir: &Path) -> bool {
    let rel = file.strip_prefix(models_dir).unwrap_or(file);
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name == "application_record.rb" {
        return true;
    }
    // Skip concerns subdirectory
    for component in rel.components() {
        if let std::path::Component::Normal(n) = component {
            if n.to_string_lossy().as_ref() == "concerns" {
                return true;
            }
        }
    }
    false
}

/// HTTP methods recognized in Rails routes.
const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];

/// Parse Rails routes.rb file line-by-line.
fn extract_rails_routes(source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut namespace_stack: Vec<String> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Track namespace blocks: `namespace :api do`
        if trimmed.starts_with("namespace ") {
            if let Some(ns) = extract_ruby_symbol_or_string(trimmed, "namespace") {
                namespace_stack.push(ns);
            }
            continue;
        }

        // Track scope blocks: `scope '/admin' do`
        if trimmed.starts_with("scope ") && trimmed.contains(" do") {
            if let Some(scope) = extract_scope_path(trimmed) {
                namespace_stack.push(scope);
            }
            continue;
        }

        // End of block
        if trimmed == "end" {
            namespace_stack.pop();
            continue;
        }

        // Match resources: `resources :users` or `resources :users, only: [:index, :show]`
        if trimmed.starts_with("resources ") || trimmed.starts_with("resources(") {
            if let Some(resource_name) = extract_ruby_symbol_or_string(trimmed, "resources") {
                let prefix = build_rails_prefix(&namespace_stack);
                let path = format!("{}/{}", prefix, resource_name);
                expand_rails_resources(&path, &resource_name, file, fragment);
            }
            continue;
        }

        // Match resource (singular): `resource :profile`
        if trimmed.starts_with("resource ") || trimmed.starts_with("resource(") {
            if let Some(resource_name) = extract_ruby_symbol_or_string(trimmed, "resource") {
                let prefix = build_rails_prefix(&namespace_stack);
                let path = format!("{}/{}", prefix, resource_name);
                expand_rails_singular_resource(&path, &resource_name, file, fragment);
            }
            continue;
        }

        // Match HTTP method routes: `get '/users', to: 'users#index'`
        for method in HTTP_METHODS {
            if trimmed.starts_with(method) && trimmed[method.len()..].starts_with(' ') {
                let prefix = build_rails_prefix(&namespace_stack);
                if let Some((path, handler)) = parse_rails_route_line(trimmed, method) {
                    let full_path = format!("{}{}", prefix, path);
                    let method_upper = method.to_uppercase();
                    let route_key = format!("rails:{}:{}", method_upper, full_path);

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

                    if let Some(h) = handler {
                        fragment.endpoints.insert(
                            route_key,
                            Endpoint {
                                path: full_path,
                                file: file.to_string(),
                                method: method_upper,
                                handler: h,
                                middleware: vec![],
                            },
                        );
                    }
                }
                break;
            }
        }

        // Match root route: `root 'pages#home'` or `root to: 'pages#home'`
        if let Some(rest) = trimmed.strip_prefix("root ") {
            let rest = rest.trim();
            let handler = extract_ruby_string(rest).or_else(|| {
                rest.strip_prefix("to:")
                    .or_else(|| rest.strip_prefix("to: "))
                    .and_then(|s| extract_ruby_string(s.trim()))
            });

            let route_key = "rails:GET:/".to_string();
            fragment.routes.insert(
                route_key.clone(),
                Route {
                    path: "/".to_string(),
                    file: file.to_string(),
                    route_type: RouteType::Controller,
                    methods: vec!["GET".to_string()],
                    handler: handler.clone(),
                },
            );

            if let Some(h) = handler {
                fragment.endpoints.insert(
                    route_key,
                    Endpoint {
                        path: "/".to_string(),
                        file: file.to_string(),
                        method: "GET".to_string(),
                        handler: h,
                        middleware: vec![],
                    },
                );
            }
        }
    }
}

/// Build the URL prefix from namespace stack.
fn build_rails_prefix(stack: &[String]) -> String {
    if stack.is_empty() {
        return String::new();
    }
    let mut prefix = String::new();
    for ns in stack {
        if !ns.starts_with('/') {
            prefix.push('/');
        }
        prefix.push_str(ns);
    }
    prefix
}

/// Expand `resources :users` into standard REST routes.
fn expand_rails_resources(path: &str, resource: &str, file: &str, fragment: &mut ManifestFragment) {
    let singular_path = format!("/{}/:id", path.trim_matches('/'));
    let base_path = format!("/{}", path.trim_matches('/'));
    let controller = resource.to_string();

    let crud = [
        ("GET", base_path.clone(), format!("{}#index", controller)),
        ("POST", base_path, format!("{}#create", controller)),
        ("GET", singular_path.clone(), format!("{}#show", controller)),
        (
            "PUT",
            singular_path.clone(),
            format!("{}#update", controller),
        ),
        (
            "PATCH",
            singular_path.clone(),
            format!("{}#update", controller),
        ),
        ("DELETE", singular_path, format!("{}#destroy", controller)),
    ];

    for (method, route_path, handler) in &crud {
        let route_key = format!("rails:{}:{}", method, route_path);
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
                handler: handler.clone(),
                middleware: vec![],
            },
        );
    }
}

/// Expand `resource :profile` (singular) into REST routes without :id.
fn expand_rails_singular_resource(
    path: &str,
    resource: &str,
    file: &str,
    fragment: &mut ManifestFragment,
) {
    let base_path = format!("/{}", path.trim_matches('/'));
    let controller = resource.to_string();

    let crud = [
        ("GET", base_path.clone(), format!("{}#show", controller)),
        ("POST", base_path.clone(), format!("{}#create", controller)),
        ("PUT", base_path.clone(), format!("{}#update", controller)),
        ("PATCH", base_path.clone(), format!("{}#update", controller)),
        ("DELETE", base_path, format!("{}#destroy", controller)),
    ];

    for (method, route_path, handler) in &crud {
        let route_key = format!("rails:{}:{}", method, route_path);
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
                handler: handler.clone(),
                middleware: vec![],
            },
        );
    }
}

/// Parse a route line like `get '/users', to: 'users#index'`.
fn parse_rails_route_line(line: &str, method: &str) -> Option<(String, Option<String>)> {
    let rest = line[method.len()..].trim();

    // Extract path - first quoted string
    let path = extract_ruby_string(rest)?;
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{}", path)
    };

    // Try to extract handler from `to: 'controller#action'`
    let handler = if let Some(to_idx) = rest.find("to:") {
        let after_to = rest[to_idx + 3..].trim();
        extract_ruby_string(after_to)
    } else {
        // Sometimes the handler is the second argument: `get '/users', 'users#index'`
        rest.find(',')
            .and_then(|i| extract_ruby_string(rest[i + 1..].trim()))
    };

    Some((path, handler))
}

/// Extract a symbol name from `resources :users` or `namespace :api`.
fn extract_ruby_symbol_or_string(line: &str, keyword: &str) -> Option<String> {
    let rest = line[keyword.len()..].trim().trim_start_matches('(').trim();

    // Try symbol first: :users
    if let Some(stripped) = rest.strip_prefix(':') {
        let name = stripped
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // Try string: '/users' or "/users"
    extract_ruby_string(rest)
}

/// Extract a scope path from `scope '/admin' do`.
fn extract_scope_path(line: &str) -> Option<String> {
    let rest = line["scope ".len()..].trim();
    extract_ruby_string(rest)
}

/// Extract a single or double-quoted Ruby string.
fn extract_ruby_string(s: &str) -> Option<String> {
    let s = s.trim();
    let quote = if s.starts_with('\'') {
        '\''
    } else if s.starts_with('"') {
        '"'
    } else {
        return None;
    };

    let end = s[1..].find(quote)?;
    Some(s[1..1 + end].to_string())
}

/// Parse a Rails model file to detect ActiveRecord classes.
fn extract_rails_model(source: &str, file: &str, fragment: &mut ManifestFragment) {
    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("class ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            // `class User < ApplicationRecord` or `class User < ActiveRecord::Base`
            let name = parts[1];
            let parent = parts[3];

            if parent == "ApplicationRecord" || parent == "ActiveRecord::Base" {
                let model = DataModel {
                    name: name.to_string(),
                    source: file.to_string(),
                    orm: "activerecord".to_string(),
                    fields: vec![],
                    relations: vec![],
                    indexes: vec![],
                };
                fragment.models.insert(name.to_string(), model);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rails() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Gemfile"),
            "source 'https://rubygems.org'\ngem 'rails', '~> 7.0'\ngem 'pg'\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Ruby,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(RailsAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_rails() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Gemfile"),
            "source 'https://rubygems.org'\ngem 'sinatra'\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Ruby,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!RailsAdapter.detect(&ctx));
    }

    #[test]
    fn extract_routes() {
        let source = r#"
Rails.application.routes.draw do
  root 'pages#home'
  get '/about', to: 'pages#about'
  resources :users
  resources :posts
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_rails_routes(source, "config/routes.rb", &mut fragment);

        assert!(fragment.routes.contains_key("rails:GET:/"));
        assert!(
            fragment.routes.contains_key("rails:GET:/about"),
            "keys: {:?}",
            fragment.routes.keys().collect::<Vec<_>>()
        );

        // Resources should expand
        assert!(fragment.routes.contains_key("rails:GET:/users"));
        assert!(fragment.routes.contains_key("rails:POST:/users"));
        assert!(fragment.routes.contains_key("rails:GET:/users/:id"));
        assert!(fragment.routes.contains_key("rails:DELETE:/users/:id"));

        let ep = &fragment.endpoints["rails:GET:/users"];
        assert_eq!(ep.handler, "users#index");
    }

    #[test]
    fn namespaced_routes() {
        let source = r#"
Rails.application.routes.draw do
  namespace :api do
    resources :items
  end
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_rails_routes(source, "config/routes.rb", &mut fragment);

        assert!(
            fragment.routes.contains_key("rails:GET:/api/items"),
            "keys: {:?}",
            fragment.routes.keys().collect::<Vec<_>>()
        );
        assert!(fragment.routes.contains_key("rails:POST:/api/items"));
    }

    #[test]
    fn extract_model() {
        let source = r#"
class User < ApplicationRecord
  validates :email, presence: true
  has_many :posts
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_rails_model(source, "app/models/user.rb", &mut fragment);

        assert!(fragment.models.contains_key("User"));
        assert_eq!(fragment.models["User"].orm, "activerecord");
    }

    #[test]
    fn extract_model_active_record_base() {
        let source = "class Legacy < ActiveRecord::Base\nend\n";
        let mut fragment = ManifestFragment::default();
        extract_rails_model(source, "app/models/legacy.rb", &mut fragment);

        assert!(fragment.models.contains_key("Legacy"));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = dir.path().join("config");
        let models = dir.path().join("app/models");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&models).unwrap();

        std::fs::write(dir.path().join("Gemfile"), "gem 'rails', '~> 7.0'\n").unwrap();

        std::fs::write(
            config.join("routes.rb"),
            "Rails.application.routes.draw do\n  resources :products\nend\n",
        )
        .unwrap();

        std::fs::write(
            models.join("product.rb"),
            "class Product < ApplicationRecord\nend\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Ruby,
            files: vec![models.join("product.rb")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(RailsAdapter.detect(&ctx));
        let frag = RailsAdapter.extract(&ctx).unwrap();
        assert!(frag.routes.contains_key("rails:GET:/products"));
        assert!(frag.models.contains_key("Product"));
    }
}
