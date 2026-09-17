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
    Tui {
        /// GitHub project URL or view-state file to open or create.
        #[arg(value_name = "URL_OR_PATH")]
        target: Option<String>,
    },
}

#[cfg(all(test, feature = "tui"))]
mod tests {
    use super::*;

    #[test]
    fn tui_url_is_optional() {
        let without_url = Cli::try_parse_from(["ghui", "tui"]).unwrap();
        assert!(matches!(
            without_url.command,
            Some(Command::Tui { target: None })
        ));

        let with_url =
            Cli::try_parse_from(["ghui", "tui", "https://github.com/orgs/example/projects/1"])
                .unwrap();
        assert!(matches!(
            with_url.command,
            Some(Command::Tui { target: Some(target) })
                if target == "https://github.com/orgs/example/projects/1"
        ));
    }
}
