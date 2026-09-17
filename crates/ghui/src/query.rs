use std::cmp::Ordering;

use crate::github::{ContentState, Item, Kind, STATUS_COLUMN};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilterExpression {
    clauses: Vec<Clause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Clause {
    negated: bool,
    field: Option<String>,
    values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SortSpec {
    pub(crate) field: String,
    pub(crate) descending: bool,
}

impl FilterExpression {
    pub(crate) fn parse(expression: &str) -> Result<Self, String> {
        let clauses = tokenize(expression)?
            .into_iter()
            .map(|token| {
                let (negated, token) = token
                    .strip_prefix('-')
                    .map_or((false, token.as_str()), |token| (true, token));
                if token.is_empty() {
                    return Err("filter contains an empty negated clause".into());
                }
                let (field, value) = token
                    .split_once(':')
                    .map_or((None, token), |(field, value)| {
                        (Some(field.to_string()), value)
                    });
                if value.is_empty() {
                    return Err(format!("filter clause '{token}' is missing a value"));
                }
                let values = value.split(',').map(str::to_string).collect::<Vec<_>>();
                if let Some(keyword) = values.iter().find(|value| value.starts_with('@')) {
                    return Err(format!("filter keyword '{keyword}' is not supported"));
                }
                Ok(Clause {
                    negated,
                    field,
                    values,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { clauses })
    }

    pub(crate) fn matches(&self, item: &Item) -> bool {
        self.clauses.iter().all(|clause| {
            let matched = clause.matches(item);
            matched != clause.negated
        })
    }
}

impl Clause {
    fn matches(&self, item: &Item) -> bool {
        let Some(field) = self.field.as_deref() else {
            return self
                .values
                .iter()
                .any(|value| matches_general_text(item, value));
        };
        match normalized(field).as_str() {
            "has" => self
                .values
                .iter()
                .any(|name| item_value(item, name).is_some_and(has_value)),
            "no" => self
                .values
                .iter()
                .any(|name| item_value(item, name).is_none_or(|value| !has_value(value))),
            "is" => self.values.iter().any(|value| matches_is(item, value)),
            "reason" => self.values.iter().any(|value| {
                item.content
                    .as_ref()
                    .and_then(|content| content.state_reason.as_deref())
                    .is_some_and(|reason| matches_value(reason, value))
            }),
            _ => item_value(item, field).is_some_and(|actual| {
                self.values.iter().any(|expected| {
                    actual
                        .split(',')
                        .map(str::trim)
                        .any(|actual| matches_value(actual, expected))
                })
            }),
        }
    }
}

impl SortSpec {
    pub(crate) fn parse(specification: &str) -> Result<Self, String> {
        let specification = specification.trim();
        if specification.is_empty() {
            return Err("sort is missing a field".into());
        }
        let (field, direction) = match specification.rsplit_once(':') {
            Some((field, direction)) if direction.eq_ignore_ascii_case("asc") => (field, "asc"),
            Some((field, direction)) if direction.eq_ignore_ascii_case("desc") => (field, "desc"),
            Some((_, direction)) => {
                return Err(format!(
                    "invalid sort direction '{direction}'; use asc or desc"
                ));
            }
            None => (specification, "asc"),
        };
        let field = field.trim_matches(['\'', '"']).trim();
        if field.is_empty() {
            return Err("sort is missing a field".into());
        }
        Ok(Self {
            field: field.to_string(),
            descending: direction.eq_ignore_ascii_case("desc"),
        })
    }
}

pub(crate) fn select_items<'a>(
    items: &'a [Item],
    filter: Option<&FilterExpression>,
    sort: Option<&SortSpec>,
) -> Vec<&'a Item> {
    let mut selected = items
        .iter()
        .filter(|item| filter.is_none_or(|filter| filter.matches(item)))
        .collect::<Vec<_>>();
    if let Some(sort) = sort {
        selected.sort_by(|left, right| {
            compare_optional(
                item_value(left, &sort.field),
                item_value(right, &sort.field),
                sort.descending,
            )
        });
    }
    selected
}

fn item_value<'a>(item: &'a Item, field: &str) -> Option<&'a str> {
    let field_name = normalized(field);
    let content = item.content.as_ref();
    match field_name.as_str() {
        "title" => content.and_then(|content| content.title.as_deref()),
        "number" | "#" => None,
        "type" => Some(match content.map(|content| content.kind) {
            Some(Kind::Issue) => "Issue",
            Some(Kind::PullRequest) => "PR",
            _ => "",
        }),
        "state" if field.eq_ignore_ascii_case(STATUS_COLUMN) => {
            Some(match content.map(|content| content.state) {
                Some(ContentState::Open) => "open",
                Some(ContentState::Closed) => "closed",
                Some(ContentState::Merged) => "merged",
                _ => "",
            })
        }
        "repo" | "repository" => project_field(item, "Repository"),
        "assignee" | "assignees" => project_field(item, "Assignees"),
        "label" | "labels" => project_field(item, "Labels"),
        "reviewer" | "reviewers" => project_field(item, "Reviewers"),
        "milestone" => project_field(item, "Milestone"),
        _ => item
            .fields
            .iter()
            .find(|(name, _)| normalized(name) == field_name)
            .map(|(_, value)| value.as_str()),
    }
}

fn project_field<'a>(item: &'a Item, field: &str) -> Option<&'a str> {
    item.fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(field))
        .map(|(_, value)| value.as_str())
}

fn matches_is(item: &Item, expected: &str) -> bool {
    let content = item.content.as_ref();
    match normalized(expected).as_str() {
        "issue" => content.is_some_and(|content| content.kind == Kind::Issue),
        "pr" | "pullrequest" => content.is_some_and(|content| content.kind == Kind::PullRequest),
        "open" => content.is_some_and(|content| content.state == ContentState::Open),
        "closed" => content.is_some_and(|content| content.state == ContentState::Closed),
        "merged" => content.is_some_and(|content| content.state == ContentState::Merged),
        _ => false,
    }
}

fn matches_general_text(item: &Item, expected: &str) -> bool {
    item.content
        .as_ref()
        .and_then(|content| content.title.as_deref())
        .into_iter()
        .chain(item.fields.iter().map(|(_, value)| value.as_str()))
        .any(|actual| {
            if expected.contains('*') {
                matches_value(actual, expected)
            } else {
                actual
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word.to_lowercase().starts_with(&expected.to_lowercase()))
            }
        })
}

fn matches_value(actual: &str, expected: &str) -> bool {
    if let Some((operator, expected)) = comparison(expected) {
        return compare_scalar(actual, expected).is_some_and(|ordering| match operator {
            ">" => ordering.is_gt(),
            ">=" => ordering.is_ge(),
            "<" => ordering.is_lt(),
            "<=" => ordering.is_le(),
            _ => false,
        });
    }
    if let Some((start, end)) = expected.split_once("..") {
        let after_start =
            start == "*" || compare_scalar(actual, start).is_some_and(Ordering::is_ge);
        let before_end = end == "*" || compare_scalar(actual, end).is_some_and(Ordering::is_le);
        return after_start && before_end;
    }
    wildcard_match(actual, expected)
}

fn comparison(expected: &str) -> Option<(&str, &str)> {
    [">=", "<=", ">", "<"].into_iter().find_map(|operator| {
        expected
            .strip_prefix(operator)
            .map(|value| (operator, value))
    })
}

fn compare_scalar(left: &str, right: &str) -> Option<Ordering> {
    match (left.parse::<f64>(), right.parse::<f64>()) {
        (Ok(left), Ok(right)) => left.partial_cmp(&right),
        _ => Some(left.to_lowercase().cmp(&right.to_lowercase())),
    }
}

fn compare_optional(left: Option<&str>, right: Option<&str>, descending: bool) -> Ordering {
    match (
        left.filter(|value| has_value(value)),
        right.filter(|value| has_value(value)),
    ) {
        (Some(left), Some(right)) => {
            let ordering = compare_scalar(left, right).unwrap_or(Ordering::Equal);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn wildcard_match(actual: &str, expected: &str) -> bool {
    let actual = actual.to_lowercase();
    let expected = expected.to_lowercase();
    match (expected.starts_with('*'), expected.ends_with('*')) {
        (true, true) => actual.contains(expected.trim_matches('*')),
        (true, false) => actual.ends_with(expected.trim_start_matches('*')),
        (false, true) => actual.starts_with(expected.trim_end_matches('*')),
        (false, false) => actual == expected,
    }
}

fn has_value(value: &str) -> bool {
    !value.trim().is_empty()
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric() || *character == '#')
        .flat_map(char::to_lowercase)
        .collect()
}

fn tokenize(expression: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in expression.chars() {
        if escaped {
            token.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if quote.is_none() && character.is_whitespace() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(character);
        }
    }
    if quote.is_some() {
        return Err("filter contains an unterminated quote".into());
    }
    if escaped {
        token.push('\\');
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{Content, ContentState};

    fn item(kind: Kind, state: ContentState, title: &str, fields: &[(&str, &str)]) -> Item {
        Item {
            id: title.into(),
            content: Some(Content {
                kind,
                state,
                state_reason: Some("COMPLETED".into()),
                number: Some(1),
                title: Some(title.into()),
                url: None,
            }),
            fields: fields
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect(),
        }
    }

    #[test]
    fn supports_github_style_filter_clauses() {
        let issue = item(
            Kind::Issue,
            ContentState::Open,
            "Render API docs",
            &[
                ("Status", "In Progress"),
                ("Labels", "bug, docs"),
                ("Points", "3"),
            ],
        );
        for expression in [
            "is:issue is:open",
            "status:\"In Progress\" label:bug,feature",
            "has:labels -label:wontfix",
            "points:>2 points:1..5",
            "Render",
            "title:*API*",
        ] {
            assert!(
                FilterExpression::parse(expression).unwrap().matches(&issue),
                "{expression}"
            );
        }
        assert!(!FilterExpression::parse("no:status")
            .unwrap()
            .matches(&issue));
    }

    #[test]
    fn rejects_unterminated_quotes() {
        assert_eq!(
            FilterExpression::parse("status:\"In Progress").unwrap_err(),
            "filter contains an unterminated quote"
        );
    }

    #[test]
    fn rejects_unsupported_keywords_and_sort_directions() {
        assert_eq!(
            FilterExpression::parse("assignee:@me").unwrap_err(),
            "filter keyword '@me' is not supported"
        );
        assert_eq!(
            SortSpec::parse("Priority:sideways").unwrap_err(),
            "invalid sort direction 'sideways'; use asc or desc"
        );
    }

    #[test]
    fn filters_and_sorts_values_with_missing_values_last() {
        let items = vec![
            item(Kind::Issue, ContentState::Open, "Three", &[("Points", "3")]),
            item(Kind::Issue, ContentState::Closed, "Missing", &[]),
            item(Kind::Issue, ContentState::Open, "One", &[("Points", "1")]),
        ];
        let filter = FilterExpression::parse("is:open").unwrap();
        let sort = SortSpec::parse("Points:desc").unwrap();
        let selected = select_items(&items, Some(&filter), Some(&sort));

        assert_eq!(
            selected
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["Three", "One"]
        );

        let selected = select_items(&items, None, Some(&sort));
        assert_eq!(
            selected
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["Three", "One", "Missing"]
        );
    }
}
