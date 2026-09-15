//! Integration tests for the `ghui login` command.

use assert_cmd::Command;

fn ghui() -> Command {
    Command::cargo_bin("ghui").unwrap()
}

/// Login command should be recognized by the CLI.
#[test]
fn login_command_exists() {
    let mut cmd = ghui();
    cmd.arg("--help");
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("login"),
        "Help should mention the login command"
    );
}

