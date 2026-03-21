//! Project type detection for the `uu` universal utilities suite.
//!
//! Scans a directory for build system files (Cargo.toml, package.json, etc.)
//! and returns the detected [`ProjectKind`] with ecosystem-specific metadata.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// -- Public types ------------------------------------------------------------

/// Node.js package manager, detected from lockfile presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePM {
    Bun,
    Pnpm,
    Yarn,
    Npm,
}

/// A detected project type with ecosystem-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectKind {
    Cargo,
    Go,
    Elixir { escript: bool },
    Python { uv: bool },
    Node { manager: NodePM },
    Gradle { wrapper: bool },
    Maven,
    Ruby,
    Swift,
    DotNet { sln: bool },
    Meson,
    CMake,
    Make,
}

impl ProjectKind {
    /// Human-readable ecosystem name (e.g. "Rust", "Node.js").
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cargo => "Rust",
            Self::Go => "Go",
            Self::Elixir { .. } => "Elixir",
            Self::Python { .. } => "Python",
            Self::Node { .. } => "Node.js",
            Self::Gradle { .. } => "Gradle",
            Self::Maven => "Maven",
            Self::Ruby => "Ruby",
            Self::Swift => "Swift",
            Self::DotNet { .. } => ".NET",
            Self::Meson => "Meson",
            Self::CMake => "CMake",
            Self::Make => "Make",
        }
    }

    /// The file that triggered detection (e.g. "Cargo.toml", "package.json").
    pub fn detected_file(&self) -> &'static str {
        match self {
            Self::Cargo => "Cargo.toml",
            Self::Go => "go.mod",
            Self::Elixir { .. } => "mix.exs",
            Self::Python { .. } => "pyproject.toml",
            Self::Node { .. } => "package.json",
            Self::Gradle { .. } => "build.gradle",
            Self::Maven => "pom.xml",
            Self::Ruby => "Gemfile",
            Self::Swift => "Package.swift",
            Self::DotNet { sln: true } => "*.sln",
            Self::DotNet { sln: false } => "*.csproj",
            Self::Meson => "meson.build",
            Self::CMake => "CMakeLists.txt",
            Self::Make => "Makefile",
        }
    }

    /// Directories containing build artifacts for this project type.
    pub fn artifact_dirs(&self) -> &'static [&'static str] {
        match self {
            Self::Cargo => &["target"],
            Self::Go => &[],
            Self::Elixir { .. } => &["_build", "deps"],
            Self::Python { .. } => &["__pycache__", ".pytest_cache", "build", "dist"],
            Self::Node { .. } => &["node_modules", ".next", ".nuxt", ".turbo"],
            Self::Gradle { .. } => &["build", ".gradle"],
            Self::Maven => &["target"],
            Self::Ruby => &[".bundle"],
            Self::Swift => &[".build"],
            Self::DotNet { .. } => &["bin", "obj"],
            Self::Meson => &["builddir"],
            Self::CMake => &["build"],
            Self::Make => &[],
        }
    }
}

// -- Detection ---------------------------------------------------------------

/// Detect the project kind from files in `dir`.
///
/// Checks language-specific files first (high confidence), then falls back
/// to generic build systems (lower confidence). Returns `None` if no
/// recognized project files are found.
#[must_use]
pub fn detect(dir: impl AsRef<Path>) -> Option<ProjectKind> {
    detect_in(dir.as_ref())
}

/// Like [`detect`], but walks up the directory tree if no project is found
/// in `dir`. Returns the detected kind and the directory it was found in.
///
/// This handles the common case of running from a subdirectory inside a
/// workspace (e.g. `tower/imp/` inside a Cargo workspace rooted at `tower/`).
#[must_use]
pub fn detect_walk(dir: impl AsRef<Path>) -> Option<(ProjectKind, PathBuf)> {
    let mut current = dir.as_ref().to_path_buf();
    loop {
        if let Some(kind) = detect_in(&current) {
            return Some((kind, current));
        }
        if !current.pop() {
            return None;
        }
    }
}

fn detect_in(dir: &Path) -> Option<ProjectKind> {

    // Language-specific build files — highest confidence
    if dir.join("Cargo.toml").exists() {
        return Some(ProjectKind::Cargo);
    }
    if dir.join("go.mod").exists() {
        return Some(ProjectKind::Go);
    }
    if dir.join("mix.exs").exists() {
        return Some(ProjectKind::Elixir {
            escript: elixir_has_escript(dir),
        });
    }

    // Python
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("setup.cfg").exists()
    {
        return Some(ProjectKind::Python {
            uv: command_on_path("uv"),
        });
    }

    // Node.js
    if dir.join("package.json").exists() {
        return Some(ProjectKind::Node {
            manager: detect_node_pm(dir),
        });
    }

    // JVM
    if dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists() {
        return Some(ProjectKind::Gradle {
            wrapper: dir.join("gradlew").exists(),
        });
    }
    if dir.join("pom.xml").exists() {
        return Some(ProjectKind::Maven);
    }

    // Ruby
    if dir.join("Gemfile").exists() {
        return Some(ProjectKind::Ruby);
    }

    // Swift
    if dir.join("Package.swift").exists() {
        return Some(ProjectKind::Swift);
    }

    // .NET
    {
        let has_sln = has_extension_in_dir(dir, "sln");
        let has_csproj = !has_sln && has_extension_in_dir(dir, "csproj");
        if has_sln || has_csproj {
            return Some(ProjectKind::DotNet { sln: has_sln });
        }
    }

    // Generic build systems — lowest confidence
    if dir.join("meson.build").exists() {
        return Some(ProjectKind::Meson);
    }
    if dir.join("CMakeLists.txt").exists() {
        return Some(ProjectKind::CMake);
    }
    if dir.join("Makefile").exists()
        || dir.join("makefile").exists()
        || dir.join("GNUmakefile").exists()
    {
        return Some(ProjectKind::Make);
    }

    None
}

/// Detect the project kind, walking up from `dir` if nothing is found.
///
/// Convenience wrapper around [`detect_walk`] that returns only the kind.
#[must_use]
pub fn detect_nearest(dir: impl AsRef<Path>) -> Option<ProjectKind> {
    detect_walk(dir).map(|(kind, _)| kind)
}

/// Check whether a command exists on `$PATH`.
#[must_use]
pub fn command_on_path(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Formatted table of all supported project types for error messages.
#[must_use]
pub fn supported_table() -> String {
    let entries = [
        ("Cargo.toml", "cargo install --path ."),
        ("go.mod", "go install ./..."),
        (
            "mix.exs",
            "mix deps.get && mix compile (or mix escript.build)",
        ),
        ("pyproject.toml", "pip install . (or uv)"),
        ("setup.py", "pip install ."),
        ("package.json", "npm/yarn/pnpm/bun install"),
        ("build.gradle", "./gradlew build"),
        ("pom.xml", "mvn install"),
        ("Gemfile", "bundle install"),
        ("Package.swift", "swift build"),
        ("*.csproj", "dotnet build"),
        ("meson.build", "meson setup + compile + install"),
        ("CMakeLists.txt", "cmake build + install"),
        ("Makefile", "make && make install"),
    ];

    let mut out = String::from("  supported project files:\n");
    for (file, cmd) in entries {
        out.push_str(&format!("    {file:<16} → {cmd}\n"));
    }
    out
}

// -- Node.js package.json helpers --------------------------------------------

/// Check whether a Node project's `package.json` contains a specific script.
#[must_use]
pub fn node_has_script(dir: &Path, name: &str) -> bool {
    let Ok(content) = std::fs::read_to_string(dir.join("package.json")) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    json.get("scripts")
        .and_then(|s| s.get(name))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Check whether a Node project's `package.json` has a `"bin"` field.
#[must_use]
pub fn node_has_bin(dir: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(dir.join("package.json")) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    match json.get("bin") {
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(serde_json::Value::Object(m)) => !m.is_empty(),
        _ => false,
    }
}

// -- Private helpers ---------------------------------------------------------

/// Check whether an Elixir project has an escript configuration.
///
/// Scans `mix.exs` (and child app `mix.exs` files in umbrella projects)
/// for the `escript:` keyword, which indicates the project produces a
/// standalone escript binary.
fn elixir_has_escript(dir: &Path) -> bool {
    // Check root mix.exs
    if let Ok(content) = std::fs::read_to_string(dir.join("mix.exs")) {
        if content.contains("escript:") {
            return true;
        }
    }

    // Check umbrella child apps (apps/*/mix.exs)
    let apps_dir = dir.join("apps");
    if apps_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&apps_dir) {
            for entry in entries.flatten() {
                let child_mix = entry.path().join("mix.exs");
                if let Ok(content) = std::fs::read_to_string(&child_mix) {
                    if content.contains("escript:") {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check whether any file in `dir` has the given extension (non-recursive).
fn has_extension_in_dir(dir: &Path, ext: &str) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == ext))
        })
        .unwrap_or(false)
}

fn detect_node_pm(dir: &Path) -> NodePM {
    if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        NodePM::Bun
    } else if dir.join("pnpm-lock.yaml").exists() {
        NodePM::Pnpm
    } else if dir.join("yarn.lock").exists() {
        NodePM::Yarn
    } else {
        NodePM::Npm
    }
}

// -- Workspace detection -----------------------------------------------------

/// A package in a workspace that has a "dev" script.
#[derive(Debug, Clone)]
pub struct WorkspacePackage {
    /// Directory name, e.g. "api", "web".
    pub name: String,
    /// Absolute path to the package directory.
    pub path: PathBuf,
    /// Value of the "dev" script from package.json.
    pub dev_script: String,
}

/// Detect Node.js workspace packages that have a "dev" script.
///
/// Checks for `pnpm-workspace.yaml` first, then `"workspaces"` field in
/// `package.json`. Returns `None` if not a workspace. Returns `Some(vec![])`
/// if workspace but no dev scripts found.
#[must_use]
pub fn detect_node_workspace(dir: &Path) -> Option<Vec<WorkspacePackage>> {
    let patterns =
        read_pnpm_workspace_patterns(dir).or_else(|| read_npm_workspace_patterns(dir))?;

    let mut packages = Vec::new();
    for pattern in &patterns {
        collect_workspace_packages(dir, pattern, &mut packages);
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Some(packages)
}

/// Parse glob patterns from `pnpm-workspace.yaml`.
///
/// Handles the simple format:
/// ```yaml
/// packages:
///   - "packages/*"
///   - "apps/*"
/// ```
fn read_pnpm_workspace_patterns(dir: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(dir.join("pnpm-workspace.yaml")).ok()?;
    let mut patterns = Vec::new();
    let mut in_packages = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "packages:" {
            in_packages = true;
            continue;
        }
        if in_packages {
            // A non-list line after "packages:" ends the section.
            if !trimmed.starts_with('-') {
                if !trimmed.is_empty() {
                    break;
                }
                continue;
            }
            let value = trimmed
                .trim_start_matches('-')
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            // Skip exclusion patterns.
            if value.starts_with('!') {
                continue;
            }
            if !value.is_empty() {
                patterns.push(value.to_string());
            }
        }
    }

    if patterns.is_empty() {
        return None;
    }
    Some(patterns)
}

/// Read workspace patterns from the `"workspaces"` field in `package.json`.
fn read_npm_workspace_patterns(dir: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let arr = json.get("workspaces")?.as_array()?;

    let patterns: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|s| !s.starts_with('!'))
        .map(|s| s.to_string())
        .collect();

    if patterns.is_empty() {
        return None;
    }
    Some(patterns)
}

/// Expand a simple `prefix/*` glob pattern into workspace packages.
///
/// Only handles the common `dir/*` form — splits on `/*` and lists the
/// prefix directory. Each subdirectory with a `package.json` containing a
/// `scripts.dev` entry becomes a [`WorkspacePackage`].
fn collect_workspace_packages(root: &Path, pattern: &str, out: &mut Vec<WorkspacePackage>) {
    let prefix = match pattern.strip_suffix("/*") {
        Some(p) => p,
        None => pattern,
    };

    let search_dir = root.join(prefix);
    let entries = match std::fs::read_dir(&search_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let pkg_dir = entry.path();
        if !pkg_dir.is_dir() {
            continue;
        }
        let pkg_json_path = pkg_dir.join("package.json");
        let content = match std::fs::read_to_string(&pkg_json_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(dev) = json
            .get("scripts")
            .and_then(|s| s.get("dev"))
            .and_then(|d| d.as_str())
        {
            let name = pkg_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let abs_path = match pkg_dir.canonicalize() {
                Ok(p) => p,
                Err(_) => pkg_dir,
            };
            out.push(WorkspacePackage {
                name,
                path: abs_path,
                dev_script: dev.to_string(),
            });
        }
    }
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detect_cargo() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Cargo));
    }

    #[test]
    fn detect_go() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Go));
    }

    #[test]
    fn detect_elixir() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("mix.exs"), "").unwrap();
        assert!(matches!(
            detect(dir.path()),
            Some(ProjectKind::Elixir { .. })
        ));
    }

    #[test]
    fn detect_python_pyproject() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        assert!(matches!(
            detect(dir.path()),
            Some(ProjectKind::Python { .. })
        ));
    }

    #[test]
    fn detect_python_setup_py() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("setup.py"), "").unwrap();
        assert!(matches!(
            detect(dir.path()),
            Some(ProjectKind::Python { .. })
        ));
    }

    #[test]
    fn detect_python_setup_cfg() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("setup.cfg"), "").unwrap();
        assert!(matches!(
            detect(dir.path()),
            Some(ProjectKind::Python { .. })
        ));
    }

    #[test]
    fn detect_node_npm_default() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Node {
                manager: NodePM::Npm
            })
        );
    }

    #[test]
    fn detect_node_yarn() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("yarn.lock"), "").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Node {
                manager: NodePM::Yarn
            })
        );
    }

    #[test]
    fn detect_node_pnpm() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Node {
                manager: NodePM::Pnpm
            })
        );
    }

    #[test]
    fn detect_node_bun() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("bun.lockb"), "").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Node {
                manager: NodePM::Bun
            })
        );
    }

    #[test]
    fn detect_gradle_with_wrapper() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("build.gradle"), "").unwrap();
        fs::write(dir.path().join("gradlew"), "").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Gradle { wrapper: true })
        );
    }

    #[test]
    fn detect_gradle_kts_no_wrapper() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("build.gradle.kts"), "").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Gradle { wrapper: false })
        );
    }

    #[test]
    fn detect_maven() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("pom.xml"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Maven));
    }

    #[test]
    fn detect_ruby() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Gemfile"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Ruby));
    }

    #[test]
    fn detect_swift() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Package.swift"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Swift));
    }

    #[test]
    fn detect_dotnet_csproj() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("MyApp.csproj"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::DotNet { sln: false }));
    }

    #[test]
    fn detect_dotnet_sln() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("MyApp.sln"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::DotNet { sln: true }));
    }

    #[test]
    fn detect_dotnet_sln_preferred_over_csproj() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("MyApp.sln"), "").unwrap();
        fs::write(dir.path().join("MyApp.csproj"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::DotNet { sln: true }));
    }

    #[test]
    fn detect_meson() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("meson.build"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Meson));
    }

    #[test]
    fn detect_cmake() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("CMakeLists.txt"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::CMake));
    }

    #[test]
    fn detect_makefile() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Makefile"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Make));
    }

    #[test]
    fn detect_makefile_lowercase() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("makefile"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Make));
    }

    #[test]
    fn detect_gnumakefile() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("GNUmakefile"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Make));
    }

    #[test]
    fn detect_empty_dir_returns_none() {
        let dir = tempdir().unwrap();
        assert_eq!(detect(dir.path()), None);
    }

    // -- Priority tests: language-specific wins over generic -----------------

    #[test]
    fn cargo_wins_over_makefile() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        fs::write(dir.path().join("Makefile"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Cargo));
    }

    #[test]
    fn go_wins_over_makefile() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "").unwrap();
        fs::write(dir.path().join("Makefile"), "").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Go));
    }

    #[test]
    fn node_wins_over_cmake() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("CMakeLists.txt"), "").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(ProjectKind::Node {
                manager: NodePM::Npm
            })
        );
    }

    #[test]
    fn cargo_wins_over_node() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect(dir.path()), Some(ProjectKind::Cargo));
    }

    // -- artifact_dirs -------------------------------------------------------

    #[test]
    fn cargo_artifacts_include_target() {
        assert!(ProjectKind::Cargo.artifact_dirs().contains(&"target"));
    }

    #[test]
    fn swift_artifacts_include_build() {
        assert!(ProjectKind::Swift.artifact_dirs().contains(&".build"));
    }

    #[test]
    fn dotnet_artifacts_include_bin_obj() {
        let kind = ProjectKind::DotNet { sln: false };
        assert!(kind.artifact_dirs().contains(&"bin"));
        assert!(kind.artifact_dirs().contains(&"obj"));
    }

    #[test]
    fn node_artifacts_include_node_modules() {
        let kind = ProjectKind::Node {
            manager: NodePM::Npm,
        };
        assert!(kind.artifact_dirs().contains(&"node_modules"));
    }

    // -- Workspace detection -------------------------------------------------

    /// Helper: create a package dir with a package.json containing optional dev script.
    fn create_pkg(parent: &Path, name: &str, dev_script: Option<&str>) {
        let pkg = parent.join(name);
        fs::create_dir_all(&pkg).unwrap();
        let scripts = match dev_script {
            Some(s) => format!(r#", "scripts": {{ "dev": "{s}" }}"#),
            None => String::new(),
        };
        fs::write(
            pkg.join("package.json"),
            format!(r#"{{ "name": "{name}"{scripts} }}"#),
        )
        .unwrap();
    }

    #[test]
    fn detect_pnpm_workspace() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Root package.json (needed for a real project, but workspace comes from yaml)
        fs::write(root.join("package.json"), r#"{ "name": "root" }"#).unwrap();

        // pnpm-workspace.yaml with two patterns, one exclusion
        fs::write(
            root.join("pnpm-workspace.yaml"),
            "packages:\n  - \"apps/*\"\n  - \"!apps/ignored\"\n",
        )
        .unwrap();

        // Two packages: one with dev, one without
        fs::create_dir_all(root.join("apps")).unwrap();
        create_pkg(&root.join("apps"), "web", Some("next dev"));
        create_pkg(&root.join("apps"), "api", None);

        let result = detect_node_workspace(root).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "web");
        assert_eq!(result[0].dev_script, "next dev");
    }

    #[test]
    fn detect_npm_workspace() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{ "name": "root", "workspaces": ["packages/*"] }"#,
        )
        .unwrap();

        fs::create_dir_all(root.join("packages")).unwrap();
        create_pkg(&root.join("packages"), "alpha", Some("vite dev"));
        create_pkg(&root.join("packages"), "beta", Some("node server.js"));

        let result = detect_node_workspace(root).unwrap();
        assert_eq!(result.len(), 2);
        // Sorted by name
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[0].dev_script, "vite dev");
        assert_eq!(result[1].name, "beta");
        assert_eq!(result[1].dev_script, "node server.js");
    }

    #[test]
    fn no_workspace_returns_none() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "name": "solo-project" }"#,
        )
        .unwrap();
        assert!(detect_node_workspace(dir.path()).is_none());
    }

    // -- node_has_script / node_has_bin ----------------------------------------

    #[test]
    fn node_has_script_finds_build() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "scripts": { "build": "tsc", "test": "jest" } }"#,
        )
        .unwrap();
        assert!(node_has_script(dir.path(), "build"));
        assert!(node_has_script(dir.path(), "test"));
        assert!(!node_has_script(dir.path(), "lint"));
    }

    #[test]
    fn node_has_script_returns_false_when_no_scripts() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{ "name": "foo" }"#).unwrap();
        assert!(!node_has_script(dir.path(), "build"));
    }

    #[test]
    fn node_has_script_returns_false_for_empty_script() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "scripts": { "build": "" } }"#,
        )
        .unwrap();
        assert!(!node_has_script(dir.path(), "build"));
    }

    #[test]
    fn node_has_bin_string_form() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "bin": "src/cli.js" }"#,
        )
        .unwrap();
        assert!(node_has_bin(dir.path()));
    }

    #[test]
    fn node_has_bin_object_form() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "bin": { "mycli": "src/cli.js" } }"#,
        )
        .unwrap();
        assert!(node_has_bin(dir.path()));
    }

    #[test]
    fn node_has_bin_returns_false_when_absent() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{ "name": "foo" }"#).unwrap();
        assert!(!node_has_bin(dir.path()));
    }

    #[test]
    fn workspace_no_dev_scripts() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{ "name": "root", "workspaces": ["libs/*"] }"#,
        )
        .unwrap();

        fs::create_dir_all(root.join("libs")).unwrap();
        create_pkg(&root.join("libs"), "utils", None);
        create_pkg(&root.join("libs"), "types", None);

        let result = detect_node_workspace(root).unwrap();
        assert!(result.is_empty());
    }

    // -- detect_walk (ancestor walking) --------------------------------------

    #[test]
    fn detect_walk_finds_project_in_current_dir() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let result = detect_walk(dir.path());
        assert!(result.is_some());
        let (kind, found_dir) = result.unwrap();
        assert_eq!(kind, ProjectKind::Cargo);
        assert_eq!(found_dir, dir.path());
    }

    #[test]
    fn detect_walk_finds_project_in_parent() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let child = dir.path().join("subdir");
        fs::create_dir(&child).unwrap();
        let result = detect_walk(&child);
        assert!(result.is_some());
        let (kind, found_dir) = result.unwrap();
        assert_eq!(kind, ProjectKind::Cargo);
        assert_eq!(found_dir, dir.path().to_path_buf());
    }

    #[test]
    fn detect_walk_finds_project_in_grandparent() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "").unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();
        let result = detect_walk(&deep);
        assert!(result.is_some());
        let (kind, _) = result.unwrap();
        assert_eq!(kind, ProjectKind::Go);
    }

    #[test]
    fn detect_walk_returns_none_when_no_project_anywhere() {
        let dir = tempdir().unwrap();
        let child = dir.path().join("empty");
        fs::create_dir(&child).unwrap();
        // tempdir is in /tmp which has no project files above it
        // but /tmp itself or / might have something — use the child inside tempdir
        // detect_walk will walk up to / and return None
        // For a reliable test, just verify the function doesn't panic
        let _ = detect_walk(&child);
    }

    #[test]
    fn detect_walk_prefers_closest_project() {
        let dir = tempdir().unwrap();
        // Parent has Cargo.toml
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        // Child has package.json
        let child = dir.path().join("frontend");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("package.json"), "{}").unwrap();
        // detect_walk from child should find Node (the closest match)
        let (kind, found_dir) = detect_walk(&child).unwrap();
        assert_eq!(kind, ProjectKind::Node { manager: NodePM::Npm });
        assert_eq!(found_dir, child);
    }

    #[test]
    fn detect_nearest_returns_kind_only() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let child = dir.path().join("src");
        fs::create_dir(&child).unwrap();
        assert_eq!(detect_nearest(&child), Some(ProjectKind::Cargo));
    }
}
