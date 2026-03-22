//! `uu build` — detect project type and build the project.

use anyhow::Result;
use project_detect::{NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate build steps for a detected project.
fn steps(kind: &ProjectKind) -> Vec<Step> {
    match kind {
        ProjectKind::Cargo => vec![step("cargo", &["build"])],
        ProjectKind::Go => vec![step("go", &["build", "./..."])],
        ProjectKind::Elixir { escript: true } => vec![step("mix", &["escript.build"])],
        ProjectKind::Elixir { escript: false } => vec![step("mix", &["compile"])],
        ProjectKind::Python { uv: true, .. } => vec![step("uv", &["run", "python", "-m", "build"])],
        ProjectKind::Python { uv: false, .. } => vec![step("python", &["-m", "build"])],
        ProjectKind::Node { manager } => {
            let cmd = match manager {
                NodePM::Bun => "bun",
                NodePM::Pnpm => "pnpm",
                NodePM::Yarn => "yarn",
                NodePM::Npm => "npm",
            };
            vec![step(cmd, &["run", "build"])]
        }
        ProjectKind::Gradle { wrapper: true } => vec![step("./gradlew", &["build"])],
        ProjectKind::Gradle { wrapper: false } => vec![step("gradle", &["build"])],
        ProjectKind::Maven => vec![step("mvn", &["package"])],
        ProjectKind::Ruby => vec![step("bundle", &["exec", "rake", "build"])],
        ProjectKind::Swift => vec![step("swift", &["build"])],
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
    }
}

pub(crate) fn execute(dry_run: bool, extra_args: Vec<String>) -> Result<()> {
    let kind = runner::detect_project()?;

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
    use project_detect::NodePM;

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
