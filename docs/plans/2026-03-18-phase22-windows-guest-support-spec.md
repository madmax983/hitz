# 🔭 Vantage: Spec for Windows Guest Support

## Problem Statement
Currently, Hitz exclusively supports Linux microVMs via a direct kernel boot protocol. While this serves modern cloud-native edge workloads well, a significant portion of the desktop and server market relies on Windows applications. Users cannot currently run Windows applications or test Windows system components within a Hitz microVM, artificially limiting the total addressable market of the hypervisor.

## The "So What?"
What business problem does this solve? Supporting Windows guests allows us to target enterprise CI/CD pipelines, desktop security sandboxes, and hybrid cloud environments where Windows remains a first-class citizen. Expanding beyond Linux transforms Hitz from a niche tool into a comprehensive, multi-platform virtual machine manager, directly driving enterprise adoption and increasing overall utility.

## Gap Analysis
Our current implementation in `hitz-boot` relies entirely on parsing ELF binaries and setting up Linux-specific boot parameters (e.g., zero page, `CMDLINE_GPA`). We lack UEFI firmware support, ACPI table generation, and the necessary bootloader infrastructure required to initialize a modern Windows kernel. Additionally, Windows requires specific hypervisor enlightenments to perform well, which are not fully implemented.

## Acceptance Criteria
- **User Story:** As an Enterprise Developer, I want to boot a Windows Server image within a Hitz microVM, so that I can securely run legacy Windows applications alongside my Linux microservices.
- **Metric Definition:** Success = A user can supply a standard Windows VHDX and UEFI firmware image, and Hitz successfully boots to the Windows login screen within 15 seconds.
- **Functional Requirements:**
  - Provide a mechanism to load and execute UEFI firmware (e.g., edk2/OVMF) instead of the direct Linux boot protocol.
  - Implement dynamic generation of ACPI tables to satisfy Windows hardware detection requirements.
  - Introduce support for standard block device formats (e.g., raw disk images, VHDX) containing a Windows partition layout.
  - Ensure virtio-blk and virtio-net are recognized by the Windows guest (requires Windows virtio drivers).

## Out of Scope
- **GPU Passthrough:** While graphics are necessary for desktop use cases, Phase 22 will only target headless/serial or basic framebuffer console booting. Full GPU passthrough is deferred to a future phase.
- **Windows License Management:** Hitz will not provide or manage Windows licenses. Users must supply their own licensed media.
- **Legacy BIOS (CSM) Support:** We will strictly target modern UEFI boot; legacy BIOS is out of scope to minimize complexity.
