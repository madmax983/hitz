//! VM Configuration Types.
//!
//! # Abstract
//! This module defines the blueprint structures for configuring a micro-VM.
//! It includes the core [`VmConfig`] which specifies hardware resources
//! (CPU, RAM), boot assets (kernel, initramfs), and host integration settings
//! (networking, port forwarding, and guest agent injection).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, NetConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // Define the full hardware and software blueprint for a micro-VM
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: Some(PathBuf::from("/boot/init.cpio")),
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: Some("console=ttyS0 quiet".to_string()),
//!     net: Some(NetConfig {
//!         mac: None,
//!         host_ip: "192.168.100.1/24".to_string(),
//!         guest_ip: "192.168.100.2/24".to_string(),
//!         adapter_name: None,
//!     }),
//!     ports: vec![],
//!     guest_cid: 4,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! assert_eq!(config.cpus, 4);
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default kernel command line for Linux direct boot.
pub const DEFAULT_CMDLINE: &str = "console=ttyS0 earlyprintk=serial";

/// Default guest RAM in MiB (256 MiB).
pub const DEFAULT_RAM_MIB: u32 = 256;

/// Default number of virtual CPUs.
pub const DEFAULT_CPUS: u32 = 1;

/// Default host IP for guest networking.
pub const DEFAULT_HOST_IP: &str = "192.168.100.1/24";
/// Default guest IP for guest networking.
pub const DEFAULT_GUEST_IP: &str = "192.168.100.2/24";

/// Default guest CID for virtio-vsock (host=2, first guest=3).
pub const DEFAULT_GUEST_CID: u32 = 3;

/// Metrics port on which the guest agent listens and the host connects.
pub const VSOCK_METRICS_PORT: u32 = 52355;

/// Host CID as defined by the virtio-vsock spec.
pub const VMADDR_CID_HOST: u32 = 2;

/// Network configuration for a VM.
///
/// This structure defines how the micro-VM connects to the host network.
/// By default, Hitz sets up a point-to-point interface (like `WinTun` on Windows
/// or `TAP` on Linux) to allow network traffic between the host and the guest.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetConfig;
///
/// let net = NetConfig {
///     mac: Some("AA:BB:CC:DD:EE:FF".to_string()),
///     host_ip: "192.168.100.1/24".to_string(),
///     guest_ip: "192.168.100.2/24".to_string(),
///     adapter_name: Some("hitz-dev-01".to_string()),
/// };
///
/// assert_eq!(net.host_ip, "192.168.100.1/24");
/// assert_eq!(net.guest_ip, "192.168.100.2/24");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetConfig {
    /// Guest MAC address (e.g. "AA:BB:CC:DD:EE:FF"). Random if `None`.
    pub mac: Option<String>,
    /// Host-side IP address with prefix (e.g. "192.168.100.1/24").
    pub host_ip: String,
    /// Guest-side IP address with prefix (e.g. "192.168.100.2/24").
    pub guest_ip: String,
    /// `WinTun` adapter name. Defaults to "hitz-{vm_id}" if `None`.
    pub adapter_name: Option<String>,
}

/// A single TCP port forward rule: `host_port` on the host forwards to
/// `guest_port` inside the VM.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::PortForward;
///
/// // Forward host port 8080 to guest port 80
/// let rule = PortForward {
///     host_port: 8080,
///     guest_port: 80,
/// };
///
/// assert_eq!(rule.host_port, 8080);
/// assert_eq!(rule.guest_port, 80);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForward {
    /// Port to listen on the host (e.g. `2222`).
    pub host_port: u16,
    /// Port to connect to in the guest (e.g. `22`).
    pub guest_port: u16,
}

// ── Guest agent mode ─────────────────────────────────────────────────────────

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

// ── VmConfig helpers ──────────────────────────────────────────────────────────

const fn default_cpus() -> u32 {
    DEFAULT_CPUS
}

const fn default_guest_cid() -> u32 {
    DEFAULT_GUEST_CID
}

/// VM configuration — everything needed to boot a micro-VM.
///
/// Serializable for the future daemon REST API (Phase 6).
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{VmConfig, GuestAgentMode};
/// use std::path::PathBuf;
///
/// let config = VmConfig {
///     kernel_path: PathBuf::from("/boot/vmlinux"),
///     initramfs_path: Some(PathBuf::from("/boot/init.cpio")),
///     disk_path: None,
///     ram_mib: 512,
///     cpus: 2,
///     cmdline: Some("console=ttyS0 quiet".to_string()),
///     net: None,
///     ports: vec![],
///     guest_cid: 4,
///     guest_agent: GuestAgentMode::Auto,
/// };
///
/// assert_eq!(config.ram_mib, 512);
/// assert_eq!(config.cpus, 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmConfig {
    /// Path to the kernel ELF binary (vmlinux).
    pub kernel_path: PathBuf,
    /// Optional initramfs (cpio archive) path.
    pub initramfs_path: Option<PathBuf>,
    /// Optional disk image path for virtio-blk.
    pub disk_path: Option<PathBuf>,
    /// Guest RAM in MiB (default: 256).
    pub ram_mib: u32,
    /// Number of virtual CPUs (default: 1).
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    /// Custom kernel command line (default: [`DEFAULT_CMDLINE`]).
    pub cmdline: Option<String>,
    /// Optional network configuration for virtio-net.
    pub net: Option<NetConfig>,
    /// TCP port forwards. Empty = no forwarding.
    ///
    /// Each rule opens a `TcpListener` on `0.0.0.0:host_port` and proxies
    /// connections to `guest_ip:guest_port`. Ignored if `net` is `None`.
    #[serde(default)]
    pub ports: Vec<PortForward>,
    /// Virtio-vsock guest CID. Must be unique per running VM. Default: 3.
    #[serde(default = "default_guest_cid")]
    pub guest_cid: u32,
    /// Guest metrics agent injection mode.
    #[serde(default)]
    pub guest_agent: GuestAgentMode,
}

impl VmConfig {
    /// Determines the final kernel boot parameters.
    ///
    /// # Abstract
    /// Evaluates whether the user supplied a custom kernel command line. If they
    /// did not, it falls back to the system's `DEFAULT_CMDLINE` to ensure the
    /// micro-VM always has a valid boot string (e.g., configuring the serial console).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{VmConfig, DEFAULT_CMDLINE, DEFAULT_RAM_MIB, DEFAULT_CPUS, DEFAULT_GUEST_CID, GuestAgentMode};
    /// use std::path::PathBuf;
    ///
    /// let mut config = VmConfig {
    ///     kernel_path: PathBuf::from("vmlinux"),
    ///     initramfs_path: None,
    ///     disk_path: None,
    ///     ram_mib: DEFAULT_RAM_MIB,
    ///     cpus: DEFAULT_CPUS,
    ///     cmdline: None,
    ///     net: None,
    ///     ports: vec![],
    ///     guest_cid: DEFAULT_GUEST_CID,
    ///     guest_agent: GuestAgentMode::Auto,
    /// };
    ///
    /// assert_eq!(config.effective_cmdline(), DEFAULT_CMDLINE);
    ///
    /// config.cmdline = Some("root=/dev/vda rw".to_string());
    /// assert_eq!(config.effective_cmdline(), "root=/dev/vda rw");
    /// ```
    #[must_use]
    pub fn effective_cmdline(&self) -> &str {
        self.cmdline.as_deref().unwrap_or(DEFAULT_CMDLINE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_cmdline_default() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: DEFAULT_CPUS,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        };
        assert_eq!(config.effective_cmdline(), DEFAULT_CMDLINE);
    }

    #[test]
    fn test_effective_cmdline_custom() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: DEFAULT_CPUS,
            cmdline: Some("custom cmdline".to_string()),
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        };
        assert_eq!(config.effective_cmdline(), "custom cmdline");
    }

    #[test]
    fn test_default_helpers() {
        assert_eq!(default_cpus(), DEFAULT_CPUS);
        assert_eq!(default_guest_cid(), DEFAULT_GUEST_CID);
    }

    #[test]
    fn test_guest_agent_mode_default() {
        assert_eq!(GuestAgentMode::default(), GuestAgentMode::Auto);
    }
}
