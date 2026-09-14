use std::error::Error;

/// List repositories for the authenticated user (non-interactive).
pub fn run(token: Option<&str>) -> Result<(), Box<dyn Error>> {
    // TODO: call the GitHub API with the provided token (or ambient credentials)
    // and print the resulting repositories.
    let has_token = token.is_some();
    println!("ghui list: not implemented yet (token provided: {has_token})");
    Ok(())
}
