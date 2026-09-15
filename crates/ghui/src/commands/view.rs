//! `view` — print a table of all issues and PRs in a GitHub project.
//!
//! The command takes a GitHub project URL (e.g.
//! `https://github.com/orgs/hlsl-tc57/projects/1`) and prints a table whose
//! columns are the item number, its type, its title, and one column per
//! project field.

use std::collections::HashMap;
use std::error::Error;

use super::login;
use serde::Deserialize;

const GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// GraphQL query used to fetch a project's items. Supports cursor pagination.
/// Uses `resource(url: $url)` instead of the deprecated `projectV2(owner, number)`.
const QUERY: &str = r#"
query GetProjectItems($url: URI!, $cursor: String) {
  resource(url: $url) {
    ... on ProjectV2 {
      title
      items(first: 100, after: $cursor) {
        pageInfo {
          hasNextPage
          endCursor
        }
        nodes {
          fieldValues(first: 100) {
            nodes {
              field {
                name
              }
              textValue
              numberValue
              dateValue
              projectV2SingleSelectFieldOption {
                name
              }
              projectV2IterationFieldOption {
                name
              }
            }
          }
          content {
            __typename
            ... on Issue {
              number
              title
              url
            }
            ... on PullRequest {
              number
              title
              url
            }
          }
        }
      }
    }
  }
}
"#;

/// A GitHub project, resolved from a project URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    pub url: String,
}

/// Whether an item is an issue, a pull request, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Issue,
    PullRequest,
    Other,
}

/// A single issue or PR within a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub kind: Kind,
    pub number: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
}

/// A project item: its issue/PR content plus the project's field values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub content: Option<Content>,
    /// Ordered (field name, value) pairs for the item.
    pub fields: Vec<(String, String)>,
}

/// A fully resolved project and its items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub title: String,
    pub items: Vec<Item>,
}

/// Abstraction over the source of project data so tests can inject a mock.
pub trait ProjectSource {
    fn fetch_project(
        &self,
        project_ref: &ProjectRef,
        token: &str,
    ) -> Result<Project, Box<dyn Error>>;
}

/// Run the `view` command: resolve the URL, fetch the project, print the table.
///
/// When `GHUI_MOCK_RESPONSE` is set, uses the pre-captured JSON data instead of
/// making real HTTP requests or accessing the keychain. This is for testing only.
pub fn run(url: &str, token: Option<&str>) -> Result<(), Box<dyn Error>> {
    let project_ref = parse_project_url(url)?;

    // Check for mock mode first (test-only)
    if let Ok(mock_response) = std::env::var("GHUI_MOCK_RESPONSE") {
        // In mock mode, we don't need a real token - use pre-captured data
        let page = decode_project(&mock_response)?;
        let project: Project = page.into();
        println!("Project: {} ({} items)", project.title, project.items.len());
        println!("{}", render_table(&project));
        return Ok(());
    }

    // Resolve token: explicit argument → GITHUB_TOKEN env var → stored credential from keyring
    let token = match token {
        Some(t) => t.to_string(),
        None => std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| login::get_token().ok())
            .ok_or_else(|| {
                format!(
                    "no GitHub token provided; set the GITHUB_TOKEN environment variable, \
                    pass --token, or run `ghui login` to store a token"
                )
            })?,
    };

    let source = HttpProjectSource::new()?;
    let project = view_project(&source, &project_ref, &token)?;
    println!("Project: {} ({} items)", project.title, project.items.len());
    println!("{}", render_table(&project));
    Ok(())
}

/// Parse a GitHub project URL into a project reference containing the full URL.
///
/// Supported forms:
/// - `https://github.com/orgs/{owner}/projects/{number}`
/// - `https://github.com/{owner}/projects/{number}`
/// - `https://github.com/{owner}/{repo}/projects/{number}`
/// - With optional trailing path segments (e.g. `/columns/1`)
pub fn parse_project_url(input: &str) -> Result<ProjectRef, Box<dyn Error>> {
    let s = input.trim();
    let base = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .ok_or_else(|| {
            format!("unsupported project URL: {s} (expected a https://github.com/... project URL)")
        })?;

    let segments: Vec<&str> = base.split('/').filter(|seg| !seg.is_empty()).collect();

    let project_num = segments
        .iter()
        .position(|seg| *seg == "projects")
        .and_then(|i| segments.get(i + 1))
        .copied();

    let _owner = if segments.first() == Some(&"orgs") {
        segments.get(1).copied()
    } else {
        segments.first().copied()
    };

    let _owner = _owner
        .filter(|o| !o.is_empty())
        .ok_or_else(|| format!("unsupported project URL: {s}"))?;
    let _num = project_num
        .and_then(|n| n.parse::<u32>().ok())
        .ok_or_else(|| format!("invalid or missing project number in URL: {s}"))?;

    // Reconstruct the base project URL from segments up to and including the number.
    let project_idx = segments
        .iter()
        .position(|seg| *seg == "projects")
        .ok_or_else(|| format!("unsupported project URL: {s}"))?;
    let _project_num_str = segments
        .get(project_idx + 1)
        .ok_or_else(|| format!("unsupported project URL: {s}"))?;
    let url_segments = &segments[..=project_idx + 1];
    let url = format!("https://github.com/{}", url_segments.join("/"));

    Ok(ProjectRef { url })
}

/// Fetch and decode a project from `source`.
pub fn view_project(
    source: &dyn ProjectSource,
    project_ref: &ProjectRef,
    token: &str,
) -> Result<Project, Box<dyn Error>> {
    source.fetch_project(project_ref, token)
}

/// Render the project's items as an aligned text table.
pub fn render_table(project: &Project) -> String {
    let cols = field_columns(&project.items);
    let mut header: Vec<String> = vec!["#".into(), "Type".into(), "Title".into()];
    header.extend(cols.iter().cloned());

    let rows: Vec<Vec<String>> = project
        .items
        .iter()
        .map(|item| {
            let number = item
                .content
                .as_ref()
                .and_then(|c| c.number)
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into());
            let title = item
                .content
                .as_ref()
                .and_then(|c| c.title.clone())
                .unwrap_or_else(|| "-".into());
            let by_name: HashMap<&str, &str> = item
                .fields
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let mut row = vec![number, kind_label(item).to_string(), title];
            for col in &cols {
                row.push(
                    by_name
                        .get(col.as_str())
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            row
        })
        .collect();

    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    let line = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{:<width$}", c, width = widths[i]))
            .collect::<Vec<_>>()
            .join("  ")
    };

    let mut out: Vec<String> = Vec::new();
    out.push(line(&header));
    out.push(
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  "),
    );
    for row in &rows {
        out.push(line(row));
    }
    out.join("\n")
}

fn field_columns(items: &[Item]) -> Vec<String> {
    let mut cols: Vec<String> = Vec::new();
    for item in items {
        for (name, _) in &item.fields {
            if !cols.iter().any(|c| c == name) {
                cols.push(name.clone());
            }
        }
    }
    cols
}

fn kind_label(item: &Item) -> &'static str {
    match item.content.as_ref().map(|c| c.kind) {
        Some(Kind::Issue) => "Issue",
        Some(Kind::PullRequest) => "PR",
        _ => "-",
    }
}

/// Real project source backed by the GitHub GraphQL API.
pub struct HttpProjectSource {
    http: reqwest::blocking::Client,
}

impl HttpProjectSource {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("ghui/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http })
    }

    fn fetch_page(
        &self,
        project_ref: &ProjectRef,
        token: &str,
        cursor: Option<&str>,
    ) -> Result<String, Box<dyn Error>> {
        let payload = serde_json::json!({
            "query": QUERY,
            "variables": {
                "url": project_ref.url,
                "cursor": cursor,
            },
        });
        let response = self
            .http
            .post(GRAPHQL_URL)
            .bearer_auth(token)
            .json(&payload)
            .send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(format!(
                "GitHub GraphQL request failed with HTTP {status}: {}",
                snippet(&body)
            )
            .into());
        }
        Ok(body)
    }
}

impl ProjectSource for HttpProjectSource {
    fn fetch_project(
        &self,
        project_ref: &ProjectRef,
        token: &str,
    ) -> Result<Project, Box<dyn Error>> {
        let mut title = String::new();
        let mut items: Vec<Item> = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let body = self.fetch_page(project_ref, token, cursor.as_deref())?;
            let page = decode_project(&body)?;
            if title.is_empty() {
                title = page.title;
            }
            items.extend(page.items);
            if page.has_next_page {
                cursor = page.end_cursor;
            } else {
                break;
            }
        }

        Ok(Project { title, items })
    }
}

/// Decode a single page of the GraphQL response into domain types.
pub(crate) fn decode_project(body: &str) -> Result<DecodedPage, Box<dyn Error>> {
    let resp: PageResponse = serde_json::from_str(body)?;

    if !resp.errors.is_empty() {
        let messages = resp
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("GitHub API error: {messages}").into());
    }

    let data = resp
        .data
        .ok_or_else(|| "GitHub API response was missing 'data'".to_string())?;
    let project = data
        .resource
        .ok_or_else(|| "project not found (check the project URL and your token)".to_string())?;

    Ok(DecodedPage {
        title: project.title,
        items: project.items.nodes.into_iter().map(Item::from).collect(),
        has_next_page: project.items.page_info.has_next_page,
        end_cursor: project.items.page_info.end_cursor,
    })
}

#[derive(Debug)]
pub(crate) struct DecodedPage {
    pub title: String,
    pub items: Vec<Item>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

impl From<DecodedPage> for Project {
    fn from(page: DecodedPage) -> Self {
        Project {
            title: page.title,
            items: page.items,
        }
    }
}

fn snippet(s: &str) -> String {
    let s = s.trim();
    if s.len() <= 300 {
        return s.to_string();
    }
    let mut end = 300;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

fn format_field_value(f: &FieldValueData) -> String {
    if let Some(t) = &f.text_value {
        return t.clone();
    }
    if let Some(opt) = &f.project_v2_single_select_field_option {
        return opt.name.clone();
    }
    if let Some(opt) = &f.project_v2_iteration_field_option {
        return opt.name.clone();
    }
    if let Some(n) = f.number_value {
        return format_number(n);
    }
    if let Some(d) = &f.date_value {
        return d.clone();
    }
    String::new()
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// ---- GraphQL response shapes (serde) ----

#[derive(Debug, Clone, Default, Deserialize)]
struct PageResponse {
    #[serde(default)]
    data: Option<PageData>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PageData {
    #[serde(default, rename = "resource")]
    resource: Option<ProjectData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectData {
    #[serde(default)]
    title: String,
    #[serde(default)]
    items: ItemsConnection,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemsConnection {
    #[serde(default)]
    nodes: Vec<ItemData>,
    #[serde(default)]
    page_info: PageInfo,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    #[serde(default)]
    has_next_page: bool,
    #[serde(default)]
    end_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemData {
    #[serde(default)]
    field_values: FieldValuesConnection,
    #[serde(default)]
    content: Option<ContentData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FieldValuesConnection {
    #[serde(default)]
    nodes: Vec<FieldValueData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldValueData {
    #[serde(default)]
    field: FieldData,
    #[serde(default)]
    text_value: Option<String>,
    #[serde(default)]
    number_value: Option<f64>,
    #[serde(default)]
    date_value: Option<String>,
    #[serde(default)]
    project_v2_single_select_field_option: Option<SelectOptionData>,
    #[serde(default)]
    project_v2_iteration_field_option: Option<SelectOptionData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FieldData {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SelectOptionData {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ContentData {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    #[serde(default)]
    number: Option<u32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

impl From<ItemData> for Item {
    fn from(d: ItemData) -> Self {
        let content = d.content.map(|c| Content {
            kind: match c.kind.as_deref() {
                Some("Issue") => Kind::Issue,
                Some("PullRequest") => Kind::PullRequest,
                _ => Kind::Other,
            },
            number: c.number,
            title: c.title,
            url: c.url,
        });
        let fields = d
            .field_values
            .nodes
            .into_iter()
            .map(|f| {
                let value = format_field_value(&f);
                (f.field.name, value)
            })
            .collect();
        Item { content, fields }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_project_url ----

    #[test]
    fn parse_project_url_orgs() {
        let r = parse_project_url("https://github.com/orgs/hlsl-tc57/projects/1").unwrap();
        assert_eq!(
            r,
            ProjectRef {
                url: "https://github.com/orgs/hlsl-tc57/projects/1".into()
            }
        );
    }

    #[test]
    fn parse_project_url_repo() {
        let r = parse_project_url("https://github.com/owner/repo/projects/5").unwrap();
        assert_eq!(
            r,
            ProjectRef {
                url: "https://github.com/owner/repo/projects/5".into()
            }
        );
    }

    #[test]
    fn parse_project_url_user() {
        let r = parse_project_url("https://github.com/someuser/projects/3").unwrap();
        assert_eq!(
            r,
            ProjectRef {
                url: "https://github.com/someuser/projects/3".into()
            }
        );
    }

    #[test]
    fn parse_project_url_orgs_trailing_path() {
        let r =
            parse_project_url("https://github.com/orgs/hlsl-tc57/projects/1/columns/2").unwrap();
        assert_eq!(
            r,
            ProjectRef {
                url: "https://github.com/orgs/hlsl-tc57/projects/1".into()
            }
        );
    }

    #[test]
    fn parse_project_url_bad_host() {
        assert!(parse_project_url("https://gitlab.com/orgs/x/projects/1").is_err());
    }

    #[test]
    fn parse_project_url_not_a_project_path() {
        assert!(parse_project_url("https://github.com/owner/repo/pulls").is_err());
    }

    #[test]
    fn parse_project_url_bad_number() {
        assert!(parse_project_url("https://github.com/orgs/x/projects/notanumber").is_err());
    }

    #[test]
    fn parse_project_url_missing_number() {
        assert!(parse_project_url("https://github.com/orgs/x/projects").is_err());
    }

    // ---- decode_project ----

    const FIXTURE: &str = r#"{
      "data": {
        "resource": {
          "title": "My Project",
          "items": {
            "pageInfo": { "hasNextPage": false, "endCursor": null },
            "nodes": [
              {
                "fieldValues": {
                  "nodes": [
                    { "field": { "name": "Status" }, "textValue": null, "numberValue": null, "dateValue": null, "projectV2SingleSelectFieldOption": { "name": "In Progress" }, "projectV2IterationFieldOption": null },
                    { "field": { "name": "Estimate" }, "textValue": null, "numberValue": 3.0, "dateValue": null, "projectV2SingleSelectFieldOption": null, "projectV2IterationFieldOption": null }
                  ]
                },
                "content": { "__typename": "Issue", "number": 42, "title": "Fix the thing", "url": "https://github.com/o/r/issues/42" }
              },
              {
                "fieldValues": {
                  "nodes": [
                    { "field": { "name": "Status" }, "textValue": null, "numberValue": null, "dateValue": null, "projectV2SingleSelectFieldOption": { "name": "Done" }, "projectV2IterationFieldOption": null },
                    { "field": { "name": "Estimate" }, "textValue": null, "numberValue": null, "dateValue": null, "projectV2SingleSelectFieldOption": null, "projectV2IterationFieldOption": null }
                  ]
                },
                "content": { "__typename": "PullRequest", "number": 7, "title": "Add feature", "url": "https://github.com/o/r/pull/7" }
              }
            ]
          }
        }
      }
    }"#;

    #[test]
    fn decode_project_fixture() {
        let page = decode_project(FIXTURE).unwrap();
        assert_eq!(page.title, "My Project");
        assert!(!page.has_next_page);
        assert_eq!(page.items.len(), 2);

        let first = &page.items[0];
        assert_eq!(first.content.as_ref().unwrap().kind, Kind::Issue);
        assert_eq!(first.content.as_ref().unwrap().number, Some(42));
        assert_eq!(
            first.content.as_ref().unwrap().title.as_deref(),
            Some("Fix the thing")
        );
        assert_eq!(
            first.fields,
            vec![
                ("Status".to_string(), "In Progress".to_string()),
                ("Estimate".to_string(), "3".to_string()),
            ]
        );

        let second = &page.items[1];
        assert_eq!(second.content.as_ref().unwrap().kind, Kind::PullRequest);
        assert_eq!(second.content.as_ref().unwrap().number, Some(7));
        assert_eq!(
            second.fields,
            vec![
                ("Status".to_string(), "Done".to_string()),
                ("Estimate".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn decode_project_graphql_errors() {
        let body =
            r#"{ "errors": [ { "message": "Could not resolve to a ProjectV2" } ], "data": null }"#;
        let err = decode_project(body).unwrap_err();
        assert!(err.to_string().contains("Could not resolve to a ProjectV2"));
    }

    #[test]
    fn decode_project_missing_data() {
        let err = decode_project(r#"{ "data": null }"#).unwrap_err();
        assert!(err.to_string().contains("missing 'data'"));
    }

    #[test]
    fn decode_project_not_found() {
        let err = decode_project(r#"{ "data": { "resource": null } }"#).unwrap_err();
        assert!(err.to_string().contains("project not found"));
    }

    #[test]
    fn decode_project_pagination_cursor() {
        let body = r#"{ "data": { "resource": { "title": "P", "items": { "pageInfo": { "hasNextPage": true, "endCursor": "abc" }, "nodes": [] } } } }"#;
        let page = decode_project(body).unwrap();
        assert!(page.has_next_page);
        assert_eq!(page.end_cursor.as_deref(), Some("abc"));
    }

    // ---- format helpers ----

    #[test]
    fn format_number_integer_vs_float() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(-2.0), "-2");
        assert_eq!(format_number(2.5), "2.5");
    }

    #[test]
    fn format_field_value_precedence() {
        let base = FieldValueData {
            field: FieldData { name: "F".into() },
            ..Default::default()
        };

        assert_eq!(
            format_field_value(&FieldValueData {
                number_value: Some(3.0),
                ..base.clone()
            }),
            "3"
        );
        assert_eq!(
            format_field_value(&FieldValueData {
                date_value: Some("2026-09-01".into()),
                ..base.clone()
            }),
            "2026-09-01"
        );
        assert_eq!(
            format_field_value(&FieldValueData {
                project_v2_iteration_field_option: Some(SelectOptionData {
                    name: "Sprint 2".into()
                }),
                ..base.clone()
            }),
            "Sprint 2"
        );

        // Single-select wins over iteration.
        let mut both = base.clone();
        both.project_v2_single_select_field_option = Some(SelectOptionData { name: "A".into() });
        both.project_v2_iteration_field_option = Some(SelectOptionData { name: "B".into() });
        assert_eq!(format_field_value(&both), "A");

        // Text wins over everything.
        let mut with_text = both.clone();
        with_text.text_value = Some("T".into());
        assert_eq!(format_field_value(&with_text), "T");
    }

    // ---- render_table ----

    fn sample_project() -> Project {
        Project {
            title: "Demo".into(),
            items: vec![
                Item {
                    content: Some(Content {
                        kind: Kind::Issue,
                        number: Some(42),
                        title: Some("Fix the thing".into()),
                        url: Some("https://github.com/o/r/issues/42".into()),
                    }),
                    fields: vec![
                        ("Status".into(), "In Progress".into()),
                        ("Estimate".into(), "3".into()),
                    ],
                },
                Item {
                    content: Some(Content {
                        kind: Kind::PullRequest,
                        number: Some(7),
                        title: Some("Add feature".into()),
                        url: None,
                    }),
                    fields: vec![
                        ("Status".into(), "Done".into()),
                        ("Estimate".into(), String::new()),
                    ],
                },
            ],
        }
    }

    /// Collapse runs of spaces to a single space and trim the trailing end.
    fn collapse(l: &str) -> String {
        let mut out = String::new();
        let mut prev_space = false;
        for ch in l.chars() {
            if ch == ' ' {
                if !prev_space {
                    out.push(' ');
                }
                prev_space = true;
            } else {
                out.push(ch);
                prev_space = false;
            }
        }
        out.trim_end().to_string()
    }

    #[test]
    fn render_table_columns_and_rows() {
        let table = render_table(&sample_project());
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 4);

        // All lines share the same width (aligned columns).
        let widths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
        assert!(widths.iter().all(|w| *w == widths[0]));

        assert_eq!(collapse(lines[0]), "# Type Title Status Estimate");
        assert!(lines[1].chars().all(|c| c == '-' || c == ' '));
        assert_eq!(collapse(lines[2]), "42 Issue Fix the thing In Progress 3");
        assert_eq!(collapse(lines[3]), "7 PR Add feature Done");
    }

    #[test]
    fn render_table_missing_content() {
        let project = Project {
            title: "P".into(),
            items: vec![Item {
                content: None,
                fields: vec![("Status".into(), "Todo".into())],
            }],
        };
        let table = render_table(&project);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[2].starts_with('-'));
    }

    #[test]
    fn render_table_no_items() {
        let table = render_table(&Project {
            title: "E".into(),
            items: vec![],
        });
        assert_eq!(table.lines().count(), 2);
    }
}
