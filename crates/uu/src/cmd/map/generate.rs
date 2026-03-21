//! `uu map` default behavior — generate a project manifest.

use anyhow::{Context, Result};
use std::path::PathBuf;

use uu_manifest::adapters::{all_adapters, AdapterLayer};
use uu_manifest::context::ProjectContext;

use super::format;

/// Arguments for the generate subcommand (and the default behavior).
pub(crate) struct GenerateArgs {
    pub path: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub format: String,
    pub stdout: bool,
    pub detect_only: bool,
    pub diff: bool,
    #[allow(dead_code)]
    pub adapters: Option<String>,
    pub dry_run: bool,
    pub all: bool,
}

pub(crate) fn execute(args: GenerateArgs) -> Result<()> {
    let root = super::resolve_root(args.path.as_ref())?;

    let kind = uu_detect::detect(&root)
        .ok_or_else(|| anyhow::anyhow!("no recognized project found in {}", root.display()))?;

    if args.detect_only {
        let ctx = ProjectContext::build(&root, &kind)?;
        eprintln!("Project: {} ({})", kind.label(), root.display());
        let all = all_adapters();
        let matching: Vec<_> = all.iter().filter(|a| a.detect(&ctx)).collect();
        if matching.is_empty() {
            eprintln!("No matching adapters");
        } else {
            eprintln!("Matching adapters:");
            for adapter in &matching {
                let layer = match adapter.layer() {
                    AdapterLayer::Language => "Language",
                    AdapterLayer::Framework => "Framework",
                };
                eprintln!("  {} ({})", adapter.name(), layer);
            }
        }
        return Ok(());
    }

    let mut manifest = uu_manifest::generate(&root)?;
    if !args.all {
        uu_manifest::filter_public(&mut manifest);
    }

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| root.join(".map.yaml"));

    let (serialized, file_ext) = match args.format.as_str() {
        "json" => (
            serde_json::to_string_pretty(&manifest)
                .context("failed to serialize manifest as JSON")?,
            "json",
        ),
        "md" | "markdown" => (format::to_markdown(&manifest), "md"),
        _ => (
            serde_yaml::to_string(&manifest).context("failed to serialize manifest as YAML")?,
            "yaml",
        ),
    };

    if args.diff {
        if output_path.exists() {
            let existing_content = std::fs::read_to_string(&output_path)
                .context("failed to read existing manifest")?;
            let existing: uu_manifest::Manifest = serde_yaml::from_str(&existing_content)
                .context("failed to parse existing manifest")?;
            let d = uu_manifest::diff::diff(&existing, &manifest);
            if d.is_empty() {
                eprintln!("No changes");
            } else {
                eprintln!("{d}");
            }
        } else {
            eprintln!("No existing manifest at {}", output_path.display());
        }
        return Ok(());
    }

    if args.dry_run {
        eprintln!("Would write {} to {}", args.format, output_path.display());
        eprintln!(
            "  {} types, {} functions, {} routes",
            manifest.types.len(),
            manifest.functions.len(),
            manifest.routes.len()
        );
        return Ok(());
    }

    if args.stdout {
        print!("{serialized}");
    } else {
        // Adjust output path extension if format differs from default
        let output_path = if args.output.is_none() && file_ext != "yaml" {
            root.join(format!(".map.{file_ext}"))
        } else {
            output_path
        };
        std::fs::write(&output_path, &serialized)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
        eprintln!(
            "Generated {} ({} types, {} functions, {} routes)",
            output_path.display(),
            manifest.types.len(),
            manifest.functions.len(),
            manifest.routes.len()
        );
    }

    Ok(())
}
