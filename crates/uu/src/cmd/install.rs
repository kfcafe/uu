//! `uu install` — detect project type and run the install command.

use std::fs;

use anyhow::Result;
use project_detect::{NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate install steps for a detected project.
fn steps(kind: &ProjectKind) -> Vec<Step> {
    match kind {
        ProjectKind::Cargo => vec![step("cargo", &["install", "--path", "."])],
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
        ProjectKind::Gradle { wrapper: true } => vec![step("./gradlew", &["build"])],
        ProjectKind::Gradle { wrapper: false } => vec![step("gradle", &["build"])],
        ProjectKind::Maven => vec![step("mvn", &["install"])],
        ProjectKind::Ruby => vec![step("bundle", &["install"])],
        ProjectKind::Swift => vec![step("swift", &["build", "-c", "release"])],
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

pub(crate) fn execute(dry_run: bool, extra_args: Vec<String>) -> Result<()> {
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

    runner::run_steps(&kind, &s, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_steps() {
        let s = steps(&ProjectKind::Cargo);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["install", "--path", "."]);
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
