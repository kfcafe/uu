//! `uu check` — detect project type and typecheck without running tests.

use anyhow::{bail, Result};
use project_detect::{NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate check/typecheck steps for a detected project.
fn steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Cargo => Ok(vec![step("cargo", &["check"])]),
        ProjectKind::Go => Ok(vec![step("go", &["test", "-run=^$", "./..."])]),
        ProjectKind::Elixir { .. } => Ok(vec![step("mix", &["compile", "--warnings-as-errors"])]),
        ProjectKind::Python { .. } => bail!(
            "Python has no built-in typecheck\n\n  \
             try:\n    \
             mypy .       # pip install mypy\n    \
             pyright .    # pip install pyright"
        ),
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            Ok(vec![step(cmd, &["run", "typecheck"])])
        }
        ProjectKind::Gradle { wrapper: true } => {
            Ok(vec![step("./gradlew", &["build", "-x", "test"])])
        }
        ProjectKind::Gradle { wrapper: false } => {
            Ok(vec![step("gradle", &["build", "-x", "test"])])
        }
        ProjectKind::Maven => Ok(vec![step("mvn", &["-DskipTests", "package"])]),
        ProjectKind::Ruby => bail!(
            "Ruby has no built-in typecheck\n\n  \
             try: srb tc    # gem install sorbet"
        ),
        ProjectKind::Swift => Ok(vec![step("swift", &["build"])]),
        ProjectKind::DotNet { .. } => Ok(vec![step("dotnet", &["build", "--no-restore"])]),
        ProjectKind::Meson => Ok(vec![step("meson", &["compile", "-C", "builddir"])]),
        ProjectKind::CMake => Ok(vec![
            step("cmake", &["-B", "build"]),
            step("cmake", &["--build", "build"]),
        ]),
        ProjectKind::Zig => Ok(vec![step("zig", &["build"])]),
        ProjectKind::Make => Ok(vec![step("make", &[])]),
        ProjectKind::Dart { .. } => Ok(vec![step("dart", &["analyze"])]),
        ProjectKind::Sbt => Ok(vec![step("sbt", &["compile"])]),
        ProjectKind::Haskell { stack: true } => Ok(vec![step("stack", &["build", "--fast"])]),
        ProjectKind::Haskell { stack: false } => Ok(vec![step("cabal", &["build"])]),
        ProjectKind::Rebar => Ok(vec![step("rebar3", &["compile"])]),
        ProjectKind::Dune => Ok(vec![step("dune", &["build"])]),
        ProjectKind::Nim => Ok(vec![step("nimble", &["check"])]),
        ProjectKind::Crystal => Ok(vec![step("crystal", &["build", "--no-codegen"])]),
        ProjectKind::Vlang => Ok(vec![step("v", &["."])]),
        ProjectKind::Gleam => Ok(vec![step("gleam", &["check"])]),
        ProjectKind::Bazel => Ok(vec![step("bazel", &["build", "//..."])]),
        ProjectKind::Php | ProjectKind::Clojure { .. } | ProjectKind::Perl
        | ProjectKind::Julia | ProjectKind::Lua => {
            bail!("{} has no built-in typecheck", kind.label())
        }
    }
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
    use project_detect::NodePM;

    #[test]
    fn cargo_check() {
        let s = steps(&ProjectKind::Cargo).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["check"]);
    }

    #[test]
    fn go_check_compiles_without_running() {
        let s = steps(&ProjectKind::Go).unwrap();
        assert_eq!(s[0].program, "go");
        assert_eq!(s[0].args, ["test", "-run=^$", "./..."]);
    }

    #[test]
    fn elixir_check() {
        let s = steps(&ProjectKind::Elixir { escript: false }).unwrap();
        assert_eq!(s[0].program, "mix");
        assert_eq!(s[0].args, ["compile", "--warnings-as-errors"]);
    }

    #[test]
    fn python_unsupported() {
        assert!(steps(&ProjectKind::Python { uv: false }).is_err());
        assert!(steps(&ProjectKind::Python { uv: true }).is_err());
    }

    #[test]
    fn node_npm_typecheck() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Npm,
        })
        .unwrap();
        assert_eq!(s[0].program, "npm");
        assert_eq!(s[0].args, ["run", "typecheck"]);
    }

    #[test]
    fn node_yarn_typecheck() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Yarn,
        })
        .unwrap();
        assert_eq!(s[0].program, "yarn");
        assert_eq!(s[0].args, ["run", "typecheck"]);
    }

    #[test]
    fn gradle_wrapper_check() {
        let s = steps(&ProjectKind::Gradle { wrapper: true }).unwrap();
        assert_eq!(s[0].program, "./gradlew");
        assert_eq!(s[0].args, ["build", "-x", "test"]);
    }

    #[test]
    fn gradle_no_wrapper_check() {
        let s = steps(&ProjectKind::Gradle { wrapper: false }).unwrap();
        assert_eq!(s[0].program, "gradle");
        assert_eq!(s[0].args, ["build", "-x", "test"]);
    }

    #[test]
    fn maven_check() {
        let s = steps(&ProjectKind::Maven).unwrap();
        assert_eq!(s[0].program, "mvn");
        assert_eq!(s[0].args, ["-DskipTests", "package"]);
    }

    #[test]
    fn ruby_unsupported() {
        assert!(steps(&ProjectKind::Ruby).is_err());
    }

    #[test]
    fn swift_check() {
        let s = steps(&ProjectKind::Swift).unwrap();
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn dotnet_check() {
        let s = steps(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["build", "--no-restore"]);
    }

    #[test]
    fn meson_check() {
        let s = steps(&ProjectKind::Meson).unwrap();
        assert_eq!(s[0].program, "meson");
        assert_eq!(s[0].args, ["compile", "-C", "builddir"]);
    }

    #[test]
    fn cmake_check() {
        let s = steps(&ProjectKind::CMake).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].args, ["-B", "build"]);
        assert_eq!(s[1].args, ["--build", "build"]);
    }

    #[test]
    fn zig_check() {
        let s = steps(&ProjectKind::Zig).unwrap();
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build"]);
    }

    #[test]
    fn make_check() {
        let s = steps(&ProjectKind::Make).unwrap();
        assert_eq!(s[0].program, "make");
        assert!(s[0].args.is_empty());
    }
}
