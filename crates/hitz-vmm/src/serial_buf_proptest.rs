use proptest::prelude::*;
use crate::serial_buf::SerialBuf;
use std::io::Write;

proptest! {
    #[test]
    fn test_serial_buf_random_writes(
        cap in 1..1000usize,
        writes in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..2000), 1..100)
    ) {
        let mut buf = SerialBuf::with_capacity(cap);
        let mut reader = buf.reader();

        for w in writes {
            buf.write_all(&w).unwrap();
        }

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(10), reader.read_chunk()).await;
        });
    }
}
