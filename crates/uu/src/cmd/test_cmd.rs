//! `uu test` — detect project type and run the test suite.

use anyhow::Result;
use project_detect::{KotlinBuild, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate test steps for a detected project.
fn steps(kind: &ProjectKind) -> Vec<Step> {
    match kind {
        ProjectKind::Cargo => vec![step("cargo", &["test"])],
        ProjectKind::Go => vec![step("go", &["test", "./..."])],
        ProjectKind::Elixir { .. } => vec![step("mix", &["test"])],
        ProjectKind::Python { uv: true } => vec![step("uv", &["run", "pytest"])],
        ProjectKind::Python { uv: false } => vec![step("pytest", &[])],
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            vec![step(cmd, &["test"])]
        }
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: true },
        } => vec![step("./gradlew", &["test"])],
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: false },
        } => vec![step("gradle", &["test"])],
        ProjectKind::Kotlin {
            build: KotlinBuild::Maven,
        } => vec![step("mvn", &["test"])],
        ProjectKind::Gradle { wrapper: true } => vec![step("./gradlew", &["test"])],
        ProjectKind::Gradle { wrapper: false } => vec![step("gradle", &["test"])],
        ProjectKind::Maven => vec![step("mvn", &["test"])],
        ProjectKind::Ruby => vec![step("bundle", &["exec", "rake", "test"])],
        ProjectKind::Swift => vec![step("swift", &["test"])],
        ProjectKind::Xcode { .. } => vec![step("xcodebuild", &["test"])],
        ProjectKind::DotNet { .. } => vec![step("dotnet", &["test"])],
        ProjectKind::Meson => vec![step("meson", &["test", "-C", "builddir"])],
        ProjectKind::CMake => vec![step("ctest", &["--test-dir", "build"])],
        ProjectKind::Zig => vec![step("zig", &["build", "test"])],
        ProjectKind::Make => vec![step("make", &["test"])],
        ProjectKind::Php => vec![step("vendor/bin/phpunit", &[])],
        ProjectKind::Dart { flutter: true } => vec![step("flutter", &["test"])],
        ProjectKind::Dart { flutter: false } => vec![step("dart", &["test"])],
        ProjectKind::Sbt => vec![step("sbt", &["test"])],
        ProjectKind::Haskell { stack: true } => vec![step("stack", &["test"])],
        ProjectKind::Haskell { stack: false } => vec![step("cabal", &["test"])],
        ProjectKind::Clojure { lein: true } => vec![step("lein", &["test"])],
        ProjectKind::Clojure { lein: false } => vec![step("clj", &["-M:test"])],
        ProjectKind::Rebar => vec![step("rebar3", &["eunit"])],
        ProjectKind::Dune => vec![step("dune", &["test"])],
        ProjectKind::Perl => vec![step("prove", &["-l"])],
        ProjectKind::Julia => vec![step("julia", &["--project", "-e", "using Pkg; Pkg.test()"])],
        ProjectKind::R { .. } => vec![step("R", &["CMD", "check", "--no-manual", "."])],
        ProjectKind::Nim => vec![step("nimble", &["test"])],
        ProjectKind::Crystal => vec![step("crystal", &["spec"])],
        ProjectKind::Vlang => vec![step("v", &["test", "."])],
        ProjectKind::Gleam => vec![step("gleam", &["test"])],
        ProjectKind::Lua => vec![step("luarocks", &["test"])],
        ProjectKind::Bazel => vec![step("bazel", &["test", "//..."])],
    }
}

pub(crate) fn execute(dry_run: bool, extra_args: Vec<String>) -> Result<()> {
    let kind = runner::detect_project()?;
    let mut s = steps(&kind);
    runner::append_args(&mut s, &extra_args);
    runner::run_steps(&kind, &s, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_test() {
        let s = steps(&ProjectKind::Cargo);
        assert_eq!(s[0].args, ["test"]);
    }

    #[test]
    fn go_test_all_packages() {
        let s = steps(&ProjectKind::Go);
        assert_eq!(s[0].args, ["test", "./..."]);
    }

    #[test]
    fn python_with_uv_runs_pytest() {
        let s = steps(&ProjectKind::Python { uv: true });
        assert_eq!(s[0].program, "uv");
        assert_eq!(s[0].args, ["run", "pytest"]);
    }

    #[test]
    fn swift_test() {
        let s = steps(&ProjectKind::Swift);
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["test"]);
    }

    #[test]
    fn kotlin_test() {
        let s = steps(&ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: false },
        });
        assert_eq!(s[0].program, "gradle");
        assert_eq!(s[0].args, ["test"]);
    }

    #[test]
    fn dotnet_test() {
        let s = steps(&ProjectKind::DotNet { sln: false });
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["test"]);
    }

    #[test]
    fn zig_test() {
        let s = steps(&ProjectKind::Zig);
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build", "test"]);
    }

    #[test]
    fn cmake_uses_ctest() {
        let s = steps(&ProjectKind::CMake);
        assert_eq!(s[0].program, "ctest");
    }

    #[test]
    fn node_yarn_test() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Yarn,
        });
        assert_eq!(s[0].program, "yarn");
        assert_eq!(s[0].args, ["test"]);
    }
}
