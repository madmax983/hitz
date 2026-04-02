use std::io::Write;
use hitz_vmm::serial_buf::SerialBuf;

fn main() {
    let mut buf = SerialBuf::with_capacity(5);
    buf.write_all(&[1, 2, 3]).unwrap();
    // try edge cases
}
