//! CLI argument tests for the `view` command (hermetic - no network).

use assert_cmd::Command;
use predicates::prelude::*;

fn ghui() -> Command {
    Command::cargo_bin("ghui").unwrap()
}

#[test]
fn view_help_shows_usage() {
    ghui()
        .arg("view")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("View all issues and PRs"))
        .stdout(predicate::str::contains("URL"));
}

#[test]
fn view_rejects_bad_url() {
    ghui()
        .env("GITHUB_TOKEN", "dummy")
        .arg("view")
        .arg("not-a-url")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported project URL"));
}

#[test]
fn view_rejects_non_github_url() {
    ghui()
        .env("GITHUB_TOKEN", "dummy")
        .arg("view")
        .arg("https://gitlab.com/orgs/x/projects/1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported project URL"));
}
