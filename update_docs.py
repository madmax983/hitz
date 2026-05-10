import re

with open('crates/hitz-hal/src/error.rs', 'r') as f:
    content = f.read()

replacement = """/// Errors from hypervisor operations.
///
/// # Optimization
///
/// We use `std::borrow::Cow<'static, str>` instead of `String` for the `reason` field
/// to eliminate unnecessary heap allocations for static error messages during runtime failures.
///
/// # Abstract"""

content = content.replace("/// Errors from hypervisor operations.\n///\n/// # Abstract", replacement)

with open('crates/hitz-hal/src/error.rs', 'w') as f:
    f.write(content)
