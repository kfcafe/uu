//! `uu doctor` — show detected project and check required external tools.

use anyhow::Result;
use uu_detect::{command_on_path, detect, supported_table, NodePM, ProjectKind};

use crate::runner::style;

/// Return the list of external programs used by `uu` for a given project kind.
fn required_tools(kind: &ProjectKind) -> Vec<&'static str> {
    match kind {
        ProjectKind::Cargo => vec!["cargo", "rustfmt", "cargo-clippy"],
        ProjectKind::Go => vec!["go", "gofmt"],
        ProjectKind::Elixir => vec!["mix"],
        ProjectKind::Python { uv: true, .. } => vec!["uv", "python", "pytest", "ruff"],
        ProjectKind::Python { uv: false, .. } => vec!["pip", "python", "pytest", "ruff"],
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
        ProjectKind::Meson => vec!["meson", "ninja"],
        ProjectKind::CMake => vec!["cmake", "ctest"],
        ProjectKind::Make => vec!["make"],
    }
}

pub(crate) fn execute() -> Result<()> {
    let dir = std::env::current_dir()?;

    match detect(&dir) {
        Some(kind) => {
            eprintln!(
                "{} {} \x1b[2m({})\x1b[0m",
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
    }

    #[test]
    fn python_no_uv_uses_pip() {
        let tools = required_tools(&ProjectKind::Python { uv: false });
        assert!(tools.contains(&"pip"));
        assert!(!tools.contains(&"uv"));
    }

    #[test]
    fn gradle_wrapper_omits_gradle_binary() {
        let with = required_tools(&ProjectKind::Gradle { wrapper: true });
        let without = required_tools(&ProjectKind::Gradle { wrapper: false });
        assert!(!with.contains(&"gradle"));
        assert!(without.contains(&"gradle"));
    }
}
