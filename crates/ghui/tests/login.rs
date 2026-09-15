//! Integration tests for the `ghui` CLI.

use assert_cmd::Command;
use predicates::prelude::*;

fn ghui() -> Command {
    Command::cargo_bin("ghui").unwrap()
}

#[test]
fn help_lists_supported_commands_and_verbose_flag() {
    let mut cmd = ghui();
    cmd.arg("--help").assert().success().stdout(
        predicate::str::contains("login")
            .and(predicate::str::contains("--verbose"))
            .and(predicate::str::contains("--token").not()),
    );
}

#[cfg(feature = "tui")]
#[test]
fn default_features_include_tui_command() {
    let mut cmd = ghui();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tui"));
}

#[test]
fn removed_token_option_is_rejected() {
    let mut cmd = ghui();
    cmd.args(["--token", "secret", "tui"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--token'"))
        .stderr(predicate::str::contains("secret").not());
}

#[test]
fn unsupported_command_is_rejected() {
    let mut cmd = ghui();
    cmd.arg("not-a-real-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unrecognized subcommand 'not-a-real-command'",
        ));
}
