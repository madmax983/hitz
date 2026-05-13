#![allow(clippy::unwrap_used)]

use hitz_vmm::SerialBuf;
use std::io::Write;
use std::time::Duration;

/// 👺 Havoc: Deadlock trigger due to missed watch channel notification.
#[test]
fn havoc_serial_buf_deadlock() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let mut buf = SerialBuf::with_capacity(16);
        let mut reader = buf.reader();

        buf.write_all(b"A").unwrap();

        let chunk = reader.read_chunk().await.unwrap();
        assert_eq!(chunk, b"A");

        buf.write_all(b"B").unwrap();

        let chunk2 = tokio::time::timeout(Duration::from_millis(500), reader.read_chunk()).await;

        assert!(chunk2.is_ok(), "reader.read_chunk() deadlocked!");
    });
}
