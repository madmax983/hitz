use serde::{Deserialize, Serialize};
use crate::config::VmConfig;
use crate::lifecycle::{VmAction, VmState};

/// Request to create a new VM.
///
/// # Abstract
/// This payload is sent to the daemon to instruct it to allocate resources
/// and start managing a new VM instance. It contains the exact configuration
/// blueprint the VM should adhere to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
/// standard shape for all 4xx and 5xx API responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// A human-readable, descriptive error message explaining the failure.
    pub message: String,
}


#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::guest::GuestAgentMode;
    use crate::{DEFAULT_GUEST_CID, DEFAULT_RAM_MIB};

    fn minimal_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    #[test]
    fn vm_info_serde_roundtrip() {
        let info = VmInfo {
            id: "vm-001".into(),
            state: VmState::Running,
            config: VmConfig {
                kernel_path: PathBuf::from("/boot/vmlinux"),
                initramfs_path: None,
                disk_path: Some(PathBuf::from("/images/root.img")),
                ram_mib: 512,
                cpus: 1,
                cmdline: Some("console=ttyS0".into()),
                net: None,
                ports: vec![],
                guest_cid: DEFAULT_GUEST_CID,
                guest_agent: GuestAgentMode::Auto,
            },
            exit_reason: None,
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let restored: VmInfo = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.id, info.id);
        assert_eq!(restored.state, info.state);
        assert_eq!(restored.config.ram_mib, info.config.ram_mib);
        assert!(restored.exit_reason.is_none());
    }

    #[test]
    fn create_vm_request_serde() {
        let req = CreateVmRequest {
            config: minimal_config(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CreateVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.config.kernel_path, req.config.kernel_path);
        assert_eq!(restored.config.ram_mib, req.config.ram_mib);
    }

    #[test]
    fn action_vm_request_serde() {
        let req = ActionVmRequest {
            action: VmAction::Stop,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: ActionVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.action, VmAction::Stop);
    }

    #[test]
    fn clone_vm_request_serde() {
        let req = CloneVmRequest {
            dest_id: "new-vm-123".into(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CloneVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.dest_id, req.dest_id);
    }

    #[test]
    fn api_error_serde() {
        let err = ApiError {
            message: "VM not found".into(),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let restored: ApiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.message, err.message);
    }

}
