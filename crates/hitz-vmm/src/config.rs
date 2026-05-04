use std::path::PathBuf;

/// Network configuration for a VM during boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetConfig {
    /// MAC
    pub mac: Option<String>,
    /// Host IP
    pub host_ip: String,
    /// Guest IP
    pub guest_ip: String,
    /// Adapter name
    pub adapter_name: Option<String>,
}

/// Guest agent injection mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GuestAgentMode {
    #[default]
    /// Auto
    Auto,
    /// Custom
    Custom(PathBuf),
    /// Disabled
    Disabled,
}

/// Internal VM configuration for the hypervisor boot sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmConfig {
    /// Kernel
    pub kernel_path: PathBuf,
    /// Initramfs
    pub initramfs_path: Option<PathBuf>,
    /// Disk
    pub disk_path: Option<PathBuf>,
    /// RAM
    pub ram_mib: u32,
    /// CPUs
    pub cpus: u32,
    /// Cmdline
    pub cmdline: String,
    /// Net
    pub net: Option<NetConfig>,
    /// CID
    pub guest_cid: u32,
    /// Agent
    pub guest_agent: GuestAgentMode,
}

impl VmConfig {
    /// Gets the cmdline to use (stubbed out to return self.cmdline for API compatibility)
    #[must_use]
    pub fn effective_cmdline(&self) -> &str {
        &self.cmdline
    }
}
