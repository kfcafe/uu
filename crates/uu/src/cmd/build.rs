//! `uu build` — detect project type and build the project.

use anyhow::Result;
use project_detect::{KotlinBuild, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate build steps for a detected project.
fn steps(kind: &ProjectKind) -> Vec<Step> {
    match kind {
        ProjectKind::Cargo => vec![step("cargo", &["build"])],
        ProjectKind::Go => vec![step("go", &["build", "./..."])],
        ProjectKind::Elixir { escript: true } => vec![step("mix", &["escript.build"])],
        ProjectKind::Elixir { escript: false } => vec![step("mix", &["compile"])],
        ProjectKind::Python { uv: true, .. } => vec![step("uv", &["run", "python", "-m", "build"])],
        ProjectKind::Python { uv: false, .. } => {
            vec![step(runner::python_cmd(), &["-m", "build"])]
        }
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            vec![step(cmd, &["run", "build"])]
        }
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: true },
        } => vec![step("./gradlew", &["build"])],
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: false },
        } => vec![step("gradle", &["build"])],
        ProjectKind::Kotlin {
            build: KotlinBuild::Maven,
        } => vec![step("mvn", &["package"])],
        ProjectKind::Gradle { wrapper: true } => vec![step("./gradlew", &["build"])],
        ProjectKind::Gradle { wrapper: false } => vec![step("gradle", &["build"])],
        ProjectKind::Maven => vec![step("mvn", &["package"])],
        ProjectKind::Ruby => vec![step("bundle", &["exec", "rake", "build"])],
        ProjectKind::Swift => vec![step("swift", &["build"])],
        ProjectKind::Xcode { .. } => vec![step("xcodebuild", &["build"])],
        ProjectKind::DotNet { .. } => vec![step("dotnet", &["build"])],
        ProjectKind::Meson => vec![
            step("meson", &["setup", "builddir"]),
            step("meson", &["compile", "-C", "builddir"]),
        ],
        ProjectKind::CMake => vec![
            step("cmake", &["-B", "build"]),
            step("cmake", &["--build", "build"]),
        ],
        ProjectKind::Zig => vec![step("zig", &["build"])],
        ProjectKind::Make => vec![step("make", &[])],
        ProjectKind::Php | ProjectKind::Julia | ProjectKind::Lua => vec![],
        ProjectKind::R { .. } => vec![step("R", &["CMD", "build", "."])],
        ProjectKind::Dart { flutter: true } => vec![step("flutter", &["build"])],
        ProjectKind::Dart { flutter: false } => {
            vec![step("dart", &["compile", "exe", "bin/main.dart"])]
        }
        ProjectKind::Sbt => vec![step("sbt", &["compile"])],
        ProjectKind::Haskell { stack: true } => vec![step("stack", &["build"])],
        ProjectKind::Haskell { stack: false } => vec![step("cabal", &["build"])],
        ProjectKind::Clojure { lein: true } => vec![step("lein", &["compile"])],
        ProjectKind::Clojure { lein: false } => vec![step("clj", &["-T:build"])],
        ProjectKind::Rebar => vec![step("rebar3", &["compile"])],
        ProjectKind::Dune => vec![step("dune", &["build"])],
        ProjectKind::Perl => vec![step("perl", &["Makefile.PL"]), step("make", &[])],
        ProjectKind::Nim => vec![step("nimble", &["build"])],
        ProjectKind::Crystal => vec![step("shards", &["build"])],
        ProjectKind::Vlang => vec![step("v", &["."])],
        ProjectKind::Gleam => vec![step("gleam", &["build"])],
        ProjectKind::Bazel => vec![step("bazel", &["build", "//..."])],
    }
}

pub(crate) fn execute(dry_run: bool, extra_args: Vec<String>) -> Result<()> {
    let kind = runner::detect_project()?;

    // Python's `build` module is a separate PyPI package — bail early with
    // an actionable message instead of letting Python print the opaque
    // "No module named build" traceback.
    if matches!(kind, ProjectKind::Python { uv: false, .. }) && !dry_run {
        let py = runner::python_cmd();
        let has_build = std::process::Command::new(py)
            .args(["-c", "import build"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !has_build {
            anyhow::bail!(
                "Python `build` package is not installed\n\n  \
                 run: {py} -m pip install build"
            );
        }
    }

    // Node projects may not have a build script — skip gracefully.
    if matches!(kind, ProjectKind::Node { .. }) {
        let dir = std::env::current_dir()?;
        if !project_detect::node_has_script(&dir, "build") {
            eprintln!(
                "{} {} \x1b[2m({})\x1b[0m",
                runner::style("36", "detected"),
                kind.label(),
                kind.detected_file()
            );
            eprintln!(
                "{} no \"build\" script in package.json — nothing to build",
                runner::style("33", "skipped")
            );
            return Ok(());
        }
    }

    let mut s = steps(&kind);
    runner::append_args(&mut s, &extra_args);
    runner::run_steps(&kind, &s, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use project_detect::{KotlinBuild, NodePM};

    #[test]
    fn cargo_build() {
        let s = steps(&ProjectKind::Cargo);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn go_build() {
        let s = steps(&ProjectKind::Go);
        assert_eq!(s[0].args, ["build", "./..."]);
    }

    #[test]
    fn node_uses_run_build() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Npm,
        });
        assert_eq!(s[0].program, "npm");
        assert_eq!(s[0].args, ["run", "build"]);
    }

    #[test]
    fn kotlin_gradle_build() {
        let s = steps(&ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: true },
        });
        assert_eq!(s[0].program, "./gradlew");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn cmake_has_two_phases() {
        let s = steps(&ProjectKind::CMake);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].args, ["-B", "build"]);
        assert_eq!(s[1].args, ["--build", "build"]);
    }

    #[test]
    fn python_with_uv() {
        let s = steps(&ProjectKind::Python { uv: true });
        assert_eq!(s[0].program, "uv");
    }

    #[test]
    fn swift_build() {
        let s = steps(&ProjectKind::Swift);
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn dotnet_build() {
        let s = steps(&ProjectKind::DotNet { sln: false });
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn zig_build() {
        let s = steps(&ProjectKind::Zig);
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn meson_has_two_phases() {
        let s = steps(&ProjectKind::Meson);
        assert_eq!(s.len(), 2);
    }
}
