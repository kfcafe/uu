//! `uu install` — detect project type and run the install command.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use project_detect::{KotlinBuild, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

fn cargo_install_path() -> &'static str {
    let manifest = fs::read_to_string("Cargo.toml").unwrap_or_default();
    if manifest.contains("[workspace]") && Path::new("crates/uu/Cargo.toml").is_file() {
        "crates/uu"
    } else {
        "."
    }
}

/// Generate install steps for a detected project.
fn steps(kind: &ProjectKind) -> Vec<Step> {
    match kind {
        ProjectKind::Cargo => vec![step("cargo", &["install", "--path", cargo_install_path()])],
        ProjectKind::Go => vec![step("go", &["install", "./..."])],
        ProjectKind::Elixir { escript: true } => {
            vec![step("mix", &["deps.get"]), step("mix", &["escript.build"])]
        }
        ProjectKind::Elixir { escript: false } => {
            vec![step("mix", &["deps.get"]), step("mix", &["compile"])]
        }
        ProjectKind::Python { uv: true } => {
            if python_has_scripts() {
                vec![step("uv", &["tool", "install", "--force", "."])]
            } else {
                vec![step("uv", &["pip", "install", "."])]
            }
        }
        ProjectKind::Python { uv: false } => vec![step("pip", &["install", "."])],
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            vec![step(cmd, &["install"])]
        }
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: true },
        } => vec![step("./gradlew", &["build"])],
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: false },
        } => vec![step("gradle", &["build"])],
        ProjectKind::Kotlin {
            build: KotlinBuild::Maven,
        } => vec![step("mvn", &["install"])],
        ProjectKind::Gradle { wrapper: true } => vec![step("./gradlew", &["build"])],
        ProjectKind::Gradle { wrapper: false } => vec![step("gradle", &["build"])],
        ProjectKind::Maven => vec![step("mvn", &["install"])],
        ProjectKind::Ruby => vec![step("bundle", &["install"])],
        ProjectKind::Swift => vec![step("swift", &["build", "-c", "release"])],
        ProjectKind::Xcode { .. } => {
            vec![step("xcodebuild", &["-configuration", "Release", "build"])]
        }
        ProjectKind::DotNet { .. } => vec![step("dotnet", &["publish", "-c", "Release"])],
        ProjectKind::Meson => vec![
            step("meson", &["setup", "builddir"]),
            step("meson", &["compile", "-C", "builddir"]),
            step("meson", &["install", "-C", "builddir"]),
        ],
        ProjectKind::CMake => vec![
            step("cmake", &["-B", "build"]),
            step("cmake", &["--build", "build"]),
            step("cmake", &["--install", "build"]),
        ],
        ProjectKind::Zig => vec![step("zig", &["build", "-Doptimize=ReleaseSafe"])],
        ProjectKind::Make => vec![step("make", &[]), step("make", &["install"])],
        ProjectKind::Php => vec![step("composer", &["install"])],
        ProjectKind::Dart { flutter: true } => vec![step("flutter", &["pub", "get"])],
        ProjectKind::Dart { flutter: false } => vec![step("dart", &["pub", "get"])],
        ProjectKind::Sbt => vec![step("sbt", &["package"])],
        ProjectKind::Haskell { stack: true } => vec![step("stack", &["install"])],
        ProjectKind::Haskell { stack: false } => vec![step("cabal", &["install"])],
        ProjectKind::Clojure { lein: true } => vec![step("lein", &["install"])],
        ProjectKind::Clojure { lein: false } => vec![step("clj", &["-T:build", "install"])],
        ProjectKind::Rebar => vec![step("rebar3", &["get-deps"]), step("rebar3", &["compile"])],
        ProjectKind::Dune => vec![step("dune", &["build"]), step("dune", &["install"])],
        ProjectKind::Perl => vec![step("cpanm", &["--installdeps", "."])],
        ProjectKind::Julia => vec![step(
            "julia",
            &["--project", "-e", "using Pkg; Pkg.instantiate()"],
        )],
        ProjectKind::R { .. } => vec![step("R", &["CMD", "INSTALL", "."])],
        ProjectKind::Nim => vec![step("nimble", &["install"])],
        ProjectKind::Crystal => vec![step("shards", &["install"])],
        ProjectKind::Vlang => vec![step("v", &["install", "."])],
        ProjectKind::Gleam => vec![step("gleam", &["deps", "download"])],
        ProjectKind::Lua => vec![step("luarocks", &["install", "--deps-only", "."])],
        ProjectKind::Bazel => vec![step("bazel", &["build", "//..."])],
    }
}

/// Check whether pyproject.toml defines `[project.scripts]` — i.e. the project
/// ships CLI entry points that should be installed onto PATH.
fn python_has_scripts() -> bool {
    let Ok(content) = fs::read_to_string("pyproject.toml") else {
        return false;
    };
    content.contains("[project.scripts]")
}

fn post_install_steps(make_default: bool) -> Vec<Step> {
    if !make_default {
        return Vec::new();
    }

    let direct_hook = Path::new("tools/uu-post-install");
    if direct_hook.is_file() {
        return vec![step("./tools/uu-post-install", &[])];
    }

    let shell_hook = Path::new("tools/uu-post-install.sh");
    if shell_hook.is_file() {
        return vec![step("bash", &["tools/uu-post-install.sh"])];
    }

    Vec::new()
}

fn parse_toml_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if !trimmed.starts_with('"') {
        return None;
    }

    let remainder = &trimmed[1..];
    let end = remainder.find('"')?;
    Some(remainder[..end].to_string())
}

fn cargo_binary_names() -> Result<Vec<String>> {
    let manifest = fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?;
    let mut package_name = None;
    let mut bin_names = Vec::new();
    let mut section = "";

    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') {
            section = trimmed;
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }

        let Some(name) = parse_toml_string(value) else {
            continue;
        };

        match section {
            "[package]" => {
                if package_name.is_none() {
                    package_name = Some(name);
                }
            }
            "[[bin]]" => bin_names.push(name),
            _ => {}
        }
    }

    if !bin_names.is_empty() {
        Ok(bin_names)
    } else if let Some(package_name) = package_name {
        Ok(vec![package_name])
    } else {
        Ok(Vec::new())
    }
}

fn split_path_entries(path: Option<OsString>) -> Vec<PathBuf> {
    path.as_deref()
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect()
}

fn find_command_on_path(command: &str, path: Option<OsString>) -> Option<PathBuf> {
    split_path_entries(path)
        .into_iter()
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

fn cargo_home_dir(home: &Path) -> PathBuf {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cargo"))
}

fn preferred_user_bin_dir(home: &Path, cargo_home: &Path, path: Option<OsString>) -> PathBuf {
    let entries = split_path_entries(path);
    for entry in entries {
        if entry == home.join("bin")
            || entry == home.join(".local/bin")
            || entry == cargo_home.join("bin")
        {
            return entry;
        }
    }

    cargo_home.join("bin")
}

fn resolve_default_destination(
    command: &str,
    home: &Path,
    cargo_home: &Path,
    path: Option<OsString>,
) -> PathBuf {
    let active = find_command_on_path(command, path.clone());
    if let Some(active) = active {
        if active.starts_with(home) {
            return active;
        }
    }

    preferred_user_bin_dir(home, cargo_home, path).join(command)
}

fn install_binary_to(source: &Path, dest: &Path) -> Result<()> {
    let parent = dest.parent().context("default destination has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    if source == dest {
        return Ok(());
    }

    let temp = dest.with_extension("tmp");
    fs::copy(source, &temp)
        .with_context(|| format!("failed to copy {} to {}", source.display(), temp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to chmod {}", temp.display()))?;
    }
    fs::rename(&temp, dest)
        .with_context(|| format!("failed to move {} to {}", temp.display(), dest.display()))?;
    Ok(())
}

fn apply_cargo_default(dry_run: bool) -> Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let cargo_home = cargo_home_dir(&home);
    let path_env = std::env::var_os("PATH");

    for bin in cargo_binary_names()? {
        let source = cargo_home.join("bin").join(&bin);
        let dest = resolve_default_destination(&bin, &home, &cargo_home, path_env.clone());

        if dry_run {
            eprintln!(
                "{} {} -> {}",
                runner::style("33", "would default"),
                bin,
                dest.display()
            );
            continue;
        }

        if !source.is_file() {
            anyhow::bail!("installed binary not found at {}", source.display());
        }

        install_binary_to(&source, &dest)?;
        let resolved = find_command_on_path(&bin, std::env::var_os("PATH"));
        match resolved {
            Some(path) if path == dest => eprintln!(
                "{} {} -> {}",
                runner::style("32", "defaulted"),
                bin,
                path.display()
            ),
            Some(path) => eprintln!(
                "{} {} installed to {}, but shell still resolves {}",
                runner::style("33", "warning"),
                bin,
                dest.display(),
                path.display()
            ),
            None => eprintln!(
                "{} {} installed to {}, but command is not on PATH",
                runner::style("33", "warning"),
                bin,
                dest.display()
            ),
        }
    }

    Ok(())
}

fn apply_default(kind: &ProjectKind, dry_run: bool) -> Result<()> {
    match kind {
        ProjectKind::Cargo => apply_cargo_default(dry_run),
        _ => Ok(()),
    }
}

pub(crate) fn execute(dry_run: bool, make_default: bool, extra_args: Vec<String>) -> Result<()> {
    let kind = runner::detect_project()?;
    let mut s = steps(&kind);
    runner::append_args(&mut s, &extra_args);
    // Node projects with a "bin" field should also link the binary onto PATH.
    if let ProjectKind::Node { manager } = &kind {
        let dir = std::env::current_dir()?;
        if project_detect::node_has_bin(&dir) {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            s.push(step(cmd, &["link"]));
        }
    }

    s.extend(post_install_steps(make_default));

    runner::run_steps(&kind, &s, dry_run)?;
    if make_default {
        apply_default(&kind, dry_run)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn install_adds_default_hook_when_present() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("tools")).unwrap();
        fs::write(dir.path().join("tools/uu-post-install.sh"), "#!/bin/sh\n").unwrap();

        let s = post_install_steps(true);

        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "bash");
        assert_eq!(s[0].args, ["tools/uu-post-install.sh"]);
    }

    #[test]
    fn default_hook_prefers_direct_executable() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("tools")).unwrap();
        fs::write(dir.path().join("tools/uu-post-install"), "#!/bin/sh\n").unwrap();
        fs::write(dir.path().join("tools/uu-post-install.sh"), "#!/bin/sh\n").unwrap();

        let s = post_install_steps(true);

        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "./tools/uu-post-install");
        assert!(s[0].args.is_empty());
    }

    #[test]
    fn install_skips_default_hook_without_default_flag() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("tools")).unwrap();
        fs::write(dir.path().join("tools/uu-post-install.sh"), "#!/bin/sh\n").unwrap();

        let s = post_install_steps(false);

        std::env::set_current_dir(original_cwd).unwrap();

        assert!(s.is_empty());
    }

    #[test]
    fn cargo_steps() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

        let s = steps(&ProjectKind::Cargo);
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["install", "--path", "."]);
    }

    #[test]
    fn cargo_workspace_steps_use_uu_crate_path() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/uu\"]",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("crates/uu")).unwrap();
        fs::write(
            dir.path().join("crates/uu/Cargo.toml"),
            "[package]\nname = \"univ-utils\"",
        )
        .unwrap();

        let s = steps(&ProjectKind::Cargo);
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["install", "--path", "crates/uu"]);
    }

    #[test]
    fn node_uses_detected_manager() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Pnpm,
        });
        assert_eq!(s[0].program, "pnpm");
    }

    #[test]
    fn zig_install() {
        let s = steps(&ProjectKind::Zig);
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build", "-Doptimize=ReleaseSafe"]);
    }

    #[test]
    fn cmake_has_three_phases() {
        let s = steps(&ProjectKind::CMake);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn swift_install() {
        let s = steps(&ProjectKind::Swift);
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["build", "-c", "release"]);
    }

    #[test]
    fn dotnet_install() {
        let s = steps(&ProjectKind::DotNet { sln: false });
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["publish", "-c", "Release"]);
    }

    #[test]
    fn python_prefers_uv() {
        let s = steps(&ProjectKind::Python { uv: true });
        assert_eq!(s[0].program, "uv");
    }
}
