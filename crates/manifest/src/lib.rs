//! Project manifest generator for the `uu` universal utilities suite.
//!
//! Scans a codebase using language and framework adapters to produce a
//! structured YAML manifest of types, functions, routes, models, and more.

pub mod adapters;
pub mod context;
pub mod diff;
pub mod schema;

use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;

pub use adapters::{Adapter, AdapterLayer};
pub use context::ProjectContext;
pub use schema::{Manifest, ManifestFragment, ProjectMeta};

/// Generate a manifest for the project rooted at `root`.
///
/// 1. Detects the project kind via `uu_detect::detect`.
/// 2. Builds a `ProjectContext` with source files and parsed configs.
/// 3. Runs all matching adapters (sorted by priority, highest first).
/// 4. Merges adapter fragments into a single `Manifest`.
pub fn generate(root: &Path) -> Result<Manifest> {
    let kind = uu_detect::detect(root)
        .ok_or_else(|| anyhow::anyhow!("could not detect project type in {}", root.display()))?;

    let ctx = ProjectContext::build(root, &kind)?;

    let all = adapters::all_adapters();
    let mut matching: Vec<_> = all.iter().filter(|a| a.detect(&ctx)).collect();
    matching.sort_by_key(|a| std::cmp::Reverse(a.priority()));

    let fragments: Vec<ManifestFragment> = matching
        .iter()
        .map(|a| a.extract(&ctx))
        .collect::<Result<Vec<_>>>()?;

    let mut manifest = merge(fragments);
    manifest.project = ProjectMeta {
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        kind: kind.label().to_string(),
        frameworks: vec![],
        generated_at: {
            let secs = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let s = secs % 60;
            let m = (secs / 60) % 60;
            let h = (secs / 3600) % 24;
            let days = secs / 86400;
            // days since 1970-01-01
            let mut y = 1970i64;
            let mut rem = days as i64;
            loop {
                let year_days = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                    366
                } else {
                    365
                };
                if rem < year_days {
                    break;
                }
                rem -= year_days;
                y += 1;
            }
            let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
            let mdays = [
                31,
                if leap { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ];
            let mut mo = 0usize;
            for &md in &mdays {
                if rem < md {
                    break;
                }
                rem -= md;
                mo += 1;
            }
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                y,
                mo + 1,
                rem + 1,
                h,
                m,
                s
            )
        },
        uu_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    clean_module_paths(&mut manifest);

    Ok(manifest)
}

/// Merge multiple adapter fragments into a single manifest.
///
/// For `BTreeMap` fields, later fragments override earlier ones on key
/// collision (framework adapters run after language adapters, so their
/// output takes precedence). `Vec` fields are concatenated. `Option`
/// fields use the last `Some` value.
pub fn merge(fragments: Vec<ManifestFragment>) -> Manifest {
    let mut manifest = Manifest::default();
    for frag in fragments {
        manifest.types.extend(frag.types);
        manifest.functions.extend(frag.functions);
        manifest.modules.extend(frag.modules);
        manifest.routes.extend(frag.routes);
        manifest.endpoints.extend(frag.endpoints);
        manifest.models.extend(frag.models);
        if frag.auth.is_some() {
            manifest.auth = frag.auth;
        }
        manifest.components.extend(frag.components);
        manifest.integrations.extend(frag.integrations);
    }
    manifest
}

/// Remove private symbols and test functions from the manifest.
pub fn filter_public(manifest: &mut Manifest) {
    manifest
        .types
        .retain(|_, type_def| !matches!(type_def.visibility, schema::Visibility::Private));
    manifest.functions.retain(|_, function| {
        !matches!(function.visibility, schema::Visibility::Private) && !function.is_test
    });
}

fn clean_module_paths(manifest: &mut Manifest) {
    manifest.modules = std::mem::take(&mut manifest.modules)
        .into_iter()
        .map(|(key, mut module)| {
            let cleaned_key = clean_module_path(&key);
            module.path = clean_module_path(&module.path);
            (cleaned_key, module)
        })
        .collect();
}

fn clean_module_path(path: &str) -> String {
    if !path.contains("::") {
        return path.to_string();
    }

    let mut parts: Vec<&str> = path.split("::").collect();
    if matches!(parts.first(), Some(&"crates")) {
        parts.remove(0);
    }
    parts.retain(|part| *part != "src");
    parts.join("::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::diff;
    use crate::schema::*;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::tempdir;

    // -- Merge tests ---------------------------------------------------------

    #[test]
    fn merge_overlapping_keys_last_wins() {
        let mut types_a = BTreeMap::new();
        types_a.insert(
            "User".into(),
            TypeDef {
                name: "User".into(),
                source: "a.rs".into(),
                kind: TypeKind::Struct,
                ..Default::default()
            },
        );
        types_a.insert(
            "Config".into(),
            TypeDef {
                name: "Config".into(),
                source: "a.rs".into(),
                ..Default::default()
            },
        );

        let mut types_b = BTreeMap::new();
        types_b.insert(
            "User".into(),
            TypeDef {
                name: "User".into(),
                source: "b.rs".into(),
                kind: TypeKind::Class,
                ..Default::default()
            },
        );

        let frag_a = ManifestFragment {
            types: types_a,
            ..Default::default()
        };
        let frag_b = ManifestFragment {
            types: types_b,
            ..Default::default()
        };

        let result = merge(vec![frag_a, frag_b]);

        // "User" should come from frag_b (last wins)
        assert_eq!(result.types["User"].source, "b.rs");
        assert_eq!(result.types["User"].kind, TypeKind::Class);
        // "Config" should still be present from frag_a
        assert_eq!(result.types["Config"].source, "a.rs");
    }

    #[test]
    fn merge_vecs_are_concatenated() {
        let frag_a = ManifestFragment {
            components: vec![Component {
                name: "Button".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let frag_b = ManifestFragment {
            components: vec![Component {
                name: "Modal".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = merge(vec![frag_a, frag_b]);
        assert_eq!(result.components.len(), 2);
        assert_eq!(result.components[0].name, "Button");
        assert_eq!(result.components[1].name, "Modal");
    }

    #[test]
    fn merge_auth_last_some_wins() {
        let frag_a = ManifestFragment {
            auth: Some(AuthConfig {
                strategy: "jwt".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let frag_b = ManifestFragment {
            auth: Some(AuthConfig {
                strategy: "oauth".into(),
                providers: vec!["google".into()],
            }),
            ..Default::default()
        };

        let result = merge(vec![frag_a, frag_b]);
        let auth = result.auth.unwrap();
        assert_eq!(auth.strategy, "oauth");
        assert_eq!(auth.providers, vec!["google"]);
    }

    #[test]
    fn merge_empty_fragment_is_identity() {
        let mut types = BTreeMap::new();
        types.insert(
            "Foo".into(),
            TypeDef {
                name: "Foo".into(),
                ..Default::default()
            },
        );

        let frag = ManifestFragment {
            types,
            ..Default::default()
        };
        let empty = ManifestFragment::default();

        let result = merge(vec![frag.clone(), empty]);
        assert_eq!(result.types, frag.types);
    }

    #[test]
    fn clean_module_paths_strips_crates_and_src() {
        let mut manifest = Manifest::default();
        manifest.modules.insert(
            "crates::manifest::src::adapters::lang::rust".into(),
            Module {
                path: "crates::manifest::src::adapters::lang::rust".into(),
                file: "crates/manifest/src/adapters/lang/rust.rs".into(),
                ..Default::default()
            },
        );

        clean_module_paths(&mut manifest);

        assert!(manifest
            .modules
            .contains_key("manifest::adapters::lang::rust"));
        assert_eq!(
            manifest.modules["manifest::adapters::lang::rust"].path,
            "manifest::adapters::lang::rust"
        );
    }

    #[test]
    fn filter_public_removes_private_symbols_and_tests() {
        let mut manifest = Manifest::default();
        manifest.types.insert(
            "PublicType".into(),
            TypeDef {
                name: "PublicType".into(),
                visibility: Visibility::Public,
                ..Default::default()
            },
        );
        manifest.types.insert(
            "PrivateType".into(),
            TypeDef {
                name: "PrivateType".into(),
                visibility: Visibility::Private,
                ..Default::default()
            },
        );
        manifest.functions.insert(
            "public_fn".into(),
            Function {
                name: "public_fn".into(),
                visibility: Visibility::Public,
                ..Default::default()
            },
        );
        manifest.functions.insert(
            "private_fn".into(),
            Function {
                name: "private_fn".into(),
                visibility: Visibility::Private,
                ..Default::default()
            },
        );
        manifest.functions.insert(
            "test_fn".into(),
            Function {
                name: "test_fn".into(),
                visibility: Visibility::Public,
                is_test: true,
                ..Default::default()
            },
        );

        filter_public(&mut manifest);

        assert!(manifest.types.contains_key("PublicType"));
        assert!(!manifest.types.contains_key("PrivateType"));
        assert!(manifest.functions.contains_key("public_fn"));
        assert!(!manifest.functions.contains_key("private_fn"));
        assert!(!manifest.functions.contains_key("test_fn"));
    }

    #[test]
    fn filter_public_keeps_public_and_internal_items() {
        let mut manifest = Manifest::default();
        manifest.types.insert(
            "PublicType".into(),
            TypeDef {
                name: "PublicType".into(),
                visibility: Visibility::Public,
                ..Default::default()
            },
        );
        manifest.types.insert(
            "InternalType".into(),
            TypeDef {
                name: "InternalType".into(),
                visibility: Visibility::Internal,
                ..Default::default()
            },
        );
        manifest.functions.insert(
            "public_fn".into(),
            Function {
                name: "public_fn".into(),
                visibility: Visibility::Public,
                ..Default::default()
            },
        );
        manifest.functions.insert(
            "internal_fn".into(),
            Function {
                name: "internal_fn".into(),
                visibility: Visibility::Internal,
                ..Default::default()
            },
        );

        filter_public(&mut manifest);

        assert_eq!(manifest.types.len(), 2);
        assert!(manifest.types.contains_key("PublicType"));
        assert!(manifest.types.contains_key("InternalType"));
        assert_eq!(manifest.functions.len(), 2);
        assert!(manifest.functions.contains_key("public_fn"));
        assert!(manifest.functions.contains_key("internal_fn"));
    }

    // -- Diff tests ----------------------------------------------------------

    #[test]
    fn diff_identical_manifests_is_empty() {
        let manifest = Manifest {
            project: ProjectMeta {
                name: "test".into(),
                kind: "Rust".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let d = diff(&manifest, &manifest);
        assert!(d.is_empty());
    }

    #[test]
    fn diff_detects_added_types() {
        let old = Manifest::default();
        let mut new = Manifest::default();
        new.types.insert(
            "NewType".into(),
            TypeDef {
                name: "NewType".into(),
                source: "src/main.rs".into(),
                kind: TypeKind::Struct,
                ..Default::default()
            },
        );

        let d = diff(&old, &new);
        assert!(!d.is_empty());
        assert!(d.types.added.contains_key("NewType"));
        assert!(d.types.removed.is_empty());
    }

    #[test]
    fn diff_detects_removed_types() {
        let mut old = Manifest::default();
        old.types.insert(
            "OldType".into(),
            TypeDef {
                name: "OldType".into(),
                source: "src/lib.rs".into(),
                kind: TypeKind::Enum,
                ..Default::default()
            },
        );
        let new = Manifest::default();

        let d = diff(&old, &new);
        assert!(!d.is_empty());
        assert!(d.types.removed.contains_key("OldType"));
        assert!(d.types.added.is_empty());
    }

    #[test]
    fn diff_detects_changed_types() {
        let mut old = Manifest::default();
        old.types.insert(
            "User".into(),
            TypeDef {
                name: "User".into(),
                source: "src/models.rs".into(),
                kind: TypeKind::Struct,
                fields: vec![Field {
                    name: "name".into(),
                    type_name: "String".into(),
                    optional: false,
                }],
                ..Default::default()
            },
        );

        let mut new = Manifest::default();
        new.types.insert(
            "User".into(),
            TypeDef {
                name: "User".into(),
                source: "src/models.rs".into(),
                kind: TypeKind::Struct,
                fields: vec![
                    Field {
                        name: "name".into(),
                        type_name: "String".into(),
                        optional: false,
                    },
                    Field {
                        name: "email".into(),
                        type_name: "String".into(),
                        optional: true,
                    },
                ],
                ..Default::default()
            },
        );

        let d = diff(&old, &new);
        assert!(!d.is_empty());
        assert!(d.types.changed.contains_key("User"));
    }

    #[test]
    fn diff_display_no_changes() {
        let d = crate::diff::ManifestDiff::default();
        assert_eq!(format!("{d}"), "No changes");
    }

    // -- Schema round-trip ---------------------------------------------------

    #[test]
    fn manifest_yaml_round_trip() {
        let mut types = BTreeMap::new();
        types.insert(
            "User".into(),
            TypeDef {
                name: "User".into(),
                source: "src/models.rs".into(),
                kind: TypeKind::Struct,
                fields: vec![Field {
                    name: "id".into(),
                    type_name: "u64".into(),
                    optional: false,
                }],
                visibility: Visibility::Public,
                ..Default::default()
            },
        );

        let manifest = Manifest {
            project: ProjectMeta {
                name: "myproject".into(),
                kind: "Rust".into(),
                generated_at: "2026-01-01T00:00:00Z".into(),
                uu_version: "0.1.0".into(),
                ..Default::default()
            },
            types,
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&manifest).expect("serialize to YAML");
        let roundtripped: Manifest = serde_yaml::from_str(&yaml).expect("deserialize from YAML");

        // `name` fields are skip_serializing, so they deserialize as empty.
        // Verify everything else survives the round trip.
        assert_eq!(roundtripped.project, manifest.project);
        assert_eq!(roundtripped.types["User"].source, "src/models.rs");
        assert_eq!(roundtripped.types["User"].kind, TypeKind::Struct);
        assert_eq!(roundtripped.types["User"].fields.len(), 1);
        assert_eq!(roundtripped.types["User"].fields[0].name, "id");
        assert_eq!(roundtripped.types["User"].visibility, Visibility::Public);
        // name is omitted during serialization, so it defaults to empty
        assert_eq!(roundtripped.types["User"].name, "");
    }

    #[test]
    fn manifest_serializes_enum_variants() {
        let mut types = BTreeMap::new();
        types.insert(
            "Status".into(),
            TypeDef {
                name: "Status".into(),
                kind: TypeKind::Enum,
                variants: vec!["Active".into(), "Inactive".into()],
                ..Default::default()
            },
        );

        let manifest = Manifest {
            project: ProjectMeta {
                name: "test".into(),
                kind: "Rust".into(),
                ..Default::default()
            },
            types,
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&manifest).expect("serialize");
        assert!(yaml.contains("variants:"));
        assert!(!yaml.contains("fields:"));
    }

    #[test]
    fn manifest_skips_empty_fields_in_yaml() {
        let manifest = Manifest {
            project: ProjectMeta {
                name: "test".into(),
                kind: "Rust".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&manifest).expect("serialize");
        // Empty collections should not appear in output
        assert!(!yaml.contains("types:"));
        assert!(!yaml.contains("functions:"));
        assert!(!yaml.contains("routes:"));
        assert!(!yaml.contains("components:"));
    }

    // -- Context tests -------------------------------------------------------

    #[test]
    fn context_build_from_tempdir() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let kind = uu_detect::detect(dir.path()).unwrap();
        let ctx = ProjectContext::build(dir.path(), &kind).unwrap();

        assert_eq!(ctx.kind, uu_detect::ProjectKind::Cargo);
        assert!(ctx.cargo_toml.is_some());
        assert!(ctx.package_json.is_none());
        assert!(ctx.go_mod.is_none());
        // Should find at least Cargo.toml and src/main.rs
        assert!(ctx.files.len() >= 2);
    }

    #[test]
    fn context_parses_package_json() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test-app", "version": "1.0.0"}"#,
        )
        .unwrap();

        let kind = uu_detect::detect(dir.path()).unwrap();
        let ctx = ProjectContext::build(dir.path(), &kind).unwrap();

        let pkg = ctx.package_json.as_ref().unwrap();
        assert_eq!(pkg["name"], "test-app");
    }
}
