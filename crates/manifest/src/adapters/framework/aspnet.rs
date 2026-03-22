//! ASP.NET adapter — extracts routes from controller attributes and EF Core DbSets.

use std::path::Path;

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{DataModel, Endpoint, ManifestFragment, Route, RouteType};

pub struct AspNetAdapter;

impl Adapter for AspNetAdapter {
    fn name(&self) -> &str {
        "aspnet"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        // Look for Microsoft.AspNetCore in any .csproj file
        for file in &ctx.files {
            if file
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "csproj")
            {
                if let Ok(content) = std::fs::read_to_string(file) {
                    if content.contains("Microsoft.AspNetCore") {
                        return true;
                    }
                }
            }
        }

        // Also check top-level .csproj
        if let Ok(entries) = std::fs::read_dir(&ctx.root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("csproj") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if content.contains("Microsoft.AspNetCore") {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        for file in &ctx.files {
            if file
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| e != "cs")
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

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            extract_aspnet_annotations(&source, &rel, &mut fragment);
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
            if matches!(name.as_ref(), "bin" | "obj" | "Properties" | "Migrations") {
                return true;
            }
        }
    }
    false
}

/// HTTP method attributes we recognize in ASP.NET controllers.
const HTTP_ATTRIBUTES: &[(&str, &str)] = &[
    ("[HttpGet", "GET"),
    ("[HttpPost", "POST"),
    ("[HttpPut", "PUT"),
    ("[HttpDelete", "DELETE"),
    ("[HttpPatch", "PATCH"),
];

/// Parse ASP.NET controller attributes and DbContext from C# source.
fn extract_aspnet_annotations(source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut is_controller = false;
    let mut controller_route = String::new();
    let mut current_class = String::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Detect [ApiController] attribute
        if trimmed == "[ApiController]" {
            is_controller = true;
            continue;
        }

        // Detect [Route("api/[controller]")] on class level
        if trimmed.starts_with("[Route(") && current_class.is_empty() {
            controller_route = extract_attribute_path(trimmed).unwrap_or_default();
            continue;
        }

        // Class declaration
        if trimmed.contains("class ") {
            let class_name = extract_csharp_class_name(trimmed);
            if let Some(name) = class_name {
                current_class = name.clone();

                // If the route contains [controller], substitute with the class name
                if controller_route.contains("[controller]") {
                    let controller_name = name.strip_suffix("Controller").unwrap_or(&name);
                    controller_route = controller_route.replace("[controller]", controller_name);
                }

                // Check for DbContext — extract DbSet properties
                if is_dbcontext(trimmed) {
                    extract_dbsets(source, file, fragment);
                    return;
                }
            }
            continue;
        }

        // HTTP method attributes on action methods
        if is_controller {
            for (attr, method) in HTTP_ATTRIBUTES {
                if trimmed.starts_with(attr) {
                    let action_path = extract_attribute_path(trimmed).unwrap_or_default();
                    let full_path = build_aspnet_path(&controller_route, &action_path);

                    let route_key = format!("aspnet:{}:{}", method, full_path);
                    fragment.routes.insert(
                        route_key.clone(),
                        Route {
                            path: full_path.clone(),
                            file: file.to_string(),
                            route_type: RouteType::ApiRoute,
                            methods: vec![method.to_string()],
                            handler: None,
                        },
                    );

                    fragment.endpoints.insert(
                        route_key,
                        Endpoint {
                            path: full_path,
                            file: file.to_string(),
                            method: method.to_string(),
                            handler: current_class.clone(),
                            middleware: vec![],
                        },
                    );
                    break;
                }
            }
        }
    }
}

/// Build the full path from controller route and action path.
fn build_aspnet_path(controller_route: &str, action_path: &str) -> String {
    let base = if controller_route.is_empty() {
        String::new()
    } else {
        let r = controller_route
            .trim_start_matches('/')
            .trim_end_matches('/');
        format!("/{}", r)
    };

    if action_path.is_empty() {
        if base.is_empty() {
            "/".to_string()
        } else {
            base
        }
    } else if action_path.starts_with('/') {
        action_path.to_string()
    } else {
        format!("{}/{}", base, action_path.trim_start_matches('/'))
    }
}

/// Check if a class declaration inherits from DbContext.
fn is_dbcontext(line: &str) -> bool {
    line.contains("DbContext") && line.contains(":")
}

/// Extract DbSet<T> properties from a DbContext class.
fn extract_dbsets(source: &str, file: &str, fragment: &mut ManifestFragment) {
    for line in source.lines() {
        let trimmed = line.trim();

        // Look for `public DbSet<Entity> Entities { get; set; }`
        if trimmed.contains("DbSet<") {
            if let Some(entity_name) = extract_dbset_type(trimmed) {
                let model = DataModel {
                    name: entity_name.clone(),
                    source: file.to_string(),
                    orm: "efcore".to_string(),
                    fields: vec![],
                    relations: vec![],
                    indexes: vec![],
                };
                fragment.models.insert(entity_name, model);
            }
        }
    }
}

/// Extract the type parameter from `DbSet<User>`.
fn extract_dbset_type(line: &str) -> Option<String> {
    let start = line.find("DbSet<")? + 6;
    let end = line[start..].find('>')?;
    let name = line[start..start + end].trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Extract the path from an attribute like `[HttpGet("{id}")]` or `[Route("api/[controller]")]`.
fn extract_attribute_path(line: &str) -> Option<String> {
    let paren_start = line.find('(')?;
    let paren_end = line.rfind(')')?;
    let inner = &line[paren_start + 1..paren_end];

    // Extract quoted string
    let start = inner.find('"')?;
    let end = inner[start + 1..].find('"')?;
    Some(inner[start + 1..start + 1 + end].to_string())
}

/// Extract class name from a C# class declaration.
fn extract_csharp_class_name(line: &str) -> Option<String> {
    let idx = line.find("class ")?;
    let rest = &line[idx + 6..];
    let name = rest
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_aspnet() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("MyApp.csproj"),
            r#"
<Project Sdk="Microsoft.NET.Sdk.Web">
  <ItemGroup>
    <PackageReference Include="Microsoft.AspNetCore.OpenApi" Version="8.0.0" />
  </ItemGroup>
</Project>
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::DotNet { sln: false },
            files: vec![dir.path().join("MyApp.csproj")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(AspNetAdapter.detect(&ctx));
    }

    #[test]
    fn detect_aspnet_from_root_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Api.csproj"),
            "<Project><ItemGroup><PackageReference Include=\"Microsoft.AspNetCore.Mvc\" /></ItemGroup></Project>",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::DotNet { sln: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(AspNetAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_aspnet() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("MyApp.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk\"><ItemGroup></ItemGroup></Project>",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::DotNet { sln: false },
            files: vec![dir.path().join("MyApp.csproj")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!AspNetAdapter.detect(&ctx));
    }

    #[test]
    fn extract_controller_routes() {
        let source = r#"
using Microsoft.AspNetCore.Mvc;

[ApiController]
[Route("api/[controller]")]
public class UsersController : ControllerBase
{
    [HttpGet]
    public IActionResult GetAll() => Ok();

    [HttpGet("{id}")]
    public IActionResult GetById(int id) => Ok();

    [HttpPost]
    public IActionResult Create(User user) => Ok();

    [HttpDelete("{id}")]
    public IActionResult Delete(int id) => Ok();
}
"#;
        let mut fragment = ManifestFragment::default();
        extract_aspnet_annotations(source, "Controllers/UsersController.cs", &mut fragment);

        assert!(
            fragment.routes.contains_key("aspnet:GET:/api/Users"),
            "keys: {:?}",
            fragment.routes.keys().collect::<Vec<_>>()
        );
        assert!(fragment.routes.contains_key("aspnet:GET:/api/Users/{id}"));
        assert!(fragment.routes.contains_key("aspnet:POST:/api/Users"));
        assert!(fragment
            .routes
            .contains_key("aspnet:DELETE:/api/Users/{id}"));

        let ep = &fragment.endpoints["aspnet:GET:/api/Users"];
        assert_eq!(ep.handler, "UsersController");
    }

    #[test]
    fn extract_dbcontext_models() {
        let source = r#"
using Microsoft.EntityFrameworkCore;

public class AppDbContext : DbContext
{
    public DbSet<User> Users { get; set; }
    public DbSet<Product> Products { get; set; }
    public DbSet<Order> Orders { get; set; }
}
"#;
        let mut fragment = ManifestFragment::default();
        extract_aspnet_annotations(source, "Data/AppDbContext.cs", &mut fragment);

        assert!(fragment.models.contains_key("User"));
        assert!(fragment.models.contains_key("Product"));
        assert!(fragment.models.contains_key("Order"));

        assert_eq!(fragment.models["User"].orm, "efcore");
    }

    #[test]
    fn controller_without_route_prefix() {
        let source = r#"
[ApiController]
public class HealthController : ControllerBase
{
    [HttpGet("/health")]
    public IActionResult Health() => Ok();
}
"#;
        let mut fragment = ManifestFragment::default();
        extract_aspnet_annotations(source, "HealthController.cs", &mut fragment);

        assert!(fragment.routes.contains_key("aspnet:GET:/health"));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let controllers = dir.path().join("Controllers");
        std::fs::create_dir_all(&controllers).unwrap();

        std::fs::write(
            dir.path().join("App.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"><ItemGroup><PackageReference Include=\"Microsoft.AspNetCore.OpenApi\" /></ItemGroup></Project>",
        )
        .unwrap();

        std::fs::write(
            controllers.join("ItemsController.cs"),
            "[ApiController]\n[Route(\"api/[controller]\")]\npublic class ItemsController : ControllerBase\n{\n    [HttpGet]\n    public IActionResult List() => Ok();\n}\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::DotNet { sln: false },
            files: vec![
                dir.path().join("App.csproj"),
                controllers.join("ItemsController.cs"),
            ],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(AspNetAdapter.detect(&ctx));
        let frag = AspNetAdapter.extract(&ctx).unwrap();
        assert!(
            frag.routes.contains_key("aspnet:GET:/api/Items"),
            "keys: {:?}",
            frag.routes.keys().collect::<Vec<_>>()
        );
    }
}
