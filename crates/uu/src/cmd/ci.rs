//! `uu ci` — detect project type and run a CI verification pipeline.
//!
//! Runs format-check + lint + test (where available) in a single command.
//! Designed for CI gates and pre-push checks.

use anyhow::{bail, Result};
use uu_detect::{command_on_path, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate CI pipeline steps for a detected project.
fn steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Cargo => Ok(vec![
            step("cargo", &["fmt", "--check"]),
            step("cargo", &["clippy", "--", "-D", "warnings"]),
            step("cargo", &["test"]),
        ]),
        ProjectKind::Go => Ok(vec![
            step("gofmt", &["-l", "."]),
            step("go", &["vet", "./..."]),
            step("go", &["test", "./..."]),
        ]),
        ProjectKind::Elixir { .. } => Ok(vec![
            step("mix", &["format", "--check-formatted"]),
            step("mix", &["compile", "--warnings-as-errors"]),
            step("mix", &["test"]),
        ]),
        ProjectKind::Python { uv: true, .. } => {
            if command_on_path("ruff") {
                Ok(vec![
                    step("uv", &["run", "ruff", "format", "--check", "."]),
                    step("uv", &["run", "ruff", "check", "."]),
                    step("uv", &["run", "pytest"]),
                ])
            } else if command_on_path("black") {
                Ok(vec![
                    step("uv", &["run", "black", "--check", "."]),
                    step("uv", &["run", "pytest"]),
                ])
            } else {
                bail!(
                    "no Python formatter/linter found\n\n  \
                     install one and try again:\n    \
                     uv pip install ruff   # recommended\n    \
                     uv pip install black"
                )
            }
        }
        ProjectKind::Python { uv: false, .. } => {
            if command_on_path("ruff") {
                Ok(vec![
                    step("ruff", &["format", "--check", "."]),
                    step("ruff", &["check", "."]),
                    step("pytest", &[]),
                ])
            } else if command_on_path("black") {
                Ok(vec![step("black", &["--check", "."]), step("pytest", &[])])
            } else {
                bail!(
                    "no Python formatter/linter found\n\n  \
                     install one and try again:\n    \
                     pip install ruff    # recommended\n    \
                     pip install black"
                )
            }
        }
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            Ok(vec![step(cmd, &["run", "lint"]), step(cmd, &["test"])])
        }
        ProjectKind::Gradle { wrapper: true } => Ok(vec![step("./gradlew", &["check"])]),
        ProjectKind::Gradle { wrapper: false } => Ok(vec![step("gradle", &["check"])]),
        ProjectKind::Maven => Ok(vec![step("mvn", &["test"])]),
        ProjectKind::Ruby => Ok(vec![step("bundle", &["exec", "rake", "test"])]),
        ProjectKind::Swift => Ok(vec![step("swift", &["build"]), step("swift", &["test"])]),
        ProjectKind::DotNet { .. } => Ok(vec![
            step("dotnet", &["format", "--verify-no-changes"]),
            step("dotnet", &["build"]),
            step("dotnet", &["test"]),
        ]),
        ProjectKind::Meson => Ok(vec![step("meson", &["test", "-C", "builddir"])]),
        ProjectKind::CMake => Ok(vec![step("ctest", &["--test-dir", "build"])]),
        ProjectKind::Make => Ok(vec![step("make", &["test"])]),
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
    use uu_detect::NodePM;

    #[test]
    fn cargo_ci_has_three_steps() {
        let s = steps(&ProjectKind::Cargo).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].to_string(), "cargo fmt --check");
        assert_eq!(s[1].to_string(), "cargo clippy -- -D warnings");
        assert_eq!(s[2].to_string(), "cargo test");
    }

    #[test]
    fn go_ci_has_three_steps() {
        let s = steps(&ProjectKind::Go).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].program, "gofmt");
        assert_eq!(s[0].args, ["-l", "."]);
        assert_eq!(s[1].to_string(), "go vet ./...");
        assert_eq!(s[2].to_string(), "go test ./...");
    }

    #[test]
    fn elixir_ci_has_three_steps() {
        let s = steps(&ProjectKind::Elixir { escript: false }).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].to_string(), "mix format --check-formatted");
        assert_eq!(s[1].to_string(), "mix compile --warnings-as-errors");
        assert_eq!(s[2].to_string(), "mix test");
    }

    #[test]
    fn node_npm_ci() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Npm,
        })
        .unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].to_string(), "npm run lint");
        assert_eq!(s[1].to_string(), "npm test");
    }

    #[test]
    fn node_yarn_ci() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Yarn,
        })
        .unwrap();
        assert_eq!(s[0].program, "yarn");
    }

    #[test]
    fn gradle_wrapper_ci() {
        let s = steps(&ProjectKind::Gradle { wrapper: true }).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].to_string(), "./gradlew check");
    }

    #[test]
    fn maven_ci() {
        let s = steps(&ProjectKind::Maven).unwrap();
        assert_eq!(s[0].to_string(), "mvn test");
    }

    #[test]
    fn ruby_ci() {
        let s = steps(&ProjectKind::Ruby).unwrap();
        assert_eq!(s[0].to_string(), "bundle exec rake test");
    }

    #[test]
    fn swift_ci() {
        let s = steps(&ProjectKind::Swift).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].to_string(), "swift build");
        assert_eq!(s[1].to_string(), "swift test");
    }

    #[test]
    fn dotnet_ci() {
        let s = steps(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].to_string(), "dotnet format --verify-no-changes");
        assert_eq!(s[1].to_string(), "dotnet build");
        assert_eq!(s[2].to_string(), "dotnet test");
    }

    #[test]
    fn meson_ci() {
        let s = steps(&ProjectKind::Meson).unwrap();
        assert_eq!(s[0].to_string(), "meson test -C builddir");
    }

    #[test]
    fn cmake_ci() {
        let s = steps(&ProjectKind::CMake).unwrap();
        assert_eq!(s[0].to_string(), "ctest --test-dir build");
    }

    #[test]
    fn make_ci() {
        let s = steps(&ProjectKind::Make).unwrap();
        assert_eq!(s[0].to_string(), "make test");
    }
}
