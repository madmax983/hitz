//! vCPU run loop — dispatches exits to devices and advances RIP.
//!
//! WHP does **not** auto-advance RIP on I/O port or MMIO exits. The VMM
//! must read the current registers, add `instruction_len` to RIP, and
//! write them back before re-entering the guest. Without this, the guest
//! infinite-loops on the faulting instruction.

use hitz_devices::serial::SerialDevice;
use hitz_hal::{HalError, Vcpu, VcpuExit};
use std::io::Write;

/// Reason the run loop terminated.
#[derive(Debug, PartialEq, Eq)]
pub enum ExitReason {
    /// Guest executed HLT.
    Halt,
    /// Guest initiated shutdown (triple fault, etc.).
    Shutdown,
    /// Unexpected exit that the run loop doesn't know how to handle.
    Unexpected(String),
}

/// Run a vCPU in a loop, dispatching I/O exits to the serial device.
///
/// Returns when the guest halts, shuts down, or hits an unrecoverable exit.
///
/// # Arguments
///
/// * `vcpu` — the virtual processor to run (must already have registers configured)
/// * `serial` — the serial device handling COM1 I/O
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    serial: &mut SerialDevice<W>,
) -> Result<ExitReason, HalError> {
    loop {
        let exit = vcpu.run()?;

        match exit {
            VcpuExit::IoPort(io) => {
                if SerialDevice::<W>::handles_port(io.port) {
                    if io.is_write {
                        // OUT — guest writing to serial register.
                        serial.pio_write(io.port, io.data[0]);
                        advance_rip(vcpu, io.instruction_len)?;
                    } else {
                        // IN — guest reading from serial register.
                        let value = serial.pio_read(io.port);
                        advance_rip_with_rax(vcpu, io.instruction_len, u64::from(value))?;
                    }
                } else {
                    // Unhandled port — log and skip.
                    tracing::debug!(port = io.port, is_write = io.is_write, "unhandled I/O port");
                    if io.is_write {
                        advance_rip(vcpu, io.instruction_len)?;
                    } else {
                        // Return 0xFF for unhandled IN (standard "nothing here" response).
                        advance_rip_with_rax(vcpu, io.instruction_len, 0xFF)?;
                    }
                }
            }

            VcpuExit::Halt => return Ok(ExitReason::Halt),
            VcpuExit::Shutdown => return Ok(ExitReason::Shutdown),

            VcpuExit::Mmio(mmio) => {
                // Phase 3 will wire MMIO through the emulator + device bus.
                tracing::debug!(gpa = %mmio.gpa, is_write = mmio.is_write, "unhandled MMIO");
                advance_rip(vcpu, mmio.instruction_len)?;
            }

            // Phase 3 will handle these differently:
            // - InterruptWindow: inject queued interrupts
            // - Canceled: check pending IRQs, request interrupt window
            // For now, just re-enter the guest.
            VcpuExit::InterruptWindow | VcpuExit::Canceled => {}

            VcpuExit::Unknown(code) => {
                return Ok(ExitReason::Unexpected(format!(
                    "unknown vCPU exit reason: {code:#x}"
                )));
            }
        }
    }
}

/// Advance RIP past the faulting instruction.
fn advance_rip<V: Vcpu>(vcpu: &mut V, instruction_len: u8) -> Result<(), HalError> {
    let mut regs = vcpu.get_regs()?;
    regs.rip += u64::from(instruction_len);
    vcpu.set_regs(&regs)
}

/// Advance RIP and set RAX (for IN instructions that return a value).
///
/// Combines both updates into a single `set_regs` call to minimize
/// round-trips to the hypervisor.
fn advance_rip_with_rax<V: Vcpu>(
    vcpu: &mut V,
    instruction_len: u8,
    rax: u64,
) -> Result<(), HalError> {
    let mut regs = vcpu.get_regs()?;
    regs.rip += u64::from(instruction_len);
    regs.rax = rax;
    vcpu.set_regs(&regs)
}
