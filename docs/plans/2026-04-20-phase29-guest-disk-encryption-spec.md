# 🔭 Vantage: Spec for Guest Disk Encryption

## Problem Statement
When deploying microVMs on untrusted hardware or in multi-tenant environments, any individual with access to the host machine can easily inspect or modify the guest virtual disk images. Currently, Hitz does not provide a built-in mechanism to encrypt these disk images at rest. Users who need to meet security compliance requirements or protect sensitive customer data are forced to rely on complex, external OS-level solutions like LUKS inside the guest, which adds significant overhead and friction to VM deployment.

## The "So What?"
What business problem does this solve? Data at rest must be secure. Without encryption, our enterprise and security-conscious customers cannot adopt Hitz for sensitive workloads. By integrating transparent disk encryption directly into the hypervisor layer (or supporting encrypted block devices), we ensure that virtual machine disks remain completely opaque to host-level inspection. This significantly bolsters Hitz's security posture and unlocks deployment in highly regulated industries.

## Gap Analysis
Our current virtio-block device implementation (used in Hitz) directly reads and writes raw disk image files. While the guest OS could potentially implement LUKS within its own filesystem, managing the encryption keys in a microVM environment—especially dynamically generated ones—is cumbersome. Standard hypervisors like QEMU support encrypted disk formats (e.g., LUKS) natively, allowing the VMM to handle decryption transparently before exposing the block device to the guest, or at least facilitating secure key injection.

## Acceptance Criteria
- 👤 **User Story:** As a Security Administrator, I want the virtual machine's root filesystem and attached block devices to be encrypted at rest, so that even if the host physical machine is compromised or drives are stolen, the guest data remains inaccessible.
- 👤 **User Story:** As a Developer, I want to provision encrypted VMs by supplying an encryption key or passphrase via the API at boot time, so that I can seamlessly integrate encrypted VMs into my automated deployment pipelines.
- ✅ **Metric Definition:** Success = A user can boot a VM using a LUKS-encrypted raw disk image by providing the correct key during the `hitz vm start` API call. The read/write performance of the encrypted disk should not degrade by more than 15% compared to unencrypted raw disks.
- **Functional Requirements:**
  - Introduce an `encryption_key` (or similar secure credential delivery mechanism) to the VM boot configuration API.
  - The `hitz-devices` or block backend layer must transparently handle the decryption/encryption of block I/O requests when configured.
  - Support for the industry-standard LUKS format is preferred for interoperability.

## 🚫 Out of Scope
- **Host Memory Encryption:** This spec covers *disk* encryption (data at rest). Memory encryption (e.g., AMD SEV, Intel TDX) is a separate, hardware-dependent feature and is out of scope.
- **Key Management Service (KMS) Integration:** We will not build a full KMS. We assume the user/orchestrator provides the key directly via the API.
