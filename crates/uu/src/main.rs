mod cmd;
mod runner;

use std::process;

use clap::{Args, Parser, Subcommand};

/// Universal utilities — zero-config developer tools that detect your project
/// and do the right thing.
///
/// Run `uu install` in a Rust project and it runs `cargo install --path .`.
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
    /// Show what would run without executing
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Extra arguments passed through to the underlying command (after --)
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Args)]
struct DevArgs {
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
    /// Show what would be removed without deleting
    #[arg(short = 'n', long)]
    dry_run: bool,
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
        Commands::Build(a) => cmd::build::execute(a.dry_run, a.args),
        Commands::Check(a) => cmd::check::execute(a.dry_run, a.args),
        Commands::Ci(a) => cmd::ci::execute(a.dry_run, a.args),
        Commands::Clean(a) => cmd::clean::execute(a.dry_run),
        Commands::Dev(a) => cmd::dev::execute(a.dry_run, a.open, a.packages, vec![]),
        Commands::Doctor => cmd::doctor::execute(),
        Commands::Fmt(a) => cmd::fmt::execute(a.dry_run, a.args),
        Commands::Install(a) => cmd::install::execute(a.dry_run, a.args),
        Commands::Lint(a) => cmd::lint::execute(a.dry_run, a.args),
        Commands::Ports(a) => cmd::ports::execute(a.port, a.kill),
        Commands::Run(a) => cmd::run::execute(a.dry_run, a.args),
        Commands::Test(a) => cmd::test_cmd::execute(a.dry_run, a.args),
    };

    if let Err(err) = result {
        eprintln!("{} {err:#}", runner::style("31", "error"));
        process::exit(1);
    }
}
