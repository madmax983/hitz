# hitz-boot

`hitz-boot` is the direct-boot loader for the Hitz micro-VM manager.

## Overview

Rather than running a traditional BIOS or UEFI firmware, Hitz directly loads a Linux kernel (`vmlinux` ELF binary) and an optional initramfs directly into guest physical memory (GPA). This dramatically reduces boot times (sub-10ms) and reduces the hypervisor attack surface.

## Responsibilities

1. **ELF Loading:** Parses the statically linked 64-bit Linux kernel ELF file and maps its `PT_LOAD` segments into guest memory.
2. **Boot Parameters (`Zero Page`):** Constructs the Linux `boot_params` structure required by the x86 64-bit boot protocol.
3. **Initramfs Setup:** Loads a `cpio` archive into memory and configures the kernel to find it.
4. **ACPI Table Generation:** Dynamically generates minimal ACPI tables (RSDP, XSDT, MADT) needed for modern Linux to boot without legacy APIC/PIC support.
5. **Page Tables:** Builds the initial identity-mapped page tables (PML4 -> PDPT -> PD) required to enter 64-bit long mode immediately.

## Usage

This crate is used internally by the `hitz-vmm` crate during the VM initialization phase, immediately before jumping into the hypervisor run loop.
