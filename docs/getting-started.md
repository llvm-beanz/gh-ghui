# Getting started

## Prerequisites

- A stable Rust toolchain (e.g. via [rustup](https://rustup.rs))
- (Optional) A GitHub personal access token for authenticated operations

## Build

```sh
# From the repository root
cargo build                # non-interactive mode
cargo build --features tui # also build the interactive TUI
```

## Run

```sh
cargo run -- --help                 # show all commands and flags
cargo run -- login                  # login and store credentials in system credential store
cargo run -- list                   # non-interactive example (stub)
cargo run --features tui -- tui     # interactive TUI (stub)
```

## Configuration

| Environment variable | Description |
| --- | --- |
| `GITHUB_TOKEN` | GitHub personal access token. `--token` accepts the same value on the command line. |

## What's next

This is a stub. Planned work:

- Real `list` implementation against the GitHub API
- Interactive TUI (browse repositories, issues, and pull requests)
