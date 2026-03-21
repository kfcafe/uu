//! Next.js framework adapter — extracts routes from App Router and Pages Router conventions.

use std::path::Path;

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{ManifestFragment, Route, RouteType};

pub struct NextJsAdapter;

impl Adapter for NextJsAdapter {
    fn name(&self) -> &str {
        "next.js"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        has_dependency(ctx, "next")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        extract_app_router(ctx, &mut fragment);
        extract_pages_router(ctx, &mut fragment);
        extract_middleware(ctx, &mut fragment);

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        50
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Framework
    }
}

/// Check whether a dependency exists in package.json dependencies or devDependencies.
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

/// Scan `app/` directory for App Router file conventions.
fn extract_app_router(ctx: &ProjectContext, fragment: &mut ManifestFragment) {
    let app_dir = ctx.root.join("app");
    if !app_dir.is_dir() {
        // Also check src/app/
        let src_app_dir = ctx.root.join("src").join("app");
        if src_app_dir.is_dir() {
            scan_app_dir(ctx, &src_app_dir, fragment);
        }
        return;
    }
    scan_app_dir(ctx, &app_dir, fragment);
}

fn scan_app_dir(ctx: &ProjectContext, app_dir: &Path, fragment: &mut ManifestFragment) {
    for file in &ctx.files {
        let rel = match file.strip_prefix(app_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let file_stem = match file.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };

        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "ts" | "tsx" | "js" | "jsx") {
            continue;
        }

        let route_type = match file_stem {
            "page" => RouteType::Page,
            "layout" => RouteType::Layout,
            "route" => RouteType::ApiRoute,
            "loading" | "error" | "not-found" => continue,
            _ => continue,
        };

        // Build URL path from directory structure
        let dir = rel.parent().unwrap_or(Path::new(""));
        let url_path = if dir.as_os_str().is_empty() {
            "/".to_string()
        } else {
            format!("/{}", dir.to_string_lossy().replace('\\', "/"))
        };

        let rel_file = file
            .strip_prefix(&ctx.root)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        let methods = if matches!(route_type, RouteType::ApiRoute) {
            extract_route_methods(file)
        } else {
            vec![]
        };

        let key = format!("nextjs:{}", url_path);
        fragment.routes.insert(
            key,
            Route {
                path: url_path,
                file: rel_file,
                route_type,
                methods,
                handler: None,
            },
        );
    }
}

/// Extract exported HTTP method names from a route.ts file (GET, POST, PUT, DELETE, PATCH).
fn extract_route_methods(file: &Path) -> Vec<String> {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let http_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
    let mut found = Vec::new();

    for method in &http_methods {
        // Match `export function GET` or `export async function GET` or `export const GET`
        let patterns = [
            format!("export function {}", method),
            format!("export async function {}", method),
            format!("export const {}", method),
        ];
        for pattern in &patterns {
            if source.contains(pattern) {
                found.push(method.to_string());
                break;
            }
        }
    }

    found
}

/// Scan `pages/` directory for Pages Router conventions.
fn extract_pages_router(ctx: &ProjectContext, fragment: &mut ManifestFragment) {
    let pages_dir = ctx.root.join("pages");
    if !pages_dir.is_dir() {
        let src_pages_dir = ctx.root.join("src").join("pages");
        if src_pages_dir.is_dir() {
            scan_pages_dir(ctx, &src_pages_dir, fragment);
        }
        return;
    }
    scan_pages_dir(ctx, &pages_dir, fragment);
}

fn scan_pages_dir(ctx: &ProjectContext, pages_dir: &Path, fragment: &mut ManifestFragment) {
    for file in &ctx.files {
        let rel = match file.strip_prefix(pages_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "ts" | "tsx" | "js" | "jsx") {
            continue;
        }

        let file_stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // Skip _app, _document, _error
        if file_stem.starts_with('_') {
            continue;
        }

        let rel_str = rel.to_string_lossy().replace('\\', "/");
        // Remove extension to build path
        let without_ext = rel_str.rsplit_once('.').map(|(p, _)| p).unwrap_or(&rel_str);

        let url_path = if without_ext == "index" {
            "/".to_string()
        } else {
            let cleaned = without_ext.strip_suffix("/index").unwrap_or(without_ext);
            format!("/{}", cleaned)
        };

        let is_api = rel.starts_with("api") || rel.starts_with("api/");
        let route_type = if is_api {
            RouteType::ApiRoute
        } else {
            RouteType::Page
        };

        let rel_file = file
            .strip_prefix(&ctx.root)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        let key = format!("nextjs-pages:{}", url_path);
        fragment.routes.insert(
            key,
            Route {
                path: url_path,
                file: rel_file,
                route_type,
                methods: vec![],
                handler: None,
            },
        );
    }
}

/// Check for middleware.ts at root.
fn extract_middleware(ctx: &ProjectContext, fragment: &mut ManifestFragment) {
    let middleware_names = ["middleware.ts", "middleware.js"];
    for name in &middleware_names {
        let path = ctx.root.join(name);
        if path.exists() {
            fragment.routes.insert(
                "nextjs:middleware".to_string(),
                Route {
                    path: "/".to_string(),
                    file: name.to_string(),
                    route_type: RouteType::Middleware,
                    methods: vec![],
                    handler: None,
                },
            );
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn build_ctx(dir: &TempDir, pkg_json: Option<serde_json::Value>) -> ProjectContext {
        let root = dir.path().to_path_buf();
        let files = walk_files(&root);
        ProjectContext {
            root,
            kind: uu_detect::ProjectKind::Node {
                manager: uu_detect::NodePM::Npm,
            },
            files,
            package_json: pkg_json,
            cargo_toml: None,
            go_mod: None,
        }
    }

    fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        fn recurse(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        recurse(&path, files);
                    } else {
                        files.push(path);
                    }
                }
            }
        }
        recurse(root, &mut files);
        files.sort();
        files
    }

    fn pkg_with_next() -> serde_json::Value {
        serde_json::json!({
            "dependencies": {
                "next": "14.0.0",
                "react": "18.0.0"
            }
        })
    }

    #[test]
    fn detect_next_in_dependencies() {
        let dir = TempDir::new().unwrap();
        let ctx = build_ctx(&dir, Some(pkg_with_next()));
        let adapter = NextJsAdapter;
        assert!(adapter.detect(&ctx));
    }

    #[test]
    fn detect_next_in_dev_dependencies() {
        let dir = TempDir::new().unwrap();
        let pkg = serde_json::json!({
            "devDependencies": { "next": "14.0.0" }
        });
        let ctx = build_ctx(&dir, Some(pkg));
        assert!(NextJsAdapter.detect(&ctx));
    }

    #[test]
    fn no_detection_without_next() {
        let dir = TempDir::new().unwrap();
        let pkg = serde_json::json!({
            "dependencies": { "react": "18.0.0" }
        });
        let ctx = build_ctx(&dir, Some(pkg));
        assert!(!NextJsAdapter.detect(&ctx));
    }

    #[test]
    fn app_router_pages_and_layouts() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create app/ directory structure
        std::fs::create_dir_all(root.join("app/products/[id]")).unwrap();
        std::fs::write(
            root.join("app/page.tsx"),
            "export default function Home() {}",
        )
        .unwrap();
        std::fs::write(
            root.join("app/layout.tsx"),
            "export default function Layout() {}",
        )
        .unwrap();
        std::fs::write(
            root.join("app/products/[id]/page.tsx"),
            "export default function Product() {}",
        )
        .unwrap();

        let ctx = build_ctx(&dir, Some(pkg_with_next()));
        let frag = NextJsAdapter.extract(&ctx).unwrap();

        assert!(frag.routes.contains_key("nextjs:/"));
        assert_eq!(frag.routes["nextjs:/"].route_type, RouteType::Page);

        assert!(frag.routes.contains_key("nextjs:/products/[id]"));
        assert_eq!(frag.routes["nextjs:/products/[id]"].path, "/products/[id]");
    }

    #[test]
    fn app_router_api_route_methods() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("app/api/users")).unwrap();
        std::fs::write(
            root.join("app/api/users/route.ts"),
            r#"
export async function GET(request: Request) { return Response.json([]); }
export async function POST(request: Request) { return Response.json({}); }
"#,
        )
        .unwrap();

        let ctx = build_ctx(&dir, Some(pkg_with_next()));
        let frag = NextJsAdapter.extract(&ctx).unwrap();

        let route = &frag.routes["nextjs:/api/users"];
        assert_eq!(route.route_type, RouteType::ApiRoute);
        assert!(route.methods.contains(&"GET".to_string()));
        assert!(route.methods.contains(&"POST".to_string()));
    }

    #[test]
    fn pages_router_extraction() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("pages/api")).unwrap();
        std::fs::write(
            root.join("pages/about.tsx"),
            "export default function About() {}",
        )
        .unwrap();
        std::fs::write(
            root.join("pages/api/users.ts"),
            "export default function handler() {}",
        )
        .unwrap();

        let ctx = build_ctx(&dir, Some(pkg_with_next()));
        let frag = NextJsAdapter.extract(&ctx).unwrap();

        assert!(frag.routes.contains_key("nextjs-pages:/about"));
        assert_eq!(
            frag.routes["nextjs-pages:/about"].route_type,
            RouteType::Page
        );

        assert!(frag.routes.contains_key("nextjs-pages:/api/users"));
        assert_eq!(
            frag.routes["nextjs-pages:/api/users"].route_type,
            RouteType::ApiRoute
        );
    }

    #[test]
    fn middleware_detected() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("middleware.ts"),
            "export function middleware() {}",
        )
        .unwrap();

        let ctx = build_ctx(&dir, Some(pkg_with_next()));
        let frag = NextJsAdapter.extract(&ctx).unwrap();

        assert!(frag.routes.contains_key("nextjs:middleware"));
        assert_eq!(
            frag.routes["nextjs:middleware"].route_type,
            RouteType::Middleware
        );
    }
}
