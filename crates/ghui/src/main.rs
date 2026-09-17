mod cli;
mod commands;
mod github;

#[cfg(feature = "tui")]
mod tui;

use std::error::Error;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ghui: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match &cli.command {
        Some(Command::Login) => commands::login::run(cli.verbose),
        Some(Command::View { url }) => commands::view::run(url),
        #[cfg(feature = "tui")]
        Some(Command::Tui { target }) => tui::run(target.as_deref()),
        None => {
            // TODO: consider launching the TUI (when enabled) as the default mode.
            let mut cmd = Cli::command();
            cmd.print_help()?;
            Ok(())
        }
    }
}
