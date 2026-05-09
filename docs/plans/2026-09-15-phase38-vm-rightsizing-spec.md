# Spec: Automated VM Resource Right-Sizing

👤 **User Story:** As a Cloud Infrastructure Operator, I want the platform to automatically recommend and apply optimal CPU/RAM limits based on historical usage metrics, so that I can maximize host density and reduce wasted capacity without degrading guest performance.

✅ **Acceptance Criteria:**
- **Metric Definition:** Success = Reduces statically allocated RAM by at least 15% across idle VMs over a 7-day period without triggering guest OOM events or CPU throttling > 5%.
- **So What? (Business Problem):** Static resource allocation leads to human error and over-provisioning. Reclaiming wasted RAM directly translates to higher density and delayed capital expenditure on new hardware. Complexity is a cost, but automated utility is a revenue generator.
- **Gap Analysis:** We have Health Assessments, but no closed-loop system that acts on this data to optimize configuration. Operators currently must manually analyze dashboards.

🚫 **Out of Scope:**
- Predictive scaling based on machine learning.
- Modifying disk size.
- Specifying implementation details (like ballooning vs. hotplug).
