use std::path::PathBuf;
use serde::{Deserialize, Serialize};


/// Controls whether and which guest metrics agent is injected into the initramfs.
///
/// Hitz supports injecting a lightweight agent into the guest environment to
/// extract process and resource usage telemetry. By default, it auto-injects
/// the bundled `hitz-guest-agent`.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::GuestAgentMode;
/// use std::path::PathBuf;
///
/// // The default mode uses the built-in guest agent (if available).
/// let default_mode = GuestAgentMode::default();
/// assert_eq!(default_mode, GuestAgentMode::Auto);
///
/// // You can also supply a custom static binary for testing or
/// // specialized data collection.
/// let custom_mode = GuestAgentMode::Custom(PathBuf::from("/usr/local/bin/my-agent"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", content = "path", rename_all = "lowercase")]
pub enum GuestAgentMode {
    /// Automatically inject the built-in agent (default).
    #[default]
    Auto,
    /// Inject a user-supplied agent binary instead of the built-in one.
    Custom(PathBuf),
    /// Do not inject any agent.
    Disabled,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn guest_agent_mode_custom_serde_roundtrip() {
        let mode = GuestAgentMode::Custom(std::path::PathBuf::from("/usr/local/bin/my-agent"));
        let json = serde_json::to_string(&mode).expect("serialize");
        let decoded: GuestAgentMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }
    #[test]
    fn guest_agent_mode_disabled_serde_roundtrip() {
        let mode = GuestAgentMode::Disabled;
        let json = serde_json::to_string(&mode).expect("serialize");
        let decoded: GuestAgentMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }
    #[test]
    fn guest_agent_mode_default_is_auto() {
        let mode: GuestAgentMode = GuestAgentMode::default();
        assert!(matches!(mode, GuestAgentMode::Auto));
    }
}
