mod cmd;
mod runner;

use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

/// Zero-config project tools for common tasks across many ecosystems.
///
/// Run `uu install` in a Rust project and it runs `cargo install --path .`,
/// or `cargo install --path crates/uu` for this repo's workspace root.
/// Run `uu test` in a Node project and it runs `npm test`. No config needed.
#[derive(Parser)]
#[command(name = "uu", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect project type and build the project
    Build(ProjectArgs),

    /// Typecheck without running tests
    Check(ProjectArgs),

    /// Run the full CI pipeline (fmt check + lint + test)
    Ci(ProjectArgs),

    /// Remove build artifacts and reclaim disk space
    Clean(CleanArgs),

    /// Start dev servers (workspace-aware — runs packages concurrently)
    Dev(DevArgs),

    /// Check that required tools are installed
    Doctor,

    /// Auto-format code
    Fmt(ProjectArgs),

    /// Detect project type and run the install command
    Install(ProjectArgs),

    /// Run the linter
    Lint(ProjectArgs),

    /// List or kill processes by port
    Ports(PortsArgs),

    /// Detect project type and run it
    Run(ProjectArgs),

    /// Detect project type and run the test suite
    Test(ProjectArgs),
}

/// Arguments shared by project-aware commands (install, run, test).
#[derive(Args)]
struct ProjectArgs {
    /// Run in a different directory
    #[arg(short = 'C', long = "dir")]
    directory: Option<PathBuf>,

    /// Show what would run without executing
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Make installed commands become the default shell command when supported
    #[arg(long = "default")]
    make_default: bool,

    /// Extra arguments passed through to the underlying command (after --)
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Args)]
struct DevArgs {
    /// Run in a different directory
    #[arg(short = 'C', long = "dir")]
    directory: Option<PathBuf>,

    /// Show what would run without executing
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Open the first detected localhost URL in your browser
    #[arg(short = 'o', long)]
    open: bool,

    /// Workspace packages to run (omit for all)
    packages: Vec<String>,
}

#[derive(Args)]
struct CleanArgs {
    /// Run in a different directory
    #[arg(short = 'C', long = "dir")]
    directory: Option<PathBuf>,

    /// Show what would be removed without deleting
    #[arg(short = 'n', long)]
    dry_run: bool,
}

/// Change to the given directory if provided.
fn chdir(dir: &Option<PathBuf>) -> Result<()> {
    if let Some(d) = dir {
        std::env::set_current_dir(d)
            .with_context(|| format!("cannot change to directory `{}`", d.display()))?;
    }
    Ok(())
}

#[derive(Args)]
struct PortsArgs {
    /// Port number to inspect
    port: Option<u16>,

    /// Kill the process on this port
    #[arg(short, long)]
    kill: bool,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Build(a) => {
            chdir(&a.directory).and_then(|()| cmd::build::execute(a.dry_run, a.args))
        }
        Commands::Check(a) => {
            chdir(&a.directory).and_then(|()| cmd::check::execute(a.dry_run, a.args))
        }
        Commands::Ci(a) => chdir(&a.directory).and_then(|()| cmd::ci::execute(a.dry_run, a.args)),
        Commands::Clean(a) => chdir(&a.directory).and_then(|()| cmd::clean::execute(a.dry_run)),
        Commands::Dev(a) => chdir(&a.directory)
            .and_then(|()| cmd::dev::execute(a.dry_run, a.open, a.packages, vec![])),
        Commands::Doctor => cmd::doctor::execute(),
        Commands::Fmt(a) => chdir(&a.directory).and_then(|()| cmd::fmt::execute(a.dry_run, a.args)),
        Commands::Install(a) => chdir(&a.directory)
            .and_then(|()| cmd::install::execute(a.dry_run, a.make_default, a.args)),
        Commands::Lint(a) => {
            chdir(&a.directory).and_then(|()| cmd::lint::execute(a.dry_run, a.args))
        }
        Commands::Ports(a) => cmd::ports::execute(a.port, a.kill),
        Commands::Run(a) => chdir(&a.directory).and_then(|()| cmd::run::execute(a.dry_run, a.args)),
        Commands::Test(a) => {
            chdir(&a.directory).and_then(|()| cmd::test_cmd::execute(a.dry_run, a.args))
        }
    };

    if let Err(err) = result {
        eprintln!("{} {err:#}", runner::style("31", "error"));
        process::exit(1);
    }
}
