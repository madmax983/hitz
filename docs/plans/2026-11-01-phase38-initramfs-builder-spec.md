# 🔭 Vantage: Spec for Automated Guest Initramfs Builder

## Problem Statement
Currently, our test suite requires engineers to manually provide a custom guest initramfs containing a TCP echo listener (via the `HITZ_TEST_INITRAMFS` environment variable) to verify port forwarding end-to-end. Without an automated, reproducible way to build this initramfs as part of our CI/CD and testing pipeline, the integration tests are often skipped, leading to a brittle testing environment and potential undetected networking regressions.

## The "So What?"
What business problem does this solve? Testing infrastructure must be hermetic, reproducible, and automatic to be effective. If developers have to manually download or construct test payloads, they will skip the tests. By automating the creation of the minimal TCP echo initramfs required for our `hitz-whp` networking tests, we ensure that every developer and CI runner executes the critical port-forwarding data-plane verification. This guarantees network reliability for our customers without imposing manual overhead on our engineering team.

## Gap Analysis
- **Current State:** As defined in Phase 21, we identified the need for a TCP echo listener inside the guest. While the spec exists for *what* the initramfs must do, we currently lack a reproducible, automated mechanism (e.g., a build script or Dockerfile) within our repository to actually generate this `.cpio.gz` artifact. The test relies on an external, manually supplied file.
- **Market Standard:** Modern infrastructure projects (like Firecracker) bundle automated scripts to predictably build their test kernels and initramfs payloads directly from source.

## Acceptance Criteria
- 👤 **User Story:** As a Core Engineer, I want an automated build script in the repository to generate the test initramfs containing the TCP echo server, so that I can run the full network integration test suite without manually assembling the guest OS payload.
- ✅ **Metric Definition:** Success = Running a single command (e.g., `make test-initramfs`) reproducibly outputs a `.cpio.gz` file that successfully passes the `phase10_port_forward_tcp` integration test 100% of the time on any developer machine or CI runner.
- **Functional Requirements:**
  - Must provide a script or toolchain definition that compiles the TCP echo listener from source.
  - Must package the compiled binary and necessary dependencies into a bootable `initramfs.cpio.gz` format compatible with our test kernel.
  - The build process must not require manual interactive steps.

## 🚫 Out of Scope
- Modifying the VMM or networking crates (this is purely an infrastructure/tooling task).
- Building the Linux kernel itself (we will continue to supply the kernel externally or address that in a separate phase).
