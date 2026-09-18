//! GitHub API access and project domain types.

use std::error::Error;
use std::time::Duration;

use serde::Deserialize;

pub(crate) type DynError = Box<dyn Error>;

const GRAPHQL_URL: &str = "https://api.github.com/graphql";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

const PROJECT_ITEMS_QUERY: &str = r#"
query GetProjectItems(
  $owner: String!
  $number: Int!
  $cursor: String
  $isOrganization: Boolean!
  $isUser: Boolean!
) {
  organization(login: $owner) @include(if: $isOrganization) {
    project: projectV2(number: $number) { ...ProjectFields }
  }
  user(login: $owner) @include(if: $isUser) {
    project: projectV2(number: $number) { ...ProjectFields }
  }
}

fragment ProjectFields on ProjectV2 {
    id
    title
    fields(first: 100) {
        nodes {
            __typename
                        ... on ProjectV2Field { id name dataType }
                        ... on ProjectV2IterationField {
                            id name
                            configuration {
                                iterations { id title }
                                completedIterations { id title }
                            }
                        }
                        ... on ProjectV2SingleSelectField { id name options { id name } }
        }
    }
    items(first: 40, after: $cursor) {
    pageInfo { hasNextPage endCursor }
    nodes {
            id
      fieldValues(first: 100) {
        nodes {
          __typename
          ... on ProjectV2ItemFieldTextValue {
            field { ... on ProjectV2FieldCommon { name } }
            text
          }
          ... on ProjectV2ItemFieldNumberValue {
            field { ... on ProjectV2FieldCommon { name } }
            number
          }
          ... on ProjectV2ItemFieldDateValue {
            field { ... on ProjectV2FieldCommon { name } }
            date
          }
          ... on ProjectV2ItemFieldSingleSelectValue {
            field { ... on ProjectV2FieldCommon { name } }
            name
          }
          ... on ProjectV2ItemFieldIterationValue {
            field { ... on ProjectV2FieldCommon { name } }
            title
          }
                    ... on ProjectV2ItemFieldMultiSelectValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        options { name }
                    }
                    ... on ProjectV2ItemFieldRepositoryValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        repository { nameWithOwner }
                    }
                    ... on ProjectV2ItemFieldLabelValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        labels(first: 100) { nodes { name } }
                    }
                    ... on ProjectV2ItemFieldMilestoneValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        milestone { title }
                    }
                    ... on ProjectV2ItemFieldPullRequestValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        pullRequests(first: 100) { nodes { number repository { nameWithOwner } } }
                    }
                    ... on ProjectV2ItemFieldReviewerValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        reviewers(first: 100) { nodes {
                            ... on Bot { displayName: login }
                            ... on EnterpriseTeam { displayName: name }
                            ... on Mannequin { displayName: login }
                            ... on Team { displayName: name }
                            ... on User { displayName: login }
                        } }
                    }
                    ... on ProjectV2ItemFieldUserValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        users(first: 100) { nodes { login } }
                    }
                    ... on ProjectV2ItemIssueFieldValue {
                        field { ... on ProjectV2FieldCommon { name } }
                        issueFieldValue {
                            __typename
                            ... on IssueFieldTextValue { value }
                            ... on IssueFieldDateValue { value }
                            ... on IssueFieldNumberValue { value }
                            ... on IssueFieldSingleSelectValue { value }
                            ... on IssueFieldMultiSelectValue { value }
                        }
                    }
        }
      }
      content {
        __typename
        ... on Issue { number title url state stateReason }
        ... on PullRequest { number title url state }
      }
    }
  }
}
"#;

#[cfg(feature = "tui")]
const UPDATE_FIELD_MUTATION: &str = r#"
mutation UpdateProjectItemField(
    $projectId: ID!
    $itemId: ID!
    $fieldId: ID!
    $value: ProjectV2FieldValue!
) {
    updateProjectV2ItemFieldValue(input: {
        projectId: $projectId
        itemId: $itemId
        fieldId: $fieldId
        value: $value
    }) { projectV2Item { id } }
}
"#;

#[cfg(feature = "tui")]
const DELETE_ITEM_MUTATION: &str = r#"
mutation DeleteProjectItem($projectId: ID!, $itemId: ID!) {
    deleteProjectV2Item(input: { projectId: $projectId, itemId: $itemId }) {
        deletedItemId
    }
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRef {
    owner: String,
    number: u32,
    owner_kind: OwnerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerKind {
    Organization,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Issue,
    PullRequest,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ContentState {
    Open,
    Closed,
    Merged,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Content {
    pub(crate) kind: Kind,
    pub(crate) state: ContentState,
    pub(crate) state_reason: Option<String>,
    pub(crate) number: Option<u32>,
    pub(crate) title: Option<String>,
    pub(crate) url: Option<String>,
}

pub(crate) const STATUS_COLUMN: &str = "State";

pub(crate) fn emoji_status(item: &Item) -> &'static str {
    let Some(content) = &item.content else {
        return "";
    };
    match content.kind {
        Kind::PullRequest => match content.state {
            ContentState::Open => "🟢",
            ContentState::Closed => "🛑",
            ContentState::Merged => "🏁",
            ContentState::Unknown => "",
        },
        Kind::Issue => {
            let status = item_field(item, "Status");
            let labels = item_field(item, "Labels");
            if status.is_some_and(|value| normalized(value).contains("duplicate"))
                || labels.is_some_and(|value| {
                    value
                        .split(',')
                        .any(|label| normalized(label) == "duplicate")
                })
            {
                "❓"
            } else if status.is_some_and(|value| normalized(value) == "inprogress") {
                "🏃"
            } else if content.state == ContentState::Closed
                && (content.state_reason.as_deref() == Some("COMPLETED")
                    || status.is_some_and(|value| {
                        matches!(normalized(value).as_str(), "fixed" | "done" | "completed")
                    }))
            {
                "✅"
            } else if content.state == ContentState::Closed {
                "❌"
            } else if content.state == ContentState::Open {
                "⚠️"
            } else {
                ""
            }
        }
        Kind::Other => "",
    }
}

fn item_field<'a>(item: &'a Item, name: &str) -> Option<&'a str> {
    item.fields
        .iter()
        .find(|(field, _)| field.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) id: String,
    pub(crate) content: Option<Content>,
    pub(crate) fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldOption {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditableFieldKind {
    Text,
    Number,
    Date,
    SingleSelect,
    Iteration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditableField {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: EditableFieldKind,
    pub(crate) options: Vec<FieldOption>,
}

#[cfg(feature = "tui")]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FieldValue {
    Text(String),
    Number(f64),
    Date(String),
    SingleSelect(String),
    Iteration(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) field_names: Vec<String>,
    pub(crate) mutable_field_names: Vec<String>,
    pub(crate) editable_fields: Vec<EditableField>,
    pub(crate) items: Vec<Item>,
}

pub(crate) trait ProjectSource {
    fn fetch_project(&self, project_ref: &ProjectRef, token: &str) -> Result<Project, DynError>;
    #[cfg(feature = "tui")]
    fn update_field(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: FieldValue,
        token: &str,
    ) -> Result<(), DynError>;
    #[cfg(feature = "tui")]
    fn remove_item(&self, project_id: &str, item_id: &str, token: &str) -> Result<(), DynError>;
}

pub(crate) fn parse_project_url(input: &str) -> Result<ProjectRef, DynError> {
    let input = input.trim();
    let path = input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .ok_or_else(|| {
            format!(
                "unsupported project URL: {input} (expected a https://github.com/... project URL)"
            )
        })?;
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let project_index = segments
        .iter()
        .position(|segment| *segment == "projects")
        .ok_or_else(|| format!("unsupported project URL: {input}"))?;
    let number = segments
        .get(project_index + 1)
        .and_then(|number| number.parse::<u32>().ok())
        .ok_or_else(|| format!("invalid or missing project number in URL: {input}"))?;
    let (owner, owner_kind) = match segments.as_slice() {
        ["orgs", owner, "projects", ..] => ((*owner).to_string(), OwnerKind::Organization),
        ["users", owner, "projects", ..] => ((*owner).to_string(), OwnerKind::User),
        _ => return Err(format!("unsupported project URL: {input}").into()),
    };

    Ok(ProjectRef {
        owner,
        number,
        owner_kind,
    })
}

pub(crate) struct GitHubProjectSource<T = ReqwestGraphQlTransport> {
    transport: T,
}

impl GitHubProjectSource<ReqwestGraphQlTransport> {
    pub(crate) fn new() -> Result<Self, DynError> {
        Ok(Self {
            transport: ReqwestGraphQlTransport::new()?,
        })
    }
}

impl<T: GraphQlTransport> ProjectSource for GitHubProjectSource<T> {
    fn fetch_project(&self, project_ref: &ProjectRef, token: &str) -> Result<Project, DynError> {
        let mut title = String::new();
        let mut id = String::new();
        let mut field_names = Vec::new();
        let mut mutable_field_names = Vec::new();
        let mut editable_fields = Vec::new();
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let payload = serde_json::json!({
                "query": PROJECT_ITEMS_QUERY,
                "variables": {
                    "owner": project_ref.owner,
                    "number": project_ref.number,
                    "cursor": cursor,
                    "isOrganization": project_ref.owner_kind == OwnerKind::Organization,
                    "isUser": project_ref.owner_kind == OwnerKind::User,
                },
            });
            let response = self.transport.execute(token, &payload)?;
            if !(200..300).contains(&response.status) {
                return Err(http_error(&response));
            }

            let page = decode_project(&response.body, response.rate_limit.as_ref())?;
            if title.is_empty() {
                id = page.id;
                title = page.title;
                field_names = page.field_names;
                mutable_field_names = page.mutable_field_names;
                editable_fields = page.editable_fields;
            }
            items.extend(page.items);
            if page.has_next_page {
                cursor =
                    Some(page.end_cursor.ok_or(
                        "GitHub API response indicated another page but omitted the cursor",
                    )?);
            } else {
                break;
            }
        }

        Ok(Project {
            id,
            title,
            field_names,
            mutable_field_names,
            editable_fields,
            items,
        })
    }

    #[cfg(feature = "tui")]
    fn update_field(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: FieldValue,
        token: &str,
    ) -> Result<(), DynError> {
        let value = match value {
            FieldValue::Text(value) => serde_json::json!({ "text": value }),
            FieldValue::Number(value) => serde_json::json!({ "number": value }),
            FieldValue::Date(value) => serde_json::json!({ "date": value }),
            FieldValue::SingleSelect(value) => {
                serde_json::json!({ "singleSelectOptionId": value })
            }
            FieldValue::Iteration(value) => serde_json::json!({ "iterationId": value }),
        };
        let payload = serde_json::json!({
            "query": UPDATE_FIELD_MUTATION,
            "variables": {
                "projectId": project_id,
                "itemId": item_id,
                "fieldId": field_id,
                "value": value,
            },
        });
        let response = self.transport.execute(token, &payload)?;
        if !(200..300).contains(&response.status) {
            return Err(http_error(&response));
        }
        let rate_limit = response.rate_limit;
        let response: MutationResponse = serde_json::from_str(&response.body)?;
        if response.errors.is_empty() {
            Ok(())
        } else {
            Err(api_error(&response.errors, rate_limit.as_ref()))
        }
    }

    #[cfg(feature = "tui")]
    fn remove_item(&self, project_id: &str, item_id: &str, token: &str) -> Result<(), DynError> {
        let payload = serde_json::json!({
            "query": DELETE_ITEM_MUTATION,
            "variables": {
                "projectId": project_id,
                "itemId": item_id,
            },
        });
        let response = self.transport.execute(token, &payload)?;
        if !(200..300).contains(&response.status) {
            return Err(http_error(&response));
        }
        let rate_limit = response.rate_limit;
        let response: MutationResponse = serde_json::from_str(&response.body)?;
        if response.errors.is_empty() {
            Ok(())
        } else {
            Err(api_error(&response.errors, rate_limit.as_ref()))
        }
    }
}

struct GraphQlResponse {
    status: u16,
    body: String,
    rate_limit: Option<RateLimit>,
}

struct RateLimit {
    limit: Option<u64>,
    remaining: Option<u64>,
    reset: Option<u64>,
}

trait GraphQlTransport {
    fn execute(
        &self,
        token: &str,
        payload: &serde_json::Value,
    ) -> Result<GraphQlResponse, DynError>;
}

pub(crate) struct ReqwestGraphQlTransport {
    http: reqwest::blocking::Client,
    endpoint: String,
}

impl ReqwestGraphQlTransport {
    fn new() -> Result<Self, DynError> {
        Self::with_endpoint(GRAPHQL_URL)
    }

    fn with_endpoint(endpoint: impl Into<String>) -> Result<Self, DynError> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("ghui/", env!("CARGO_PKG_VERSION")))
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            endpoint: endpoint.into(),
        })
    }
}

impl GraphQlTransport for ReqwestGraphQlTransport {
    fn execute(
        &self,
        token: &str,
        payload: &serde_json::Value,
    ) -> Result<GraphQlResponse, DynError> {
        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(token)
            .json(payload)
            .send()?;
        let status = response.status().as_u16();
        let rate_limit = RateLimit::from_headers(response.headers());
        let body = response.text()?;
        Ok(GraphQlResponse {
            status,
            body,
            rate_limit,
        })
    }
}

impl RateLimit {
    fn from_headers(headers: &reqwest::header::HeaderMap) -> Option<Self> {
        fn value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
            headers.get(name)?.to_str().ok()?.parse().ok()
        }

        let rate_limit = Self {
            limit: value(headers, "x-ratelimit-limit"),
            remaining: value(headers, "x-ratelimit-remaining"),
            reset: value(headers, "x-ratelimit-reset"),
        };
        (rate_limit.limit.is_some() || rate_limit.remaining.is_some() || rate_limit.reset.is_some())
            .then_some(rate_limit)
    }

    fn display(&self) -> String {
        let remaining = self
            .remaining
            .map_or_else(|| "unknown".into(), |value| value.to_string());
        let limit = self
            .limit
            .map_or_else(|| "unknown".into(), |value| value.to_string());
        let reset = self.reset.map_or_else(
            || "unknown".into(),
            |value| format!("Unix timestamp {value}"),
        );
        format!("rate limit {remaining}/{limit} remaining; resets at {reset}")
    }
}

struct DecodedPage {
    id: String,
    title: String,
    field_names: Vec<String>,
    mutable_field_names: Vec<String>,
    editable_fields: Vec<EditableField>,
    items: Vec<Item>,
    has_next_page: bool,
    end_cursor: Option<String>,
}

fn decode_project(body: &str, rate_limit: Option<&RateLimit>) -> Result<DecodedPage, DynError> {
    let response: PageResponse = serde_json::from_str(body)?;
    let blocking_errors = response
        .errors
        .iter()
        .filter(|error| !error.is_restricted_field_value())
        .collect::<Vec<_>>();
    if !blocking_errors.is_empty() {
        let messages = blocking_errors
            .into_iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(api_error_message(&messages, rate_limit));
    }

    let data = response
        .data
        .ok_or_else(|| "GitHub API response was missing 'data'".to_string())?;
    let project = data
        .organization
        .or(data.user)
        .and_then(|owner| owner.project)
        .ok_or_else(|| "project not found (check the project URL and your token)".to_string())?;
    let fields = project.fields.nodes;
    Ok(DecodedPage {
        id: project.id,
        title: project.title,
        field_names: fields
            .iter()
            .filter_map(|field| (field.name != "Title").then_some(field.name.clone()))
            .collect(),
        mutable_field_names: fields
            .iter()
            .filter_map(|field| field.editable().map(|field| field.name))
            .collect(),
        editable_fields: fields
            .into_iter()
            .filter_map(|field| field.editable())
            .collect(),
        items: project.items.nodes.into_iter().map(Item::from).collect(),
        has_next_page: project.items.page_info.has_next_page,
        end_cursor: project.items.page_info.end_cursor,
    })
}

#[cfg(feature = "tui")]
fn api_error(errors: &[ApiError], rate_limit: Option<&RateLimit>) -> DynError {
    let messages = errors
        .iter()
        .map(|error| error.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    api_error_message(&messages, rate_limit)
}

fn api_error_message(messages: &str, rate_limit: Option<&RateLimit>) -> DynError {
    match rate_limit {
        Some(rate_limit) => {
            format!("GitHub API error: {messages} ({})", rate_limit.display()).into()
        }
        None => format!("GitHub API error: {messages}").into(),
    }
}

fn http_error(response: &GraphQlResponse) -> DynError {
    let message = format!(
        "GitHub GraphQL request failed with HTTP {}: {}",
        response.status,
        response_snippet(&response.body)
    );
    match &response.rate_limit {
        Some(rate_limit) => format!("{message} ({})", rate_limit.display()).into(),
        None => message.into(),
    }
}

fn response_snippet(body: &str) -> String {
    let body = body.trim();
    if body.len() <= 300 {
        return body.to_string();
    }
    let mut end = 300;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &body[..end])
}

fn field_value(value: FieldValueData) -> Option<(String, String)> {
    match value {
        FieldValueData::Text { field, text } => display_field(field, text.unwrap_or_default()),
        FieldValueData::Number { field, number } => {
            display_field(field, number.map(format_number).unwrap_or_default())
        }
        FieldValueData::Date { field, date } => display_field(field, date.unwrap_or_default()),
        FieldValueData::SingleSelect { field, name } => {
            display_field(field, name.unwrap_or_default())
        }
        FieldValueData::Iteration { field, title } => {
            display_field(field, title.unwrap_or_default())
        }
        FieldValueData::MultiSelect { field, options } => display_field(
            field,
            join_values(options.into_iter().map(|option| option.name)),
        ),
        FieldValueData::Repository { field, repository } => {
            display_field(field, repository.name_with_owner)
        }
        FieldValueData::Labels { field, labels } => display_field(field, labels.names()),
        FieldValueData::Milestone { field, milestone } => display_field(field, milestone.title),
        FieldValueData::PullRequests {
            field,
            pull_requests,
        } => display_field(field, pull_requests.references()),
        FieldValueData::Reviewers { field, reviewers } => {
            display_field(field, reviewers.display_names())
        }
        FieldValueData::Users { field, users } => display_field(field, users.logins()),
        FieldValueData::IssueField {
            field,
            issue_field_value,
        } => display_field(
            field,
            issue_field_value
                .map(IssueFieldValueData::display)
                .unwrap_or_default(),
        ),
        FieldValueData::Unsupported => None,
    }
}

fn display_field(field: FieldData, value: String) -> Option<(String, String)> {
    (field.name != "Title").then_some((field.name, value))
}

fn format_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

#[derive(Default, Deserialize)]
struct PageResponse {
    #[serde(default)]
    data: Option<PageData>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Default, Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    path: Vec<serde_json::Value>,
}

impl ApiError {
    fn is_restricted_field_value(&self) -> bool {
        self.message
            .contains("has enabled OAuth App access restrictions")
            && self.path.iter().any(|segment| segment == "fieldValues")
    }
}

#[derive(Default, Deserialize)]
struct PageData {
    #[serde(default)]
    organization: Option<OwnerData>,
    #[serde(default)]
    user: Option<OwnerData>,
}

#[derive(Default, Deserialize)]
struct OwnerData {
    #[serde(default, rename = "project")]
    project: Option<ProjectData>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectData {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    fields: FieldsConnection,
    #[serde(default)]
    items: ItemsConnection,
}

#[derive(Default, Deserialize)]
struct FieldsConnection {
    #[serde(default)]
    nodes: Vec<ProjectFieldData>,
}

#[derive(Default, Deserialize)]
struct ProjectFieldData {
    #[serde(default, rename = "__typename")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    id: String,
    #[serde(default, rename = "dataType")]
    data_type: String,
    #[serde(default)]
    options: Vec<FieldOptionData>,
    #[serde(default)]
    configuration: IterationConfigurationData,
}

impl ProjectFieldData {
    fn editable(&self) -> Option<EditableField> {
        let kind = match (self.kind.as_str(), self.data_type.as_str()) {
            ("ProjectV2Field", "TEXT") => EditableFieldKind::Text,
            ("ProjectV2Field", "NUMBER") => EditableFieldKind::Number,
            ("ProjectV2Field", "DATE") => EditableFieldKind::Date,
            ("ProjectV2SingleSelectField", _) => EditableFieldKind::SingleSelect,
            ("ProjectV2IterationField", _) => EditableFieldKind::Iteration,
            _ => return None,
        };
        let options = match kind {
            EditableFieldKind::Iteration => self
                .configuration
                .iterations
                .iter()
                .chain(&self.configuration.completed_iterations)
                .map(FieldOption::from)
                .collect(),
            _ => self.options.iter().map(FieldOption::from).collect(),
        };
        Some(EditableField {
            id: self.id.clone(),
            name: self.name.clone(),
            kind,
            options,
        })
    }
}

#[derive(Default, Deserialize)]
struct FieldOptionData {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    title: String,
}

impl From<&FieldOptionData> for FieldOption {
    fn from(option: &FieldOptionData) -> Self {
        Self {
            id: option.id.clone(),
            name: if option.name.is_empty() {
                option.title.clone()
            } else {
                option.name.clone()
            },
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IterationConfigurationData {
    #[serde(default)]
    iterations: Vec<FieldOptionData>,
    #[serde(default)]
    completed_iterations: Vec<FieldOptionData>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemsConnection {
    #[serde(default)]
    nodes: Vec<ItemData>,
    #[serde(default)]
    page_info: PageInfo,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    #[serde(default)]
    has_next_page: bool,
    #[serde(default)]
    end_cursor: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemData {
    #[serde(default)]
    id: String,
    #[serde(default)]
    field_values: FieldValuesConnection,
    #[serde(default)]
    content: Option<ContentData>,
}

#[derive(Default, Deserialize)]
struct FieldValuesConnection {
    #[serde(default)]
    nodes: Vec<Option<FieldValueData>>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum FieldValueData {
    #[serde(rename = "ProjectV2ItemFieldTextValue")]
    Text {
        field: FieldData,
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(rename = "ProjectV2ItemFieldNumberValue")]
    Number {
        field: FieldData,
        #[serde(default)]
        number: Option<f64>,
    },
    #[serde(rename = "ProjectV2ItemFieldDateValue")]
    Date {
        field: FieldData,
        #[serde(default)]
        date: Option<String>,
    },
    #[serde(rename = "ProjectV2ItemFieldSingleSelectValue")]
    SingleSelect {
        field: FieldData,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "ProjectV2ItemFieldIterationValue")]
    Iteration {
        field: FieldData,
        #[serde(default)]
        title: Option<String>,
    },
    #[serde(rename = "ProjectV2ItemFieldMultiSelectValue")]
    MultiSelect {
        field: FieldData,
        #[serde(default)]
        options: Vec<NamedData>,
    },
    #[serde(rename = "ProjectV2ItemFieldRepositoryValue")]
    Repository {
        field: FieldData,
        repository: RepositoryData,
    },
    #[serde(rename = "ProjectV2ItemFieldLabelValue")]
    Labels {
        field: FieldData,
        #[serde(default)]
        labels: NamedConnection,
    },
    #[serde(rename = "ProjectV2ItemFieldMilestoneValue")]
    Milestone {
        field: FieldData,
        milestone: MilestoneData,
    },
    #[serde(rename = "ProjectV2ItemFieldPullRequestValue")]
    PullRequests {
        field: FieldData,
        #[serde(default, rename = "pullRequests")]
        pull_requests: PullRequestConnection,
    },
    #[serde(rename = "ProjectV2ItemFieldReviewerValue")]
    Reviewers {
        field: FieldData,
        #[serde(default)]
        reviewers: ReviewerConnection,
    },
    #[serde(rename = "ProjectV2ItemFieldUserValue")]
    Users {
        field: FieldData,
        #[serde(default)]
        users: UserConnection,
    },
    #[serde(rename = "ProjectV2ItemIssueFieldValue")]
    IssueField {
        field: FieldData,
        #[serde(default, rename = "issueFieldValue")]
        issue_field_value: Option<IssueFieldValueData>,
    },
    #[serde(other)]
    Unsupported,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryData {
    #[serde(default)]
    name_with_owner: String,
}

#[derive(Default, Deserialize)]
struct NamedConnection {
    #[serde(default)]
    nodes: Vec<Option<NamedData>>,
}

impl NamedConnection {
    fn names(self) -> String {
        join_values(self.nodes.into_iter().flatten().map(|node| node.name))
    }
}

#[derive(Default, Deserialize)]
struct NamedData {
    #[serde(default)]
    name: String,
}

#[derive(Default, Deserialize)]
struct MilestoneData {
    #[serde(default)]
    title: String,
}

#[derive(Default, Deserialize)]
struct PullRequestConnection {
    #[serde(default)]
    nodes: Vec<Option<PullRequestData>>,
}

impl PullRequestConnection {
    fn references(self) -> String {
        join_values(self.nodes.into_iter().flatten().map(|pull_request| {
            format!(
                "{}#{}",
                pull_request.repository.name_with_owner, pull_request.number
            )
        }))
    }
}

#[derive(Default, Deserialize)]
struct PullRequestData {
    #[serde(default)]
    number: u32,
    #[serde(default)]
    repository: RepositoryData,
}

#[derive(Default, Deserialize)]
struct ReviewerConnection {
    #[serde(default)]
    nodes: Vec<Option<ReviewerData>>,
}

impl ReviewerConnection {
    fn display_names(self) -> String {
        join_values(
            self.nodes
                .into_iter()
                .flatten()
                .map(|reviewer| reviewer.display_name),
        )
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewerData {
    #[serde(default)]
    display_name: String,
}

#[derive(Default, Deserialize)]
struct UserConnection {
    #[serde(default)]
    nodes: Vec<Option<UserData>>,
}

impl UserConnection {
    fn logins(self) -> String {
        join_values(self.nodes.into_iter().flatten().map(|user| user.login))
    }
}

#[derive(Default, Deserialize)]
struct UserData {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum IssueFieldValueData {
    IssueFieldTextValue {
        value: String,
    },
    IssueFieldDateValue {
        value: String,
    },
    IssueFieldNumberValue {
        value: f64,
    },
    IssueFieldSingleSelectValue {
        value: String,
    },
    IssueFieldMultiSelectValue {
        value: Option<String>,
    },
    #[serde(other)]
    Unsupported,
}

impl IssueFieldValueData {
    fn display(self) -> String {
        match self {
            Self::IssueFieldTextValue { value }
            | Self::IssueFieldDateValue { value }
            | Self::IssueFieldSingleSelectValue { value } => value,
            Self::IssueFieldNumberValue { value } => format_number(value),
            Self::IssueFieldMultiSelectValue { value } => value.unwrap_or_default(),
            Self::Unsupported => String::new(),
        }
    }
}

fn join_values(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Default, Deserialize)]
struct FieldData {
    #[serde(default)]
    name: String,
}

#[derive(Default, Deserialize)]
struct ContentData {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default, rename = "stateReason")]
    state_reason: Option<String>,
    #[serde(default)]
    number: Option<u32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

impl From<ItemData> for Item {
    fn from(data: ItemData) -> Self {
        let content = data.content.map(|content| Content {
            kind: match content.kind.as_deref() {
                Some("Issue") => Kind::Issue,
                Some("PullRequest") => Kind::PullRequest,
                _ => Kind::Other,
            },
            state: match content.state.as_deref() {
                Some("OPEN") => ContentState::Open,
                Some("CLOSED") => ContentState::Closed,
                Some("MERGED") => ContentState::Merged,
                _ => ContentState::Unknown,
            },
            state_reason: content.state_reason,
            number: content.number,
            title: content.title,
            url: content.url,
        });
        let fields = data
            .field_values
            .nodes
            .into_iter()
            .flatten()
            .filter_map(field_value)
            .collect();
        Self {
            id: data.id,
            content,
            fields,
        }
    }
}

#[cfg(feature = "tui")]
#[derive(Default, Deserialize)]
struct MutationResponse {
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;

    #[cfg(feature = "tui")]
    use super::FieldValue;
    use super::{DynError, Project, ProjectRef, ProjectSource};

    #[cfg(feature = "tui")]
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct UpdateRequest {
        pub(crate) project_id: String,
        pub(crate) item_id: String,
        pub(crate) field_id: String,
        pub(crate) value: FieldValue,
        pub(crate) token: String,
    }

    #[cfg(feature = "tui")]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct RemoveRequest {
        pub(crate) project_id: String,
        pub(crate) item_id: String,
        pub(crate) token: String,
    }

    pub(crate) struct MockProjectSource {
        result: Result<Project, String>,
        requests: RefCell<Vec<(ProjectRef, String)>>,
        #[cfg(feature = "tui")]
        updates: RefCell<Vec<UpdateRequest>>,
        #[cfg(feature = "tui")]
        removals: RefCell<Vec<RemoveRequest>>,
    }

    impl MockProjectSource {
        pub(crate) fn returning(project: Project) -> Self {
            Self {
                result: Ok(project),
                requests: RefCell::default(),
                #[cfg(feature = "tui")]
                updates: RefCell::default(),
                #[cfg(feature = "tui")]
                removals: RefCell::default(),
            }
        }

        pub(crate) fn failing(message: impl Into<String>) -> Self {
            Self {
                result: Err(message.into()),
                requests: RefCell::default(),
                #[cfg(feature = "tui")]
                updates: RefCell::default(),
                #[cfg(feature = "tui")]
                removals: RefCell::default(),
            }
        }

        pub(crate) fn requests(&self) -> Vec<(ProjectRef, String)> {
            self.requests.borrow().clone()
        }

        #[cfg(feature = "tui")]
        pub(crate) fn updates(&self) -> Vec<UpdateRequest> {
            self.updates.borrow().clone()
        }

        #[cfg(feature = "tui")]
        pub(crate) fn removals(&self) -> Vec<RemoveRequest> {
            self.removals.borrow().clone()
        }
    }

    impl ProjectSource for MockProjectSource {
        fn fetch_project(
            &self,
            project_ref: &ProjectRef,
            token: &str,
        ) -> Result<Project, DynError> {
            self.requests
                .borrow_mut()
                .push((project_ref.clone(), token.to_string()));
            self.result.clone().map_err(Into::into)
        }

        #[cfg(feature = "tui")]
        fn update_field(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
            value: FieldValue,
            token: &str,
        ) -> Result<(), DynError> {
            self.updates.borrow_mut().push(UpdateRequest {
                project_id: project_id.into(),
                item_id: item_id.into(),
                field_id: field_id.into(),
                value,
                token: token.into(),
            });
            Ok(())
        }

        #[cfg(feature = "tui")]
        fn remove_item(
            &self,
            project_id: &str,
            item_id: &str,
            token: &str,
        ) -> Result<(), DynError> {
            self.removals.borrow_mut().push(RemoveRequest {
                project_id: project_id.into(),
                item_id: item_id.into(),
                token: token.into(),
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use httpmock::prelude::*;

    use super::*;

    #[derive(Default)]
    struct FakeTransport {
        responses: RefCell<VecDeque<GraphQlResponse>>,
        requests: RefCell<Vec<(String, serde_json::Value)>>,
    }

    impl FakeTransport {
        fn with_responses(responses: impl IntoIterator<Item = GraphQlResponse>) -> Self {
            Self {
                responses: RefCell::new(responses.into_iter().collect()),
                requests: RefCell::default(),
            }
        }
    }

    impl GraphQlTransport for FakeTransport {
        fn execute(
            &self,
            token: &str,
            payload: &serde_json::Value,
        ) -> Result<GraphQlResponse, DynError> {
            self.requests
                .borrow_mut()
                .push((token.to_string(), payload.clone()));
            self.responses
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "unexpected GraphQL request".into())
        }
    }

    fn project_page(number: u32, has_next_page: bool, cursor: Option<&str>) -> GraphQlResponse {
        GraphQlResponse {
            status: 200,
            rate_limit: None,
            body: serde_json::json!({
                "data": { "organization": { "project": {
                    "title": "Project",
                    "items": {
                        "pageInfo": { "hasNextPage": has_next_page, "endCursor": cursor },
                        "nodes": [{
                            "fieldValues": { "nodes": [] },
                            "content": {
                                "__typename": "Issue",
                                "number": number,
                                "title": format!("Issue {number}"),
                                "url": format!("https://github.com/o/r/issues/{number}")
                            }
                        }]
                    }
                } } }
            })
            .to_string(),
        }
    }

    fn organization_project_ref() -> ProjectRef {
        parse_project_url("https://github.com/orgs/example/projects/1").unwrap()
    }

    #[test]
    fn parses_organization_and_user_project_urls() {
        assert_eq!(
            parse_project_url("https://github.com/orgs/example/projects/1").unwrap(),
            ProjectRef {
                owner: "example".into(),
                number: 1,
                owner_kind: OwnerKind::Organization,
            }
        );
        assert_eq!(
            parse_project_url("https://github.com/users/example/projects/2").unwrap(),
            ProjectRef {
                owner: "example".into(),
                number: 2,
                owner_kind: OwnerKind::User,
            }
        );
    }

    #[test]
    fn rejects_unsupported_project_urls() {
        assert!(parse_project_url("https://gitlab.com/orgs/x/projects/1").is_err());
        assert!(parse_project_url("https://github.com/owner/repo/projects/1").is_err());
        assert!(parse_project_url("https://github.com/orgs/x/projects/nope").is_err());
    }

    #[test]
    fn derives_all_emoji_statuses() {
        let item = |kind, state, state_reason: Option<&str>, fields: &[(&str, &str)]| Item {
            id: "item".into(),
            content: Some(Content {
                kind,
                state,
                state_reason: state_reason.map(str::to_string),
                number: None,
                title: None,
                url: None,
            }),
            fields: fields
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect(),
        };

        assert_eq!(
            emoji_status(&item(Kind::PullRequest, ContentState::Open, None, &[])),
            "🟢"
        );
        assert_eq!(
            emoji_status(&item(Kind::PullRequest, ContentState::Closed, None, &[])),
            "🛑"
        );
        assert_eq!(
            emoji_status(&item(Kind::PullRequest, ContentState::Merged, None, &[])),
            "🏁"
        );
        assert_eq!(
            emoji_status(&item(Kind::Issue, ContentState::Open, None, &[])),
            "⚠️"
        );
        assert_eq!(
            emoji_status(&item(
                Kind::Issue,
                ContentState::Closed,
                Some("COMPLETED"),
                &[]
            )),
            "✅"
        );
        assert_eq!(
            emoji_status(&item(
                Kind::Issue,
                ContentState::Open,
                None,
                &[("Status", "In Progress")],
            )),
            "🏃"
        );
        assert_eq!(
            emoji_status(&item(
                Kind::Issue,
                ContentState::Closed,
                Some("NOT_PLANNED"),
                &[("Labels", "bug, duplicate")],
            )),
            "❓"
        );
        assert_eq!(
            emoji_status(&item(
                Kind::Issue,
                ContentState::Closed,
                Some("NOT_PLANNED"),
                &[],
            )),
            "❌"
        );
    }

    #[test]
    fn decodes_project_field_values() {
        let response = serde_json::json!({
            "data": { "organization": { "project": {
                "title": "My Project",
                "id": "project-id",
                "fields": { "nodes": [
                    { "__typename": "ProjectV2Field", "id": "title", "name": "Title", "dataType": "TITLE" },
                    { "__typename": "ProjectV2SingleSelectField", "id": "status", "name": "Status", "options": [
                        { "id": "todo", "name": "Todo" }, { "id": "done", "name": "Done" }
                    ] },
                    { "__typename": "ProjectV2Field", "id": "estimate", "name": "Estimate", "dataType": "NUMBER" },
                    { "__typename": "ProjectV2Field", "id": "notes", "name": "Release notes", "dataType": "TEXT" },
                    { "__typename": "ProjectV2Field", "id": "repository", "name": "Repository", "dataType": "REPOSITORY" },
                    { "__typename": "ProjectV2Field", "id": "closed", "name": "Closed", "dataType": "CLOSED" },
                    { "__typename": "ProjectV2Field", "id": "labels", "name": "Labels", "dataType": "LABELS" },
                    { "__typename": "ProjectV2Field", "id": "assignees", "name": "Assignees", "dataType": "ASSIGNEES" },
                    { "__typename": "ProjectV2Field", "id": "reviewers", "name": "Reviewers", "dataType": "REVIEWERS" },
                    { "__typename": "ProjectV2Field", "id": "prs", "name": "Linked pull requests", "dataType": "LINKED_PULL_REQUESTS" },
                    { "__typename": "ProjectV2Field", "id": "milestone", "name": "Milestone", "dataType": "MILESTONE" },
                    { "__typename": "ProjectV2Field", "id": "components", "name": "Components", "dataType": "MULTI_SELECT" },
                    { "__typename": "ProjectV2IterationField", "id": "sprint", "name": "Sprint", "configuration": {
                        "iterations": [{ "id": "sprint-1", "title": "Sprint 1" }],
                        "completedIterations": [{ "id": "sprint-0", "title": "Sprint 0" }]
                    } }
                ] },
                "items": {
                    "pageInfo": { "hasNextPage": false, "endCursor": null },
                    "nodes": [{
                        "id": "item-id",
                        "fieldValues": { "nodes": [
                            { "__typename": "ProjectV2ItemFieldTextValue", "field": { "name": "Title" }, "text": "Fix" },
                            { "__typename": "ProjectV2ItemFieldSingleSelectValue", "field": { "name": "Status" }, "name": "Done" },
                            { "__typename": "ProjectV2ItemFieldNumberValue", "field": { "name": "Estimate" }, "number": 3.0 },
                            { "__typename": "ProjectV2ItemFieldRepositoryValue", "field": { "name": "Repository" }, "repository": { "nameWithOwner": "octo/repo" } },
                            { "__typename": "ProjectV2ItemIssueFieldValue", "field": { "name": "Closed" }, "issueFieldValue": { "__typename": "IssueFieldSingleSelectValue", "value": "true" } },
                            { "__typename": "ProjectV2ItemFieldLabelValue", "field": { "name": "Labels" }, "labels": { "nodes": [{ "name": "bug" }, null, { "name": "urgent" }] } },
                            { "__typename": "ProjectV2ItemFieldUserValue", "field": { "name": "Assignees" }, "users": { "nodes": [{ "login": "octocat" }, null, { "login": "hubot" }] } },
                            { "__typename": "ProjectV2ItemFieldReviewerValue", "field": { "name": "Reviewers" }, "reviewers": { "nodes": [{ "displayName": "reviewer" }, null, { "displayName": "core-team" }] } },
                            { "__typename": "ProjectV2ItemFieldPullRequestValue", "field": { "name": "Linked pull requests" }, "pullRequests": { "nodes": [null, { "number": 7, "repository": { "nameWithOwner": "octo/repo" } }] } },
                            { "__typename": "ProjectV2ItemFieldMilestoneValue", "field": { "name": "Milestone" }, "milestone": { "title": "v1.0" } },
                            { "__typename": "ProjectV2ItemFieldMultiSelectValue", "field": { "name": "Components" }, "options": [{ "name": "API" }, { "name": "TUI" }] }
                        ] },
                        "content": { "__typename": "Issue", "state": "OPEN", "stateReason": "REOPENED", "number": 42, "title": "Fix", "url": "https://github.com/o/r/issues/42" }
                    }]
                }
            } } }
        });

        let page = decode_project(&response.to_string(), None).unwrap();

        assert_eq!(page.title, "My Project");
        assert_eq!(
            page.field_names,
            [
                "Status",
                "Estimate",
                "Release notes",
                "Repository",
                "Closed",
                "Labels",
                "Assignees",
                "Reviewers",
                "Linked pull requests",
                "Milestone",
                "Components",
                "Sprint"
            ]
        );
        assert_eq!(page.items[0].id, "item-id");
        assert_eq!(
            page.items[0].content.as_ref().unwrap().state,
            ContentState::Open
        );
        assert_eq!(
            page.items[0]
                .content
                .as_ref()
                .unwrap()
                .state_reason
                .as_deref(),
            Some("REOPENED")
        );
        assert_eq!(
            page.editable_fields[0].kind,
            EditableFieldKind::SingleSelect
        );
        assert_eq!(page.editable_fields[0].options[1].name, "Done");
        assert_eq!(page.editable_fields[3].kind, EditableFieldKind::Iteration);
        assert_eq!(page.editable_fields[3].options[0].id, "sprint-1");
        assert_eq!(
            page.items[0].fields,
            [
                ("Status".into(), "Done".into()),
                ("Estimate".into(), "3".into()),
                ("Repository".into(), "octo/repo".into()),
                ("Closed".into(), "true".into()),
                ("Labels".into(), "bug, urgent".into()),
                ("Assignees".into(), "octocat, hubot".into()),
                ("Reviewers".into(), "reviewer, core-team".into()),
                ("Linked pull requests".into(), "octo/repo#7".into()),
                ("Milestone".into(), "v1.0".into()),
                ("Components".into(), "API, TUI".into())
            ]
        );
    }

    #[test]
    fn tolerates_oauth_restrictions_on_item_field_values() {
        let response = serde_json::json!({
            "data": { "user": { "project": {
                "id": "project-id",
                "title": "LLVM",
                "fields": { "nodes": [] },
                "items": {
                    "pageInfo": { "hasNextPage": false, "endCursor": null },
                    "nodes": [{
                        "id": "item-id",
                        "fieldValues": { "nodes": [null] },
                        "content": {
                            "__typename": "Issue",
                            "number": 1,
                            "title": "Visible issue",
                            "url": "https://github.com/llvm/llvm-project/issues/1"
                        }
                    }]
                }
            } } },
            "errors": [{
                "message": "Although you appear to have the correct authorization credentials, the `llvm` organization has enabled OAuth App access restrictions",
                "path": ["user", "project", "items", "nodes", 0, "fieldValues", "nodes", 0, "reviewers"]
            }]
        });

        let page = decode_project(&response.to_string(), None).unwrap();

        assert_eq!(page.title, "LLVM");
        assert_eq!(
            page.items[0].content.as_ref().unwrap().title.as_deref(),
            Some("Visible issue")
        );
        assert!(page.items[0].fields.is_empty());
    }

    #[test]
    fn reports_project_level_and_unrelated_graphql_errors() {
        let restricted_project = serde_json::json!({
            "data": { "user": null },
            "errors": [{
                "message": "The `llvm` organization has enabled OAuth App access restrictions",
                "path": ["user", "project"]
            }]
        });
        let unrelated_field_error = serde_json::json!({
            "data": { "user": null },
            "errors": [{
                "message": "Unexpected resolver failure",
                "path": ["user", "project", "items", "nodes", 0, "fieldValues"]
            }]
        });

        assert!(decode_project(&restricted_project.to_string(), None)
            .err()
            .expect("project-level restriction should fail")
            .to_string()
            .contains("OAuth App access restrictions"));
        assert!(decode_project(&unrelated_field_error.to_string(), None)
            .err()
            .expect("unrelated GraphQL error should fail")
            .to_string()
            .contains("Unexpected resolver failure"));
    }

    #[test]
    fn fetches_all_pages_and_records_requests() {
        let source = GitHubProjectSource {
            transport: FakeTransport::with_responses([
                project_page(1, true, Some("next")),
                project_page(2, false, None),
            ]),
        };

        let project = source
            .fetch_project(&organization_project_ref(), "secret")
            .unwrap();

        assert_eq!(project.items.len(), 2);
        let requests = source.transport.requests.borrow();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, "secret");
        assert_eq!(requests[0].1["variables"]["owner"], "example");
        assert_eq!(requests[1].1["variables"]["cursor"], "next");
        let query = requests[0].1["query"].as_str().unwrap();
        assert!(query.contains("organization(login: $owner)"));
        assert!(!query.contains("resource(url:"));
    }

    #[test]
    fn updates_project_field_with_typed_mutation_value() {
        let source = GitHubProjectSource {
            transport: FakeTransport::with_responses([GraphQlResponse {
                status: 200,
                rate_limit: None,
                body: r#"{ "data": { "updateProjectV2ItemFieldValue": { "projectV2Item": { "id": "item" } } } }"#.into(),
            }]),
        };

        source
            .update_field(
                "project",
                "item",
                "status",
                FieldValue::SingleSelect("done".into()),
                "secret",
            )
            .unwrap();

        let requests = source.transport.requests.borrow();
        assert_eq!(requests[0].0, "secret");
        assert_eq!(requests[0].1["variables"]["projectId"], "project");
        assert_eq!(requests[0].1["variables"]["itemId"], "item");
        assert_eq!(
            requests[0].1["variables"]["value"],
            serde_json::json!({ "singleSelectOptionId": "done" })
        );
        assert!(requests[0].1["query"]
            .as_str()
            .unwrap()
            .contains("updateProjectV2ItemFieldValue"));
    }

    #[test]
    fn removes_item_from_project() {
        let source = GitHubProjectSource {
            transport: FakeTransport::with_responses([GraphQlResponse {
                status: 200,
                rate_limit: None,
                body: r#"{ "data": { "deleteProjectV2Item": { "deletedItemId": "item" } } }"#
                    .into(),
            }]),
        };

        source.remove_item("project", "item", "secret").unwrap();

        let requests = source.transport.requests.borrow();
        assert_eq!(requests[0].0, "secret");
        assert_eq!(requests[0].1["variables"]["projectId"], "project");
        assert_eq!(requests[0].1["variables"]["itemId"], "item");
        assert!(requests[0].1["query"]
            .as_str()
            .unwrap()
            .contains("deleteProjectV2Item"));
    }

    #[test]
    fn reports_http_and_pagination_errors() {
        let http_error_source = GitHubProjectSource {
            transport: FakeTransport::with_responses([GraphQlResponse {
                status: 403,
                rate_limit: Some(RateLimit {
                    limit: Some(5000),
                    remaining: Some(0),
                    reset: Some(1770000000),
                }),
                body: "rate limit exceeded".into(),
            }]),
        };
        let error = http_error_source
            .fetch_project(&organization_project_ref(), "invalid")
            .unwrap_err()
            .to_string();
        assert!(error.contains("HTTP 403: rate limit exceeded"));
        assert!(error.contains("0/5000 remaining"));
        assert!(error.contains("1770000000"));

        let pagination_source = GitHubProjectSource {
            transport: FakeTransport::with_responses([project_page(1, true, None)]),
        };
        assert!(pagination_source
            .fetch_project(&organization_project_ref(), "token")
            .unwrap_err()
            .to_string()
            .contains("omitted the cursor"));
    }

    #[test]
    fn reqwest_transport_sends_authenticated_json_request() {
        let server = MockServer::start();
        let payload = serde_json::json!({ "query": "query Test { viewer { login } }" });
        let request = server.mock(|when, then| {
            when.method(POST)
                .path("/graphql")
                .header("authorization", "Bearer secret")
                .json_body(payload.clone());
            then.status(200)
                .header("x-ratelimit-limit", "5000")
                .header("x-ratelimit-remaining", "42")
                .header("x-ratelimit-reset", "1770000000")
                .body("response body");
        });
        let transport = ReqwestGraphQlTransport::with_endpoint(server.url("/graphql")).unwrap();

        let response = transport.execute("secret", &payload).unwrap();

        request.assert();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "response body");
        assert_eq!(
            response.rate_limit.unwrap().display(),
            "rate limit 42/5000 remaining; resets at Unix timestamp 1770000000"
        );
    }

    #[test]
    fn graphql_errors_include_rate_limit_context() {
        let response = serde_json::json!({
            "errors": [{ "message": "API rate limit exceeded" }]
        });
        let rate_limit = RateLimit {
            limit: Some(5000),
            remaining: Some(0),
            reset: Some(1770000000),
        };

        let error = decode_project(&response.to_string(), Some(&rate_limit))
            .err()
            .expect("rate limit error should fail")
            .to_string();

        assert!(error.contains("API rate limit exceeded"));
        assert!(error.contains("0/5000 remaining"));
        assert!(error.contains("1770000000"));
    }
}
