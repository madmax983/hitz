# 🔭 Vantage: Spec for Cloud-init Provisioning

## Problem Statement
Currently, Hitz boots a raw kernel and rootfs, but configuring the guest OS (setting up SSH keys, user accounts, networking, running startup scripts) requires manual intervention or building custom baked images per VM.

## The "So What?"
What business problem does this solve? Users want to launch "cattle, not pets". Without an automated way to inject configuration data at boot time, Hitz cannot integrate well with modern infrastructure-as-code tools or CI/CD pipelines. Supporting a standard like `cloud-init` via NoCloud data source solves this, allowing customers to dynamically configure thousands of VMs automatically, thereby increasing platform adoption.

## Gap Analysis
Standard hypervisors and cloud platforms (like AWS EC2, QEMU/KVM, Firecracker) provide metadata services or secondary block volumes to deliver initialization data on boot. Hitz currently lacks any automated mechanism to pass runtime configuration down to the guest, meaning users are stuck maintaining numerous monolithic images instead of a single, composable golden image.

## Acceptance Criteria
- 👤 **User Story:** As a DevOps Engineer, I want to pass cloud-init configuration data when creating a VM, so that the VM automatically configures users, SSH keys, and installs packages on first boot.
- ✅ **Metric Definition:** Success = The VM automatically boots and executes the provided cloud-init payload without errors. Volume generation and attachment overhead must add < 100ms to the total VM creation time.

## 🚫 Out of Scope
- **Direct Cloud Provider Integration:** Direct integration with AWS/Azure/GCP metadata services.
- **Runtime Modification:** Real-time modification of cloud-init data after VM start.
