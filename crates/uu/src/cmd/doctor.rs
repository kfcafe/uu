//! `uu doctor` — show detected project and check required external tools.

use anyhow::Result;
use project_detect::{command_on_path, detect_walk, supported_table, NodePM, ProjectKind};

use crate::runner::{self, style};

/// Return the list of external programs used by `uu` for a given project kind.
fn required_tools(kind: &ProjectKind) -> Vec<&'static str> {
    match kind {
        ProjectKind::Cargo => vec!["cargo", "rustfmt", "cargo-clippy"],
        ProjectKind::Go => vec!["go", "gofmt"],
        ProjectKind::Elixir { .. } => vec!["mix"],
        ProjectKind::Python { uv: true, .. } => vec!["uv", runner::python_cmd(), "pytest", "ruff"],
        ProjectKind::Python { uv: false, .. } => vec!["pip", runner::python_cmd(), "pytest", "ruff"],
        ProjectKind::Node { manager } => match manager {
            NodePM::Bun => vec!["bun"],
            NodePM::Pnpm => vec!["pnpm"],
            NodePM::Yarn => vec!["yarn"],
            NodePM::Npm => vec!["npm"],
        },
        ProjectKind::Gradle { wrapper: true } => vec!["java"],
        ProjectKind::Gradle { wrapper: false } => vec!["gradle", "java"],
        ProjectKind::Maven => vec!["mvn", "java"],
        ProjectKind::Ruby => vec!["bundle", "ruby", "rubocop"],
        ProjectKind::Swift => vec!["swift"],
        ProjectKind::DotNet { .. } => vec!["dotnet"],
        ProjectKind::Meson => vec!["meson", "ninja"],
        ProjectKind::CMake => vec!["cmake", "ctest"],
        ProjectKind::Zig => vec!["zig"],
        ProjectKind::Make => vec!["make"],
        ProjectKind::Php => vec!["php", "composer"],
        ProjectKind::Dart { flutter: true } => vec!["flutter", "dart"],
        ProjectKind::Dart { flutter: false } => vec!["dart"],
        ProjectKind::Sbt => vec!["sbt", "java"],
        ProjectKind::Haskell { stack: true } => vec!["stack", "ghc"],
        ProjectKind::Haskell { stack: false } => vec!["cabal", "ghc"],
        ProjectKind::Clojure { lein: true } => vec!["lein", "java"],
        ProjectKind::Clojure { lein: false } => vec!["clj", "java"],
        ProjectKind::Rebar => vec!["rebar3", "erl"],
        ProjectKind::Dune => vec!["dune", "ocaml"],
        ProjectKind::Perl => vec!["perl", "cpanm"],
        ProjectKind::Julia => vec!["julia"],
        ProjectKind::Nim => vec!["nim", "nimble"],
        ProjectKind::Crystal => vec!["crystal", "shards"],
        ProjectKind::Vlang => vec!["v"],
        ProjectKind::Gleam => vec!["gleam"],
        ProjectKind::Lua => vec!["lua", "luarocks"],
        ProjectKind::Bazel => vec!["bazel"],
    }
}

pub(crate) fn execute() -> Result<()> {
    let dir = std::env::current_dir()?;

    match detect_walk(&dir) {
        Some((kind, project_dir)) => {
            let note = if project_dir != dir {
                format!(" in {}", project_dir.display())
            } else {
                String::new()
            };
            eprintln!(
                "{} {} \x1b[2m({})\x1b[0m{note}",
                style("36", "detected"),
                kind.label(),
                kind.detected_file()
            );
            eprintln!();

            let tools = required_tools(&kind);
            for tool in &tools {
                let found = command_on_path(tool);
                let icon = if found { "✓" } else { "✗" };
                let color = if found { "32" } else { "31" };
                eprintln!("  \x1b[1;{color}m{icon}\x1b[0m {tool}",);
            }
        }
        None => {
            eprintln!(
                "{} no project detected in {}",
                style("33", "note"),
                dir.display()
            );
            eprintln!();
            eprint!("{}", supported_table());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_tools_include_basics() {
        let tools = required_tools(&ProjectKind::Cargo);
        assert!(tools.contains(&"cargo"));
        assert!(tools.contains(&"rustfmt"));
        assert!(tools.contains(&"cargo-clippy"));
    }

    #[test]
    fn go_tools_include_gofmt() {
        let tools = required_tools(&ProjectKind::Go);
        assert!(tools.contains(&"go"));
        assert!(tools.contains(&"gofmt"));
    }

    #[test]
    fn node_tools_match_manager() {
        let tools = required_tools(&ProjectKind::Node {
            manager: NodePM::Yarn,
        });
        assert_eq!(tools, vec!["yarn"]);
    }

    #[test]
    fn python_uv_prefers_uv() {
        let tools = required_tools(&ProjectKind::Python { uv: true });
        assert!(tools.contains(&"uv"));
        assert!(!tools.contains(&"pip"));
        let py = runner::python_cmd();
        assert!(tools.contains(&py));
    }

    #[test]
    fn python_no_uv_uses_pip() {
        let tools = required_tools(&ProjectKind::Python { uv: false });
        assert!(tools.contains(&"pip"));
        assert!(!tools.contains(&"uv"));
        let py = runner::python_cmd();
        assert!(tools.contains(&py));
    }

    #[test]
    fn swift_tools() {
        let tools = required_tools(&ProjectKind::Swift);
        assert_eq!(tools, vec!["swift"]);
    }

    #[test]
    fn dotnet_tools() {
        let tools = required_tools(&ProjectKind::DotNet { sln: false });
        assert_eq!(tools, vec!["dotnet"]);
    }

    #[test]
    fn zig_tools() {
        let tools = required_tools(&ProjectKind::Zig);
        assert_eq!(tools, vec!["zig"]);
    }

    #[test]
    fn gradle_wrapper_omits_gradle_binary() {
        let with = required_tools(&ProjectKind::Gradle { wrapper: true });
        let without = required_tools(&ProjectKind::Gradle { wrapper: false });
        assert!(!with.contains(&"gradle"));
        assert!(without.contains(&"gradle"));
    }
}
