# 🔭 Vantage: Spec for VM Cloning

## Problem Statement
Currently, users of Hitz must manually define configuration parameters (such as CPU, RAM, kernel paths, and block devices) every time they want to create a new microVM. When attempting to scale out workloads or reproduce specific testing environments, operators find themselves repeating complex configuration commands. This manual repetition is tedious, error-prone, and slows down iterative development and deployment workflows.

## The "So What?"
What business problem does this solve? In enterprise and CI/CD environments, the ability to rapidly provision identical instances is critical for horizontal scaling and parallel testing. If users cannot duplicate a known-good configuration with a single command, Hitz fails to meet the operational velocity expected from modern cloud-native tooling. Implementing a VM cloning feature significantly reduces friction for scaling, standardizes deployments, and enhances overall developer productivity.

## Gap Analysis
The existing `hitz-cli` supports a `clone` command placeholder, but the daemon and API layers do not currently implement the necessary logic to duplicate an existing VM's configuration state, allocate a new UUID, and register the new instance. Users currently rely on shell scripts or external orchestration tools to duplicate parameters, which is a brittle workaround. We lack a native, atomic operation to duplicate a VM's definition.

## Acceptance Criteria
- 👤 **User Story:** As an Infrastructure Operator, I want to clone an existing VM configuration into a new instance, so that I can rapidly spin up identical worker nodes for my cluster.
- 👤 **User Story:** As a QA Engineer, I want to duplicate a staging VM's setup exactly, so that I can reliably reproduce bugs in an isolated environment.
- ✅ **Metric Definition:** Success = A user executes `hitz vm clone <source_vm_id>`, and the system provisions a new VM with an identical configuration (CPU, RAM, network, disks) but a distinct, newly generated UUID. The cloning operation must complete in under 50ms and return the new VM's ID without booting the VM.
- **Functional Requirements:**
  - Update the API schema and daemon backend to support a `Clone` action.
  - The new VM must inherit all configuration fields from the source VM.
  - The new VM must be assigned a unique UUID.
  - The new VM must be created in a `Stopped` state; cloning should not automatically start the new instance.

## 🚫 Out of Scope
- **Live State Cloning:** Cloning the runtime state (RAM contents, CPU registers) of a running VM is explicitly out of scope. This feature focuses purely on configuration duplication for stopped or running VMs (resulting in a new, stopped VM).
- **Deep Disk Copying:** Cloning a VM will reference the same underlying base disk images if configured. Automated differential disk creation or deep copying of block devices is deferred to a future storage management phase.
