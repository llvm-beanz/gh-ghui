use clap::{Parser, Subcommand};

/// ghui — command line tools for working with GitHub projects.
#[derive(Debug, Parser)]
#[command(name = "ghui", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// View all issues and PRs in a GitHub project as a table.
    View {
        /// URL of the GitHub project (e.g. https://github.com/orgs/hlsl-tc57/projects/1).
        #[arg(value_name = "URL")]
        url: String,

        /// GitHub Projects-style filter expression.
        #[arg(short, long, value_name = "EXPRESSION")]
        filter: Option<String>,

        /// Field to sort by, optionally followed by :asc or :desc.
        #[arg(short, long, value_name = "FIELD[:asc|desc]")]
        sort: Option<String>,
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

    #[test]
    fn view_accepts_filter_and_sort_options() {
        let cli = Cli::try_parse_from([
            "ghui",
            "view",
            "--filter",
            "is:issue status:\"In Progress\"",
            "--sort",
            "Priority:desc",
            "https://github.com/orgs/example/projects/1",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Some(Command::View { filter: Some(filter), sort: Some(sort), .. })
                if filter == "is:issue status:\"In Progress\"" && sort == "Priority:desc"
        ));
    }
}
