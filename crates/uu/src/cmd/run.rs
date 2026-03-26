//! `uu run` — detect project type and run it.

use std::env;

use anyhow::{bail, Result};
use project_detect::{KotlinBuild, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate run steps for a detected project.
fn steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Cargo => Ok(vec![step("cargo", &["run"])]),
        ProjectKind::Go => Ok(vec![step("go", &["run", "."])]),
        ProjectKind::Elixir { .. } => Ok(vec![step("mix", &["run"])]),
        ProjectKind::Python { uv: true } => python_steps("uv", &["run"]),
        ProjectKind::Python { uv: false } => python_steps(runner::python_cmd(), &[]),
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            Ok(vec![step(cmd, &["start"])])
        }
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: true },
        } => Ok(vec![step("./gradlew", &["run"])]),
        ProjectKind::Kotlin {
            build: KotlinBuild::Gradle { wrapper: false },
        } => Ok(vec![step("gradle", &["run"])]),
        ProjectKind::Kotlin {
            build: KotlinBuild::Maven,
        } => Ok(vec![step("mvn", &["compile", "exec:java"])]),
        ProjectKind::Gradle { wrapper: true } => Ok(vec![step("./gradlew", &["run"])]),
        ProjectKind::Gradle { wrapper: false } => Ok(vec![step("gradle", &["run"])]),
        ProjectKind::Maven => Ok(vec![step("mvn", &["compile", "exec:java"])]),
        ProjectKind::Ruby => Ok(vec![step("bundle", &["exec", "ruby", "app.rb"])]),
        ProjectKind::Swift => Ok(vec![step("swift", &["run"])]),
        ProjectKind::DotNet { .. } => Ok(vec![step("dotnet", &["run"])]),
        ProjectKind::Zig => Ok(vec![step("zig", &["build", "run"])]),
        ProjectKind::Make => Ok(vec![step("make", &["run"])]),
        ProjectKind::Xcode { .. }
        | ProjectKind::Meson
        | ProjectKind::CMake
        | ProjectKind::R { .. } => {
            bail!(
                "uu can't auto-run {} projects — run the built binary directly",
                kind.label()
            )
        }
        ProjectKind::Php => Ok(vec![step("php", &["-S", "localhost:8000"])]),
        ProjectKind::Dart { flutter: true } => Ok(vec![step("flutter", &["run"])]),
        ProjectKind::Dart { flutter: false } => Ok(vec![step("dart", &["run"])]),
        ProjectKind::Sbt => Ok(vec![step("sbt", &["run"])]),
        ProjectKind::Haskell { stack: true } => Ok(vec![step("stack", &["run"])]),
        ProjectKind::Haskell { stack: false } => Ok(vec![step("cabal", &["run"])]),
        ProjectKind::Clojure { lein: true } => Ok(vec![step("lein", &["run"])]),
        ProjectKind::Clojure { lein: false } => Ok(vec![step("clj", &["-M", "-m", "main"])]),
        ProjectKind::Rebar => Ok(vec![step("rebar3", &["shell"])]),
        ProjectKind::Dune => Ok(vec![step("dune", &["exec", "."])]),
        ProjectKind::Perl => Ok(vec![step("perl", &["app.pl"])]),
        ProjectKind::Julia => Ok(vec![step("julia", &["--project", "src/main.jl"])]),
        ProjectKind::Nim => Ok(vec![step("nimble", &["run"])]),
        ProjectKind::Crystal => Ok(vec![step("crystal", &["run", "src/main.cr"])]),
        ProjectKind::Vlang => Ok(vec![step("v", &["run", "."])]),
        ProjectKind::Gleam => Ok(vec![step("gleam", &["run"])]),
        ProjectKind::Lua => Ok(vec![step("lua", &["init.lua"])]),
        ProjectKind::Bazel => Ok(vec![step("bazel", &["run", "//:main"])]),
    }
}

/// Detect Python entry point and return the run command.
fn python_steps(base_cmd: &str, prefix_args: &[&str]) -> Result<Vec<Step>> {
    let dir = env::current_dir()?;

    // Django
    if dir.join("manage.py").exists() {
        let mut args: Vec<&str> = prefix_args.to_vec();
        args.extend(["manage.py", "runserver"]);
        return Ok(vec![step(base_cmd, &args)]);
    }

    // Flask / generic
    if dir.join("app.py").exists() {
        let mut args: Vec<&str> = prefix_args.to_vec();
        args.push("app.py");
        return Ok(vec![step(base_cmd, &args)]);
    }

    // Explicit main
    if dir.join("main.py").exists() {
        let mut args: Vec<&str> = prefix_args.to_vec();
        args.push("main.py");
        return Ok(vec![step(base_cmd, &args)]);
    }

    bail!("no Python entry point found — create main.py, app.py, or manage.py")
}

pub(crate) fn execute(dry_run: bool, extra_args: Vec<String>) -> Result<()> {
    let kind = runner::detect_project()?;
    let mut s = steps(&kind)?;
    runner::append_args(&mut s, &extra_args);
    runner::run_steps(&kind, &s, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_run() {
        let s = steps(&ProjectKind::Cargo).unwrap();
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["run"]);
    }

    #[test]
    fn go_run() {
        let s = steps(&ProjectKind::Go).unwrap();
        assert_eq!(s[0].args, ["run", "."]);
    }

    #[test]
    fn swift_run() {
        let s = steps(&ProjectKind::Swift).unwrap();
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["run"]);
    }

    #[test]
    fn dotnet_run() {
        let s = steps(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["run"]);
    }

    #[test]
    fn kotlin_run() {
        let s = steps(&ProjectKind::Kotlin {
            build: KotlinBuild::Maven,
        })
        .unwrap();
        assert_eq!(s[0].program, "mvn");
        assert_eq!(s[0].args, ["compile", "exec:java"]);
    }

    #[test]
    fn zig_run() {
        let s = steps(&ProjectKind::Zig).unwrap();
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build", "run"]);
    }

    #[test]
    fn cmake_is_unsupported() {
        assert!(steps(&ProjectKind::CMake).is_err());
    }

    #[test]
    fn node_uses_start() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Npm,
        })
        .unwrap();
        assert_eq!(s[0].args, ["start"]);
    }
}
