//! Project type detection for the `uu` universal utilities suite.
//!
//! Scans a directory for build system files (Cargo.toml, package.json, etc.)
//! and returns the detected [`ProjectKind`] with ecosystem-specific metadata.

use std::path::Path;
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
    Elixir,
    Python { uv: bool },
    Node { manager: NodePM },
    Gradle { wrapper: bool },
    Maven,
    Ruby,
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
            Self::Elixir => "Elixir",
            Self::Python { .. } => "Python",
            Self::Node { .. } => "Node.js",
            Self::Gradle { .. } => "Gradle",
            Self::Maven => "Maven",
            Self::Ruby => "Ruby",
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
            Self::Elixir => "mix.exs",
            Self::Python { .. } => "pyproject.toml",
            Self::Node { .. } => "package.json",
            Self::Gradle { .. } => "build.gradle",
            Self::Maven => "pom.xml",
            Self::Ruby => "Gemfile",
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
            Self::Elixir => &["_build", "deps"],
            Self::Python { .. } => &["__pycache__", ".pytest_cache", "build", "dist"],
            Self::Node { .. } => &["node_modules", ".next", ".nuxt", ".turbo"],
            Self::Gradle { .. } => &["build", ".gradle"],
            Self::Maven => &["target"],
            Self::Ruby => &[".bundle"],
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
    let dir = dir.as_ref();

    // Language-specific build files — highest confidence
    if dir.join("Cargo.toml").exists() {
        return Some(ProjectKind::Cargo);
    }
    if dir.join("go.mod").exists() {
        return Some(ProjectKind::Go);
    }
    if dir.join("mix.exs").exists() {
        return Some(ProjectKind::Elixir);
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
        ("mix.exs", "mix deps.get && mix compile"),
        ("pyproject.toml", "pip install . (or uv)"),
        ("setup.py", "pip install ."),
        ("package.json", "npm/yarn/pnpm/bun install"),
        ("build.gradle", "./gradlew build"),
        ("pom.xml", "mvn install"),
        ("Gemfile", "bundle install"),
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

// -- Private helpers ---------------------------------------------------------

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
        assert_eq!(detect(dir.path()), Some(ProjectKind::Elixir));
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
    fn node_artifacts_include_node_modules() {
        let kind = ProjectKind::Node {
            manager: NodePM::Npm,
        };
        assert!(kind.artifact_dirs().contains(&"node_modules"));
    }
}
