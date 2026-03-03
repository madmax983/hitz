//! REST API types for Hitz.
//!
//! Request/response types and the `VmAction` enum. No I/O — pure data.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default kernel command line for Linux direct boot.
pub const DEFAULT_CMDLINE: &str = "console=ttyS0 earlyprintk=serial";

/// Default guest RAM in MiB (256 MiB).
pub const DEFAULT_RAM_MIB: u32 = 256;

/// VM configuration — everything needed to boot a micro-VM.
///
/// Serializable for the future daemon REST API (Phase 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    /// Path to the kernel ELF binary (vmlinux).
    pub kernel_path: PathBuf,
    /// Optional initramfs (cpio archive) path.
    pub initramfs_path: Option<PathBuf>,
    /// Optional disk image path for virtio-blk.
    pub disk_path: Option<PathBuf>,
    /// Guest RAM in MiB (default: 256).
    pub ram_mib: u32,
    /// Custom kernel command line (default: [`DEFAULT_CMDLINE`]).
    pub cmdline: Option<String>,
}

impl VmConfig {
    /// Returns the effective command line: custom if set, otherwise the default.
    #[must_use]
    pub fn effective_cmdline(&self) -> &str {
        self.cmdline.as_deref().unwrap_or(DEFAULT_CMDLINE)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn effective_cmdline_default() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cmdline: None,
        };
        assert_eq!(cfg.effective_cmdline(), DEFAULT_CMDLINE);
    }

    #[test]
    fn effective_cmdline_custom() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cmdline: Some("root=/dev/vda rw".into()),
        };
        assert_eq!(cfg.effective_cmdline(), "root=/dev/vda rw");
    }

    #[test]
    fn default_ram_mib_value() {
        assert_eq!(DEFAULT_RAM_MIB, 256);
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: Some(PathBuf::from("/boot/init.cpio")),
            disk_path: Some(PathBuf::from("/images/root.img")),
            ram_mib: 512,
            cmdline: Some("console=ttyS0".into()),
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.kernel_path, cfg.kernel_path);
        assert_eq!(restored.initramfs_path, cfg.initramfs_path);
        assert_eq!(restored.disk_path, cfg.disk_path);
        assert_eq!(restored.ram_mib, cfg.ram_mib);
        assert_eq!(restored.cmdline, cfg.cmdline);
    }

    #[test]
    fn serde_roundtrip_minimal() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cmdline: None,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.kernel_path, cfg.kernel_path);
        assert!(restored.initramfs_path.is_none());
        assert!(restored.disk_path.is_none());
        assert_eq!(restored.ram_mib, DEFAULT_RAM_MIB);
        assert!(restored.cmdline.is_none());
    }
}
