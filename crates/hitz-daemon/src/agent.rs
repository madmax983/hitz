//! Guest agent embedding and cpio overlay construction.

use hitz_api::GuestAgentMode;
use hitz_vmm::CpioBuilder;

/// The built-in agent binary, embedded at compile time.
///
/// Empty slice when cross-compilation is unavailable (the build script writes
/// a zero-byte placeholder so `include_bytes!` always compiles).
static AGENT_BYTES: &[u8] = include_bytes!(env!("HITZ_AGENT_BIN"));

const INIT_SCRIPT: &[u8] = b"#!/bin/sh\n/sbin/hitz-agent &\n";

/// Build a newc cpio overlay containing the guest agent and its init script.
///
/// The overlay can be appended directly to the user's initramfs — the
/// kernel processes concatenated cpio archives left-to-right.
#[must_use]
pub fn build_agent_overlay(agent_bytes: &[u8]) -> Vec<u8> {
    CpioBuilder::new()
        .add_file("sbin/hitz-agent", agent_bytes, 0o755)
        .add_file("etc/init.d/S99hitz-agent", INIT_SCRIPT, 0o755)
        .finish()
}

/// Resolve which agent bytes to use given the config mode.
///
/// Returns `None` when injection should be skipped.
#[must_use]
pub fn resolve_agent_bytes(mode: &GuestAgentMode) -> Option<Vec<u8>> {
    match mode {
        GuestAgentMode::Disabled => None,
        GuestAgentMode::Auto => {
            if AGENT_BYTES.is_empty() {
                tracing::warn!(
                    "GuestAgentMode::Auto: no agent binary available; skipping injection"
                );
                None
            } else {
                Some(AGENT_BYTES.to_vec())
            }
        }
        GuestAgentMode::Custom(path) => match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                tracing::warn!(path = %path.display(), "failed to read custom agent: {e}");
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_is_non_empty_cpio() {
        let overlay = build_agent_overlay(b"ELF_FAKE_BINARY");
        assert_eq!(&overlay[0..6], b"070701");
        assert!(overlay.len() > 300);
    }

    #[test]
    fn overlay_contains_agent_and_init_script() {
        let overlay = build_agent_overlay(b"AGENT");
        let text = String::from_utf8_lossy(&overlay);
        assert!(text.contains("sbin/hitz-agent"));
        assert!(text.contains("S99hitz-agent"));
    }
}
