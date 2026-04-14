import subprocess

code = """
    #[test]
    fn vm_action_serde_roundtrip() {
        let actions = vec![VmAction::Start, VmAction::Stop, VmAction::Restart];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let decoded: VmAction = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, action);
        }
    }

    #[test]
    fn vm_state_serde_roundtrip() {
        let states = vec![VmState::Created, VmState::Running, VmState::Stopped, VmState::Failed];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let decoded: VmState = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn create_vm_request_serde() {
        let req = CreateVmRequest { config: minimal_config() };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: CreateVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.config.ram_mib, req.config.ram_mib);
    }

    #[test]
    fn clone_vm_request_serde() {
        let req = CloneVmRequest { dest_id: "new-vm".to_string() };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: CloneVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.dest_id, "new-vm");
    }

    #[test]
    fn action_vm_request_serde() {
        let req = ActionVmRequest { action: VmAction::Stop };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: ActionVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.action, VmAction::Stop);
    }

    #[test]
    fn api_error_serde() {
        let err = ApiError { error: "Something went wrong".to_string() };
        let json = serde_json::to_string(&err).unwrap();
        let decoded: ApiError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.error, err.error);
    }
"""

with open("crates/hitz-api/src/lib.rs", "r") as f:
    content = f.read()

if "vm_action_serde_roundtrip" not in content:
    content = content.replace("    #[test]\n    fn effective_cmdline_default() {", code + "\n    #[test]\n    fn effective_cmdline_default() {")
    with open("crates/hitz-api/src/lib.rs", "w") as f:
        f.write(content)
