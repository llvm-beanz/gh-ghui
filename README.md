# ghui

Command line tools for working with GitHub projects.

`ghui` is a Cargo workspace that builds the `ghui` CLI. It supports two modes of operation:

- **Non-interactive** — classic subcommands that take flags/environment and print results.
- **Interactive** — a terminal user interface built with [ratatui](https://ratatui.rs) (crossterm backend), enabled with the `tui` cargo feature.


## Requirements

- A stable Rust toolchain

## Build

```sh
cargo build                 # non-interactive mode
cargo build --features tui  # also build the interactive TUI
```

## Usage

```sh
cargo run -- --help
cargo run -- login
cargo run -- list
cargo run --features tui -- tui
```

## Repository layout

| Path | Description |
| --- | --- |
| `crates/ghui/` | Main CLI crate (entry point, CLI definition, subcommands, TUI) |
| `docs/` | User documentation |
| `.github/` | Project metadata and Copilot workspace instructions |

## Documentation

- [Getting started](docs/getting-started.md)

## License

[MIT](LICENSE)
