# 🔭 Vantage: Spec for ARM64 Support

## Problem Statement
Currently, Hitz is tightly coupled to the x86_64 architecture and Windows Hypervisor Platform (WHP). Users wanting to leverage modern, energy-efficient ARM64 hardware (such as cloud instances with Graviton processors or Edge computing nodes using ARM) cannot run Hitz microVMs. This locks out a rapidly growing segment of the infrastructure market.

## The "So What?"
What business problem does this solve? Customer demand and cloud economics. Deploying workloads on ARM64 servers can reduce cloud infrastructure costs by up to 20-40% compared to x86_64 instances while delivering comparable or better performance. If Hitz cannot natively run on ARM64, users looking to migrate to cheaper infrastructure cannot use our hypervisor, leading to competitive disadvantage and lost revenue. Supporting ARM64 ensures Hitz remains competitive and future-proofs the platform against the industry-wide shift towards ARM.

## Gap Analysis
- **Current State:** The VMM and bootloaders are currently built specifically for x86_64 environments and cannot compile or run on ARM processors.
- **Market Standard:** Enterprise microVM managers like Firecracker have robust support for aarch64. We currently only address half the server market.

## Acceptance Criteria
- 👤 **User Story:** As a Cloud Architect, I want to deploy Hitz microVMs on ARM64 servers, so that I can run my workloads more cost-effectively and reduce power consumption.
- ✅ **Metric Definition:** Success = A user can compile Hitz for `aarch64` targets. When executed on an ARM64 host, the daemon successfully boots an ARM64 Linux kernel and initramfs to a functional shell in under 100ms.
- **Functional Requirements:**
  - Support compiling the project for ARM64 environments.
  - Implement a hypervisor backend suitable for an ARM64 platform.
  - Modify the CLI and Daemon configurations to allow specifying or inferring the guest CPU architecture.

## 🚫 Out of Scope
- **Cross-Architecture Emulation:** Emulating x86_64 guests on ARM64 hosts is out of scope. Guests must run the same architecture as the host (hardware virtualization).
- **Windows ARM64 Guest Support:** The initial phase will focus strictly on booting Linux ARM64 guests.
