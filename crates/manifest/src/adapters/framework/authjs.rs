//! Auth.js / NextAuth adapter — detects authentication providers and strategy.

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{AuthConfig, ManifestFragment};

pub struct AuthJsAdapter;

impl Adapter for AuthJsAdapter {
    fn name(&self) -> &str {
        "authjs"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        has_auth_dependency(ctx)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut providers = Vec::new();
        let mut strategy = String::new();

        // Look for auth config files
        let auth_files = find_auth_config_files(ctx);

        for file in &auth_files {
            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            extract_providers(&source, &mut providers);
            if strategy.is_empty() {
                strategy = extract_strategy(&source);
            }
        }

        if !providers.is_empty() || !strategy.is_empty() {
            fragment.auth = Some(AuthConfig {
                providers,
                strategy,
            });
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        30
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Framework
    }
}

/// Check for @auth/core or next-auth in package.json.
fn has_auth_dependency(ctx: &ProjectContext) -> bool {
    let Some(pkg) = &ctx.package_json else {
        return false;
    };
    for section in ["dependencies", "devDependencies"] {
        if let Some(deps) = pkg.get(section) {
            if deps.get("@auth/core").is_some() || deps.get("next-auth").is_some() {
                return true;
            }
        }
    }
    false
}

/// Find auth config files in common locations.
fn find_auth_config_files(ctx: &ProjectContext) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();

    // Common auth config file names
    let candidates = [
        "auth.ts",
        "auth.js",
        "src/auth.ts",
        "src/auth.js",
        "lib/auth.ts",
        "lib/auth.js",
        "src/lib/auth.ts",
        "src/lib/auth.js",
    ];

    for candidate in &candidates {
        let path = ctx.root.join(candidate);
        if path.exists() {
            found.push(path);
        }
    }

    // Also check for [...nextauth] route handler
    let nextauth_patterns = [
        "app/api/auth/[...nextauth]/route.ts",
        "app/api/auth/[...nextauth]/route.js",
        "pages/api/auth/[...nextauth].ts",
        "pages/api/auth/[...nextauth].js",
    ];

    for pattern in &nextauth_patterns {
        let path = ctx.root.join(pattern);
        if path.exists() {
            found.push(path);
        }
    }

    found
}

/// Extract provider names from auth config source code.
fn extract_providers(source: &str, providers: &mut Vec<String>) {
    // Look for common provider imports/usages:
    // import Google from "@auth/core/providers/google"
    // import GitHub from "next-auth/providers/github"
    // GoogleProvider, GitHubProvider, CredentialsProvider
    let known_providers = [
        ("google", "Google"),
        ("github", "GitHub"),
        ("credentials", "Credentials"),
        ("discord", "Discord"),
        ("twitter", "Twitter"),
        ("facebook", "Facebook"),
        ("apple", "Apple"),
        ("azure-ad", "AzureAD"),
        ("auth0", "Auth0"),
        ("cognito", "Cognito"),
        ("email", "Email"),
        ("okta", "Okta"),
        ("slack", "Slack"),
        ("spotify", "Spotify"),
        ("twitch", "Twitch"),
        ("linkedin", "LinkedIn"),
    ];

    for (slug, display_name) in &known_providers {
        // Match import paths like providers/google or providers/github
        let pattern = format!("providers/{}", slug);
        if source.contains(&pattern) && !providers.contains(&display_name.to_string()) {
            providers.push(display_name.to_string());
        }
    }

    providers.sort();
}

/// Extract session strategy from auth config.
fn extract_strategy(source: &str) -> String {
    // Look for strategy: "jwt" or strategy: "database"
    if source.contains("strategy: \"jwt\"") || source.contains("strategy: 'jwt'") {
        return "jwt".to_string();
    }
    if source.contains("strategy: \"database\"") || source.contains("strategy: 'database'") {
        return "database".to_string();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_next_auth() {
        let dir = TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Node {
                manager: project_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: Some(serde_json::json!({
                "dependencies": { "next-auth": "5.0.0" }
            })),
            cargo_toml: None,
            go_mod: None,
        };
        assert!(AuthJsAdapter.detect(&ctx));
    }

    #[test]
    fn detect_auth_core() {
        let dir = TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Node {
                manager: project_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: Some(serde_json::json!({
                "dependencies": { "@auth/core": "0.30.0" }
            })),
            cargo_toml: None,
            go_mod: None,
        };
        assert!(AuthJsAdapter.detect(&ctx));
    }

    #[test]
    fn no_detection_without_auth() {
        let dir = TempDir::new().unwrap();
        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Node {
                manager: project_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: Some(serde_json::json!({
                "dependencies": { "express": "4.18.0" }
            })),
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!AuthJsAdapter.detect(&ctx));
    }

    #[test]
    fn extract_providers_from_config() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join("auth.ts"),
            r#"
import NextAuth from "next-auth"
import Google from "next-auth/providers/google"
import GitHub from "next-auth/providers/github"
import Credentials from "next-auth/providers/credentials"

export const { handlers, auth, signIn, signOut } = NextAuth({
  providers: [Google, GitHub, Credentials({})],
  session: {
    strategy: "jwt"
  }
})
"#,
        )
        .unwrap();

        let ctx = ProjectContext {
            root: root.to_path_buf(),
            kind: project_detect::ProjectKind::Node {
                manager: project_detect::NodePM::Npm,
            },
            files: vec![root.join("auth.ts")],
            package_json: Some(serde_json::json!({
                "dependencies": { "next-auth": "5.0.0" }
            })),
            cargo_toml: None,
            go_mod: None,
        };

        let frag = AuthJsAdapter.extract(&ctx).unwrap();
        let auth = frag.auth.unwrap();

        assert!(auth.providers.contains(&"Google".to_string()));
        assert!(auth.providers.contains(&"GitHub".to_string()));
        assert!(auth.providers.contains(&"Credentials".to_string()));
        assert_eq!(auth.strategy, "jwt");
    }

    #[test]
    fn extract_database_strategy() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("auth.ts"),
            r#"
import Google from "@auth/core/providers/google"
export default { providers: [Google], session: { strategy: "database" } }
"#,
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: project_detect::ProjectKind::Node {
                manager: project_detect::NodePM::Npm,
            },
            files: vec![dir.path().join("auth.ts")],
            package_json: Some(serde_json::json!({
                "dependencies": { "@auth/core": "0.30.0" }
            })),
            cargo_toml: None,
            go_mod: None,
        };

        let frag = AuthJsAdapter.extract(&ctx).unwrap();
        let auth = frag.auth.unwrap();
        assert!(auth.providers.contains(&"Google".to_string()));
        assert_eq!(auth.strategy, "database");
    }
}
