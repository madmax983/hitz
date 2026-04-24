#[cfg(loom)]
mod loom_tests {
    use hitz_vmm::SerialBuf;
    use loom::thread;
    use std::io::Write;
    use std::sync::Arc;
    use tokio::runtime::Builder;

    #[test]
    fn havoc_serial_buf_loom_test() {
        loom::model(|| {
            let buf = SerialBuf::with_capacity(10);
            let mut reader1 = buf.reader();

            let mut writer = buf.clone();

            thread::spawn(move || {
                writer.write_all(b"test").unwrap();
                writer.close();
            });

            // loom doesn't support full tokio scheduler.
            // but we can spawn a small basic block_on to test the read
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let _ = reader1.read_chunk().await;
            });
        });
    }
}
