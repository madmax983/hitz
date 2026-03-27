#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use crate::serial_buf::SerialBuf;
    use proptest::prelude::*;
    use std::io::Write;
    use std::time::Duration;

    fn test_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
    }

    proptest! {
        #[test]
        fn havoc_fuzz_serial_buf_read_write(capacity in 1..1000usize, write_size in 1..5000usize) {
            let rt = test_rt();
            rt.block_on(async {
                let mut buf = SerialBuf::with_capacity(capacity);
                let mut reader = buf.reader();

                let data = vec![42u8; write_size];
                buf.write_all(&data).unwrap();

                let chunk_opt = tokio::time::timeout(Duration::from_millis(50), reader.read_chunk()).await;

                if let Ok(Some(chunk)) = chunk_opt {
                    assert!(chunk.len() <= capacity);
                }
            });
        }
    }
}

#[cfg(test)]
mod havoc_sync_tests {
    use crate::serial_buf::SerialBuf;

    #[test]
    #[should_panic(expected = "capacity must be greater than 0")]
    fn havoc_capacity_zero_panic() {
        let _ = SerialBuf::with_capacity(0);
    }
}
