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

    /// View all issues and PRs in a GitHub project as a table.
    View {
        /// URL of the GitHub project (e.g. https://github.com/orgs/hlsl-tc57/projects/1).
        #[arg(value_name = "URL")]
        url: String,
    },

    /// Launch the interactive terminal UI.
    #[cfg(feature = "tui")]
    Tui,
}
