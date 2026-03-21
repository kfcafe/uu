//! `uu map query <name>` — look up a specific symbol by name.

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use super::format::{
    bold, dim, find_references, find_similar, print_function, print_model, print_module,
    print_route, print_type,
};

#[derive(Args)]
pub(crate) struct QueryArgs {
    /// Symbol name to look up (type, function, module, route)
    name: String,

    /// Project directory (default: current directory)
    #[arg(short = 'C', long = "dir")]
    path: Option<PathBuf>,

    /// Include private symbols and test functions
    #[arg(long)]
    all: bool,

    /// Show cross-references (what uses this symbol)
    #[arg(short, long)]
    refs: bool,
}

pub(crate) fn execute(args: QueryArgs) -> Result<()> {
    let root = super::resolve_root(args.path.as_ref())?;
    let manifest = super::build_manifest(&root, args.all)?;
    let name = &args.name;
    let name_lower = name.to_lowercase();

    let mut found = false;

    // Search types — exact match or suffix match (e.g., "User" matches "models::User")
    for (key, typedef) in &manifest.types {
        let key_lower = key.to_lowercase();
        if key_lower == name_lower
            || key_lower.ends_with(&format!("::{name_lower}"))
            || key_lower
                .rsplit("::")
                .next()
                .is_some_and(|last| last == name_lower)
        {
            found = true;
            print_type(key, typedef);

            if args.refs {
                let refs = find_references(key, &manifest);
                if !refs.is_empty() {
                    println!("  {}:", dim("referenced by"));
                    for r in &refs {
                        println!("    {r}");
                    }
                    println!();
                }
            }
        }
    }

    // Search functions
    for (key, func) in &manifest.functions {
        let key_lower = key.to_lowercase();
        if key_lower == name_lower
            || key_lower.ends_with(&format!("::{name_lower}"))
            || key_lower
                .rsplit("::")
                .next()
                .is_some_and(|last| last == name_lower)
        {
            found = true;
            print_function(key, func);
        }
    }

    // Search routes
    for (key, route) in &manifest.routes {
        if key.to_lowercase().contains(&name_lower) {
            found = true;
            print_route(key, route);
        }
    }

    // Search modules
    for (key, module) in &manifest.modules {
        let key_lower = key.to_lowercase();
        if key_lower == name_lower
            || key_lower.ends_with(&format!("::{name_lower}"))
            || key_lower
                .rsplit("::")
                .next()
                .is_some_and(|last| last == name_lower)
        {
            found = true;
            print_module(key, module);
        }
    }

    // Search models
    for (key, model) in &manifest.models {
        if key.to_lowercase() == name_lower {
            found = true;
            print_model(key, model);
        }
    }

    // Search endpoints
    for (key, endpoint) in &manifest.endpoints {
        if key.to_lowercase().contains(&name_lower) {
            found = true;
            println!(
                "{} {} {}",
                bold(&format!("endpoint {key}")),
                endpoint.method,
                endpoint.path
            );
            if !endpoint.file.is_empty() {
                println!("  {} {}", dim("file:"), endpoint.file);
            }
            if !endpoint.handler.is_empty() {
                println!("  {} {}", dim("handler:"), endpoint.handler);
            }
            println!();
        }
    }

    if !found {
        eprintln!("No symbol found matching '{name}'");
        let suggestions = find_similar(name, &manifest);
        if !suggestions.is_empty() {
            eprintln!("Did you mean:");
            for s in suggestions.iter().take(5) {
                eprintln!("  {s}");
            }
        }
        std::process::exit(1);
    }

    Ok(())
}
