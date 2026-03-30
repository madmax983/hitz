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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod havoc_proptest {
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
        fn havoc_serial_buf_read_write_capacity(cap in 1..10000usize, writes in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 1..5000), 1..10)) {
            let rt = test_rt();
            rt.block_on(async {
                let mut buf = SerialBuf::with_capacity(cap);
                let mut reader = buf.reader();

                let mut expected_written = 0;
                for data in writes {
                    expected_written += data.len();
                    buf.write_all(&data).unwrap();
                }

                // Close so reader knows we're done
                buf.close();

                let mut total_read = 0;
                loop {
                    let chunk_opt = tokio::time::timeout(Duration::from_millis(50), reader.read_chunk()).await;
                    match chunk_opt {
                        Ok(Some(chunk)) => {
                            total_read += chunk.len();
                        }
                        Ok(None) => break,
                        Err(e) => panic!("timeout: {e:?}"),
                    }
                }

                // if we wrote more than capacity, total_read should be cap. Else expected_written
                let expected_read = std::cmp::min(expected_written, cap);
                assert_eq!(total_read, expected_read);
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod havoc_edge_cases {
    use crate::serial_buf::SerialBuf;
    use std::io::Write;

    #[test]
    fn havoc_test_serial_buf_read_past_capacity() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let mut buf = SerialBuf::with_capacity(5);
            let mut reader = buf.reader();

            // writer laps reader multiple times
            for _ in 0..10 {
                buf.write_all(b"1234567890").unwrap();
            }

            // reader finally wakes up
            let chunk =
                tokio::time::timeout(std::time::Duration::from_millis(50), reader.read_chunk())
                    .await
                    .unwrap()
                    .unwrap();
            // should read the last 5 bytes ("67890")
            assert_eq!(chunk, b"67890");
        });
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod havoc_race_tests {
    use crate::serial_buf::SerialBuf;
    use std::io::Write;

    #[test]
    fn havoc_test_serial_buf_loom_2() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let mut buf = SerialBuf::with_capacity(5);
            let mut reader = buf.reader();

            buf.write_all(b"abc").unwrap();
            let c = tokio::time::timeout(std::time::Duration::from_millis(50), reader.read_chunk())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(c, b"abc");

            buf.write_all(b"defg").unwrap();
            let c = tokio::time::timeout(std::time::Duration::from_millis(50), reader.read_chunk())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(c, b"defg");
        });
    }
}
