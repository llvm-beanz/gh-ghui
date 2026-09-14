use clap::{Parser, Subcommand};

/// ghui — command line tools for working with GitHub projects.
#[derive(Debug, Parser)]
#[command(name = "ghui", version, about, long_about = None)]
pub struct Cli {
    /// GitHub personal access token (falls back to the `GITHUB_TOKEN` environment variable).
    #[arg(long, env = "GITHUB_TOKEN", global = true)]
    pub token: Option<String>,

    /// Increase output verbosity.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List repositories for the authenticated user (non-interactive).
    List,

    /// Launch the interactive terminal UI.
    #[cfg(feature = "tui")]
    Tui,
}
