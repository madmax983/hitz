//! vCPU run loop — dispatches exits to devices and advances RIP.
//!
//! WHP does **not** auto-advance RIP on I/O port or MMIO exits. The VMM
//! must read the current registers, add `instruction_len` to RIP, and
//! write them back before re-entering the guest. Without this, the guest
//! infinite-loops on the faulting instruction.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use hitz_devices::MmioBus;
use hitz_devices::SerialDevice;
use hitz_hal::{GuestMemAccess, HalError, IoPortExit, Vcpu, VcpuExit};
use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::mmio_decode;

/// Safety net: maximum iterations before the run loop bails out.
/// Prevents infinite loops in the VMM from hanging the host.
const MAX_RUN_ITERATIONS: u64 = 100_000_000;

/// Legacy 8259 PIC ports. Linux probes these during early boot even when
/// no PIC is present. Absorb writes and return 0x00 for reads (no IRQs
/// pending) to prevent the kernel from entering spurious-IRQ error paths.
const PIC_PORTS: [u16; 4] = [0x20, 0x21, 0xA0, 0xA1];

/// Reason the run loop terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Devices shared across all vCPU threads.
///
/// Each vCPU locks this only during I/O exits (microseconds per lock).
/// Compute-bound guests experience zero contention.
pub struct SharedDevices<W: Write> {
    /// Serial console (COM1).
    pub serial: SerialDevice<W>,
    /// MMIO bus with virtio transports.
    pub mmio_bus: MmioBus,
}

/// Increment the exit counter with the given reason label.
///
/// Extracted so each match arm stays a single call site.
fn record_exit(counter: &Counter<u64>, reason: &'static str) {
    counter.add(1, &[KeyValue::new("exit_reason", reason)]);
}

/// Run a vCPU in a loop, dispatching I/O and MMIO exits to devices.
///
/// # Abstract
///
/// This is the heart of the virtual machine. It repeatedly tells the hypervisor
/// to run the virtual CPU until an exit occurs (like a memory-mapped I/O request
/// or a halt instruction). It handles routing these exits to the appropriate
/// virtual devices and then resumes execution.
///
/// # The Hero's Journey
///
/// ```text
/// use hitz_vmm::run_loop::{run_vcpu_loop, SharedDevices, ExitReason};
/// use hitz_devices::{MmioBus, SerialDevice};
/// use hitz_hal::{GuestMemAccess, Vcpu, StandardRegs, SpecialRegs, VcpuExit};
/// use std::sync::{Arc, Mutex};
/// use std::sync::atomic::{AtomicBool, Ordering};
///
/// // Minimal dummy struct to implement the required trait bounds for the doc test.
/// struct DummyVcpu;
/// impl Vcpu for DummyVcpu {
///     fn run(&mut self) -> Result<VcpuExit, hitz_hal::HalError> { Ok(VcpuExit::Halt) }
///     fn get_regs(&self) -> Result<StandardRegs, hitz_hal::HalError> { Ok(StandardRegs::default()) }
///     fn set_regs(&mut self, _regs: &StandardRegs) -> Result<(), hitz_hal::HalError> { Ok(()) }
///     fn get_sregs(&self) -> Result<SpecialRegs, hitz_hal::HalError> { unimplemented!() }
///     fn set_sregs(&mut self, _sregs: &SpecialRegs) -> Result<(), hitz_hal::HalError> { Ok(()) }
///     fn inject_interrupt(&mut self, _vector: u8) -> Result<(), hitz_hal::HalError> { Ok(()) }
///     fn request_interrupt_window(&mut self) -> Result<(), hitz_hal::HalError> { Ok(()) }
///     fn cancel_handle(&self) -> hitz_hal::VcpuCancelHandle { unimplemented!() }
///     fn cancel_via(_handle: &hitz_hal::VcpuCancelHandle) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// }
///
/// struct DummyMem;
/// impl GuestMemAccess for DummyMem {
///     fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
///     fn write_guest(&self, _gpa: u64, _buf: &[u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// }
///
/// let mut vcpu = DummyVcpu;
/// let mem = DummyMem;
/// let devices = Mutex::new(SharedDevices {
///     serial: SerialDevice::new(std::io::sink()),
///     mmio_bus: MmioBus::new(),
/// });
/// let stop_flag = AtomicBool::new(false);
///
/// // Hand over control to the run loop!
/// let result = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag);
///
/// // In this dummy example, the vCPU immediately halts.
/// assert!(matches!(result.expect("should halt"), ExitReason::Halt));
/// ```
///
/// # The Fine Print
///
/// Returns when the guest halts, shuts down, or hits an unrecoverable exit.
///
/// ## Arguments
///
/// * `vcpu` — the virtual processor to run (must already have registers configured)
/// * `devices` — shared device state (serial + MMIO bus) behind a mutex
/// * `mem` — guest physical memory accessor for DMA operations
/// * `stop_flag` — set to `true` by another thread to cancel the run loop
///
/// ## Panics
///
/// Panics if the device mutex is poisoned (a vCPU thread panicked while
/// holding the lock). This is intentional — a poisoned lock means the
/// device state is inconsistent and recovery is not possible.
#[allow(clippy::expect_used)]
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    mem: &dyn GuestMemAccess,
    stop_flag: &AtomicBool,
) -> Result<ExitReason, HalError> {
    let mut pending_irq: Option<u8> = None;
    let mut iterations: u64 = 0;

    // Create once; all calls are no-ops when no SDK is registered.
    let exit_counter = opentelemetry::global::meter("hitz")
        .u64_counter("hitz.vcpu.exits")
        .with_description("Number of vCPU exits, labeled by exit reason")
        .build();

    loop {
        iterations += 1;
        if iterations > MAX_RUN_ITERATIONS {
            record_exit(&exit_counter, "Unexpected");
            return Ok(ExitReason::Unexpected(
                "iteration limit reached".to_string(),
            ));
        }

        if stop_flag.load(Ordering::Relaxed) {
            record_exit(&exit_counter, "Canceled");
            return Ok(ExitReason::Canceled);
        }

        // Poll devices for async I/O (e.g. network RX).
        poll_devices(vcpu, devices, &mut pending_irq)?;

        let exit = vcpu.run()?;

        if let Some(reason) =
            dispatch_exit(vcpu, devices, mem, exit, &mut pending_irq, &exit_counter)?
        {
            return Ok(reason);
        }
    }
}

#[allow(clippy::expect_used)]
fn poll_devices<V: Vcpu, W: Write>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    pending_irq: &mut Option<u8>,
) -> Result<(), HalError> {
    let mut devs = devices.lock().expect("device lock poisoned");
    let pending_vector = devs.mmio_bus.poll_devices();
    drop(devs);

    if let Some(vector) = pending_vector
        && vcpu.inject_interrupt(vector).is_err()
    {
        *pending_irq = Some(vector);
        vcpu.request_interrupt_window()?;
    }
    Ok(())
}

#[allow(clippy::expect_used)]
fn dispatch_exit<V: Vcpu, W: Write>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    mem: &dyn GuestMemAccess,
    exit: VcpuExit,
    pending_irq: &mut Option<u8>,
    exit_counter: &Counter<u64>,
) -> Result<Option<ExitReason>, HalError> {
    match exit {
        VcpuExit::IoPort(io) => {
            record_exit(exit_counter, "IoPort");
            let mut devs = devices.lock().expect("device lock poisoned");
            handle_io_port(vcpu, &mut devs.serial, &io)?;
        }
        VcpuExit::Halt => {
            record_exit(exit_counter, "Halt");
            return Ok(Some(ExitReason::Halt));
        }
        VcpuExit::Shutdown => {
            record_exit(exit_counter, "Shutdown");
            return Ok(Some(ExitReason::Shutdown));
        }
        VcpuExit::Mmio(mmio) => {
            record_exit(exit_counter, "Mmio");
            handle_mmio(vcpu, devices, mem, &mmio, pending_irq)?;
        }
        VcpuExit::InterruptWindow => {
            record_exit(exit_counter, "InterruptWindow");
            // Guest is now interruptible. WHP auto-clears the
            // deliverability notification after this exit fires.
            if let Some(vector) = pending_irq.take() {
                vcpu.inject_interrupt(vector)?;
                tracing::debug!(vector, "deferred interrupt injected via interrupt window");
            }
        }
        VcpuExit::Canceled => {
            record_exit(exit_counter, "Canceled");
            // vCPU run was canceled (e.g. by another thread).
            // If we have a pending IRQ, re-request the interrupt window
            // so we get notified once the guest becomes interruptible.
            if pending_irq.is_some() {
                vcpu.request_interrupt_window()?;
            }
        }
        VcpuExit::Unknown(code) => {
            record_exit(exit_counter, "Unexpected");
            return Ok(Some(ExitReason::Unexpected(format!(
                "unknown vCPU exit reason: {code:#x}"
            ))));
        }
    }
    Ok(None)
}

/// Dispatch an MMIO exit to the appropriate device handler.
#[allow(clippy::expect_used)]
fn handle_mmio<V: Vcpu, W: Write>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    mem: &dyn GuestMemAccess,
    mmio: &hitz_hal::MmioExit,
    pending_irq: &mut Option<u8>,
) -> Result<(), HalError> {
    let byte_count = usize::from(mmio.instruction_byte_count);
    let safe_bytes = mmio
        .instruction_bytes
        .get(..byte_count)
        .unwrap_or(&mmio.instruction_bytes[..]);
    let decoded = mmio_decode::decode_mmio_instruction(safe_bytes);

    let Some(decoded) = decoded else {
        tracing::warn!(
            gpa = %mmio.gpa,
            bytes = ?safe_bytes,
            "undecodable MMIO instruction, skipping"
        );
        // Last resort: use WHP's instruction_len (may be 0).
        return advance_rip(vcpu, mmio.instruction_len);
    };

    // WHP does NOT populate InstructionLength for MMIO exits
    // (it's always 0). Use the decoder-computed length instead.
    let instr_len = decoded.instruction_len;

    if !mmio.is_write {
        // Read: lock → device read → unlock, then update registers.
        let size = usize::from(decoded.size);
        let mut data = [0u8; 8];
        {
            let mut devs = devices.lock().expect("device lock poisoned");
            devs.mmio_bus.read(mmio.gpa.as_u64(), &mut data[..size]);
        }
        let value = u64::from_le_bytes(data);

        let mut regs = vcpu.get_regs()?;
        mmio_decode::set_register(&mut regs, decoded.register, value);
        regs.rip = regs.rip.wrapping_add(u64::from(instr_len));
        return vcpu.set_regs(&regs);
    }

    let mut data = [0u8; 4];
    let value = if let Some(imm) = decoded.immediate {
        u64::from(imm)
    } else {
        let regs = vcpu.get_regs()?;
        mmio_decode::register_value(&regs, decoded.register)
    };
    let size = usize::from(decoded.size);
    data[..size].copy_from_slice(&value.to_le_bytes()[..size]);

    // Lock → device write → unlock, then handle IRQ.
    let irq = {
        let mut devs = devices.lock().expect("device lock poisoned");
        devs.mmio_bus.write(mmio.gpa.as_u64(), &data[..size], mem)
    };
    advance_rip(vcpu, instr_len)?;

    if let Some(vector) = irq {
        // Try to inject immediately. If the guest has IF=0
        // (interrupts disabled) or is in interrupt shadow,
        // WHP rejects the injection — stash the IRQ and
        // request an interrupt window exit.
        if vcpu.inject_interrupt(vector).is_err() {
            *pending_irq = Some(vector);
            vcpu.request_interrupt_window()?;
            tracing::debug!(vector, "interrupt deferred, requested interrupt window");
        }
    }

    Ok(())
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
            return advance_rip(vcpu, io.instruction_len);
        }
        let value = serial.pio_read(io.port);
        return advance_rip_with_rax(vcpu, io.instruction_len, u64::from(value));
    }

    if PIC_PORTS.contains(&io.port) {
        // Legacy 8259 PIC stub — absorb writes, return 0x00 for reads.
        if io.is_write {
            return advance_rip(vcpu, io.instruction_len);
        }
        return advance_rip_with_rax(vcpu, io.instruction_len, 0x00);
    }

    tracing::debug!(port = io.port, is_write = io.is_write, "unhandled I/O port");
    if io.is_write {
        advance_rip(vcpu, io.instruction_len)
    } else {
        // Return 0xFF for unhandled IN (standard "nothing here" response).
        advance_rip_with_rax(vcpu, io.instruction_len, 0xFF)
    }
}

/// Advance RIP past the faulting instruction.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn advance_rip<V: Vcpu>(vcpu: &mut V, instruction_len: u8) -> Result<(), HalError> {
    let mut regs = vcpu.get_regs()?;
    regs.rip = regs.rip.wrapping_add(u64::from(instruction_len));
    vcpu.set_regs(&regs)
}

/// Advance RIP and set RAX (for IN instructions that return a value).
///
/// Combines both updates into a single `set_regs` call to minimize
/// round-trips to the hypervisor.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn advance_rip_with_rax<V: Vcpu>(
    vcpu: &mut V,
    instruction_len: u8,
    rax: u64,
) -> Result<(), HalError> {
    let mut regs = vcpu.get_regs()?;
    regs.rip = regs.rip.wrapping_add(u64::from(instruction_len));
    regs.rax = rax;
    vcpu.set_regs(&regs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard: the exit counter must not panic when the global meter
    /// is the default no-op meter (no provider registered).
    #[test]
    fn exit_counter_noop_does_not_panic() {
        // No OTel provider registered in test context → all calls are no-ops.
        let meter = opentelemetry::global::meter("hitz");
        let counter = meter
            .u64_counter("hitz.vcpu.exits")
            .with_description("vCPU exit events")
            .build();
        counter.add(1, &[KeyValue::new("exit_reason", "Halt")]);
        // If we reach here without panic, the test passes.
    }

    struct DummyVcpu {
        regs: hitz_hal::StandardRegs,
        sregs: hitz_hal::SpecialRegs,
        exit: VcpuExit,
    }

    impl Vcpu for DummyVcpu {
        type CancelHandle = ();
        fn cancel_handle(&self) -> Self::CancelHandle {}
        fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), HalError> {
            Ok(())
        }
        fn run(&mut self) -> Result<VcpuExit, HalError> {
            match &self.exit {
                VcpuExit::Halt => Ok(VcpuExit::Halt),
                VcpuExit::Canceled => Ok(VcpuExit::Canceled),
                VcpuExit::Unknown(c) => Ok(VcpuExit::Unknown(*c)),
                _ => unimplemented!(),
            }
        }
        fn get_regs(&self) -> Result<hitz_hal::StandardRegs, HalError> {
            Ok(self.regs.clone())
        }
        fn set_regs(&mut self, regs: &hitz_hal::StandardRegs) -> Result<(), HalError> {
            self.regs = regs.clone();
            Ok(())
        }
        fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, HalError> {
            Ok(self.sregs.clone())
        }
        fn set_sregs(&mut self, sregs: &hitz_hal::SpecialRegs) -> Result<(), HalError> {
            self.sregs = sregs.clone();
            Ok(())
        }
        fn inject_interrupt(&mut self, _vector: u8) -> Result<(), HalError> {
            Ok(())
        }
        fn request_interrupt_window(&mut self) -> Result<(), HalError> {
            Ok(())
        }
    }

    struct DummyMem;
    impl GuestMemAccess for DummyMem {
        fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), HalError> {
            Ok(())
        }
        fn write_guest(&self, _gpa: u64, _buf: &[u8]) -> Result<(), HalError> {
            Ok(())
        }
    }

    #[test]
    fn test_advance_rip() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        advance_rip(&mut vcpu, 5).expect("advance_rip should succeed");
        assert_eq!(vcpu.regs.rip, 105);
    }

    #[test]
    fn test_advance_rip_wrapping() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: u64::MAX,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        advance_rip(&mut vcpu, 1).expect("advance_rip should succeed");
        assert_eq!(vcpu.regs.rip, 0);
    }

    #[test]
    fn test_advance_rip_with_rax() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 0,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        advance_rip_with_rax(&mut vcpu, 5, 42).expect("advance_rip_with_rax should succeed");
        assert_eq!(vcpu.regs.rip, 105);
        assert_eq!(vcpu.regs.rax, 42);
    }

    #[test]
    fn test_run_vcpu_loop_halt() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let stop_flag = AtomicBool::new(false);
        let result = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag)
            .expect("run_vcpu_loop should succeed");
        assert_eq!(result, ExitReason::Halt);
    }

    #[test]
    fn test_run_vcpu_loop_canceled() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let stop_flag = AtomicBool::new(true);
        let result = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag)
            .expect("run_vcpu_loop should succeed");
        assert_eq!(result, ExitReason::Canceled);
    }

    #[test]
    fn test_run_vcpu_loop_unknown_exit() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Unknown(0x1337),
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let stop_flag = AtomicBool::new(false);
        let result = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag)
            .expect("run_vcpu_loop should succeed");
        assert_eq!(
            result,
            ExitReason::Unexpected("unknown vCPU exit reason: 0x1337".to_string())
        );
    }

    #[test]
    fn test_run_vcpu_loop_max_iterations() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Canceled, // Using Canceled to avoid returning immediately from run loop so we can test limits
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let stop_flag = AtomicBool::new(false);
        let result = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag)
            .expect("run_vcpu_loop should succeed");
        assert!(matches!(result, ExitReason::Unexpected(_)));
        if let ExitReason::Unexpected(msg) = result {
            assert!(msg.contains("iteration limit reached"));
        }
    }

    #[test]
    fn test_handle_io_port_serial_write() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());
        let io = hitz_hal::IoPortExit {
            port: 0x3F8,
            data: [b'A', 0, 0, 0],
            len: 1,
            is_write: true,
            instruction_len: 2,
        };
        handle_io_port(&mut vcpu, &mut serial, &io).expect("handle_io_port should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        assert_eq!(serial.writer(), &vec![b'A']);
    }

    #[test]
    fn test_handle_io_port_pic_read() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 42,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(std::io::sink());
        let io = hitz_hal::IoPortExit {
            port: 0x20, // PIC port
            data: [0, 0, 0, 0],
            len: 1,
            is_write: false,
            instruction_len: 2,
        };
        handle_io_port(&mut vcpu, &mut serial, &io).expect("handle_io_port should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        assert_eq!(vcpu.regs.rax, 0x00);
    }

    #[test]
    fn test_handle_io_port_unhandled_read() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 42,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(std::io::sink());
        let io = hitz_hal::IoPortExit {
            port: 0x1234, // Unknown port
            data: [0, 0, 0, 0],
            len: 1,
            is_write: false,
            instruction_len: 2,
        };
        handle_io_port(&mut vcpu, &mut serial, &io).expect("handle_io_port should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        assert_eq!(vcpu.regs.rax, 0xFF);
    }

    #[test]
    fn test_handle_io_port_unhandled_write() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 42,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(std::io::sink());
        let io = hitz_hal::IoPortExit {
            port: 0x1234, // Unknown port
            data: [0, 0, 0, 0],
            len: 1,
            is_write: true,
            instruction_len: 2,
        };
        handle_io_port(&mut vcpu, &mut serial, &io).expect("handle_io_port should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        // Write is absorbed, RIP advanced
    }
}
