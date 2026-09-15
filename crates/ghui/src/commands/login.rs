//! `login` — authenticate with GitHub and store the token.
//!
//! Uses the GitHub OAuth device flow to obtain a token, opens a browser for
//! the user to authorize, and stores the token in the system credential store
//! via the `keyring` crate.

use std::error::Error;
use std::thread;
use std::time::Duration;

use keyring::Entry;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use webbrowser;

const CLIENT_ID: &str = "Ov23li5S3LwmTDwXKubU";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const KEYRING_SERVICE: &str = "ghui";
const KEYRING_USER: &str = "github_token";

/// OAuth device code response.
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// OAuth access token response.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    token_type: Option<String>,
    scope: Option<String>,
}

/// Run the login command: start device flow, open browser, poll for token,
/// store in keyring.
pub fn run() -> Result<(), Box<dyn Error>> {
    println!("Starting GitHub OAuth device flow...");

    let client = Client::builder().user_agent("ghui").build()?;

    // Step 1: Request device code
    let response = client
        .post(GITHUB_DEVICE_CODE_URL)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "ghui")
        .form(&[("client_id", CLIENT_ID), ("scope", "repo")])
        .send()
        .map_err(|e| format!("Failed to request device code: {e}"))?;

    let raw_body = response
        .text()
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let device_resp: DeviceCodeResponse = serde_json::from_str(&raw_body).map_err(|e| {
        format!("Failed to parse device code response: {e}. GitHub returned: {raw_body}")
    })?;

    println!();
    println!("To authorize this device, visit:");
    println!("  {}", device_resp.verification_uri);
    println!("Enter code: {}", device_resp.user_code);
    println!();

    // Step 2: Open browser
    if let Err(e) = webbrowser::open(&device_resp.verification_uri) {
        eprintln!("Warning: Could not open browser automatically: {e}");
        eprintln!("Please open the URL above manually in your browser.");
    }

    // Step 3: Poll for token
    let token = poll_for_token(
        &client,
        &device_resp.device_code,
        device_resp.interval,
        device_resp.expires_in,
    )?;

    // Step 4: Store token in keyring
    store_token(&token)?;

    println!("Successfully authenticated! Token stored in system keyring");
    Ok(())
}

/// Poll the GitHub token endpoint until authorization is complete.
fn poll_for_token(
    client: &Client,
    device_code: &str,
    interval_secs: u64,
    expires_in: u64,
) -> Result<String, Box<dyn Error>> {
    let interval = Duration::from_secs(interval_secs);
    let start = std::time::Instant::now();
    let mut poll_count = 0;

    loop {
        if start.elapsed() > Duration::from_secs(expires_in) {
            return Err("Device code expired. Please run `ghui login` again.".into());
        }

        poll_count += 1;
        print!("\rWaiting for authorization... (attempt {poll_count})");
        std::io::Write::flush(&mut std::io::stdout())?;

        let resp = client
            .post(GITHUB_TOKEN_URL)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, "ghui")
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send();

        match resp {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text()?;

                if let Ok(err_resp) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(err) = err_resp.get("error").and_then(|e| e.as_str()) {
                        match err {
                            "authorization_pending" => {
                                poll_count += 1;
                                print!("\rWaiting for authorization... (attempt {poll_count})");
                                std::io::Write::flush(&mut std::io::stdout())?;
                                thread::sleep(interval);
                                continue;
                            }
                            "slow_down" => {
                                poll_count += 1;
                                print!("\rWaiting for authorization... (attempt {poll_count})");
                                std::io::Write::flush(&mut std::io::stdout())?;
                                thread::sleep(interval + Duration::from_secs(1));
                                continue;
                            }
                            _ => {
                                let desc = err_resp
                                    .get("error_description")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("");
                                return Err(format!("OAuth error: {err} {desc}").into());
                            }
                        }
                    }
                }

                let token_resp: AccessTokenResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse token response: {e}"))?;
                println!("\rAuthorization granted!                    ");
                return Ok(token_resp.access_token);
            }
            Ok(resp) => {
                let body = resp.text()?;

                if let Ok(err_resp) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(err) = err_resp.get("error").and_then(|e| e.as_str()) {
                        match err {
                            "authorization_pending" => {
                                thread::sleep(interval);
                            }
                            "slow_down" => {
                                thread::sleep(interval + Duration::from_secs(1));
                            }
                            _ => {
                                let desc = err_resp
                                    .get("error_description")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("");
                                return Err(format!("OAuth error: {err} {desc}").into());
                            }
                        }
                    } else {
                        return Err(format!("Unexpected response: {body}").into());
                    }
                } else {
                    return Err(format!("Unexpected response: {body}").into());
                }
            }
            Err(e) => {
                eprintln!("\rHTTP error during polling: {e}");
                std::io::Write::flush(&mut std::io::stdout())?;
                thread::sleep(interval);
            }
        }
    }
}

/// Store the token in the system keyring.
fn store_token(token: &str) -> Result<(), Box<dyn Error>> {
    eprintln!("Debug: Storing token of length {}", token.len());
    eprintln!("Debug: Service={}, User={}", KEYRING_SERVICE, KEYRING_USER);

    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    entry
        .set_password(token)
        .map_err(|e| format!("Failed to store token in keyring: {e}"))?;

    eprintln!("Debug: set_password() succeeded");

    // Verify the token was stored by reading it back
    let stored = entry
        .get_password()
        .map_err(|e| format!("Failed to verify token in keyring: {e}"))?;

    eprintln!("Debug: Retrieved token length: {}", stored.len());

    if stored != token {
        eprintln!(
            "Debug: Token mismatch! Expected len={}, Got len={}",
            token.len(),
            stored.len()
        );
        return Err("Token verification failed: stored token does not match".into());
    }

    eprintln!("Debug: Token successfully stored and verified");
    Ok(())
}

/// Retrieve a stored token from the keyring.
pub fn get_token() -> Result<String, Box<dyn Error>> {
    eprintln!(
        "Debug: Retrieving token, Service={}, User={}",
        KEYRING_SERVICE, KEYRING_USER
    );

    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    let result = entry.get_password();

    match result {
        Ok(token) => {
            eprintln!("Debug: Token retrieved, length: {}", token.len());
            let token = token.trim().to_string();
            if token.is_empty() {
                eprintln!("Debug: Token is empty after trim");
                return Err("Token is empty".into());
            }
            Ok(token)
        }
        Err(e) => {
            eprintln!("Debug: Failed to get password: {}", e);
            Err(format!("Failed to read token from keyring: {e}").into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyring_constants() {
        assert_eq!(KEYRING_SERVICE, "ghui");
        assert_eq!(KEYRING_USER, "github_token");
    }
}
