//! Interactive terminal UI, built on ratatui.
//!
//! This module is only compiled when the `tui` cargo feature is enabled.

use std::error::Error;

/// Run the interactive UI until the user exits it.
pub fn run() -> Result<(), Box<dyn Error>> {
    // TODO:
    //  - switch to the alternate screen and enable raw mode (crossterm)
    //  - create a ratatui `Terminal` with a `CrosstermBackend`
    //  - render app state and drive it from crossterm events
    println!("ghui tui: not implemented yet");
    Ok(())
}
