//! Shared execution logic for project-aware commands.

use std::env;
use std::fmt;
use std::process::{self, Command, Stdio};

use anyhow::{Context, Result};
use uu_detect::{detect, supported_table, ProjectKind};

/// A single shell command to execute.
pub(crate) struct Step {
    pub program: String,
    pub args: Vec<String>,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            if arg.contains(' ') {
                write!(f, " '{arg}'")?;
            } else {
                write!(f, " {arg}")?;
            }
        }
        Ok(())
    }
}

/// Build a [`Step`] from a program name and argument slices.
pub(crate) fn step(program: &str, args: &[&str]) -> Step {
    Step {
        program: program.to_owned(),
        args: args.iter().map(|&s| s.to_owned()).collect(),
    }
}

/// Detect the project kind in the current working directory.
pub(crate) fn detect_project() -> Result<ProjectKind> {
    let dir = env::current_dir().context("failed to read current directory")?;
    detect(&dir).ok_or_else(|| {
        anyhow::anyhow!(
            "no recognized project in {}\n\n{}",
            dir.display(),
            supported_table()
        )
    })
}

/// Append extra CLI arguments to the last step in a sequence.
pub(crate) fn append_args(steps: &mut [Step], extra_args: &[String]) {
    if let Some(last) = steps.last_mut() {
        last.args.extend(extra_args.iter().cloned());
    }
}

/// Print the detected project, then execute each step (or print in dry-run mode).
///
/// Exits with the failing step's exit code on failure.
pub(crate) fn run_steps(kind: &ProjectKind, steps: &[Step], dry_run: bool) -> Result<()> {
    eprintln!(
        "{} {} \x1b[2m({})\x1b[0m",
        style("36", "detected"),
        kind.label(),
        kind.detected_file()
    );

    for s in steps {
        if dry_run {
            eprintln!("{} {s}", style("33", "would run"));
        } else {
            eprintln!("{} {s}", style("32", "running"));

            let status = Command::new(&s.program)
                .args(&s.args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .with_context(|| {
                    format!(
                        "`{}` not found — is it installed and in your PATH?",
                        s.program
                    )
                })?;

            if !status.success() {
                process::exit(status.code().unwrap_or(1));
            }
        }
    }

    if !dry_run {
        eprintln!("{}", style("32", "done ✓"));
    }

    Ok(())
}

/// Right-aligned, colored label for terminal output.
pub(crate) fn style(color: &str, label: &str) -> String {
    format!("  \x1b[1;{color}m{label:>10}\x1b[0m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_display_simple() {
        let s = step("cargo", &["install", "--path", "."]);
        assert_eq!(s.to_string(), "cargo install --path .");
    }

    #[test]
    fn step_display_quotes_spaces() {
        let s = step("cargo", &["install", "--path", "my project"]);
        assert_eq!(s.to_string(), "cargo install --path 'my project'");
    }

    #[test]
    fn append_args_extends_last_step() {
        let mut steps = vec![step("make", &[]), step("make", &["install"])];
        append_args(&mut steps, &["DESTDIR=/tmp".to_owned()]);
        assert_eq!(steps[0].args, Vec::<String>::new());
        assert_eq!(steps[1].args, ["install", "DESTDIR=/tmp"]);
    }

    #[test]
    fn append_args_noop_on_empty() {
        let mut steps: Vec<Step> = vec![];
        append_args(&mut steps, &["--flag".to_owned()]);
        assert!(steps.is_empty());
    }
}
