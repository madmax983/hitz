import re

with open("crates/hitz-hal/src/error.rs", "r") as f:
    code = f.read()

# Add DeviceLockPoisoned to HalError
code = code.replace("""    /// Failed to create a vCPU.
    #[error("failed to create vCPU {vcpu_id}: {reason}")]
    CreateVcpu {""", """    /// Device lock was poisoned by a panicked vCPU.
    #[error("device lock poisoned")]
    DeviceLockPoisoned,

    /// Failed to create a vCPU.
    #[error("failed to create vCPU {vcpu_id}: {reason}")]
    CreateVcpu {""")

with open("crates/hitz-hal/src/error.rs", "w") as f:
    f.write(code)
