//! `uu dev` — start dev servers, workspace-aware.

use std::env;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use project_detect::{detect_node_workspace, NodePM, ProjectKind, WorkspacePackage};

use crate::runner::{self, step, Step};

/// Color codes for package output prefixes, cycled round-robin.
const COLORS: &[&str] = &["36", "33", "35", "32", "34"];

/// Map a Node package manager to its CLI command.
fn manager_cmd(pm: &NodePM) -> &'static str {
    match pm {
        NodePM::Bun => "bun",
        NodePM::Pnpm => "pnpm",
        NodePM::Yarn => "yarn",
        NodePM::Npm => "npm",
    }
}

/// Generate steps for single-project (non-workspace) dev mode.
///
/// Node projects run `{manager} run dev`. Non-Node projects fall back to the
/// same command `uu run` would use.
fn single_dev_steps(kind: &ProjectKind) -> Result<Vec<Step>> {
    match kind {
        ProjectKind::Node { manager } => Ok(vec![step(manager_cmd(manager), &["run", "dev"])]),
        ProjectKind::Cargo => Ok(vec![step("cargo", &["run"])]),
        ProjectKind::Go => Ok(vec![step("go", &["run", "."])]),
        ProjectKind::Elixir { .. } => Ok(vec![step("mix", &["run"])]),
        ProjectKind::Python { uv } => python_dev_steps(*uv),
        ProjectKind::Gradle { wrapper: true } => Ok(vec![step("./gradlew", &["run"])]),
        ProjectKind::Gradle { wrapper: false } => Ok(vec![step("gradle", &["run"])]),
        ProjectKind::Maven => Ok(vec![step("mvn", &["compile", "exec:java"])]),
        ProjectKind::Ruby => Ok(vec![step("bundle", &["exec", "ruby", "app.rb"])]),
        ProjectKind::Swift => Ok(vec![step("swift", &["run"])]),
        ProjectKind::DotNet { .. } => Ok(vec![step("dotnet", &["watch", "run"])]),
        ProjectKind::Zig => Ok(vec![step("zig", &["build", "run"])]),
        ProjectKind::Make => Ok(vec![step("make", &["run"])]),
        ProjectKind::Meson | ProjectKind::CMake => {
            bail!(
                "uu can't auto-dev {} projects — use `uu run` after building",
                kind.label()
            )
        }
        ProjectKind::Php => Ok(vec![step("php", &["-S", "localhost:8000"])]),
        ProjectKind::Dart { flutter: true } => Ok(vec![step("flutter", &["run"])]),
        ProjectKind::Dart { flutter: false } => Ok(vec![step("dart", &["run"])]),
        ProjectKind::Sbt => Ok(vec![step("sbt", &["~run"])]),
        ProjectKind::Haskell { stack: true } => Ok(vec![step("stack", &["run"])]),
        ProjectKind::Haskell { stack: false } => Ok(vec![step("cabal", &["run"])]),
        ProjectKind::Clojure { lein: true } => Ok(vec![step("lein", &["run"])]),
        ProjectKind::Clojure { lein: false } => Ok(vec![step("clj", &["-M", "-m", "main"])]),
        ProjectKind::Rebar => Ok(vec![step("rebar3", &["shell"])]),
        ProjectKind::Dune => Ok(vec![step("dune", &["exec", "."])]),
        ProjectKind::Perl => Ok(vec![step("perl", &["app.pl"])]),
        ProjectKind::Julia => Ok(vec![step("julia", &["--project", "src/main.jl"])]),
        ProjectKind::Nim => Ok(vec![step("nimble", &["run"])]),
        ProjectKind::Crystal => Ok(vec![step("crystal", &["run", "src/main.cr"])]),
        ProjectKind::Vlang => Ok(vec![step("v", &["run", "."])]),
        ProjectKind::Gleam => Ok(vec![step("gleam", &["run"])]),
        ProjectKind::Lua => Ok(vec![step("lua", &["init.lua"])]),
        ProjectKind::Bazel => Ok(vec![step("bazel", &["run", "//:main"])]),
    }
}

/// Detect Python entry point and return the dev command.
fn python_dev_steps(uv: bool) -> Result<Vec<Step>> {
    let dir = env::current_dir().context("failed to read current directory")?;
    let (cmd, prefix): (&str, &[&str]) = if uv {
        ("uv", &["run"] as &[&str])
    } else {
        (runner::python_cmd(), &[] as &[&str])
    };

    if dir.join("manage.py").exists() {
        let mut args: Vec<&str> = prefix.to_vec();
        args.extend(["manage.py", "runserver"]);
        return Ok(vec![step(cmd, &args)]);
    }
    if dir.join("app.py").exists() {
        let mut args: Vec<&str> = prefix.to_vec();
        args.push("app.py");
        return Ok(vec![step(cmd, &args)]);
    }
    if dir.join("main.py").exists() {
        let mut args: Vec<&str> = prefix.to_vec();
        args.push("main.py");
        return Ok(vec![step(cmd, &args)]);
    }

    bail!("no Python entry point found — create main.py, app.py, or manage.py")
}

pub(crate) fn execute(
    dry_run: bool,
    open: bool,
    packages: Vec<String>,
    extra_args: Vec<String>,
) -> Result<()> {
    let kind = runner::detect_project()?;

    // Try workspace mode for Node.js projects.
    if let ProjectKind::Node { ref manager } = kind {
        let dir = env::current_dir().context("failed to read current directory")?;
        if let Some(ws_packages) = detect_node_workspace(&dir) {
            if !ws_packages.is_empty() {
                let selected = if packages.is_empty() {
                    ws_packages
                } else {
                    filter_packages(&ws_packages, &packages)?
                };
                return run_concurrent(&kind, &selected, manager_cmd(manager), dry_run, open);
            }
        }
    }

    // Fall back to single-project dev.
    let mut s = single_dev_steps(&kind)?;
    runner::append_args(&mut s, &extra_args);
    runner::run_steps(&kind, &s, dry_run)
}

/// Filter workspace packages to the requested names.
///
/// Bails with a helpful error listing available packages if any name is
/// unrecognized.
fn filter_packages(
    all: &[WorkspacePackage],
    requested: &[String],
) -> Result<Vec<WorkspacePackage>> {
    let mut selected = Vec::new();
    let mut unknown = Vec::new();

    for name in requested {
        match all.iter().find(|p| p.name == *name) {
            Some(pkg) => selected.push(pkg.clone()),
            None => unknown.push(name.as_str()),
        }
    }

    if !unknown.is_empty() {
        let available: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        bail!(
            "unknown package(s): {}\n  available: {}",
            unknown.join(", "),
            available.join(", ")
        );
    }

    Ok(selected)
}

/// Try to extract a localhost URL from a line of output.
fn extract_localhost_url(line: &str) -> Option<String> {
    // Match http://localhost:PORT or http://127.0.0.1:PORT, with optional path
    let prefixes = ["http://localhost:", "http://127.0.0.1:"];
    for prefix in prefixes {
        if let Some(start) = line.find(prefix) {
            let url_start = start;
            let rest = &line[url_start..];
            // Take until whitespace or end
            let url: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            return Some(url);
        }
    }
    None
}

/// Open a URL in the default browser.
fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "start";

    let _ = Command::new(cmd)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Run dev scripts for multiple workspace packages concurrently.
fn run_concurrent(
    kind: &ProjectKind,
    packages: &[WorkspacePackage],
    cmd: &str,
    dry_run: bool,
    open: bool,
) -> Result<()> {
    eprintln!(
        "{} {} workspace ({}, {} packages)",
        runner::style("36", "detected"),
        kind.label(),
        cmd,
        packages.len(),
    );

    if dry_run {
        for pkg in packages {
            eprintln!(
                "{} {} · {}",
                runner::style("33", "would run"),
                pkg.name,
                pkg.dev_script,
            );
        }
        return Ok(());
    }

    let max_width = packages.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let shutdown = Arc::new(AtomicBool::new(false));
    let opened = Arc::new(AtomicBool::new(false));

    {
        let flag = Arc::clone(&shutdown);
        ctrlc::set_handler(move || {
            flag.store(true, Ordering::SeqCst);
            eprintln!("\n{}", runner::style("33", "shutting down…"));
        })
        .context("failed to set Ctrl+C handler")?;
    }

    let mut children: Vec<std::process::Child> = Vec::new();
    let mut threads: Vec<thread::JoinHandle<()>> = Vec::new();

    for (i, pkg) in packages.iter().enumerate() {
        let color = COLORS[i % COLORS.len()];
        eprintln!(
            "{} \x1b[1;{color}m{}\x1b[0m · {}",
            runner::style("32", "running"),
            pkg.name,
            pkg.dev_script,
        );

        let mut child = Command::new(cmd)
            .args(["run", "dev"])
            .current_dir(&pkg.path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("`{cmd}` not found — is it installed and in your PATH?"))?;

        // Take streams before storing the child handle.
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        if let Some(out) = stdout {
            let name = pkg.name.clone();
            let color = color.to_string();
            let opened = Arc::clone(&opened);
            threads.push(thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(|r| r.ok()) {
                    eprintln!("  \x1b[1;{color}m[{name:>max_width$}]\x1b[0m {line}");
                    if open && !opened.load(Ordering::SeqCst) {
                        if let Some(url) = extract_localhost_url(&line) {
                            opened.store(true, Ordering::SeqCst);
                            eprintln!("{} {url}", runner::style("36", "opening"));
                            open_browser(&url);
                        }
                    }
                }
            }));
        }

        if let Some(err) = stderr {
            let name = pkg.name.clone();
            let color = color.to_string();
            let opened = Arc::clone(&opened);
            threads.push(thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(|r| r.ok()) {
                    eprintln!("  \x1b[1;{color}m[{name:>max_width$}]\x1b[0m {line}");
                    if open && !opened.load(Ordering::SeqCst) {
                        if let Some(url) = extract_localhost_url(&line) {
                            opened.store(true, Ordering::SeqCst);
                            eprintln!("{} {url}", runner::style("36", "opening"));
                            open_browser(&url);
                        }
                    }
                }
            }));
        }

        children.push(child);
    }

    // Wait for shutdown signal or all children to exit.
    while !shutdown.load(Ordering::SeqCst) {
        let all_done = children
            .iter_mut()
            .all(|c| matches!(c.try_wait(), Ok(Some(_))));
        if all_done {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Kill remaining children.
    for child in &mut children {
        let _ = child.kill();
    }

    // Wait for reader threads to finish.
    for t in threads {
        let _ = t.join();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_single_dev() {
        let s = single_dev_steps(&ProjectKind::Node {
            manager: NodePM::Pnpm,
        })
        .unwrap();
        assert_eq!(s[0].program, "pnpm");
        assert_eq!(s[0].args, ["run", "dev"]);
    }

    #[test]
    fn zig_dev() {
        let s = single_dev_steps(&ProjectKind::Zig).unwrap();
        assert_eq!(s[0].program, "zig");
        assert_eq!(s[0].args, ["build", "run"]);
    }

    #[test]
    fn non_node_fallback() {
        let s = single_dev_steps(&ProjectKind::Cargo).unwrap();
        assert_eq!(s[0].program, "cargo");
        assert_eq!(s[0].args, ["run"]);
    }

    #[test]
    fn swift_dev_falls_back_to_run() {
        let s = single_dev_steps(&ProjectKind::Swift).unwrap();
        assert_eq!(s[0].program, "swift");
        assert_eq!(s[0].args, ["run"]);
    }

    #[test]
    fn dotnet_dev_uses_watch() {
        let s = single_dev_steps(&ProjectKind::DotNet { sln: false }).unwrap();
        assert_eq!(s[0].program, "dotnet");
        assert_eq!(s[0].args, ["watch", "run"]);
    }

    #[test]
    fn extract_vite_url() {
        let line = "  ➜  Local:   http://localhost:5173/";
        assert_eq!(
            extract_localhost_url(line),
            Some("http://localhost:5173/".to_string())
        );
    }

    #[test]
    fn extract_127_url() {
        let line = "Listening on http://127.0.0.1:3001";
        assert_eq!(
            extract_localhost_url(line),
            Some("http://127.0.0.1:3001".to_string())
        );
    }

    #[test]
    fn no_url_returns_none() {
        assert_eq!(extract_localhost_url("ready in 200ms"), None);
    }
}
