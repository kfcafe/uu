//! `uu map stats` — show codebase statistics and summary.

use anyhow::Result;
use std::collections::BTreeMap;

use uu_manifest::schema::*;

use super::format::{bold, cyan, dim, green, yellow};
use super::CommonArgs;

pub(crate) fn execute(args: CommonArgs) -> Result<()> {
    let root = super::resolve_root(args.path.as_ref())?;
    let manifest = super::build_manifest(&root, args.all)?;

    // Header
    println!(
        "\n{} {} {}",
        bold(&format!("Project: {}", manifest.project.name)),
        dim(&format!("({})", manifest.project.kind)),
        dim(&manifest.project.generated_at),
    );
    println!();

    // Summary counts
    let total_types = manifest.types.len();
    let total_fns = manifest.functions.len();
    let total_modules = manifest.modules.len();
    let total_routes = manifest.routes.len();
    let total_endpoints = manifest.endpoints.len();
    let total_models = manifest.models.len();
    let total_components = manifest.components.len();
    let total_integrations = manifest.integrations.len();

    println!("{}", bold("Summary"));
    println!("  {:<16} {}", cyan("Types:"), total_types);
    println!("  {:<16} {}", green("Functions:"), total_fns);
    println!("  {:<16} {}", yellow("Modules:"), total_modules);
    if total_routes > 0 {
        println!("  {:<16} {}", "Routes:", total_routes);
    }
    if total_endpoints > 0 {
        println!("  {:<16} {}", "Endpoints:", total_endpoints);
    }
    if total_models > 0 {
        println!("  {:<16} {}", "Models:", total_models);
    }
    if total_components > 0 {
        println!("  {:<16} {}", "Components:", total_components);
    }
    if total_integrations > 0 {
        println!("  {:<16} {}", "Integrations:", total_integrations);
    }
    println!();

    // Visibility breakdown
    if total_types > 0 || total_fns > 0 {
        let (pub_types, priv_types, int_types) = count_visibility_types(&manifest);
        let (pub_fns, priv_fns, int_fns) = count_visibility_fns(&manifest);
        let async_fns = manifest.functions.values().filter(|f| f.is_async).count();

        println!("{}", bold("Visibility"));
        if total_types > 0 {
            println!(
                "  Types:     {} public, {} internal, {} private",
                pub_types, int_types, priv_types
            );
        }
        if total_fns > 0 {
            println!(
                "  Functions: {} public, {} internal, {} private, {} async",
                pub_fns, int_fns, priv_fns, async_fns
            );
        }
        println!();
    }

    // Type breakdown by kind
    if total_types > 0 {
        let mut kind_counts: BTreeMap<&str, usize> = BTreeMap::new();
        for t in manifest.types.values() {
            let kind = match t.kind {
                TypeKind::Struct => "Structs",
                TypeKind::Class => "Classes",
                TypeKind::Interface => "Interfaces",
                TypeKind::Enum => "Enums",
                TypeKind::Trait => "Traits",
                TypeKind::Protocol => "Protocols",
                TypeKind::Union => "Unions",
                TypeKind::TypeAlias => "Type aliases",
            };
            *kind_counts.entry(kind).or_default() += 1;
        }

        println!("{}", bold("Type breakdown"));
        for (kind, count) in &kind_counts {
            println!("  {kind:<16} {count}");
        }
        println!();
    }

    // Top modules by symbol count
    if total_modules > 0 {
        let mut module_stats: Vec<(&str, usize, usize)> = Vec::new();

        for (mod_name, module) in &manifest.modules {
            let file_prefix = &module.file;

            let type_count = if file_prefix.is_empty() {
                0
            } else {
                manifest
                    .types
                    .values()
                    .filter(|t| !t.source.is_empty() && t.source.starts_with(file_prefix.as_str()))
                    .count()
            };

            let fn_count = if file_prefix.is_empty() {
                0
            } else {
                manifest
                    .functions
                    .values()
                    .filter(|f| !f.source.is_empty() && f.source.starts_with(file_prefix.as_str()))
                    .count()
            };

            if type_count > 0 || fn_count > 0 {
                module_stats.push((mod_name, type_count, fn_count));
            }
        }

        module_stats.sort_by_key(|(_, types, fns)| std::cmp::Reverse(*types + *fns));

        if !module_stats.is_empty() {
            println!("{}", bold("Top modules by symbol count"));
            for (name, types, fns) in module_stats.iter().take(10) {
                let mut parts = Vec::new();
                if *types > 0 {
                    parts.push(format!("{types} types"));
                }
                if *fns > 0 {
                    parts.push(format!("{fns} functions"));
                }
                println!("  {:<40} {}", name, dim(&parts.join(", ")));
            }
            println!();
        }
    }

    // Traits + implementors
    let traits: Vec<_> = manifest
        .types
        .iter()
        .filter(|(_, t)| matches!(t.kind, TypeKind::Trait))
        .collect();
    if !traits.is_empty() {
        println!("{}", bold("Traits"));
        for (name, _) in &traits {
            let implementors: Vec<_> = manifest
                .types
                .iter()
                .filter(|(_, t)| t.implements.iter().any(|i| i == *name))
                .map(|(k, _)| k.as_str())
                .collect();
            if implementors.is_empty() {
                println!("  {name} {}", dim("(no implementors)"));
            } else {
                println!("  {name} {} {}", dim("→"), implementors.join(", "));
            }
        }
        println!();
    }

    // Auth summary
    if let Some(auth) = &manifest.auth {
        println!("{}", bold("Auth"));
        if !auth.strategy.is_empty() {
            println!("  Strategy: {}", auth.strategy);
        }
        if !auth.providers.is_empty() {
            println!("  Providers: {}", auth.providers.join(", "));
        }
        println!();
    }

    Ok(())
}

fn count_visibility_types(manifest: &uu_manifest::Manifest) -> (usize, usize, usize) {
    let mut pub_count = 0;
    let mut priv_count = 0;
    let mut int_count = 0;
    for t in manifest.types.values() {
        match t.visibility {
            Visibility::Public => pub_count += 1,
            Visibility::Private => priv_count += 1,
            Visibility::Internal => int_count += 1,
        }
    }
    (pub_count, priv_count, int_count)
}

fn count_visibility_fns(manifest: &uu_manifest::Manifest) -> (usize, usize, usize) {
    let mut pub_count = 0;
    let mut priv_count = 0;
    let mut int_count = 0;
    for f in manifest.functions.values() {
        match f.visibility {
            Visibility::Public => pub_count += 1,
            Visibility::Private => priv_count += 1,
            Visibility::Internal => int_count += 1,
        }
    }
    (pub_count, priv_count, int_count)
}
