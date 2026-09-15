# ghui

Command line tools for working with GitHub projects.

`ghui` is a Cargo workspace that builds the `ghui` CLI. It includes:

- **OAuth login** - a device flow that stores credentials in the system keyring.
- **Non-interactive** — classic subcommands that take flags/environment and print results.
- **Interactive mode** - a terminal user interface built with [ratatui](https://ratatui.rs) and crossterm. The TUI is included by default.


## Requirements

- A stable Rust toolchain

## Build

```sh
cargo build                       # includes the TUI by default
cargo build --no-default-features # build without the TUI
```

## Usage

```sh
cargo run -- --help
cargo run -- login
cargo run -- tui
```

Use `--verbose` for diagnostic progress while logging in. Login requests the
read-only `read:project` scope. GitHub does not provide a read-only OAuth scope
for private repositories, so this narrow scope only permits repository access
available without the broad `repo` scope.

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
