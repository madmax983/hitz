# 🔭 Vantage: Spec for OCI Container Image Support

## Problem Statement
Currently, Hitz requires users to supply raw disk images (`.img`, `.qcow2`, etc.) and a compiled Linux kernel to run a micro-VM. This forces users into manual image building processes (e.g., using `packer` or `debootstrap`), which is slow, complex, and breaks compatibility with the modern cloud-native ecosystem where applications are packaged as Docker/OCI containers.

## The "So What?"
What business problem does this solve? Frictionless application deployment is key to developer adoption. Developers are already comfortable building and pushing OCI container images. If they are forced to convert containers into raw disk images to use Hitz, the "Time to Value" spikes and they will abandon the tool. By allowing Hitz to natively pull and run OCI container images as micro-VMs, we bridge the gap between lightweight virtualization and standard container workflows, significantly reducing deployment friction and opening the door to Kubernetes integrations (e.g., via Kata Containers or a custom CRI).

## Gap Analysis
- **Current State:** Hitz only boots from a supplied kernel and block device image. There is no understanding of OCI registries, image manifests, or layer extraction.
- **Market Standard:** Tools like Firecracker (via Firecracker-containerd) and Podman (via podman machine) provide seamless integration with OCI images. Developers expect to be able to run `hitz run nginx:latest` just as they would with Docker. Our lack of OCI support isolates us from the standard cloud-native supply chain.

## Acceptance Criteria
- 👤 **User Story:** As a Developer, I want to deploy a micro-VM directly from an OCI container image (e.g., `alpine:latest`), so that I can use standard registry artifacts without building custom disk images.
- 👤 **User Story:** As a CI Pipeline, I want to pull an image from a private registry using credentials, so that I can securely run proprietary applications in an isolated micro-VM environment.
- ✅ **Metric Definition:** Success = A user can execute `hitz run docker.io/library/nginx:latest` and have the micro-VM running and serving traffic in under 5 seconds (excluding image download time).
- **Functional Requirements:**
  - Introduce an image puller capable of fetching manifests and layers from standard OCI registries.
  - Implement a mechanism (e.g., a lightweight snapshotter or overlayfs implementation within the host/daemon) to extract container layers into a rootfs readable by the micro-VM (either via a generated block device or virtio-fs).
  - Automatically synthesize a bootable kernel and initramfs (if not provided by the image) that mounts the extracted OCI rootfs and executes the container's `ENTRYPOINT`/`CMD`.
  - Support authentication for private registries.

## 🚫 Out of Scope
- **Full Kubernetes CRI Implementation:** We are building the foundational capability to run OCI images via the CLI. Implementing a full Container Runtime Interface (CRI) for Kubernetes integration is a separate, future phase.
- **Complex Container Orchestration:** Handling multi-container pods, advanced networking namespaces (beyond the VM's isolated network), or docker-compose equivalents is out of scope. Hitz remains a single-VM manager.
