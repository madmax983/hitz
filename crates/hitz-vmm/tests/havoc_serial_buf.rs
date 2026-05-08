#[cfg(test)]
mod tests {
    use hitz_vmm::serial_buf::SerialBuf;
    use std::time::Duration;

    #[tokio::test]
    async fn test_serial_buf_drop_tight_loop() {
        let buf = SerialBuf::new();
        let mut reader = buf.reader();

        // Drop the buffer without calling close()
        drop(buf);

        // This should not spin forever, but it will prior to the fix.
        // We wrap it in a timeout to detect the infinite loop.
        let result: Result<Option<Vec<u8>>, tokio::time::error::Elapsed> =
            tokio::time::timeout(Duration::from_millis(50), reader.read_chunk()).await;

        // If it spins forever, timeout occurs and result is Err(_).
        // If it handles the drop correctly, it should return Ok(None).
        assert!(result.is_ok(), "Tight loop detected! The reader spun forever because changed().await returned Err immediately.");
        assert!(result.unwrap().is_none(), "Reader should return None when writer is dropped.");
    }
}
