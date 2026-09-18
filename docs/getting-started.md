# Getting started

## Prerequisites

- A stable Rust toolchain (e.g. via [rustup](https://rustup.rs))
- [GitHub CLI](https://cli.github.com/)

## Install

Authenticate GitHub CLI and add the Projects write scope:

```sh
gh auth login --hostname github.com
gh auth refresh --hostname github.com --scopes project
gh extension install llvm-beanz/gh-ghui
```

## Build

```sh
# From the repository root
cargo build                       # includes the TUI by default
cargo build --no-default-features # build without the TUI
```

## Run

```sh
gh ghui --help                 # show all commands and flags
gh ghui view <url>             # table of issues/PRs in a project
gh ghui tui                    # interactive TUI
```

For local development, replace `gh ghui` with
`cargo run --bin gh-ghui --`.

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

Use `--filter` with `gh ghui view` to apply a GitHub Projects-style filter, and
`--sort` to order the matching items:

```sh
gh ghui view --filter 'is:issue status:"In Progress" -label:duplicate' --sort Priority:desc URL
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

ghui first uses `GH_TOKEN`, then `GITHUB_TOKEN`, then the active `github.com`
account stored by GitHub CLI. Environment tokens must provide the same scopes.
Run `gh auth status --hostname github.com` to inspect the active account.

GitHub CLI's normal web login includes `repo` and `read:org`. Editing ProjectV2
items additionally requires `project`; add it with:

```sh
gh auth refresh --hostname github.com --scopes project
```

Organizations can restrict OAuth application access or require SAML SSO
authorization. In those cases, an organization owner may need to approve the
GitHub CLI OAuth application. When GitHub returns accessible project items with
restricted field values, ghui displays the items and leaves those fields blank.
A restriction that prevents access to the project itself is reported as an
error.

