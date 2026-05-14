#![allow(clippy::unwrap_used)]
use hitz_vmm::SerialBuf;
use proptest::prelude::*;
use std::io::Write;

proptest! {
    /// Fuzz test for serial buffer writing with tiny capacity to force
    /// overlapping wrapping writes and verify no data corruption occurs.
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

/// Tests the notify race condition to ensure a delayed read does not deadlock
/// or timeout due to lost notifications.
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

#[test]
fn havoc_serial_read_watch_channel_race() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let mut buf = SerialBuf::new();
    let mut reader = buf.reader();

    rt.block_on(async {
        // Write initial data so the channel version increments.
        buf.write_all(b"initial").unwrap();

        // Read it. This consumes the data but does NOT update the watch channel's
        // seen version because `changed().await` isn't called.
        let data = reader.read_chunk().await.unwrap();
        assert_eq!(data, b"initial");

        // Now we spawn a background task that writes more data shortly.
        // It writes exactly when the reader checks `total_written` and finds it empty,
        // but BEFORE `changed().await` is called.
        let mut w_buf = buf.clone();
        let _ = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            w_buf.write_all(b"second").unwrap();
        });

        // This read will:
        // 1. Lock inner, see no data.
        // 2. Call `borrow_and_update()` (updating the seen version)
        // 3. Call `changed().await`
        // 4. Then wake up when "second" is written.
        //
        // WITHOUT `borrow_and_update()`, if the sleep above happens before `changed().await`,
        // the immediate return of `changed().await` consumes the "initial" version bump
        // but the subsequent wait misses the "second" version bump because it already matched.
        let next_chunk = tokio::time::timeout(std::time::Duration::from_millis(200), reader.read_chunk()).await;

        let chunk = next_chunk.expect("Havoc: Deadlocked due to missed wakeup!").unwrap();
        assert_eq!(chunk, b"second");
    });
}
