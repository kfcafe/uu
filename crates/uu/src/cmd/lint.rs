//! `uu lint` — detect project type and run the linter.

use anyhow::{bail, Result};
use uu_detect::{command_on_path, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate lint steps for a detected project.
fn steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Cargo => Ok(vec![step("cargo", &["clippy", "--", "-D", "warnings"])]),
        ProjectKind::Go => Ok(vec![step("go", &["vet", "./..."])]),
        ProjectKind::Elixir => Ok(vec![step("mix", &["compile", "--warnings-as-errors"])]),
        ProjectKind::Python { uv: true, .. } => {
            if command_on_path("ruff") {
                Ok(vec![step("uv", &["run", "ruff", "check", "."])])
            } else if command_on_path("flake8") {
                Ok(vec![step("uv", &["run", "flake8"])])
            } else {
                bail!(
                    "no Python linter found\n\n  \
                     install one and try again:\n    \
                     uv pip install ruff    # recommended\n    \
                     uv pip install flake8"
                )
            }
        }
        ProjectKind::Python { uv: false, .. } => {
            if command_on_path("ruff") {
                Ok(vec![step("ruff", &["check", "."])])
            } else if command_on_path("flake8") {
                Ok(vec![step("flake8", &[])])
            } else {
                bail!(
                    "no Python linter found\n\n  \
                     install one and try again:\n    \
                     pip install ruff    # recommended\n    \
                     pip install flake8"
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
            Ok(vec![step(cmd, &["run", "lint"])])
        }
        ProjectKind::Gradle { wrapper: true } => Ok(vec![step("./gradlew", &["check"])]),
        ProjectKind::Gradle { wrapper: false } => Ok(vec![step("gradle", &["check"])]),
        ProjectKind::Maven => bail!(
            "Maven has no built-in linter\n\n  \
             try: mvn checkstyle:check"
        ),
        ProjectKind::Ruby => Ok(vec![step("bundle", &["exec", "rubocop"])]),
        ProjectKind::Meson => bail!("Meson has no built-in linter"),
        ProjectKind::CMake => bail!(
            "CMake has no built-in linter\n\n  \
             try: cmake-lint CMakeLists.txt"
        ),
        ProjectKind::Make => bail!("Make has no built-in linter"),
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
    fn cargo_lint() {
        let s = steps(&ProjectKind::Cargo).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["clippy", "--", "-D", "warnings"]);
    }

    #[test]
    fn go_lint() {
        let s = steps(&ProjectKind::Go).unwrap();
        assert_eq!(s[0].program, "go");
        assert_eq!(s[0].args, ["vet", "./..."]);
    }

    #[test]
    fn elixir_lint() {
        let s = steps(&ProjectKind::Elixir).unwrap();
        assert_eq!(s[0].program, "mix");
        assert_eq!(s[0].args, ["compile", "--warnings-as-errors"]);
    }

    #[test]
    fn node_npm_lint() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Npm,
        })
        .unwrap();
        assert_eq!(s[0].program, "npm");
        assert_eq!(s[0].args, ["run", "lint"]);
    }

    #[test]
    fn node_yarn_lint() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Yarn,
        })
        .unwrap();
        assert_eq!(s[0].program, "yarn");
        assert_eq!(s[0].args, ["run", "lint"]);
    }

    #[test]
    fn gradle_wrapper_lint() {
        let s = steps(&ProjectKind::Gradle { wrapper: true }).unwrap();
        assert_eq!(s[0].program, "./gradlew");
        assert_eq!(s[0].args, ["check"]);
    }

    #[test]
    fn ruby_lint() {
        let s = steps(&ProjectKind::Ruby).unwrap();
        assert_eq!(s[0].program, "bundle");
        assert_eq!(s[0].args, ["exec", "rubocop"]);
    }

    #[test]
    fn maven_unsupported() {
        assert!(steps(&ProjectKind::Maven).is_err());
    }

    #[test]
    fn cmake_unsupported() {
        assert!(steps(&ProjectKind::CMake).is_err());
    }

    #[test]
    fn make_unsupported() {
        assert!(steps(&ProjectKind::Make).is_err());
    }

    #[test]
    fn meson_unsupported() {
        assert!(steps(&ProjectKind::Meson).is_err());
    }
}
