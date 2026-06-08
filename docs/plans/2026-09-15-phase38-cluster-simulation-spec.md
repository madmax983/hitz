# 🔭 Vantage: Spec for Cluster Simulation

## Problem Statement
Currently, users can generate Terraform HCL for a single Hitz VM configuration via the API. However, they cannot easily simulate or model a fleet of interconnected VMs (a cluster) using our CLI or API, leaving them to manually assemble individual exports into a larger infrastructure definition.

## The "So What?"
What business problem does this solve? Simulating clusters. Users want to define "Deploy 5 Web nodes and 1 DB node" and receive a complete, ready-to-apply cluster definition. This drastically reduces the time and effort required to move from local prototyping to production deployment.

## Gap Analysis
- **Current State:** The system currently exports single VM configurations individually.
- **Market Standard:** Tools like Docker Compose or Kubernetes manifest generators allow users to specify replicas or node counts to generate multi-instance configurations automatically.

## Acceptance Criteria
- 👤 **User Story:** As a DevOps Engineer, I want to define a cluster topology (e.g., 3 frontend VMs, 1 backend VM), so that I can generate a single Terraform plan representing the entire cluster.
- ✅ **Metric Definition:** Success = A user provides a definition of node types and counts, and the system outputs a valid, multi-resource Terraform HCL file correctly numbering and naming the instances.
- **Functional Requirements:**
  - Introduce an API endpoint or CLI capability that accepts an aggregation of multiple VM templates and their desired replica counts.
  - The export functionality must output multiple VM resources correctly indexed (e.g., `web_0`, `web_1`) based on the requested topology.

## 🚫 Out of Scope
- **Direct Execution:** Applying the Terraform plan via the daemon is out of scope. We are generating the definition, not executing it.
