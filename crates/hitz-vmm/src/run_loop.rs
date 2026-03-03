//! vCPU run loop — dispatches exits to devices and advances RIP.
//!
//! WHP does **not** auto-advance RIP on I/O port or MMIO exits. The VMM
//! must read the current registers, add `instruction_len` to RIP, and
//! write them back before re-entering the guest. Without this, the guest
//! infinite-loops on the faulting instruction.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use hitz_devices::mmio_bus::MmioBus;
use hitz_devices::serial::SerialDevice;
use hitz_hal::{GuestMemAccess, HalError, IoPortExit, Vcpu, VcpuExit};

use crate::mmio_decode;

/// Safety net: maximum iterations before the run loop bails out.
/// Prevents infinite loops in the VMM from hanging the host.
const MAX_RUN_ITERATIONS: u64 = 100_000_000;

/// Legacy 8259 PIC ports. Linux probes these during early boot even when
/// no PIC is present. Absorb writes and return 0x00 for reads (no IRQs
/// pending) to prevent the kernel from entering spurious-IRQ error paths.
const PIC_PORTS: [u16; 4] = [0x20, 0x21, 0xA0, 0xA1];

/// Reason the run loop terminated.
#[derive(Debug, PartialEq, Eq)]
pub enum ExitReason {
    /// Guest executed HLT.
    Halt,
    /// Guest initiated shutdown (triple fault, etc.).
    Shutdown,
    /// VM was stopped via the stop flag (user-initiated cancel).
    Canceled,
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
    stop_flag: Option<&AtomicBool>,
) -> Result<ExitReason, HalError> {
    let mut pending_irq: Option<u8> = None;
    let mut iterations: u64 = 0;

    loop {
        iterations += 1;
        if iterations > MAX_RUN_ITERATIONS {
            return Ok(ExitReason::Unexpected(
                "iteration limit reached".to_string(),
            ));
        }

        if let Some(flag) = stop_flag
            && flag.load(Ordering::Relaxed)
        {
            return Ok(ExitReason::Canceled);
        }

        let exit = vcpu.run()?;

        match exit {
            VcpuExit::IoPort(io) => {
                handle_io_port(vcpu, serial, &io)?;
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
                            // Try to inject immediately. If the guest has IF=0
                            // (interrupts disabled) or is in interrupt shadow,
                            // WHP rejects the injection — stash the IRQ and
                            // request an interrupt window exit.
                            if vcpu.inject_interrupt(vector).is_err() {
                                pending_irq = Some(vector);
                                vcpu.request_interrupt_window()?;
                                tracing::debug!(
                                    vector,
                                    "interrupt deferred, requested interrupt window"
                                );
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

            VcpuExit::InterruptWindow => {
                // Guest is now interruptible. WHP auto-clears the
                // deliverability notification after this exit fires.
                if let Some(vector) = pending_irq.take() {
                    vcpu.inject_interrupt(vector)?;
                    tracing::debug!(vector, "deferred interrupt injected via interrupt window");
                }
            }

            VcpuExit::Canceled => {
                // vCPU run was canceled (e.g. by another thread).
                // If we have a pending IRQ, re-request the interrupt window
                // so we get notified once the guest becomes interruptible.
                if pending_irq.is_some() {
                    vcpu.request_interrupt_window()?;
                }
            }

            VcpuExit::Unknown(code) => {
                return Ok(ExitReason::Unexpected(format!(
                    "unknown vCPU exit reason: {code:#x}"
                )));
            }
        }
    }
}

/// Dispatch an I/O port exit to the appropriate device handler.
fn handle_io_port<V: Vcpu, W: Write>(
    vcpu: &mut V,
    serial: &mut SerialDevice<W>,
    io: &IoPortExit,
) -> Result<(), HalError> {
    if SerialDevice::<W>::handles_port(io.port) {
        if io.is_write {
            serial.pio_write(io.port, io.data[0]);
            advance_rip(vcpu, io.instruction_len)?;
        } else {
            let value = serial.pio_read(io.port);
            advance_rip_with_rax(vcpu, io.instruction_len, u64::from(value))?;
        }
    } else if PIC_PORTS.contains(&io.port) {
        // Legacy 8259 PIC stub — absorb writes, return 0x00 for reads.
        if io.is_write {
            advance_rip(vcpu, io.instruction_len)?;
        } else {
            advance_rip_with_rax(vcpu, io.instruction_len, 0x00)?;
        }
    } else {
        tracing::debug!(port = io.port, is_write = io.is_write, "unhandled I/O port");
        if io.is_write {
            advance_rip(vcpu, io.instruction_len)?;
        } else {
            // Return 0xFF for unhandled IN (standard "nothing here" response).
            advance_rip_with_rax(vcpu, io.instruction_len, 0xFF)?;
        }
    }
    Ok(())
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
