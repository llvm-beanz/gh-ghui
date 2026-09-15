//! `login` - authenticate with GitHub and store the token.
//!
//! Uses the GitHub OAuth device flow to obtain a token, opens a browser for
//! the user to authorize, and stores the token in the system credential store.

use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

use keyring::Entry;
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use reqwest::StatusCode;
use serde::Deserialize;

type DynError = Box<dyn Error>;

const CLIENT_ID: &str = "Ov23li5S3LwmTDwXKubU";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const OAUTH_SCOPE: &str = "read:project";
const KEYRING_SERVICE: &str = "ghui";
const KEYRING_USER: &str = "github_token";

#[derive(Clone, Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TokenEndpointResponse {
    Access {
        access_token: String,
    },
    Error {
        error: String,
        error_description: Option<String>,
    },
}

#[derive(Debug)]
enum TokenPollResponse {
    Authorized(String),
    Pending,
    SlowDown,
    Denied(String),
}

trait OAuthApi {
    fn request_device_code(&self) -> Result<DeviceCodeResponse, DynError>;
    fn request_access_token(&self, device_code: &str) -> Result<TokenPollResponse, DynError>;
}

struct GitHubOAuthApi {
    client: Client,
}

impl GitHubOAuthApi {
    fn new() -> Result<Self, DynError> {
        let client = Client::builder().user_agent("ghui").build()?;
        Ok(Self { client })
    }
}

impl OAuthApi for GitHubOAuthApi {
    fn request_device_code(&self) -> Result<DeviceCodeResponse, DynError> {
        let response = self
            .client
            .post(GITHUB_DEVICE_CODE_URL)
            .header(ACCEPT, "application/json")
            .form(&[("client_id", CLIENT_ID), ("scope", OAUTH_SCOPE)])
            .send()
            .map_err(|error| format!("Failed to request device authorization: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| format!("Failed to read GitHub's response: {error}"))?;

        parse_device_response(status, &body)
    }

    fn request_access_token(&self, device_code: &str) -> Result<TokenPollResponse, DynError> {
        let response = self
            .client
            .post(GITHUB_TOKEN_URL)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|error| format!("Failed to check authorization status: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| format!("Failed to read GitHub's response: {error}"))?;

        parse_token_response(status, &body)
    }
}

fn parse_device_response(status: StatusCode, body: &str) -> Result<DeviceCodeResponse, DynError> {
    if !status.is_success() {
        return Err(format!("GitHub rejected device authorization with HTTP {status}").into());
    }

    serde_json::from_str(body)
        .map_err(|_| "GitHub returned an invalid device authorization response".into())
}

fn parse_token_response(status: StatusCode, body: &str) -> Result<TokenPollResponse, DynError> {
    let response: TokenEndpointResponse = serde_json::from_str(body).map_err(|_| -> DynError {
        if status.is_success() {
            "GitHub returned an invalid token response".into()
        } else {
            format!("GitHub token request failed with HTTP {status}").into()
        }
    })?;

    match response {
        TokenEndpointResponse::Access { access_token } if status.is_success() => {
            if access_token.is_empty() {
                Err("GitHub returned an empty access token".into())
            } else {
                Ok(TokenPollResponse::Authorized(access_token))
            }
        }
        TokenEndpointResponse::Access { .. } => {
            Err(format!("GitHub token request failed with HTTP {status}").into())
        }
        TokenEndpointResponse::Error { error, .. } if error == "authorization_pending" => {
            Ok(TokenPollResponse::Pending)
        }
        TokenEndpointResponse::Error { error, .. } if error == "slow_down" => {
            Ok(TokenPollResponse::SlowDown)
        }
        TokenEndpointResponse::Error {
            error,
            error_description,
        } => {
            let message = error_description.unwrap_or(error);
            Ok(TokenPollResponse::Denied(message))
        }
    }
}

trait BrowserLauncher {
    fn open(&self, url: &str) -> Result<(), DynError>;
}

struct SystemBrowser;

impl BrowserLauncher for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), DynError> {
        webbrowser::open(url)?;
        Ok(())
    }
}

trait TokenStore {
    fn store(&self, token: &str) -> Result<(), DynError>;
}

struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn store(&self, token: &str) -> Result<(), DynError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| format!("Failed to access the system keyring: {error}"))?;
        entry
            .set_password(token)
            .map_err(|error| format!("Failed to store credentials: {error}"))?;
        Ok(())
    }
}

trait Sleeper {
    fn sleep(&self, duration: Duration);
}

struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

/// Run the GitHub OAuth device flow.
pub fn run(verbose: bool) -> Result<(), DynError> {
    let api = GitHubOAuthApi::new()?;
    execute_login(
        &api,
        &SystemBrowser,
        &KeyringTokenStore,
        &ThreadSleeper,
        verbose,
    )
}

fn execute_login(
    api: &impl OAuthApi,
    browser: &impl BrowserLauncher,
    token_store: &impl TokenStore,
    sleeper: &impl Sleeper,
    verbose: bool,
) -> Result<(), DynError> {
    verbose_message(verbose, "Requesting a device code from GitHub");
    let device = api.request_device_code()?;

    println!("Open {}", device.verification_uri);
    println!("Enter code: {}", device.user_code);

    if let Err(error) = browser.open(&device.verification_uri) {
        eprintln!("Could not open a browser automatically: {error}");
    } else {
        verbose_message(verbose, "Opened the authorization page in your browser");
    }

    println!("Waiting for authorization...");
    let token = poll_for_token(
        api,
        &device.device_code,
        device.interval,
        device.expires_in,
        sleeper,
        verbose,
    )?;

    verbose_message(verbose, "Saving credentials to the system keyring");
    token_store.store(&token)?;
    println!("Login successful. Credentials stored in the system keyring.");
    Ok(())
}

fn poll_for_token(
    api: &impl OAuthApi,
    device_code: &str,
    interval_secs: u64,
    expires_in: u64,
    sleeper: &impl Sleeper,
    verbose: bool,
) -> Result<String, DynError> {
    let mut interval = Duration::from_secs(interval_secs);
    let expires_after = Duration::from_secs(expires_in);
    let started_at = Instant::now();
    let mut attempts = 0_u64;

    loop {
        if started_at.elapsed() >= expires_after {
            return Err("The device code expired. Run `ghui login` again.".into());
        }

        attempts += 1;
        verbose_message(
            verbose,
            &format!("Checking authorization (attempt {attempts})"),
        );

        match api.request_access_token(device_code)? {
            TokenPollResponse::Authorized(token) => return Ok(token),
            TokenPollResponse::Pending => sleeper.sleep(interval),
            TokenPollResponse::SlowDown => {
                interval += Duration::from_secs(5);
                verbose_message(
                    verbose,
                    &format!("GitHub requested a slower polling interval ({interval:?})"),
                );
                sleeper.sleep(interval);
            }
            TokenPollResponse::Denied(message) => {
                return Err(format!("GitHub authorization failed: {message}").into());
            }
        }
    }
}

fn verbose_message(verbose: bool, message: &str) {
    if verbose {
        eprintln!("ghui: {message}");
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    struct MockOAuthApi {
        device: DeviceCodeResponse,
        responses: RefCell<VecDeque<TokenPollResponse>>,
    }

    impl MockOAuthApi {
        fn new(responses: impl IntoIterator<Item = TokenPollResponse>) -> Self {
            Self {
                device: DeviceCodeResponse {
                    device_code: "device-code".into(),
                    user_code: "ABCD-1234".into(),
                    verification_uri: "https://github.example/device".into(),
                    expires_in: 60,
                    interval: 0,
                },
                responses: RefCell::new(responses.into_iter().collect()),
            }
        }
    }

    impl OAuthApi for MockOAuthApi {
        fn request_device_code(&self) -> Result<DeviceCodeResponse, DynError> {
            Ok(self.device.clone())
        }

        fn request_access_token(&self, _device_code: &str) -> Result<TokenPollResponse, DynError> {
            self.responses
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "No mocked token response remains".into())
        }
    }

    #[derive(Default)]
    struct MockBrowser {
        opened_urls: RefCell<Vec<String>>,
    }

    impl BrowserLauncher for MockBrowser {
        fn open(&self, url: &str) -> Result<(), DynError> {
            self.opened_urls.borrow_mut().push(url.into());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockTokenStore {
        tokens: RefCell<Vec<String>>,
    }

    impl TokenStore for MockTokenStore {
        fn store(&self, token: &str) -> Result<(), DynError> {
            self.tokens.borrow_mut().push(token.into());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockSleeper {
        durations: RefCell<Vec<Duration>>,
    }

    impl Sleeper for MockSleeper {
        fn sleep(&self, duration: Duration) {
            self.durations.borrow_mut().push(duration);
        }
    }

    #[test]
    fn execute_login_authorized_stores_token() -> Result<(), DynError> {
        let api = MockOAuthApi::new([
            TokenPollResponse::Pending,
            TokenPollResponse::Authorized("access-token".into()),
        ]);
        let browser = MockBrowser::default();
        let token_store = MockTokenStore::default();
        let sleeper = MockSleeper::default();

        execute_login(&api, &browser, &token_store, &sleeper, false)?;

        assert_eq!(
            browser.opened_urls.into_inner(),
            ["https://github.example/device"]
        );
        assert_eq!(token_store.tokens.into_inner(), ["access-token"]);
        assert_eq!(sleeper.durations.into_inner(), [Duration::ZERO]);
        Ok(())
    }

    #[test]
    fn poll_for_token_slow_down_persists_increased_interval() -> Result<(), DynError> {
        let api = MockOAuthApi::new([
            TokenPollResponse::SlowDown,
            TokenPollResponse::Pending,
            TokenPollResponse::Authorized("access-token".into()),
        ]);
        let sleeper = MockSleeper::default();

        let token = poll_for_token(&api, "device-code", 2, 60, &sleeper, false)?;

        assert_eq!(token, "access-token");
        assert_eq!(
            sleeper.durations.into_inner(),
            [Duration::from_secs(7), Duration::from_secs(7)]
        );
        Ok(())
    }

    #[test]
    fn parse_device_response_does_not_expose_response_body() {
        let body = r#"{"device_code":"secret-device-code"}"#;

        let error = parse_device_response(StatusCode::OK, body)
            .unwrap_err()
            .to_string();

        assert!(!error.contains("secret-device-code"));
        assert_eq!(
            error,
            "GitHub returned an invalid device authorization response"
        );
    }

    #[test]
    fn parse_token_response_maps_authorization_errors() -> Result<(), DynError> {
        let pending = parse_token_response(StatusCode::OK, r#"{"error":"authorization_pending"}"#)?;
        let denied = parse_token_response(
            StatusCode::OK,
            r#"{"error":"access_denied","error_description":"The user declined"}"#,
        )?;

        assert!(matches!(pending, TokenPollResponse::Pending));
        assert!(matches!(
            denied,
            TokenPollResponse::Denied(message) if message == "The user declined"
        ));
        Ok(())
    }

    #[test]
    fn oauth_scope_is_read_only_for_projects() {
        assert_eq!(OAUTH_SCOPE, "read:project");
    }
}
