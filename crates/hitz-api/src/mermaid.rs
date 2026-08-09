#![allow(clippy::module_name_repetitions, clippy::unwrap_used)]

use crate::config::VmConfig;

use std::fmt::Write;

/// Trait to generate a Mermaid graph from configuration.
pub trait ToMermaid {
    /// Generates the Mermaid definition string.
    #[must_use]
    fn to_mermaid(&self) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self) -> String {
        let mut out = String::new();
        out.push_str("graph TD\n");
        let _ = writeln!(
            out,
            "  VM[Micro-VM: {cpus} vCPU, {ram} MiB RAM]",
            cpus = self.cpus,
            ram = self.ram_mib
        );
        if let Some(ref net) = self.net {
            let _ = writeln!(
                out,
                "  HOST_NET((Host Net: {hip})) -->|Virtual Switch| VM_NET((Guest Net: {gip}))",
                hip = net.host_ip,
                gip = net.guest_ip
            );
            out.push_str("  VM_NET --- VM\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_mermaid_basic() {
        let json = r#"{"kernel_path":"vmlinux","ram_mib":1024,"cpus":2,"net":{"host_ip":"10.0.0.1/24","guest_ip":"10.0.0.2/24"}}"#;
        let config: VmConfig = serde_json::from_str(json).unwrap();
        let graph = config.to_mermaid();
        assert!(graph.contains("graph TD"));
        assert!(graph.contains("Micro-VM: 2 vCPU"));
    }
}
