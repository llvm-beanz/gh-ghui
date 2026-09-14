# ghui — workspace instructions

Project: Rust Cargo workspace (resolver = "2") providing the `ghui` CLI for working with GitHub projects. The project is an early stub: structure, CLI skeleton, and docs layout are in place; subcommands are mostly TODO placeholders.

## Structure
- `crates/ghui/` — main CLI crate.
  - `src/main.rs` — entry point; parses `Cli` and dispatches to subcommands.
  - `src/cli.rs` — clap `Parser`/`Subcommand` definitions (register new subcommands here).
  - `src/commands/` — non-interactive subcommand implementations (one module per command, e.g. `list.rs`).
  - `src/tui/` — interactive ratatui TUI; only compiled with the `tui` cargo feature.
- `docs/` — user documentation (markdown); `README.md` links into it.
- `.github/` — project metadata and this file.

## Build & run
- `cargo build` — non-interactive mode only.
- `cargo build --features tui` — also builds the interactive TUI.
- `cargo test --all-features` — runs unit, integration, and doc tests for the workspace.
- `cargo run -- <subcommand>` to run the CLI (e.g. `cargo run -- list`, `cargo run --features tui -- tui`).

## Testing
- Tests are part of the change: when adding or modifying a function, add or update its tests in the same change; do not consider a task complete until `cargo test --all-features` passes.
- Before finishing a change, also run `cargo clippy --all-features --workspace` and `cargo fmt --check`, and fix any findings.
- Unit tests: co-located `#[cfg(test)]` module in the file under test. Name tests by function and scenario (e.g. `parse_owner_repo_missing_slash`).
- Integration tests: `crates/ghui/tests/`. Exercise the CLI end-to-end (args → stdout/stderr + exit code) with `assert_cmd` + `predicates`; use `tempfile` for files the CLI reads or writes.
- Cover error paths and edge cases (bad input, missing token, API failure), not just the happy path.
- Keep tests hermetic and deterministic: no real network access. Put GitHub API access behind an injected client (trait) so tests use a mock; reach for `wiremock`/`httpmock` only when real HTTP semantics matter.
- Feature-gated code must be tested too: keep TUI state/transitions/formatting in pure functions and test them under `--features tui` (or `--all-features`) without requiring a real terminal.
- Structure for testability: keep parsing, formatting, and decision logic in small pure functions; keep `run(...)` a thin adapter over them.
- Test-only dependencies go in `[dev-dependencies]`; if shared, still declare them in `workspace.dependencies` in the root `Cargo.toml`.

## Conventions
- New non-interactive commands: add a module in `crates/ghui/src/commands/` exporting `pub fn run(...) -> Result<(), Box<dyn Error>>`, add the variant in `src/cli.rs`, and dispatch in `src/main.rs`.
- TUI code must stay feature-gated (`#[cfg(feature = "tui")]`); only `ratatui` and `crossterm` (declared as workspace dependencies) may be used there.
- Add new dependencies to `workspace.dependencies` in the root `Cargo.toml` and reference them from crate manifests.
- `Cargo.lock` is intentionally committed (binary workspace) — do not ignore it.
- Keep user-facing docs in `docs/` and update `README.md` when commands or features change.
