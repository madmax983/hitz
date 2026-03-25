# Phase 15: Cloud-init Provisioning — Spec

## Problem Statement
Currently, Hitz boots a raw kernel and rootfs, but configuring the guest OS (setting up SSH keys, user accounts, networking, running startup scripts) requires manual intervention or building custom baked images per VM.

## The "So What?"
Users want to launch "cattle, not pets". Without an automated way to inject configuration data at boot time, Hitz cannot integrate well with modern infrastructure-as-code tools or CI/CD pipelines. Supporting a standard like `cloud-init` via NoCloud data source solves this.

## Goal
Enable automatic, declarative guest OS configuration using standard `cloud-init` user-data and meta-data, injected via a secondary virtio-block device.

## Scope

**In Scope:**
- Injecting a secondary FAT/ISO virtio-block volume containing NoCloud data (`user-data`, `meta-data`, `vendor-data`).
- Modifying `VmConfig` and `CreateVmRequest` to accept cloud-init payload strings or file paths.
- Automatically generating the NoCloud volume at VM creation time.

**Out of Scope:**
- Direct integration with AWS/Azure/GCP metadata services.
- Real-time modification of cloud-init data after VM start.

## Acceptance Criteria
- **User Story:** As a DevOps Engineer, I want to pass a cloud-init `user-data` file when creating a VM, so that the VM automatically configures users, SSH keys, and installs packages on first boot.
- **Metric:** The VM must boot and execute the cloud-init payload without panicking the VMM. The volume generation must add < 100ms to the VM creation time.
- **Functionality:**
  - The CLI and API must support providing cloud-init data.
  - Hitz must generate a valid NoCloud datasource image (e.g., a FAT16 filesystem with the volume label `cidata`).
  - The generated image must be attached as a read-only virtio-block device during boot.

## Proposed API Additions

### `VmConfig` Updates
```json
{
  "cloud_init": {
    "user_data": "I2Nsb3VkLWNvbmZpZwp1c2VyczoKICAtIG5hbWU6IGhpdHpsaW51eA==", // base64 encoded or string
    "meta_data": "..." // optional
  }
}
```

### CLI Updates
```bash
hitz vm create \
  --kernel vmlinux \
  --disk rootfs.ext4 \
  --user-data ./cloud-config.yaml
```
