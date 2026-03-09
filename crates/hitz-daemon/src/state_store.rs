//! Persistent VM state store — per-VM directories with JSON files.

use std::path::PathBuf;

use hitz_api::{VmConfig, VmState};
use serde::{Deserialize, Serialize};

use crate::error::DaemonError;

#[derive(Serialize, Deserialize)]
struct PersistedState {
    state: VmState,
    created_at: u64,
    updated_at: u64,
}

/// Persists VM configuration and state to per-VM directories.
///
/// Layout: `<base_dir>/<vm-id>/config.json` and `<base_dir>/<vm-id>/state.json`.
pub struct StateStore {
    base_dir: PathBuf,
}

impl StateStore {
    /// Create a new store, creating `base_dir` if it does not exist.
    pub fn new(base_dir: PathBuf) -> Result<Self, DaemonError> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Persist VM config to `<base_dir>/<id>/config.json`.
    pub fn save_config(&self, id: &str, config: &VmConfig) -> Result<(), DaemonError> {
        let dir = self.vm_dir(id);
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        std::fs::write(dir.join("config.json"), json)?;
        Ok(())
    }

    /// Persist VM state to `<base_dir>/<id>/state.json`.
    ///
    /// Preserves `created_at` from any existing `state.json`.
    pub fn save_state(&self, id: &str, state: VmState) -> Result<(), DaemonError> {
        let dir = self.vm_dir(id);
        std::fs::create_dir_all(&dir)?;
        let now = now_ms();
        let created_at = self.load_raw_state(id).map_or(now, |s| s.created_at);
        let persisted = PersistedState {
            state,
            created_at,
            updated_at: now,
        };
        let json = serde_json::to_string_pretty(&persisted)
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        std::fs::write(dir.join("state.json"), json)?;
        Ok(())
    }

    /// Delete the VM directory for `id`. No-op if directory does not exist.
    pub fn delete(&self, id: &str) -> Result<(), DaemonError> {
        let dir = self.vm_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Load all valid VMs from the state directory.
    ///
    /// Rules:
    /// - Missing or corrupt `config.json` → skip (log warning).
    /// - Missing `state.json` → treat as `Created`.
    /// - `state = Running` → clamped to `Stopped` (daemon restart recovery).
    pub fn load_all(&self) -> Result<Vec<(String, VmConfig, VmState)>, DaemonError> {
        let mut results = Vec::new();
        let read_dir = match std::fs::read_dir(&self.base_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(results),
            Err(e) => return Err(DaemonError::Io(e)),
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(id) = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
            else {
                continue;
            };

            let Ok(config_json) = std::fs::read_to_string(path.join("config.json")) else {
                tracing::warn!(vm_id = %id, "missing config.json — skipping");
                continue;
            };
            let config: VmConfig = match serde_json::from_str(&config_json) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(vm_id = %id, error = %e, "corrupt config.json — skipping");
                    continue;
                }
            };

            let state = self.load_raw_state(&id).map_or(VmState::Created, |s| {
                if s.state == VmState::Running {
                    VmState::Stopped
                } else {
                    s.state
                }
            });

            results.push((id, config, state));
        }

        Ok(results)
    }

    fn vm_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    fn load_raw_state(&self, id: &str) -> Option<PersistedState> {
        let json = std::fs::read_to_string(self.vm_dir(id).join("state.json")).ok()?;
        serde_json::from_str(&json).ok()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() * 1_000 + u64::from(d.subsec_millis()))
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_config() -> VmConfig {
        // Paths don't need to exist — StateStore only serializes, never opens files.
        VmConfig {
            kernel_path: PathBuf::from("/fake/kernel"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 128,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: hitz_api::DEFAULT_GUEST_CID,
            guest_agent: hitz_api::GuestAgentMode::Disabled,
        }
    }

    #[test]
    fn save_and_load_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        let config = make_config();
        store.save_config("vm1", &config).unwrap();
        let results = store.load_all().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "vm1");
        assert_eq!(results[0].1, config);
    }

    #[test]
    fn missing_state_json_defaults_to_created() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        store.save_config("vm1", &make_config()).unwrap();
        // No save_state call — state.json absent
        let results = store.load_all().unwrap();
        assert_eq!(results[0].2, VmState::Created);
    }

    #[test]
    fn running_clamped_to_stopped_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        store.save_config("vm1", &make_config()).unwrap();
        store.save_state("vm1", VmState::Running).unwrap();
        let results = store.load_all().unwrap();
        assert_eq!(results[0].2, VmState::Stopped);
    }

    #[test]
    fn corrupt_config_json_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        let vm_dir = dir.path().join("bad-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        std::fs::write(vm_dir.join("config.json"), b"not json at all").unwrap();
        let results = store.load_all().unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn delete_removes_vm_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        store.save_config("vm1", &make_config()).unwrap();
        store.save_state("vm1", VmState::Created).unwrap();
        store.delete("vm1").unwrap();
        let results = store.load_all().unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn stopped_and_failed_states_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        store.save_config("a", &make_config()).unwrap();
        store.save_state("a", VmState::Stopped).unwrap();
        store.save_config("b", &make_config()).unwrap();
        store.save_state("b", VmState::Failed).unwrap();
        let mut results = store.load_all().unwrap();
        results.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(results[0].2, VmState::Stopped);
        assert_eq!(results[1].2, VmState::Failed);
    }

    #[test]
    fn created_at_preserved_across_state_updates() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().to_path_buf()).unwrap();
        store.save_config("vm1", &make_config()).unwrap();
        store.save_state("vm1", VmState::Created).unwrap();
        // Read created_at before update
        let raw_before = store.load_raw_state("vm1").unwrap();
        // Update state
        store.save_state("vm1", VmState::Stopped).unwrap();
        let raw_after = store.load_raw_state("vm1").unwrap();
        assert_eq!(raw_before.created_at, raw_after.created_at);
    }
}
