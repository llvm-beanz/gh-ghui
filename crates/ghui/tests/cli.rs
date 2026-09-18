//! Integration tests for the `gh-ghui` extension executable.

use assert_cmd::Command;
use predicates::prelude::*;

fn ghui() -> Command {
    Command::cargo_bin("gh-ghui").unwrap()
}

#[test]
fn help_lists_supported_commands_without_custom_login() {
    ghui()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("view"))
        .stdout(predicate::str::contains("login").not());
}

#[cfg(feature = "tui")]
#[test]
fn default_features_include_tui_command() {
    ghui()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tui"));
}

#[test]
fn token_option_is_rejected_without_echoing_its_value() {
    ghui()
        .args(["--token", "secret", "view"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--token'"))
        .stderr(predicate::str::contains("secret").not());
}

#[test]
fn unsupported_command_is_rejected() {
    ghui()
        .arg("not-a-real-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unrecognized subcommand 'not-a-real-command'",
        ));
}
