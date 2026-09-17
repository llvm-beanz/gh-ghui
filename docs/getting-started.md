# Getting started

## Prerequisites

- A stable Rust toolchain (e.g. via [rustup](https://rustup.rs))
- A supported system credential store

## Build

```sh
# From the repository root
cargo build                       # includes the TUI by default
cargo build --no-default-features # build without the TUI
```

## Run

```sh
cargo run -- --help                 # show all commands and flags
cargo run -- login                  # login and store credentials in system credential store
cargo run -- --verbose login        # login with diagnostic progress
cargo run -- view <url>             # table of issues/PRs in a project
cargo run -- tui                    # interactive TUI
```

## TUI tabs

Use `:tabnew` to open another view of the current project, or
`:tabnew <url>` to open a project in a new tab. Switch tabs with `Ctrl+Tab`,
`Ctrl+Shift+Tab`, `:tabnext`, or `:tabprevious`. Close the active tab with
`:tabclose`.

Saving with `:w` or `:wq` records every open tab, each tab's selected row and
columns, and the active tab.

## Permissions

Login requests only the `read:project` OAuth scope. GitHub does not offer a
read-only OAuth scope for private repositories; requesting private repository
access would require the much broader `repo` scope, so it is intentionally not
requested. Credentials are stored in the system keyring and cannot be supplied
on the command line.

