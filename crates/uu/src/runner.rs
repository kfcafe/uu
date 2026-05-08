//! Shared execution logic for project-aware commands.

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};

use anyhow::{Context, Result};
use project_detect::{detect_walk, supported_table, ProjectKind};

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

/// Resolve the Python interpreter: prefer `python3` (always present on macOS/Homebrew),
/// fall back to `python`.
pub(crate) fn python_cmd() -> &'static str {
    use std::sync::OnceLock;
    static CMD: OnceLock<&str> = OnceLock::new();
    CMD.get_or_init(|| {
        let found = Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        if found {
            "python3"
        } else {
            "python"
        }
    })
}

/// Build a [`Step`] from a program name and argument slices.
pub(crate) fn step(program: &str, args: &[&str]) -> Step {
    Step {
        program: program.to_owned(),
        args: args.iter().map(|&s| s.to_owned()).collect(),
    }
}

/// A detected project and the directory where its marker file was found.
pub(crate) struct DetectedProject {
    pub kind: ProjectKind,
    pub dir: PathBuf,
}

/// Detect the project kind, walking up from the current directory.
///
/// If the project file is found in a parent directory, changes the working
/// directory to that parent so commands run in the right place.
pub(crate) fn detect_project() -> Result<ProjectKind> {
    let project = detect_project_with_dir()?;
    if project.dir != env::current_dir().context("failed to read current directory")? {
        env::set_current_dir(&project.dir).with_context(|| {
            format!(
                "cannot change to detected project root `{}`",
                project.dir.display()
            )
        })?;
    }
    Ok(project.kind)
}

/// Detect the project kind and the directory where it was found.
pub(crate) fn detect_project_with_dir() -> Result<DetectedProject> {
    let dir = env::current_dir().context("failed to read current directory")?;
    let (kind, project_dir) = detect_walk(&dir).ok_or_else(|| {
        anyhow::anyhow!(
            "no recognized project in {}\n\n{}",
            dir.display(),
            supported_table()
        )
    })?;

    Ok(DetectedProject {
        kind,
        dir: project_dir,
    })
}

/// Change to `dir` and run `f`, then restore the original working directory.
pub(crate) fn with_current_dir<T>(dir: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let original = env::current_dir().context("failed to read current directory")?;
    env::set_current_dir(dir)
        .with_context(|| format!("cannot change to directory `{}`", dir.display()))?;
    let result = f();
    env::set_current_dir(&original)
        .with_context(|| format!("cannot restore working directory `{}`", original.display()))?;
    result
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
