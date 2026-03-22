//! `uu clean` — remove build artifacts and reclaim disk space.

use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use project_detect::ProjectKind;

use crate::runner::{self, style};

pub(crate) fn execute(dry_run: bool) -> Result<()> {
    let kind = runner::detect_project()?;
    let dir = env::current_dir().context("failed to read current directory")?;

    eprintln!(
        "{} {} \x1b[2m({})\x1b[0m",
        style("36", "detected"),
        kind.label(),
        kind.detected_file()
    );

    let mut total_freed: u64 = 0;

    // Run native clean command if the ecosystem has one
    if let Some((program, args)) = native_clean_cmd(&kind) {
        if dry_run {
            eprintln!("{} {program} {}", style("33", "would run"), args.join(" "));
        } else {
            eprintln!("{} {program} {}", style("32", "running"), args.join(" "));
            let status = Command::new(program)
                .args(&args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .with_context(|| {
                    format!("`{program}` not found — is it installed and in your PATH?")
                })?;

            if !status.success() {
                eprintln!(
                    "{} {program} exited with code {}",
                    style("33", "warning"),
                    status.code().unwrap_or(-1)
                );
            }
        }
    }

    // Remove known artifact directories
    for dirname in kind.artifact_dirs() {
        let path = dir.join(dirname);
        if !path.exists() {
            continue;
        }

        let size = dir_size(&path);
        total_freed += size;

        if dry_run {
            eprintln!(
                "{} {dirname}/ \x1b[2m({})\x1b[0m",
                style("33", "would rm"),
                human_size(size)
            );
        } else {
            eprintln!(
                "{} {dirname}/ \x1b[2m({})\x1b[0m",
                style("31", "removing"),
                human_size(size)
            );
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }

    if total_freed > 0 {
        let verb = if dry_run { "would free" } else { "freed" };
        eprintln!("{} {}", style("32", verb), human_size(total_freed));
    } else if kind.artifact_dirs().is_empty() {
        eprintln!(
            "{} no known artifact directories for {}",
            style("33", "skip"),
            kind.label()
        );
    } else {
        eprintln!("{} nothing to clean", style("32", "done ✓"));
    }

    Ok(())
}

/// Return the native clean command for ecosystems that have one.
fn native_clean_cmd(kind: &ProjectKind) -> Option<(&'static str, Vec<&'static str>)> {
    match kind {
        ProjectKind::Cargo => Some(("cargo", vec!["clean"])),
        ProjectKind::Go => Some(("go", vec!["clean"])),
        ProjectKind::Gradle { wrapper: true } => Some(("./gradlew", vec!["clean"])),
        ProjectKind::Gradle { wrapper: false } => Some(("gradle", vec!["clean"])),
        ProjectKind::Maven => Some(("mvn", vec!["clean"])),
        ProjectKind::Make => Some(("make", vec!["clean"])),
        ProjectKind::Swift => Some(("swift", vec!["package", "clean"])),
        ProjectKind::DotNet { .. } => Some(("dotnet", vec!["clean"])),
        ProjectKind::Sbt => Some(("sbt", vec!["clean"])),
        ProjectKind::Haskell { stack: true } => Some(("stack", vec!["clean"])),
        ProjectKind::Haskell { stack: false } => Some(("cabal", vec!["clean"])),
        ProjectKind::Dune => Some(("dune", vec!["clean"])),
        ProjectKind::Gleam => Some(("gleam", vec!["clean"])),
        ProjectKind::Bazel => Some(("bazel", vec!["clean"])),
        // These ecosystems don't have a clean command — we remove dirs directly
        _ => None,
    }
}

/// Recursively compute the total size of a directory in bytes.
fn dir_size(path: &Path) -> u64 {
    dir_size_inner(path).unwrap_or(0)
}

fn dir_size_inner(path: &Path) -> io::Result<u64> {
    let mut total: u64 = 0;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            if ft.is_dir() {
                total += dir_size_inner(&entry.path())?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

/// Format bytes as a human-readable size string.
fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_bytes() {
        assert_eq!(human_size(42), "42 B");
    }

    #[test]
    fn human_size_kilobytes() {
        assert_eq!(human_size(2048), "2.0 KB");
    }

    #[test]
    fn human_size_megabytes() {
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn human_size_gigabytes() {
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn native_clean_for_cargo() {
        let (cmd, args) = native_clean_cmd(&ProjectKind::Cargo).unwrap();
        assert_eq!(cmd, "cargo");
        assert_eq!(args, ["clean"]);
    }

    #[test]
    fn native_clean_for_swift() {
        let (cmd, args) = native_clean_cmd(&ProjectKind::Swift).unwrap();
        assert_eq!(cmd, "swift");
        assert_eq!(args, ["package", "clean"]);
    }

    #[test]
    fn native_clean_for_dotnet() {
        let (cmd, args) = native_clean_cmd(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(cmd, "dotnet");
        assert_eq!(args, ["clean"]);
    }

    #[test]
    fn no_native_clean_for_node() {
        let kind = ProjectKind::Node {
            manager: project_detect::NodePM::Npm,
        };
        assert!(native_clean_cmd(&kind).is_none());
    }

    #[test]
    fn dir_size_of_nonexistent_is_zero() {
        assert_eq!(dir_size(Path::new("/nonexistent/path")), 0);
    }

    #[test]
    fn dir_size_counts_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap(); // 5 bytes
        std::fs::write(dir.path().join("b.txt"), "world!").unwrap(); // 6 bytes
        assert_eq!(dir_size(dir.path()), 11);
    }
}
