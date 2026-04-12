use serde::{Deserialize, Serialize};

/// Action that can be performed on a running VM.
///
/// This enum represents the discrete commands that can be issued to
/// the daemon to control a micro-VM's lifecycle over the REST API.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmAction;
///
/// let action = VmAction::Start;
///
/// match action {
///     VmAction::Start => println!("Starting the VM..."),
///     VmAction::Stop => println!("Stopping the VM..."),
///     VmAction::Restart => println!("Restarting the VM..."),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmAction {
    /// Start (resume) a created or stopped VM.
    Start,
    /// Stop (pause/shutdown) a running VM.
    Stop,
    /// Restart a running VM by stopping it and then starting it.
    Restart,
}

/// Current lifecycle state of a VM.
///
/// This tracks the operational status of a micro-VM from its initial
/// creation, through active execution, until it is gracefully stopped
/// or encounters a fatal error.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmState;
///
/// let state = VmState::Running;
///
/// assert!(matches!(state, VmState::Running));
///
/// if state == VmState::Failed {
///     println!("The VM encountered a fatal error.");
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmState {
    /// VM has been created but not yet started.
    Created,
    /// VM is actively running.
    Running,
    /// VM has been stopped (gracefully or by request).
    Stopped,
    /// VM encountered a fatal error.
    Failed,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn vm_action_serde_roundtrip() {
        for action in [VmAction::Start, VmAction::Stop, VmAction::Restart] {
            let json = serde_json::to_string(&action).expect("serialize");
            let restored: VmAction = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, action);
        }
    }
    #[test]
    fn vm_state_serde_roundtrip() {
        for state in [
            VmState::Created,
            VmState::Running,
            VmState::Stopped,
            VmState::Failed,
        ] {
            let json = serde_json::to_string(&state).expect("serialize");
            let restored: VmState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, state);
        }
    }
}
