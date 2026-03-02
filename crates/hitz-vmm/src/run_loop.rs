//! vCPU run loop — dispatches exits to devices and advances RIP.
//!
//! WHP does **not** auto-advance RIP on I/O port or MMIO exits. The VMM
//! must read the current registers, add `instruction_len` to RIP, and
//! write them back before re-entering the guest. Without this, the guest
//! infinite-loops on the faulting instruction.

use hitz_devices::mmio_bus::MmioBus;
use hitz_devices::serial::SerialDevice;
use hitz_hal::{GuestMemAccess, HalError, Vcpu, VcpuExit};
use std::io::Write;

use crate::mmio_decode;

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

/// Run a vCPU in a loop, dispatching I/O and MMIO exits to devices.
///
/// Returns when the guest halts, shuts down, or hits an unrecoverable exit.
///
/// # Arguments
///
/// * `vcpu` — the virtual processor to run (must already have registers configured)
/// * `serial` — the serial device handling COM1 I/O
/// * `mmio_bus` — the MMIO bus with registered virtio devices
/// * `mem` — guest physical memory accessor for DMA operations
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    serial: &mut SerialDevice<W>,
    mmio_bus: &mut MmioBus,
    mem: &dyn GuestMemAccess,
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

            VcpuExit::Halt => {
                return Ok(ExitReason::Halt);
            }
            VcpuExit::Shutdown => {
                return Ok(ExitReason::Shutdown);
            }

            VcpuExit::Mmio(mmio) => {
                let decoded = mmio_decode::decode_mmio_instruction(
                    &mmio.instruction_bytes[..usize::from(mmio.instruction_byte_count)],
                );

                if let Some(decoded) = decoded {
                    // WHP does NOT populate InstructionLength for MMIO exits
                    // (it's always 0). Use the decoder-computed length instead.
                    let instr_len = decoded.instruction_len;

                    if mmio.is_write {
                        let mut data = [0u8; 4];
                        let value = if let Some(imm) = decoded.immediate {
                            u64::from(imm)
                        } else {
                            let regs = vcpu.get_regs()?;
                            mmio_decode::register_value(&regs, decoded.register)
                        };
                        let size = usize::from(decoded.size);
                        data[..size].copy_from_slice(&value.to_le_bytes()[..size]);

                        let irq = mmio_bus.write(mmio.gpa.as_u64(), &data[..size], mem);
                        advance_rip(vcpu, instr_len)?;
                        if let Some(vector) = irq {
                            // Best-effort interrupt injection. WHP may reject if
                            // APIC emulation is not configured or the vCPU is in
                            // interrupt shadow. Phase 3 block I/O is synchronous
                            // so the guest can poll for completion without IRQs.
                            if let Err(e) = vcpu.inject_interrupt(vector) {
                                tracing::debug!(vector, error = %e, "interrupt injection skipped");
                            }
                        }
                    } else {
                        // Read: get data from device, write to destination register.
                        let size = usize::from(decoded.size);
                        let mut data = [0u8; 8];
                        mmio_bus.read(mmio.gpa.as_u64(), &mut data[..size]);
                        let value = u64::from_le_bytes(data);

                        let mut regs = vcpu.get_regs()?;
                        mmio_decode::set_register(&mut regs, decoded.register, value);
                        regs.rip += u64::from(instr_len);
                        vcpu.set_regs(&regs)?;
                    }
                } else {
                    tracing::warn!(
                        gpa = %mmio.gpa,
                        bytes = ?&mmio.instruction_bytes[..usize::from(mmio.instruction_byte_count)],
                        "undecodable MMIO instruction, skipping"
                    );
                    // Last resort: use WHP's instruction_len (may be 0).
                    advance_rip(vcpu, mmio.instruction_len)?;
                }
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
