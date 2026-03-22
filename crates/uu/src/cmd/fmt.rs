//! `uu fmt` — detect project type and run the formatter.

use anyhow::{bail, Result};
use project_detect::{command_on_path, NodePM, ProjectKind};

use crate::runner::{self, step, Step};

/// Generate format steps for a detected project.
fn steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Cargo => Ok(vec![step("cargo", &["fmt"])]),
        ProjectKind::Go => Ok(vec![step("gofmt", &["-w", "."])]),
        ProjectKind::Elixir { .. } => Ok(vec![step("mix", &["format"])]),
        ProjectKind::Python { uv: true, .. } => {
            if command_on_path("ruff") {
                Ok(vec![step("uv", &["run", "ruff", "format", "."])])
            } else if command_on_path("black") {
                Ok(vec![step("uv", &["run", "black", "."])])
            } else {
                bail!(
                    "no Python formatter found\n\n  \
                     install one and try again:\n    \
                     uv pip install ruff   # recommended\n    \
                     uv pip install black"
                )
            }
        }
        ProjectKind::Python { uv: false, .. } => {
            if command_on_path("ruff") {
                Ok(vec![step("ruff", &["format", "."])])
            } else if command_on_path("black") {
                Ok(vec![step("black", &["."])])
            } else {
                bail!(
                    "no Python formatter found\n\n  \
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
            Ok(vec![step(cmd, &["run", "format"])])
        }
        ProjectKind::Gradle { wrapper: true } => Ok(vec![step("./gradlew", &["spotlessApply"])]),
        ProjectKind::Gradle { wrapper: false } => Ok(vec![step("gradle", &["spotlessApply"])]),
        ProjectKind::Maven => bail!(
            "Maven has no built-in formatter\n\n  \
             try: mvn com.spotify.fmt:fmt-maven-plugin:format"
        ),
        ProjectKind::Ruby => Ok(vec![step("bundle", &["exec", "rubocop", "-a"])]),
        ProjectKind::Swift => bail!(
            "Swift has no built-in formatter\n\n  \
             try: swift-format format -i -r .    # brew install swift-format"
        ),
        ProjectKind::DotNet { .. } => Ok(vec![step("dotnet", &["format"])]),
        ProjectKind::Meson => bail!(
            "Meson has no built-in formatter\n\n  \
             try: muon fmt meson.build"
        ),
        ProjectKind::CMake => bail!(
            "CMake has no built-in formatter\n\n  \
             try: cmake-format -i CMakeLists.txt"
        ),
        ProjectKind::Zig => Ok(vec![step("zig", &["fmt", "."])]),
        ProjectKind::Make => bail!("Make has no built-in formatter"),
        ProjectKind::Dart { .. } => Ok(vec![step("dart", &["format", "."])]),
        ProjectKind::Sbt => Ok(vec![step("sbt", &["scalafmtAll"])]),
        ProjectKind::Dune => Ok(vec![step("dune", &["fmt"])]),
        ProjectKind::Crystal => Ok(vec![step("crystal", &["tool", "format", "."])]),
        ProjectKind::Vlang => Ok(vec![step("v", &["fmt", "."])]),
        ProjectKind::Gleam => Ok(vec![step("gleam", &["format"])]),
        ProjectKind::Php => bail!("PHP has no built-in formatter\n\n  try: vendor/bin/php-cs-fixer fix"),
        ProjectKind::Haskell { .. } => bail!("Haskell has no built-in formatter\n\n  try: ormolu -i **/*.hs"),
        ProjectKind::Clojure { .. } => bail!("Clojure has no built-in formatter\n\n  try: lein cljfmt fix"),
        ProjectKind::Rebar => bail!("Erlang has no built-in formatter\n\n  try: rebar3 fmt"),
        ProjectKind::Perl => bail!("Perl has no built-in formatter\n\n  try: perltidy -b *.pl"),
        ProjectKind::Julia => bail!("Julia has no built-in formatter\n\n  try: using JuliaFormatter; format(\".\")"),
        ProjectKind::Nim => bail!("Nim has no built-in formatter\n\n  try: nimpretty *.nim"),
        ProjectKind::Lua => bail!("Lua has no built-in formatter\n\n  try: stylua ."),
        ProjectKind::Bazel => Ok(vec![step("buildifier", &["."])]),
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
    fn cargo_fmt() {
        let s = steps(&ProjectKind::Cargo).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["fmt"]);
    }

    #[test]
    fn go_fmt() {
        let s = steps(&ProjectKind::Go).unwrap();
        assert_eq!(s[0].program, "gofmt");
        assert_eq!(s[0].args, ["-w", "."]);
    }

    #[test]
    fn elixir_format() {
        let s = steps(&ProjectKind::Elixir { escript: false }).unwrap();
        assert_eq!(s[0].program, "mix");
        assert_eq!(s[0].args, ["format"]);
    }

    #[test]
    fn node_npm_format() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Npm,
        })
        .unwrap();
        assert_eq!(s[0].program, "npm");
        assert_eq!(s[0].args, ["run", "format"]);
    }

    #[test]
    fn node_yarn_format() {
        let s = steps(&ProjectKind::Node {
            manager: NodePM::Yarn,
        })
        .unwrap();
        assert_eq!(s[0].program, "yarn");
        assert_eq!(s[0].args, ["run", "format"]);
    }

    #[test]
    fn gradle_wrapper_format() {
        let s = steps(&ProjectKind::Gradle { wrapper: true }).unwrap();
        assert_eq!(s[0].program, "./gradlew");
        assert_eq!(s[0].args, ["spotlessApply"]);
    }

    #[test]
    fn swift_unsupported() {
        assert!(steps(&ProjectKind::Swift).is_err());
    }

    #[test]
    fn dotnet_fmt() {
        let s = steps(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["format"]);
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
    fn zig_fmt() {
        let s = steps(&ProjectKind::Zig).unwrap();
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["fmt", "."]);
    }

    #[test]
    fn make_unsupported() {
        assert!(steps(&ProjectKind::Make).is_err());
    }
}
