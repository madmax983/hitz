use hitz_vmm::serial_buf::SerialBuf;
use std::io::Write;

#[test]
fn havoc_serial_buf_zero_capacity_write() {
    let mut buf = SerialBuf::with_capacity(0);
    // Write into 0 capacity -> panics with divide by zero!
    let _ = buf.write_all(b"Hello");
}
