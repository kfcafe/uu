//! `uu map` — generate and explore project manifests.
//!
//! Subcommands:
//!   (default)  Generate a project manifest
//!   query      Look up a specific type, function, or symbol
//!   search     Search across all symbols by name
//!   stats      Show codebase statistics and summary
//!   tree       Show module hierarchy as a tree

mod format;
mod generate;
mod query;
mod search;
mod stats;
mod tree;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use uu_manifest::Manifest;

// -- CLI structure -----------------------------------------------------------

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub(crate) struct MapArgs {
    #[command(subcommand)]
    command: Option<MapCommand>,

    /// Project directory (default: current directory)
    path: Option<PathBuf>,

    /// Output file (default: .map.yaml)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output format: yaml, json, md
    #[arg(short, long, default_value = "yaml")]
    format: String,

    /// Write to stdout instead of file
    #[arg(long)]
    stdout: bool,

    /// Show detected language and frameworks without generating manifest
    #[arg(long)]
    detect_only: bool,

    /// Show diff against existing manifest
    #[arg(long)]
    diff: bool,

    /// Only run specific adapters (comma-separated)
    #[arg(long)]
    adapters: Option<String>,

    /// Show what would be generated without writing
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Include private symbols and test functions
    #[arg(long)]
    all: bool,
}

#[derive(Subcommand)]
enum MapCommand {
    /// Look up a specific type, function, or symbol
    Query(query::QueryArgs),

    /// Search across all symbols by name
    Search(search::SearchArgs),

    /// Show codebase statistics and summary
    Stats(CommonArgs),

    /// Show module hierarchy as a tree
    Tree(CommonArgs),
}

#[derive(Args)]
pub(crate) struct CommonArgs {
    /// Project directory (default: current directory)
    #[arg(short = 'C', long = "dir")]
    path: Option<PathBuf>,

    /// Include private symbols and test functions
    #[arg(long)]
    all: bool,
}

// -- Dispatch ----------------------------------------------------------------

pub(crate) fn execute(args: MapArgs) -> Result<()> {
    match args.command {
        Some(MapCommand::Query(a)) => query::execute(a),
        Some(MapCommand::Search(a)) => search::execute(a),
        Some(MapCommand::Stats(a)) => stats::execute(a),
        Some(MapCommand::Tree(a)) => tree::execute(a),
        None => generate::execute(generate::GenerateArgs {
            path: args.path,
            output: args.output,
            format: args.format,
            stdout: args.stdout,
            detect_only: args.detect_only,
            diff: args.diff,
            adapters: args.adapters,
            dry_run: args.dry_run,
            all: args.all,
        }),
    }
}

// -- Shared helpers ----------------------------------------------------------

/// Resolve a project root directory from an optional CLI path.
fn resolve_root(path: Option<&PathBuf>) -> Result<PathBuf> {
    let root = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().context("cannot determine current directory")?,
    };
    root.canonicalize()
        .with_context(|| format!("cannot resolve path `{}`", root.display()))
}

/// Generate a manifest from a project directory.
fn build_manifest(root: &std::path::Path, include_all: bool) -> Result<Manifest> {
    let mut manifest = uu_manifest::generate(root)?;
    if !include_all {
        uu_manifest::filter_public(&mut manifest);
    }
    Ok(manifest)
}
