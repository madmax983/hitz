#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
use hitz_vmm::SerialBuf;
use proptest::prelude::*;
use std::io::Write;

proptest! {
    #[test]
    fn torture_serial_buf_write_edge_cases_v3(
        cap in 1..10usize, // Make capacity tiny to force overlapping wrapping writes
        writes in prop::collection::vec(
            prop::collection::vec(0u8..255u8, 0..50),
            1..20
        )
    ) {
        let mut buf = SerialBuf::with_capacity(cap);
        let mut reader = buf.reader();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        let mut expected = Vec::new();

        for w in writes {
            buf.write_all(&w).unwrap();
            expected.extend(&w);
        }

        buf.close();

        let mut all_reads: Vec<u8> = Vec::new();
        rt.block_on(async {
            while let Some(chunk) = reader.read_chunk().await {
                all_reads.extend(chunk);
            }
        });

        let expected_read = if expected.len() > cap {
            &expected[expected.len() - cap..]
        } else {
            &expected[..]
        };

        assert_eq!(all_reads, expected_read, "cap={}, total_written={}", cap, expected.len());
    }
}

#[test]
fn havoc_test_notify_race_condition() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let buf = SerialBuf::with_capacity(10);
        let mut reader = buf.reader();

        let mut writer_buf = buf.clone();
        let handle = tokio::spawn(async move {
            tokio::task::yield_now().await;
            writer_buf.write_all(b"test").unwrap();
        });

        // Simulate the gap between lock drop and notified().await
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let chunk_opt =
            tokio::time::timeout(std::time::Duration::from_millis(50), reader.read_chunk()).await;

        let chunk = chunk_opt.unwrap().unwrap();
        assert_eq!(
            chunk, b"test",
            "Havoc expected the data, but got deadlock/timeout!"
        );
        let _ = handle.await;
    });
}
