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

See the [TUI reference](tui-reference.md) for the complete command and shortcut
list.

## Filtering and sorting

Use `--filter` with `ghui view` to apply a GitHub Projects-style filter, and
`--sort` to order the matching items:

```sh
ghui view --filter 'is:issue status:"In Progress" -label:duplicate' --sort Priority:desc URL
```

Filters support field values, quoted values, comma-separated alternatives,
negation, `has:`, `no:`, `is:`, `reason:`, general text, `*` wildcards,
numeric/date comparisons, and inclusive `..` ranges. Multiple clauses are
combined with AND. The relative GitHub keywords `@me`, `@today`, `@current`,
`@previous`, and `@next` are not currently supported.

Sort specifications use `FIELD`, `FIELD:asc`, or `FIELD:desc`. Missing values
sort after populated values. In the TUI, use `:filter EXPRESSION` and
`:sort FIELD[:asc|desc]`; use bare `:filter` or `:sort` to clear them. Filter
and sort settings are stored independently for each saved tab.

## Permissions

Login requests the read-only `read:project` and `read:org` OAuth scopes.
Project access requires `read:project`; displaying organization and enterprise
team reviewers requires `read:org`. GitHub does not offer a read-only OAuth
scope for private repositories; requesting private repository access would
require the much broader `repo` scope, so it is intentionally not requested.
Credentials are stored in the system keyring and cannot be supplied on the
command line.

Organizations can restrict third-party OAuth App access independently of token
scopes. When GitHub returns accessible project items with restricted field
values, ghui displays the items and leaves those fields blank. A restriction
that prevents access to the project itself is still reported as an error.

