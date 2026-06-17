//! REST API Request and Response Definitions.
//!
//! # Abstract
//! This module defines the serializable data structures used to communicate
//! between the Hitz CLI and the background daemon via the REST API.
//! It includes types for creating VMs, controlling their lifecycle, and
//! interrogating their state.
//!
//! # The Hero's Journey
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
use crate::lifecycle::{VmAction, VmState};
use serde::{Deserialize, Serialize};

/// Request to create a new VM with the given configuration.
///
/// # Abstract
/// This type is the payload sent from the CLI to the daemon's REST API when
/// asking to instantiate a new micro-VM. It wraps the core [`VmConfig`] so the
/// daemon knows exactly how to build the partition.
///
/// # The Hero's Journey
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
/// # The Hero's Journey
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
/// # The Hero's Journey
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
/// # The Hero's Journey
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
/// # The Hero's Journey
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
