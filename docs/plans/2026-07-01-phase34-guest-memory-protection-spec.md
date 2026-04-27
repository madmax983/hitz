# 🔭 Vantage: Spec for Guest-to-Host Memory Protection

## Problem Statement
Currently, our memory boundaries and bounds-checking abstractions (while using safe integer operations to prevent crashes) rely heavily on host-side process isolation. We do not have a formal framework or integration for utilizing hardware-assisted guest memory protection (such as AMD SEV or Intel TDX) when exposing microVMs to untrusted host administrators or highly sensitive, compliant workloads. The lack of encrypted memory bounds prevents Hitz from being deployed in highly regulated "Confidential Computing" scenarios.

## The "So What?"
What business problem does this solve? "Confidential Computing" is a massive growth sector for enterprise cloud deployments. Customers handling financial, healthcare, or government data require cryptographic proof that the hypervisor operator (the host) cannot read the memory of the running virtual machine. Without hardware-assisted memory protection, our market is limited to trusted infrastructure. Implementing this feature unlocks high-compliance enterprise sectors and positions Hitz as a viable alternative for confidential micro-workloads.

## Gap Analysis
- **Current State:** The VMM maps guest memory directly into the host's virtual address space (e.g., via `VirtualAlloc`), making the memory entirely transparent to the host OS and any user with administrator privileges.
- **Market Standard:** Modern hypervisors (QEMU/KVM, Hyper-V) support confidential computing extensions (AMD SEV-SNP, Intel TDX) to encrypt guest RAM with hardware-managed keys, rendering it unreadable to the host. Hitz currently lacks the API and hypervisor configuration hooks to enable these hardware features.

## Acceptance Criteria
- 👤 **User Story:** As a Security-Conscious Enterprise Customer, I want to boot my microVM with hardware memory encryption enabled, so that I can confidently process PII on untrusted host infrastructure without fear of host-level memory scraping.
- 👤 **User Story:** As a Platform Operator, I want to expose a configuration flag during VM creation to request a confidential VM, so that I can offer "Confidential Computing" as a premium tier to my users.
- ✅ **Metric Definition:** Success = A user creates a VM with a `confidential: true` flag. The VM successfully boots on supported hardware (e.g., AMD SEV), and any host-side attempt to read the guest's physical memory pages (via host memory dumps or debugger attachment) returns encrypted ciphertext, while the guest continues to operate at >85% of standard unencrypted performance.
- **Functional Requirements:**
  - Introduce a configuration option in the REST API for creating a VM to request hardware-assisted memory encryption.
  - Plumb this configuration down to the WHP/VMM layer to initialize the partition with the appropriate hardware isolation flags.
  - Ensure the bootloader and kernel loading sequence properly handle the memory encryption context (e.g., measuring the initial boot image if required by the hardware protocol).

## 🚫 Out of Scope
- **Software-Based Memory Encryption:** We are strictly leveraging hardware extensions (AMD SEV/Intel TDX). Software-emulated obfuscation is not acceptable.
- **Remote Attestation Verification:** Providing a full attestation service or verification client is out of scope for this initial enablement phase. We only focus on enabling the hardware encryption boundary.
