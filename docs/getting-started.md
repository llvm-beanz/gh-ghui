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
cargo run -- tui                    # interactive TUI (stub)
```

## Permissions

Login requests only the `read:project` OAuth scope. GitHub does not offer a
read-only OAuth scope for private repositories; requesting private repository
access would require the much broader `repo` scope, so it is intentionally not
requested. Credentials are stored in the system keyring and cannot be supplied
on the command line.

## What's next

This is a stub. Planned work:

- Interactive TUI (browse repositories, issues, and pull requests)
