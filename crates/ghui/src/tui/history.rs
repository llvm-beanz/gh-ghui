use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::github::DynError;

const MAX_ENTRIES: usize = 1000;

pub(crate) fn record(entries: &mut Vec<String>, command: String) -> bool {
    if command.is_empty() || entries.last() == Some(&command) {
        return false;
    }
    entries.push(command);
    if entries.len() > MAX_ENTRIES {
        entries.drain(..entries.len() - MAX_ENTRIES);
    }
    true
}

pub(crate) fn default_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("GHUI_HISTORY_FILE") {
        return Some(PathBuf::from(path));
    }

    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA").map(|path| PathBuf::from(path).join("ghui/history.json"))
    }
    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(|path| PathBuf::from(path).join("Library/Application Support/ghui/history.json"))
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/state")))
            .map(|path| path.join("ghui/history.json"))
    }
}

pub(crate) fn load(path: &Path) -> Result<Vec<String>, DynError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(crate) fn save(path: &Path, entries: &[String]) -> Result<(), DynError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let entries = &entries[entries.len().saturating_sub(MAX_ENTRIES)..];
    fs::write(path, serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "ghui-history-{}-{unique}/history.json",
            std::process::id()
        ))
    }

    #[test]
    fn missing_history_is_empty() {
        assert!(load(&temp_path()).unwrap().is_empty());
    }

    #[test]
    fn history_round_trips_and_creates_parent_directory() {
        let path = temp_path();
        let entries = vec!["columns".into(), "refresh".into()];

        save(&path, &entries).unwrap();

        assert_eq!(load(&path).unwrap(), entries);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn save_limits_persisted_history() {
        let path = temp_path();
        let entries = (0..MAX_ENTRIES + 2)
            .map(|index| format!("command-{index}"))
            .collect::<Vec<_>>();

        save(&path, &entries).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded.len(), MAX_ENTRIES);
        assert_eq!(loaded[0], "command-2");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn record_ignores_consecutive_duplicates() {
        let mut entries = vec!["refresh".into()];

        assert!(!record(&mut entries, "refresh".into()));
        assert!(record(&mut entries, "columns".into()));

        assert_eq!(entries, ["refresh", "columns"]);
    }
}
