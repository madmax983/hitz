# hitz-hal

## Abstract

The Hypervisor Abstraction Layer for Hitz. This crate defines the platform-agnostic traits that form the contract between the Virtual Machine Monitor (VMM) and any underlying hypervisor backend (such as Windows Hypervisor Platform or Linux KVM).

## The Hero's Journey

To interact with a hypervisor safely and consistently, we use the `hitz-hal` traits:

```rust
use hitz_hal::{PartitionConfig, MemSizeMiB};

// In a real application, you would instantiate a specific hypervisor implementation.
// let hv = WindowsHypervisor::new().unwrap();

let config = PartitionConfig {
    vcpu_count: 4,
    memory_size: MemSizeMiB::new(2048),
};

// hv.create_partition(&config).expect("failed to create partition");
```

## Details

This crate contains zero platform dependencies. It consists entirely of:
- **Traits**: `Hypervisor`, `Partition`, `Vcpu`, `GuestMemAccess`
- **Newtypes**: `Gpa`, `VcpuId`, `VmId`, `IrqLine`, `MmioSlot`, etc.
- **Errors**: `HalError`

By depending strictly on `hitz-hal`, components like `hitz-devices` and `hitz-vmm` can be built and tested without needing access to a specific hypervisor backend.
