use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::github::DynError;

const CURRENT_VERSION: u32 = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewState {
    pub project_url: Option<String>,
    pub selected: Option<usize>,
    #[serde(default)]
    pub columns: Option<Vec<String>>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
}

#[allow(dead_code)]
impl ViewState {
    pub fn new(project_url: Option<String>, selected: Option<usize>) -> Self {
        Self {
            project_url,
            selected,
            columns: None,
            filter: None,
            sort: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    version: u32,
    pub tabs: Vec<ViewState>,
    pub active_tab: usize,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            tabs: vec![ViewState::default()],
            active_tab: 0,
        }
    }
}

#[derive(Deserialize)]
struct LegacyViewState {
    project_url: Option<String>,
    selected: Option<usize>,
    #[serde(default)]
    columns: Option<Vec<String>>,
}

#[allow(dead_code)]
impl SessionState {
    pub fn new(tabs: Vec<ViewState>, active_tab: usize) -> Self {
        let tabs = if tabs.is_empty() {
            vec![ViewState::default()]
        } else {
            tabs
        };
        Self {
            version: CURRENT_VERSION,
            active_tab: active_tab.min(tabs.len() - 1),
            tabs,
        }
    }

    pub fn to_text(&self) -> Result<String, DynError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_text(text: &str) -> Result<Self, DynError> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(1) => {
                let legacy: LegacyViewState = serde_json::from_value(value)?;
                Ok(Self {
                    version: CURRENT_VERSION,
                    tabs: vec![ViewState {
                        project_url: legacy.project_url,
                        selected: legacy.selected,
                        columns: legacy.columns,
                        filter: None,
                        sort: None,
                    }],
                    active_tab: 0,
                })
            }
            Some(version) if version == 2 || version == u64::from(CURRENT_VERSION) => {
                let mut state: Self = serde_json::from_value(value)?;
                state.version = CURRENT_VERSION;
                if state.tabs.is_empty() {
                    state.tabs.push(ViewState::default());
                }
                state.active_tab = state.active_tab.min(state.tabs.len() - 1);
                Ok(state)
            }
            Some(version) => Err(format!(
                "unsupported session state version {version} (expected {CURRENT_VERSION})"
            )
            .into()),
            None => Err("session state is missing its version".into()),
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), DynError> {
        fs::write(path, self.to_text()?)?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, DynError> {
        Self::from_text(&fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn sample_state() -> SessionState {
        SessionState::new(
            vec![ViewState::new(
                Some("https://github.com/orgs/example/projects/1".into()),
                Some(12),
            )],
            0,
        )
    }

    #[test]
    fn state_round_trips_as_readable_text() {
        let mut state = sample_state();
        state.tabs[0].columns = Some(vec!["Title".into(), "Status".into()]);
        state.tabs[0].filter = Some("is:issue status:todo".into());
        state.tabs[0].sort = Some("Priority:desc".into());

        let text = state.to_text().unwrap();

        assert!(text.contains("\"project_url\""));
        assert!(text.contains("https://github.com/orgs/example/projects/1"));
        assert!(text.contains("\"columns\""));
        assert!(text.contains("\"filter\""));
        assert!(text.contains("\"sort\""));
        assert_eq!(SessionState::from_text(&text).unwrap(), state);
    }

    #[test]
    fn state_without_columns_enables_all_columns() {
        let state =
            SessionState::from_text(r#"{ "version": 1, "project_url": null, "selected": null }"#)
                .unwrap();

        assert_eq!(state.tabs[0].columns, None);
        assert_eq!(state.tabs[0].filter, None);
        assert_eq!(state.tabs[0].sort, None);
    }

    #[test]
    fn version_two_state_migrates_without_filter_or_sort() {
        let state = SessionState::from_text(
            r#"{ "version": 2, "tabs": [{ "project_url": null, "selected": 1, "columns": null }], "active_tab": 0 }"#,
        )
        .unwrap();

        assert_eq!(state.tabs[0].selected, Some(1));
        assert_eq!(state.tabs[0].filter, None);
        assert_eq!(state.tabs[0].sort, None);
        assert!(state.to_text().unwrap().contains("\"version\": 3"));
    }

    #[test]
    fn state_round_trips_on_disk() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ghui-view-state-{unique}.json"));
        let state = sample_state();

        state.save(&path).unwrap();
        let loaded = SessionState::load(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(loaded, state);
    }

    #[test]
    fn rejects_unknown_state_versions() {
        let error =
            SessionState::from_text(r#"{ "version": 99, "project_url": null, "selected": null }"#)
                .unwrap_err();

        assert!(error
            .to_string()
            .contains("unsupported session state version 99"));
    }

    #[test]
    fn state_round_trips_multiple_tabs_and_active_tab() {
        let state = SessionState::new(
            vec![
                ViewState::new(Some("https://github.com/orgs/a/projects/1".into()), Some(2)),
                ViewState::new(Some("https://github.com/orgs/b/projects/2".into()), Some(5)),
            ],
            1,
        );

        assert_eq!(
            SessionState::from_text(&state.to_text().unwrap()).unwrap(),
            state
        );
    }
}
