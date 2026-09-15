use clap::{Parser, Subcommand};

/// ghui — command line tools for working with GitHub projects.
#[derive(Debug, Parser)]
#[command(name = "ghui", version, about, long_about = None)]
pub struct Cli {
    /// Increase output verbosity.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authenticate with GitHub and store the token.
    Login,

    /// Launch the interactive terminal UI.
    #[cfg(feature = "tui")]
    Tui,
}
