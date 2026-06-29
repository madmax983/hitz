use std::path::PathBuf;

pub const DEFAULT_CMDLINE: &str = "console=ttyS0 earlyprintk=serial";
pub const DEFAULT_RAM_MIB: u32 = 256;
pub const DEFAULT_CPUS: u32 = 1;
pub const DEFAULT_HOST_IP: &str = "192.168.100.1/24";
pub const DEFAULT_GUEST_IP: &str = "192.168.100.2/24";
pub const DEFAULT_GUEST_CID: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetConfig {
    pub mac: Option<String>,
    pub host_ip: String,
    pub guest_ip: String,
    pub adapter_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortForward {
    pub host_port: u16,
    pub guest_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GuestAgentMode {
    #[default]
    Auto,
    Custom(PathBuf),
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmConfig {
    pub kernel_path: PathBuf,
    pub initramfs_path: Option<PathBuf>,
    pub disk_path: Option<PathBuf>,
    pub ram_mib: u32,
    pub cpus: u32,
    pub cmdline: Option<String>,
    pub net: Option<NetConfig>,
    pub ports: Vec<PortForward>,
    pub guest_cid: u32,
    pub guest_agent: GuestAgentMode,
}

impl VmConfig {
    #[must_use]
    pub fn effective_cmdline(&self) -> &str {
        self.cmdline.as_deref().unwrap_or(DEFAULT_CMDLINE)
    }
}
