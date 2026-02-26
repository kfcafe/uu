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
    /// Detect project type and run the install command
    Install(ProjectArgs),

    /// Detect project type and run it
    Run(ProjectArgs),

    /// Detect project type and run the test suite
    Test(ProjectArgs),

    /// Remove build artifacts and reclaim disk space
    Clean(CleanArgs),

    /// List or kill processes by port
    Ports(PortsArgs),
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
        Commands::Install(a) => cmd::install::execute(a.dry_run, a.args),
        Commands::Run(a) => cmd::run::execute(a.dry_run, a.args),
        Commands::Test(a) => cmd::test_cmd::execute(a.dry_run, a.args),
        Commands::Clean(a) => cmd::clean::execute(a.dry_run),
        Commands::Ports(a) => cmd::ports::execute(a.port, a.kill),
    };

    if let Err(err) = result {
        eprintln!("{} {err:#}", runner::style("31", "error"));
        process::exit(1);
    }
}
