//! `uu install` — detect project type and run the install command.

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use project_detect::{detect, KotlinBuild, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

#[derive(Debug)]
struct InstallTarget {
    kind: ProjectKind,
    dir: PathBuf,
}

#[derive(Debug)]
struct CargoInstallTarget {
    path_arg: String,
}

fn parse_toml_string_array(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_string = false;

    for ch in value.chars() {
        match ch {
            '"' if in_string => {
                items.push(current.clone());
                current.clear();
                in_string = false;
            }
            '"' => in_string = true,
            _ if in_string => current.push(ch),
            _ => {}
        }
    }

    items
}

fn workspace_members(manifest: &str) -> Vec<String> {
    let mut in_workspace = false;
    let mut collecting = false;
    let mut members = String::new();

    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') {
            if collecting {
                break;
            }
            in_workspace = trimmed == "[workspace]";
            continue;
        }

        if !in_workspace {
            continue;
        }

        if collecting {
            members.push_str(trimmed);
            if trimmed.contains(']') {
                break;
            }
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "members" {
            continue;
        }

        members.push_str(value.trim());
        if !value.contains(']') {
            collecting = true;
        }
        if value.contains(']') {
            break;
        }
    }

    parse_toml_string_array(&members)
}

fn expand_workspace_member(root: &Path, member: &str) -> Vec<PathBuf> {
    if let Some(prefix) = member.strip_suffix("/*") {
        let base = root.join(prefix);
        return fs::read_dir(base)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.join("Cargo.toml").is_file())
            .collect();
    }

    let path = root.join(member);
    if path.join("Cargo.toml").is_file() {
        vec![path]
    } else {
        Vec::new()
    }
}

fn cargo_package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() == "name" {
            return parse_toml_string(value);
        }
    }
    None
}

fn is_installable_cargo_package(dir: &Path) -> bool {
    let manifest_path = dir.join("Cargo.toml");
    let Ok(manifest) = fs::read_to_string(&manifest_path) else {
        return false;
    };
    if !manifest.contains("[package]") {
        return false;
    }

    manifest.contains("[[bin]]")
        || dir.join("src/main.rs").is_file()
        || fs::read_dir(dir.join("src/bin"))
            .map(|mut entries| entries.any(|entry| entry.is_ok()))
            .unwrap_or(false)
}

fn cargo_install_target_in(dir: &Path) -> Result<CargoInstallTarget> {
    let manifest = fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
    if manifest.contains("[package]") || !manifest.contains("[workspace]") {
        return Ok(CargoInstallTarget {
            path_arg: ".".to_owned(),
        });
    }

    let members = workspace_members(&manifest);
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();

    for member in members {
        for path in expand_workspace_member(dir, &member) {
            if !seen.insert(path.clone()) || !is_installable_cargo_package(&path) {
                continue;
            }
            candidates.push(path);
        }
    }

    match candidates.as_slice() {
        [only] => Ok(CargoInstallTarget {
            path_arg: only
                .strip_prefix(dir)
                .unwrap_or(only)
                .display()
                .to_string(),
        }),
        [] => anyhow::bail!(
            "Cargo workspace root is not directly installable and no installable workspace member was found"
        ),
        many => {
            let names = many
                .iter()
                .map(|path| {
                    let manifest = fs::read_to_string(path.join("Cargo.toml")).unwrap_or_default();
                    cargo_package_name(&manifest).unwrap_or_else(|| path.display().to_string())
                })
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "Cargo workspace root is not directly installable and has multiple installable members: {names}. Run from the member directory or use `cargo install --path <member>`"
            )
        }
    }
}

fn cargo_install_target() -> Result<CargoInstallTarget> {
    let dir = std::env::current_dir().context("failed to read current directory")?;
    cargo_install_target_in(&dir)
}

fn collect_install_targets(root: &Path, targets: &mut Vec<InstallTarget>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", root.display()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.') || name == "target" || name == "node_modules")
        {
            continue;
        }
        match detect(&path) {
            Some(kind) if !matches!(kind, ProjectKind::Make) => {
                targets.push(InstallTarget { kind, dir: path });
            }
            _ => collect_install_targets(&path, targets)?,
        }
    }
    Ok(())
}

fn make_install_targets(root: &Path) -> Result<Vec<InstallTarget>> {
    let mut targets = Vec::new();
    collect_install_targets(root, &mut targets)?;
    targets.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(targets)
}

fn install_targets_for(kind: ProjectKind, dir: PathBuf) -> Result<Vec<InstallTarget>> {
    if matches!(kind, ProjectKind::Make) {
        let targets = make_install_targets(&dir)?;
        if !targets.is_empty() {
            return Ok(targets);
        }
    }
    Ok(vec![InstallTarget { kind, dir }])
}

/// Generate install steps for a detected project.
fn steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Cargo => {
            let target = cargo_install_target()?;
            Ok(vec![step(
                "cargo",
                &["install", "--path", &target.path_arg],
            )])
        }
        ProjectKind::Go => Ok(vec![step("go", &["install", "./..."])]),
        ProjectKind::Elixir { escript: true } => Ok(vec![
            step("mix", &["deps.get"]),
            step("mix", &["escript.build"]),
        ]),
        ProjectKind::Elixir { escript: false } => {
            Ok(vec![step("mix", &["deps.get"]), step("mix", &["compile"])])
        }
        ProjectKind::Python { uv: true } => {
            if python_has_scripts() {
                Ok(vec![step("uv", &["tool", "install", "--force", "."])])
            } else {
                Ok(vec![step("uv", &["pip", "install", "."])])
            }
        }
        ProjectKind::Python { uv: false } => Ok(vec![step("pip", &["install", "."])]),
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            Ok(vec![step(cmd, &["install"])])
        }
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: true },
        } => Ok(vec![step("./gradlew", &["build"])]),
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: false },
        } => Ok(vec![step("gradle", &["build"])]),
        ProjectKind::Kotlin {
            build: KotlinBuild::Maven,
        } => Ok(vec![step("mvn", &["install"])]),
        ProjectKind::Gradle { wrapper: true } => Ok(vec![step("./gradlew", &["build"])]),
        ProjectKind::Gradle { wrapper: false } => Ok(vec![step("gradle", &["build"])]),
        ProjectKind::Maven => Ok(vec![step("mvn", &["install"])]),
        ProjectKind::Ruby => Ok(vec![step("bundle", &["install"])]),
        ProjectKind::Swift => Ok(vec![step("swift", &["build", "-c", "release"])]),
        ProjectKind::Xcode { .. } => Ok(vec![step(
            "xcodebuild",
            &["-configuration", "Release", "build"],
        )]),
        ProjectKind::DotNet { .. } => Ok(vec![step("dotnet", &["publish", "-c", "Release"])]),
        ProjectKind::Meson => Ok(vec![
            step("meson", &["setup", "builddir"]),
            step("meson", &["compile", "-C", "builddir"]),
            step("meson", &["install", "-C", "builddir"]),
        ]),
        ProjectKind::CMake => Ok(vec![
            step("cmake", &["-B", "build"]),
            step("cmake", &["--build", "build"]),
            step("cmake", &["--install", "build"]),
        ]),
        ProjectKind::Zig => Ok(vec![step("zig", &["build", "-Doptimize=ReleaseSafe"])]),
        ProjectKind::Make => Ok(vec![step("make", &[]), step("make", &["install"])]),
        ProjectKind::Php => Ok(vec![step("composer", &["install"])]),
        ProjectKind::Dart { flutter: true } => Ok(vec![step("flutter", &["pub", "get"])]),
        ProjectKind::Dart { flutter: false } => Ok(vec![step("dart", &["pub", "get"])]),
        ProjectKind::Sbt => Ok(vec![step("sbt", &["package"])]),
        ProjectKind::Haskell { stack: true } => Ok(vec![step("stack", &["install"])]),
        ProjectKind::Haskell { stack: false } => Ok(vec![step("cabal", &["install"])]),
        ProjectKind::Clojure { lein: true } => Ok(vec![step("lein", &["install"])]),
        ProjectKind::Clojure { lein: false } => Ok(vec![step("clj", &["-T:build", "install"])]),
        ProjectKind::Rebar => Ok(vec![
            step("rebar3", &["get-deps"]),
            step("rebar3", &["compile"]),
        ]),
        ProjectKind::Dune => Ok(vec![step("dune", &["build"]), step("dune", &["install"])]),
        ProjectKind::Perl => Ok(vec![step("cpanm", &["--installdeps", "."])]),
        ProjectKind::Julia => Ok(vec![step(
            "julia",
            &["--project", "-e", "using Pkg; Pkg.instantiate()"],
        )]),
        ProjectKind::R { .. } => Ok(vec![step("R", &["CMD", "INSTALL", "."])]),
        ProjectKind::Nim => Ok(vec![step("nimble", &["install"])]),
        ProjectKind::Crystal => Ok(vec![step("shards", &["install"])]),
        ProjectKind::Vlang => Ok(vec![step("v", &["install", "."])]),
        ProjectKind::Gleam => Ok(vec![step("gleam", &["deps", "download"])]),
        ProjectKind::Lua => Ok(vec![step("luarocks", &["install", "--deps-only", "."])]),
        ProjectKind::Bazel => Ok(vec![step("bazel", &["build", "//..."])]),
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

fn parse_json_string(value: &str) -> Option<String> {
    let value = value.trim_start();
    if !value.starts_with('"') {
        return None;
    }

    let mut escaped = false;
    let mut result = String::new();
    for ch in value[1..].chars() {
        if escaped {
            result.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(result),
            _ => result.push(ch),
        }
    }

    None
}

fn parse_json_string_field(content: &str, field: &str) -> Option<String> {
    let marker = format!("\"{field}\"");
    let start = content.find(&marker)?;
    let after_marker = &content[start + marker.len()..];
    let colon = after_marker.find(':')?;
    parse_json_string(after_marker[colon + 1..].trim_start())
}

fn unscoped_package_name(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).to_owned()
}

fn node_bin_names() -> Vec<String> {
    let Ok(content) = fs::read_to_string("package.json") else {
        return Vec::new();
    };

    let Some(bin_start) = content.find("\"bin\"") else {
        return Vec::new();
    };
    let after_bin = &content[bin_start + "\"bin\"".len()..];
    let Some(colon) = after_bin.find(':') else {
        return Vec::new();
    };
    let value = after_bin[colon + 1..].trim_start();

    if value.starts_with('"') {
        return parse_json_string_field(&content, "name")
            .map(|name| vec![unscoped_package_name(&name)])
            .unwrap_or_default();
    }

    if !value.starts_with('{') {
        return Vec::new();
    }

    let Some(end) = value.find('}') else {
        return Vec::new();
    };
    let object = &value[1..end];
    let mut names = Vec::new();
    let mut rest = object.trim_start();
    while !rest.is_empty() {
        let Some(name) = parse_json_string(rest) else {
            break;
        };
        names.push(name);
        let Some(next) = rest.find(',') else {
            break;
        };
        rest = rest[next + 1..].trim_start();
    }
    names
}

fn python_script_names() -> Vec<String> {
    let Ok(content) = fs::read_to_string("pyproject.toml") else {
        return Vec::new();
    };

    let mut in_scripts = false;
    let mut names = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            in_scripts = trimmed == "[project.scripts]";
            continue;
        }
        if !in_scripts {
            continue;
        }
        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"');
        if !name.is_empty() {
            names.push(name.to_owned());
        }
    }
    names
}

fn defaultable_command_names(kind: &ProjectKind) -> Vec<String> {
    match kind {
        ProjectKind::Node { .. } => node_bin_names(),
        ProjectKind::Python { .. } => python_script_names(),
        _ => Vec::new(),
    }
}

fn report_default_unsupported(kind: &ProjectKind) {
    eprintln!(
        "{} defaulting is not yet supported for {} projects",
        runner::style("33", "warning"),
        kind.label()
    );
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

fn apply_path_command_defaults(kind: &ProjectKind, dry_run: bool) -> Result<()> {
    let names = defaultable_command_names(kind);
    if names.is_empty() {
        report_default_unsupported(kind);
        return Ok(());
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let cargo_home = cargo_home_dir(&home);
    let path_env = std::env::var_os("PATH");

    for command in names {
        let dest = resolve_default_destination(&command, &home, &cargo_home, path_env.clone());
        if dry_run {
            eprintln!(
                "{} {} -> {}",
                runner::style("33", "would default"),
                command,
                dest.display()
            );
            continue;
        }

        let source = find_command_on_path(&command, std::env::var_os("PATH"))
            .with_context(|| format!("installed command `{command}` is not on PATH"))?;
        install_binary_to(&source, &dest)?;
        let resolved = find_command_on_path(&command, std::env::var_os("PATH"));
        match resolved {
            Some(path) if path == dest => eprintln!(
                "{} {} -> {}",
                runner::style("32", "defaulted"),
                command,
                path.display()
            ),
            Some(path) => eprintln!(
                "{} {} installed to {}, but shell still resolves {}",
                runner::style("33", "warning"),
                command,
                dest.display(),
                path.display()
            ),
            None => eprintln!(
                "{} {} installed to {}, but command is not on PATH",
                runner::style("33", "warning"),
                command,
                dest.display()
            ),
        }
    }

    Ok(())
}

fn apply_default(kind: &ProjectKind, dry_run: bool) -> Result<()> {
    match kind {
        ProjectKind::Cargo => apply_cargo_default(dry_run),
        _ => apply_path_command_defaults(kind, dry_run),
    }
}

pub(crate) fn execute(dry_run: bool, make_default: bool, extra_args: Vec<String>) -> Result<()> {
    let project = runner::detect_project_with_dir()?;
    let targets = install_targets_for(project.kind, project.dir)?;
    let original = env::current_dir().context("failed to read current directory")?;

    for target in targets {
        runner::with_current_dir(&target.dir, || {
            let mut s = steps(&target.kind)?;
            runner::append_args(&mut s, &extra_args);
            // Node projects with a "bin" field should also link the binary onto PATH.
            if let ProjectKind::Node { manager } = &target.kind {
                if project_detect::node_has_bin(&target.dir) {
                    let cmd = match manager {
                        NodePM::Bun => "bun",
                        NodePM::Pnpm => "pnpm",
                        NodePM::Yarn => "yarn",
                        NodePM::Npm => "npm",
                    };
                    s.push(step(cmd, &["link"]));
                }
            }

            let display_dir = target
                .dir
                .strip_prefix(&original)
                .unwrap_or(&target.dir)
                .display();
            eprintln!("{} {}", runner::style("36", "project"), display_dir);
            runner::run_steps(&target.kind, &s, dry_run)?;
            if make_default {
                apply_default(&target.kind, dry_run)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn node_bin_names_reads_object_form() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "name": "@scope/pkg", "bin": { "one": "cli.js", "two": "other.js" } }"#,
        )
        .unwrap();

        let names = node_bin_names();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(names, ["one", "two"]);
    }

    #[test]
    fn node_bin_names_uses_package_name_for_string_form() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "name": "@scope/mycli", "bin": "cli.js" }"#,
        )
        .unwrap();

        let names = node_bin_names();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(names, ["mycli"]);
    }

    #[test]
    fn python_script_names_reads_project_scripts() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project.scripts]\none = \"pkg:one\"\ntwo = \"pkg:two\"\n[tool.other]\nthree = \"ignored\"\n",
        )
        .unwrap();

        let names = python_script_names();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(names, ["one", "two"]);
    }

    #[test]
    fn cargo_steps() {
        let _lock = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

        let s = steps(&ProjectKind::Cargo).unwrap();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["install", "--path", "."]);
    }

    #[test]
    fn cargo_install_target_detects_single_workspace_member() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/uu\"]",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("crates/uu/src")).unwrap();
        fs::write(
            dir.path().join("crates/uu/Cargo.toml"),
            "[package]\nname = \"univ-utils\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("crates/uu/src/main.rs"), "fn main() {}\n").unwrap();

        assert_eq!(
            cargo_install_target_in(dir.path()).unwrap().path_arg,
            "crates/uu"
        );
    }

    #[test]
    fn cargo_install_target_errors_for_multiple_workspace_members() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]",
        )
        .unwrap();

        for name in ["a", "b"] {
            let crate_dir = dir.path().join("crates").join(name);
            fs::create_dir_all(crate_dir.join("src")).unwrap();
            fs::write(
                crate_dir.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            )
            .unwrap();
            fs::write(crate_dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        }

        let err = cargo_install_target_in(dir.path()).unwrap_err().to_string();
        assert!(err.contains("multiple installable members"));
        assert!(err.contains("a"));
        assert!(err.contains("b"));
    }

    #[test]
    fn cargo_install_target_supports_globbed_workspace_members() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]",
        )
        .unwrap();
        let crate_dir = dir.path().join("crates/app");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(crate_dir.join("src/main.rs"), "fn main() {}\n").unwrap();

        assert_eq!(
            cargo_install_target_in(dir.path()).unwrap().path_arg,
            "crates/app"
        );
    }

    #[test]
    fn cargo_install_target_errors_without_installable_member() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/lib\"]",
        )
        .unwrap();
        let crate_dir = dir.path().join("crates/lib");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(crate_dir.join("src/lib.rs"), "pub fn x() {}\n").unwrap();

        let err = cargo_install_target_in(dir.path()).unwrap_err().to_string();
        assert!(err.contains("no installable workspace member was found"));
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
        fs::create_dir_all(dir.path().join("crates/uu/src")).unwrap();
        fs::write(
            dir.path().join("crates/uu/Cargo.toml"),
            "[package]\nname = \"univ-utils\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("crates/uu/src/main.rs"), "fn main() {}\n").unwrap();

        let s = steps(&ProjectKind::Cargo).unwrap();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(
            cargo_install_target_in(dir.path()).unwrap().path_arg,
            "crates/uu"
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["install", "--path", "crates/uu"]);
    }

    #[test]
    fn node_uses_detected_manager() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Pnpm,
        })
        .unwrap();
        assert_eq!(s[0].program, "pnpm");
    }

    #[test]
    fn zig_install() {
        let s = steps(&ProjectKind::Zig).unwrap();
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build", "-Doptimize=ReleaseSafe"]);
    }

    #[test]
    fn cmake_has_three_phases() {
        let s = steps(&ProjectKind::CMake).unwrap();
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn swift_install() {
        let s = steps(&ProjectKind::Swift).unwrap();
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["build", "-c", "release"]);
    }

    #[test]
    fn dotnet_install() {
        let s = steps(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["publish", "-c", "Release"]);
    }

    #[test]
    fn python_prefers_uv() {
        let s = steps(&ProjectKind::Python { uv: true }).unwrap();
        assert_eq!(s[0].program, "uv");
    }
}
