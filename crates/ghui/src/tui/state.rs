use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::github::DynError;

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewState {
    version: u32,
    pub project_url: Option<String>,
    pub selected: Option<usize>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            project_url: None,
            selected: None,
        }
    }
}

#[allow(dead_code)]
impl ViewState {
    pub fn new(project_url: Option<String>, selected: Option<usize>) -> Self {
        Self {
            version: CURRENT_VERSION,
            project_url,
            selected,
        }
    }

    pub fn to_text(&self) -> Result<String, DynError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_text(text: &str) -> Result<Self, DynError> {
        let state: Self = serde_json::from_str(text)?;
        if state.version != CURRENT_VERSION {
            return Err(format!(
                "unsupported view state version {} (expected {CURRENT_VERSION})",
                state.version
            )
            .into());
        }
        Ok(state)
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

    fn sample_state() -> ViewState {
        ViewState::new(
            Some("https://github.com/orgs/example/projects/1".into()),
            Some(12),
        )
    }

    #[test]
    fn state_round_trips_as_readable_text() {
        let state = sample_state();

        let text = state.to_text().unwrap();

        assert!(text.contains("\"project_url\""));
        assert!(text.contains("https://github.com/orgs/example/projects/1"));
        assert_eq!(ViewState::from_text(&text).unwrap(), state);
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
        let loaded = ViewState::load(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(loaded, state);
    }

    #[test]
    fn rejects_unknown_state_versions() {
        let error =
            ViewState::from_text(r#"{ "version": 99, "project_url": null, "selected": null }"#)
                .unwrap_err();

        assert!(error
            .to_string()
            .contains("unsupported view state version 99"));
    }
}
