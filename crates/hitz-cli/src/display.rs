use hitz_api::{VmAction, VmState};

pub struct DisplayVmAction<'a>(pub &'a VmAction);

impl<'a> std::fmt::Display for DisplayVmAction<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.0 {
            VmAction::Start => "start",
            VmAction::Stop => "stop",
            VmAction::Restart => "restart",
        };
        write!(f, "{s}")
    }
}

pub struct DisplayVmState<'a>(pub &'a VmState);

impl<'a> std::fmt::Display for DisplayVmState<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.0 {
            VmState::Created => "Created",
            VmState::Running => "Running",
            VmState::Stopped => "Stopped",
            VmState::Failed => "Failed",
        };
        write!(f, "{s}")
    }
}
