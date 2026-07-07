//! Serial console device (16550 UART) wrapping `vm-superio`.
//!
//! COM1 (0x3F8–0x3FF) is the only port supported. The device captures
//! guest output written via OUT instructions to the transmit-holding
//! register and makes it available through the inner `Write` sink.
//!
//! Phase 2 uses [`NoopTrigger`] — no interrupt injection. Phase 3 will
//! swap in a real trigger that calls `WHvCancelRunVirtualProcessor`.

use std::convert::Infallible;
use std::io::Write;

use vm_superio::serial::NoEvents;
use vm_superio::{Serial, Trigger};

/// COM1 base I/O port.
const COM1_BASE: u16 = 0x3F8;
/// Number of I/O ports per UART (8 registers).
const UART_PORT_COUNT: u16 = 8;

/// A no-op interrupt trigger for use when interrupt injection isn't wired up.
///
/// `type E = Infallible` guarantees `trigger()` can never fail, which
/// lets the serial device compile without an interrupt delivery backend.
#[derive(Debug, Clone, Copy)]
struct NoopTrigger;

impl Trigger for NoopTrigger {
    type E = Infallible;

    fn trigger(&self) -> Result<(), Self::E> {
        Ok(())
    }
}

/// Serial console device wrapping `vm_superio::Serial`.
///
/// Generic over `W: Write` so tests can use `Vec<u8>` while production
/// uses a pipe or PTY.
#[derive(Debug)]
/// A 16550A-compatible UART serial device (COM1).
///
/// # Abstract
///
/// This structure wraps `vm-superio`'s Serial implementation to provide a virtual
/// COM port for the guest. It serves as the primary console for early boot messages
/// and standard I/O interaction with the Linux kernel via PIO (Port I/O).
///
/// ## Examples
///
/// ```
/// # use hitz_devices::SerialDevice;
/// // 1. Create a serial device routing output to a simple vector.
/// let output = Vec::new();
/// let mut serial = SerialDevice::new(output);
///
/// // 2. Guest writes 'H' to COM1 (I/O port 0x3f8).
/// serial.pio_write(0x3f8, b'H');
/// ```
///
/// # The Fine Print
///
/// - The standard I/O port base for COM1 is `0x3f8`.
/// - Hitz only exposes writing to the serial port currently. Reads return `0`.
/// - An interrupt is usually asserted when the Transmit Holding Register (THR) is empty,
///   but Hitz uses a `NoopTrigger` to avoid injecting unnecessary interrupts since it's
///   synchronous and fast.
pub struct SerialDevice<W: Write> {
    inner: Serial<NoopTrigger, NoEvents, W>,
}

impl<W: Write> SerialDevice<W> {
    /// Create a new serial device writing guest output to `out`.
    pub fn new(out: W) -> Self {
        Self {
            inner: Serial::new(NoopTrigger, out),
        }
    }

    /// Returns `true` if `port` falls within the COM1 range (0x3F8–0x3FF).
    #[must_use]
    pub const fn handles_port(port: u16) -> bool {
        port >= COM1_BASE && port < COM1_BASE + UART_PORT_COUNT
    }

    /// Handle an OUT instruction (guest → device write).
    ///
    /// `port` is the raw I/O port number; this method converts it to the
    /// register offset before delegating to the inner UART.
    pub fn pio_write(&mut self, port: u16, value: u8) {
        // Safe: callers gate on handles_port(), so offset is 0..7.
        #[allow(clippy::cast_possible_truncation)]
        let offset = (port - COM1_BASE) as u8;
        // NoopTrigger is infallible, so write() can only fail on I/O errors
        // from the output sink. For Vec<u8> this is impossible; for real
        // sinks we log and swallow — losing a byte is better than crashing.
        if let Err(e) = self.inner.write(offset, value) {
            tracing::warn!(port, offset, %e, "serial write failed");
        }
    }

    /// Handle an IN instruction (device → guest read).
    ///
    /// Returns the register value for the given port.
    pub fn pio_read(&mut self, port: u16) -> u8 {
        // Safe: callers gate on handles_port(), so offset is 0..7.
        #[allow(clippy::cast_possible_truncation)]
        let offset = (port - COM1_BASE) as u8;
        self.inner.read(offset)
    }

    /// Access the output sink (e.g. for inspecting captured bytes in tests).
    pub fn writer(&self) -> &W {
        self.inner.writer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_port_com1_range() {
        // All 8 COM1 ports should be handled.
        for port in 0x3F8..=0x3FF {
            assert!(
                SerialDevice::<Vec<u8>>::handles_port(port),
                "port {port:#x}"
            );
        }
        // Outside range.
        assert!(!SerialDevice::<Vec<u8>>::handles_port(0x3F7));
        assert!(!SerialDevice::<Vec<u8>>::handles_port(0x400));
        assert!(!SerialDevice::<Vec<u8>>::handles_port(0x2F8)); // COM2
    }

    #[test]
    fn thr_write_appears_in_output() {
        let mut serial = SerialDevice::new(Vec::new());

        // Write 'H' to THR (offset 0 = port 0x3F8).
        serial.pio_write(0x3F8, b'H');
        serial.pio_write(0x3F8, b'i');

        assert_eq!(serial.writer(), b"Hi");
    }

    #[test]
    fn lsr_reports_ready() {
        let mut serial = SerialDevice::new(Vec::new());

        // LSR is at offset 5 = port 0x3FD.
        let lsr = serial.pio_read(0x3FD);

        // THRE (bit 5) and TEMT (bit 6) should be set — transmitter is idle.
        assert_ne!(lsr & 0x20, 0, "THRE should be set");
        assert_ne!(lsr & 0x40, 0, "TEMT should be set");
    }

    #[test]
    fn write_then_read_lsr_still_ready() {
        let mut serial = SerialDevice::new(Vec::new());

        serial.pio_write(0x3F8, b'X');

        // LSR should still report ready after a write (virtual device is
        // always "done" transmitting).
        let lsr = serial.pio_read(0x3FD);
        assert_ne!(lsr & 0x60, 0, "THRE+TEMT should be set after write");
    }

    #[test]
    fn test_noop_trigger() {
        use vm_superio::Trigger;
        let trigger = NoopTrigger;
        assert!(trigger.trigger().is_ok());
    }

    struct FailingSink;
    impl std::io::Write for FailingSink {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("failing sink"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_pio_write_failing_sink() {
        let mut dev = SerialDevice::new(FailingSink);
        // Writing to failing sink will trigger the inner.write to return Err.
        // It should log a warning but not panic.
        dev.pio_write(0x3F8, b'A');
    }
}
