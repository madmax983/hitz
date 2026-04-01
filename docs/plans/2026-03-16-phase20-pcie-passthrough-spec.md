# 🔭 Vantage: Spec for PCIe Device Passthrough

## Problem Statement
High-performance workloads, such as machine learning training, video encoding, and specialized network packet processing, require direct access to hardware accelerators (e.g., GPUs, FPGAs, SmartNICs). Currently, a Hitz microVM is limited to emulated or paravirtualized devices (virtio), which introduce unacceptable latency and overhead for these specialized use cases.

## The "So What?"
What business problem does this solve? AI/ML and edge computing are rapidly growing markets. Customers need to run containerized or VM-isolated AI workloads without sacrificing the bare-metal performance of their expensive GPU hardware. Without PCIe passthrough, Hitz cannot serve as the underlying infrastructure for modern, hardware-accelerated cloud workloads. With this feature, Hitz can capture a significant share of the AI infrastructure market by offering secure, fast-booting microVMs with direct GPU access.

## Gap Analysis
Standard hypervisors like KVM and Hyper-V support PCIe passthrough (via VFIO or DDA, respectively). Currently, Hitz lacks the ability to assign a physical host PCIe device directly to a guest microVM. This creates a significant gap compared to bare-metal performance or fully-featured hypervisors when running specialized workloads.

## Acceptance Criteria
- **User Story:** As an AI Researcher or Infrastructure Operator, I want to attach a physical GPU directly to my microVM so that my deep learning models can train with bare-metal hardware performance.
- **Metric Definition:** Success = A microVM can access the assigned physical PCIe device natively, achieving at least 95% of bare-metal throughput and latency for the device's specific workload (e.g., CUDA operations).
- **Functional Requirements:**
  - The API must support assigning a specific host PCIe device to a VM during creation or configuration.
  - The guest OS must be able to load native, vendor-provided drivers for the assigned hardware (e.g., Nvidia drivers).
  - The VM must securely isolate the device, ensuring it cannot access host memory outside the VM's boundaries.

## Out of Scope
- **Live Migration with Passthrough Devices:** Migrating a running VM that is currently utilizing a physical PCIe device is extremely complex and deferred to a later phase.
- **Virtual GPU (vGPU) Splitting:** Dividing a single physical GPU into multiple virtual GPUs to share across several VMs is out of scope. We are focusing on 1:1 physical device passthrough.
