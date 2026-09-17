//! `view` - print a table of all issues and PRs in a GitHub project.

use std::collections::HashMap;
use std::io::{self, Write};

use super::login;
use crate::github::{
    parse_project_url, DynError, GitHubProjectSource, Item, Kind, Project, ProjectSource,
};

/// Run the `view` command: resolve the URL, fetch the project, print the table.
pub fn run(url: &str) -> Result<(), DynError> {
    let source = GitHubProjectSource::new()?;
    execute_view(url, &SystemTokenProvider, &source, &mut io::stdout())
}

trait TokenProvider {
    fn token(&self) -> Option<String>;
}

struct SystemTokenProvider;

impl TokenProvider for SystemTokenProvider {
    fn token(&self) -> Option<String> {
        std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|token| !token.is_empty())
            .or_else(|| login::get_token().ok())
    }
}

fn execute_view(
    url: &str,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
    output: &mut dyn Write,
) -> Result<(), DynError> {
    let project_ref = parse_project_url(url)?;
    let token = token_provider
        .token()
        .ok_or("no GitHub token found; set GITHUB_TOKEN or run `ghui login`")?;
    let project = source.fetch_project(&project_ref, &token)?;

    writeln!(
        output,
        "Project: {} ({} items)",
        project.title,
        project.items.len()
    )?;
    writeln!(output, "{}", render_table(&project))?;
    Ok(())
}

fn render_table(project: &Project) -> String {
    let columns = field_columns(&project.items);
    let mut header: Vec<String> = vec!["#".into(), "Type".into(), "Title".into()];
    header.extend(columns.iter().cloned());

    let rows: Vec<Vec<String>> = project
        .items
        .iter()
        .map(|item| {
            let number = item
                .content
                .as_ref()
                .and_then(|content| content.number)
                .map(|number| number.to_string())
                .unwrap_or_else(|| "-".into());
            let title = item
                .content
                .as_ref()
                .and_then(|content| content.title.clone())
                .unwrap_or_else(|| "-".into());
            let values: HashMap<&str, &str> = item
                .fields
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            let mut row = vec![number, kind_label(item).to_string(), title];
            for column in &columns {
                row.push(
                    values
                        .get(column.as_str())
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            row
        })
        .collect();

    let mut widths: Vec<usize> = header.iter().map(|cell| cell.chars().count()).collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let line = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| format!("{:<width$}", cell, width = widths[index]))
            .collect::<Vec<_>>()
            .join("  ")
    };

    let mut output = vec![
        line(&header),
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("  "),
    ];
    output.extend(rows.iter().map(|row| line(row)));
    output.join("\n")
}

fn field_columns(items: &[Item]) -> Vec<String> {
    let mut columns = Vec::new();
    for item in items {
        for (name, _) in &item.fields {
            if !columns.contains(name) {
                columns.push(name.clone());
            }
        }
    }
    columns
}

fn kind_label(item: &Item) -> &'static str {
    match item.content.as_ref().map(|content| content.kind) {
        Some(Kind::Issue) => "Issue",
        Some(Kind::PullRequest) => "PR",
        _ => "-",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::testing::MockProjectSource;
    use crate::github::Content;

    struct FixedToken(Option<String>);

    impl TokenProvider for FixedToken {
        fn token(&self) -> Option<String> {
            self.0.clone()
        }
    }

    fn sample_project() -> Project {
        Project {
            title: "Demo".into(),
            field_names: vec!["Status".into(), "Estimate".into()],
            mutable_field_names: vec!["Status".into(), "Estimate".into()],
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

    #[test]
    fn execute_view_uses_injected_dependencies() {
        let source = MockProjectSource::returning(sample_project());
        let mut output = Vec::new();

        execute_view(
            "https://github.com/orgs/example/projects/1",
            &FixedToken(Some("token".into())),
            &source,
            &mut output,
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Project: Demo (2 items)"));
        assert!(output.contains("Fix the thing"));
        assert_eq!(source.requests()[0].1, "token");
    }

    #[test]
    fn execute_view_requires_token_without_using_keychain() {
        let mut output = Vec::new();
        let error = execute_view(
            "https://github.com/orgs/example/projects/1",
            &FixedToken(None),
            &MockProjectSource::returning(sample_project()),
            &mut output,
        )
        .unwrap_err();

        assert!(error.to_string().contains("no GitHub token"));
        assert!(output.is_empty());
    }

    #[test]
    fn execute_view_propagates_project_source_errors() {
        let source = MockProjectSource::failing("request failed");
        let error = execute_view(
            "https://github.com/orgs/example/projects/1",
            &FixedToken(Some("token".into())),
            &source,
            &mut Vec::new(),
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "request failed");
        assert_eq!(source.requests().len(), 1);
    }

    #[test]
    fn render_table_columns_and_rows() {
        let table = render_table(&sample_project());
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(collapse_spaces(lines[0]), "# Type Title Status Estimate");
        assert_eq!(
            collapse_spaces(lines[2]),
            "42 Issue Fix the thing In Progress 3"
        );
        assert_eq!(collapse_spaces(lines[3]), "7 PR Add feature Done");
    }

    #[test]
    fn render_table_handles_missing_content_and_empty_projects() {
        let project = Project {
            title: "P".into(),
            field_names: vec!["Status".into()],
            mutable_field_names: vec!["Status".into()],
            items: vec![Item {
                content: None,
                fields: vec![("Status".into(), "Todo".into())],
            }],
        };
        assert!(render_table(&project)
            .lines()
            .nth(2)
            .unwrap()
            .starts_with('-'));
        assert_eq!(
            render_table(&Project {
                title: "Empty".into(),
                field_names: vec![],
                mutable_field_names: vec![],
                items: vec![],
            })
            .lines()
            .count(),
            2
        );
    }

    fn collapse_spaces(line: &str) -> String {
        line.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
