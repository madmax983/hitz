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

impl std::fmt::Display for VmAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        };
        write!(f, "{s}")
    }
}

impl VmAction {
    /// Returns the gerund form of the action (e.g., "Starting", "Stopping").
    #[must_use]
    pub const fn gerund(&self) -> &'static str {
        match self {
            Self::Start => "Starting",
            Self::Stop => "Stopping",
            Self::Restart => "Restarting",
        }
    }

    /// Returns the past tense form of the action (e.g., "started", "stopped").
    #[must_use]
    pub const fn past_tense(&self) -> &'static str {
        match self {
            Self::Start => "started",
            Self::Stop => "stopped",
            Self::Restart => "restarted",
        }
    }
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
