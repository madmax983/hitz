#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use hitz_vmm::SerialBuf;
use std::io::Write;
use std::sync::Arc;
use std::thread;

#[derive(Arbitrary, Debug)]
struct FuzzData {
    capacity: usize,
    write_chunks: Vec<Vec<u8>>,
    reader_count: usize,
    close_at: usize,
}

fuzz_target!(|data: FuzzData| {
    if data.capacity == 0 || data.capacity > 1024 * 1024 * 10 { // Max 10MB to avoid OOM
        return;
    }

    let buf = SerialBuf::with_capacity(data.capacity);
    let buf_arc = Arc::new(buf);

    let reader_count = std::cmp::min(data.reader_count, 10);

    let mut handles = vec![];

    for _ in 0..reader_count {
        let buf_clone = buf_arc.clone();
        handles.push(thread::spawn(move || {
            let mut reader = buf_clone.reader();
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                loop {
                    // Small timeout just to make sure we don't block forever if close fails
                    match tokio::time::timeout(std::time::Duration::from_millis(50), reader.read_chunk()).await {
                        Ok(Some(_)) => continue,
                        Ok(None) => break,
                        Err(_) => break, // timeout
                    }
                }
            });
        }));
    }

    // Write thread
    let buf_clone = buf_arc.clone();
    let chunks = data.write_chunks.clone();
    let close_at = data.close_at;
    handles.push(thread::spawn(move || {
        let mut b = (*buf_clone).clone();
        for (i, chunk) in chunks.into_iter().enumerate() {
            if i == close_at {
                b.close();
            }
            let _ = b.write_all(&chunk);
        }
        b.close(); // Make sure we close eventually
    }));

    for handle in handles {
        let _ = handle.join();
    }
});
