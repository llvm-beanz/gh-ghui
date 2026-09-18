# gh-ghui

A GitHub CLI extension for working with GitHub Projects.

`gh-ghui` includes:

- **GitHub CLI authentication** - uses the active account from `gh auth`.
- **Non-interactive output** - prints filtered and sorted project items.
- **Interactive mode** - a terminal user interface built with [ratatui](https://ratatui.rs) and crossterm. The TUI is included by default.

## Install

Authenticate GitHub CLI and grant Projects write access:

```sh
gh auth login --hostname github.com
gh auth refresh --hostname github.com --scopes project
```

Install and run the extension:

```sh
gh extension install llvm-beanz/gh-ghui
gh ghui tui
```

`GH_TOKEN` and `GITHUB_TOKEN` are also supported, in that order, before stored
GitHub CLI credentials.

## Develop

- A stable Rust toolchain
- GitHub CLI

Build and test from the repository root:

```sh
cargo build                       # includes the TUI by default
cargo build --no-default-features # build without the TUI
cargo test --all-features
```

Run the development binary:

```sh
cargo run --bin gh-ghui -- --help
cargo run --bin gh-ghui -- view https://github.com/orgs/hlsl-tc57/projects/1
cargo run --bin gh-ghui -- tui
```

GitHub CLI's normal web login includes `repo` and `read:org`. The additional
`project` scope permits editing ProjectV2 items. Organization OAuth restrictions
or SAML SSO policies can still require an owner to approve or authorize the
GitHub CLI OAuth application.

## Repository layout

| Path | Description |
| --- | --- |
| `crates/ghui/` | Extension crate (entry point, CLI definition, subcommands, TUI) |
| `docs/` | User documentation |
| `.github/` | Project metadata and Copilot workspace instructions |

## Documentation

- [Getting started](docs/getting-started.md)
- [TUI reference](docs/tui-reference.md)

## License

[MIT](LICENSE)
