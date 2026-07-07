//! REST API Request and Response Definitions.
//!
//! # Abstract
//! This module defines the serializable data structures used to communicate
//! between the Hitz CLI and the background daemon via the REST API.
//! It includes types for creating VMs, controlling their lifecycle, and
//! interrogating their state.
//!
//! ## Examples
//!
//! ```rust
//! use hitz_api::{CreateVmRequest, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // 1. Construct a request to create a new VM.
//! let req = CreateVmRequest {
//!     config: VmConfig {
//!         kernel_path: PathBuf::from("/vmlinux"),
//!         initramfs_path: None,
//!         disk_path: None,
//!         ram_mib: 512,
//!         cpus: 2,
//!         cmdline: None,
//!         net: None,
//!         ports: vec![],
//!         guest_cid: 3,
//!         guest_agent: GuestAgentMode::Auto,
//!     },
//! };
//!
//! // 2. Serialize to JSON to send to the daemon over HTTP.
//! let json = serde_json::to_string(&req).unwrap();
//! println!("Sending payload: {}", json);
//! ```

use crate::config::VmConfig;
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

/// Request to create a new VM with the given configuration.
///
/// # Abstract
/// This type is the payload sent from the CLI to the daemon's REST API when
/// asking to instantiate a new micro-VM. It wraps the core [`VmConfig`] so the
/// daemon knows exactly how to build the partition.
///
/// ## Examples
/// ```rust
/// use hitz_api::{CreateVmRequest, VmConfig, GuestAgentMode};
/// use std::path::PathBuf;
///
/// // Create the request object containing the blueprint for our new VM
/// let req = CreateVmRequest {
///     config: VmConfig {
///         kernel_path: PathBuf::from("/path/to/vmlinux"),
///         initramfs_path: None,
///         disk_path: None,
///         ram_mib: 256,
///         cpus: 1,
///         cmdline: None,
///         net: None,
///         ports: vec![],
///         guest_agent: GuestAgentMode::Auto,
///         guest_cid: 3,
///     },
/// };
///
/// // Verify we packed 1 CPU for the journey
/// assert_eq!(req.config.cpus, 1);
/// ```
///
/// # Details
/// This request must be serialized into JSON and sent via a `PUT` request to
/// the daemon at `/vms/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    /// Configuration blueprint for the VM to create.
    pub config: VmConfig,
}

/// Request to perform an action on an existing VM.
///
/// # Abstract
/// This type is used to command the daemon to change the lifecycle state of a VM.
/// Instead of separate endpoints for starting, stopping, or restarting, a single
/// `POST /vms/{id}/action` endpoint accepts this payload.
///
/// ## Examples
/// ```rust
/// use hitz_api::{ActionVmRequest, VmAction};
///
/// // Prepare a command to gracefully halt the VM
/// let req = ActionVmRequest {
///     action: VmAction::Stop,
/// };
///
/// // Confirm the intent is to stop
/// assert_eq!(req.action, VmAction::Stop);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionVmRequest {
    /// The discrete action to perform (e.g., [`VmAction::Start`] or [`VmAction::Stop`]).
    pub action: VmAction,
}

/// Request to clone an existing VM.
///
/// # Abstract
/// Used to duplicate an existing VM configuration into a new instance with a
/// specified destination ID. This is particularly useful for spawning multiple
/// identical workers.
///
/// ## Examples
/// ```rust
/// use hitz_api::CloneVmRequest;
///
/// // Tell the daemon to create a carbon-copy VM named 'cloned-vm-01'
/// let req = CloneVmRequest {
///     dest_id: "cloned-vm-01".to_string(),
/// };
///
/// assert_eq!(req.dest_id, "cloned-vm-01");
/// ```
///
/// # Details
/// Sent via `POST /vms/{src_id}/clone`. The source VM can be in any [`VmState`],
/// but the newly cloned VM will always start in the [`VmState::Created`] state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneVmRequest {
    /// The new unique identifier for the cloned VM instance.
    pub dest_id: String,
}

/// Information about a VM instance returned by the API.
///
/// # Abstract
/// Summarizes the identity, current state, and configuration of a micro-VM.
/// The daemon responds with this payload when querying a specific VM or listing
/// all VMs.
///
/// ## Examples
/// ```rust
/// use hitz_api::{VmInfo, VmState, VmConfig, GuestAgentMode};
/// use std::path::PathBuf;
///
/// // The daemon returns the vital statistics of the running VM
/// let info = VmInfo {
///     id: "my-vm".to_string(),
///     state: VmState::Running,
///     config: VmConfig {
///         kernel_path: PathBuf::from("/vmlinux"),
///         initramfs_path: None,
///         disk_path: None,
///         ram_mib: 512,
///         cpus: 2,
///         cmdline: None,
///         net: None,
///         ports: vec![],
///         guest_agent: GuestAgentMode::Auto,
///         guest_cid: 3,
///     },
///     exit_reason: None,
/// };
///
/// // We can check if it's still alive
/// assert_eq!(info.id, "my-vm");
/// assert_eq!(info.state, VmState::Running);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    /// The unique identifier assigned to this VM.
    pub id: String,
    /// The current lifecycle status (e.g., [`VmState::Running`]).
    pub state: VmState,
    /// The exact configuration the VM was created with.
    pub config: VmConfig,
    /// If the VM exited or failed, a human-readable explanation of why.
    pub exit_reason: Option<String>,
}

/// Error response from the API.
///
/// # Abstract
/// When an API call fails (like trying to start a VM that doesn't exist), the
/// daemon returns this structured error detailing what went wrong. It's the
/// first line of defense for debugging CLI interactions.
///
/// ## Examples
/// ```rust
/// use hitz_api::ApiError;
///
/// // The daemon encountered a user error and politely explains why
/// let err = ApiError {
///     message: "VM not found: my-vm".to_string(),
/// };
///
/// assert_eq!(err.message, "VM not found: my-vm");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// A human-readable, descriptive error message explaining the failure.
    pub message: String,
}
