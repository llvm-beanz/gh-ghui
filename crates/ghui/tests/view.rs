//! Integration tests for the `view` command (hermetic — no network).

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

#[test]
fn view_requires_token() {
    ghui()
        .env_remove("GITHUB_TOKEN")
        .arg("view")
        .arg("https://github.com/orgs/hlsl-tc57/projects/1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no GitHub token"));
}

// ---- Mock mode tests (hermetic — no network, no keychain) ----

/// Pre-captured project response for mocking.
fn mock_response() -> String {
    serde_json::json!({
        "data": {
            "resource": {
                "title": "Test Project",
                "items": {
                    "pageInfo": { "hasNextPage": false, "endCursor": null },
                    "nodes": [
                        {
                            "fieldValues": {
                                "nodes": [
                                    { "field": { "name": "Status" }, "textValue": null, "numberValue": null, "dateValue": null, "projectV2SingleSelectFieldOption": { "name": "Done" }, "projectV2IterationFieldOption": null },
                                    { "field": { "name": "Estimate" }, "textValue": null, "numberValue": 5.0, "dateValue": null, "projectV2SingleSelectFieldOption": null, "projectV2IterationFieldOption": null }
                                ]
                            },
                            "content": { "__typename": "Issue", "number": 42, "title": "Fix the thing", "url": "https://github.com/o/r/issues/42" }
                        },
                        {
                            "fieldValues": {
                                "nodes": [
                                    { "field": { "name": "Status" }, "textValue": null, "numberValue": null, "dateValue": null, "projectV2SingleSelectFieldOption": { "name": "In Progress" }, "projectV2IterationFieldOption": null },
                                    { "field": { "name": "Estimate" }, "textValue": null, "numberValue": 3.0, "dateValue": null, "projectV2SingleSelectFieldOption": null, "projectV2IterationFieldOption": null }
                                ]
                            },
                            "content": { "__typename": "PullRequest", "number": 7, "title": "Add feature", "url": "https://github.com/o/r/pull/7" }
                        }
                    ]
                }
            }
        }
    })
    .to_string()
}

#[test]
fn view_with_mock_response_succeeds() {
    // Mock mode bypasses token check and network calls
    ghui()
        .env("GHUI_MOCK_RESPONSE", mock_response())
        .arg("view")
        .arg("https://github.com/orgs/hlsl-tc57/projects/1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Project"))
        .stdout(predicate::str::contains("2 items"))
        .stdout(predicate::str::contains("Fix the thing"))
        .stdout(predicate::str::contains("Add feature"));
}

#[test]
fn view_with_mock_response_shows_table() {
    ghui()
        .env("GHUI_MOCK_RESPONSE", mock_response())
        .arg("view")
        .arg("https://github.com/orgs/hlsl-tc57/projects/1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Status"))
        .stdout(predicate::str::contains("Estimate"))
        .stdout(predicate::str::contains("Done"))
        .stdout(predicate::str::contains("5"))
        .stdout(predicate::str::contains("3"));
}

#[test]
fn view_with_mock_response_handles_errors() {
    // Mock response with GraphQL errors
    let error_response = serde_json::json!({
        "errors": [{ "message": "Could not resolve to a ProjectV2" }],
        "data": null
    })
    .to_string();

    ghui()
        .env("GHUI_MOCK_RESPONSE", error_response)
        .arg("view")
        .arg("https://github.com/orgs/hlsl-tc57/projects/1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not resolve to a ProjectV2"));
}

#[test]
fn view_with_mock_response_handles_missing_project() {
    // Mock response with null project
    let null_response = serde_json::json!({
        "data": { "resource": null }
    })
    .to_string();

    ghui()
        .env("GHUI_MOCK_RESPONSE", null_response)
        .arg("view")
        .arg("https://github.com/orgs/hlsl-tc57/projects/1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("project not found"));
}
