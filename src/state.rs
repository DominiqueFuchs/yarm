use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Bump this when the state format or scan logic changes in a way that
/// invalidates previously persisted data. Old state files with a
/// different version are silently discarded.
const STATE_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
struct StateEnvelope {
    version: u32,
    state: State,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub repositories: Vec<PathBuf>,
    #[serde(default)]
    pub last_scan: Option<u64>,
}

impl State {
    /// Sets the last scan timestamp to now.
    pub fn mark_scanned(&mut self) {
        self.last_scan = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
    }

    /// Returns the last scan time as a `SystemTime`, if available.
    pub fn last_scan_time(&self) -> Option<SystemTime> {
        self.last_scan
            .map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
    }
}

/// Loads the yarm state from `~/.local/share/yarm/state.bin`.
/// Returns a default state if the file does not exist, cannot be decoded,
/// or was written by a different state version.
pub fn load() -> Result<State> {
    let Some(path) = state_path() else {
        return Ok(State::default());
    };

    if !path.exists() {
        return Ok(State::default());
    }

    let bytes = fs::read(&path).context("Failed to read yarm state file")?;
    match bitcode::deserialize::<StateEnvelope>(&bytes) {
        Ok(envelope) if envelope.version == STATE_VERSION => Ok(envelope.state),
        _ => {
            let _ = fs::remove_file(&path);
            Ok(State::default())
        }
    }
}

/// Saves the yarm state to `~/.local/share/yarm/state.bin`.
pub fn save(state: &State) -> Result<()> {
    let Some(path) = state_path() else {
        anyhow::bail!("Could not determine data directory");
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create yarm data directory")?;
    }

    let envelope = StateEnvelope {
        version: STATE_VERSION,
        state: State {
            repositories: state.repositories.clone(),
            last_scan: state.last_scan,
        },
    };
    let bytes = bitcode::serialize(&envelope).context("Failed to encode yarm state")?;
    fs::write(&path, bytes).context("Failed to write yarm state file")
}

/// Returns the path to the yarm state file.
fn state_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("yarm/state.bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_roundtrip() {
        let state = State {
            repositories: vec![
                PathBuf::from("/home/user/projects/repo-a"),
                PathBuf::from("/home/user/work/repo-b"),
            ],
            ..State::default()
        };

        let envelope = StateEnvelope {
            version: STATE_VERSION,
            state,
        };

        let bytes = bitcode::serialize(&envelope).unwrap();
        let decoded: StateEnvelope = bitcode::deserialize(&bytes).unwrap();

        assert_eq!(decoded.version, STATE_VERSION);
        assert_eq!(decoded.state.repositories.len(), 2);
        assert_eq!(decoded.state.repositories[0], PathBuf::from("/home/user/projects/repo-a"));
        assert_eq!(decoded.state.repositories[1], PathBuf::from("/home/user/work/repo-b"));
    }

    #[test]
    fn test_empty_state_roundtrip() {
        let envelope = StateEnvelope {
            version: STATE_VERSION,
            state: State::default(),
        };

        let bytes = bitcode::serialize(&envelope).unwrap();
        let decoded: StateEnvelope = bitcode::deserialize(&bytes).unwrap();

        assert_eq!(decoded.version, STATE_VERSION);
        assert!(decoded.state.repositories.is_empty());
    }

    #[test]
    fn test_old_version_rejected() {
        let envelope = StateEnvelope {
            version: STATE_VERSION - 1,
            state: State {
                repositories: vec![PathBuf::from("/some/repo")],
                ..State::default()
            },
        };

        let bytes = bitcode::serialize(&envelope).unwrap();
        let decoded: StateEnvelope = bitcode::deserialize(&bytes).unwrap();

        assert_ne!(decoded.version, STATE_VERSION);
    }
}
