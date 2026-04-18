# 🔭 Vantage: Spec for Kubernetes CRI Integration

## Problem Statement
While Hitz can now run OCI container images natively (Phase 31), it is still a standalone virtualization engine accessed via CLI or our custom API. Modern enterprises deploy applications at scale using Kubernetes. Because Hitz does not speak the Container Runtime Interface (CRI) protocol, Kubernetes cannot natively schedule Pods onto Hitz microVMs. This severely limits our total addressable market to niche CLI/scripting users.

## The "So What?"
What business problem does this solve? Enterprise adoption and revenue generation. Organizations want the security boundaries of hardware virtualization (like Kata Containers) without changing their existing Kubernetes manifests. If a DevOps team can switch their Kubernetes cluster to use Hitz under the hood simply by updating a `RuntimeClass`, they gain immense security benefits with zero application refactoring. Building a CRI implementation transforms Hitz from a developer tool into an enterprise infrastructure primitive, unlocking massive scale and integrations.

## Gap Analysis
- **Current State:** Hitz only boots VMs via the `hitz-cli` or custom daemon REST/RPC API. It has no concept of Kubernetes primitives like Pods, sandboxes, or the gRPC-based CRI specification.
- **Market Standard:** Competing solutions like Firecracker use `firecracker-containerd`, and Kata Containers use `kata-agent` with containerd/CRI-O. The market expects a CRI-compatible shim (`containerd-shim-hitz-v1`) to seamlessly slot into standard Kubernetes architectures.

## Acceptance Criteria
- 👤 **User Story:** As a Kubernetes Administrator, I want to define a `RuntimeClass` for Hitz, so that my developers can deploy standard Pods into hardware-isolated microVMs without changing their `Deployments` or `StatefulSets`.
- 👤 **User Story:** As a Security Auditor, I want each Kubernetes Pod to be fully encapsulated in a separate Windows Hypervisor Platform (WHP) partition, so that kernel exploits in one container cannot compromise the host or other Pods.
- ✅ **Metric Definition:** Success = A standard Kubernetes cluster (e.g., `k3s` or `kubeadm`) can successfully schedule, run, and delete a multi-container Pod (e.g., Nginx + a sidecar) using the Hitz CRI runtime, passing the standard Kubernetes end-to-end (e2e) conformance test suite for basic Pod lifecycles.
- **Functional Requirements:**
  - Implement a `containerd` shim (e.g., `containerd-shim-hitz-v1`) that implements the Container Runtime Interface (CRI) gRPC API.
  - Map the CRI concept of a "Pod Sandbox" directly to a single Hitz microVM.
  - Map CRI "Containers" to processes running within that single microVM (requires a lightweight guest agent within the microVM to manage processes/namespaces, similar to `kata-agent`).
  - Implement CRI networking (CNI integration) to ensure Pods receive standard Kubernetes IP addresses and can communicate across the cluster.

## 🚫 Out of Scope
- **Advanced Kubernetes Features:** Storage classes (CSI plugins), complex network policies, and GPU passthrough via device plugins are out of scope for the initial CRI integration. The focus is strictly on basic stateless Pod lifecycles.
- **Replacing Containerd/CRI-O:** We are not building a top-level CRI runtime daemon from scratch. We are building a shim that plugs into existing engines like `containerd`.
