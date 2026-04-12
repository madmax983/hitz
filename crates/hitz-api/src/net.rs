use serde::{Deserialize, Serialize};

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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{DEFAULT_HOST_IP, DEFAULT_GUEST_IP};

    #[test]
    fn net_config_serde_roundtrip() {
        let cfg = NetConfig {
            mac: Some("AA:BB:CC:DD:EE:FF".into()),
            host_ip: DEFAULT_HOST_IP.into(),
            guest_ip: DEFAULT_GUEST_IP.into(),
            adapter_name: Some("hitz-test".into()),
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: NetConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.host_ip, cfg.host_ip);
        assert_eq!(restored.guest_ip, cfg.guest_ip);
        assert_eq!(restored.mac, cfg.mac);
    }
    #[test]
    fn port_forward_serde_roundtrip() {
        let pf = PortForward {
            host_port: 2222,
            guest_port: 22,
        };
        let json = serde_json::to_string(&pf).expect("serialize");
        let restored: PortForward = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, pf);
    }
}
