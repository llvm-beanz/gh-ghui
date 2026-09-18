//! Authentication through the GitHub CLI.

use std::env;
use std::error::Error;
use std::process::{Command, Output};

type DynError = Box<dyn Error>;

pub(crate) fn token() -> Result<String, DynError> {
    resolve_token(
        |name| env::var(name).ok(),
        |executable| {
            Command::new(executable)
                .args(["auth", "token", "--hostname", "github.com"])
                .output()
        },
    )
}

fn resolve_token(
    environment: impl Fn(&str) -> Option<String>,
    run_gh: impl FnOnce(&str) -> std::io::Result<Output>,
) -> Result<String, DynError> {
    for name in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Some(token) = environment(name).filter(|token| !token.is_empty()) {
            return Ok(token);
        }
    }

    let executable = environment("GH_PATH").unwrap_or_else(|| "gh".into());
    let output = run_gh(&executable).map_err(|error| {
        format!(
            "could not run GitHub CLI authentication: {error}; run `gh auth login --hostname github.com`, then `gh auth refresh --hostname github.com --scopes project`"
        )
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if detail.is_empty() {
            format!("gh auth token exited with {}", output.status)
        } else {
            detail
        };
        return Err(format!(
            "GitHub CLI authentication failed: {detail}; run `gh auth login --hostname github.com`, then `gh auth refresh --hostname github.com --scopes project`"
        )
        .into());
    }

    let token = String::from_utf8(output.stdout)?.trim().to_string();
    if token.is_empty() {
        return Err("GitHub CLI returned an empty authentication token".into());
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use std::process::ExitStatus;

    use super::*;

    #[cfg(unix)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }

    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: exit_status(code),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn environment_tokens_follow_github_cli_precedence() {
        let token = resolve_token(
            |name| match name {
                "GH_TOKEN" => Some("preferred".into()),
                "GITHUB_TOKEN" => Some("fallback".into()),
                _ => None,
            },
            |_| panic!("GitHub CLI should not run"),
        )
        .unwrap();

        assert_eq!(token, "preferred");
    }

    #[test]
    fn uses_invoking_github_cli_for_stored_credentials() {
        let token = resolve_token(
            |name| (name == "GH_PATH").then(|| "custom-gh".into()),
            |executable| {
                assert_eq!(executable, "custom-gh");
                Ok(output(0, "stored-token\n", ""))
            },
        )
        .unwrap();

        assert_eq!(token, "stored-token");
    }

    #[test]
    fn failed_github_cli_authentication_is_actionable() {
        let error = resolve_token(|_| None, |_| Ok(output(1, "", "not logged in")))
            .unwrap_err()
            .to_string();

        assert!(error.contains("not logged in"));
        assert!(error.contains("gh auth login --hostname github.com"));
        assert!(error.contains("gh auth refresh --hostname github.com --scopes project"));
    }
}
