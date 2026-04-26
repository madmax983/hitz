👤 **User Story:** As a Platform Operator, I want the microVM's background network threads to shut down synchronously when the VM exits, so that host resources are cleanly released and zombie threads do not accumulate.

✅ **Acceptance Criteria:**
- Must safely inherit or wrap the parent VM's stop flag.
- Must remove the associated TODO comment in the net initialization block.
- Must guarantee network threads exit cleanly within 500ms of a graceful VM shutdown.

🚫 **Out of Scope:** Graceful connection draining, or altering the teardown logic of other virtual devices (e.g., block, serial).
