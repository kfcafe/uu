//! `uu install` — detect project type and run the install command.

use anyhow::Result;
use uu_detect::{NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate install steps for a detected project.
fn steps(kind: &ProjectKind) -> Vec<Step> {
    match kind {
        ProjectKind::Cargo => vec![step("cargo", &["install", "--path", "."])],
        ProjectKind::Go => vec![step("go", &["install", "./..."])],
        ProjectKind::Elixir => vec![step("mix", &["deps.get"]), step("mix", &["compile"])],
        ProjectKind::Python { uv: true } => vec![step("uv", &["pip", "install", "."])],
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
        ProjectKind::Make => vec![step("make", &[]), step("make", &["install"])],
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
    fn cmake_has_three_phases() {
        let s = steps(&ProjectKind::CMake);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn python_prefers_uv() {
        let s = steps(&ProjectKind::Python { uv: true });
        assert_eq!(s[0].program, "uv");
    }
}
