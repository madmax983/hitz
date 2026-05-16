# Phase 38: Virtio-RNG Support

## User Story
As a security-conscious administrator, I want virtual machines to have access to a high-quality source of entropy provided by the host, so that cryptographic operations inside the guest are secure and performant.

## Acceptance Criteria
- The guest OS must detect a hardware random number generator.
- The hypervisor must supply entropy from a secure host source to the guest upon request.
- **Metric Definition:** The throughput of random bytes provided to the guest should be > 1 MB/s to prevent entropy starvation during boot.
- **So What?:** "What business problem does this solve?" Guests with low entropy can experience slow boots or weak cryptographic keys, impacting both performance and security. Supplying entropy directly from the host resolves this.
- **Gap Analysis:** Currently, guests rely on self-generated entropy, which can be insufficient especially during early boot. Standard hypervisors like QEMU/KVM offer Virtio-RNG to solve this common problem.

## Out of Scope
- Advanced features like rate-limiting entropy per guest or integration with specialized hardware security modules (HSMs) are deferred to future phases.
