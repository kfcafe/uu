//! Spring Boot adapter — extracts routes from annotations and JPA entities.

use std::path::Path;

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{DataModel, Endpoint, ManifestFragment, Route, RouteType};

pub struct SpringAdapter;

impl Adapter for SpringAdapter {
    fn name(&self) -> &str {
        "spring"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        // Check build.gradle
        let gradle = ctx.root.join("build.gradle");
        if let Ok(content) = std::fs::read_to_string(gradle) {
            if content.contains("org.springframework.boot") {
                return true;
            }
        }

        // Check build.gradle.kts
        let gradle_kts = ctx.root.join("build.gradle.kts");
        if let Ok(content) = std::fs::read_to_string(gradle_kts) {
            if content.contains("org.springframework.boot") {
                return true;
            }
        }

        // Check pom.xml
        let pom = ctx.root.join("pom.xml");
        if let Ok(content) = std::fs::read_to_string(pom) {
            if content.contains("spring-boot") {
                return true;
            }
        }

        false
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        for file in &ctx.files {
            if !file
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "java" || e == "kt")
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

            extract_spring_annotations(&source, &rel, &mut fragment);
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
            if matches!(name.as_ref(), "build" | "target" | ".gradle" | "test") {
                return true;
            }
        }
    }
    false
}

/// Mapping annotation names to HTTP methods.
const MAPPING_ANNOTATIONS: &[(&str, &str)] = &[
    ("@GetMapping", "GET"),
    ("@PostMapping", "POST"),
    ("@PutMapping", "PUT"),
    ("@DeleteMapping", "DELETE"),
    ("@PatchMapping", "PATCH"),
    ("@RequestMapping", ""),
];

/// Parse Spring annotations from Java/Kotlin source using line-based parsing.
fn extract_spring_annotations(source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut class_base_path = String::new();
    let mut is_controller = false;
    let mut is_entity = false;
    let mut current_class = String::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Detect controller classes
        if trimmed.starts_with("@RestController") || trimmed.starts_with("@Controller") {
            is_controller = true;
            continue;
        }

        // Detect @RequestMapping on class level for base path
        if is_controller && trimmed.starts_with("@RequestMapping") && current_class.is_empty() {
            class_base_path = extract_annotation_path(trimmed).unwrap_or_default();
            continue;
        }

        // Detect @Entity annotation
        if trimmed.starts_with("@Entity") {
            is_entity = true;
            continue;
        }

        // Class declaration
        if trimmed.contains("class ") && (is_controller || is_entity) {
            current_class = extract_class_name(trimmed).unwrap_or_default();

            if is_entity && !current_class.is_empty() {
                let model = DataModel {
                    name: current_class.clone(),
                    source: file.to_string(),
                    orm: "jpa".to_string(),
                    fields: vec![],
                    relations: vec![],
                    indexes: vec![],
                };
                fragment.models.insert(current_class.clone(), model);
                is_entity = false;
            }
            continue;
        }

        // Method-level mapping annotations
        if is_controller {
            for (annotation, method) in MAPPING_ANNOTATIONS {
                if trimmed.starts_with(annotation) {
                    let path = extract_annotation_path(trimmed).unwrap_or_default();
                    let full_path = if class_base_path.is_empty() {
                        if path.is_empty() {
                            "/".to_string()
                        } else if path.starts_with('/') {
                            path.clone()
                        } else {
                            format!("/{}", path)
                        }
                    } else {
                        let base = class_base_path.trim_end_matches('/');
                        if path.is_empty() {
                            base.to_string()
                        } else if path.starts_with('/') {
                            format!("{}{}", base, path)
                        } else {
                            format!("{}/{}", base, path)
                        }
                    };

                    let http_method = if method.is_empty() {
                        // @RequestMapping defaults to GET (or could be any)
                        extract_request_mapping_method(trimmed).unwrap_or_else(|| "GET".to_string())
                    } else {
                        method.to_string()
                    };

                    let route_key = format!("spring:{}:{}", http_method, full_path);
                    fragment.routes.insert(
                        route_key.clone(),
                        Route {
                            path: full_path.clone(),
                            file: file.to_string(),
                            route_type: RouteType::ApiRoute,
                            methods: vec![http_method.clone()],
                            handler: None,
                        },
                    );

                    fragment.endpoints.insert(
                        route_key,
                        Endpoint {
                            path: full_path,
                            file: file.to_string(),
                            method: http_method,
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

/// Extract path from annotation like `@GetMapping("/users")` or `@GetMapping(value = "/users")`.
fn extract_annotation_path(line: &str) -> Option<String> {
    let paren_start = line.find('(')?;
    let paren_end = line.rfind(')')?;
    let inner = &line[paren_start + 1..paren_end];

    // Handle value = "..." or just "..."
    let value = if inner.contains("value") {
        inner
            .split("value")
            .nth(1)?
            .trim()
            .trim_start_matches('=')
            .trim()
    } else {
        inner.trim()
    };

    // Extract quoted string
    let start = value.find('"')?;
    let end = value[start + 1..].find('"')?;
    Some(value[start + 1..start + 1 + end].to_string())
}

/// Extract method from `@RequestMapping(method = RequestMethod.POST, ...)`.
fn extract_request_mapping_method(line: &str) -> Option<String> {
    let upper = line.to_uppercase();
    if upper.contains("REQUESTMETHOD.POST") || upper.contains("METHOD.POST") {
        Some("POST".to_string())
    } else if upper.contains("REQUESTMETHOD.PUT") || upper.contains("METHOD.PUT") {
        Some("PUT".to_string())
    } else if upper.contains("REQUESTMETHOD.DELETE") || upper.contains("METHOD.DELETE") {
        Some("DELETE".to_string())
    } else if upper.contains("REQUESTMETHOD.PATCH") || upper.contains("METHOD.PATCH") {
        Some("PATCH".to_string())
    } else {
        None
    }
}

/// Extract class name from a line like `public class UserController {`.
fn extract_class_name(line: &str) -> Option<String> {
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
    fn detect_spring_gradle() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("build.gradle"),
            r#"
plugins {
    id 'org.springframework.boot' version '3.2.0'
}
dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web'
}
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Gradle { wrapper: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(SpringAdapter.detect(&ctx));
    }

    #[test]
    fn detect_spring_pom() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("pom.xml"),
            r#"
<project>
  <parent>
    <artifactId>spring-boot-starter-parent</artifactId>
  </parent>
</project>
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Maven,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(SpringAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_spring() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("build.gradle"), "apply plugin: 'java'\n").unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Gradle { wrapper: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!SpringAdapter.detect(&ctx));
    }

    #[test]
    fn extract_controller_routes() {
        let source = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping
    public List<User> listUsers() { return List.of(); }

    @GetMapping("/{id}")
    public User getUser(@PathVariable Long id) { return null; }

    @PostMapping
    public User createUser(@RequestBody User user) { return null; }

    @DeleteMapping("/{id}")
    public void deleteUser(@PathVariable Long id) {}
}
"#;
        let mut fragment = ManifestFragment::default();
        extract_spring_annotations(source, "UserController.java", &mut fragment);

        assert!(
            fragment.routes.contains_key("spring:GET:/api/users"),
            "keys: {:?}",
            fragment.routes.keys().collect::<Vec<_>>()
        );
        assert!(fragment.routes.contains_key("spring:GET:/api/users/{id}"));
        assert!(fragment.routes.contains_key("spring:POST:/api/users"));
        assert!(fragment
            .routes
            .contains_key("spring:DELETE:/api/users/{id}"));

        let ep = &fragment.endpoints["spring:GET:/api/users"];
        assert_eq!(ep.handler, "UserController");
    }

    #[test]
    fn extract_entity() {
        let source = r#"
@Entity
public class Product {
    @Id
    @GeneratedValue
    private Long id;
    private String name;
    private BigDecimal price;
}
"#;
        let mut fragment = ManifestFragment::default();
        extract_spring_annotations(source, "Product.java", &mut fragment);

        assert!(fragment.models.contains_key("Product"));
        assert_eq!(fragment.models["Product"].orm, "jpa");
    }

    #[test]
    fn controller_without_base_path() {
        let source = r#"
@RestController
public class HealthController {

    @GetMapping("/health")
    public String health() { return "ok"; }
}
"#;
        let mut fragment = ManifestFragment::default();
        extract_spring_annotations(source, "HealthController.java", &mut fragment);

        assert!(fragment.routes.contains_key("spring:GET:/health"));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("src/main/java/com/example");
        std::fs::create_dir_all(&src).unwrap();

        std::fs::write(
            dir.path().join("build.gradle"),
            "plugins { id 'org.springframework.boot' version '3.2.0' }\n",
        )
        .unwrap();

        std::fs::write(
            src.join("ItemController.java"),
            "@RestController\npublic class ItemController {\n    @GetMapping(\"/items\")\n    public String list() { return \"[]\"; }\n}\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Gradle { wrapper: false },
            files: vec![src.join("ItemController.java")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(SpringAdapter.detect(&ctx));
        let frag = SpringAdapter.extract(&ctx).unwrap();
        assert!(frag.routes.contains_key("spring:GET:/items"));
    }
}
