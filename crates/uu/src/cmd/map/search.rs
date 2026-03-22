//! `uu map search <term>` — search across all symbols by name.

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use uu_manifest::schema::*;

use super::format::{cyan, dim, green, magenta, yellow};

#[derive(Args)]
pub(crate) struct SearchArgs {
    /// Search term (case-insensitive substring match)
    term: String,

    /// Project directory (default: current directory)
    #[arg(short = 'C', long = "dir")]
    path: Option<PathBuf>,

    /// Include private symbols and test functions
    #[arg(long)]
    all: bool,

    /// Filter by category: types, functions, routes, modules, models, endpoints
    #[arg(short, long)]
    category: Option<String>,
}

struct SearchResult {
    category: &'static str,
    name: String,
    detail: String,
    source: String,
}

pub(crate) fn execute(args: SearchArgs) -> Result<()> {
    let root = super::resolve_root(args.path.as_ref())?;
    let manifest = super::build_manifest(&root, args.all)?;
    let term = args.term.to_lowercase();
    let category = args.category.as_deref().map(str::to_lowercase);

    let mut results: Vec<SearchResult> = Vec::new();

    // Types
    if should_search(&category, "types") {
        for (key, t) in &manifest.types {
            if key.to_lowercase().contains(&term)
                || t.fields
                    .iter()
                    .any(|f| f.name.to_lowercase().contains(&term))
                || t.methods.iter().any(|m| m.to_lowercase().contains(&term))
                || t.variants.iter().any(|v| v.to_lowercase().contains(&term))
            {
                let kind = match t.kind {
                    TypeKind::Struct => "struct",
                    TypeKind::Class => "class",
                    TypeKind::Interface => "interface",
                    TypeKind::Enum => "enum",
                    TypeKind::Trait => "trait",
                    TypeKind::Protocol => "protocol",
                    TypeKind::Union => "union",
                    TypeKind::TypeAlias => "type alias",
                };
                let detail = if !t.fields.is_empty() {
                    format!("{kind}, {} fields", t.fields.len())
                } else if !t.variants.is_empty() {
                    format!("{kind}, {} variants", t.variants.len())
                } else if !t.methods.is_empty() {
                    format!("{kind}, {} methods", t.methods.len())
                } else {
                    kind.to_string()
                };
                results.push(SearchResult {
                    category: "type",
                    name: key.clone(),
                    detail,
                    source: t.source.clone(),
                });
            }
        }
    }

    // Functions
    if should_search(&category, "functions") {
        for (key, f) in &manifest.functions {
            if key.to_lowercase().contains(&term) || f.signature.to_lowercase().contains(&term) {
                let detail = if f.signature.is_empty() {
                    if f.is_async {
                        "async".to_string()
                    } else {
                        String::new()
                    }
                } else {
                    f.signature.clone()
                };
                results.push(SearchResult {
                    category: "fn",
                    name: key.clone(),
                    detail,
                    source: f.source.clone(),
                });
            }
        }
    }

    // Routes
    if should_search(&category, "routes") {
        for (key, r) in &manifest.routes {
            if key.to_lowercase().contains(&term)
                || r.path.to_lowercase().contains(&term)
                || r.handler
                    .as_deref()
                    .is_some_and(|h| h.to_lowercase().contains(&term))
            {
                let methods = if r.methods.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", r.methods.join(", "))
                };
                results.push(SearchResult {
                    category: "route",
                    name: if r.path.is_empty() {
                        key.clone()
                    } else {
                        r.path.clone()
                    },
                    detail: methods,
                    source: r.file.clone(),
                });
            }
        }
    }

    // Modules
    if should_search(&category, "modules") {
        for (key, m) in &manifest.modules {
            if key.to_lowercase().contains(&term)
                || m.exports.iter().any(|e| e.to_lowercase().contains(&term))
            {
                let detail = if m.exports.is_empty() {
                    String::new()
                } else {
                    format!("{} exports", m.exports.len())
                };
                results.push(SearchResult {
                    category: "module",
                    name: key.clone(),
                    detail,
                    source: m.file.clone(),
                });
            }
        }
    }

    // Models
    if should_search(&category, "models") {
        for (key, m) in &manifest.models {
            if key.to_lowercase().contains(&term)
                || m.fields
                    .iter()
                    .any(|f| f.name.to_lowercase().contains(&term))
            {
                let detail = if m.orm.is_empty() {
                    format!("{} fields", m.fields.len())
                } else {
                    format!("{}, {} fields", m.orm, m.fields.len())
                };
                results.push(SearchResult {
                    category: "model",
                    name: key.clone(),
                    detail,
                    source: m.source.clone(),
                });
            }
        }
    }

    // Endpoints
    if should_search(&category, "endpoints") {
        for (key, e) in &manifest.endpoints {
            if key.to_lowercase().contains(&term)
                || e.path.to_lowercase().contains(&term)
                || e.handler.to_lowercase().contains(&term)
            {
                results.push(SearchResult {
                    category: "endpoint",
                    name: if e.path.is_empty() {
                        key.clone()
                    } else {
                        e.path.clone()
                    },
                    detail: e.method.clone(),
                    source: e.file.clone(),
                });
            }
        }
    }

    // Display results
    if results.is_empty() {
        eprintln!("No matches for '{}'", args.term);
        std::process::exit(1);
    }

    println!("Found {} matches for '{}':\n", results.len(), args.term);

    // Group by category
    let mut current_category = "";
    for result in &results {
        if result.category != current_category {
            current_category = result.category;
            let label = match current_category {
                "type" => cyan("Types:"),
                "fn" => green("Functions:"),
                "route" => magenta("Routes:"),
                "module" => yellow("Modules:"),
                "model" => cyan("Models:"),
                "endpoint" => magenta("Endpoints:"),
                _ => current_category.to_string(),
            };
            println!("  {label}");
        }

        let source_info = if result.source.is_empty() {
            String::new()
        } else {
            dim(&format!("  {}", result.source))
        };
        let detail_info = if result.detail.is_empty() {
            String::new()
        } else {
            dim(&format!("  ({})", result.detail))
        };

        println!("    {:<30}{}{}", result.name, detail_info, source_info);
    }

    println!();

    Ok(())
}

fn should_search(filter: &Option<String>, category: &str) -> bool {
    match filter {
        None => true,
        Some(f) => {
            let f = f.to_lowercase();
            category.starts_with(&f)
                || f.starts_with(category)
                || (f == "fn" && category == "functions")
                || (f == "type" && category == "types")
                || (f == "mod" && category == "modules")
        }
    }
}
