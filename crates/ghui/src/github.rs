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
  items(first: 100, after: $cursor) {
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
        }
      }
      content {
        __typename
        ... on Issue { number title url }
        ... on PullRequest { number title url }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Content {
    pub(crate) kind: Kind,
    pub(crate) number: Option<u32>,
    pub(crate) title: Option<String>,
    pub(crate) url: Option<String>,
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
                return Err(format!(
                    "GitHub GraphQL request failed with HTTP {}: {}",
                    response.status,
                    response_snippet(&response.body)
                )
                .into());
            }

            let page = decode_project(&response.body)?;
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
            return Err(format!(
                "GitHub GraphQL request failed with HTTP {}: {}",
                response.status,
                response_snippet(&response.body)
            )
            .into());
        }
        let response: MutationResponse = serde_json::from_str(&response.body)?;
        if response.errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "GitHub API error: {}",
                response
                    .errors
                    .iter()
                    .map(|error| error.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
            .into())
        }
    }
}

struct GraphQlResponse {
    status: u16,
    body: String,
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
        let body = response.text()?;
        Ok(GraphQlResponse { status, body })
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

fn decode_project(body: &str) -> Result<DecodedPage, DynError> {
    let response: PageResponse = serde_json::from_str(body)?;
    if !response.errors.is_empty() {
        let messages = response
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("GitHub API error: {messages}").into());
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
    nodes: Vec<FieldValueData>,
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
    #[serde(other)]
    Unsupported,
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
            number: content.number,
            title: content.title,
            url: content.url,
        });
        let fields = data
            .field_values
            .nodes
            .into_iter()
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

    pub(crate) struct MockProjectSource {
        result: Result<Project, String>,
        requests: RefCell<Vec<(ProjectRef, String)>>,
        #[cfg(feature = "tui")]
        updates: RefCell<Vec<UpdateRequest>>,
    }

    impl MockProjectSource {
        pub(crate) fn returning(project: Project) -> Self {
            Self {
                result: Ok(project),
                requests: RefCell::default(),
                #[cfg(feature = "tui")]
                updates: RefCell::default(),
            }
        }

        pub(crate) fn failing(message: impl Into<String>) -> Self {
            Self {
                result: Err(message.into()),
                requests: RefCell::default(),
                #[cfg(feature = "tui")]
                updates: RefCell::default(),
            }
        }

        pub(crate) fn requests(&self) -> Vec<(ProjectRef, String)> {
            self.requests.borrow().clone()
        }

        #[cfg(feature = "tui")]
        pub(crate) fn updates(&self) -> Vec<UpdateRequest> {
            self.updates.borrow().clone()
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
                            { "__typename": "ProjectV2ItemFieldNumberValue", "field": { "name": "Estimate" }, "number": 3.0 }
                        ] },
                        "content": { "__typename": "Issue", "number": 42, "title": "Fix", "url": "https://github.com/o/r/issues/42" }
                    }]
                }
            } } }
        });

        let page = decode_project(&response.to_string()).unwrap();

        assert_eq!(page.title, "My Project");
        assert_eq!(
            page.field_names,
            ["Status", "Estimate", "Release notes", "Sprint"]
        );
        assert_eq!(page.items[0].id, "item-id");
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
                ("Estimate".into(), "3".into())
            ]
        );
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
    fn reports_http_and_pagination_errors() {
        let http_error_source = GitHubProjectSource {
            transport: FakeTransport::with_responses([GraphQlResponse {
                status: 401,
                body: "bad credentials".into(),
            }]),
        };
        assert!(http_error_source
            .fetch_project(&organization_project_ref(), "invalid")
            .unwrap_err()
            .to_string()
            .contains("HTTP 401: bad credentials"));

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
            then.status(200).body("response body");
        });
        let transport = ReqwestGraphQlTransport::with_endpoint(server.url("/graphql")).unwrap();

        let response = transport.execute("secret", &payload).unwrap();

        request.assert();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "response body");
    }
}
